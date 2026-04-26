# Audit Triage — Run 1

8 models audited (claude-opus-4.5, claude-opus-4.7, claude-sonnet-4, claude-sonnet-4.6,
gpt-4.1, gpt-5.2, gpt-5.3-codex, gpt-5.4, gpt-5.5). gemini-2.5-pro unavailable.

## PRODUCTION BUGS

### B1. Go `extractImports` stored `token.Pos` not line number
- **Found by:** 4/8 (opus-4.5, gpt-5.2, gpt-5.3-codex, sonnet-4.6)
- **Status:** FIXED (commit d69764e). Test added asserting line < 10.
- **Severity:** high

### B2. Freshen deletes rows before re-index; parse failure = data loss
- **Found by:** 8/8 (all models)
- **Description:** Both Rust and Go delete existing rows for a stale file before
  attempting to re-parse/re-index. If parsing fails (syntax error, read error),
  the file silently disappears from the DB. Rust returns Ok(()); Go continues.
- **Severity:** high

### B3. Go ignores DB write errors (stmt.Exec unchecked)
- **Found by:** 6/8 (opus-4.5, opus-4.7, gpt-4.1, gpt-5.3-codex, gpt-5.4, gpt-5.5)
- **Description:** Every Go extractor uses `stmt, _ := db.Prepare(...)` and
  ignores `stmt.Exec` return values. A locked/full DB silently produces
  partial output. Nil stmt from failed Prepare → panic.
- **Severity:** high

### B4. Go line count off-by-one (strings.Split trailing newline)
- **Found by:** 5/8 (opus-4.7, gpt-5.2, gpt-5.3-codex, gpt-5.5, sonnet-4.6)
- **Description:** `strings.Split(src, "\n")` produces N+1 elements for a file
  ending in `\n`. Extra empty line inserted into `lines` table, `files.lines`
  wrong. Rust `source.lines()` is correct.
- **Severity:** high (affects every real file)

### B5. Full index deletes old DB before proving new one can be built
- **Found by:** 3/8 (gpt-5.4, gpt-5.5, sonnet-4.6)
- **Description:** Both `os.Remove(dbPath)` / `fs::remove_file` before starting.
  Any subsequent failure leaves no usable index.
- **Severity:** medium

### B6. Rust `extract_span` byte/char index mismatch on multi-byte UTF-8
- **Found by:** 2/8 (opus-4.7, sonnet-4.6)
- **Description:** Column values from proc_macro2 are character offsets but
  `line[start_col..end]` slices by bytes. Panics on non-ASCII source.
- **Severity:** high (panic)

### B7. Rust `collect_rs_files` excludes any path component named "target"
- **Found by:** 2/8 (opus-4.7, gpt-4.1)
- **Description:** A legitimate `src/target/mod.rs` would be excluded.
- **Severity:** low (unlikely in practice)

## CROSS-LANGUAGE INCONSISTENCIES

### C1. Qualified name separator: Rust `::` vs Go `.`
- **Found by:** 5/8 (opus-4.7, gpt-5.2, gpt-5.3-codex, gpt-5.4, gpt-5.5)
- **Description:** `Point::distance` vs `Point.Distance`. Any cross-language
  query needs to handle both. Symbol kind vocabularies also differ.
- **Severity:** medium (by design, but undocumented)

### C2. Call callee naming inconsistency
- **Found by:** 5/8 (opus-4.7, gpt-5.2, gpt-5.4, gpt-5.5, sonnet-4.6)
- **Description:** Rust stores bare method name (`distance`), Go stores
  `receiver.Method` (`p.Distance`). Neither matches the qualified symbol name.
  Call→symbol joins are unreliable.
- **Severity:** medium

### C3. Go full index vs freshen index different file sets
- **Found by:** 4/8 (opus-4.5, gpt-5.3-codex, gpt-5.4, gpt-5.5)
- **Description:** `fullIndex` uses `go/packages`; `doFreshen` uses
  `collectGoFiles` + fallback parser. Build-tagged files may appear
  only in one mode.
- **Severity:** high

### C4. Go excludes `_test.go`; Rust indexes all `.rs`
- **Found by:** 2/8 (gpt-5.5, sonnet-4.6)
- **Description:** Different coverage decisions, undocumented.
- **Severity:** low (intentional but should be documented)

## TEST GAPS

### T1. Freshen tests don't verify all tables cleaned
- **Found by:** 4/8 (gpt-5.2, gpt-5.4, gpt-5.5, sonnet-4.6)
- **Description:** Tests check symbols count but not lines, calls, imports,
  defs, FTS, or directories after freshen.
- **Severity:** medium

### T2. No tests for parse errors, empty files, binary files, CRLF, NUL
- **Found by:** 4/8 (opus-4.7, gpt-4.1, gpt-5.5, sonnet-4.6)
- **Severity:** medium

### T3. Go channel tests check presence not counts/accuracy
- **Found by:** 2/8 (gpt-5.5, sonnet-4.6)
- **Severity:** low

### T4. Tests don't validate call→symbol joinability
- **Found by:** 3/8 (gpt-5.2, gpt-5.4, gpt-5.5)
- **Severity:** medium (tests assert broken behavior)

## SECURITY / ROBUSTNESS

### S1. Symlink following — can index files outside project
- **Found by:** 3/8 (gpt-4.1, gpt-5.4, gpt-5.5)
- **Severity:** medium

### S2. No file-size limits
- **Found by:** 2/8 (gpt-4.1, gpt-5.5)
- **Severity:** low

### S3. No transaction boundaries — crash leaves partial DB
- **Found by:** 3/8 (opus-4.7, gpt-5.4, gpt-5.5)
- **Severity:** medium (also a performance issue)

### S4. PRAGMA synchronous=OFF — crash can corrupt DB
- **Found by:** 1/8 (gpt-5.4)
- **Severity:** low (indexing is idempotent, DB is rebuildable)

## HARDENING

### H1. No schema version check on freshen
- **Found by:** 2/8 (opus-4.7, gpt-5.5)
- **Severity:** low

### H2. Go `loadPackages` ignores `pkg.Errors`
- **Found by:** 2/8 (gpt-5.5, sonnet-4.6)
- **Severity:** medium

### H3. Rust struct/const/static `end_line` is just the identifier span
- **Found by:** 2/8 (gpt-5.5, sonnet-4.6)
- **Severity:** low (symbols have limited range info)

## OPEN-ENDED ANSWERS

### A. Most damaging thing not in findings

| Model | Answer |
|-------|--------|
| opus-4.5 | Freshen can silently corrupt (subsumes B2) |
| opus-4.7 | No transactions — crash leaves partial DB, and freshen trusts incomplete data |
| gpt-5.3-codex | No crash-safe transactional model |
| gpt-5.4 | `PRAGMA synchronous=OFF` — crash can corrupt the persisted artifact |
| gpt-5.5 | No versioned golden corpus defining expected output — can't distinguish bugs from approximations |
| sonnet-4.6 | Go nil pointer panic when loadPackages encounters type errors |

Consensus: **lack of transactional integrity** (3/6 models independently).

### B. Largest missed opportunity

| Model | Answer |
|-------|--------|
| opus-4.5 | No incremental FTS — full rebuild every time |
| opus-4.7 | No call→symbol resolution — calls are string matches, not resolved edges |
| gpt-5.3-codex | (truncated) |
| gpt-5.4 | Resolve call sites to symbol IDs for real call graph |
| gpt-5.5 | Same — normalize references to stable semantic IDs |
| sonnet-4.6 | Cross-language normalization layer |

Consensus: **resolve calls to symbol IDs** (4/6 models independently).

## PRIORITY ORDER

### fix-now (production bugs)
1. B4 — Go line count off-by-one (trivial fix, affects every file)
2. B3 — Go ignores DB write errors (systematic, partial output)
3. B2 — Freshen data loss on parse failure (both languages)
4. B6 — Rust extract_span panic on UTF-8

### strengthen (test gaps + consistency)
5. T1 — Freshen tests: verify all tables
6. C3 — Go full/freshen file set mismatch
7. C2 — Call callee naming → make joinable to symbols
8. T2 — Edge case tests (parse errors, empty files)

### defer (design decisions, low severity)
9. B5 — Atomic full reindex (write to temp, rename)
10. S3 — Transaction boundaries
11. C1 — Cross-language naming convention (by design)
12. S1, S2, H1, H2, H3 — hardening items
