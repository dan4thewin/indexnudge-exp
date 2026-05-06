#!/bin/bash
set -eu

BENCHMARK_DIR="$(cd "$(dirname "$0")" && pwd)"
IWE_DIR="$HOME/src/iwe"
IWE_COMMIT="5607c859bac6a72373897b1614bc9ecc2926f2c0"
INDEXNUDGE_DIR="$HOME/src/indexnudge"

usage() {
    echo "Usage: bench.sh <model> <task> <run> [condition]"
    echo "  model:     claude-opus-4.6, gemini-2.5-pro, etc."
    echo "  task:      explore, implement, debug"
    echo "  run:       1, 2, 3, etc."
    echo "  condition: label for shim version (default: baseline)"
    exit 1
}

[[ $# -lt 3 ]] && usage

MODEL="$1"
TASK="$2"
RUN="$3"
CONDITION="${4:-baseline}"

source "$BENCHMARK_DIR/prompts.sh"

# Map model shortname to provider + model
case "$MODEL" in
    claude-opus-4.6)
        PROVIDER="github_copilot"
        MODEL_NAME="claude-opus-4.6"
        GOOSE_BIN="${GOOSE_BIN:-goose}"
        ;;
    gpt-5.5)
        PROVIDER="github_copilot"
        MODEL_NAME="gpt-5.5"
        GOOSE_BIN="$HOME/src/indexnudge/goose/target/release/goose"
        ;;
    *)
        PROVIDER="${GOOSE_PROVIDER:-anthropic}"
        MODEL_NAME="$MODEL"
        ;;
esac

case "$TASK" in
    explore)   PROMPT="$PROMPT_EXPLORE" ;;
    implement) PROMPT="$PROMPT_IMPLEMENT" ;;
    debug)     PROMPT="$PROMPT_DEBUG" ;;
    *)         echo "Unknown task: $TASK"; exit 1 ;;
esac

# Results directory
RESULTS_DIR="$BENCHMARK_DIR/results/$CONDITION/$MODEL/$TASK"
mkdir -p "$RESULTS_DIR"

SESSION_NAME="bench-${CONDITION}-${MODEL}-${TASK}-${RUN}"

echo "=== Benchmark Run ==="
echo "  Model:     $MODEL"
echo "  Task:      $TASK"
echo "  Run:       $RUN"
echo "  Condition: $CONDITION"
echo "  Session:   $SESSION_NAME"
echo ""

# Reset iwe repo
cd "$IWE_DIR"
git checkout "$IWE_COMMIT" -- . 2>/dev/null
git checkout "$IWE_COMMIT" 2>/dev/null || git checkout indexnudge-enhancements
git clean -fd 2>/dev/null

# Plant bug for debug task
if [[ "$TASK" == "debug" ]]; then
    bash "$BENCHMARK_DIR/plant-bug.sh"
fi

# Fresh index
export PATH="$INDEXNUDGE_DIR/bin-rust:$HOME/src/indexnudge/bin:$PATH"
"$INDEXNUDGE_DIR/bin-rust/indexnudge" "$IWE_DIR"

START_TIME=$(date +%s)
echo "Running goose..."
${GOOSE_BIN:-goose} run \
    -t "$PROMPT" \
    --name "$SESSION_NAME" \
    --provider "$PROVIDER" \
    --model "$MODEL_NAME" \
    -q 2>&1 | tee "$RESULTS_DIR/run-${RUN}.log"
END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))
echo ""
echo "Wall clock: ${ELAPSED}s"

# Export structured session
goose session export \
    --name "$SESSION_NAME" \
    --format json \
    > "$RESULTS_DIR/run-${RUN}.json" 2>/dev/null

# Score
echo ""
echo "=== Score ==="
echo "Wall clock: ${ELAPSED}s"
echo "$ELAPSED" > "$RESULTS_DIR/run-${RUN}.time"
python3 "$BENCHMARK_DIR/score.py" "$RESULTS_DIR/run-${RUN}.json" --verbose

# Reset iwe repo after
cd "$IWE_DIR"
git checkout "$IWE_COMMIT" -- . 2>/dev/null
git clean -fd 2>/dev/null
