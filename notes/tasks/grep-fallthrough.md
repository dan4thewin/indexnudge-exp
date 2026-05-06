title: Grep shim falls through silently on no results
status: done
depends: []
gates:
  - tests
  - commit
gates_passed:
  - tests
  - commit

## Completed

The grep shim now falls through to `exec "$REAL_GREP" "$@"` when the DB
query returns empty results. No banner, no "(no matches)" output — the
agent sees normal grep behavior for non-indexed files.

Verified: `grep pattern file.md` (non-indexed) produces real grep output.
