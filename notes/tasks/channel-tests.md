title: Go channel test assertions
status: ready
depends: []
gates:
  - tests
  - commit

T3 — channel extraction tests only check presence, not exact counts
or locations. Upgrade to exact count assertions and verify line numbers
for send, recv, make, close, range, and select operations.
