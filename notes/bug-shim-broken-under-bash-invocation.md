# Bug: `bash <shim-name>` silently breaks every shim

**Status: resolved (2026-05-31).** Fixed by switching `dirname "$0"`
to `dirname "${BASH_SOURCE[0]}"` in all six shims. Two regression
tests added to `mk shim-test`:

```
PASS: bash sqlite3 shim (rust)
PASS: bash sqlite3 shim (go)
```

## Reproduction (before fix)

```bash
cd ~/project
export PATH=/path/to/indexnudge/bin-rust:$PATH

bash sqlite3 .indexnudge-rust.db 'SELECT COUNT(*) FROM files;'
# /path/to/indexnudge/bin-rust/sqlite3: line 5:
#   /home/dangood/src/project/_indexnudge_common.bash: No such file or directory
# (no query result, no useful error to the agent)
```

Affected commands: `bash sqlite3 ...`, `bash cat ...`, `bash grep ...`,
`bash head ...`, `bash sed ...`, `bash tail ...`. All six shims had
the same bug.

## How an agent hit this

Two plausible paths:

1. **Explicit interpreter prefix.** Some agents reach for `bash <cmd>`
   when they want to be explicit about which shell, or as a defensive
   habit. Looks redundant but should work.

2. **Sandboxed bash wrapper.** Tools that proxy commands through
   `bash` (e.g., to apply environment overrides or capture output)
   end up invoking shims this way.

Either way, the result was silent failure with a cryptic error. The
agent saw "no such file or directory" mentioning an unfamiliar file,
got confused, and either gave up or routed around the shim.

## Root cause

All six shims started with:

```bash
SHIM_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SHIM_DIR/_indexnudge_common.bash"
```

When invoked as `sqlite3 ...` directly, `$0` is the path bash resolved
via `$PATH` — typically the absolute shim path. `dirname "$0"` is the
shim dir, source works.

When invoked as `bash sqlite3 ...`, bash still finds the shim via
`$PATH` (so the script *loads* correctly), but `$0` is set to the
bare argument as the user typed it — `"sqlite3"`. `dirname "sqlite3"`
is `"."`, and `cd "." && pwd` is the user's CWD. The source path then
points into the user's project directory, which doesn't have
`_indexnudge_common.bash`. `set -euo pipefail` aborts the script
before `exec sqlite3 "$@"` ever runs, so the query never executes.

## Fix

Use `${BASH_SOURCE[0]}` instead of `$0`. `BASH_SOURCE[0]` always
points to the actual file currently being executed regardless of
invocation style:

```bash
SHIM_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
```

Verified working across all four invocation styles:

- `sqlite3 ...` (direct)
- `bash sqlite3 ...` (bash + bare name, was broken)
- `bash -c "sqlite3 ..."` (bash subshell)
- `bash /abs/path/sqlite3 ...` (bash + absolute path)

## Why this matters beyond the immediate bug

The whole point of the shim infrastructure is "agents will reach for
sed/grep/sqlite3 in their existing habits; intercept transparently."
A silent failure on a minor invocation variant (`bash` prefix) defeats
that goal — agents can't tell whether their query ran, got no results,
or never executed at all. Adding the regression to `mk shim-test`
locks in coverage for the most common bash-prefix case.
