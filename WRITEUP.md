# Nudging an AI Agent Toward Better Tools

AI coding agents read source code the way a programmer at a terminal
might — grep for a pattern, sed to read around the hits, cat to see
the whole file.  These tools work.  They also waste 75% of the
agent's tool calls on orientation: reconstructing structural
understanding that a database could answer in one query.

The question that started this project: can you make an agent prefer
a structured database over raw file reads, without hardcoding the
behavior?

The answer is yes, but not the way you'd expect.

## The experiments

The experiments used a fixed task: investigate case exit status
propagation in a Rust compiler's `transform.rs` (~1,400 lines).
The task was given as a one-shot `goose run` against the same
codebase each time, with tool call logs captured and compared.  The
correct answer is "there is no bug" — reaching it requires reading
two functions, understanding a phi insertion pattern, and comparing
them.  A good tool user would do this in 2–3 targeted calls.

### Avenue 1: positive instructions

Added to the developer extension's system prompt: "When you have
access to the analyze tool, prefer it over grep/cat for understanding
code structure."

The agent used analyze once at the start to orient, then fell back to
grep/sed for everything else.  37 total calls, 19K tokens, 5
minutes.  Reached the correct conclusion.

Soft instructions don't override trained habits.

### Avenue 2: prescriptive prohibitions

Rewrote the instructions with explicit rules: "Do NOT use cat, sed,
or head/tail to read source code.  Do NOT use grep.  Use analyze for
all code investigation."

Worse than doing nothing.  60 total calls, 38K tokens, 14 minutes.
The model tried to fix the non-existent bug, hit build failures,
git-stashed, reverted, and spent the second half thrashing through
another file with sed.  Didn't reach the correct conclusion.

Prescriptive prohibitions are counterproductive.  The stronger
framing may have increased cognitive load without changing behavior.

### Avenue 3: richer tool output

The model falls back to sed because analyze doesn't show function
bodies — only call graphs and line numbers.  Modified analyze to
return the actual code: tracked `end_line` on every symbol, carried
source text in the analysis output, printed numbered source lines
for focused symbols.

3 analyze calls (up from 1–2), including one that followed the call
graph deeper than any previous run.  Still 14 grep, 11 sed — but
less sed for the focused function and more for files outside the
focus.

Partial win.  Enriching tool output helps at the margin.

### Avenue 4: negative reinforcement

Created PATH shims that returned error messages pointing to the
correct tools.  grep → "ERROR: grep is disabled.  Use rg."
sed → "ERROR: sed is disabled.  Use analyze."

7 analyze calls, 0 grep, 0 sed, 33 rg calls.  The model used rg
idiomatically — context flags, fixed-string mode, counting mode —
and followed analyze call graphs deeply.  60 total calls, 33K
tokens, 7 minutes.

Negative reinforcement works where instructions fail.  The model
can use the right tools.  It won't volunteer to.

### Avenue 5: transparent upgrade

Instead of blocking grep, upgrade it.  The shim intercepts grep, runs
`git grep -p` instead, returns JSON with function names containing
each match plus a hint to use analyze.  The model gets better results
than it asked for, plus a breadcrumb to the right workflow.

5 analyze calls, 14 grep (all returning JSON), 1 sed, 5 awk (routed
around the sed shim), 2 write, 1 edit.  40 total calls (lowest),
23K tokens (second lowest), 4 minutes (fastest).  Correct conclusion.

The model partially followed the grep→analyze breadcrumb but also
discovered `awk 'NR>=X && NR<=Y'` as a sed replacement.  Block one
tool, they find another.

### Summary

| Metric     | Soft | Ban | Enrich | Block | Upgrade |
|------------|-----:|----:|-------:|------:|--------:|
| analyze    |    1 |   2 |      3 |     7 |       5 |
| grep       |   10 |  18 |     14 |     0 |      14 |
| sed        |    3 |  14 |     11 |     0 |       1 |
| Total      |   37 |  60 |     60 |    60 |      40 |
| Tokens     |  19K |  38K |   31K |   33K |     23K |
| Duration   |   5m |  14m |   10m |    7m |      4m |
| Correct?   |  Yes |   No |  Maybe | Maybe |     Yes |

## From experiments to indexnudge

Three findings shaped what came next:

1. Instructions don't override training.  Neither soft nor hard
   system prompt instructions reliably change tool selection.
2. Models route around obstacles.  Block sed, they discover awk.
   The underlying behavior — line-oriented code navigation — will
   find expression through whatever tools remain.
3. Transparent upgrades are the sweet spot.  Replace the tool's
   output with something better, and the model benefits without
   needing to change its behavior.

The key insight: instead of fighting the model's habits, use them as
a delivery mechanism.  The agent will call grep and sed.  Let it.
Intercept those calls, serve better data, and show by example what
the right query looks like.

## The index

indexnudge is a standalone CLI that walks a source tree and produces
a `.indexnudge.db` — a SQLite database containing:

- Every line of source, tagged with its enclosing symbol, full-text
  searchable via FTS5
- Every symbol definition — functions, methods, structs, traits,
  interfaces — with name, kind, file, line range, and signature
- Every call site, caller to callee
- Every variable binding and assignment
- Every import

Two indexers share the same schema: a Rust indexer using `syn` and a
Go indexer using `go/ast` with `go/types` for accurate type
information.  The Go indexer adds a `channels` table tracking the
lifecycle of Go channels — where created, written to, read from,
closed, and selected on.  `go/types` ensures that `range` over a
slice doesn't appear in the channels table; only actual channel
operations do.

The schema is the interface.  `.schema` is the documentation.

## The tree patch

Every agent session starts with `tree`.  It's the orientation
tool — the first call in almost every session.  A small patch to
goose's tree tool (maintained as `patches/goose-tree-indexnudge.patch`)
detects `.indexnudge.db` by walking up from the target directory and,
when found, serves structured tables from the database instead of a
filesystem listing.  The output ends with a nudge:

```
Every line of source is in `lines`. Every function call is in `calls`.
For deeper exploration: sqlite3 .indexnudge.db .schema
```

The agent follows the breadcrumb.  On its next tool call it runs
sqlite3 — not grep.

The patch also checks, once per process lifetime, whether the
indexnudge shims are on PATH.  It finds `sed` in PATH order and reads
the first 512 bytes — the real sed is an ELF binary, the shim is a
bash script containing "indexnudge."  If the shims aren't first on
PATH, the tree output includes a warning with the exact `export
PATH=...` command to fix it.

## The shims

Six shell shims — `sed`, `grep`, `head`, `tail`, `cat`, `sqlite3` —
sit in `bin/` and intercept the patterns agents use to read code.
Each shim:

1. Detects `.indexnudge.db` by walking up from the target file
2. Serves the data from the database instead of the filesystem
3. Shows the SQL query that produced the result
4. Falls through to the real tool when the database isn't available
   or the command isn't a code-reading pattern

The sed shim adds one line of negative reinforcement:

```
** STOP USING sed. Query the DB directly: sqlite3 .indexnudge.db
   "SELECT content FROM lines WHERE ..." **
```

One contextual scolding at the moment of the wrong choice changes
behavior within a single occurrence.

The sqlite3 shim is the simplest and most important.  Every path
leads to sqlite3 — the other shims redirect there, and once the
agent learns, it goes direct.  The sqlite3 shim calls
`indexnudge --freshen` before every query, guaranteeing that the
database reflects the current state of the source files.

Freshness is evaluated lazily at the read boundary.  No daemons, no
filesystem watchers, no hooks on write or edit.  The agent can
mutate files through any mechanism; the index is refreshed when
something reads from it.

## Results

### Brush (Rust, 148 files, 32K lines)

One-shot goose run with a code exploration prompt.  Shims on PATH,
patched tree.

7 of 12 tool calls went straight to the database.  The single sed
call was the agent's first code-read attempt.  After the scolding,
every subsequent read was a `SELECT content FROM lines`.

### TibbitWiki (Go, 14 files, 3.7K lines)

Same setup with the Go indexer.

The agent queried the `channels` table on its second tool call —
tracing the goroutine architecture without any prompting about where
to look.  It used `cat` for four small files, which is reasonable at
that scale.  Zero sed, zero grep, zero head.

## Why it works

The system prompt tells the agent what to do.  The shim tells it what
it just did wrong.  Both are needed, but they work at different
moments.

The prompt instruction is 10,000 tokens in the past by the time the
agent decides to read lines 80–160 of a file.  The shim's correction
fires at that exact moment, in the output the agent is actively
processing.

Enriched output without the scolding doesn't change behavior — the
agent sees example queries but doesn't adopt them.  The scolding
without the enriched output would force adaptation but wouldn't show
the right path.  Together, they produce a behavioral shift within one
occurrence that persists for the rest of the session.

The hierarchy, from most to least effective:

1. In-band correction at the moment of the wrong choice
2. Tool output enrichment (showing what the right choice looks like)
3. System prompt instruction
4. Adding better tools alongside existing ones

## Open questions

**Does learning persist across sessions?**  No.  The agent has no
memory.  Each session, it tries sed once, gets scolded, and converges.
That first sed call is the cost of the pattern — a kind of toll.

**What about larger codebases?**  The TibbitWiki run used `cat` for
small files.  On a 100K-line codebase the agent would presumably not
cat a 3,000-line file.  Whether it goes to sqlite3 or falls back to
sed for the first read is the question — and the shim is there for
exactly that case.

**Should the index replace the filesystem entirely?**  No.  The
agent still needs `write` and `edit` to modify files.  The index
replaces *reads*, not writes.  The freshness mechanism bridges the
gap.
