# Teaching an AI Agent to Use a Code Index

## The problem

AI coding agents read source code the way a human might — `cat`, `head`,
`sed -n '80,160p'`, `grep`.  These tools work, but they're wasteful.  The
agent burns context window and turns scrolling through raw source when a
structured index of every symbol, call, import, and line already exists.

The question: **can you make an agent prefer a structured database over
raw file reads, without hardcoding the behavior?**

This is the story of how we got there, iterating through prompt
engineering, tool removal, tool modification, shell shims, and finally
in-band negative reinforcement — and what worked.

## Background: goose and the developer extension

[Goose](https://github.com/block/goose) is Block's open-source AI agent.
Its built-in **developer extension** provides four tools: `write`, `edit`,
`shell`, and `tree`.  The agent explores codebases by calling `tree` for
structure, then `shell` to run `cat`, `sed`, `grep`, `head`, etc. to read
source files.

## Phase 1: the `analyze` tool (added, then removed)

Goose originally shipped an `analyze` tool — a tree-sitter-based code
structure analyzer with modes for directory overview, file detail, and
symbol focus with call graphs.  It was substantial: ~3,000 lines of Rust
across cache, formatter, parser, graph, and per-language modules
supporting Rust, Go, Python, JavaScript, Ruby, Swift, Java, and Kotlin.

The problem wasn't capability — it was adoption.  The agent had to
*choose* `analyze` over the familiar `shell` + `grep`/`sed`/`cat` pattern.
Two tools doing similar things created decision overhead.  In commit
`ced5c1b10` ("simplify developer extension"), `analyze` was removed
entirely, along with the `goose-mcp` crate — a net reduction of ~9,800
lines.

**Lesson: adding a better tool alongside the existing tools doesn't
change behavior.  The agent defaults to what it knows.**

## Phase 2: codeindex — a standalone indexer

Rather than embedding analysis in goose, we built
[codeindex](https://github.com/dan4thewin/codeindex) — a standalone Rust
CLI (~860 lines) that walks a Rust codebase and produces a `.codeindex.db`
SQLite database with these tables:

| Table | Contents |
|-------|----------|
| `lines` | Every source line, tagged with enclosing symbol |
| `lines_fts` | FTS5 full-text search over source lines |
| `symbols` | Functions, structs, enums, traits, impls — name, kind, file, line range, signature |
| `calls` | Function/method call sites, caller → callee |
| `defs` | Variable/binding definitions |
| `imports` | `use` statements |
| `files` | Per-file line counts, language, indexed mtime |
| `directories` | Aggregated file/line counts per directory |

The database is designed for agent consumption — structured, queryable,
and complete.  Every line of source is in there.  The agent never needs
to read a raw file.

## Phase 3: prompt engineering

The developer extension's system prompt was modified to direct the agent
toward the database:

```
Query `.codeindex.db` in the project root with `sqlite3` to understand
the codebase.  Start with `directories` and `files` for orientation,
then `symbols`, `calls`, `imports`, `defs` for structure, and
`lines_fts` for full-text search.  The database contains every line
of source and all structural metadata.  Run `.schema` to see
the full schema.
```

This replaced the upstream instructions which said:

```
...prefer the flow of using tree to understand the codebase structure
and file sizes. When you need to search, prefer rg... Then use cat or
sed to gather the context you need...
```

**Result: partial success.**  The agent used sqlite3 for structural
queries (symbol lookup, call graphs) but continued reaching for `sed` and
`head` to read code.  The prompt instruction was too far from the point
of action — by the time the agent needed to read lines 80–160 of a file,
the prompt was 10,000 tokens in the past.

## Phase 4: modifying `tree` (the entry point)

The key insight: every agent session starts with `tree`.  It's the
orientation tool — the first thing called.  If `tree` itself could
detect `.codeindex.db` and serve structured data instead of a filesystem
listing, the agent would see the database *before* it formed any plans.

The `tree` tool in goose was modified (in DG's fork) to:

1. Walk up from the target directory looking for `.codeindex.db` (same
   convention as `.git` discovery)
2. When found, query `directories` and `files` tables and format the
   output as structured tables
3. Append a nudge: "Every line of source is in `lines`. Every function
   call is in `calls`. For deeper exploration: `sqlite3 <db> .schema`"
4. Fall back to normal filesystem walking when no `.codeindex.db` exists

This was invisible to the agent — it called `tree` as always, but got
enriched output with breadcrumbs toward the database.  The agent
consistently followed the breadcrumbs and went straight to sqlite3 for
structural queries on its next tool call.

**Lesson: modifying the tool the agent already reaches for is more
effective than adding a new tool or putting instructions in the prompt.**

## Phase 5: early shims (blocking)

The `tree` modification got the agent to sqlite3 for structure, but it
still used `sed`, `head`, and `cat` to read code.  Early shim attempts
(found in `/tmp/goose_shims/`) took a hard-blocking approach:

- **sed**: `echo "ERROR: sed is disabled. Use 'analyze'..." && exit 1`
- **cat**: blocked source file extensions (`.rs`, `.py`, `.go`, etc.),
  passed through non-code files
- **grep**: the cleverest — upgraded to `git grep -p` with JSON output
  including function context, plus a nudge

The sed and cat shims were blunt instruments.  The grep shim was more
interesting because it *improved* the output rather than just blocking.

**Problem: the shims referenced `analyze`, which no longer existed.**
They were from an earlier iteration.  And hard blocking just made the
agent try alternate tools.

## Phase 6: enriching shims

The new shims (in `bin/`) follow the `tree` philosophy: **enrich, don't
block**.  Each shim intercepts the read pattern, serves the data from
`.codeindex.db` with additional context, and shows the SQL query that
produced the result.

### sqlite3

The simplest and most important shim.  Since every path leads to
sqlite3 — the other shims redirect there, and once the agent learns,
it goes direct — this shim guarantees freshness:

```bash
for arg; do
    if [[ "$arg" == *.codeindex.db ]]; then
        codeindex --freshen "$dir"
    fi
done
exec sqlite3 "$@"
```

### sed

Intercepts `sed -n 'N,Mp' file` (the line-range read pattern).  Serves
lines from the database with:

- The SQL query that produced the output (teaching by example)
- Line numbers
- Enclosing symbol context (which function am I looking at?)
- Ready-to-run sqlite3 commands to read each enclosing function in full

```
── sqlite3 .codeindex.db ──
query: SELECT line_num, content FROM lines WHERE file='src/main.rs'
       AND line_num BETWEEN 490 AND 500;
** STOP USING sed. Query the DB directly: sqlite3 .codeindex.db
   "SELECT content FROM lines WHERE ..." **

 490 │     pipeline_context: &mut PipelineExecutionContext,
 491 │     mut params: ExecutionParameters,
   ...

── enclosing: execute_in_pipeline | method | lines 488-543 ──
── full method: sqlite3 .codeindex.db "SELECT content FROM lines
   WHERE file='...' AND line_num BETWEEN 488 AND 543 ORDER BY
   line_num;" ──
```

### head, tail

Same pattern — intercept `head -n N file` and `tail -n N file`, serve
from the database.  Pass through for pipes (`command | head -20`)
since those aren't file reads.

### grep

Intercepts `grep pattern file` and routes to FTS5 MATCH for simple
alphanumeric patterns, LIKE fallback for complex patterns.  Every match
includes the enclosing symbol name:

```
src/main.rs:47  │ create_schema(&conn)?;  # in: full_index
src/main.rs:211 │ fn create_schema(...    # in: create_schema
```

### cat

Serves the full file from the database with a symbol summary and a
nudge toward focused exploration via the symbols table.

### Common infrastructure

All shims source `_codeindex_common.bash`, which:

- Removes the shim directory from `$PATH` so `exec sed "$@"` finds the
  real tool (works regardless of where the real tool lives — `/usr/bin`,
  Homebrew, Linuxbrew)
- Provides `find_codeindex_db()` — walks up from a path to find
  `.codeindex.db`
- Provides `freshen_db()` — calls `codeindex --freshen` if available

## Phase 7: freshness

A critical concern: the agent edits a file via `edit` or `write`, then
reads it back through a shim serving stale database content.  It doesn't
see its own change.  This could cause loops or silent validation against
old code.

The solution has two parts:

### Incremental reindexing (`codeindex --freshen`)

`codeindex --freshen <dir>` opens the existing database (doesn't
rebuild), walks the directory, and compares each file's filesystem mtime
to the `indexed_mtime` stored in the `files` table.  Only stale, new, or
deleted files are processed.  A single-file `syn` parse + sqlite insert
is milliseconds.

### Lazy evaluation at the read boundary

The sqlite3 shim calls `codeindex --freshen` before every query against
`.codeindex.db`.  Since every read path terminates at sqlite3 — whether
directly or through the other shims — freshness is guaranteed at the
point of consumption.

No daemons, no filesystem watchers, no hooks on `edit`/`write`/`shell`.
The agent can mutate files through any tool call; the index is refreshed
lazily when something reads from it.

## Phase 8: experimental results

We ran controlled one-shot experiments — invoking goose with a code
exploration prompt against the [brush](https://github.com/reubeno/brush)
shell project (148 files, 32K lines, 1764 symbols) and counting tool
usage.

### Run 1: no shims on PATH

The agent used `tree` (got codeindex-enhanced output), went to sqlite3
for structural queries, but used `sed -n` for all code reading.  The
system prompt instruction to use sqlite3 was effective for structure but
not for reading code.

### Run 2: shims on PATH, no exhortation

Shims intercepted every `sed` and `head` call, serving enriched output
with SQL queries and enclosing symbols.  The agent saw the SQL queries
but **never copied them** — it continued using `sed -n` for every code
read (5 sed calls, 0 `SELECT content FROM lines`).

The shims were transparent infrastructure.  The agent didn't notice or
learn from the enriched output.

### Run 3: shims on PATH, with negative reinforcement

Added a single line to the sed shim output:

```
** STOP USING sed. Query the DB directly: sqlite3 .codeindex.db
   "SELECT content FROM lines WHERE file='...' AND line_num BETWEEN
   START AND END ORDER BY line_num;" **
```

Results:

| Metric | Without exhortation | With exhortation |
|--------|-------------------|-----------------|
| `sed -n` calls | 5 | **1** |
| sqlite3 calls | 6 | **10** |
| `SELECT content` (reading code) | 0 | **3** |
| Other shell tools | grep, head | **none** |

The agent used sed exactly once — its first code-read attempt.  After
seeing the `STOP USING sed` message, every subsequent code read went
through `sqlite3 ... "SELECT content FROM lines ..."` directly.  It
also began querying symbol boundaries *first* and reading exact ranges,
rather than guessing line numbers.

**One contextual scolding was more effective than the system prompt
instruction, the enriched output, and the example queries combined.**

## Why it works

The system prompt tells the agent what to do.  The shim tells it what
it just did wrong.  Both are needed, but they work at different moments:

- **Prompt instruction** (far from action): gets the agent to sqlite3
  for structural queries from the start.  Effective for navigation, not
  for reading.
- **Enriched output** (at point of action): shows SQL queries as
  templates.  Educational but not behavioral — the agent sees the
  queries but doesn't adopt them.
- **Negative reinforcement** (at point of wrong action): fires at the
  exact moment the agent makes the wrong choice, in the output it's
  actively processing.  Changes behavior within one occurrence.

The hierarchy: **in-band correction > tool output enrichment > system
prompt instruction > adding new tools**.

## Architecture summary

```
Agent session starts
        │
        ▼
    ┌─────┐     detects .codeindex.db
    │tree │ ──────────────────────────► structured tables + nudge
    └─────┘                             "use sqlite3 for deeper exploration"
        │
        ▼
  Agent uses sqlite3 for structure
  (symbols, calls, imports)
        │
        ▼
  Agent tries sed/head/grep/cat       ◄── habit
        │
        ▼
    ┌──────┐    intercepts, serves from DB
    │ shim │ ──────────────────────────► enriched output + SQL query
    └──────┘    "STOP USING sed"         + enclosing symbol context
        │
        ▼
    ┌─────────┐  freshens DB first
    │sqlite3  │ ─────────────────────► codeindex --freshen
    │ shim    │                         (mtime-based, per-file)
    └─────────┘
        │
        ▼
  Agent converges on sqlite3-only
```

## What's in the box

```
codeindex/
├── src/main.rs          # Rust indexer (~860 lines)
├── Cargo.toml
├── mkfile               # build, install, test, clean
├── bin/
│   ├── _codeindex_common.bash  # shared helpers
│   ├── codeindex        # symlink → target/release/codeindex
│   ├── sqlite3          # freshen-before-query shim
│   ├── sed              # line-range read → DB query + scolding
│   ├── head             # head -n N file → DB query
│   ├── tail             # tail -n N file → DB query
│   ├── grep             # pattern search → FTS5/LIKE + symbol context
│   └── cat              # full file → DB + symbol summary
└── README.md
```

To use: `export PATH=/path/to/codeindex/bin:$PATH`

## Open questions

- **Does learning persist across sessions?**  No.  The agent has no
  memory.  Each session, it tries sed once, gets scolded, and converges.
  That first sed call is the cost of the pattern.

- **Should the scolding be in all shims?**  Currently only in sed.  The
  other shims (head, tail, grep, cat) enrich but don't scold.  In
  practice, once the agent stops using sed it stops using all of them —
  the scolding in sed is sufficient.

- **What about non-Rust codebases?**  codeindex currently only indexes
  `.rs` files.  The shim architecture is language-agnostic — it just
  needs a populated `.codeindex.db`.  The indexer needs language support
  added (tree-sitter would be the obvious path, which circles back to
  what `analyze` did — but now as an offline index builder, not a
  runtime tool).

- **Should the `tree` tool in goose call `--freshen` too?**  Currently
  it reads the database as-is.  The sqlite3 shim handles freshness for
  direct queries, but the tree tool shells out to sqlite3 directly.
  Wiring it through the shim (or calling freshen itself) would close
  this gap.

## Ideas from the neighborhood

A survey of six other projects named `codeindex` on GitHub revealed a
crowded space but distinct approaches.  Nobody else has our shim
architecture or in-band behavior modification — those are genuinely
novel.  But several projects have ideas worth adopting:

- **MCP server registration** (lexandro/codeindex-mcp): their
  `register project .` command auto-writes `.mcp.json` for agent
  discovery.  We currently require manually prepending `bin/` to PATH.
  A `codeindex init` command that wires up the shims and writes agent
  config would lower the setup barrier.

- **Call chain traversal** (LydiaCai1203/codeindex): configurable
  forward/reverse call graph depth.  Our `calls` table has the raw data
  but we don't expose traversal queries.  A `codeindex calls --depth=3
  function_name` command — or a view/query template — would make the
  call graph immediately useful to agents.

- **Agent config injection** (dreamlx/codeindex): auto-modifying agent
  configuration files (like `CLAUDE.md`) to include project context.
  Similar spirit to our shims but at the config layer.  We could
  generate a `.goosehints` fragment or append to the system prompt
  template automatically.

- **File watching** (LydiaCai1203/codeindex): chokidar-based real-time
  re-indexing.  Our lazy `--freshen` at the read boundary is simpler
  and daemonless, but a watch mode could be useful for long development
  sessions where the agent makes many edits between reads.

- **Multi-language support**: every competitor supports multiple
  languages.  Our indexer is Rust-only.  The shim architecture is
  language-agnostic (it just needs a populated `.codeindex.db`), so
  the gap is purely in the indexer.  Tree-sitter is the obvious path
  — which circles back to what the removed `analyze` tool did, but
  now as an offline index builder rather than a runtime tool.

## Key insight

The most effective way to change an AI agent's tool usage is not
instruction (prompt engineering), not competition (better tools), and
not enrichment (showing what it could do).  It's **contextual negative
feedback at the exact moment of the wrong choice**.  One scolding in
the output of the wrong tool, containing the right command to use
instead, changed behavior in a single occurrence.

This is perhaps unsurprising — it's how humans learn too.
