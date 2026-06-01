# Bug: `symbols.cyclomatic` undercounts vs Mozilla `rust-code-analysis`

**Status: largely resolved (2026-05-31).** Three missing decision-point
kinds added to the counter; one over-counting attempt (or-pattern
alternatives) tested and rejected. Schema version bumped 7 → 8 so
existing DBs auto-rebuild on next `--freshen` to pick up the
recomputed values.

## Resolution

The undercount had two contributing causes, both fixed in
`rust/src/main.rs::count_decision_points` and the equivalent
`go/main.go::cyclomaticComplexity`:

1. **Match-arm guards** (`pat if cond => ...`) — were missed because
   the guard expression sits as a second child of `match_pattern`,
   not under a separate guard node. Now: `match_pattern` with named
   children beyond the pattern itself adds +1.

2. **`let-else`** (`let pat = expr else { ... };`) — was missed because
   `let_declaration` wasn't in the decision-point match arm. Now:
   `let_declaration` with a `block` child adds +1.

3. **Closures** — contribute +1 to the enclosing function (matching
   Mozilla's convention that each closure is an alternative codepath
   being constructed). The closure body itself remains a separate
   symbol with its own CC.

One tested-and-rejected fix:

- **`or_pattern` alternatives** (`A | B | C => ...`) — the original
  bug report speculated Mozilla counts each alternative. Testing on
  safebash showed this over-counts heavily on lexer-style code
  (`scan_word` jumped from 47 to 64, against Mozilla's 47). Reverted.
  McCabe original is preferred: a multi-alternative arm is one
  decision regardless of `|` count.

## Numbers after the fix

Against the bug report's safebash baseline (Mozilla 656 fns, avg 6.8,
max 58 — `sash/src/` only):

```
        Mozilla   indexnudge   delta
fns         656          438    -218  (closures still missing from `symbols`)
avg_cc      6.8          9.3    +2.5  (smaller because we don't list closures
                                       as fns, so the average is over the
                                       larger non-closure functions only)
max_cc       58           58       0  ✓
```

Per-function deltas (Mozilla → ours):

| function | Mozilla | now | delta |
|---|---:|---:|---:|
| handle_param_expansion | 58 | 58 | **0** |
| handle_pipeline (transform) | 54 | 54 | **0** |
| check_stmt | 47 | 47 | **0** |
| scan_word | 47 | 47 | **0** |
| parse_parameter_expansion | 44 | 44 | **0** |
| validate_stmt_ctx | 43 | 43 | **0** |
| emit | 42 | 42 | **0** |
| emit_param_expansion_inline | 41 | 41 | **0** |
| lower_for | 44 | 43 | −1 |
| allocate_variables | 36 | 34 | −2 |
| verify_ssa_taint | 36 | 34 | −2 |
| lower_case | 44 | 41 | −3 |
| lower_value | 48 | 39 | −9 |
| lower_one | 57 | 47 | −10 |

**8 of 14 functions match exactly.** Worst remaining delta is −10
(lower_one). Mean delta dropped from −8.3 (before) to −3.1 (now).
Rankings are stable.

## Remaining gap: closures don't appear in `symbols`

This is the source of the 656 vs 438 function-count delta. Mozilla
lists every closure as a separate counted entity; indexnudge's
visitor skips them.

Deferred. Adding closures as `symbols` rows requires:

  - Inventing synthetic names (`<closure@line:col>` or similar)
  - Deciding whether they should appear in `callee_candidates`,
    `lines.symbol` attribution, etc.
  - Coordinating with the other 3 views that touch `symbols`

The CC counter already adds +1 for each closure in the enclosing
function's CC, so the *complexity* effect is captured. Only the
function-count metric differs.

## Counting convention now documented

In `AGENTS.md.gpp` 'Complexity hotspots' section. Users querying
`symbols.cyclomatic` can predict the number from the source without
running the indexer.

## Reproduction (after fix)

```bash
cd ~/src/safebash
rm -f .indexnudge-rust.db
indexnudge .

sqlite3 .indexnudge-rust.db "
  SELECT COUNT(*), AVG(cyclomatic), MAX(cyclomatic)
  FROM symbols
  WHERE kind IN ('function','method')
    AND file LIKE 'sash/src/%'
    AND cyclomatic IS NOT NULL;"
# 438, 9.3, 58
```

## Related

- `notes/feature-request-complexity-metric.md` — the original FR.

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

---

## Update — 2026-05-31 (after schema v8 fixes)

After v8 (match-arm guards, let-else, closures-as-+1-in-enclosing),
the picture is dramatically better. Mozilla-confirmed on safebash:

```
                Mozilla   v1      v8
fns                656    438    438     (still off — see below)
avg cc             6.8    8.8    9.3
max cc              58     48     58 ✓
```

8 of 14 hotspots now match exactly. Mean per-fn delta dropped from
−8.3 to −3.1. Rejected `|`-alternated patterns after testing —
correctly identified as catastrophically over-counting on
lexer-style code (e.g., `b' ' | b'\t' | ... | b'$' => break`).

### Residual on six functions in safebash

| function | Mozilla | v8 | delta |
|---|---:|---:|---:|
| `lower_one` | 57 | 47 | **−10** |
| `lower_value` | 48 | 39 | **−9** |
| `lower_case` | 44 | 41 | −3 |
| `allocate_variables` | 36 | 34 | −2 |
| `verify_ssa_taint` | 36 | 34 | −2 |
| `lower_for` | 44 | 43 | −1 |

All deltas negative. Three observations from inspecting `lower_one`
and `lower_value`:

1. **`matches!` macro** — `lower_one` contains at least 2 sites
   (`sash/src/transform/statements.rs:104,134`) and the broader
   file has 5. Each `matches!(x, A | B | C)` expands to a `match`
   with one explicit arm and a `_` arm — Mozilla appears to count
   each as +1. Tree-sitter sees a `macro_invocation` node, not a
   `match_expression`, so a node-kind-based counter would miss
   them unless macro names are special-cased.

2. **Nested item-level `fn` declarations** —
   `sash/src/transform/statements.rs:248` defines `fn uncheck_block`
   inside `lower_one`. By the same convention you applied to
   closures (+1 in the enclosing fn), nested item-level `fn`
   declarations should also count +1.

3. **Multi-line struct-destructure match arms** —
   `sash/src/codegen/values.rs` has at least 4 arms shaped like

   ```rust
   SsaValue::ParamExpansion {
       var, op, word, ...
   } => handle_param_expansion(...)
   ```

   These are normal `match_arm` nodes to tree-sitter and *should*
   be counted, but worth verifying the walk handles the
   pattern-block-then-`=>` layout. If counted, point 1+2 cover
   most of the gap; if not, this is the third source.

### Function-count gap (438 vs 656)

The agent's v2 note says closures-as-symbols was deferred because
adding them ripples through `callee_candidates`, `string_literals`,
and `block_dupes`. Reasonable deferral. Two ways to close the gap
without ripples:

- **`symbol_metrics` sibling table** — separate the metric data
  from the symbol catalog. Add closures as metric-only rows keyed
  by `(file, start_line)`. Existing views are untouched; new
  `closure_count` queries become trivial.
- **`kind = 'closure'` with synthetic name** — `closure@line` as
  the name. All three existing views already filter `kind` and
  would naturally exclude them; the only ripple is whether the
  views' implicit "joinable name" assumption holds.

The metric-only table is the lower-risk path.

### Self-test fixture

A small Rust fixture with hand-verified CC numbers would let you
spot regressions like the original v1 bug or the over-count caused
by the `|`-pattern experiment immediately. Suggested test cases:

- empty fn → 1
- single `if` → 2
- single `?` operator → 2
- single closure `|| {}` → 2 in outer (+1 row for inner)
- nested item-level `fn` → 2 in outer (+1 row for inner)
- `matches!(x, A | B)` → 2 in outer
- `match x { A | B => 1, _ => 2 }` → 2 in outer (NOT 3; McCabe original)
- match arm guard `pat if cond => ...` → 3 (1 base + 1 arm + 1 guard)
- `let Some(x) = opt else { return }` → 2 in outer
- chained `a && b && c` → 3 in outer

If you build that fixture, safebash's `lower_one` (currently
indexed at 47, Mozilla 57) becomes a natural acceptance test for
the real Rust idioms.
