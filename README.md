# indexnudge

A persistent code index for AI agents, stored as SQLite.

## The problem

AI coding agents navigate source code the way a programmer might —
`grep` for a pattern, `sed -n '80,160p'` to read around the hits,
`cat` to see the whole file.  This works, but it's wasteful.  The
agent burns context window and tool calls reconstructing structural
understanding that a database could answer in one query.

Models are fluent in SQL.  They write joins and subqueries as
naturally as they write grep commands.  If the codebase were a
database, one `SELECT` would replace ten greps.

indexnudge makes the codebase a database.

## What it does

1. **Index** — parse source files and store everything in SQLite:
   every line of source (full-text searchable), every symbol
   definition, every call site, every variable binding, every import.
2. **Nudge** — shell shims intercept `sed`, `grep`, `head`, `tail`,
   `cat` and serve results from the database instead, showing the
   SQL query that produced them.  The agent sees by example how to
   query the index directly.
3. **Scold** — one line of negative reinforcement in the shim output
   ("STOP USING sed") changes behavior within a single occurrence.
   The agent converges on sqlite3-only access for the rest of the
   session.

## Languages

| Language | Indexer | Parser | Language-specific tables |
|----------|---------|--------|------------------------|
| Rust | `rust/` | `syn` (native AST) | — |
| Go | `go/` | `go/ast` + `go/types` | `channels` (make, send, recv, close, range, select) |

Both indexers produce the same shared schema — the shims and the
goose tree patch work identically regardless of which indexer built
the database.

## Schema

The shared schema lives in `schema/schema.sql.gpp` and is
preprocessed with `gpp` for language-specific extensions.

### Shared tables (all languages)

| Table | Contents |
|-------|----------|
| `lines` | Every source line, tagged with enclosing symbol |
| `lines_fts` | FTS5 full-text search over source |
| `symbols` | Functions, methods, structs, traits, interfaces — name, kind, file, line range, signature |
| `calls` | Caller → callee edges |
| `defs` | Variable/binding definitions |
| `imports` | Import/use statements |
| `files` | Per-file line counts, language, indexed mtime |
| `directories` | Aggregated file/line counts per directory |

### Go-specific

| Table | Contents |
|-------|----------|
| `channels` | Channel lifecycle: where created, written to, read from, closed, selected on. Uses `go/types` for accuracy — `range` over a slice won't appear here. |

## How the nudge works

Every agent session starts with `tree`.  The goose tree tool is
patched (via `patches/goose-tree-indexnudge.patch`) to detect
`.indexnudge.db` and serve structured tables from the database
instead of a filesystem listing.  The output ends with:

```
Every line of source is in `lines`. Every function call is in `calls`.
For deeper exploration: sqlite3 .indexnudge.db .schema
```

The agent follows the breadcrumb.  On its next tool call it runs
`sqlite3` — not `grep`.

When habit reasserts itself and the agent reaches for `sed -n`, the
shim intercepts, serves the data from the database, shows the SQL
query that produced it, and adds:

```
** STOP USING sed. Query the DB directly: sqlite3 .indexnudge.db
   "SELECT content FROM lines WHERE ..." **
```

One contextual scolding at the moment of the wrong choice is more
effective than system prompt instructions, enriched output, and
example queries combined.

## Experimental results

### Brush (Rust, 148 files, 32K lines)

Goose oneshot with a code exploration prompt.  Shims on PATH, patched
tree, negative reinforcement in sed shim.

| Tool | Calls |
|------|-------|
| tree | 1 |
| sqlite3 (direct) | 7 |
| sed (shimmed) | 1 |
| head | 2 |
| grep | 2 |
| cat | 0 |

7 of 12 calls went straight to the database.  The single sed call
was the agent's first code-read attempt — after the scolding, every
subsequent read was a `SELECT content FROM lines`.

### TibbitWiki (Go, 14 files, 3.7K lines)

Same setup with the Go indexer and Go-specific `channels` table.

| Tool | Calls |
|------|-------|
| tree | 1 |
| sqlite3 (direct) | 2 |
| cat | 4 |
| sed / grep / head | 0 |

The agent queried the `channels` table immediately — tracing
goroutine architecture (wiki.loop, watcher.eventLoop, hub) without
prompting.  It used `cat` for four small files (57–577 lines), which
is reasonable for files that size.  Zero sed, zero grep, zero head.

## Freshness

The agent edits a file, then reads it back through a shim.  If the
database is stale, it doesn't see its own change.

`indexnudge --freshen <dir>` compares each file's mtime to the
`indexed_mtime` stored in the `files` table.  Only changed, new, or
deleted files are reprocessed.  The sqlite3 shim calls `--freshen`
before every query.  Since every read path terminates at sqlite3 —
directly or through the other shims — freshness is guaranteed at the
point of consumption.

No daemons, no filesystem watchers, no hooks on `edit` or `write`.

## Project structure

```
indexnudge/
├── rust/                # Rust indexer (syn-based)
│   ├── src/main.rs
│   └── Cargo.toml
├── go/                  # Go indexer (go/ast + go/types)
│   ├── main.go
│   └── go.mod
├── schema/
│   └── schema.sql.gpp   # shared schema, gpp-preprocessed
├── bin/                  # shims (shared, language-agnostic)
│   ├── _indexnudge_common.bash
│   ├── sed, grep, head, tail, cat, sqlite3
│   └── indexnudge
├── bin-go/               # Go shim set (symlinks + Go binary)
├── patches/
│   └── goose-tree-indexnudge.patch
├── goose/                # goose submodule (pinned, patched)
└── test_brush.sh, test_tibbiwiki.sh
```

## Setup

```bash
# Build the indexer (pick one)
cd rust && cargo build --release
cd go && go build -o indexnudge-go .

# Index a project
indexnudge /path/to/project          # Rust projects
indexnudge-go -dir /path/to/project  # Go projects

# Put shims on PATH
export PATH=/path/to/indexnudge/bin:$PATH       # Rust
export PATH=/path/to/indexnudge/bin-go:$PATH     # Go
```

## Background

This project grew out of five controlled experiments with goose
trying to improve tool selection in delegated coding tasks.  The
full account is in the writeup.  The short version:

1. System prompt instructions ("prefer analyze over grep") don't
   change behavior.  Soft or hard, the model follows trained habits.
2. Blocking tools forces adaptation but models route around —
   block sed, they discover awk.
3. Enriching tool output helps incrementally but doesn't change
   the default choice.
4. Contextual negative feedback at the moment of the wrong choice
   changes behavior in one occurrence.

The hierarchy: **in-band correction > tool output enrichment >
system prompt instruction > adding new tools**.
