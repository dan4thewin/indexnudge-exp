# Status

31 commits on master, not pushed. No remote configured.

## Done

- Core: freshen, shims, rename, goose submodule+patch, tree diagnostics
- Tests: 11 Rust, 20 Go, shim tests pass with controlled PATH
- Docs: README, EXPERIMENTS.md rewritten, claims scoped to Opus 4.6/goose
- Audit remediation COMPLETE:
  - B4: line-count off-by-one (TrimSuffix + regression test)
  - B3: unchecked DB write errors (all extractors return error, errcheck clean)
  - B6: Rust extract_span UTF-8 panic — won't fix (preconditions can't arise, panic correct)
  - B2+B5+S3+S4: rename protocol, DELETE journal, busy_timeout, concurrent freshen guard
  - parse_error column: index lines for unparseable files, skip structure, set flag
  - mkfile shim-test uses controlled PATH (prevents cross-language indexer contamination)
- Concurrency design fully implemented
- Design decisions documented (anti-goals, model selection, WAL rejection, synchronous=OFF justified)
- Test results documented (brush, TibbitWiki)
- iwe knowledge graph set up, .goosehints directs future sessions here
- Wiki page on rime: Indexnudge Adversarial Audit

## iwe enhancement work

Spec and implementation plan written at ~/src/iwe/:
- ENHANCEMENTS_SPEC.md — full spec, all design questions resolved
- IMPLEMENTATION_PLAN.md — exact files, changes, tests, phase ordering
- .goosehints — iwe-specific project hints
- run-enhancements.sh — launch script for autonomous goose session

Four phases: config plumbing → bare front matter → --meta find filter → virtual directories
Plus task management wrapper (shell script, separate from iwe)

Ready to launch via run-enhancements.sh.

## Session observations

- ed with line addresses works naturally with shim-annotated output; the edit tool's exact-match requirement breaks the shim loop
- Behavior changes stick when they relieve active friction, not when they're theoretically better
- "Ingrained in training data" was a rationalization for tree; real blocker is not self-monitoring tool usage patterns
- Delegates get full system prompt including global/project hints — can be counterproductive for work in different repos
- Distinct goose sessions are better than delegates for cross-repo work: own cwd, own .goosehints, own PATH, own delegates

## Remaining (strengthen tier)

1. Incremental FTS (per-file delete/insert during freshen)
2. Freshen edge case tests (empty files, CRLF, NUL)
3. Go full/freshen file set mismatch — shared enumerator
4. Call callee naming joinable to symbols
5. Go call→symbol resolution via types.Info
6. Symlink blocking
