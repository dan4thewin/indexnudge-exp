title: File size limits
status: design
depends: []
gates:
  - tests
  - commit

S2 — no upper bound on file size. A multi-megabyte generated file gets
fully indexed (lines, symbols, calls, etc.), bloating the DB and slowing
queries. Need to decide: skip files over N bytes? Index lines only?
Log a warning? What threshold?
