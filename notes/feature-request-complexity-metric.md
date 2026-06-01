# Feature Request: Cyclomatic complexity column on `symbols`

**Status: resolved (2026-05-31).** Implemented as a nullable
`cyclomatic INTEGER` column on `symbols`, populated for `function`
and `method` kinds in both the Rust (tree-sitter) and Go (`go/ast`)
indexers. Schema version bumped 4 → 5; freshen rebuilds old DBs.
Index `idx_symbols_cyclomatic` added for ORDER-BY hotspot queries.

See `AGENTS.md.gpp` "Complexity hotspots" section for query patterns.
On iwe: 1722 functions in CC 1–5, 49 in 6–10, 49 in 11–20, 10 with
CC ≥ 21 (hottest is `index_node` at 33).

Original request below preserved for context.

## Problem

Cyclomatic complexity is one of the most useful per-function signals
for "where do I refactor next?" and "did this change spike complexity?"
Today the index has every ingredient to compute it (tree-sitter AST,
function spans, blocks, match arms), but no way to query it. Every
project that wants CC has to bolt on an external runner:

```bash
# contrib/cyclo.sh in safebash — spawns its own tree-sitter pass
bash contrib/cyclo.sh sash/src --top 30
```

That runs in ~5 seconds on a mid-sized Rust crate. Doing the same
work as a SQL query against an already-indexed DB would be ~5 ms,
and would compose with everything else in the schema:

```sql
-- top-CC functions in one module
SELECT name, cyclomatic, end_line - start_line + 1 AS lines
FROM symbols
WHERE file LIKE 'sash/src/codegen/%' AND kind = 'function'
ORDER BY cyclomatic DESC LIMIT 20;

-- functions that grew in CC vs a fixed snapshot
SELECT s.name, s.cyclomatic, b.cyclomatic AS baseline
FROM symbols s JOIN baseline_symbols b USING (qualified_name)
WHERE s.cyclomatic > b.cyclomatic + 5;

-- whitelist coverage: which whitelisted fns are now well below threshold?
SELECT a.symbol, s.cyclomatic
FROM cyclo_allow a JOIN symbols s ON a.symbol = s.qualified_name
WHERE s.cyclomatic < 20;
```

## How this bit a real agent

During a safebash refactor I needed three views of CC:

1. "Top 30 in the whole crate" — used `cyclo.sh --top 30`.
2. "Top 10 in just `codegen/`" — `cyclo.sh` has no module filter, so
   I had to grep its output.
3. "Did this commit increase any function's CC by more than 5?" — no
   tooling at all; I ran `cyclo.sh` before and after and diffed by
   hand.

All three would have been one-liners against the DB. Worse, the
external runner re-parses every file on every invocation, so a tight
loop of "edit, re-measure, edit" was the slowest part of the session.

## Proposed schema

A single nullable column on `symbols`:

```sql
ALTER TABLE symbols ADD COLUMN cyclomatic INTEGER;
CREATE INDEX idx_symbols_cyclomatic ON symbols(cyclomatic);
```

Populated only for `kind IN ('function', 'method')`. NULL elsewhere.

If a metric pluralizes later (cognitive complexity, nesting depth,
LOC, parameter count), promote to a sibling table:

```sql
CREATE TABLE symbol_metrics (
    symbol_id   INTEGER PRIMARY KEY REFERENCES symbols(id),
    cyclomatic  INTEGER,
    cognitive   INTEGER,
    max_nesting INTEGER,
    sloc        INTEGER
);
```

I'd start with the single column and migrate if other metrics arrive.

## Counting rules

McCabe-style: 1 + count of branching nodes. For tree-sitter-rust:

- `if_expression`, `else_clause`
- `match_arm` (each arm > 1)
- `loop_expression`, `while_expression`, `for_expression`
- `binary_expression` with `&&` or `||`
- `?` operator (try_expression)
- `if_let_expression`, `while_let_expression`

`contrib/cyclo.sh` in safebash uses exactly this set and lines up
with `complexity` (https://github.com/rrrene/complexity) within ±1
on every function we spot-checked. Happy to share the node list.

## Performance

For Rust the per-function cost is "walk a subtree you already have."
Indexnudge already visits every function body to populate `blocks`
and `match_arms`, so adding a CC counter inside that walk is free.
The integer column adds ~8 bytes per row × 1.3k symbols on the
safebash crate = ~10 KB.

## Why this belongs in indexnudge

Three reasons:

1. **No duplicate parse.** External CC tools re-parse the whole tree
   to count branches that indexnudge already walked.
2. **Compositional queries.** Joining CC with `match_arms`,
   `blocks`, or a project-local `cyclo_allow` table is impossible
   from an external runner.
3. **Stable across languages.** The Rust grammar set above has near-
   identical analogues in the bash/python/go grammars. Counting
   rules can ship in `bin-rust`, `bin-go`, `bin-py` consistently.

## Scope

Code change inside the existing function-visitor. New column,
nullable, computed at index time. No breaking change — every
existing query keeps working.
