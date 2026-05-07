#!/bin/bash
set -eu

BENCHMARK_DIR="$(cd "$(dirname "$0")" && pwd)"
CONDITION="${1:-baseline}"
RUNS="${2:-3}"

MODELS=("claude-opus-4.6" "gpt-5.5")
TASKS=("explore" "implement" "debug")

echo "=== Full Baseline Suite ==="
echo "  Runs per cell: $RUNS"
echo "  Total runs: $(( ${#MODELS[@]} * ${#TASKS[@]} * RUNS ))"
echo "  Estimated time: ~90 minutes"
echo ""

# Skip runs that already have results
for model in "${MODELS[@]}"; do
    for task in "${TASKS[@]}"; do
        for run in $(seq 1 "$RUNS"); do
            results_dir="$BENCHMARK_DIR/results/$CONDITION/$model/$task"
            if [[ -f "$results_dir/run-${run}.json" ]]; then
                echo "SKIP: $model / $task / run $run (already exists)"
                continue
            fi
            echo ""
            echo "=============================="
            echo "  $model / $task / run $run"
            echo "=============================="
            bash "$BENCHMARK_DIR/bench.sh" "$model" "$task" "$run" "$CONDITION"
        done
    done
done

echo ""
echo "=== Baseline Summary ==="
for model in "${MODELS[@]}"; do
    echo ""
    echo "=== $model ==="
    for task in "${TASKS[@]}"; do
        echo ""
        echo "--- $task ---"
        for run in $(seq 1 "$RUNS"); do
            json="$BENCHMARK_DIR/results/$CONDITION/$model/$task/run-${run}.json"
            time_file="$BENCHMARK_DIR/results/$CONDITION/$model/$task/run-${run}.time"
            if [[ -f "$json" ]]; then
                wall=$(cat "$time_file" 2>/dev/null || echo "?")
                python3 "$BENCHMARK_DIR/score.py" "$json" 2>/dev/null | \
                    awk -v r="$run" -v w="$wall" 'NR<=3{printf "%s ", $0} END{printf "| wall=%ss | run=%s\n", w, r}'
            fi
        done
    done
done
