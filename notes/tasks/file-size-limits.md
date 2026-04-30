title: Go collectGoFiles respects .gitignore
status: ready
depends: []
gates:
  - tests
  - commit

Originally about file size limits — concluded size limits aren't needed.
Large files in the DB don't hurt query performance (queries target symbols
or line ranges, not full table scans). Disk space is irrelevant for an
ephemeral DB.

The real fix: respect .gitignore so generated/vendored files the user
already excludes don't get indexed.

Rust: already uses `ignore::WalkBuilder` which respects .gitignore by default.
No change needed.

Go: uses `filepath.WalkDir` which ignores .gitignore entirely. Fix by
replacing with `git ls-files` for the file list. This inherently respects
all gitignore rules (nested .gitignore, global gitignore, .git/info/exclude)
without reimplementing the logic. Filter the output for .go files, exclude
_test.go and vendor/.

Verify: create a test with a .gitignore that excludes a .go file, confirm
it doesn't appear in the index.
