# Feature Request: AGENTS.md snippet should document `--freshen` for non-shim clients

## Problem

The "Freshness" subsection in all three AGENTS.md snippets
(`AGENTS.md.snippet`, `AGENTS.md.rust.snippet`, `AGENTS.md.go.snippet`)
says:

> If the `sqlite3` shim is on `PATH`, the database is re-freshened
> automatically before each query (only changed files are re-parsed).
> You don't need to re-run the indexer after editing source files.

That's accurate but incomplete. The shim only intercepts shell
invocations of `sqlite3` the binary. **Any process that opens the
SQLite DB via `libsqlite3` directly bypasses the shim and reads a
stale snapshot.** Concretely:

| Caller                                | Triggers shim? |
|---------------------------------------|----------------|
| `bash$ sqlite3 .indexnudge-rust.db`   | yes            |
| `python3 -c "import sqlite3; ..."`    | **no**         |
| Rust `rusqlite::Connection::open()`   | **no**         |
| Node `better-sqlite3`                 | **no**         |
| `sqlite-utils`, `litecli`, `dbeaver`  | **no**         |

Agents and helper scripts that drive the index from Python (or any
other language binding) silently get whatever was on disk at the
last shim invocation.

## How this bit a real agent

During a safebash refactor, I wrote a Python helper that:

1. Loaded `.cyclo-allow` entries
2. Queried `symbols.start_line` for each whitelisted function
3. Inserted a `// cyclo-allow:` comment line above each fn

The script worked correctly the first time. When I re-ran it to
verify idempotency, the queries returned the **pre-insertion** line
numbers — even though several files had been written 30 seconds
earlier. Python's `sqlite3` module never triggered the shim, so
`indexed_mtime` in the `files` table was never re-checked. The
script's diagnostics fired correctly (sanity check on `text[idx]`),
but the warnings looked confusing because nothing in the snippet
told me a non-shim caller would see stale data.

## Proposed snippet change

Replace the Freshness subsection with something like:

```markdown
### Freshness

The database is re-freshened on-demand by the `sqlite3` shim in
`indexnudge/bin/`. If that shim is first on `PATH`, every shell
invocation of `sqlite3` re-parses changed files before querying.

**Non-shim callers bypass this.** Anything that links `libsqlite3`
directly — Python's `sqlite3` module, Rust `rusqlite`, Node
`better-sqlite3`, `sqlite-utils`, `litecli`, GUI browsers — opens
the DB file directly and sees whatever snapshot the last shim
invocation left behind.

When you drive the index from a script, run `indexnudge --freshen`
explicitly first:

```bash
indexnudge --freshen .   # incremental: only re-parses changed files
python3 my_helper.py     # now sees current state
```

Or wrap the shell call inside the script (`subprocess.run(["sqlite3", ...])`)
so the shim handles it. Within a single script run, one freshen at
the top is enough — line numbers stay stable until the script itself
mutates files.
```

The key adds are:

1. The explicit caller table so people recognize their situation.
2. The `indexnudge --freshen` recipe — currently undocumented in the
   snippet even though the binary supports it.
3. The "one freshen at the top is enough" guidance so people don't
   over-refresh inside hot loops.

## Related

- `notes/bug-freshen-mtime.md` covers the orthogonal case where
  `--freshen` itself misses `ed` edits within the same second.
  Combined with this gap, a Python script editing files via `ed`
  has two layers of staleness to defeat.

## Scope

Snippet-only change. No code changes to indexnudge itself. Files to
update:

- `AGENTS.md.snippet`
- `AGENTS.md.rust.snippet`
- `AGENTS.md.go.snippet`

All three currently share the same Freshness wording verbatim.
