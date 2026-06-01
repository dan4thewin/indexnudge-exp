# Bug: freshen stores indexed_size=0 after re-indexing a file

## Summary

When freshen detects a changed file and re-indexes it, the resulting
`indexed_size` in the `files` table is set to **0** instead of the
file's actual byte count. This causes subsequent freshens to always
re-index the file (since `disk_size != 0` is always true), defeating
the mtime+size caching.

More critically: if the freshen somehow doesn't re-trigger on the next
call (e.g., mtime matches), the `indexed_size=0` means the next edit
that happens in the same second as an index won't be detected — the
exact scenario the size check was meant to fix.

## Reproduction

```bash
cd ~/src/safebash

# Full reindex (works correctly, stores real size)
indexnudge .
sqlite3 .indexnudge-rust.db "SELECT indexed_size FROM files WHERE file='sash/src/codegen/dispatch.rs'"
# Returns: 44278 (correct)

# Edit a file
ed -s sash/src/codegen/dispatch.rs << 'EDCMD'
1a
// test
.
w
EDCMD

# Query (triggers freshen via shim)
sqlite3 .indexnudge-rust.db "SELECT indexed_size FROM files WHERE file='sash/src/codegen/dispatch.rs'"
# Returns: 0  ← BUG: should be 44286 (the new file size)
```

## Root cause

The freshen code path calls a different index function (or the same
function with a different code path) that doesn't store
`indexed_size`. Likely the `UPDATE files SET indexed_mtime=? WHERE ...`
statement in the freshen path doesn't include `indexed_size`, or
the size value isn't computed/passed at that point.

## Fix

The freshen index path should `stat()` the file and store both
`indexed_mtime` and `indexed_size` in the same UPDATE statement
that marks the file as freshly indexed.
