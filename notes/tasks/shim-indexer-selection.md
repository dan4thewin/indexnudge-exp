title: Shim selects correct language indexer
status: ready
depends: []
gates:
  - tests
  - commit

Observed in session: shim calls whichever `indexnudge` is first on PATH.
If the Rust indexer runs against a Go project, it finds no .rs files,
deletes all rows, and renames back an empty DB. The mkfile fix only
covers shim-test. In real usage both bin-rust/ and bin-go/ may be on
PATH. Fix: freshen checks `SELECT DISTINCT language FROM files` and
refuses to modify if the language doesn't match the indexer.
