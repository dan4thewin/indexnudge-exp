#!/bin/bash
set -u

BENCHMARK_DIR="$(cd "$(dirname "$0")" && pwd)"
CONDITION="${1:-baseline}"
RUNS="${2:-3}"

MODELS=("claude-opus-4.6" "gpt-5.5")
TASKS=("explore" "implement" "debug")

TOTAL=$(( ${#MODELS[@]} * ${#TASKS[@]} * RUNS ))

echo "=== Full Benchmark Suite ==="
echo "  Condition: $CONDITION"
echo "  Runs per cell: $RUNS"
echo "  Models: ${MODELS[*]}"
echo "  Tasks: ${TASKS[*]}"
echo "  Total runs: $TOTAL"
echo ""

# Run sequentially but each in its own session (setsid) so killing
# this script doesn't kill the in-progress goose run.
DONE=0
FAILED=0

for model in "${MODELS[@]}"; do
    for task in "${TASKS[@]}"; do
        for run in $(seq 1 "$RUNS"); do
            label="$model / $task / run $run"
            results_dir="$BENCHMARK_DIR/results/$CONDITION/$model/$task"

            # Skip if already completed
            if [[ -f "$results_dir/run-${run}.time" ]]; then
                echo "  SKIP (done): $label"
                ((DONE++))
                continue
            fi

            echo -n "  RUN: $label ..."
            if setsid bash "$BENCHMARK_DIR/bench.sh" "$model" "$task" "$run" "$CONDITION"; then
                echo " ok"
                ((DONE++))
            else
                echo " FAILED"
                ((FAILED++))
            fi
        done
    done
done

echo ""
echo "=== Complete: $DONE done, $FAILED failed (of $TOTAL) ==="

# Summary
echo ""
echo "=== Summary ==="
for model in "${MODELS[@]}"; do
    for task in "${TASKS[@]}"; do
        echo ""
        echo "--- $model / $task ---"
        for run in $(seq 1 "$RUNS"); do
            score="$BENCHMARK_DIR/results/$CONDITION/$model/$task/run-${run}.score"
            if [[ -f "$score" ]]; then
                cat "$score"
                echo "---"
            fi
        done
    done
done
