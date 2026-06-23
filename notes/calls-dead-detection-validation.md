# Qualified calls + dead struct/function detection — implementation & validation

Implements `notes/feature-request-calls-and-dead-detection.md` (the
maintainer-analysis verdict in that file's header drove the scope). Rust
indexer, schema **v12**. Go is a follow-up (no enums; different shapes).

## What shipped

| FR part | shipped |
|---|---|
| §1 normalize call keys + split construction from invocation | yes |
| §2 `struct_refs` view (dead-struct detection) | yes |
| §3a SQL fixpoint for transitive deadness | **no** (declined — see below) |
| §3b document "re-run after each removal" | yes (AGENTS.md) |
| §4 `visibility` column + pub-filtered dead-fn query | yes |
| (found during validation) call-graph traversal gaps | fixed |

### §1 — call keys (`rust/src/main.rs::extract_call`)

The FR said qualified calls were "dropped." They weren't — `extract_call`
stored `callee = node_text(func)`, i.e. the **full path** for a
`scoped_identifier`. The dead-fn query failed on a key mismatch
(`symbols.name='foo'` ≠ `calls.callee='a::b::foo'`).

Now: for a `scoped_identifier`/`Self::` callee, `callee` = final segment,
`callee_qname` = full written path. `callee_candidates` was relaxed
(`... OR s.qualified_name IS NULL ...`) so populating `callee_qname` (a
module path, never equal to a free fn's `parent::name`) doesn't reject
the new bare-name matches.

**Construction vs invocation:** tree-sitter parses `Enum::Variant(x)` /
`TupleStruct(x)` as a `call_expression`. A PascalCase final segment is
tagged `kind='construct'`, so these don't pollute the dead-*function*
signal — and they feed `struct_refs`/`enum_variant_refs` construction
detection for free.

### §4 — `visibility` column

Parsed from each item's `visibility_modifier` (`pub`, `pub(crate)`,
`pub(super)`, `pub(in ...)`; NULL = private). rustc already flags non-pub
dead code, so the dead-fn query filters `visibility IS NOT NULL` — exactly
the pub-reachable blind spot the index uniquely covers.

### §2 — `struct_refs` view

`construct` rows from: struct-literal `Name { .. }` (captured by a new
`struct_expression` handler as a `refs` row, kind `construct`),
tuple-struct `Name(..)` (`calls` kind `construct`), and any
`Name::f(..)` associated call (`calls.callee_qname`, struct name anchored
as a full `::`-segment). `type` rows from `refs` kind `type`. Dead = a
struct with `type` refs and no `construct`. The associated-fn handling is
what removes the FR's `Interner` false positive (built via `Interner::new`).

### §3 — transitive deadness

3b only: documented in AGENTS.md that the detector reports one layer and
should be re-run after each removal (it converged in safebash). 3a (an
SQL fixpoint) was declined — it needs construct-site → enclosing-symbol →
is-dead linkage plus iteration to catch only what a re-run already
catches.

## Two call-graph gaps found by validating against safebash

The pub-filtered dead-fn query first returned **2** survivors — both
**false positives** from call-graph traversal holes, not dead code:

1. **`emit_c`** — called only as `print!("{}", emit_c::emit_c(&ssa))`.
   `extract_macro_call` recorded the macro name but never looked inside
   the `token_tree`. tree-sitter doesn't parse macro bodies as
   expressions (flat tokens), so there's no `call_expression` to walk.
   Fix: a heuristic — an `identifier` immediately before a parenthesised
   `token_tree` is a call (recurses into nested trees).
2. **`to_tok`** — called as `to_tok(lt).expect(..)`. `extract_call`
   recursed into arguments only, never the **function position**, so the
   receiver of a method chain was dropped. Fix: walk the receiver of a
   `field_expression` callee.

Both are common patterns (macro-wrapped calls; method chains), so these
were material holes in the call graph, independent of the FR's framing.

## Validation result (safebash, schema v12)

Dead-*function* candidates (pub, normalized callees, excluding constructs):

```
68  — FR baseline (full-path callees never matched short names)
33  — §1 key normalization
 2  — §4 pub-only filter
 0  — after the macro + method-chain call-graph fixes
```

The two that survived to "2" were both false positives exposing the
call-graph gaps above; fixing them takes safebash to zero genuinely-dead
pub functions (consistent with a warning-clean, actively-maintained tree).

`struct_refs` surfaces `SashTable` (a `pub struct` in `sash-rt` used only
via `*mut SashTable` raw pointers — an FFI/runtime opaque-allocation
candidate, the documented residual category, not a clear delete). The
FR's `Interner` false positive is correctly **not** flagged (built via
`Interner::new`). The FR's `Coercion` is gone (already removed upstream).

`enum_variant_refs` (v11, unchanged) returns 0 dead variants now — the 5
from the prior audit were removed upstream; the view still produces 2673
rows across 32 enums, so it's healthy, just nothing dead remains.

## Limitations (documented, candidate-generator philosophy)

- **Macro-internal calls** are heuristic (identifier-before-paren). Misses
  nothing structural but can over-credit (e.g. `matches!(x, Foo(_))`
  reads as a `Foo` construct). Safe direction for dead-fn (fewer
  false-positive deletions; possible false-negatives).
- **Dead-fn residual FPs** to discount by hand: trait-impl methods called
  through the trait, FFI (`#[no_mangle]`/`extern`), entry points,
  `#[test]`.
- **struct_refs**: FFI/raw-pointer-allocated and builder/`From`-only
  structs may still appear; `Name {` substring matching is avoided by
  using structured `refs`/`calls`, but associated-call liveness is
  broad (any `Name::f()` counts).
- **Transitive deadness**: one layer; re-run after each removal.

## Tests

`calls_dead_detection_tests` (5): qualified/`Self` calls keyed by final
segment + full-path qname; PascalCase calls tagged `construct`;
visibility parsed (`pub`/`pub(crate)`/private); `struct_refs` dead
detection (type-only structs dead; literal/tuple/`::new` live); construct
forms. Full suite 36 pass; `mk test` + shim green.

## Go follow-up

`calls` keying, `construct` split, and `visibility` would mirror cleanly
in Go; `struct_refs` maps to Go structs but the construct/type signals
come from `go/types` rather than text. Dead *functions* in Go are better
served by `x/tools/cmd/deadcode` + `staticcheck` (see
`notes/tasks/go-enum-variant-refs.md` for the same reasoning). Not done
here.
