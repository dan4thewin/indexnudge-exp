# Bug: freshen fails to detect file changes from `ed`

## Summary

`indexnudge --freshen` does not re-index files that were modified by
`ed` (or potentially other tools that preserve or round mtime). The
freshen logic compares `indexed_mtime` against the file's current
mtime, but when edits happen within the same second (or `ed`'s write
semantics don't bump mtime reliably), the comparison passes and the
stale index data persists.

## Reproduction

```bash
cd ~/src/safebash

# Index the file
indexnudge .

# Query — correct result
sqlite3 .indexnudge-rust.db "SELECT start_line FROM match_arms WHERE file='sash/src/codegen/dispatch.rs' AND pattern LIKE '%ReadLineMulti%'"
# Returns: 760

# Edit the file with ed (removes ~140 lines near the top)
ed -s sash/src/codegen/dispatch.rs << 'EDCMD'
276,417c
            SsaInst::Command { name, args, redirects, .. } if name == "exec" => {
                return self.emit_exec(name, args, redirects, builder, module, ctx);
            }
.
w
EDCMD

# ReadLineMulti is now at line 621 (shifted by ~139 lines)
grep -n "ReadLineMulti" sash/src/codegen/dispatch.rs
# Returns: 621

# Freshen — does NOT detect the change
indexnudge --freshen
# Output: (nothing, or "All files up to date")

# Query — WRONG result (stale)
sqlite3 .indexnudge-rust.db "SELECT start_line FROM match_arms WHERE file='sash/src/codegen/dispatch.rs' AND pattern LIKE '%ReadLineMulti%'"
# Returns: 760  ← WRONG, should be 621

# Even touch doesn't help:
touch sash/src/codegen/dispatch.rs
indexnudge --freshen
# Still returns 760

# Full reindex fixes it:
indexnudge .
sqlite3 .indexnudge-rust.db "SELECT start_line FROM match_arms ..."
# Returns: 621  ← correct
```

## Root cause

The `files.indexed_mtime` stored in the DB and the file's mtime match
within the comparison tolerance. Observed values:

```
File mtime (stat):     1778907532
DB indexed_mtime:      1778907532.9104223
```

The comparison likely truncates or rounds, making them appear equal.
When `ed` writes the file, it may:
1. Write within the same second as the last index
2. Use `rename()` semantics that preserve mtime
3. Set an mtime that rounds to the same value

## Impact

Any agent workflow that edits a file and then queries the index in the
same operation gets stale data. This caused repeated extraction failures
in a safebash refactoring session — the agent used boundary data from
before an edit to extract code from after the edit, getting wrong content.

## Suggested fix

Options (in order of robustness):

1. **Content hash** — store SHA256 of file content in `files` table.
   Freshen compares hash, not mtime. Guarantees detection regardless of
   timestamp semantics. Cost: one read per file per freshen.

2. **File size** — store byte count alongside mtime. If either changes,
   re-index. Catches all edits that change file length (nearly all of
   them). Near-zero cost.

3. **Force re-index on mtime equality with sub-second precision** —
   if the stored mtime matches to the second but the file's nanosecond
   component differs, re-index. May over-index but never misses.

4. **Always re-index if queried file was modified since last index
   (inotify/kqueue)** — platform-specific but eliminates the race entirely.

Option 2 (file size) is the simplest fix and would have caught every
failure in this session (all edits changed line count).
