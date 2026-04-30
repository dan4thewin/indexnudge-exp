title: Shim output uses symbol structure throughout
status: ready
depends: []
gates:
  - tests
  - commit

All shims should make symbol context prominent when serving from the DB.

**grep** — group matches by symbol with line ranges:

  ── src/main.rs ──

    Extractor::visit_expr_call (L770-L782)
      772 │ self.calls.push(CallRecord {

    Extractor::visit_expr_method_call (L784-L793)
      784 │ self.calls.push(CallRecord {

**cat/head/tail/sed** — insert symbol boundary dividers in the stream:

  ── struct CallRecord (L473-L479) ──
    473 │ struct CallRecord {
    474 │     caller_file: String,
    ...
    479 │ }
  ── fn insert_calls (L1008-L1024) ──
    1008 │ fn insert_calls(conn: &Connection, ...

Content is the same, but symbol boundaries are visible inline. The
agent sees a table of contents embedded in the code without needing
to query the symbols table separately.

Implementation: join against symbols table to get name, start_line,
end_line for the symbol containing each line. For grep, group and
deduplicate. For cat/head/tail/sed, emit a divider when the symbol
changes between consecutive lines. Lines with no symbol (top-level
code, blank lines between functions) get no divider.

Shared formatting logic in _indexnudge_common.bash.
