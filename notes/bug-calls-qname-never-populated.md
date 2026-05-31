# Bug: `calls.callee_qname` is never populated

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
