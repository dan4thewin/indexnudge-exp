title: Schema version check on freshen
status: ready
depends: []
gates:
  - tests
  - commit

H1 — freshen should refuse to run if the DB schema doesn't match the
current indexer version. Currently a schema mismatch (e.g., missing
callee_qname column) causes runtime SQL errors. Store a version in
a pragma or metadata table; freshen checks it and exits with a clear
message telling the user to re-run full index.
