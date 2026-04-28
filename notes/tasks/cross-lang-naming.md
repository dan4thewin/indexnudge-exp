title: Document cross-language naming conventions
status: ready
depends: []
gates:
  - docs
  - commit

C1+C4 — Go and Rust indexers produce different qualified_name formats
(Go: "pkg.Func", Rust: "mod::func"). This is intentional — document
the conventions so shim/query authors know what to expect from each
language's symbols and callee_qname fields.
