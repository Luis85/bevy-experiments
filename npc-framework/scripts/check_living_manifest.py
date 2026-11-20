"""Check assets/living-world-manifest.json against the files on disk and the island's loads.

Usage: python -I check_manifest.py WORKSPACE   (the npc-framework folder)

Checks: entries sorted by path, no duplicate paths, exact keys, file exists with the
recorded bytes and sha256, licence CC0-1.0. Unlisted files are reported from
 (a) every file under the folders whose whole content is in scope (FULL_DIRS), minus our
     own provenance records and the UI icon set (its own manifest), and
 (b) every asset path the island crates name in source, plus the external buffers and
     images those glTF/GLB files reference (testbed/ is shared with the examples, so only
     the files the island loads are in scope there).
"""
import hashlib, json, os, re, struct, sys, urllib.parse

ws = sys.argv[1]
root = os.path.join(ws, "assets")
KEYS = ["path", "url", "source", "author", "license", "bytes", "sha256"]
FULL_DIRS = ["living", "hex", "furniture", "props", "characters", "textures", "licenses"]
ISLAND = ["content/island_content", "content/island_view", "apps/island_game"]
EXEMPT = re.compile(r"^licenses/(.*-sources\.txt|Maki-.*|Temaki-.*)$")  # own records; ui-icons-manifest
LIT = re.compile(r'"((?:living|hex|furniture|props|characters|textures|testbed)/[^"{}\s]+\.(?:glb|gltf|bin|png|jpg|ogg|txt))"')

raw = open(os.path.join(root, "living-world-manifest.json"), encoding="utf-8", newline="").read()
m = json.loads(raw)
bad = []
fmt_ok = raw == json.dumps(m, indent=2) + "\n" and "\r" not in raw
if not fmt_ok: bad.append("format: not indent-2 LF with trailing newline")
paths = [f["path"] for f in m["files"]]
if paths != sorted(paths): bad.append("not sorted by path")
dups = sorted({p for p in paths if paths.count(p) > 1})
if dups: bad.append(f"duplicates {dups}")
lic = set()
for f in m["files"]:
    if list(f.keys()) != KEYS: bad.append(f"keys {f.get('path')}: {list(f.keys())}")
    lic.add(f["license"])
    if f["license"] != "CC0-1.0": bad.append(f"licence {f['path']}: {f['license']}")
    p = os.path.join(root, *f["path"].split("/"))
    if not os.path.isfile(p): bad.append(f"MISSING {f['path']}"); continue
    b = open(p, "rb").read()
    if len(b) != f["bytes"] or hashlib.sha256(b).hexdigest() != f["sha256"]: bad.append(f"MISMATCH {f['path']}")
listed = set(paths)

def rel(p): return "/".join(os.path.relpath(p, root).split(os.sep))
unlisted = set()
for d in FULL_DIRS:
    for dp, _, fs in os.walk(os.path.join(root, d)):
        for x in fs:
            r = rel(os.path.join(dp, x))
            if r not in listed and not EXEMPT.match(r): unlisted.add(r)

def deps(r):
    p = os.path.join(root, *r.split("/"))
    b = open(p, "rb").read()
    if b[:4] == b"glTF":
        n = struct.unpack_from("<I", b, 12)[0]; doc = json.loads(b[20:20 + n])
    else:
        doc = json.loads(b)
    out = []
    for it in doc.get("buffers", []) + doc.get("images", []):
        u = it.get("uri", "")
        if u and not u.startswith("data:"):
            out.append(rel(os.path.normpath(os.path.join(os.path.dirname(p), urllib.parse.unquote(u)))))
    return out

loaded, missing_load = set(), []
for c in ISLAND:
    for dp, _, fs in os.walk(os.path.join(ws, c)):
        for x in fs:
            if x.endswith((".rs", ".json", ".ron", ".toml")):
                for lit in LIT.findall(open(os.path.join(dp, x), encoding="utf-8", errors="replace").read()):
                    loaded.add(lit)
queue = sorted(loaded)
while queue:
    r = queue.pop()
    if not os.path.isfile(os.path.join(root, *r.split("/"))):
        missing_load.append(r); continue
    if r.endswith((".glb", ".gltf")):
        for d in deps(r):
            if d not in loaded: loaded.add(d); queue.append(d)
# the audio catalog reads only manifest-listed `living/audio/` files that exist; the
# literal names in catalog.rs tests are fixtures, not loads
missing_load = sorted(x for x in missing_load if not x.startswith("living/audio/"))
unlisted |= {r for r in loaded if r not in listed and not r.startswith("living/audio/")}
print("entries", len(m["files"]), "bad", len(bad), "licenses", sorted(lic), "format", "ok" if fmt_ok else "BAD")
for b in bad: print("  BAD", b)
print("island-named assets", len(loaded), "not on disk", len(missing_load), missing_load)
print("unlisted", len(unlisted), sorted(unlisted))
sys.exit(1 if bad or unlisted else 0)
