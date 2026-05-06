# Shared helpers for indexnudge shims. Source this, don't execute it.
# Requires bash 4+ for associative arrays / local -n.

# Remove the shim directory from PATH so `command -v foo` and `exec foo`
# find the real tool, not ourselves.  Uses only bash builtins to avoid
# recursion (the shimmed grep/sed/etc. would re-source this file).
shopt -s nullglob
_new_path=""
IFS=: read -ra _path_parts <<< "$PATH"
for _p in "${_path_parts[@]}"; do
    f=("$_p"/*indexnudge*)
    (( ${#f[*]} > 0 )) && continue
    _new_path="${_new_path:+${_new_path}:}${_p}"
done
PATH="$_new_path"
export PATH
unset _new_path _path_parts _p f

# Walk up from a path to find .indexnudge-<lang>.db. Sets CODEINDEX_DB and CODEINDEX_DIR.
# Detects language from file extension. Returns 1 if not found.
find_indexnudge_db() {
    local start="$1"
    # Detect language from file extension
    local lang=""
    case "$start" in
        *.go)   lang="go" ;;
        *.rs)   lang="rust" ;;
        *)      lang="" ;;
    esac
    [[ -f "$start" ]] && start=$(dirname "$start")
    start=$(cd "$start" 2>/dev/null && pwd) || return 1
    local dir="$start"
    while true; do
        if [[ -n "$lang" && -f "$dir/.indexnudge-${lang}.db" ]]; then
            CODEINDEX_DB="$dir/.indexnudge-${lang}.db"
            CODEINDEX_DIR="$dir"
            return 0
        fi
        # Fallback: try any .indexnudge-*.db
        if [[ -z "$lang" ]]; then
            for db in "$dir"/.indexnudge-*.db; do
                if [[ -f "$db" ]]; then
                    CODEINDEX_DB="$db"
                    CODEINDEX_DIR="$dir"
                    return 0
                fi
            done
        fi
        [[ "$dir" == "/" ]] && return 1
        dir=$(dirname "$dir")
    done
}

# Freshen the DB if found. No-op if not found or indexnudge not on PATH.
freshen_db() {
    :
}

# Resolve a potentially relative path to absolute.
resolve_path() {
    local f="$1"
    if [[ "$f" != /* ]]; then
        f="$PWD/$f"
    fi
    echo "$f"
}

# Get the relative path of a file within the indexnudge dir.
# Assumes CODEINDEX_DIR is set.
rel_path() {
    local abs
    abs=$(resolve_path "$1")
    echo "${abs#"$CODEINDEX_DIR/"}"
}

