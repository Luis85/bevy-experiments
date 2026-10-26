#!/bin/sh
# PreToolUse hook: block edits to oracles (goldens, replays, baselines, lint
# config, verify/CI definitions, the hooks themselves) unless the user
# approved a re-baseline.
#
# Wired in .claude/settings.json for Edit|Write|MultiEdit|NotebookEdit (checks
# tool_input.file_path / notebook_path) and for Bash|PowerShell (best-effort
# check of tool_input.command for writes to protected paths).
#
# Contract (Claude Code hooks): hook JSON arrives on stdin; exit 0 = allow,
# exit 2 = block, and stderr is shown to Claude as the reason.
#
# POSIX sh only (Git Bash on Windows, dash/bash on Linux). No jq: the few
# fields we need are pulled out of the JSON with sed. A JSON string value
# never contains a raw `"`, so `"key": "value"` can be matched textually.
#
# Approval: the USER creates `.claude/rebaseline-approved` (gitignored) and
# deletes it afterwards. The agent may never create, edit or delete that
# marker, nor the Stop hook's state in target/claude-hooks/ (always blocked).
#
# Path forms folded before matching (file tools and command targets): `\` and
# `/`, case, `.` and `..`, a trailing dot or space on any component (Windows
# drops them), `\\?\` and `\\.\` prefixes, MSYS `/c/`, NTFS streams
# (`a.json::$DATA`), and 8.3 short names (`cygpath -l` when available, plus a
# name heuristic for the protected names: `SNAPSH~1`, `BASELI~1.JSO`, ..).
#
# Shell commands (best effort, not a sandbox): the command is split into
# segments (`;`, `&&`, `||`, `|`, `&`, parentheses, braces, backticks), `cd`/
# `pushd`/`Set-Location` are followed for relative paths, `sh -c`/`pwsh
# -Command`/`cmd /c` and `sudo`/`env` wrappers are looked through, and only
# the paths a command *writes* are checked:
# - delete/move verbs (rm, rmdir, unlink, del, erase, rd, mv, move,
#   Remove-Item/ri, Move-Item/mi, Rename-Item/rni, ren) on a protected path
#   or on a directory that contains one (`rm -rf tests`, `rm -r .claude`);
# - write verbs (touch, truncate, chmod, tee, mkdir, New-Item/ni,
#   Set-Content/sc, Add-Content/ac, Out-File, Clear-Content, `sed -i`,
#   `perl -i`, `dd of=`) and redirects (`>`, `>>`);
# - copy verbs (cp, copy, Copy-Item/cpi, install, rsync, scp, xcopy,
#   robocopy, ln): only the destination (copying FROM an oracle is fine);
# - git checkout|restore|reset|stash|apply|am|clean|mv|rm|switch|revert|
#   cherry-pick|merge|pull naming a protected path, and the files a
#   `git apply`/`git am`/`patch` patch touches (read from the patch);
# - `find .. -delete` / `-exec`, `xargs <verb>` (the paths piped in);
# - `[IO.File]::Write*|Append*|Delete|Copy|Move|Replace|Create|Open*` and
#   `python -c`/`py -c`/`node -e`/`perl -e` with a write mode or a
#   delete/rename call next to a protected path;
# - npcctl's `--out`/`--record`, `cargo insta accept|review`,
#   `INSTA_UPDATE=always|force`, `INSTA_FORCE_PASS`.
#
# Env (tests only; Claude cannot set a hook's environment):
#   NPC_HOOK_ROOT  workspace root (default: two levels above this script).
# Tests: xtask/tests/hooks.rs. Rationale: CLAUDE.md "Golden rules" and
# docs/research/agent-driven-gamedev.md §1.7.

set -u

# This hook runs before every edit and shell command, and process spawns are
# slow in Git Bash, so: a builtin-only prefilter first, and each helper below
# is a single sed/grep/awk.
INPUT=$(tr '\r\n' '  ')

# Prefilter: no protected word, short name, delete/patch verb anywhere in the
# payload -> allow at once.
case "$INPUT" in
    *[Rr][Ee][Pp][Ll][Aa][Yy][Ss]*|*[Gg][Oo][Ll][Dd][Ee][Nn]*|*[Ss][Nn][Aa][Pp]*|\
    *[Bb][Aa][Ss][Ee][Ll][Ii][Nn][Ee]*|*[Cc][Ll][Ii][Pp][Pp][Yy]*|*[Dd][Ee][Nn][Yy]*|\
    *[Aa][Rr][Cc][Hh]_[Ff][Ii][Tt][Nn][Ee][Ss][Ss]*|*[Hh][Oo][Oo][Kk][Ss]*|\
    *[Ss][Ee][Tt][Tt][Ii][Nn][Gg][Ss]*|*[Ii][Nn][Ss][Tt][Aa]*|*[Mm][Aa][Ii][Nn].[Rr][Ss]*|\
    *[Ww][Oo][Rr][Kk][Ff][Ll][Oo][Ww]*|*[Rr][Ee][Bb][Aa][Ss][Ee]*|*~[0-9]*) ;;
    *'"tool_name":"Bash"'*|*'"tool_name": "Bash"'*|*'"tool_name":"PowerShell"'*|*'"tool_name": "PowerShell"'*)
        # Shell tools: delete/move/patch verbs can hit an oracle by its
        # directory or through a patch file, without naming it.
        case "$INPUT" in
            *[Rr][Mm]*|*[Dd][Ee][Ll]*|*[Rr][Dd]\ *|*[Rr][Ii]\ *|*[Ee][Rr][Aa][Ss][Ee]*|\
            *[Mm][Vv]\ *|*[Mm][Oo][Vv][Ee]*|*[Mm][Ii]\ *|*[Rr][Ee][Nn]*|*[Aa][Pp][Pp][Ll][Yy]*|\
            *[Pp][Aa][Tt][Cc][Hh]*|*[Gg][Ii][Tt]\ *[Aa][Mm]*|*[Ss][Tt][Aa][Ss][Hh]*|\
            *[Cc][Hh][Ee][Cc][Kk][Oo][Uu][Tt]*|*[Rr][Ee][Ss][Tt][Oo][Rr][Ee]*) ;;
            *) exit 0 ;;
        esac
        ;;
    *) exit 0 ;;
esac

if [ -n "${NPC_HOOK_ROOT:-}" ]; then
    ROOT=$NPC_HOOK_ROOT
else
    case "$0" in
        *\\*) ROOT=$(cd "$(dirname "$0")/../.." && pwd) ;; # Windows path
        */*) ROOT=$(cd "${0%/*}/../.." && pwd) ;;
        *) ROOT=$(cd ../.. && pwd) ;;
    esac
fi
MARKER="$ROOT/.claude/rebaseline-approved"

LOWER='y/ABCDEFGHIJKLMNOPQRSTUVWXYZ/abcdefghijklmnopqrstuvwxyz/'

# Protected oracles, as an ERE over a normalized path (lower case, `/`).
# `(^|/)` anchors a component start; the CI workflow lives one level above
# the root (`../.github/..`), so it is matched by its own name.
PROT='(^|/)tests/(replays|golden)(/|$)|(^|/)snapshots(/|$)|\.snap$|\.proptest-regressions$|(^|/)bench/baseline\.json$|(^|/)\.?clippy\.toml$|(^|/)deny\.toml$|(^|/)xtask/tests/arch_fitness\.rs$|(^|/)xtask/src/main\.rs$|(^|/)\.claude/hooks(/|$)|(^|/)\.claude/settings(\.local)?\.json$|(^|/)\.github/workflows/npc-framework\.yml$'
# Never writable by the agent, even with the marker.
ALWAYS='(^|/)\.claude/rebaseline-approved$|(^|/)target/claude-hooks(/|$)'
# Unanchored forms of PROT for free text (a command line, a script).
PROT_TEXT='tests/(replays|golden)|snapshots|\.snap([^a-z.]|$)|\.proptest-regressions|bench/baseline\.json|clippy\.toml|deny\.toml|xtask/tests/arch_fitness\.rs|xtask/src/main\.rs|\.claude/hooks|\.claude/settings(\.local)?\.json|\.github/workflows/npc-framework\.yml'

# json_str KEY: the string value of "KEY" in $INPUT (JSON escapes kept).
json_str() {
    printf '%s\n' "$INPUT" | sed -n -E "s/.*\"$1\"[[:space:]]*:[[:space:]]*\"(([^\"\\\\]|\\\\.)*)\".*/\\1/p"
}

# norm: paths on stdin, one per line -> JSON-unescaped, forward slashes,
# `\\?\` / `\\.\` dropped, /c/ -> c:/, `.` and `..` folded, trailing dots and
# spaces dropped from every component, an NTFS stream suffix dropped, lower
# case, protected 8.3 short names expanded.
NORM_SED='s#\\\\#/#g
s#\\#/#g
s#^//[?.]/##
s#^/([A-Za-z])/#\1:/#
s#/+#/#g
s#/(\./)+#/#g
s#^\./##
s#/\.$##
:a
s#(^|/)[^/]+/\.\./#\1#
ta
s#([^/]*[^/. ])[. ]+(/|$)#\1\2#g
s#(/[^/:]+):[^/]*$#\1#
s#^([^/:][^/:]+):[^/]*$#\1#
y/ABCDEFGHIJKLMNOPQRSTUVWXYZ/abcdefghijklmnopqrstuvwxyz/
s#(^|/)snapsh~[0-9]+(/|$)#\1snapshots\2#g
s#(^|/)replay~[0-9]+(/|$)#\1replays\2#g
s#(^|/)baseli~[0-9]+(\.jso?n?)?$#\1baseline.json#
s#(^|/)clippy~[0-9]+(\.tom?l?)?$#\1clippy.toml#
s#(^|/)deny~[0-9]+(\.tom?l?)?$#\1deny.toml#
s#(^|/)arch_f~[0-9]+(\.rs)?$#\1arch_fitness.rs#
s#(^|/)settin~[0-9]+(\.jso?n?)?$#\1settings.json#
s#(^|/)rebase~[0-9]+$#\1rebaseline-approved#
s#(^|/)claude~[0-9]+(/|$)#\1.claude\2#g
s#(^|/)github~[0-9]+(/|$)#\1.github\2#g
s#(^|/)workfl~[0-9]+(/|$)#\1workflows\2#g
s#(^|/)npc-fr~[0-9]+\.yml$#\1npc-framework.yml#'

norm() {
    printf '%s\n' "$1" | sed -E "$NORM_SED"
}

# long PATH: the long form of a path with 8.3 components (`TESTS~1`), via
# cygpath when available (Git Bash): the longest existing prefix is
# expanded (a new file's directory exists, the file does not yet).
long() {
    p=$1
    if ! command -v cygpath >/dev/null 2>&1; then
        printf '%s\n' "$p"
        return
    fi
    rest=
    while [ -n "$p" ] && [ ! -e "$p" ]; do
        case "$p" in */*) ;; *) break ;; esac
        rest="/${p##*/}$rest"
        p=${p%/*}
    done
    if [ -n "$p" ] && [ -e "$p" ] && l=$(cygpath -l -m "$p" 2>/dev/null); then
        printf '%s%s\n' "$l" "$rest"
    else
        printf '%s%s\n' "$p" "$rest"
    fi
}

block() {
    {
        printf 'BLOCKED by .claude/hooks/protect_goldens.sh: %s\n' "$1"
        printf 'This is a protected oracle (golden, snapshot, replay, baseline, lint config,\n'
        printf 'verify/CI definition or hook).\n'
        printf 'Do NOT change it to make a check pass. If a test or golden looks wrong, or a\n'
        printf 're-baseline is really needed, stop and ask the user, with what changed and why.\n'
        printf 'Only the user can approve, by creating .claude/rebaseline-approved.\n'
    } >&2
    exit 2
}

block_always() {
    {
        printf 'BLOCKED by .claude/hooks/protect_goldens.sh: %s\n' "$1"
        printf 'Only the user may create or remove the re-baseline marker or the hook state.\n'
        printf 'Ask the user to approve the re-baseline instead.\n'
    } >&2
    exit 2
}

# check_path PATH: block if PATH (absolute, or relative to ROOT) is protected.
# Paths outside the workspace are not this hook's business, except the CI
# workflow next to it (`../.github/workflows/npc-framework.yml`).
check_path() {
    rootn=$(norm "$ROOT")
    rootn=${rootn%/}
    n=$(norm "$1")
    # Short and long forms on either side: compare long forms.
    case "$n$rootn" in
        *~[0-9]*)
            n=$(norm "$(long "$n")")
            rootn=$(norm "$(long "$rootn")")
            rootn=${rootn%/}
            ;;
    esac
    case "$n" in
        /*|[a-z]:/*) ;;
        *) n="$rootn/$n" ;;
    esac
    n=$(norm "$n")
    case "$n" in
        "${rootn%/*}/.github/workflows/npc-framework.yml")
            [ -f "$MARKER" ] && return 0
            block "../.github/workflows/npc-framework.yml"
            ;;
    esac
    case "$n" in
        "$rootn"/*) r=${n#"$rootn"/} ;;
        *) return 0 ;;
    esac
    # Never writable by the agent, even with the marker.
    case "$r" in
        .claude/rebaseline-approved|target/claude-hooks|target/claude-hooks/*) block_always "$r" ;;
    esac
    if printf '%s\n' "$r" | grep -Eq "$PROT"; then
        [ -f "$MARKER" ] && return 0
        block "$r"
    fi
    return 0
}

# The command analysis: lower-cased command on stdin, one line per written
# path on stdout; a path deleted or moved away is preceded by a line `@D@`.
TARGETS_AWK='
function isabs(p) { return p ~ /^\// || p ~ /^[a-z]:\// }
function join(d, p) { if (d == "" || isabs(p)) return p; return d "/" p }
function emit(kind, p) {
    if (p == "" || p ~ /^-/) return
    sub(/^:(\([^)]*\))?\/?/, "", p)
    if (p == "*" || p == "./" || p == "./*") p = "."
    sub(/\/\*$/, "", p)
    if (kind == "D") print "@D@"
    print join(cwd, p)
}
function scanpatch(f,   line, n, parts, k, p) {
    f = join(cwd, f)
    while ((getline line < f) > 0) {
        if (line ~ /^(\+\+\+|---) / || line ~ /^(rename|copy) (from|to) / || line ~ /^diff --git /) {
            sub(/^(\+\+\+ |--- |rename from |rename to |copy from |copy to |diff --git )/, "", line)
            sub(/\t.*/, "", line)
            n = split(line, parts, " ")
            for (k = 1; k <= n; k++) {
                p = parts[k]
                sub(/^[ab]\//, "", p)
                if (p != "/dev/null") print tolower(p)
            }
        }
    }
    close(f)
}
BEGIN { q = sprintf("%c", 39); cwd = "" }
{
    s = $0
    gsub(/>&/, ">@", s)
    gsub(/&>/, "@>", s)
    nseg = split(s, seg, /(\|\||&&|[;&|(){}`])/)
    prevn = 0
    for (i = 1; i <= nseg; i++) {
        line = seg[i]
        gsub("[\"" q "]", "", line)
        nt = split(line, tok, /[ \t]+/)
        m = 0
        delete a
        for (j = 1; j <= nt; j++) if (tok[j] != "") a[++m] = tok[j]
        # Redirects anywhere in the segment.
        for (j = 1; j <= m; j++) {
            t = a[j]
            if ((t == ">" || t == ">>" || t == ">|") && j < m) emit("W", a[j + 1])
            else if (t ~ /^[0-9]*>>?[^>@&]/) { sub(/^[0-9]*>>?/, "", t); emit("W", t) }
        }
        k = 1
        while (k <= m) {
            w = a[k]
            if (w ~ /^[a-z_][a-z0-9_]*=/ || w ~ /^(sudo|env|command|builtin|exec|nohup|time|then|do|else|!|&|\.|call|start)$/) { k++; continue }
            if (w ~ /^(bash|sh|zsh|dash|pwsh|powershell|cmd)(\.exe)?$/) {
                k++
                while (k <= m && a[k] ~ /^[-\/]/) k++
                continue
            }
            break
        }
        if (k > m) { prevn = 0; continue }
        verb = a[k]
        sub(/\.exe$/, "", verb)
        sub(/^.*\//, "", verb)
        # `xargs VERB`: the paths come from the previous segment.
        if (verb == "xargs") {
            k++
            while (k <= m && a[k] ~ /^-/) k++
            if (k > m) { prevn = 0; continue }
            verb = a[k]
            sub(/^.*\//, "", verb)
            if (verb ~ /^(rm|rmdir|unlink|shred|del|mv|cp|touch|chmod|tee|truncate)$/)
                for (j = 1; j <= prevn; j++) emit(verb == "cp" ? "W" : "D", prev[j])
        }
        if (verb ~ /^(cd|pushd|chdir|set-location|sl)$/) {
            d = ""
            for (j = k + 1; j <= m; j++) if (a[j] !~ /^-/) { d = a[j]; break }
            if (d == "" || d == "~" || d == "-") cwd = ""
            else cwd = join(cwd, d)
        } else if (verb == "popd") {
            cwd = ""
        } else if (verb ~ /^(rm|rmdir|unlink|shred|del|erase|rd|ri|remove-item|mv|move|move-item|mi|rename-item|rni|ren|rename)$/) {
            for (j = k + 1; j <= m; j++) emit("D", a[j])
        } else if (verb ~ /^(touch|truncate|chmod|chown|tee|new-item|ni|set-content|sc|add-content|ac|out-file|clear-content|clc|mkdir|md)$/) {
            for (j = k + 1; j <= m; j++) emit("W", a[j])
        } else if (verb == "sed" || verb == "perl") {
            inplace = 0
            for (j = k + 1; j <= m; j++) if (a[j] ~ /^-[a-z]*i/ || a[j] ~ /^--in-place/) inplace = 1
            if (inplace) for (j = k + 1; j <= m; j++) emit("W", a[j])
        } else if (verb == "dd") {
            for (j = k + 1; j <= m; j++) if (a[j] ~ /^of=/) { t = a[j]; sub(/^of=/, "", t); emit("W", t) }
        } else if (verb ~ /^(cp|copy|copy-item|cpi|cpp|install|rsync|scp|xcopy|robocopy|ln)$/) {
            dest = ""
            npos = 0
            for (j = k + 1; j <= m; j++) {
                t = a[j]
                if (t ~ /^--target-directory=/) { sub(/^--target-directory=/, "", t); dest = t; continue }
                if ((t == "-t" || t == "--target-directory" || t ~ /^-dest/ || t == "-target") && j < m) { dest = a[++j]; continue }
                if (t ~ /^-/ || t ~ /^\/[a-z]+$/ && (verb == "xcopy" || verb == "robocopy")) continue
                pos[++npos] = t
            }
            if (dest == "") {
                if ((verb == "xcopy" || verb == "robocopy") && npos >= 2) dest = pos[2]
                else if (npos >= 2 || verb == "copy-item" || verb == "cpi") dest = pos[npos]
            }
            if (dest != "") emit("W", dest)
        } else if (verb == "git") {
            j = k + 1
            while (j <= m && a[j] ~ /^-/) { if (a[j] == "-c" || a[j] == "-C") j++; j++ }
            sc = (j <= m) ? a[j] : ""
            if (sc ~ /^(checkout|restore|reset|stash|apply|am|clean|mv|rm|switch|revert|cherry-pick|merge|pull)$/) {
                for (j2 = j + 1; j2 <= m; j2++) {
                    emit("D", a[j2])
                    if (sc == "apply" || sc == "am") scanpatch(a[j2])
                }
            }
        } else if (verb == "patch") {
            for (j = k + 1; j <= m; j++) if (a[j] !~ /^-/ && a[j] != "<") scanpatch(a[j])
        } else if (verb == "find") {
            if (line ~ /-delete|-exec/)
                for (j = k + 1; j <= m && a[j] !~ /^-/; j++) emit("D", a[j])
        }
        prevn = 0
        for (j = k + 1; j <= m; j++) if (a[j] !~ /^-/) prev[++prevn] = a[j]
    }
}
'

# Directories that hold oracles (relative to ROOT, normalized): deleting or
# moving one of them deletes the oracles inside.
ANCESTORS='^(\.|tests|bench|xtask|xtask/(tests|src)|\.claude|\.github|\.github/workflows|crates|apps|content|(crates|apps|content)/[^/]+|(crates|apps|content)/[^/]+/tests)$'

TOOL=$(json_str tool_name)

case "$TOOL" in
    Edit|Write|MultiEdit|NotebookEdit)
        f=$(json_str file_path)
        [ -z "$f" ] && f=$(json_str notebook_path)
        [ -n "$f" ] && check_path "$f"
        exit 0
        ;;
    Bash|PowerShell)
        cmd=$(json_str command)
        [ -z "$cmd" ] && exit 0
        # JSON-unescape (\" -> ", \\ -> /, \n -> newline), backslashes to /,
        # lower case.
        c=$(printf '%s\n' "$cmd" | sed -e 's#\\\\#/#g' -e 's#\\"#"#g' -e 's#\\n# ; #g' -e 's#\\t# #g' -e 's#\\#/#g' -e "$LOWER")
        # 8.3 short names in the command: expand each token via cygpath.
        case "$c" in
            *~[0-9]*)
                for t in $(printf '%s\n' "$c" | tr -s ' ;&|<>()"=' '\n' | grep '~[0-9]'); do
                    l=$(long "$t" | sed -e "$LOWER")
                    [ "$l" != "$t" ] && c=$(printf '%s\n' "$c" | sed "s#$t#$l#g")
                done
                ;;
        esac
        targets=$(printf '%s\n' "$c" | awk "$TARGETS_AWK" | sed -E "$NORM_SED")
        # The whole-text forms: redirects, npcctl --out/--record.
        tgt="[^[:space:]\"';&|]*"
        if printf '%s\n' "$c" | grep -Eq -e ">[[:space:]]*[\"']?${tgt}(rebaseline-approved|claude-hooks/)"; then
            block_always "command writes .claude/rebaseline-approved or target/claude-hooks/"
        fi
        if printf '%s\n' "$targets" | grep -Eq "$ALWAYS"; then
            block_always "command writes .claude/rebaseline-approved or target/claude-hooks/"
        fi
        [ -f "$MARKER" ] && exit 0
        if printf '%s' "$c" | grep -Eq -e 'cargo[[:space:]]+insta[[:space:]]+(accept|review)|insta_update=(always|force)|insta_force_pass'; then
            block "command accepts insta snapshots"
        fi
        if printf '%s\n' "$targets" | grep -Eq "$PROT"; then
            block "command appears to write a protected path"
        fi
        if printf '%s\n' "$c" | grep -Eq \
            -e ">[[:space:]]*[\"']?${tgt}($PROT_TEXT)" \
            -e "--(out|record)[[:space:]=]+[\"']?${tgt}($PROT_TEXT)"; then
            block "command appears to write a protected path"
        fi
        # Deleting or moving a directory that holds oracles.
        rootn=$(norm "$ROOT")
        rootn=${rootn%/}
        dels=$(printf '%s\n' "$targets" | sed -n '/^@d@$/{n;p;}')
        if [ -n "$dels" ]; then
            rel=$(printf '%s\n' "$dels" | while IFS= read -r p; do
                case "$p" in
                    "$rootn"|"$rootn"/) printf '.\n' ;;
                    "$rootn"/*) printf '%s\n' "${p#"$rootn"/}" ;;
                    /*|[a-z]:/*)
                        # An ancestor of the workspace itself.
                        case "$rootn/" in "${p%/}"/*) printf '.\n' ;; esac ;;
                    *) printf '%s\n' "${p%/}" ;;
                esac
            done)
            if printf '%s\n' "$rel" | grep -Eq "$ANCESTORS"; then
                block "command deletes or moves a directory that holds protected oracles"
            fi
        fi
        # Inline scripts and .NET file APIs (free text, best effort).
        case "$c" in
            *io.file*|*io.directory*)
                if printf '%s\n' "$c" | grep -Eq '\[(system\.)?io\.(file|directory)\]::(write|append|delete|copy|move|replace|create|open|set)' &&
                    printf '%s\n' "$c" | grep -Eq "$PROT_TEXT"; then
                    block "command writes a protected path through [IO.File]"
                fi
                ;;
        esac
        case "$c" in
            *python*|*py\ *|*py.exe*|*node*|*perl*)
                if printf '%s\n' "$c" | grep -Eq '(^|[^a-z0-9_])(python[0-9.]*|py|node|perl)(\.exe)?[[:space:]]+(-[a-z0-9]+[[:space:]]+)*-[a-z]*[ce]' &&
                    printf '%s\n' "$c" | grep -Eq "$PROT_TEXT" &&
                    printf '%s\n' "$c" | grep -Eq "[\"'][wax]b?\+?[\"']|mode[[:space:]]*=|write|unlink|remove|rename|replace|rmtree|truncate|shutil|copy|move|rmsync|rmdir"; then
                    block "an inline script appears to write a protected path"
                fi
                ;;
        esac
        exit 0
        ;;
esac
exit 0
