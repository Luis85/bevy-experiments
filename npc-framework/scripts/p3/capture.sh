#!/bin/sh
# Usage: capture.sh <npcctl binary> <out dir> <corpus dir>
#
# Captures journals (JSONL), run summaries, replays and inspect snapshots for
# every corpus file plus a few extra village seeds: the "before" and "after"
# evidence of a re-baseline (first used for the P3 equivalence proof,
# docs/_notes/p3-rebaseline-evidence.md; compare two captures with
# compare.py). Run it with the old and the new npcctl on a *copy* of
# tests/replays, never on the corpus itself.
#
# Corpus files of every kind (the dispatch of `npcctl replay verify`):
# - village replays (`"scenario": "village"|"village_n"|..`): --seed,
#   --scenario, --villagers, --commands <the replay>;
# - testbed replays (`"scenario": "testbed[:<name>]"`): the same flags;
# - agent session records (`{"scenario": {..doc..}, "replay": {..}}`): the
#   scenario document and the replay are written to <out>/<name>.scenario.json
#   and <out>/<name>.commands.json and run as a scenario file.
set -eu
BIN="$1"; OUT="$2"; CORPUS="$3"
mkdir -p "$OUT"
# Prints one line for a corpus file: seed, --scenario value, villagers,
# ticks, commands file.
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
    print(r["seed"], scen, rep.get("villagers", 1), rep["ticks"], cmds)
else:
    print(r["seed"], r["scenario"], r.get("villagers", 1), r["ticks"], path)
'
for f in "$CORPUS"/*.json; do
  name=$(basename "$f" .json)
  line=$(python -I -c "$KIND_PY" "$f" "$OUT" "$name")
  set -- $line
  seed=$1; scen=$2; vill=$3; ticks=$4; cmds=$5
  W="--seed $seed --scenario $scen --villagers $vill --commands $cmds"
  "$BIN" run $W --ticks "$ticks" --journal "$OUT/$name.journal.jsonl" --out "$OUT/$name.run.json"
  "$BIN" run $W --ticks "$ticks" --verbose --journal "$OUT/$name.verbose.jsonl" --out "$OUT/$name.run_verbose.json"
  "$BIN" replay record $W --ticks "$ticks" --checkpoint-every 100 --out "$OUT/$name.replay.json"
  "$BIN" inspect $W --tick "$ticks" --all --out "$OUT/$name.inspect_all.json"
done
# Extra village runs (the village's own seeds and sizes).
for seed in 1 2 3 7; do
  "$BIN" run --seed $seed --ticks 7200 --journal "$OUT/seed$seed.journal.jsonl" --out "$OUT/seed$seed.run.json"
done
"$BIN" run --seed 5 --villagers 9 --ticks 4800 --journal "$OUT/n9_seed5.journal.jsonl" --out "$OUT/n9_seed5.run.json"
for t in 600 1800 4000; do
  "$BIN" inspect --seed 1 --tick $t --unit 5 --out "$OUT/inspect_unit5_t$t.json"
done
"$BIN" diverge --seed-a 1 --seed-b 2 --ticks 2400 --out "$OUT/diverge_1_2.json" || true
echo "captured into $OUT"
