# Experiment Report: Block Spans, Match Arms, and References

## Summary

Implemented features #1–#3 from the block-spans feature request on an
experiment branch (`experiment/block-spans`) of indexnudge. The Rust
indexer was rewritten from `syn` to `tree-sitter`, adding three new
tables to the SQLite database:

- **`blocks`** — every multi-line `{}` pair with open/close lines
- **`match_arms`** — each arm's pattern, full span, and body span
- **`refs`** — every identifier reference (use, def, type, call, field)

All existing tables (symbols, calls, imports, defs, lines, lines_fts)
are preserved with the same schema.

## Validation

Validated against `indexnudge/rust/src/main.rs` (1463 lines):

| Table | Row count | Notes |
|-------|-----------|-------|
| blocks | ~50 | Every multi-line `{}` — fn, impl, if, else, match, for, loop |
| match_arms | 10 | All match arms with pattern text and body boundaries |
| refs | 944 | Every identifier/type/field reference |
| symbols | ~40 | Unchanged from before |

9/9 tests pass including:
- Block span queries (open→close line lookup)
- Match arm extraction with pattern text
- Free variable analysis via SQL
- Freshen migration (adds new tables to old DBs)

## Schema

```sql
CREATE TABLE blocks (
    file          TEXT,
    open_line     INTEGER,
    close_line    INTEGER,
    kind          TEXT,    -- 'fn','impl','trait','if','else','match','for','while','loop','closure','unsafe','block'
    parent_symbol TEXT
);

CREATE TABLE match_arms (
    file        TEXT,
    parent_fn   TEXT,
    pattern     TEXT,
    start_line  INTEGER,
    end_line    INTEGER,
    body_start  INTEGER,
    body_end    INTEGER
);

CREATE TABLE refs (
    file     TEXT,
    line_num INTEGER,
    name     TEXT,
    kind     TEXT    -- 'use','def','type','field','call'
);
```

## Performance

tree-sitter parses the 1463-line main.rs in <5ms. Full index of the
file including all table insertions takes ~15ms. The `refs` table is
the largest addition (~944 rows for one file, roughly 0.6 rows/line).
For a 6000-line file expect ~4000 refs rows.

DB size impact: the refs table approximately doubles the database size.
For the iwe project (142 files, ~15k lines total) expect ~10k refs
rows and a DB around 8–10MB.

## Pushback on Feature #4 (Expression Spans)

**Recommendation: do not implement expression spans.**

Rationale:

1. **Block spans already solve the extraction problem.** The feature
   request's motivating failures were about brace-balanced extraction
   — getting the end of a match arm wrong, capturing one `}` too many.
   Block spans (#1) and match arms (#2) directly solve this. Expression
   spans would tell you "this function call spans lines 812–822" but
   that's rarely the boundary you need for extraction.

2. **The storage cost is disproportionate to value.** A 6000-line file
   has perhaps 50–200 multi-line expressions but thousands of total
   expression nodes. Filtering to only multi-line ones helps, but the
   table would still be dominated by trivial cases (a function call
   with 3 args split across lines) that don't cause extraction failures.

3. **Free variable analysis (#3) covers the remaining gap.** When the
   request says "I need to know where this expression ends," what it
   really needs is "I need to know what I'm extracting and what it
   depends on." The `refs` table + free-var query answers that directly.

4. **Query-time tree-sitter is a better fit for this.** If expression
   spans are ever needed for a specific line, a subcommand like
   `indexnudge span <file> <line>` that parses once and walks up the
   AST to find the enclosing expression would be instant (~5ms) and
   not require precomputation. This avoids polluting the DB with
   thousands of rows that are almost never queried.

If extraction failures persist after adopting blocks + match_arms +
free-vars, revisit expression spans as a query-time tool rather than
a precomputed table.

## Branch Location

```
repo:   ~/src/indexnudge
branch: experiment/block-spans
commit: ae86ee0
```

To try it:
```bash
cd ~/src/indexnudge && git checkout experiment/block-spans
cd rust && cargo build --release
./target/release/indexnudge ~/src/safebash
```

## Setup: Freshen and the sqlite3 Shim

The database can go stale when source files change. The **sqlite3 shim**
solves this transparently: when an agent (or human) runs
`sqlite3 .indexnudge-rust.db "..."`, the shim intercepts the call,
runs `indexnudge --freshen` to re-index any modified files, then
executes the real sqlite3.

This means the DB is always up-to-date at query time, with no manual
step required.

### Installation

Add `indexnudge/bin-rust/` to `PATH` **before** the system sqlite3:

```bash
export PATH="$HOME/src/indexnudge/bin-rust:$PATH"
```

This directory contains:
- `indexnudge` — the indexer binary (tree-sitter, with blocks/refs)
- `sqlite3` — shim that freshens before querying
- `cat`, `grep`, `head`, `sed`, `tail` — shims that serve DB content
  (optional; the AGENTS.md instructions direct agents to sqlite3 directly)

### How freshen works

1. The shim detects `*.indexnudge*.db` in the sqlite3 arguments
2. It resolves `indexnudge` on PATH (saved before PATH filtering)
3. Calls `indexnudge --freshen <project-dir>`
4. Freshen compares file mtimes against `files.indexed_mtime` in the DB
5. Only changed/new files are re-parsed and re-inserted
6. If the DB predates this branch, `ensure_new_tables()` adds `blocks`,
   `match_arms`, and `refs` tables before re-indexing

Typical freshen latency: <50ms for 0 changes, ~100ms per changed file.

### First index

```bash
cd ~/src/safebash
indexnudge .      # creates .indexnudge-rust.db
```

After that, all `sqlite3 .indexnudge-rust.db` queries auto-freshen.

---

## AGENTS.md Section: Code Index Database

The following can be added to `AGENTS.md` to instruct agents on using
the new tables.

---

## Code Index Database

This project has a `.indexnudge-rust.db` SQLite database containing
structural information about the Rust source code. Use it for all
code navigation.

### Quick reference

```bash
# Project overview
sqlite3 .indexnudge-rust.db "SELECT file, lines FROM files ORDER BY file;"

# Symbols in a file
sqlite3 .indexnudge-rust.db "SELECT name, kind, start_line, end_line FROM symbols WHERE file='src/codegen/dispatch.rs' ORDER BY start_line;"

# Full-text search
sqlite3 .indexnudge-rust.db "SELECT file, line_num, content FROM lines_fts WHERE content MATCH '\"emit_redirects\"';"

# Read specific lines
sqlite3 .indexnudge-rust.db "SELECT line_num, content FROM lines WHERE file='src/codegen/dispatch.rs' AND line_num BETWEEN 276 AND 320;"
```

### Block spans — brace-balanced extraction

```bash
# What closes the { that opens at line 282?
sqlite3 .indexnudge-rust.db "SELECT close_line FROM blocks WHERE file='src/codegen/dispatch.rs' AND open_line=282;"

# All blocks inside a function
sqlite3 .indexnudge-rust.db "SELECT open_line, close_line, kind FROM blocks WHERE file='src/codegen/dispatch.rs' AND parent_symbol='emit' ORDER BY open_line;"

# Find the enclosing block for a given line
sqlite3 .indexnudge-rust.db "SELECT open_line, close_line, kind FROM blocks WHERE file='src/codegen/dispatch.rs' AND open_line <= 300 AND close_line >= 300 ORDER BY (close_line - open_line) LIMIT 1;"
```

### Match arm boundaries

```bash
# All arms in a match inside function 'emit'
sqlite3 .indexnudge-rust.db "SELECT pattern, start_line, end_line, body_start, body_end FROM match_arms WHERE file='src/codegen/dispatch.rs' AND parent_fn='emit' ORDER BY start_line;"

# Extract a specific arm by pattern
sqlite3 .indexnudge-rust.db "SELECT body_start, body_end FROM match_arms WHERE file='src/codegen/dispatch.rs' AND pattern LIKE '%Command%exec%';"
```

### Free variable analysis — what does a range depend on?

```bash
# Identifiers used in lines 282–416 that are NOT defined within that range
sqlite3 .indexnudge-rust.db "
SELECT DISTINCT r.name, r.kind FROM refs r
WHERE r.file='src/codegen/dispatch.rs'
  AND r.line_num BETWEEN 282 AND 416
  AND r.kind != 'def'
  AND r.name NOT IN (
    SELECT name FROM refs
    WHERE file='src/codegen/dispatch.rs'
      AND line_num BETWEEN 282 AND 416
      AND kind='def'
  )
ORDER BY r.name;"
```

This tells you exactly what parameters an extracted function would need.

### Call graph

```bash
# What does function 'emit' call?
sqlite3 .indexnudge-rust.db "SELECT DISTINCT callee, kind FROM calls WHERE caller_symbol='emit' ORDER BY callee;"

# Who calls 'emit_redirects'?
sqlite3 .indexnudge-rust.db "SELECT caller_symbol, line_num FROM calls WHERE callee='emit_redirects';"
```

### Guidelines

- Use `sqlite3 .indexnudge-rust.db` for ALL code navigation
- Do NOT use grep/cat/head/tail to read source files — the database is faster and gives structural context
- Use `ed` for file mutations — pair line-number addresses with DB line numbers
- The database auto-freshens when queried via the sqlite3 shim
