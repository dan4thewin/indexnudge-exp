# Audit Remediation

Reconciled plan from 8-model adversarial audit (run-1). See `audit/run-1/reconciliation_gpt-5.4.txt` for full plan including amendment. Concurrency design in `notes/concurrency-design.md` subsumes and reorders several items.

## Fix-now

1. B4: Go line-count off-by-one (strings.Split trailing newline)
2. B3: Go unchecked DB write errors (all extractors)
3. B6: Rust extract_span panic on multi-byte UTF-8
4. B2 + B5 + S3 + S4: **Concurrency and failure safety** (one coherent change):
   - Switch `journal_mode=WAL` → `journal_mode=DELETE`
   - Switch `synchronous=OFF` → `synchronous=NORMAL`
   - Add `parse_error INTEGER DEFAULT 0` to files table
   - Full reindex: rename canonical → `.bak`, build to `.tmp`, rename on success
   - Freshen: rename canonical → `.bak`, `BEGIN IMMEDIATE` on `.bak`, do work, `COMMIT`, rename back on success. `ROLLBACK` on failure, canonical absent, fallback.
   - Parse failure: index lines, drop structure, set `parse_error = 1`
   - Concurrent freshen guard: if `.bak` exists and canonical absent, exit immediately
   - Shims: `PRAGMA busy_timeout = 5000` on reader side
   - Tree patch: report files with `parse_error = 1`
   - Tests: parse error → lines present, structure absent, flag set. Fix file → flag cleared. Concurrent freshen → second one exits. Failed freshen → canonical absent, `.bak` untouched.

## Strengthen

1. T1+T2: Freshen tests verify all tables + edge case tests (empty files, CRLF, NUL, parse errors)
2. C3+H2: Go full/freshen file set mismatch — one shared enumerator. Stop ignoring pkg.Errors.
3. C2+T4: Call callee naming joinable to symbols — add `callee_qname` field, validate with join-count assertions
4. Go call→symbol resolution via types.Info.Uses/Selections (~20 lines on existing infrastructure)
5. S1: Symlink blocking — skip symlinks by default via Lstat/symlink_metadata

## Defer

- C1+C4: Cross-language naming docs (intentional, document don't change)
- H1: Schema version check — refuse freshen on mismatch
- B7: Rust `target` exclusion scope (low risk)
- H3: Rust struct/const end_line uses identifier span only
- S2: File-size limits
- T3: Go channel tests — upgrade to exact count/location assertions
