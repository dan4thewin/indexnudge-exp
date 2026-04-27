# Test Results

## Brush (Rust, ~30 files)

Ran goose with patched tree and indexnudge shims on PATH against the
brush shell project. 5 runs total across sessions, varying prompts
about command execution flow.

Best run (run 5): 12 tool calls total.
- tree: 1 (served from .indexnudge.db)
- sqlite3 direct: 7 (agent went straight to DB queries unprompted)
- sed: 1 (shim redirected to DB)
- head: 2 (one README, one via shim)
- grep: 2 (Cargo.toml — non-indexed file, correct fallback)
- cat: 0

7/12 calls were direct sqlite3 queries. The agent picked up the nudge
from tree output and went to the DB on its own.

## TibbitWiki (Go, 14 files, 3.7K lines)

Ran goose with Go indexer, patched tree, bin-go shims. Prompt asked
about project structure, goroutine architecture, channel usage.

7 tool calls: tree, 2 sqlite3, 4 cat. Zero sed/grep/head. The agent
went to the channels table immediately — the type-aware channel tracking
(make/send/recv/close/select/range) paid off on the first query. Agent
correctly traced the goroutine architecture (wiki.loop, watcher.eventLoop,
hub) without prompting about where to look.

The `cat` calls were on small files (140-955 lines) — arguably more
efficient than piecemeal queries for files that size. Agent made good
choices.

## Observations

- The nudge works. Tree output showing DB-sourced data is the key signal.
- All results from Opus 4.6 on goose. Generalizability to other models
  and harnesses is unproven.
- Smaller files get `cat`'d whole rather than queried — this is rational
  behavior, not a failure of nudging.
- The channels table was the highest-value addition for Go — immediate
  payoff on a real codebase.
