title: Build shim effectiveness benchmark harness
status: ready
depends: []
gates:
  - tests
  - commit

## Goal

Measure whether shim changes actually improve agent navigation behavior.
Repeatable benchmark: same repo, same tasks, same conditions. Run before
and after each shim change to detect improvements and regressions.

## Setup

- Target repo: ~/src/iwe at a fixed commit (indexnudge-enhancements branch tip)
- Fresh .indexnudge.db from Rust indexer before each run
- Models: claude-opus-4.6, gemini-2.5-pro (largest context window — 1M tokens)
- 3 runs per model per task per condition

## Tasks

### Task 1 — Exploration
"How does `iwe find --meta` filter documents? Trace the data flow from CLI
argument parsing through to the filtered result set. List every function
involved and explain the filtering logic."

Pure reading comprehension. Verifiable: known chain of functions
(main.rs CLI parsing → FindOptions → DocumentFinder::find → metadata_matches
→ Graph.get_metadata → serde_yaml parse).

### Task 2 — Implementation
"Add a `--count` flag to `iwe find` that prints only the number of matching
documents instead of the full list. Ensure cargo build and cargo test pass."

Small feature touching 2-3 files (main.rs, find.rs, possibly render.rs).
Verifiable: builds, tests pass, flag works.

### Task 3 — Debug
Plant a subtle off-by-one in the --meta filter (e.g., skip first matching
document). Prompt: "Users report that `iwe find --meta status=ready` is
missing one result. Diagnose and fix."

Verifiable: the bug is found and fixed, tests pass.

## Harness

```bash
# bench.sh <model> <task> <run-number>
cd ~/src/iwe
git checkout <fixed-commit>
git clean -fd
indexnudge .   # fresh full index
goose run -t "$PROMPT" --name "bench-$MODEL-$TASK-$RUN" --output-format json -q \
    > "results/$MODEL/$TASK/run-$RUN.json"
```

Reset between runs: git checkout + git clean restores pristine state.
Task 3 needs planted bug reapplied after reset.

## Scoring

Parse session JSON to classify each shell tool call:

- direct_db_structural: sqlite3 query hitting symbols/calls/imports/defs
- direct_db_lines: sqlite3 query hitting lines/lines_fts
- shim_hit: shim served from DB (output contains sqlite3 banner)
- shim_fallthrough: shim fell through to real command
- shim_evasion: /usr/bin/grep, /usr/bin/sed, awk (deliberate bypass)
- other: build commands, cd, ls, etc.

Metrics per run:
- correct: 0 or 1 (manual rubric or automated check)
- total_tools: count of shell tool calls
- total_tokens: sum of text content lengths (proxy)
- db_ratio: (structural + lines + shim_hit) / total_tools
- structural_ratio: structural / (structural + lines)
- evasion_ratio: evasion / total_tools

Score formula (correctness gates everything):
  score = correct * (0.3 * db_ratio + 0.3 * structural_ratio
                    + 0.2 * (1 - evasion_ratio)
                    + 0.2 * efficiency_normalized)

efficiency_normalized = baseline_tokens * baseline_tools / (tokens * tools)

## Workflow

1. Establish baseline: current shims, both models, all 3 tasks, 3 runs each
2. After each shim change: rebuild, re-run full benchmark
3. Compare score distributions (mean + stddev per condition)
4. Record results in notes/benchmark-results/
