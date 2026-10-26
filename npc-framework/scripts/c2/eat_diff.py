"""Classify pre/post journal differences for the C2 eat-claim fix.

One-off evidence script (wave C2), kept so docs/_notes/c2-eat-claim-evidence.md
stays reproducible; it is not part of any gate and knows only the village
journal kinds of that fix. For a new re-baseline, capture with
scripts/p3/capture.sh and compare with scripts/p3/compare.py.

Usage: eat_diff.py PRE_DIR POST_DIR
For every *.jsonl journal pair:
- meal releases: pre `Release{Finished}` entries in the tick a unit's `EatFood`
  leaf stops (Finished); post `Release{Finished}` entries in the tick the unit's
  `TakeFood` effect applies. Pairs them per (unit, target) and checks the post
  one is exactly one tick earlier.
- removes those entries from both sides, drops `seq`, and compares the rest.
  Remaining differences are printed (first ones) with context.
"""
import json
import os
import sys
from collections import defaultdict

pre_dir, post_dir = sys.argv[1], sys.argv[2]


def load(path):
    return [json.loads(l) for l in open(path, encoding="utf-8")]


def is_release(e):
    k = e["kind"]
    return k["type"] == "Release" and k["reason"] == "Finished"


def meal_releases_pre(entries):
    stops = {(e["tick"], e["unit"]) for e in entries
             if e["kind"]["type"] == "LeafStop" and e["kind"]["action"] == "EatFood"
             and e["kind"]["reason"] == "Finished"}
    return [i for i, e in enumerate(entries) if is_release(e) and (e["tick"], e["unit"]) in stops]


def meal_releases_post(entries):
    takes = {(e["tick"], e["unit"]) for e in entries
             if e["kind"]["type"] == "Effect" and e["kind"]["effect"].get("type") == "TakeFood"}
    return [i for i, e in enumerate(entries) if is_release(e) and (e["tick"], e["unit"]) in takes]


def strip(entries, idx):
    drop = set(idx)
    out = []
    for i, e in enumerate(entries):
        if i in drop:
            continue
        e = dict(e)
        e.pop("seq")
        out.append(e)
    return out


total_ok = True
for name in sorted(os.listdir(pre_dir)):
    if not name.endswith(".jsonl"):
        continue
    a, b = load(os.path.join(pre_dir, name)), load(os.path.join(post_dir, name))
    ia, ib = meal_releases_pre(a), meal_releases_post(b)
    ra = sorted((a[i]["unit"], a[i]["kind"]["target"], a[i]["tick"]) for i in ia)
    rb = sorted((b[i]["unit"], b[i]["kind"]["target"], b[i]["tick"]) for i in ib)
    # Pair by (unit, target) in order.
    pa, pb = defaultdict(list), defaultdict(list)
    for u, t, tick in ra:
        pa[(u, t)].append(tick)
    for u, t, tick in rb:
        pb[(u, t)].append(tick)
    shifted = sum(1 for k in pa for x, y in zip(pa[k], pb.get(k, [])) if y == x - 1)
    unpaired_pre = sum(max(0, len(pa[k]) - len(pb.get(k, []))) for k in pa)
    unpaired_post = sum(max(0, len(pb[k]) - len(pa.get(k, []))) for k in pb)
    sa, sb = strip(a, ia), strip(b, ib)
    same_rest = sa == sb
    print(f"{name}: lines pre {len(a)} post {len(b)}; meal releases pre {len(ia)} post {len(ib)}, "
          f"moved one tick earlier {shifted}, unpaired pre {unpaired_pre} post {unpaired_post}; "
          f"rest {'IDENTICAL' if same_rest else 'DIFFERENT'}")
    if not same_rest:
        total_ok = False
        # First differing position and the per-entry diff summary.
        n = min(len(sa), len(sb))
        first = next((i for i in range(n) if sa[i] != sb[i]), n)
        print(f"  first difference at entry {first} (tick {sa[first]['tick'] if first < len(sa) else '-'})")
        import difflib
        la = [json.dumps(e, sort_keys=True) for e in sa]
        lb = [json.dumps(e, sort_keys=True) for e in sb]
        sm = difflib.SequenceMatcher(a=la, b=lb, autojunk=False)
        shown = 0
        for op, i1, i2, j1, j2 in sm.get_opcodes():
            if op == "equal":
                continue
            print(f"  {op}: pre[{i1}:{i2}] post[{j1}:{j2}]")
            for l in la[i1:i2][:6]:
                print("    - " + l)
            for l in lb[j1:j2][:6]:
                print("    + " + l)
            shown += 1
            if shown >= 12:
                print("  ...")
                break
print("VERDICT:", "only meal-release timing differs" if total_ok else "OTHER DIFFERENCES (see above)")
