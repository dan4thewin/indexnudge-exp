# Bug: match_arms body_start points to wrong line

## Summary

The `body_start` column in the `match_arms` table sometimes points to
a line in the middle of the first expression within the arm body,
rather than to the first line after the `=> {`.

## Reproduction

```bash
cd ~/src/safebash

# Query the Bless arm in dispatch.rs (committed version):
sqlite3 .indexnudge-rust.db "
SELECT start_line, end_line, body_start, body_end 
FROM match_arms 
WHERE file='sash/src/codegen/dispatch.rs' AND pattern LIKE '%Bless%'"
```

Returns: `start_line=720, end_line=787, body_start=724, body_end=787`

But in the file:
```
720: } => {
721:     // Runtime bless enforcement...
722:     let saved = emit_redirects(
723:         redirects,
724:         builder,       ← body_start points HERE
```

Line 724 is the 3rd argument of a function call that started at line 722.
The correct `body_start` should be **721** — the first line of code
inside the `=> { ... }` block.

## Expected behavior

`body_start` should be the line immediately after the `=> {` opening
(or the same line if the arm uses `=> expr` without braces). For a
multi-line pattern like:

```rust
            SsaInst::Bless {
                source,
                target,
                test,
            } => {
                // first line of body   ← body_start should be here
```

`body_start` = the line with `// first line of body`.

## Impact

When extracting a match arm body using the index boundaries, the
extraction starts too deep and misses the initial lines. Or worse,
includes the `} => {` pattern-closing syntax as part of the body,
producing invalid Rust when pasted into a method.

## Likely cause

The tree-sitter `match_arm` node's `value` field (the body) probably
starts at the expression node rather than the opening `{` of the block.
If the body is a block expression, the `body_start` should be derived
from the block's first child statement, not the expression-level span.
