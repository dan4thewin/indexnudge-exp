# Bug: `calls.callee_qname` is never populated *in Rust*

**Status: resolved via `callee_candidates` view (2026-05-31).** The column
remains in the schema — Go populates it via `go/types` and that data is
genuinely useful for cross-package call resolution. The view exposes it
on the join surface so agents get the same disambiguation API in both
languages. Original report below preserved for context.

## Resolution

Rather than drop the column or try to write a Rust type resolver,
we added a `callee_candidates` view to the shared schema. The view:

- Joins `calls` against `symbols` on the short name `callee`.
- For Go: when `callee_qname` is populated and matches a local
  symbol's `qualified_name`, returns exactly that one row.
- For Go: when `callee_qname` is populated but refers to an
  unindexed package (`fmt.Println`), returns one row with the qname
  visible and `candidate_qname IS NULL`.
- For Rust (today): `callee_qname IS NULL`, so every local symbol
  with a matching short name is returned. Agents narrow with
  proximity / parent-type / arity filters at query time.

On the iwe codebase (10,466 calls), the view resolves:

```
0 candidates  (foreign/std)    7975
1 candidate   (resolved!)      1579   ← 23x what self-qualification would give
2-5 candidates                  791   narrow ambiguity, filter-friendly
6+ candidates                   121   wide ambiguity, needs context
```

See `AGENTS.md.gpp` for the query patterns now shipped to projects.
The view is defined identically in `schema/schema.sql.gpp`,
`rust/src/main.rs::create_schema`, and `go/main.go`'s schema block.
Schema version bumped to 4 (Rust); freshen rebuilds old DBs.

## Future work

If Rust `callee_qname` resolution is ever implemented (use-tracking
+ type-of-receiver), it slots into the existing view with no schema
change — the LEFT JOIN already prefers exact qname matches when
they exist.

## Summary

The `calls` table schema includes a `callee_qname` column intended
to disambiguate calls by fully-qualified callee (e.g.
`crate::ssa::BuiltinCmd::from_name` vs another `from_name` method
in a different impl). On every safebash crate index I've looked at,
the column is **100% NULL** while `callee` is fully populated.

## Reproduction

```bash
cd ~/src/safebash
indexnudge --freshen .

sqlite3 .indexnudge-rust.db <<'SQL'
SELECT COUNT(*) AS total,
       SUM(CASE WHEN callee IS NOT NULL THEN 1 ELSE 0 END) AS callee_set,
       SUM(CASE WHEN callee_qname IS NOT NULL THEN 1 ELSE 0 END) AS qname_set
FROM calls;
SQL
```

Result on safebash today:

```
total       callee_set  qname_set
10672       10672       0
```

## Impact

Low to moderate. Two query patterns are noticeably degraded:

1. **Disambiguating overloaded short names.** `from_name` could
   live on multiple types; `match` could be a Rust keyword vs a
   user method. Without `callee_qname` you fall back to "look at
   the file and infer from imports."

2. **Filtering calls *to* a specific impl.** When a trait is
   implemented for many types, every call to `.foo()` collapses
   into `callee = 'foo'`. A populated `callee_qname` would let
   you ask "calls to `<MyType as MyTrait>::foo`."

For the safebash refactors so far the issue was sidesteppable —
the codebase doesn't have a lot of method-name collisions. But the
column existing-but-empty is a foot-gun: an agent sees it in
`.schema`, writes a query using it, gets empty results, and
silently fails.

## Possible resolutions

1. **Implement it.** Tree-sitter + the existing `symbols` table
   carry enough info to resolve qualified names for most call
   sites (free functions and `impl` methods at least).
2. **Drop the column.** If implementation is non-trivial, removing
   it from the schema is better than leaving a NULL-only column
   that suggests a feature that isn't there.
3. **Document the gap.** README / snippet note that `callee_qname`
   is reserved-for-future-use, query on `callee` only.

Any of the three is fine; the current "advertised but empty" state
is the worst option.

## Related

- `notes/feature-request-string-literal-index.md` proposes a
  separate `string_literals` table — similar shape, would benefit
  from the same qualified-name resolution if it's built.
