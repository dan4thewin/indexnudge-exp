title: Grep shim falls through silently on no results
status: ready
depends: []
gates:
  - tests
  - commit

When the grep shim queries the DB and gets no matches, it prints
`── sqlite3 ... ──` and `(no matches)` then exits 1. It should
instead fall through to `exec "$REAL_GREP" "$@"` silently. The
agent sees DB chrome for a failed query and concludes shims are
broken, then bypasses them for the rest of the session. One bad
first impression poisons the whole interaction.

Fix: replace the no-result else branch with `exec "$REAL_GREP" "$@"`.
