title: Build shim effectiveness benchmark harness
status: done
depends: []
gates:
  - tests
  - commit
gates_passed:
  - tests
  - commit

## Completed

Benchmark harness built and baseline collected:
- 2 models: claude-opus-4.6, gpt-5.5 (via patched goose 1.33.0 with responses API)
- 3 tasks: explore, implement, debug (against iwe codebase)
- 3 runs per model per task
- Scoring: db_ratio, structural_ratio, evasion_ratio, wall clock
- Results in benchmark/results/baseline/

Also ran: gpt-5.4 (3 models total), gemini-2.5-pro (dropped — unreliable).
Copilot CLI comparison with gpt-5.5 (different tool ecosystem, not directly comparable).

## Key findings

- Opus 4.6: high db_ratio (~0.87), zero structural queries on explore/debug,
  only uses symbols during implement. Never evades shims.
- GPT-5.5: also high db_ratio, uses structural queries on every task type
  (struct_ratio 0.17-0.35). Zero evasion.
- GPT-5.4: highest structural ratio (0.4-0.58 on explore). Zero shim usage —
  goes direct to sqlite3.
- The nudge opportunity: Opus needs grouped-by-symbol output to see structure
  without explicitly querying symbols table.

## Files

- benchmark/bench.sh — single run driver
- benchmark/run-baseline.sh — full suite runner
- benchmark/score.py — session JSON scorer
- benchmark/prompts.sh — task prompts
- benchmark/plant-bug.sh — debug task bug planter
- benchmark/copilot-bench.sh — copilot CLI comparison
- benchmark/results/baseline/ — all baseline data
