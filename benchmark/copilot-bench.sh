#!/bin/bash
# Copilot CLI benchmark runner — opus-4.7-1m-internal / xhigh on iwe.
# Mirrors pi-bench.sh structure: tmpdir clone, condition-specific shim/AGENTS.md
# setup, JSONL output, immediate scoring.
set -eu

BENCHMARK_DIR="$(cd "$(dirname "$0")" && pwd)"
IWE_SRC="$HOME/src/iwe"
IWE_COMMIT="5607c859bac6a72373897b1614bc9ecc2926f2c0"
INDEXNUDGE_DIR="$HOME/src/indexnudge"
COPILOT_BIN="${COPILOT_BIN:-$HOME/bin/copilot}"

usage() {
    echo "Usage: copilot-bench.sh <model> <task> <run> [condition] [effort]"
    echo "  model:     claude-opus-4.7-1m-internal, etc."
    echo "  task:      explore, implement, debug"
    echo "  run:       1, 2, 3, ..."
    echo "  condition: copilot-baseline-bash, copilot-baseline-default,"
    echo "             copilot-instructions-bash, copilot-instructions-default,"
    echo "             copilot-shims-bash, copilot-shims-default"
    echo "  effort:    low, medium, high, xhigh, max (default: xhigh)"
    exit 1
}

[[ $# -lt 3 ]] && usage

MODEL="$1"
TASK="$2"
RUN="$3"
CONDITION="${4:-copilot-baseline-bash}"
EFFORT="${5:-xhigh}"

source "$BENCHMARK_DIR/prompts.sh"

case "$TASK" in
    explore)   PROMPT="$PROMPT_EXPLORE" ;;
    implement) PROMPT="$PROMPT_IMPLEMENT" ;;
    debug)     PROMPT="$PROMPT_DEBUG" ;;
    *)         echo "Unknown task: $TASK"; exit 1 ;;
esac

RESULTS_DIR="$BENCHMARK_DIR/results/$CONDITION/$MODEL/$TASK"
mkdir -p "$RESULTS_DIR"
LOG="$RESULTS_DIR/run-${RUN}.log"

echo "=== Copilot Benchmark Run ==="
echo "  Model:     $MODEL"
echo "  Task:      $TASK"
echo "  Run:       $RUN"
echo "  Condition: $CONDITION"
echo "  Effort:    $EFFORT"

# --- Private copy of iwe ---
WORKDIR=$(mktemp -d "/tmp/copilot-bench-iwe-${TASK}-${RUN}-XXXXXX")
trap 'rm -rf "$WORKDIR"' EXIT
git clone --no-checkout "$IWE_SRC" "$WORKDIR" 2>/dev/null
cd "$WORKDIR"
git checkout "$IWE_COMMIT" -- . 2>/dev/null

# Plant bug for debug task
if [[ "$TASK" == "debug" ]]; then
    export IWE_WORKDIR="$WORKDIR"
    bash "$BENCHMARK_DIR/plant-bug.sh"
fi

# --- Condition-specific setup ---
SHIM_PATH=""
HAS_AGENTS_MD="no"

# Split condition into intervention level and tool mode
INTERVENTION="${CONDITION#copilot-}"          # strip copilot- prefix
TOOL_MODE="${INTERVENTION##*-}"               # bash | default
LEVEL="${INTERVENTION%-*}"                    # baseline | instructions | shims

case "$LEVEL" in
    baseline)
        # No index, no shims, no AGENTS.md
        ;;
    instructions)
        # Index + AGENTS.md, no shims
        "$INDEXNUDGE_DIR/bin-rust/indexnudge" "$WORKDIR" >/dev/null
        cp "$BENCHMARK_DIR/goosehints-scolding" "$WORKDIR/AGENTS.md"
        HAS_AGENTS_MD="yes"
        ;;
    shims)
        # Index + AGENTS.md + shims on PATH
        "$INDEXNUDGE_DIR/bin-rust/indexnudge" "$WORKDIR" >/dev/null
        cp "$BENCHMARK_DIR/goosehints-scolding" "$WORKDIR/AGENTS.md"
        HAS_AGENTS_MD="yes"
        mkdir -p "$WORKDIR/.copilot-shims"
        for f in cat grep head sed tail sqlite3 _indexnudge_common.bash; do
            cp "$(readlink -f "$INDEXNUDGE_DIR/bin-rust/$f")" "$WORKDIR/.copilot-shims/$f"
            chmod +x "$WORKDIR/.copilot-shims/$f" 2>/dev/null || true
        done
        ln -sf "$INDEXNUDGE_DIR/bin-rust/indexnudge" "$WORKDIR/.copilot-shims/indexnudge"
        SHIM_PATH="$WORKDIR/.copilot-shims"
        ;;
    *)
        echo "Unknown intervention level: $LEVEL"
        exit 1
        ;;
esac

# Tool-mode flag
case "$TOOL_MODE" in
    bash)
        AVAILABLE_TOOLS_FLAG=(--available-tools=bash,edit,create)
        ;;
    default)
        AVAILABLE_TOOLS_FLAG=()
        ;;
    *)
        echo "Unknown tool mode: $TOOL_MODE"
        exit 1
        ;;
esac

# Custom-instructions flag: load AGENTS.md only when we wrote one
if [[ "$HAS_AGENTS_MD" == "yes" ]]; then
    CUSTOM_INSTR_FLAG=()
else
    CUSTOM_INSTR_FLAG=(--no-custom-instructions)
fi

# PATH with shims (only for shims-* conditions)
if [[ -n "$SHIM_PATH" ]]; then
    export PATH="$SHIM_PATH:$PATH"
fi

echo "  Workdir:   $WORKDIR"
echo "  Log:       $LOG"
echo "  Shims:     ${SHIM_PATH:-none}"
echo "  AGENTS.md: $HAS_AGENTS_MD"
echo "  Tools:     ${AVAILABLE_TOOLS_FLAG[*]:-default}"

# --- Run copilot in non-interactive mode ---
START_TIME=$(date +%s)

cd "$WORKDIR"
"$COPILOT_BIN" \
    -p "$PROMPT" \
    --model "$MODEL" \
    --effort "$EFFORT" \
    --output-format json \
    --allow-all \
    --no-ask-user \
    "${CUSTOM_INSTR_FLAG[@]}" \
    "${AVAILABLE_TOOLS_FLAG[@]}" \
    > "$RESULTS_DIR/run-${RUN}.json" 2>"$LOG" \
    || echo "copilot exited non-zero (continuing)" >&2

END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))
echo "$ELAPSED" > "$RESULTS_DIR/run-${RUN}.time"

# Score
python3 "$BENCHMARK_DIR/copilot-score.py" "$RESULTS_DIR/run-${RUN}.json" --verbose \
    > "$RESULTS_DIR/run-${RUN}.score" 2>&1 \
    || echo "scoring failed (continuing)" >&2

echo ""
echo "Wall clock: ${ELAPSED}s"
echo "Done."
