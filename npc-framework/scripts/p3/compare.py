"""Compare pre-split and post-split captures.

Usage: compare.py PRE_DIR POST_DIR
- *.jsonl (journals): byte-identical.
- *.json: byte-identical after removing hash-bearing fields
  (final_hash; replay version/content_hash/checkpoint hashes).
Prints one line per file and a final verdict.
"""
import hashlib
import json
import os
import sys

pre, post = sys.argv[1], sys.argv[2]
names = sorted(set(os.listdir(pre)) | set(os.listdir(post)))
ok = True
rows = []


def strip(name, obj):
    """Drop fields whose value depends on the hash encoding / format version."""
    removed = []
    if isinstance(obj, dict):
        for k in ("journal", "record"):
            if isinstance(obj.get(k), str):
                obj[k] = obj[k].replace("cap-pre", "cap").replace("cap-post", "cap")
        if "final_hash" in obj:
            obj.pop("final_hash")
            removed.append("final_hash")
        if name.endswith(".replay.json"):
            for k in ("version", "content_hash"):
                if k in obj:
                    obj.pop(k)
                    removed.append(k)
            if "checkpoints" in obj:
                obj["checkpoints"] = [c[0] for c in obj["checkpoints"]]
                removed.append("checkpoint hashes (ticks kept)")
    return removed


for name in names:
    a, b = os.path.join(pre, name), os.path.join(post, name)
    if not (os.path.exists(a) and os.path.exists(b)):
        rows.append((name, "MISSING on one side"))
        ok = False
        continue
    ba, bb = open(a, "rb").read(), open(b, "rb").read()
    if name.endswith(".jsonl"):
        same = ba == bb
        lines = ba.count(b"\n")
        rows.append((name, f"{'IDENTICAL' if same else 'DIFFERENT'} bytes ({lines} lines, sha256 {hashlib.sha256(ba).hexdigest()[:16]})"))
        ok &= same
        continue
    ja, jb = json.loads(ba), json.loads(bb)
    if ba == bb:
        rows.append((name, "IDENTICAL bytes"))
        continue
    ra, rb = strip(name, ja), strip(name, jb)
    # Re-serialize both sides the same way after stripping.
    sa = json.dumps(ja, sort_keys=False)
    sb = json.dumps(jb, sort_keys=False)
    same = sa == sb
    rows.append((name, f"{'IDENTICAL' if same else 'DIFFERENT'} after removing {sorted(set(ra) | set(rb))}"))
    ok &= same

for name, verdict in rows:
    print(f"{name}: {verdict}")
print("VERDICT:", "all equivalent" if ok else "DIFFERENCES FOUND")
sys.exit(0 if ok else 1)
