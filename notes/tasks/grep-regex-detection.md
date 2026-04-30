title: Grep shim routes regex through SQLite REGEXP
status: ready
depends:
  - grep-fallthrough
gates:
  - tests
  - commit

Originally about detecting regex and falling through to real grep.
SQLite has REGEXP support — use it instead.

Three routing tiers:
- Simple alphanumeric → FTS5 MATCH (fastest, ranked)
- Regex patterns → WHERE content REGEXP 'pattern' on lines table
- LIKE goes away entirely — it was the broken middle ground

Fallthrough to real grep only for unsupported flags (-P for PCRE,
or flags the shim doesn't parse).

Perf: REGEXP is a full table scan on lines, but grep -r is also a
full filesystem scan — not worse. Sub-second for typical codebases.

This eliminates the false-result problem where LIKE treated regex
metacharacters literally.
