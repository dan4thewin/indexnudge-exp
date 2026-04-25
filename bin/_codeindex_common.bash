# Shared helpers for codeindex shims. Source this, don't execute it.
# Requires bash 4+ for associative arrays / local -n.

# Remove the shim directory from PATH so `command -v foo` and `exec foo`
# find the real tool, not ourselves.  Uses only bash builtins to avoid
# recursion (the shimmed grep/sed/etc. would re-source this file).
SHIM_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_new_path=""
IFS=: read -ra _path_parts <<< "$PATH"
for _p in "${_path_parts[@]}"; do
    [[ "$_p" == "$SHIM_DIR" ]] && continue
    _new_path="${_new_path:+${_new_path}:}${_p}"
done
PATH="$_new_path"
export PATH
unset _new_path _path_parts _p

# Walk up from a path to find .codeindex.db. Sets CODEINDEX_DB and CODEINDEX_DIR.
# Returns 1 if not found.
find_codeindex_db() {
    local start="$1"
    [[ -f "$start" ]] && start=$(dirname "$start")
    start=$(cd "$start" 2>/dev/null && pwd) || return 1
    local dir="$start"
    while true; do
        if [[ -f "$dir/.codeindex.db" ]]; then
            CODEINDEX_DB="$dir/.codeindex.db"
            CODEINDEX_DIR="$dir"
            return 0
        fi
        [[ "$dir" == "/" ]] && return 1
        dir=$(dirname "$dir")
    done
}

# Freshen the DB if found. No-op if not found or codeindex not on PATH.
freshen_db() {
    [[ -n "$CODEINDEX_DB" ]] || return 0
    command -v codeindex >/dev/null 2>&1 || return 0
    codeindex --freshen "$CODEINDEX_DIR" 2>/dev/null
}

# Resolve a potentially relative path to absolute.
resolve_path() {
    local f="$1"
    if [[ "$f" != /* ]]; then
        f="$PWD/$f"
    fi
    echo "$f"
}

# Get the relative path of a file within the codeindex dir.
# Assumes CODEINDEX_DIR is set.
rel_path() {
    local abs
    abs=$(resolve_path "$1")
    echo "${abs#"$CODEINDEX_DIR/"}"
}
