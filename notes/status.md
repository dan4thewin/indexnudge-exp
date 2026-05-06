title: Status
status: active

## Current state (2026-05-06)

### Completed this session
- All audit remediation (fix-now + strengthen tiers) — 6 commits
- iwe enhancements (phases 0-3) pushed to dan4thewin/iwe#indexnudge-enhancements
- Benchmark harness built and baseline collected (opus-4.6, gpt-5.4, gpt-5.5)
- Goose 1.33.0 built with responses API patch + tree patch
- DB renamed to `.indexnudge-go.db` / `.indexnudge-rust.db` (in progress)
- Grep shim rewritten with SQL JOIN grouped-by-symbol output (in progress)
- Grep fallthrough on no results — done

### In progress (see HANDOFF.md)
- shim-indexer-selection: DB rename mostly done, freshen and tests pending
- shim-group-by-symbol: grep works but has intermittent hang under investigation
- Both blocked on debugging the hang

### Key benchmark findings
- Opus 4.6: never uses symbols table on explore/debug tasks
- GPT-5.4/5.5: use structural queries naturally
- Zero evasion from both models — shims work for indexed files
- The intervention: grouped-by-symbol output should move Opus toward structural navigation

### Task summary
- 3 done: benchmark-harness, grep-fallthrough, all-shims-fallthrough
- 2 active: shim-indexer-selection, shim-group-by-symbol
- 1 blocked: rotating-nudges (awaiting benchmark data)
- 10 ready: remaining strengthen + infrastructure tasks

### Unpushed
12+ commits on master (no remote configured — see tasks/git-remote)
