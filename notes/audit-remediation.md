# Audit Remediation

Reconciled plan from 8-model adversarial audit (run-1). See `audit/run-1/reconciliation_gpt-5.4.txt` for full plan including amendment. Concurrency design in `notes/concurrency-design.md` subsumes and reorders several items.

## Fix-now — COMPLETE

All fix-now items resolved (commits 2965079, a5d0de0):

- [x] B4: Go line-count off-by-one — TrimSuffix before Split, regression test added
- [x] B3: Go unchecked DB write errors — all extractors return error, errcheck clean
- [x] B6: Rust extract_span panic on multi-byte UTF-8 — won't fix, preconditions can't arise from syn output, panic is correct for corruption
- [x] B2+B5+S3+S4: Concurrency and failure safety:
  - [x] `journal_mode=DELETE` (no sidecar files to break rename)
  - [x] `synchronous=OFF` (justified: rename is durability boundary, not fsync)
  - [x] Rename protocol: full index builds to .tmp, freshen renames to .bak
  - [x] Concurrent freshen guard: .bak sentinel
  - [x] `BEGIN IMMEDIATE` / `COMMIT` in freshen
  - [x] Shims: `busy_timeout=5000`
  - [x] mkfile: shim-test uses controlled PATH (prevents cross-language indexer contamination)
  - [ ] `parse_error INTEGER DEFAULT 0` column in files table
  - [ ] Parse failure handling: index lines, drop structure, set `parse_error = 1`
  - [ ] Tree patch: report files with `parse_error = 1`
  - [ ] Tests: parse error scenarios, concurrent freshen, failed freshen

## Strengthen

1.  Incremental FTS: per-file FTS delete/insert during freshen instead of full rebuild. Current full rebuild is the slowest part of freshen on large repos.
2.  T1+T2: Freshen tests verify all tables + edge case tests (empty files, CRLF, NUL, parse errors)
3.  C3+H2: Go full/freshen file set mismatch — one shared enumerator. Stop ignoring pkg.Errors.
4.  C2+T4: Call callee naming joinable to symbols — add `callee_qname` field, validate with join-count assertions
5.  Go call→symbol resolution via types.Info.Uses/Selections (~20 lines on existing infrastructure)
6.  S1: Symlink blocking — skip symlinks by default via Lstat/symlink_metadata

## Defer

- C1+C4: Cross-language naming docs (intentional, document don't change)
- H1: Schema version check — refuse freshen on mismatch
- B7: Rust `target` exclusion scope (low risk)
- H3: Rust struct/const end_line uses identifier span only
- S2: File-size limits
- T3: Go channel tests — upgrade to exact count/location assertions
