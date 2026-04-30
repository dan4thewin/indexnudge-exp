title: Rotating structural query nudges across all shims
status: ready
depends:
  - grep-fallthrough
gates:
  - tests
  - commit

Every shim invocation should emit a rotating one-liner nudge showing
a structural query example. The agent doesn't know what the symbols,
calls, or imports tables contain — showing executable examples teaches
the workflow.

Pool of nudges (cycle via RANDOM or counter):
- SELECT name, kind, file, start_line FROM symbols WHERE name LIKE '%Foo%'
- SELECT DISTINCT file FROM files — see all indexed files
- .schema — see all available tables
- SELECT callee, callee_qname FROM calls WHERE caller_file='path'
- SELECT name, start_line, end_line FROM symbols WHERE file='path' — file outline

Implementation: `emit_nudge` function in `_indexnudge_common.bash`.
Every shim calls it after producing output. On fallthrough to real
command (no DB results), do NOT emit a nudge — avoid confusing the
agent when the shim is transparent.

Depends on grep-fallthrough so that failed queries fall through
silently rather than printing DB chrome + nudge on an empty result.
