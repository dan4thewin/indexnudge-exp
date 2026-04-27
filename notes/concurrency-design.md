# Concurrency Design

## Problem

The shims call `indexnudge --freshen` before every query. Multiple shims can
fire concurrently (parallel tool calls). The agent must never see corrupt or
mid-write data. Stale data during a failed freshen is also unacceptable —
the agent should fall back to filesystem reads rather than trust a stale index.

## Design

Two mechanisms, serving different purposes:

### 1. Rename protocol (failure safety)

Prevents stale reads after a failed freshen.

**Full reindex:**
1. If `.indexnudge.db` exists, rename to `.indexnudge.db.bak`
2. Build new index to `.indexnudge.db.tmp`
3. On success: rename `.tmp` → `.indexnudge.db`, delete `.bak`
4. On failure: `.tmp` is partial, `.bak` is last good. Canonical name absent — tree and shims fall back to filesystem.

**Freshen:**
1. Rename `.indexnudge.db` → `.indexnudge.db.bak`
2. Open `.bak`, `BEGIN IMMEDIATE`
3. Do all freshen work (delete stale rows, insert new lines/structure)
4. `COMMIT` → rename `.bak` → `.indexnudge.db`, delete nothing
5. On failure: `ROLLBACK`, `.bak` is untouched (pre-freshen state). Canonical name absent — fallback.

Parse errors are not failures — lines are always inserted, structure is dropped,
`parse_error = 1` set in files table. Freshen continues and commits normally.

Concurrent freshen: if canonical name is absent and `.bak` exists, another freshen
is in progress. Exit immediately, let the shim fall back.

### 2. SQLite locking (concurrent read/write safety)

Prevents mid-write reads during freshen.

SQLite lock escalation during `COMMIT` (rollback journal mode, not WAL):

1. `BEGIN IMMEDIATE` → acquires RESERVED lock. New readers still allowed.
2. Writer does INSERT/DELETE in memory.
3. `COMMIT` → RESERVED → PENDING (new readers blocked, `SQLITE_BUSY` returned
   immediately) → waits for existing SHARED locks to drain → EXCLUSIVE → writes
   to disk → releases all locks.

Source: https://www.sqlite.org/lockingv3.html

> "A PENDING lock means that the process holding the lock wants to write
> to the database as soon as possible and is just waiting on all current
> SHARED locks to clear so that it can get an EXCLUSIVE lock. No new
> SHARED locks are permitted against the database if a PENDING lock is active,
> though existing SHARED locks are allowed to continue."

> "Obtain a PENDING lock and then an EXCLUSIVE lock on the database file.
> If other processes still have SHARED locks, the writer might have to wait
> until those SHARED locks clear before it is able to obtain an EXCLUSIVE lock."

The PENDING state is implicit — part of the COMMIT path, not a separate
statement. Writers don't explicitly request it.

During freshen work (step 2), readers are still allowed and see pre-transaction
state. At commit, new readers are blocked (get `SQLITE_BUSY`), existing readers
drain, then the write completes. The commit window is tiny (disk flush of
changed pages).

**Reader-side handling:** shims should set `PRAGMA busy_timeout = 5000` so that
a reader blocked by PENDING retries automatically rather than failing immediately.
Freshen commits take milliseconds, so the retry succeeds almost immediately.

**WAL mode is incompatible with this design.** WAL allows readers to proceed
during writes — which means readers can see stale data during freshen.
Switch from `journal_mode=WAL` to `journal_mode=DELETE` (default rollback journal)
to get proper lock escalation semantics.

## Races considered

**Race 1: Reader open when freshen starts.** Rename changes directory entry but
reader's fd still references the inode. Reader finishes its query on old data.
Next reader opens new inode after commit+rename. Acceptable — the old query
was in flight before freshen, one query of slightly stale data.

**Race 2: Two concurrent freshens.** Both try to rename canonical to `.bak`.
Second one finds no canonical, checks for `.bak`, sees it exists, exits
immediately. First one completes normally.

**Race 3: Full reindex with reader mid-query.** Reader holds fd on old inode.
Full reindex creates new inode. Reader finishes on old data, fd closes, old inode
freed. Next query opens new DB. One stale query in flight — acceptable, same
as race 1.

## Schema changes required

- `PRAGMA journal_mode=DELETE` (remove WAL)
- `PRAGMA synchronous=NORMAL` (not OFF — need crash safety for rollback journal)
- `ALTER TABLE files ADD COLUMN parse_error INTEGER DEFAULT 0`

## Tree patch addition

Query `SELECT file FROM files WHERE parse_error = 1` and append note:

```
Note: 2 files have parse errors (lines available, symbols/calls missing):
  src/main.rs, src/lib.rs
```
