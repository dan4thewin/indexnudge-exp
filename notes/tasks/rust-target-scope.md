title: Rust target exclusion scope
status: ready
depends: []
gates:
  - tests
  - commit

B7 — collect_rs_files skips any path component named "target". This is
overly broad: a legitimate directory named "target" inside the project
would be skipped. Should only skip the top-level "target" directory
(Cargo's build output) relative to the project root.
