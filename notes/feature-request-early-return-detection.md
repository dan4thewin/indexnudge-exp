# Feature Request: early-return detection in match arms

## Problem

When extracting a match arm into a standalone function, the arm's
return type must match what it produces. Most arms in a match like:

```rust
let kind = match node.kind() {
    "word" => { ... }    // returns ExprKind
    "regex" => { ... }   // returns ExprKind
    ...
};
Expr { kind, span }
```

return `ExprKind` (the match expression's type). But some arms do:

```rust
"command_substitution" => {
    ...
    return Expr { kind: ..., span };  // early return from OUTER function
    ...
    ExprKind::CommandSubstitution(stmts)  // normal path
}
```

This `return` bypasses the match and returns from the enclosing function
with a different type (`Expr` instead of `ExprKind`). Extracting this
arm requires either:
- Changing the extracted function's return type to `Expr`
- Refactoring the early returns to not bypass the match

The index currently can't tell me "this arm contains an early return
from the enclosing function." I have to read the code, discover the
`return Expr {...}` after extraction fails to compile, then redesign.

## Proposed solution

Add a boolean column `has_early_return` to `match_arms`:

```sql
ALTER TABLE match_arms ADD COLUMN has_early_return INTEGER DEFAULT 0;
```

Set to 1 if the arm body contains a `return` statement (tree-sitter
`return_expression` node) that returns from the enclosing function,
not just from a closure within the arm.

## Query

```sql
-- Which arms are safe to extract without return-type changes?
SELECT pattern, start_line, end_line
FROM match_arms
WHERE file = 'sash/src/lower/expressions.rs'
AND parent_fn = 'lower_expr'
AND has_early_return = 0
AND (end_line - start_line) > 20;

-- Which arms need return-type refactoring before extraction?
SELECT pattern, start_line, end_line
FROM match_arms
WHERE file = 'sash/src/lower/expressions.rs'
AND parent_fn = 'lower_expr'
AND has_early_return = 1;
```

## Implementation

tree-sitter-rust parses `return` as `return_expression`. Walk each
match arm's body subtree; if any `return_expression` is found that
is NOT inside a closure (`closure_expression`), set `has_early_return=1`.

## Cost

Negligible — it's one extra boolean per match arm, computed during the
existing arm-extraction walk. No new table, no schema migration needed
beyond the column addition.
