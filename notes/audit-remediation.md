title: Audit Remediation
status: done

Reconciled plan from 8-model adversarial audit (run-1). See `audit/run-1/reconciliation_gpt-5.4.txt` for full plan including amendment. Concurrency design in `notes/concurrency-design.md` subsumes and reorders several items.

## Fix-now — COMPLETE

All fix-now items resolved:

- [x] B4: Go line-count off-by-one — TrimSuffix before Split, regression test
- [x] B3: Go unchecked DB write errors — all extractors return error, errcheck clean
- [x] B6: Rust extract_span panic — won't fix, preconditions can't arise, panic is correct
- [x] B2+B5+S3+S4: Concurrency and failure safety (rename protocol, DELETE journal, busy_timeout, shims, parse_error column)

## Strengthen — COMPLETE

- [x] S1: Incremental FTS — per-file insert alongside lines, no full rebuild
- [x] T1+T2: Freshen tests verify all tables + edge cases
- [x] C3: Shared file enumerator (fullIndex + doFreshen use collectGoFiles)
- [x] C2+C5+T4: callee_qname + types.Info resolution + join tests
- [x] S6: Symlink blocking

## Defer — moved to tasks/

Remaining items tracked as individual task notes in `notes/tasks/`.
