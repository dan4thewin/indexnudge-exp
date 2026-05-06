title: Rotating structural query nudges across all shims
status: blocked
depends:
  - shim-group-by-symbol
gates:
  - tests
  - commit
  - benchmark

Blocked: the grouped-by-symbol output already shows symbol names and
line ranges prominently. The rotating nudges may be redundant — the
benchmark post-shim-changes will tell us if further nudging is needed.

Defer until after grouped-by-symbol is working and measured. If Opus
still doesn't use structural queries after seeing symbol-grouped grep
output, then nudges become the next intervention.
