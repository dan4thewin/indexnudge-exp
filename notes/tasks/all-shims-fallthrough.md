title: All shims fall through silently on no DB results
status: ready
depends: []
gates:
  - tests
  - commit

The grep shim no-result fallthrough fix (grep-fallthrough) should be
applied to all shims: cat, head, tail, sed. Any shim that queries
the DB and gets zero rows should silently exec the real command
instead of printing DB chrome and exiting. The agent should never
see indexnudge output when the DB can't answer the question.

Audit each shim in bin/ for the pattern:
- Query returns empty → print banner → exit
Replace with:
- Query returns empty → exec real command
