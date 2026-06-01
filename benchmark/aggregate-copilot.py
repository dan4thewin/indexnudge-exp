#!/usr/bin/env python3
"""Aggregate copilot-* benchmark results into a markdown summary.

Walks benchmark/results/copilot-*/<model>/<task>/run-*.json, scores each one,
and emits a markdown report with per-condition aggregates and condition-vs-condition
comparison tables for each task.
"""
import json
import os
import sys
import statistics
from pathlib import Path

BENCH_DIR = Path(__file__).resolve().parent
RESULTS = BENCH_DIR / "results"

# Conditions in display order
CONDITIONS = [
    "copilot-baseline-bash",
    "copilot-baseline-default",
    "copilot-instructions-bash",
    "copilot-instructions-default",
    "copilot-shims-bash",
    "copilot-shims-default",
]
TASKS = ["explore", "implement", "debug"]

# Import the scorer
sys.path.insert(0, str(BENCH_DIR))
from importlib import import_module
copilot_score = import_module("copilot-score")


def score_run(json_path):
    with open(json_path) as f:
        lines = f.readlines()
    return copilot_score.score_copilot_session(lines)


def collect(model):
    """Returns {condition: {task: [run_metrics, ...]}}"""
    out = {}
    for cond in CONDITIONS:
        out[cond] = {}
        for task in TASKS:
            d = RESULTS / cond / model / task
            runs = []
            if d.exists():
                for json_file in sorted(d.glob("run-*.json")):
                    if json_file.stat().st_size == 0:
                        continue
                    # Skip run-0 (smoke) by default
                    if json_file.stem == "run-0":
                        continue
                    try:
                        s = score_run(json_file)
                        wall_file = json_file.with_suffix(".time")
                        s["wall_clock"] = int(wall_file.read_text().strip()) if wall_file.exists() else 0
                        s["run_id"] = json_file.stem
                        runs.append(s)
                    except Exception as e:
                        print(f"  WARN: failed to score {json_file}: {e}", file=sys.stderr)
            out[cond][task] = runs
    return out


def stat_line(values):
    """Returns 'mean ± stddev (n=N)' or '—' if empty."""
    if not values:
        return "—"
    if len(values) == 1:
        return f"{values[0]:.2f} (n=1)"
    mean = statistics.mean(values)
    stdev = statistics.stdev(values)
    return f"{mean:.2f} ± {stdev:.2f} (n={len(values)})"


METRICS = [
    ("tool_calls",       "tool calls"),
    ("nav_calls",        "nav calls"),
    ("db_ratio",         "db_ratio"),
    ("native_ratio",     "native_ratio"),
    ("structural_ratio", "structural_ratio"),
    ("line_ratio",       "line_ratio"),
    ("evasion_ratio",    "evasion_ratio"),
    ("output_tokens",    "output_tokens"),
    ("wall_clock",       "wall_clock (s)"),
]


def render(data, model):
    out = []
    out.append(f"# Copilot CLI benchmark: `{model}` @ xhigh")
    out.append("")
    out.append("Codebase: iwe @ `5607c859` on `indexnudge-enhancements`.")
    out.append("Tasks: `explore` (read-only), `implement` (--count flag), `debug` (planted bug).")
    out.append("")
    out.append("## Conditions")
    out.append("")
    out.append("| Condition | Index | AGENTS.md | Shims | Tools |")
    out.append("|---|---|---|---|---|")
    for cond in CONDITIONS:
        intervention = cond.replace("copilot-", "")
        tool_mode = intervention.rsplit("-", 1)[1]
        level = intervention.rsplit("-", 1)[0]
        idx = "yes" if level in ("instructions", "shims") else "no"
        agents = "yes" if level in ("instructions", "shims") else "no"
        shims = "yes" if level == "shims" else "no"
        tools = "bash+edit+create" if tool_mode == "bash" else "default (incl. view/grep/glob)"
        out.append(f"| `{cond}` | {idx} | {agents} | {shims} | {tools} |")
    out.append("")

    for task in TASKS:
        out.append(f"## Task: `{task}`")
        out.append("")
        # Table: rows = conditions, columns = metrics
        header = "| Condition | " + " | ".join(label for _, label in METRICS) + " |"
        sep = "|---" * (1 + len(METRICS)) + "|"
        out.append(header)
        out.append(sep)
        for cond in CONDITIONS:
            runs = data[cond][task]
            row = [f"`{cond}`"]
            for key, _label in METRICS:
                values = [r.get(key, 0) for r in runs]
                row.append(stat_line(values))
            out.append("| " + " | ".join(row) + " |")
        out.append("")

    # Per-condition raw category breakdown (sum across all runs and tasks)
    out.append("## Tool category totals (summed across all runs and tasks)")
    out.append("")
    all_cats = set()
    cond_totals = {}
    for cond in CONDITIONS:
        agg = {}
        for task in TASKS:
            for r in data[cond][task]:
                for cat, count in r.get("categories", {}).items():
                    agg[cat] = agg.get(cat, 0) + count
                    all_cats.add(cat)
        cond_totals[cond] = agg
    cat_order = sorted(all_cats)
    out.append("| Condition | " + " | ".join(cat_order) + " |")
    out.append("|---" * (1 + len(cat_order)) + "|")
    for cond in CONDITIONS:
        row = [f"`{cond}`"]
        for cat in cat_order:
            row.append(str(cond_totals[cond].get(cat, 0)))
        out.append("| " + " | ".join(row) + " |")
    out.append("")

    # Headline takeaways (computed)
    out.append("## Headline numbers")
    out.append("")
    for task in TASKS:
        def avg_db(cond):
            vals = [r["db_ratio"] for r in data[cond][task]]
            return statistics.mean(vals) if vals else 0
        baselines = (avg_db("copilot-baseline-bash") + avg_db("copilot-baseline-default")) / 2
        instr = (avg_db("copilot-instructions-bash") + avg_db("copilot-instructions-default")) / 2
        shims = (avg_db("copilot-shims-bash") + avg_db("copilot-shims-default")) / 2
        out.append(f"- **{task}** mean db_ratio: baseline={baselines:.2f} → instructions={instr:.2f} → shims={shims:.2f}")
    out.append("")

    return "\n".join(out)


def main():
    model = sys.argv[1] if len(sys.argv) > 1 else "claude-opus-4.7-1m-internal"
    data = collect(model)
    report = render(data, model)
    output = RESULTS / f"copilot-{model.replace('/', '-')}-summary.md"
    output.write_text(report)
    print(f"Wrote {output}")
    print()
    print(report)


if __name__ == "__main__":
    main()
