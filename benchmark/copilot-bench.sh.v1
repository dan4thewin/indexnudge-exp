#!/bin/bash
set -eu

BENCHMARK_DIR="$(cd "$(dirname "$0")" && pwd)"
IWE_DIR="$HOME/src/iwe"
IWE_COMMIT="5607c859bac6a72373897b1614bc9ecc2926f2c0"
INDEXNUDGE_DIR="$HOME/src/indexnudge"

MODEL="${1:-gpt-5.5}"
TASK="${2:-explore}"
RUN="${3:-1}"
CONDITION="${4:-copilot-baseline}"

source "$BENCHMARK_DIR/prompts.sh"

case "$TASK" in
    explore)   PROMPT="$PROMPT_EXPLORE" ;;
    implement) PROMPT="$PROMPT_IMPLEMENT" ;;
    debug)     PROMPT="$PROMPT_DEBUG" ;;
    *)         echo "Unknown task: $TASK"; exit 1 ;;
esac

RESULTS_DIR="$BENCHMARK_DIR/results/$CONDITION/$MODEL/$TASK"
mkdir -p "$RESULTS_DIR"

# Reset iwe repo
cd "$IWE_DIR"
git checkout "$IWE_COMMIT" -- . 2>/dev/null
git checkout indexnudge-enhancements 2>/dev/null
git clean -fd 2>/dev/null

# Plant bug for debug task
if [[ "$TASK" == "debug" ]]; then
    bash "$BENCHMARK_DIR/plant-bug.sh"
fi

# Fresh index
export PATH="$INDEXNUDGE_DIR/bin-rust:$INDEXNUDGE_DIR/bin:$PATH"
"$INDEXNUDGE_DIR/bin-rust/indexnudge" "$IWE_DIR"

# Build the full prompt with context
CONTEXT="You are working in $IWE_DIR — a Rust workspace for a markdown knowledge-graph tool.

## Codebase Index

There is a .indexnudge.db SQLite database in this directory containing every line
of source and all structural metadata. Use it to navigate the code:

  sqlite3 .indexnudge.db .schema          -- see all tables
  sqlite3 .indexnudge.db \"SELECT name, kind, file, start_line, end_line FROM symbols WHERE name LIKE '%find%'\"
  sqlite3 .indexnudge.db \"SELECT content FROM lines WHERE file='path' AND line_num BETWEEN 10 AND 20\"
  sqlite3 .indexnudge.db \"SELECT DISTINCT file FROM files\"

Tables: symbols (name, qualified_name, kind, file, start_line, end_line),
        calls (caller_file, callee, callee_qname, kind),
        imports, defs, files, directories, lines, lines_fts

Shell shims for cat, grep, head, sed, tail also query the DB automatically
and show annotated output with line numbers.

## Build and Test

  cargo build
  cargo test
  cargo test -p liwe

## Editing

Use ed for file mutations — it accepts line-number addresses (10,20c, 15a, 30d).

## Task

$PROMPT"

echo "=== Copilot Benchmark ==="
echo "  Model:     $MODEL"
echo "  Task:      $TASK"
echo "  Run:       $RUN"
echo "  Condition: $CONDITION"
echo ""

START_TIME=$(date +%s)

cd "$IWE_DIR"
~/bin/copilot -p "$CONTEXT" --model "$MODEL" --allow-all \
    2>&1 | tee "$RESULTS_DIR/run-${RUN}.log"

END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))

echo ""
echo "Wall clock: ${ELAPSED}s"
echo "$ELAPSED" > "$RESULTS_DIR/run-${RUN}.time"

# Reset iwe repo after
cd "$IWE_DIR"
git checkout "$IWE_COMMIT" -- . 2>/dev/null
git clean -fd 2>/dev/null
