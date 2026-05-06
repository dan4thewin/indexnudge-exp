title: Shim selects correct language indexer
status: active
depends: []
gates:
  - tests
  - commit

## Problem

Shim calls whichever `indexnudge` is first on PATH. If the Rust indexer
runs against a Go project, it finds no .rs files, deletes all rows, and
renames back an empty DB.

## Solution (in progress)

Renamed DB files to `.indexnudge-go.db` / `.indexnudge-rust.db`. Each
indexer creates its own DB. `find_indexnudge_db` in the common bash
detects language from file extension and finds the matching DB.

## Status

- [x] Go indexer outputs `.indexnudge-go.db`
- [x] Rust indexer outputs `.indexnudge-rust.db`
- [x] `find_indexnudge_db` detects language, finds correct DB
- [x] `freshen_db` stubbed to no-op (safe, prevents contamination)
- [ ] `freshen_db` calls correct indexer based on DB language
- [ ] mkfile shim-test updated for new DB names
- [ ] Go/Rust tests pass with new names
- [ ] Goose tree patch updated for `.indexnudge-*.db` glob

See HANDOFF.md for current debugging state.
