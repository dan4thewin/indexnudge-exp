#!/bin/bash
# Pi benchmark runner — analogous to bench.sh but for pi agent.
set -eu

BENCHMARK_DIR="$(cd "$(dirname "$0")" && pwd)"
IWE_SRC="$HOME/src/iwe"
IWE_COMMIT="5607c859bac6a72373897b1614bc9ecc2926f2c0"
INDEXNUDGE_DIR="$HOME/src/indexnudge"
PI_BIN="node /home/dangood/src/pi-mono/packages/coding-agent/dist/cli.js"

usage() {
    echo "Usage: pi-bench.sh <model> <task> <run> [condition]"
    echo "  model:     claude-opus-4.6, etc."
    echo "  task:      explore, implement, debug"
    echo "  run:       1, 2, 3, etc."
    echo "  condition: no-index, scolding-only, sqlite3-only, symbols-only, hints-only"
    exit 1
}

[[ $# -lt 3 ]] && usage

MODEL="$1"
TASK="$2"
RUN="$3"
CONDITION="${4:-no-index}"

source "$BENCHMARK_DIR/prompts.sh"

case "$TASK" in
    explore)   PROMPT="$PROMPT_EXPLORE" ;;
    implement) PROMPT="$PROMPT_IMPLEMENT" ;;
    debug)     PROMPT="$PROMPT_DEBUG" ;;
    *)         echo "Unknown task: $TASK"; exit 1 ;;
esac

# Results directory
RESULTS_DIR="$BENCHMARK_DIR/results/pi-${CONDITION}/$MODEL/$TASK"
mkdir -p "$RESULTS_DIR"

LOG="$RESULTS_DIR/run-${RUN}.log"

echo "=== Pi Benchmark Run ==="
echo "  Model:     $MODEL"
echo "  Task:      $TASK"
echo "  Run:       $RUN"
echo "  Condition: $CONDITION"

# --- Private copy of iwe ---
WORKDIR=$(mktemp -d "/tmp/pi-bench-iwe-${TASK}-${RUN}-XXXXXX")
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
AGENTS_MD=""
SHIM_PATH=""

case "$CONDITION" in
    no-index)
        # No index, no shims, no AGENTS.md — pure pi baseline
        ;;
    sqlite3-only)
        # Index exists, sqlite3 shim freshens, no other shims, no AGENTS.md
        "$INDEXNUDGE_DIR/bin-rust/indexnudge" "$WORKDIR"
        mkdir -p "$WORKDIR/.pi-shims"
        for f in sqlite3 _indexnudge_common.bash; do
            cp "$(readlink -f "$INDEXNUDGE_DIR/bin-rust/$f")" "$WORKDIR/.pi-shims/$f"
        done
        ln -sf "$INDEXNUDGE_DIR/bin-rust/indexnudge" "$WORKDIR/.pi-shims/indexnudge"
        chmod +x "$WORKDIR/.pi-shims/sqlite3"
        SHIM_PATH="$WORKDIR/.pi-shims"
        ;;
    symbols-only)
        # Index + full shims with symbols, NO hints, NO preamble scolding, NO AGENTS.md
        "$INDEXNUDGE_DIR/bin-rust/indexnudge" "$WORKDIR"
        mkdir -p "$WORKDIR/.pi-shims"
        for f in cat grep head sed tail sqlite3 _indexnudge_common.bash; do
            cp "$(readlink -f "$INDEXNUDGE_DIR/bin-rust/$f")" "$WORKDIR/.pi-shims/$f"
            chmod +x "$WORKDIR/.pi-shims/$f" 2>/dev/null || true
        done
        ln -sf "$INDEXNUDGE_DIR/bin-rust/indexnudge" "$WORKDIR/.pi-shims/indexnudge"
        # Disable hints and preamble
        sed -i '/get_hint/s/^/# /' "$WORKDIR/.pi-shims/cat" "$WORKDIR/.pi-shims/grep" \
            "$WORKDIR/.pi-shims/head" "$WORKDIR/.pi-shims/sed" "$WORKDIR/.pi-shims/tail"
        sed -i '/emit_preamble/s/^/# /' "$WORKDIR/.pi-shims/cat" "$WORKDIR/.pi-shims/grep" \
            "$WORKDIR/.pi-shims/head" "$WORKDIR/.pi-shims/sed" "$WORKDIR/.pi-shims/tail"
        SHIM_PATH="$WORKDIR/.pi-shims"
        ;;
    hints-only)
        # Index + full shims with hints, NO symbols, NO preamble, NO AGENTS.md
        # NOTE: current shims have symbols baked in; to truly isolate hints we'd need
        # baseline shims + hints. For now this uses full shims (symbols+hints) minus preamble.
        "$INDEXNUDGE_DIR/bin-rust/indexnudge" "$WORKDIR"
        mkdir -p "$WORKDIR/.pi-shims"
        for f in cat grep head sed tail sqlite3 _indexnudge_common.bash; do
            cp "$(readlink -f "$INDEXNUDGE_DIR/bin-rust/$f")" "$WORKDIR/.pi-shims/$f"
            chmod +x "$WORKDIR/.pi-shims/$f" 2>/dev/null || true
        done
        ln -sf "$INDEXNUDGE_DIR/bin-rust/indexnudge" "$WORKDIR/.pi-shims/indexnudge"
        # Disable preamble only, keep hints
        sed -i '/emit_preamble/s/^/# /' "$WORKDIR/.pi-shims/cat" "$WORKDIR/.pi-shims/grep" \
            "$WORKDIR/.pi-shims/head" "$WORKDIR/.pi-shims/sed" "$WORKDIR/.pi-shims/tail"
        SHIM_PATH="$WORKDIR/.pi-shims"
        ;;
    scolding-only)
        # Index + sqlite3 shim (for freshen) + AGENTS.md scolding, NO other shims
        "$INDEXNUDGE_DIR/bin-rust/indexnudge" "$WORKDIR"
        mkdir -p "$WORKDIR/.pi-shims"
        for f in sqlite3 _indexnudge_common.bash; do
            cp "$(readlink -f "$INDEXNUDGE_DIR/bin-rust/$f")" "$WORKDIR/.pi-shims/$f"
        done
        ln -sf "$INDEXNUDGE_DIR/bin-rust/indexnudge" "$WORKDIR/.pi-shims/indexnudge"
        chmod +x "$WORKDIR/.pi-shims/sqlite3"
        SHIM_PATH="$WORKDIR/.pi-shims"
        cp "$BENCHMARK_DIR/goosehints-scolding" "$WORKDIR/AGENTS.md"
        ;;
esac

# Set PATH with shims if needed
if [[ -n "$SHIM_PATH" ]]; then
    export PATH="$SHIM_PATH:$PATH"
fi

echo "  Workdir:   $WORKDIR"
echo "  Log:       $LOG"
echo "  Shims:     ${SHIM_PATH:-none}"
echo "  AGENTS.md: $(test -f "$WORKDIR/AGENTS.md" && echo yes || echo no)"

# --- Resolve model ---
case "$MODEL" in
    claude-opus-4.6)
        PI_MODEL="github-copilot/claude-opus-4"
        ;;
    claude-sonnet-4)
        PI_MODEL="github-copilot/claude-sonnet-4"
        ;;
    *)
        PI_MODEL="$MODEL"
        ;;
esac

# --- Run pi in non-interactive mode ---
START_TIME=$(date +%s)

# Use -nc (no context files) unless we have an AGENTS.md to load
PI_NC_FLAG="-nc"
if [[ -f "$WORKDIR/AGENTS.md" ]]; then
    PI_NC_FLAG=""
fi

$PI_BIN --model "$PI_MODEL" -p "$PROMPT" --mode json $PI_NC_FLAG \
    > "$RESULTS_DIR/run-${RUN}.json" 2>"$LOG"

END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))
echo "$ELAPSED" > "$RESULTS_DIR/run-${RUN}.time"

# Score
python3 "$BENCHMARK_DIR/pi-score.py" "$RESULTS_DIR/run-${RUN}.json" --verbose \
    > "$RESULTS_DIR/run-${RUN}.score" 2>&1

echo ""
echo "Wall clock: ${ELAPSED}s"
echo "Done."
