# Copilot CLI benchmark: `claude-opus-4.7-1m-internal` @ xhigh

Codebase: iwe @ `5607c859` on `indexnudge-enhancements`.
Tasks: `explore` (read-only), `implement` (--count flag), `debug` (planted bug).

## Conditions

| Condition | Index | AGENTS.md | Shims | Tools |
|---|---|---|---|---|
| `copilot-baseline-bash` | no | no | no | bash+edit+create |
| `copilot-baseline-default` | no | no | no | default (incl. view/grep/glob) |
| `copilot-instructions-bash` | yes | yes | no | bash+edit+create |
| `copilot-instructions-default` | yes | yes | no | default (incl. view/grep/glob) |
| `copilot-shims-bash` | yes | yes | yes | bash+edit+create |
| `copilot-shims-default` | yes | yes | yes | default (incl. view/grep/glob) |

## Task: `explore`

| Condition | tool calls | nav calls | db_ratio | native_ratio | structural_ratio | line_ratio | evasion_ratio | output_tokens | wall_clock (s) |
|---|---|---|---|---|---|---|---|---|---|
| `copilot-baseline-bash` | 24.00 ± 3.00 (n=3) | 21.33 ± 3.21 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 6637.33 ± 942.27 (n=3) | 119.33 ± 11.59 (n=3) |
| `copilot-baseline-default` | 17.67 ± 2.08 (n=3) | 16.67 ± 2.08 (n=3) | 0.00 ± 0.00 (n=3) | 1.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 4541.00 ± 683.39 (n=3) | 77.33 ± 11.02 (n=3) |
| `copilot-instructions-bash` | 12.33 ± 3.21 (n=3) | 12.33 ± 3.21 (n=3) | 1.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 0.36 ± 0.07 (n=3) | 0.60 ± 0.04 (n=3) | 0.00 ± 0.00 (n=3) | 4833.33 ± 659.99 (n=3) | 85.67 ± 11.55 (n=3) |
| `copilot-instructions-default` | 12.00 ± 2.00 (n=3) | 11.00 ± 2.00 (n=3) | 0.76 ± 0.22 (n=3) | 0.24 ± 0.22 (n=3) | 0.23 ± 0.03 (n=3) | 0.46 ± 0.24 (n=3) | 0.00 ± 0.00 (n=3) | 4321.67 ± 1247.09 (n=3) | 75.00 ± 24.43 (n=3) |
| `copilot-shims-bash` | 12.67 ± 3.06 (n=3) | 12.67 ± 3.06 (n=3) | 1.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 0.27 ± 0.15 (n=3) | 0.69 ± 0.17 (n=3) | 0.00 ± 0.00 (n=3) | 5248.00 ± 1791.98 (n=3) | 100.67 ± 22.94 (n=3) |
| `copilot-shims-default` | 14.00 ± 1.73 (n=3) | 13.00 ± 1.73 (n=3) | 0.98 ± 0.04 (n=3) | 0.02 ± 0.04 (n=3) | 0.37 ± 0.11 (n=3) | 0.56 ± 0.09 (n=3) | 0.00 ± 0.00 (n=3) | 5000.67 ± 411.07 (n=3) | 89.33 ± 8.96 (n=3) |

## Task: `implement`

| Condition | tool calls | nav calls | db_ratio | native_ratio | structural_ratio | line_ratio | evasion_ratio | output_tokens | wall_clock (s) |
|---|---|---|---|---|---|---|---|---|---|
| `copilot-baseline-bash` | 29.67 ± 3.21 (n=3) | 16.33 ± 2.08 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 7769.33 ± 1346.46 (n=3) | 271.67 ± 11.02 (n=3) |
| `copilot-baseline-default` | 30.33 ± 1.53 (n=3) | 19.00 ± 2.65 (n=3) | 0.00 ± 0.00 (n=3) | 0.96 ± 0.07 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 7542.00 ± 832.00 (n=3) | 250.00 ± 21.17 (n=3) |
| `copilot-instructions-bash` | 35.67 ± 7.51 (n=3) | 21.67 ± 5.51 (n=3) | 0.61 ± 0.16 (n=3) | 0.00 ± 0.00 (n=3) | 0.32 ± 0.10 (n=3) | 0.42 ± 0.17 (n=3) | 0.13 ± 0.11 (n=3) | 10651.67 ± 2404.79 (n=3) | 532.67 ± 413.73 (n=3) |
| `copilot-instructions-default` | 41.00 ± 8.54 (n=3) | 26.00 ± 5.20 (n=3) | 0.50 ± 0.09 (n=3) | 0.26 ± 0.16 (n=3) | 0.21 ± 0.06 (n=3) | 0.33 ± 0.12 (n=3) | 0.00 ± 0.00 (n=3) | 11620.67 ± 1735.59 (n=3) | 323.67 ± 35.57 (n=3) |
| `copilot-shims-bash` | 34.00 ± 3.61 (n=3) | 22.67 ± 2.08 (n=3) | 0.73 ± 0.10 (n=3) | 0.00 ± 0.00 (n=3) | 0.31 ± 0.10 (n=3) | 0.43 ± 0.10 (n=3) | 0.03 ± 0.05 (n=3) | 10345.00 ± 526.72 (n=3) | 322.67 ± 24.54 (n=3) |
| `copilot-shims-default` | 34.33 ± 2.31 (n=3) | 22.67 ± 3.06 (n=3) | 0.48 ± 0.03 (n=3) | 0.31 ± 0.17 (n=3) | 0.14 ± 0.02 (n=3) | 0.31 ± 0.04 (n=3) | 0.05 ± 0.09 (n=3) | 10095.00 ± 1636.90 (n=3) | 338.33 ± 87.32 (n=3) |

## Task: `debug`

| Condition | tool calls | nav calls | db_ratio | native_ratio | structural_ratio | line_ratio | evasion_ratio | output_tokens | wall_clock (s) |
|---|---|---|---|---|---|---|---|---|---|
| `copilot-baseline-bash` | 29.33 ± 2.31 (n=3) | 15.00 ± 3.61 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 8304.00 ± 214.17 (n=3) | 263.67 ± 7.09 (n=3) |
| `copilot-baseline-default` | 37.00 ± 4.00 (n=3) | 20.67 ± 0.58 (n=3) | 0.00 ± 0.00 (n=3) | 0.90 ± 0.05 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 0.00 ± 0.00 (n=3) | 11682.00 ± 1213.65 (n=3) | 350.00 ± 38.11 (n=3) |
| `copilot-instructions-bash` | 29.00 ± 4.58 (n=3) | 16.00 ± 4.58 (n=3) | 0.63 ± 0.26 (n=3) | 0.00 ± 0.00 (n=3) | 0.40 ± 0.31 (n=3) | 0.43 ± 0.31 (n=3) | 0.00 ± 0.00 (n=3) | 8830.00 ± 924.60 (n=3) | 296.00 ± 5.29 (n=3) |
| `copilot-instructions-default` | 43.33 ± 22.28 (n=3) | 24.67 ± 9.81 (n=3) | 0.30 ± 0.11 (n=3) | 0.44 ± 0.16 (n=3) | 0.22 ± 0.15 (n=3) | 0.13 ± 0.03 (n=3) | 0.00 ± 0.00 (n=3) | 17152.67 ± 12792.45 (n=3) | 438.67 ± 231.17 (n=3) |
| `copilot-shims-bash` | 51.00 ± 28.35 (n=3) | 33.67 ± 23.07 (n=3) | 0.64 ± 0.27 (n=3) | 0.00 ± 0.00 (n=3) | 0.36 ± 0.08 (n=3) | 0.38 ± 0.19 (n=3) | 0.22 ± 0.32 (n=3) | 20367.33 ± 16238.11 (n=3) | 500.67 ± 278.89 (n=3) |
| `copilot-shims-default` | 46.67 ± 17.62 (n=3) | 21.33 ± 8.02 (n=3) | 0.43 ± 0.15 (n=3) | 0.43 ± 0.17 (n=3) | 0.21 ± 0.07 (n=3) | 0.22 ± 0.16 (n=3) | 0.00 ± 0.00 (n=3) | 15938.00 ± 7865.75 (n=3) | 420.00 ± 133.29 (n=3) |

## Tool category totals (summed across all runs and tasks)

| Condition | build_test | direct_db_lines | direct_db_other | direct_db_structural | git | meta_tool | mutation | native_glob | native_grep | native_view | other | other_tool | shim_evasion | shim_fallthrough | shim_hit |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `copilot-baseline-bash` | 33 | 0 | 0 | 0 | 17 | 0 | 16 | 0 | 0 | 0 | 25 | 0 | 0 | 158 | 0 |
| `copilot-baseline-default` | 36 | 0 | 0 | 0 | 8 | 12 | 23 | 3 | 30 | 128 | 7 | 0 | 0 | 8 | 0 |
| `copilot-instructions-bash` | 45 | 71 | 2 | 34 | 13 | 0 | 17 | 0 | 0 | 0 | 6 | 0 | 9 | 34 | 0 |
| `copilot-instructions-default` | 47 | 53 | 5 | 29 | 10 | 11 | 23 | 0 | 0 | 59 | 11 | 2 | 0 | 39 | 0 |
| `copilot-shims-bash` | 46 | 88 | 5 | 39 | 19 | 0 | 13 | 0 | 0 | 0 | 8 | 0 | 39 | 27 | 9 |
| `copilot-shims-default` | 45 | 57 | 9 | 31 | 28 | 10 | 22 | 0 | 0 | 47 | 9 | 0 | 4 | 21 | 2 |

## Headline numbers

- **explore** mean db_ratio: baseline=0.00 → instructions=0.88 → shims=0.99
- **implement** mean db_ratio: baseline=0.00 → instructions=0.56 → shims=0.61
- **debug** mean db_ratio: baseline=0.00 → instructions=0.46 → shims=0.53
