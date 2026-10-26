#!/bin/sh
# Stop hook: run `cargo xtask verify --quick` before Claude may finish a turn.
# If it reports ok:false, block the stop with the failing steps as the reason.
#
# Blocking uses the Stop hook JSON form on stdout,
#   {"decision": "block", "reason": "..."}   (exit 0),
# rather than exit 2 + stderr, so the reason survives whatever the build log
# contains. Not blocking = exit 0 with no stdout.
#
# Skips when no *.rs / *.toml / *.json file under the workspace is newer than
# the stamp of the last verify run (target/claude-hooks/verify.stamp).
#
# Loop guard: Claude Code sets "stop_hook_active": true when it is already
# continuing because of a Stop hook. Then we re-verify only if files changed
# since the failed run (no new edits = the agent is reporting, let it stop),
# and never block more than NPC_VERIFY_MAX_BLOCKS (3) times in a row. Claude
# Code itself also caps consecutive Stop-hook continuations.
#
# POSIX sh (Git Bash on Windows, dash/bash on Linux); no jq: the verify JSON
# `{ok, steps:[{name, ok, ms, summary, log_tail}]}` is read with grep/awk.
#
# Env (users/tests; Claude cannot set a hook's environment):
#   NPC_VERIFY_HOOK=off      disable this hook.
#   NPC_VERIFY_CMD           command to run (default: cargo xtask verify --quick).
#   NPC_VERIFY_MAX_BLOCKS    consecutive blocks before giving up (default 3).
#   NPC_HOOK_ROOT            workspace root (default: two levels above here).
# Tests: xtask/tests/hooks.rs.

set -u

if [ -n "${NPC_HOOK_ROOT:-}" ]; then
    ROOT=$NPC_HOOK_ROOT
else
    ROOT=$(cd "$(dirname "$0")/../.." && pwd)
fi
STATE="$ROOT/target/claude-hooks"
STAMP="$STATE/verify.stamp"
BLOCKS="$STATE/blocks"
OUT="$STATE/last-verify.json"
ERR="$STATE/last-verify.stderr"
REASON="$STATE/reason.txt"
MAX_BLOCKS=${NPC_VERIFY_MAX_BLOCKS:-3}

INPUT=$(cat | tr '\r\n' '  ')

[ "${NPC_VERIFY_HOOK:-on}" = "off" ] && exit 0

ACTIVE=false
if printf '%s' "$INPUT" | grep -Eq -e '"stop_hook_active"[[:space:]]*:[[:space:]]*true'; then
    ACTIVE=true
fi

mkdir -p "$STATE" || exit 0

# Did any source/config file change since the last verify?
changed() {
    [ -f "$STAMP" ] || return 0
    first=$(find "$ROOT" \
        \( -path "$ROOT/target" -o -path "$ROOT/assets" -o -path "$ROOT/screenshots" \
           -o -path "$ROOT/.claude" -o -name .git -o -name node_modules \) -prune \
        -o -type f \( -name '*.rs' -o -name '*.toml' -o -name '*.json' \) -newer "$STAMP" -print 2>/dev/null | head -n 1)
    [ -n "$first" ]
}

# block: print $REASON as a Stop "block" decision (JSON-escaped) and exit.
block() {
    blocks=$((blocks + 1))
    echo "$blocks" > "$BLOCKS"
    awk 'BEGIN { ORS = ""; print "{\"decision\":\"block\",\"reason\":\"" }
         {
             gsub(/\\/, "\\\\"); gsub(/"/, "\\\""); gsub(/\t/, "    ")
             gsub(/[\001-\037\177]/, "")
             if (NR > 1) print "\\n"
             print
         }
         END { print "\"}\n" }' "$REASON"
    exit 0
}

if ! changed; then
    exit 0
fi

blocks=0
[ -f "$BLOCKS" ] && blocks=$(cat "$BLOCKS" 2>/dev/null || echo 0)
case "$blocks" in ''|*[!0-9]*) blocks=0 ;; esac
if [ "$ACTIVE" = true ] && [ "$blocks" -ge "$MAX_BLOCKS" ]; then
    echo 0 > "$BLOCKS"
    echo "verify_on_stop: still failing after $blocks blocked stops; letting the turn end." >&2
    exit 0
fi

if [ -n "${NPC_VERIFY_CMD:-}" ]; then
    CMD=$NPC_VERIFY_CMD
else
    # xtask not there yet (or no cargo): nothing to gate on.
    if [ ! -f "$ROOT/xtask/Cargo.toml" ] || ! command -v cargo >/dev/null 2>&1; then
        echo "verify_on_stop: cargo xtask not available, skipping." >&2
        exit 0
    fi
    CMD='cargo xtask verify --quick'
fi

# The stamp marks the START of this run, so edits made while it runs count
# as changes next time.
: > "$STAMP.new"
(cd "$ROOT" && eval "$CMD") > "$OUT" 2> "$ERR"
rc=$?
mv -f "$STAMP.new" "$STAMP"

JSON=$(tr '\r\n' '  ' < "$OUT")

if ! printf '%s' "$JSON" | grep -Eq -e '"ok"[[:space:]]*:[[:space:]]*(true|false)'; then
    # No verdict at all: xtask failed to build or crashed.
    {
        echo "\`$CMD\` produced no JSON verdict (exit $rc). Last stderr lines:"
        tail -n 30 "$ERR"
        echo "Fix the build, then finish. If this is not caused by your change, tell the user."
    } > "$REASON"
    block
fi

if ! printf '%s' "$JSON" | grep -Eq -e '"ok"[[:space:]]*:[[:space:]]*false'; then
    rm -f "$BLOCKS"
    exit 0
fi

{
    echo "\`$CMD\` reports ok:false. Failing steps:"
    # awk: plain [ ] classes (no [[:space:]]) for old mawk; newlines are already spaces.
    printf '%s\n' "$JSON" | awk '
        function strval(s, key,    re, v) {
            re = "\"" key "\"[ ]*:[ ]*\"([^\"\\\\]|\\\\.)*\""
            if (!match(s, re)) return ""
            v = substr(s, RSTART, RLENGTH)
            sub("^\"" key "\"[ ]*:[ ]*\"", "", v)
            v = substr(v, 1, length(v) - 1)
            gsub(/\\n/, "\n    ", v); gsub(/\\t/, " ", v); gsub(/\\"/, "\"", v); gsub(/\\\\/, "\\", v)
            return v
        }
        {
            s = $0
            while ((i = index(s, "\"name\"")) > 0) {
                s = substr(s, i)
                j = index(substr(s, 2), "\"name\"")
                chunk = (j > 0) ? substr(s, 1, j) : s
                s = (j > 0) ? substr(s, j + 1) : ""
                if (chunk !~ /"ok"[ ]*:[ ]*false/) continue
                printf "- %s: %s\n", strval(chunk, "name"), strval(chunk, "summary")
                tail = strval(chunk, "log_tail")
                if (length(tail) > 1500) tail = "..." substr(tail, length(tail) - 1500)
                if (tail != "") printf "    %s\n", tail
            }
        }'
    echo "Full verdict: target/claude-hooks/last-verify.json"
    echo "Fix the cause; do not edit goldens, snapshots or lint config to pass."
    echo "If the failure is pre-existing or a test looks wrong, say so to the user instead."
} > "$REASON"
block
