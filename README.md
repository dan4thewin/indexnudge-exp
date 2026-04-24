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

## Proposed schema

This is a starting point, not final.  Expect it to change as we
learn what queries the model actually writes and what's missing.

```sql
-- Every source line, full-text searchable
CREATE TABLE lines (
    file    TEXT,
    line    INTEGER,
    content TEXT,
    symbol  TEXT,     -- enclosing function/method/class
    PRIMARY KEY (file, line)
);
CREATE VIRTUAL TABLE lines_fts USING fts5(content, file, symbol);

-- Every symbol definition
CREATE TABLE symbols (
    id         INTEGER PRIMARY KEY,
    name       TEXT,
    kind       TEXT,    -- function, class, struct, method, variable, const
    file       TEXT,
    start_line INTEGER,
    end_line   INTEGER,
    parent     TEXT,    -- enclosing class/impl/module
    signature  TEXT     -- full signature text
);

-- Caller → callee edges
CREATE TABLE calls (
    caller_file   TEXT,
    caller_symbol TEXT,
    callee        TEXT,
    line          INTEGER
);

-- Variable definitions and modifications
CREATE TABLE defs (
    symbol TEXT,    -- enclosing function
    file   TEXT,
    line   INTEGER,
    var    TEXT,
    kind   TEXT     -- assign, define, parameter, return, mutate
);

-- Imports and dependencies
CREATE TABLE imports (
    file   TEXT,
    module TEXT,
    line   INTEGER
);

CREATE INDEX idx_symbols_name ON symbols(name);
CREATE INDEX idx_calls_callee ON calls(callee);
CREATE INDEX idx_calls_caller ON calls(caller_symbol);
CREATE INDEX idx_defs_var ON defs(var);
```

Open questions:

- Is `lines` with FTS5 the right granularity, or should full-text
  search be at the symbol/function level instead?
- Should the `defs` table track reads as well as writes?
- Do we need a `types` table for typed languages?
- Should cross-file relationships (e.g. trait implementations)
  get their own table?
- What about scope — should `defs` track scope depth?

## Example queries

```sql
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

- **Language:** Go.  All new tooling in this ecosystem is Go.
- **Parser:** Tree-sitter via Go bindings.
- **Output:** `.codeindex.db` at repo root (gitignored).
- **Incremental updates:** Track file mtimes, re-index only changed
  files.  Eventually a filesystem watcher for automatic updates.
- **Multi-language:** Tree-sitter grammars for Rust, Go, Python, C,
  JavaScript/TypeScript.  Start with Rust (our primary codebase).

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

Just started.  README and ideas only.
