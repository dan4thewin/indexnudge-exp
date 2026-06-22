# Feature Request: dead-code identification (never-constructed variants, dead symbols)

## Problem

`rustc`'s `dead_code` lint is the first line of defense, and a codebase
that keeps internal modules `pub(crate)` gets a lot from it. But the lint
goes **silent for anything publicly reachable** — a `pub enum` variant
that is *matched* and *compared* but never *constructed* is not flagged,
because rustc assumes some external crate might build it.

Concrete motivating case — safebash (`.indexnudge-rust.db`), 2026-06-22.
Heredoc support was dropped: the lexer now rejects `<<` with a sentinel,
but the IR variant `RedirectOp::HereDoc` (`sash/src/ir.rs`) survived. It
is referenced in **7 places** — match arms in `codegen/redirects.rs`,
`emit_c.rs`, `emit_janet.rs` (×2), `validate.rs`, and one `==` comparison
in `ssa_taint.rs` — but **constructed nowhere**. The build is
warning-clean. The dead variant sailed through a whole parser migration
unnoticed and was only found by reading code and grepping by hand.

You can't cheaply make rustc see it: the `ir` module is genuine public
API (integration tests build IR nodes), and demoting just `RedirectOp` to
`pub(crate)` trips `private_in_public` because `Redirect.op` is `pub`.
You'd have to demote the whole `Redirect`/`ir` chain.

**The existing `calls` graph does not fill this gap.** `calls.callee`
stores the short name as written and doesn't resolve `self.method()` or
`super::mod::fn()` (per the Rust AGENTS.md: `callee_qname` is always
NULL). On safebash, "functions never appearing in `calls.callee`" returns
67 candidates that are mostly **live** — `cli_main`, `compile_to_object`,
`handle_general_command`, … — far too noisy to act on.

So today there is no readily-runnable query that answers "what here is
dead?" — which is exactly the kind of thing a code index should make a
one-liner.

## Proposed solution

A view that classifies every `Type::Variant` (and, more generally, every
qualified path) by its **syntactic context**, mirroring the existing
`string_literals` recursive-CTE approach (zero storage, no per-language
table). Buckets:

| context     | example                                            |
|-------------|----------------------------------------------------|
| `construct` | `=> RedirectOp::HereDoc`, `op: RedirectOp::HereDoc`, `RedirectOp::HereDoc(x)`, `let r = RedirectOp::HereDoc` |
| `match_pat` | `RedirectOp::HereDoc =>`, `A \| RedirectOp::HereDoc =>` |
| `compare`   | `r.op == RedirectOp::HereDoc`, `x != RedirectOp::HereDoc` |

Call it `enum_variant_refs` (columns: `file, line_num, symbol, enum,
variant, context`). A "dead variant" is then any variant with zero
`construct` rows but at least one `match_pat`/`compare` row.

## Query

```sql
-- Variants referenced but never constructed (dead — safe to delete).
SELECT v.enum, v.variant
FROM (SELECT DISTINCT enum, variant FROM enum_variant_refs) v
WHERE NOT EXISTS (
    SELECT 1 FROM enum_variant_refs c
    WHERE c.enum = v.enum AND c.variant = v.variant
      AND c.context = 'construct'
);
-- On safebash this returns exactly one row: RedirectOp / HereDoc.

-- All the sites to clean up once you've decided to remove it.
SELECT file, line_num, context FROM enum_variant_refs
WHERE enum = 'RedirectOp' AND variant = 'HereDoc'
ORDER BY file, line_num;
```

## Implementation

tree-sitter `scoped_identifier` (Rust) / `selector_expression` over a type
(Go) gives `Enum::Variant`. Classify by the parent node:

- inside `match_pattern` / `or_pattern` → `match_pat`
- operand of a `binary_expression` with `==` / `!=` → `compare`
- everything else (call callee, field-init value, `=>` RHS, `let` value,
  bare expression) → `construct`

The enum/variant split is the last two `::` segments. To avoid matching
associated functions (`Vec::new`) vs variants, cross-check the first
segment against `symbols` where `kind='enum'` if variants are recorded;
otherwise a `PascalCase::PascalCase` heuristic (variant segment is not
immediately followed by `(` of a fn-call on a non-enum type) is a sound
first cut and matches the `string_literals` "good enough, zero config"
philosophy.

## Secondary: dead functions / symbols

The variant view is the high-precision win; dead *functions* are harder
because the call graph is incomplete. Two options, in priority order:

1. **Fix callee resolution** so `calls.callee` (or a new `callee_qname`)
   captures method calls (`recv.method()`) and qualified calls
   (`super::m::fn()`, `Self::fn()`). Then "symbols never a callee" becomes
   trustworthy.
2. **A `symbol_refs` count** — references to a symbol name outside its own
   definition span — as a heuristic, with documented false positives:
   FFI (`#[no_mangle]` / `extern`), trait-impl methods (called via the
   trait), entry points (`main`, `cli_main`), and `#[test]` fns.

## Cost

A view over `lines`, like `string_literals` — zero storage, one schema
version bump, freshen rebuilds old DBs. Dominant cost is a single
recursive scan of `lines` per query.

## Motivating codebase

safebash (`/home/dangood/src/safebash`, `.indexnudge-rust.db`). The
HereDoc cleanup itself is tracked there as
`notes/tasks/finish-heredoc-removal.md`; this FR is about making the
*next* such variant findable with one query instead of by hand.
