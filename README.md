# codeindex

A persistent code index for AI agents, stored as SQLite.

## Problem

AI agents navigating codebases waste 75% of their tool calls on
orientation: grepping for patterns, reading code with sed/cat, and
reconstructing structural understanding from scratch every session.
We ran controlled experiments and found that agents make 30+ tool
calls just to understand what a codebase does before they can start
working.  Instructions telling them to use better tools don't help —
the behavior is deeply embedded in training data.

But models are fluent in SQL.  They write joins and subqueries as
naturally as they write grep commands.  If the codebase were a
database, the agent could answer structural questions in one query
instead of a spiral of grep → sed → grep → sed.

## Idea

Build a tool that indexes a source tree into a SQLite database.
The agent opens the database and queries it directly.  No new tool
API to learn, no documentation to read — `.schema` *is* the
interface.

One `SELECT` replaces ten greps.  Full-text search replaces
`grep -rn`.  Join across symbol and call tables replaces manual
call-graph tracing.  The model's SQL fluency becomes a superpower
instead of lying dormant.

## What it indexes

Tree-sitter parses source files and extracts:

- **Every line of source** — searchable via FTS5
- **Every symbol definition** — functions, methods, classes, structs
- **Caller→callee relationships** — static call graph
- **Variable mutation points** — where variables are defined, assigned, modified
- **Imports/dependencies** — module relationships

## Schema

```sql
-- Project structure: files and directories
CREATE TABLE files (
    file     TEXT PRIMARY KEY,
    lines    INTEGER,
    language TEXT
);

CREATE TABLE directories (
    path        TEXT PRIMARY KEY,
    file_count  INTEGER,
    total_lines INTEGER
);

-- Every source line, full-text searchable
CREATE TABLE lines (
    file    TEXT,
    line    INTEGER,
    content TEXT,
    symbol  TEXT,     -- enclosing function/method/class
    PRIMARY KEY (file, line)
);
CREATE VIRTUAL TABLE lines_fts USING fts5(file, line_num, content, symbol);

-- Every symbol definition
CREATE TABLE symbols (
    id             INTEGER PRIMARY KEY,
    name           TEXT,
    qualified_name TEXT,    -- e.g. "Foo::bar" for methods
    kind           TEXT,    -- function, method, struct, enum, trait, impl, const, static, type, macro
    file           TEXT,
    start_line     INTEGER,
    end_line       INTEGER,
    parent         TEXT,    -- enclosing impl/trait/mod
    signature      TEXT     -- full signature text
);

-- Caller → callee edges
CREATE TABLE calls (
    caller_file   TEXT,
    caller_symbol TEXT,
    callee        TEXT,
    kind          TEXT,    -- function, method, macro
    line          INTEGER
);

-- Variable definitions and modifications
CREATE TABLE defs (
    symbol TEXT,    -- enclosing function
    file   TEXT,
    line   INTEGER,
    var    TEXT,
    kind   TEXT     -- let, let_mut, assign, parameter
);

-- Imports and dependencies
CREATE TABLE imports (
    file   TEXT,
    module TEXT,
    line   INTEGER
);
```

## Example queries

```sql
-- "what's the shape of this project?"
SELECT * FROM directories ORDER BY path;

-- "what are the biggest files?"
SELECT file, lines FROM files ORDER BY lines DESC LIMIT 10;

-- "how does exit status flow through case statements?"
SELECT s.name, s.file, s.start_line, s.end_line
FROM defs d
JOIN symbols s ON d.symbol = s.name AND d.file = s.file
WHERE d.var = '?';

-- "who calls lower_case?"
SELECT caller_symbol, caller_file, line
FROM calls WHERE callee = 'lower_case';

-- "show me the function"
SELECT content FROM lines
WHERE file = 'sash/src/transform.rs'
  AND line BETWEEN 1274 AND 1424;

-- "what functions mention phi?"
SELECT DISTINCT symbol FROM lines_fts
WHERE lines_fts MATCH 'phi';

-- "what does lower_if call?"
SELECT callee, line FROM calls
WHERE caller_symbol = 'lower_if';
```

## Design

This is a **proof of concept**.  The goal is to determine whether a
SQL-queryable code index significantly improves agentic coding.  If
it does, expect a rewrite for multi-language support.

- **Language:** Rust (POC only — best index quality for Rust targets)
- **Parser:** `syn` for AST extraction.  Native Rust parser, no FFI,
  no grammar files.  Gets exact Rust semantics including macro
  invocations (though not macro expansion).
- **Output:** `.codeindex.db` at repo root (gitignored).
- **Target language:** Rust only (POC scope).
- **Incremental updates:** Out of scope for POC.  Full re-index.
- **Agent integration:** Agent runs `sqlite3` directly against the
  database.  No custom tool API — `.schema` is the interface.

## Design decisions

**Symbol naming:** Store both short name (`bar`) and qualified name
(`Foo::bar`) for methods/associated functions.  Short name in `name`,
qualified in a `qualified_name` column.  Agents search either way.

**Call extraction:** Store what `syn` can resolve statically.
`x.process()` → callee `process` (no type qualifier — `syn` can't
resolve it).  Macro invocations stored as calls with `kind: "macro"`.
Don't fake type resolution.

**Source lines:** Store every line in SQLite with FTS5.  Duplicates
the source (~10-20MB for 100k LOC) but the whole point is the agent
shouldn't need to shell out to read code.

**Variable tracking (`defs` table):** Best-effort.  Extract `let`
bindings, assignments, parameter definitions, and `&mut` where
visible.  Experiments showed agents chasing variable assignments
(e.g., exit status flow) — this table supports that pattern.

## Evaluation plan

Index a real Rust codebase.  Run the same set of agentic tasks
(orientation, bug finding, refactoring) with and without the index.
Measure tool calls, wall time, and outcome quality.

Target codebase: goose itself (Rust, ~100k LOC, complex enough to
be realistic).

## Background

This project grew out of experiments with goose (Block's AI agent)
trying to get better tool usage from delegated tasks.  Five
controlled runs showed that:

1. System prompt instructions don't change tool selection behavior
2. Blocking bad tools forces adaptation but models route around
3. Enriching tool output (function bodies, JSON grep) improves
   efficiency
4. Models are fluent in SQL and leverage it immediately

Full writeup: `GooseToolSelectionExperiments` in TibbitWiki.

## Status

POC.  Rust implementation, Rust-only indexing, using `syn`.
