#!/bin/bash
# Run the full copilot-opus47 benchmark suite.
# 6 conditions × 3 tasks × 3 runs = 54 runs.
set -u

BENCHMARK_DIR="$(cd "$(dirname "$0")" && pwd)"
MODEL="${MODEL:-claude-opus-4.7-1m-internal}"
EFFORT="${EFFORT:-xhigh}"
RUNS="${RUNS:-3}"
PROGRESS_LOG="${PROGRESS_LOG:-$BENCHMARK_DIR/results/copilot-sweep.log}"

CONDITIONS=(
    "copilot-baseline-bash"
    "copilot-baseline-default"
    "copilot-instructions-bash"
    "copilot-instructions-default"
    "copilot-shims-bash"
    "copilot-shims-default"
)
TASKS=("explore" "implement" "debug")

mkdir -p "$(dirname "$PROGRESS_LOG")"

log() {
    local ts; ts=$(date +"%Y-%m-%d %H:%M:%S")
    echo "[$ts] $*" | tee -a "$PROGRESS_LOG"
}

log "=== Copilot Opus-4.7 sweep ==="
log "Model: $MODEL | Effort: $EFFORT | Runs per cell: $RUNS"
log "Total runs: $(( ${#CONDITIONS[@]} * ${#TASKS[@]} * RUNS ))"

DONE_COUNT=0
SKIP_COUNT=0
FAIL_COUNT=0
TOTAL=$(( ${#CONDITIONS[@]} * ${#TASKS[@]} * RUNS ))

START_TS=$(date +%s)

for cond in "${CONDITIONS[@]}"; do
    for task in "${TASKS[@]}"; do
        for run in $(seq 1 "$RUNS"); do
            results_dir="$BENCHMARK_DIR/results/$cond/$MODEL/$task"
            json_file="$results_dir/run-${run}.json"

            if [[ -s "$json_file" ]]; then
                log "SKIP $cond / $task / run $run (exists)"
                SKIP_COUNT=$((SKIP_COUNT + 1))
                continue
            fi

            log "RUN  $cond / $task / run $run"
            if bash "$BENCHMARK_DIR/copilot-bench.sh" \
                    "$MODEL" "$task" "$run" "$cond" "$EFFORT" \
                    >>"$PROGRESS_LOG" 2>&1; then
                wall=$(cat "$results_dir/run-${run}.time" 2>/dev/null || echo "?")
                log "DONE $cond / $task / run $run (${wall}s)"
                DONE_COUNT=$((DONE_COUNT + 1))
            else
                log "FAIL $cond / $task / run $run (exit $?)"
                FAIL_COUNT=$((FAIL_COUNT + 1))
            fi
        done
    done
done

END_TS=$(date +%s)
ELAPSED=$((END_TS - START_TS))
HOURS=$((ELAPSED / 3600))
MINUTES=$(((ELAPSED % 3600) / 60))

log "=== Sweep complete ==="
log "Total: $TOTAL | Done: $DONE_COUNT | Skipped: $SKIP_COUNT | Failed: $FAIL_COUNT"
log "Elapsed: ${HOURS}h ${MINUTES}m"
