#!/bin/sh
# Usage: stage_replays.sh <npcctl binary> [<corpus name> ...]
#
# Stage 1 of a re-baseline (docs/rebaselining.md): re-records corpus files
# tests/replays/<name>.json (every corpus file when no name is given) with
# the given npcctl into tests/replays.next/<name>.json, with the original's
# seed, scenario (or scenario document), villager count, length, commands
# and checkpoint interval. The originals are only read; the staged files are
# what corpus_equivalence.rs (village and testbed) checks and what the user
# swaps in after approving.
#
# - Village/testbed replays: `replay record --seed --scenario --villagers
#   --ticks --checkpoint-every --commands <the original>`.
# - Agent session records (`{"scenario": {..doc..}, "replay": {..}}`): the
#   document and the replay are extracted to target/rebaseline/stage/ and
#   recorded as a scenario file, which prints a session record again.
#
# The id remap of a scenario that gained an entity (`<name>.idmap.json`) is
# written by hand; this script never edits commands.
set -eu
[ $# -ge 1 ] || { echo "usage: $0 <npcctl> [<corpus name> ...]" >&2; exit 2; }
BIN="$1"; shift
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
CORPUS="$ROOT/tests/replays"
NEXT="$ROOT/tests/replays.next"
TMP="$ROOT/target/rebaseline/stage"
mkdir -p "$NEXT" "$TMP"
if [ $# -eq 0 ]; then
    set -- $(cd "$CORPUS" && ls *.json | sed 's/\.json$//')
fi
# One line per corpus file: seed, --scenario value, villagers, ticks,
# checkpoint interval, commands file.
KIND_PY='
import json, os, sys
path, out, name = sys.argv[1:4]
r = json.load(open(path, encoding="utf-8"))
if isinstance(r.get("scenario"), dict) and "replay" in r:
    rep = r["replay"]
    scen = os.path.join(out, name + ".scenario.json").replace(os.sep, "/")
    cmds = os.path.join(out, name + ".commands.json").replace(os.sep, "/")
    json.dump(r["scenario"], open(scen, "w", encoding="utf-8"))
    json.dump(rep, open(cmds, "w", encoding="utf-8"))
    seed = r["seed"]
else:
    rep = r
    scen = r["scenario"]
    cmds = path.replace(os.sep, "/")
    seed = r["seed"]
cps = rep.get("checkpoints") or [[100, 0]]
print(seed, scen, rep.get("villagers", 1), rep["ticks"], cps[0][0], cmds)
'
for name in "$@"; do
    f="$CORPUS/$name.json"
    [ -f "$f" ] || { echo "no corpus file $f" >&2; exit 2; }
    line=$(python -I -c "$KIND_PY" "$f" "$TMP" "$name")
    set -- $line
    "$BIN" replay record --seed "$1" --scenario "$2" --villagers "$3" --ticks "$4" \
        --checkpoint-every "$5" --commands "$6" --out "$TMP/$name.json" >/dev/null
    # Staged next to the corpus, never over it.
    cp "$TMP/$name.json" "$NEXT/$name.json"
    "$BIN" replay verify "$NEXT/$name.json" >/dev/null
    echo "staged tests/replays.next/$name.json"
done
