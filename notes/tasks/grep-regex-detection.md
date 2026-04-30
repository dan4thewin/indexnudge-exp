title: Grep shim detects regex and falls through
status: ready
depends: []
gates:
  - tests
  - commit

The grep shim routes non-alphanumeric patterns to LIKE, which treats
regex metacharacters literally. `grep -r 'foo.*bar'` hits the LIKE
path, which searches for the literal string `foo.*bar` — silently
wrong results.

Fix: classify the pattern before querying. If it contains regex
metacharacters (. * + ? [ ] ( ) { } ^ $ \), fall through to real
grep immediately. Route only:
- Simple alphanumeric tokens → FTS5 MATCH
- Simple strings with spaces/punctuation but no regex → LIKE
- Actual regex → real grep
