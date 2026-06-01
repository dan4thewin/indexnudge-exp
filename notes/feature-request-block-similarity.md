# Feature Request: Block-similarity / duplication candidate view

**Status: resolved as a view (2026-05-31).** Implemented as
`block_dupes` — a SQLite view using `ROW_NUMBER()` window functions
and 4-way self-join over a sequenced `lines` table. No storage, no
per-language code, ~170 ms cold scan on a 30K-line codebase.
Schema version bumped 6 → 7; freshen rebuilds old DBs.

The view surfaces every 4-line block that appears 2+ times across the
codebase. On safebash today (the FR's motivating codebase), the top
substantive duplications are:

| occurrences | preview |
|---|---|
| 23 | `builder: &mut FunctionBuilder, module: &mut ObjectModule, ctx: &mut InstCtx,` (handler fn signature) |
| 21 | `module: &mut ObjectModule, ctx: &mut InstCtx, ) -> bool { let vars = &mut *ctx.vars;` |
| 16 | `) -> bool { let vars = ... let literals = ... let externs = ...` (handler setup boilerplate) |
| 9  | `let out_slot = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot,...)` |
| 8  | `patches.push(Patch { start: node.start_byte(), end: node.end_byte(), replacement, ...` |
| 7  | `let stderr = String::from_utf8_lossy(&out.stderr); assert!(out.status.success(), "compile fail...` |

The 23/21/16 cluster is exactly the kind of refactor candidate the FR
called out: 23 handler functions with identical signature and setup
boilerplate. An `InstCtx` wrapper or a macro is the natural next step.

The FR's specific `emit_int_to_str` example has already been refactored
in current safebash (single `itoa` callsite at types.rs:75) — but if it
hadn't, the 19-callsite pattern would have shown up in `block_dupes`
just like the 23-callsite handler pattern does.

## Why a view instead of a table

Same trade-off as `callee_candidates` and `string_literals`. Two big
wins, two acceptable limitations:

✅ Wins:
- Zero storage; recomputed on demand at ~170 ms.
- Zero per-language indexer code — works for Rust, Go, and anything
  else where `lines` is populated.

⚠️ Limitations (documented in AGENTS.md snippet):
- **k=4 fixed.** Different window sizes need an agent-written CTE.
  4 is the FR's recommended starting point and catches the dominant
  case (the function signatures above are exactly 4 lines, the setup
  boilerplate is exactly 4 lines).
- **No identifier-renaming normalization.** `let ptr = ...` and `let
  dest = ...` hash differently. Testing on safebash and iwe showed
  the dominant duplicates are already literally identical (signatures,
  test scaffolding, parameter unpacking) so this matters less in
  practice than the FR theorized.
- **No cluster extension / subsumption.** Overlapping shingles within
  one larger block all appear separately. Agents merge with their own
  filter; usually inspecting top-N is enough.

Normalization is `trim(lower(content))` — collapses leading-whitespace
diffs and case differences but nothing more. Blank lines are skipped
via `ROW_NUMBER()` over a `WHERE trim(content) != ''` subquery so the
4-shingle remains contiguous in non-blank-line space.

Original request below preserved for context.

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
