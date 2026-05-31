# Feature Request: Extract string literals as a queryable column / table

## Problem

The index knows about *symbol* references — `calls`, `refs`,
`defs` — but treats string literals as opaque text inside `lines`.
That makes a very common refactor pattern invisible to SQL:

> "Find every site that dispatches on the string `"echo"`."

Today the only answer is `lines.content LIKE '%"echo"%'`, which:

1. matches the literal in code, comments, doc strings, and macros
   indiscriminately,
2. can't distinguish `if name == "echo"` from `match s { "echo" => ... }`
   from `format!("echo {}", x)`,
3. has no way to ask "give me all functions whose body mentions
   any of {`"echo"`, `"true"`, `"false"`, `":"`}".

## How this bit a real agent

During the BuiltinCmd refactor in safebash I needed to:

1. Find every callsite that hardcoded a builtin name (`echo`, `true`,
   `false`, `:`, `printf`, `exec`, `trap`, `read`, `bless`, `getenv`,
   `argv`) and route it through a new `BuiltinCmd::from_name` enum.
2. Make sure no string-keyed dispatch survived after the refactor.

Step 1 wanted "give me every function whose body contains `==
"echo"` or matches `"echo"` as a pattern." I ended up running ten
separate `grep` invocations across the codebase, deduping by hand,
and then re-grepping a second time because `format!("...echo...")`
calls polluted the results.

Step 2 wanted "after the refactor, are there any remaining string
literals from {echo, true, false, ...} outside of `BuiltinCmd::from_name`
and `BuiltinCmd::sigil_str`?" That should be one SQL query. It
wasn't — it was twenty greps and a spreadsheet.

## Proposed schema

A new table populated from existing tree-sitter walks:

```sql
CREATE TABLE string_literals (
    file        TEXT,
    line_num    INTEGER,
    symbol      TEXT,         -- enclosing function (NULL for module-level)
    value       TEXT,         -- the literal value, unescaped
    context     TEXT          -- 'compare' | 'match_pat' | 'fmt' | 'arg' | 'macro' | 'other'
);
CREATE INDEX idx_strlit_value ON string_literals(value);
CREATE INDEX idx_strlit_file_line ON string_literals(file, line_num);
CREATE INDEX idx_strlit_symbol ON string_literals(symbol);
```

`context` is the small useful win — it captures the syntactic role
of the literal so the agent can filter out noise:

| node                          | context     |
|-------------------------------|-------------|
| `binary_expression (== / !=)` | `compare`   |
| `match_arm.pattern`           | `match_pat` |
| `format!`, `println!`, etc.   | `fmt`       |
| function call argument        | `arg`       |
| macro invocation (non-fmt)    | `macro`     |
| anything else                 | `other`     |

For the BuiltinCmd refactor that becomes:

```sql
-- Step 1: find every dispatch site
SELECT symbol, file, line_num, value
FROM string_literals
WHERE value IN ('echo','true','false',':','printf','exec','trap',
                'read','bless','getenv','argv')
  AND context IN ('compare','match_pat')
ORDER BY symbol;

-- Step 2: confirm no surviving string-keyed dispatch outside the enum
SELECT *
FROM string_literals
WHERE value IN ('echo','true','false',':','printf','exec','trap',
                'read','bless','getenv','argv')
  AND context IN ('compare','match_pat')
  AND symbol NOT IN ('BuiltinCmd::from_name','BuiltinCmd::sigil_str');
```

## Why a separate table (not a `lines.literals` column)

- A line can hold multiple literals (`if name == "foo" || name == "bar"`).
- A literal can span multiple lines (Rust raw / multiline strings).
- The `context` filter is the highest-leverage piece, and a column
  with TSV values would defeat indexing.

## Performance

Tree-sitter already visits every `string_literal` and
`raw_string_literal` node while building `lines` and `refs`. The
new table adds one row per literal — for the safebash crate
that's ~3.5k rows (estimated from `grep -c '"' sash/src/**/*.rs`),
~150 KB on disk including indexes. Negligible.

## Out of scope

- Numeric literals. (Rarely dispatched on; can be done later.)
- Char literals. (Same.)
- Interpolated parts of format strings.

## Why this belongs in indexnudge

The schema already commits to "symbol-aware view of the source" with
`refs` and `calls`. Treating string literals as a first-class
reference kind is the natural extension. Without it, every project
that does string-keyed dispatch (parsers, CLI tools, web routers,
compilers) keeps falling back to grep for the most important
refactor question they could ask.
