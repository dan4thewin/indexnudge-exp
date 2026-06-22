title: Go enum_variant_refs (dead typed-constant detection)
status: ready
depends: []
gates:
  - tests
  - commit

Port the Rust `enum_variant_refs` view to the Go indexer — but the shape
is different because **Go has no enums**, and Go's existing dead-code
tooling already covers part of the gap.

## Background: does Go even need this?

Yes, for one specific pattern, but not for dead *functions*.

Go's existing tooling (stronger than Rust's default toolchain for
functions):

- **`staticcheck` U1000** — flags unused *unexported* funcs/types/
  consts/fields. Goes **silent for exported** identifiers (external
  packages might use them) — same blind spot rustc's `dead_code` has.
- **`golang.org/x/tools/cmd/deadcode`** — whole-program reachability
  from `main`. Finds unreachable *functions* even if exported. But:
  function-level only, needs a main program (not libraries), blind to
  reflection/cgo/plugins.

So Go's **dead-function** story is already good — the secondary
"dead functions" idea from the Rust FR is lower value in Go; point
users at `deadcode` / `staticcheck` instead of building it.

**The gap Go shares with Rust:** a *value* that is matched/compared but
never constructed. Go's analog of a never-constructed enum variant is a
**typed constant** (or a sum-type-via-interface marker) that appears in
`switch`/`==` but is never assigned:

```go
type RedirectOp int
const (
    Append RedirectOp = iota
    HereDoc            // matched in switches, compared, never produced
)
```

Neither `staticcheck` (sees it as "used" — it's referenced) nor
`deadcode` (functions only) catches this. This is the real Go gap the
view fills.

## Implementation shape (different from Rust)

Rust keys on `scoped_identifier` `Enum::Variant`. Go's equivalent is a
**`selector_expression`/`Ident` over a typed const**, but Go variant
references are usually written **bare** (`HereDoc`, not `RedirectOp.HereDoc`),
because the constants live at package scope. That makes the pure-SQL
`::`-peeling trick from the Rust view inapplicable — there's no
`Enum::Variant` token to peel.

Two viable approaches:

1. **Extraction-time (recommended for Go).** During the existing
   tree-sitter/go-ast walk, record references to package-level typed
   constants into a table (or reuse `refs`), classified by context
   (construct = assignment/return/composite-lit/`:` field value;
   match_pat = `case` clause; compare = `==`/`!=` operand). Resolve the
   bare identifier to its declaring type via `go/types` (already loaded
   in the Go indexer — see `loadPackages`). This is more robust than
   the Rust line-based view and leverages type info the Rust side
   doesn't have.

2. **Line-based view (parity with Rust).** Only works for the qualified
   form `pkg.Const`; misses the common bare form. Not recommended.

Prefer approach 1: a `dead_const` candidate is a package-level const of
a named type that appears in `case`/`==` contexts but never on the LHS
of an assignment, a `return`, a composite-literal field, or a function
argument. Use `go/types` `Info.Uses`/`Info.Defs` to bind identifiers to
their `types.Const` object, and the object's type to group "variants".

## Reference

- Rust impl: `rust/src/main.rs` `CREATE VIEW enum_variant_refs` +
  `enum_variant_refs_view_tests`.
- Validation write-up: `notes/enum-variant-refs-validation.md`
  (Self-resolution + qualified-path subtleties — the Go version sidesteps
  both by using type info instead of text).
- Original FR: `notes/feature-request-dead-code-detection.md`.

## Out of scope

- Dead *functions* in Go — use `x/tools/cmd/deadcode` and
  `staticcheck U1000`; don't reimplement.
