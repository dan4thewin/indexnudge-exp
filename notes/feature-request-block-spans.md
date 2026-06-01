# Feature Request: Block Spans and Match Arm Tracking

## Problem

When refactoring large Rust files — extracting match arms into methods,
splitting functions into modules — the code index provides symbol
locations (function start/end) and line content, but lacks the
structural information needed to safely extract code blocks.

Every extraction failure in a recent safebash refactoring session came
from one of these gaps:

1. **Getting the end of a match arm wrong** — capturing one `}` too
   many or too few, producing unbalanced braces in the extracted code.

2. **Not knowing what names a block references** — requiring manual
   grep to figure out which variables/functions from the surrounding
   scope the extracted block needs as parameters.

## What Would Make Extraction Mechanical

### 1. Brace-balanced block spans

Given any line containing an opening `{`, return the line containing
its matching `}`.

```sql
SELECT close_line FROM blocks
WHERE file = 'sash/src/codegen/dispatch.rs'
AND open_line = 282;
-- Returns: 416
```

This would make every extraction a guaranteed brace-balanced chunk.
No more counting braces manually or guessing where an arm ends.

### 2. Match arm boundaries

Track each arm of a `match` expression as a named span:

```sql
SELECT arm_pattern, start_line, end_line FROM match_arms
WHERE file = 'sash/src/codegen/dispatch.rs'
AND parent_function = 'emit'
ORDER BY start_line;
```

Returning:
```
SsaInst::Assign { target, value }          | 48  | 50
SsaInst::Command { .. } if name == "echo"  | 51  | 83
SsaInst::Command { .. } if name == "exec"  | 276 | 417
...
```

Each arm has: pattern text, body start line, body end line, total lines.
A refactoring agent can then say "extract the arm at line 276" and get
exact boundaries without parsing.

### 3. Free variable analysis (references from outside scope)

Given a line range, report which identifiers are used but not defined
within that range:

```sql
SELECT name, kind FROM free_vars
WHERE file = 'sash/src/codegen/dispatch.rs'
AND range_start = 282
AND range_end = 416;
```

Returning:
```
vars        | variable
literals    | variable
externs     | variable
builder     | parameter
module      | parameter
emit_redirects | function
lower_exec  | function
```

This tells the extractor exactly what the new function's parameters
need to be. No more "build, see what's missing, add it, rebuild".

### 4. Expression/statement spans

For multi-line expressions (function calls, struct literals, chained
method calls), provide the complete span:

```sql
SELECT start_line, end_line FROM expressions
WHERE file = 'sash/src/codegen/dispatch.rs'
AND start_line = 812;
-- Returns: 812, 822 (for a multi-line function call)
```

## Implementation Notes

### Source: tree-sitter AST

All of this information is available from a tree-sitter parse.
tree-sitter-rust provides exact byte ranges for every node:

- `match_expression` → contains `match_arm` children
- `match_arm` → has `pattern` and `value` (body) with exact spans
- `block` → `{` to matching `}` with exact span
- Identifier resolution needs scope analysis (harder, but even just
  "identifiers referenced in range" without resolution is useful)

### Schema additions

```sql
CREATE TABLE blocks (
    file    TEXT,
    open_line   INTEGER,
    close_line  INTEGER,
    kind        TEXT,  -- 'fn', 'impl', 'match', 'if', 'for', 'block'
    parent_symbol TEXT
);

CREATE TABLE match_arms (
    file        TEXT,
    parent_fn   TEXT,
    pattern     TEXT,
    start_line  INTEGER,
    end_line    INTEGER,
    body_start  INTEGER,
    body_end    INTEGER
);

CREATE TABLE references (
    file        TEXT,
    line_num    INTEGER,
    name        TEXT,
    kind        TEXT  -- 'variable', 'function', 'type', 'method'
);
```

### Incremental updates

Same as existing symbols — re-parse changed files, update spans.
tree-sitter is fast enough to parse on every query (a 6000-line
Rust file parses in ~5ms).

## Priority

Block spans (#1) and match arm tracking (#2) would eliminate 90% of
the extraction failures. Free variable analysis (#3) would eliminate
the remaining trial-and-error around parameter lists.

These would make "extract match arm N into a method" a single
deterministic operation instead of the error-prone guess-and-check
loop it is today.
