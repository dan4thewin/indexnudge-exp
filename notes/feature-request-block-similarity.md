# Feature Request: Block-similarity / duplication candidate view

## Problem

When you're refactoring "extract this repeated pattern into a
helper", the hardest part isn't doing the extraction — it's finding
every site that repeats the pattern. Today the index can tell you
*structural* facts (functions, blocks, match arms, calls) and *line*
facts (FTS over content), but nothing about *similar code*.

The result is a familiar agent failure mode: extract a helper for
the four most obvious callsites, declare success, and miss the other
fifteen.

## How this bit a real agent

During Phase 8 of a safebash refactor, the brief was "extract an
`emit_int_to_str(builder, module, externs, int_val) -> (ptr, len)`
helper from codegen." The audit estimated **4 callsites**. The
actual count was **19**. Every site was a verbatim run of:

```rust
let ptr = builder.ins().stack_addr(I64, slot, 0);
let len_val = builder.ins().call(externs.itoa, &[ptr, int_val]);
let len = builder.inst_results(len_val)[0];
```

…sometimes with a stack-slot setup line above and a `len` shadowed
to a different name. Five-times as much repetition as the audit
saw. Finding all 19 required iteratively grepping for `itoa`,
`stack_addr`, `inst_results`, and cross-referencing by hand. A
proper duplication view would have surfaced the full set in one
query.

## Proposed view

A precomputed table of "near-duplicate block runs," keyed by a
normalized hash of consecutive non-trivial lines:

```sql
CREATE TABLE block_dupes (
    cluster_id    INTEGER,        -- one id per equivalence class
    file          TEXT,
    start_line    INTEGER,
    end_line      INTEGER,
    line_count    INTEGER,        -- size of the matched run
    enclosing_fn  TEXT
);
CREATE INDEX idx_block_dupes_cluster ON block_dupes(cluster_id);
CREATE INDEX idx_block_dupes_file ON block_dupes(file);

-- "candidates worth a helper" — runs of 4+ lines duplicated 3+ times
SELECT cluster_id, COUNT(*) AS occurrences, MAX(line_count) AS sz
FROM block_dupes
GROUP BY cluster_id
HAVING occurrences >= 3 AND sz >= 4
ORDER BY occurrences * sz DESC;

-- "show me all sites of a specific cluster"
SELECT file, start_line, end_line, enclosing_fn
FROM block_dupes WHERE cluster_id = 47;
```

## Normalization

The hash should ignore noise that doesn't affect "is this the same
pattern":

- Whitespace differences.
- Local variable names (rename to `v1`, `v2`, … in order of
  appearance).
- Numeric and string literals (replace with `<n>` / `<s>`).
- Comments.

Keep verbatim:

- Keywords, operators, punctuation.
- Method names, field accesses, type names. (These are the actual
  "shape" of a duplication.)

This catches `let ptr = builder.ins().stack_addr(...)` even when
the next site says `let dest = builder.ins().stack_addr(...)`.

## Algorithm sketch

Standard rolling-hash over consecutive lines per file, with
extension to maximal runs:

1. For each line in `lines`, compute the normalized hash.
2. For each (file, line_num), produce a "k-line shingle" hash by
   combining the next k normalized hashes (start at k=4).
3. Group shingles by hash. Any group with ≥ 2 entries is a cluster.
4. Greedily extend each cluster to the longest matching run that
   stays in agreement across all participants.
5. Drop clusters that are fully contained within larger ones.

Cost is O(N) hashes + O(C log C) grouping where N = total lines,
C = number of unique shingles. Cheap.

## Why this belongs in indexnudge

It's the missing third leg of "what shape is this code?":

| question                              | today        | with this |
|---------------------------------------|--------------|-----------|
| Where is symbol `foo` defined?        | `symbols`    | unchanged |
| Where is `foo` called?                | `calls`      | unchanged |
| Where is the same pattern repeated?   | grep + luck  | `block_dupes` |

Without it, agents systematically under-count refactor targets and
ship "extract the helper" PRs that miss the majority of callsites.

## Out of scope

- Cross-file *function-level* similarity (jPlag/MOSS territory).
  This proposal is just about run-of-lines, which catches the
  vast majority of "extract a helper" candidates.
- Semantic similarity across different ASTs. Hash on normalized
  tokens, not on AST equivalence.

## Scope

Code change inside the indexer to run the shingle pass after `lines`
is populated. New table + indexes. No breaking change.
