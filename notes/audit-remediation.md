# Audit Remediation

Reconciled plan from 8-model adversarial audit (run-1). See `audit/run-1/reconciliation_gpt-5.4.txt` for full plan including amendment.

## Fix-now

1. B4: Go line-count off-by-one (strings.Split trailing newline)
2. B3: Go unchecked DB write errors (all extractors)
3. B2: Freshen data loss on parse failure — **amended**: index lines always, drop structure on parse failure, record `parse_error INTEGER DEFAULT 0` in files table. Tree patch appends note listing files with parse errors. Tests: broken file → lines present, symbols empty, `parse_error = 1`; fix file, freshen → structure restored, `parse_error = 0`.
4. B6: Rust extract_span panic on multi-byte UTF-8

## Strengthen

1. B5: Atomic full reindex — build to temp DB, rename on success, never delete old DB first. (Promoted from defer per reconciled plan.)
2. T1+T2: Freshen tests verify all tables + edge case tests (empty files, CRLF, NUL, parse errors)
3. C3+H2: Go full/freshen file set mismatch — one shared enumerator. Stop ignoring pkg.Errors.
4. C2+T4: Call callee naming joinable to symbols — add `callee_qname` field, validate with join-count assertions
5. Go call→symbol resolution via types.Info.Uses/Selections (~20 lines on existing infrastructure)
6. S1: Symlink blocking — skip symlinks by default via Lstat/symlink_metadata

## Defer

- S3+S4: Transaction boundaries + synchronous=NORMAL
- C1+C4: Cross-language naming docs (intentional, document don't change)
- H1: Schema version check — refuse freshen on mismatch
- B7: Rust `target` exclusion scope (low risk)
- H3: Rust struct/const end_line uses identifier span only
- S2: File-size limits
- T3: Go channel tests — upgrade to exact count/location assertions
