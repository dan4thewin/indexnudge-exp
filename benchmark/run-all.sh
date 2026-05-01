#!/bin/bash
set -eu

BENCHMARK_DIR="$(cd "$(dirname "$0")" && pwd)"
CONDITION="${1:-baseline}"
RUNS="${2:-3}"

MODELS=("claude-opus-4.6" "google-gemini-2.5-pro")
TASKS=("explore" "implement" "debug")

echo "=== Full Benchmark Suite ==="
echo "  Condition: $CONDITION"
echo "  Runs per cell: $RUNS"
echo "  Models: ${MODELS[*]}"
echo "  Tasks: ${TASKS[*]}"
echo "  Total runs: $(( ${#MODELS[@]} * ${#TASKS[@]} * RUNS ))"
echo ""

for model in "${MODELS[@]}"; do
    for task in "${TASKS[@]}"; do
        for run in $(seq 1 "$RUNS"); do
            echo ""
            echo "=============================="
            echo "  $model / $task / run $run"
            echo "=============================="
            bash "$BENCHMARK_DIR/bench.sh" "$model" "$task" "$run" "$CONDITION"
        done
    done
done

echo ""
echo "=== Summary ==="
for model in "${MODELS[@]}"; do
    for task in "${TASKS[@]}"; do
        echo ""
        echo "--- $model / $task ---"
        for run in $(seq 1 "$RUNS"); do
            json="$BENCHMARK_DIR/results/$CONDITION/$model/$task/run-${run}.json"
            if [[ -f "$json" ]]; then
                python3 "$BENCHMARK_DIR/score.py" "$json" 2>/dev/null | head -8
                echo "---"
            fi
        done
    done
done
