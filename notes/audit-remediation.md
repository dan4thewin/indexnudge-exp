# Audit Remediation

Reconciled plan from 8-model adversarial audit (run-1).
See `audit/run-1/reconciliation_gpt-5.4.txt` for full plan.

## Fix-now

- [ ] B4: Go line-count off-by-one (strings.Split trailing newline)
- [ ] B3: Go unchecked DB write errors (all extractors)
- [ ] B2: Freshen data loss on parse failure — amended plan: index lines, drop structure, record `parse_error` in files table, tree patch shows parse error note
- [ ] B6: Rust extract_span panic on multi-byte UTF-8

## Strengthen

- [ ] T1+T2: Freshen tests verify all tables + edge case tests
- [ ] C3+H2: Go full/freshen file set mismatch + pkg.Errors
- [ ] C2+T4: Call callee naming joinable to symbols
- [ ] Go call→symbol resolution via types.Info.Uses/Selections (~20 lines on existing infrastructure)

## Defer

- [ ] B5: Atomic full reindex (temp DB + rename)
- [ ] S3+S4: Transaction boundaries + synchronous=NORMAL
- [ ] C1+C4: Cross-language naming docs
- [ ] S1: Symlink blocking
- [ ] H1: Schema version check
