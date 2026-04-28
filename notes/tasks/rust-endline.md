title: Rust struct/const end_line accuracy
status: ready
depends: []
gates:
  - tests
  - commit

H3 — Rust extractor uses identifier span for end_line on structs and
constants, which gives the line of the name, not the closing brace.
For structs, enums, and impl blocks, end_line should reflect the full
span of the item body.
