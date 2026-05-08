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

# Get the next rotating hint. Increments counter in the DB.
# Usage: hint=$(get_hint "$rel")  — pass the current file for contextual hints.
get_hint() {
    local rel="${1:-}"
    local file_clause=""
    [[ -n "$rel" ]] && file_clause="'$rel'"

    # Ensure hints table exists and atomically increment counter
    local n
    n=$(sqlite3 -cmd '.timeout 5000' "$CODEINDEX_DB" "
        CREATE TABLE IF NOT EXISTS hint_state (key TEXT PRIMARY KEY, value INTEGER);
        INSERT INTO hint_state (key, value) VALUES ('counter', 0)
            ON CONFLICT(key) DO UPDATE SET value = value + 1;
        SELECT value FROM hint_state WHERE key = 'counter';
    " 2>/dev/null) || return 1

    local db_name
    db_name=$(basename "$CODEINDEX_DB")

    case $(( n % 6 )) in
        0) echo "── tip: sqlite3 $db_name \"SELECT file, lines FROM files;\" ──" ;;
        1) if [[ -n "$rel" ]]; then
               echo "── tip: sqlite3 $db_name \"SELECT name, kind, start_line, end_line FROM symbols WHERE file='$rel';\" ──"
           else
               echo "── tip: sqlite3 $db_name \"SELECT name, kind, start_line, end_line FROM symbols WHERE file='...';\" ──"
           fi ;;
        2) echo "── tip: sqlite3 $db_name \"SELECT caller, callee_qname FROM calls WHERE file='${rel:-...}';\" — trace call graph ──" ;;
        3) echo "── tip: sqlite3 $db_name \"SELECT module, name FROM imports WHERE file='${rel:-...}';\" — see dependencies ──" ;;
        4) echo "── tip: ed file <<'EOF' — use DB line numbers for precise addressing ──" ;;
        5) if [[ -n "$rel" ]]; then
               echo "── tip: sqlite3 $db_name \"SELECT name, kind, start_line, end_line FROM symbols WHERE file='$rel';\" ──"
           else
               echo "── tip: sqlite3 $db_name \"SELECT name, kind, start_line, end_line FROM symbols WHERE file='...';\" ──"
           fi ;;
    esac
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
    if [[ "$abs" == "$CODEINDEX_DIR" ]]; then
        echo "."
    else
        echo "${abs#"$CODEINDEX_DIR/"}"
    fi
}

