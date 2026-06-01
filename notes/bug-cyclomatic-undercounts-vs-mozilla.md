# Bug: `symbols.cyclomatic` undercounts vs Mozilla `rust-code-analysis`

## Summary

`symbols.cyclomatic` (introduced in user_version 7) systematically
returns lower numbers than Mozilla's `rust-code-analysis` for the
same Rust functions in the same codebase. The rankings agree on
hotspots, but the absolute numbers don't, and the function counts
differ by ~50%.

This blocks safebash from deleting its external CC runner
(`contrib/cyclo.sh`) and using the index column as the source of
truth — once the numbers go into commit-hook thresholds, they
need to be defensible against a well-tested reference.

## Aggregate comparison (safebash, `sash/src/` only)

| metric | Mozilla `rust-code-analysis` | indexnudge | delta |
|---|---:|---:|---:|
| functions | 656 | 438 | **−218** |
| avg CC | 6.8 | 8.8 | +2.0 |
| max CC | 58 | 48 | −10 |

The function-count gap is the largest single piece of evidence.
Mozilla counts ~50% more "functions" — almost certainly because it
surfaces closures and nested fns as separate counted entities.
indexnudge's own note says nested closures *are* separate symbols
but don't contribute to their enclosing function's CC. If they
*aren't* showing up in `symbols` at all, that explains both the
count gap and (partly) the higher avg-CC for the index (closures
are mostly small, so removing them raises the average of what's
left).

## Per-function comparison (top of safebash hotspots)

Every function here exists in both tools' output. Same name, same
file, same `start_line`.

| function | Mozilla | index | delta |
|---|---:|---:|---:|
| `handle_param_expansion` | 58 | 45 | **−13** |
| `lower_one` | 57 | 45 | **−12** |
| `handle_pipeline` (transform) | 54 | 46 | −8 |
| `lower_value` | 48 | 30 | **−18** |
| `scan_word` | 47 | 47 | 0 |
| `check_stmt` | 47 | 47 | 0 |
| `parse_parameter_expansion` | 44 | 42 | −2 |
| `lower_for` | 44 | 31 | **−13** |
| `lower_case` | 44 | 33 | **−11** |
| `validate_stmt_ctx` | 43 | 41 | −2 |
| `emit` | 42 | 42 | 0 |
| `emit_param_expansion_inline` | 41 | 32 | −9 |
| `verify_ssa_taint` | 36 | 31 | −5 |
| `allocate_variables` | 36 | 24 | **−12** |

Three exact matches → the basic counting rules line up in many
cases. The non-zero deltas are all *negative* (index lower than
Mozilla) — not jitter, a systematic miss.

## Likely missed node kinds

Spot-checking which functions have the largest deltas:

- `lower_value` (−18), `lower_for` (−13), `lower_case` (−11),
  `allocate_variables` (−12) — these are heavy on `if let` /
  `while let` / `match` over enum variants with bindings and
  guards.

- `handle_param_expansion` (−13), `lower_one` (−12) — heavy on
  long `match` ladders, some with `|`-alternated patterns and
  some with guards.

Candidates worth checking in the indexnudge tree-sitter walk:

1. **`if let` / `while let`** — `if_let_expression`,
   `while_let_expression` nodes. Should each contribute 1.
2. **`let else`** — `let_declaration` with an `else` clause.
   Should contribute 1.
3. **`match` arm guards** (`pat if cond => ...`) — the `if cond`
   should contribute 1 in addition to the arm itself.
4. **`|`-alternated patterns** in match arms (`A | B | C => ...`).
   Different tools count these differently:
   - McCabe original: 1 arm = 1, regardless of alternations.
   - Mozilla's tool: each pattern alternative counts. (Likely the
     source of much of the delta.)
   - Strictly defensible either way, but the docstring should say
     which convention is used.
5. **`?` operator** — listed in your snippet ("Rust `?` operator")
   so presumably implemented. Worth confirming.
6. **`&&` and `||` inside guard conditions** (vs. inside regular
   expression positions). Both should count.

The `|`-alternated patterns case is the most likely single
contributor. `lower_case` (CC ~33/44) almost certainly uses
`Some(a) | Some(b) | Some(c)` patterns; if the index counts that
arm as +1 and Mozilla counts it as +3, you get exactly the kind
of single-digit delta we see across many of the larger functions.

## Reproduction

```bash
cd ~/src/safebash

# Mozilla
bash contrib/cyclo.sh sash/src --top 30
# 656 functions, avg 6.8, max 58

# indexnudge
sqlite3 .indexnudge-rust.db "
  SELECT COUNT(*), AVG(cyclomatic), MAX(cyclomatic)
  FROM symbols
  WHERE kind IN ('function','method')
    AND file LIKE 'sash/src/%'
    AND cyclomatic IS NOT NULL;"
# 438, 8.8, 48

# Per-function side-by-side
sqlite3 .indexnudge-rust.db "
  SELECT name, file, start_line, cyclomatic
  FROM symbols
  WHERE name IN ('handle_param_expansion','lower_one','handle_pipeline',
                 'lower_value','scan_word','check_stmt','lower_for',
                 'lower_case','emit','emit_param_expansion_inline',
                 'verify_ssa_taint','allocate_variables')
    AND kind IN ('function','method') AND cyclomatic IS NOT NULL
  ORDER BY cyclomatic DESC;"
```

Then compare against `cyclo.sh` output (above).

## Suggested actions

1. **Document the counting convention.** The AGENTS.md snippet
   says "1 + each `if`/loop/match arm/`&&`/`||`/`?`" — extend
   that to explicitly cover `if let`, `while let`, `let else`,
   match guards, and the `A | B` alternation choice. A reader
   should be able to predict the number without running the
   indexer.

2. **Add the missing nodes** if they really aren't counted.
   `if_let_expression`, `while_let_expression`, `let_else`, and
   `match_arm`'s guard `if` are the high-confidence ones.

3. **Surface closures in `symbols`** (or add a separate symbol
   kind for them) so the function-count metric matches. Then
   their CC becomes queryable too.

4. **Add a self-test** against a small Rust fixture with known
   CC numbers, asserting the indexer matches. This is a small
   investment that prevents regressions and gives users
   confidence in the numbers.

## Severity

Medium. The rankings agree, so as a "what should I refactor
next?" signal the index is fine. As an absolute metric for
commit-hook thresholds it's not yet trustworthy.

## Related

- `notes/feature-request-complexity-metric.md` — the original FR
  that this implements.
