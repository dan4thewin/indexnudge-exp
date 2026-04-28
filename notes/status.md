# Status

30 commits on master, not pushed. No remote configured.

## Done

- Core: freshen, shims, rename, goose submodule+patch, tree diagnostics
- Tests: 11 Rust, 18 Go, 11 shim, 4 tree diagnostic cases verified
- Docs: README, EXPERIMENTS.md rewritten, claims scoped to Opus 4.6/goose
- Audit: 8-model adversarial audit, triage, reconciled remediation plan
- Bug fixes: extractImports token.Pos → line number (B1)
- Audit remediation complete:
  - B4: line-count off-by-one (TrimSuffix + regression test)
  - B3: unchecked DB write errors (all extractors return error, errcheck clean)
  - B6: Rust extract_span UTF-8 panic — won't fix (preconditions can't arise, panic is correct for corruption)
  - B2+B5+S3+S4: rename protocol, DELETE journal, busy_timeout, concurrent freshen guard
- Concurrency design implemented (rename protocol, DELETE journal, synchronous=OFF justified by rename as durability boundary)
- Design decisions documented (anti-goals, model selection, WAL rejection)
- Test results documented (brush, TibbitWiki)
- iwe knowledge graph set up, .goosehints directs future sessions here
- iwe skill improved and installed to ~/.config/goose/skills/
- Wiki page on rime: Indexnudge Adversarial Audit (report card, excerpts, plan changes)
- mkfile shim-test uses controlled PATH to prevent cross-language indexer contamination

## Concurrency design — remaining items

- parse_error column in files table
- Parse failure handling (index lines, drop structure, set flag)
- Tree patch to report files with parse_error = 1
- Tests: parse error scenarios, concurrent freshen, failed freshen

## iwe enhancements

Spec written at ~/src/iwe/ENHANCEMENTS_SPEC.md covering:
1. Bare front matter support (enables TibbitWiki compatibility)
2. --meta key=value filtering on find (enables querying by front matter fields)
3. Virtual directory nodes (auto-include directory contents)
4. Task management wrapper (workflow policy, gate enforcement, readiness propagation)

## Session observations

- ed with line addresses works naturally with shim-annotated output; the edit tool's exact-match requirement is what breaks the shim loop
- Behavior changes stick when they relieve active friction, not when they're theoretically better
- iwe knowledge graph needs task-tracking notes with checkboxes, not just design docs — todo extension is ephemeral, doesn't persist across sessions
- Targeted iwe retrieval (retrieve -k specific-key -d 0) beats bulk retrieval (retrieve -k root -d 2 | grep)
