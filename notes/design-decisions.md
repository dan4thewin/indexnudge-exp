# Design Decisions

Decisions made during development with rationale, so future sessions
don't relitigate them.

## Cross-language normalization is an anti-goal

One audit model suggested a cross-language normalization layer — mapping
language-specific values to common ones (`impl`/`interface` → `type_impl`,
`::` → `.`, etc.). Rejected. Each language has its own idioms. Flattening
`impl` and `interface` into the same bucket loses information. The whole
point of the index is to give the agent precise structural data for the
language it's working in. The agent works in one language per session,
not both.

Keep native separators in storage (`Point::distance` for Rust,
`Point.Distance` for Go). Document the differences, don't normalize them.

## Language-specific indexers, not a multi-language parser

Rather than write Go support into the Rust program or use tree-sitter
for both, we wrote a separate Go program. Each language has its own native
parser (`syn` for Rust, `go/ast` + `go/types` for Go). This gives deep
language-specific features (Go channel tracking via type info) that a
generic parser can't provide. The shared schema is the contract.

## WAL mode is incompatible with the concurrency design

WAL allows readers to proceed during writes — readers never block writers
and vice versa. This means readers can see stale data during freshen,
which violates the design goal of "never serve stale data during an update."
Rollback journal mode (`journal_mode=DELETE`) gives proper lock escalation:
RESERVED → PENDING (blocks new readers) → EXCLUSIVE (waits for existing
readers to drain). See `concurrency-design.md` for details.

## synchronous=OFF is safe under rename protocol

The concurrency design originally specified `synchronous=NORMAL`. We use
`synchronous=OFF` instead. Rationale: the rename is the durability boundary,
not fsync. In both paths (full index and freshen), the data is written to a
temporary file/backup that gets renamed to the canonical path only on success.
If a crash happens mid-write, the temporary is discarded — the canonical DB
was never modified. So fsync during writes is wasted I/O. The rename (which
is atomic on Linux/macOS) is what makes the data durable.

Performance impact is significant — ~100x difference on bulk inserts.

## Audit model selection

For future audits, 5 well-chosen models catch everything 9 models found.
The best combination: claude-opus-4.7 (densest, unique high-severity finds),
gpt-5.5 (best structured, best open-ended), gpt-5.2 (FTS insight),
gpt-5.3-codex (maximum efficiency), one sonnet for Go depth.
Drop claude-sonnet-4 (added nothing) and be cautious with agent-mode
models that waste tokens on tool calls during static analysis.

Full report card in the wiki: `~/notes/hold/Indexnudge Adversarial Audit.md`
on rime.
