title: Shim output uses symbol structure throughout
status: active
depends:
  - shim-indexer-selection
gates:
  - tests
  - commit
  - benchmark

## Goal

Shim output (grep, cat, head, tail, sed) groups/divides results by symbol
context from the DB. The agent sees function structure without querying
the symbols table explicitly.

## grep — grouped by symbol

Output format:
```
── file.rs ──

  function_name (L100-L120)
    105 │ matched line content
    110 │ another match

  other_function (L200-L250)
    220 │ match here
```

Implementation: single SQL JOIN query, no shell loops or awk.

## cat/head/tail/sed — symbol dividers

Insert symbol boundary headers in sequential file output:
```
  ── fn insert_calls (L1008-L1024) ──
    1008 │ fn insert_calls(conn: &Connection, ...
```

## Status

- [x] grep shim rewritten with SQL JOIN (pure SQL, no awk)
- [x] Tested: produces grouped output correctly
- [x] Fallthrough: non-indexed files fall through to real grep silently
- [ ] Intermittent hang under investigation (see HANDOFF.md)
- [ ] cat/head/tail/sed dividers not yet implemented
- [ ] Benchmark re-run after shim changes
