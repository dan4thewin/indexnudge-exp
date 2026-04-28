title: Status

## Current state

All audit remediation complete (fix-now + strengthen tiers). 9 commits ahead of pre-session state, no remote configured.

## Session 2025-04-28

- B4+B3: line-count off-by-one, unchecked DB errors
- B2+B5+S3+S4: rename protocol, DELETE journal, busy_timeout, shim safety
- parse_error column + handling + tests
- S1: incremental FTS
- T1+T2: freshen tests, edge cases
- S6: symlink blocking
- C3: shared file enumerator
- C2+C5+T4: callee_qname, types.Info, join tests
- 9 task notes created in notes/tasks/

## Observations

- ed with line addresses is the right mutation tool when shims annotate output
- edit tool's exact-match requirement is what breaks the shim loop
- Cross-language indexer contamination: shim runs wrong indexer when both on PATH (task created)
- synchronous=OFF is safe under rename protocol (rename is durability boundary)

## iwe enhancement work

Spec at ~/src/iwe/ENHANCEMENTS_SPEC.md, plan at ~/src/iwe/IMPLEMENTATION_PLAN.md.
Autonomous session implemented Phases 0-3 (bare frontmatter, --meta filter, virtual dirs).
All tests passing. Changes not yet committed or reviewed.

## Remaining work

9 tasks in notes/tasks/ — 8 ready, 1 design (file-size-limits).
