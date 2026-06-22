# `enum_variant_refs` — implementation & validation (Rust)

Implements `notes/feature-request-dead-code-detection.md`. A pure-SQL
view over `lines` (zero storage, like `string_literals`) that classifies
every `Enum::Variant` reference into `construct` / `match_pat` /
`compare`, so a never-constructed-but-referenced variant ("dead variant")
is a one-line query.

## The view

`CREATE VIEW enum_variant_refs` in `rust/src/main.rs::create_schema`
(schema v11). Columns: `file, line_num, symbol, enum, variant, context`.

Mechanics:

1. A recursive CTE peels `Enum::Variant` tokens out of each line using
   `ltrim`/`rtrim` with a word-character set to extract the identifier
   on each side of `::`. No regex (SQLite has none by default).
2. The walk advances **one `::`-segment at a time** so overlapping pairs
   in a qualified path surface: `crate::ir::RedirectOp::HereDoc` yields
   `(crate,ir)`, `(ir,RedirectOp)`, `(RedirectOp,HereDoc)`.
3. Filters: both segments must be `PascalCase`, and `enum` must be a
   locally-defined enum (`symbols.kind='enum'`). This drops module paths
   (`std::process`), method calls (`Vec::new`), and external-enum
   variants — keeping precision high for "dead code in *this* crate".
4. Per-occurrence classification from the text immediately before/after
   each token (`pre`/`post`), not the whole line.

Dead-variant query (the FR headline):

```sql
SELECT v.enum, v.variant
FROM (SELECT DISTINCT enum, variant FROM enum_variant_refs) v
WHERE NOT EXISTS (
    SELECT 1 FROM enum_variant_refs c
    WHERE c.enum = v.enum AND c.variant = v.variant
      AND c.context = 'construct'
);
```

## Two subtleties that took real-world data to surface

The FR predicted safebash would return "exactly one row: RedirectOp /
HereDoc." The first cut returned **29** — all false positives — because
of two idioms the FR's mental model didn't account for. Both were found
by running against the live safebash index, not by unit tests.

### 1. `Self::Variant` construction (29 → 5)

safebash builds `BuiltinCmd` variants inside `impl BuiltinCmd` via
`Self::Echo`, `Self::Test`, … — the idiomatic form. The view only knew
the literal enum name, so every `Self::`-constructed variant looked
never-constructed and was flagged dead (all 24 `BuiltinCmd` variants).

Fix: resolve `enum = 'Self'` to the enclosing type, taken from
`lines.symbol` (the attributed qualified name, e.g.
`BuiltinCmd::from_name` → `BuiltinCmd`). One `CASE` in the outer SELECT.

### 2. Qualified-path classification (false negatives on the real target)

`ssa_taint.rs` compares `r.op == crate::ir::RedirectOp::HereDoc` and
`validate.rs` matches `… | crate::ir::RedirectOp::HereDoc`. Two bugs:

- **Overlapping pairs were lost.** The first per-occurrence peeling
  advanced *past* the variant, so after consuming `ir` the leading `::`
  of `::RedirectOp::HereDoc` produced empty-enum rows and the
  `(RedirectOp, HereDoc)` pair was never emitted. Fix: advance by one
  segment (`rest := text after the ::`), re-examining from each segment.
- **The path prefix broke operator adjacency.** `pre` ends in
  `crate::ir::`, so `pre LIKE '%== '` / `'%| '` missed the operator that
  sits *before* the path. Fix: strip the trailing qualified-path run with
  `rtrim(pre, wordchars || ':')` before the operator/`|` checks.

Without these, the qualified `==` was misread as `construct` — which
would have given HereDoc a spurious construct row and **missed the one
variant the whole feature exists to find.** This is why the dangerous
direction (false-positive construct → false-negative dead) drove the
design: ambiguous cases bias toward `construct`, and identity match arms
(`X => X`) are split per-occurrence so the RHS construction is seen.

## Validation result (safebash, schema v11)

The dead-variant query returns **5** candidates:

| enum         | variant   | assessment |
|--------------|-----------|------------|
| `RedirectOp` | `HereDoc`  | the known target — matched in 6 arms, compared once (`ssa_taint.rs`), constructed nowhere |
| `ArithOp`    | `Not`      | genuine — codegen has `panic!("ICE: unary ! in binary arithmetic context")`; the constructor maps only binary ops; unary `!` is built as the separate `IrUnaryArithOp::Not` |
| `SsaInst`    | `Phi`      | genuine — only match patterns; SSA never inserts phi nodes |
| `StmtKind`   | `Subshell` | genuine — only match patterns; no construction site |
| `TestExpr`   | `Paren`    | genuine — only match patterns; no construction site |

So the FR's "exactly one row" was an incomplete expectation: safebash
has **four more** never-constructed (vestigial) variants beyond HereDoc.
This is the feature working — it generates candidates; a human confirms
(the remaining four could in principle be built via macro / glob-import
bare form / `Default`, none of which were found on inspection).

HereDoc cleanup sites (the FR's second query) — all 8 references:

```
codegen/redirects.rs:43   match_pat
codegen/redirects.rs:96   match_pat
emit_c.rs:1623            match_pat
emit_c.rs:1651            match_pat
emit_janet.rs:870         match_pat
emit_janet.rs:876         match_pat
ssa_taint.rs:179          compare      (qualified `==`)
validate.rs:1829          match_pat    (qualified or-pattern)
```

## Known limitations (documented, not bugs)

- **Bare variants after a glob import** (`use Enum::*; … Variant`) are
  not seen — the view requires the `Enum::Variant` qualified form.
- **Macro-constructed variants** are opaque to a line scan.
- **External-crate enums** are excluded by the local-enum filter (by
  design — you can't delete a variant you don't own).
- A variant constructed *only* on an identity match arm in a single file
  is correctly counted as constructed (per-occurrence split), but the
  reverse — a variant whose only construction is inside a `matches!`
  with a guard wrapping `=>` — may bias to `match_pat`; the dead
  detection then errs toward "not dead" (safe direction).

These match the `string_literals` "good enough, zero config" philosophy:
the view generates high-precision *candidates*, and the human verifies
before deleting.

## Tests

`enum_variant_refs_view_tests` in `rust/src/main.rs` — 5 tests:
buckets, dead-variant detection, noise exclusion, local-enum requirement,
and qualified-path classification (the regression for fix #2). The
sample also includes an identity arm (`Status::Err => Status::Err`) and a
`Self::`-constructed variant (`Status::Pending`) to lock in fix #1.

## Go

Not implemented — Go has no enums and a different shape. Tracked in
`notes/tasks/go-enum-variant-refs.md`.
