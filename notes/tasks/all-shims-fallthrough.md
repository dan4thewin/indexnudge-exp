title: All shims fall through silently on no DB results
status: done
depends: []
gates:
  - tests
  - commit
gates_passed:
  - tests
  - commit

## Completed

The grep shim fallthrough is done. The new grep shim (rewritten with
SQL JOIN) falls through on empty results. The other shims (cat, head,
tail, sed) already fall through correctly — they check `in_index` count
before serving from DB, and exec real command otherwise.

Verified by running shims against non-indexed files (.md) — all
produce real command output with no DB chrome.
