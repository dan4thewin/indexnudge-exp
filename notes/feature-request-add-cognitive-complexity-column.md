# Add `cognitive` column to `symbols`

McCabe cyclomatic complexity has a well-known weakness: it penalises
breadth (wide `match` statements) the same way it penalises depth
(nested `if`/`while`/`for`). A 20-arm dispatcher on an enum scores
CC 20, identical to a function with four levels of nested branching —
even though one is trivially auditable and the other is a refactor
target.

Downstream of indexnudge in safebash, this has produced a culture of
`// cyclo-allow:` comments above wide-but-shallow dispatchers, and
pre-commit hook logic that excludes them from the top-N report. The
allow list is curated by hand, decays without enforcement, and was
silently shadowed by intervening doc comments / `#[allow]` attrs for
months before the issue was caught (safebash `.githooks/pre-commit`,
fix 2026-06-05). It's a workaround for a metric mismatch, not the
right shape.

**Cognitive Complexity** (Campbell, SonarSource 2018) is the
industry's standard answer: explicitly designed to distinguish
"wide dispatcher" from "deeply nested logic." Surfacing it as a
second column would let downstream tools (safebash's pre-commit
report, anyone else's) pick the metric appropriate to their
question without needing per-function annotations.

## What to compute

For each function/method already getting a `cyclomatic` value,
compute and store its Cognitive Complexity per the [Sonar
spec](https://www.sonarsource.com/docs/CognitiveComplexity.pdf).
Rust-relevant rules in summary:

| construct | cyclomatic | cognitive |
|---|---|---|
| `if`, `for`, `while`, `loop` at top of a function | +1 | +1 |
| same constructs nested inside another | +1 | **+1 + nesting_level** |
| each `else if` in a chain | +1 | +1 (no nesting bonus) |
| `else` clause | 0 | +1 |
| `match` statement | +N (one per arm) | **+1 + nesting_level** (the match itself, regardless of arm count) |
| each arm inside a `match` | counted | 0 (only nested content adds) |
| sequence of `&&` or `||` operators | +1 each | +1 for the operator group, +1 each time the operator kind switches |
| `?` operator | +1 | 0 |
| early `return` | +1 | 0 |
| `break` / `continue` to a label | 0 | +1 |
| closures, nested functions | counts inside | counts inside; nesting level applies to their body |

The key behavioural test: a 20-arm `match` whose arms are all
single-call leaves should score `cognitive = 1`, even though
`cyclomatic` would be 20. A 3-level nested `if` doing real branching
should score around `cognitive = 6` (+1, +2, +3).

## Reference implementations

Three sources to cross-check against, listed by preference:

1. **Mozilla `rust-code-analysis`** —
   [`src/metrics/cognitive.rs`](https://github.com/mozilla/rust-code-analysis/blob/main/src/metrics/cognitive.rs)
   computes cognitive for Rust over the same tree-sitter-rust
   grammar indexnudge uses. Counting model is the natural sibling
   of the cyclomatic implementation indexnudge already calibrates
   against. **Strongly preferred** — keeps the same calibration
   relationship (`cognitive.sum` matches us within tree-sitter
   grammar quirks, same way `cyclomatic.sum` does today). Same
   spot-check protocol can validate.

2. **rust-clippy** —
   [`clippy_lints/src/cognitive_complexity.rs`](https://github.com/rust-lang/rust-clippy/blob/master/clippy_lints/src/cognitive_complexity.rs)
   computes a simplified variant (just counts CC-style increments
   minus a returns-discount). Not the canonical Sonar algorithm.
   Worth reading but not the calibration target.

3. **SonarSource Rust analyzer** — closed-source but the spec PDF
   above is authoritative; the implementation in `sonar-rust` (no
   public source) is what the spec maps to.

Recommend matching Mozilla `cognitive.sum`, same way the current
`cyclomatic` column matches `cyclomatic.sum`. Document the
calibration relationship in the same place
`bug-cyclomatic-undercounts-vs-mozilla.md` documents the existing
one.

## Schema change

```sql
ALTER TABLE symbols ADD COLUMN cognitive INTEGER;
CREATE INDEX idx_symbols_cognitive ON symbols(cognitive);
```

`cognitive` is NULL when `cyclomatic` is NULL (same set of rows:
functions and methods only).

Schema version bump triggers auto-rebuild on next `--freshen` —
same path the cyclomatic fix used (v8 → v9).

## Downstream wins

Two known consumers benefit immediately:

1. **safebash `.githooks/pre-commit`** — the top-5 hotspot report
   can use `MAX(cyclomatic, cognitive)` as the headline, OR show
   both columns. The cyclo-allow culture (and the recursive-CTE
   filter that walks doc/attr blocks looking for allow comments)
   becomes unnecessary; cognitive already discounts the pure
   dispatchers that drove that workaround.

2. **AGENTS.md complexity-hotspots query** (built-in snippet
   in indexnudge's AGENTS.md.rust.snippet) — gains a second
   column. The "wide-but-shallow dispatcher" false positives
   that currently appear in top-N stop appearing without manual
   intervention.

For curious downstreams: divergence between `cyclomatic` and
`cognitive` is itself a useful signal. High CC + low Cognitive =
dispatcher (safe). Low CC + high Cognitive = suspicious (rare —
might indicate something is off in either the code or the
algorithm). High both = the real refactor targets.

## Validation

Same protocol as the existing cyclomatic spot-check:
`rust-code-analysis-cli -m -p src -O json`, read each function
space's `metrics.cognitive.sum`, compare to indexnudge's
`SELECT name, cognitive FROM symbols WHERE file=...`. Aim for
exact match on the unambiguous cases; document any divergence
due to tree-sitter grammar version skew, matching the existing
cyclomatic discrepancy disclosure in safebash's AGENTS.md
indexnudge block.

Add 4–6 unit tests in `rust/src/main.rs::cognitive_tests` (mirror
`cyclomatic_tests` structure) covering:

  - 20-arm match on enum with single-call arms → cognitive 1
  - 4-level nested if doing real branching → cognitive 10 (+1+2+3+4)
  - Long `else if` ladder, 6 cases → cognitive 6 (no nesting bonus)
  - Top-level if inside a closure inside a function → cognitive
    accounts for closure as +1 nesting level on the if
  - Match with one nested match arm → outer cognitive 1, plus
    +2 from the nested match (+1 base, +1 nesting)
  - Function with `?`s and early returns → cognitive 0 for those,
    while cyclomatic would have charged each one

## Out of scope

- Replacing `cyclomatic` with `cognitive`. Both metrics answer
  different questions; both belong in the schema.
- Surfacing additional metrics (Halstead, NPATH, essential CC).
  If a need surfaces later, file separately.
- Changing the `cyclo-allow` comment convention in safebash. That's
  a downstream decision once they see whether cognitive eliminates
  the need.
