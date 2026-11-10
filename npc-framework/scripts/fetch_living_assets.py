"""Fetch the curated living-world CC0 assets; never execute downloaded content.

Run from any directory: python scripts/fetch_living_assets.py
Existing manifest hashes are checked, including on a fresh download. Models are
audited as glTF JSON before promotion. The manifest records exact source bytes.
"""
from pathlib import Path
import hashlib
import json
import re
import struct
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "assets"
MANIFEST = ASSETS / "living-world-manifest.json"
HEADERS = {"User-Agent": "Mozilla/5.0 (NPC Framework asset provenance audit)"}
entries = {}
if MANIFEST.exists():
    entries = {x["path"]: x for x in json.loads(MANIFEST.read_text())["files"]}


def read(url):
    return urllib.request.urlopen(urllib.request.Request(url, headers=HEADERS), timeout=45).read()


def promote(url, relative, source, author, license="CC0-1.0"):
    path = ASSETS / relative
    data = path.read_bytes() if path.exists() else read(url)
    digest = hashlib.sha256(data).hexdigest()
    old = entries.get(relative)
    if old and old["sha256"] != digest:
        raise ValueError(f"Asset changed: {relative}; review upstream before updating manifest")
    if relative.endswith((".glb", ".gltf")):
        model_json(data)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    entries[relative] = dict(path=relative, url=url, source=source, author=author,
                             license=license, bytes=len(data), sha256=digest)
    return data


def model_json(data):
    if data[:4] == b"glTF":
        length, kind = struct.unpack_from("<II", data, 12)
        if kind != 0x4E4F534A:
            raise ValueError("Missing GLB JSON chunk")
        return json.loads(data[20:20 + length])
    return json.loads(data)


def github_pack(repo, files):
    source = "https://github.com/KayKit-Game-Assets/" + repo
    previous = next((e for e in entries.values() if e["source"] == source), None)
    if previous:
        revision = previous["url"].split("/")[5]
    else:
        revision = json.loads(read("https://api.github.com/repos/KayKit-Game-Assets/" + repo + "/commits/main"))["sha"]
    base = f"https://raw.githubusercontent.com/KayKit-Game-Assets/{repo}/{revision}/"
    promote(base + "LICENSE.txt", f"licenses/{repo}.txt", source, "Kay Lousberg")
    for upstream, relative in files:
        url = base + upstream
        data = promote(url, relative, source, "Kay Lousberg")
        doc = model_json(data)
        for item in doc.get("buffers", []) + doc.get("images", []):
            uri = item.get("uri", "")
            if uri and not uri.startswith("data:"):
                dest = Path(relative).parent / urllib.parse.unquote(uri)
                if ".." in dest.parts:
                    raise ValueError("Unexpected parent URI")
                promote(urllib.parse.urljoin(url, uri), dest.as_posix(), source, "Kay Lousberg")


def main():
    hex_root = "addons/kaykit_medieval_hexagon_pack/Assets/gltf/"
    names = ["blacksmith", "lumbermill", "market", "mine", "tavern", "watermill", "well", "windmill", "home_B", "tower_A"]
    files = [(hex_root + f"buildings/blue/building_{n}_blue.gltf", f"living/buildings/building_{n}_blue.gltf") for n in names]
    for n in ["tree_single_A", "tree_single_B", "tree_single_A_cut", "tree_single_B_cut", "trees_A_small", "trees_B_small", "rock_single_B", "rock_single_D", "rock_single_E", "waterplant_A", "waterplant_B", "waterlily_A", "cloud_big", "cloud_small"]:
        files.append((hex_root + f"decoration/nature/{n}.gltf", f"living/nature/{n}.gltf"))
    github_pack("KayKit-Medieval-Hexagon-Pack-1.0", files)
    github_pack("KayKit-Character-Pack-Adventures-1.0", [
        (f"addons/kaykit_character_pack_adventures/Characters/gltf/{name}.glb", f"living/characters/{name}.glb")
        for name in ["Barbarian", "Mage", "Rogue"]])
    for name, model_id in [("VillageWoman", "nIItLV9nxS"), ("TravellerWoman", "qJ2gsTUBHL"), ("FarmerMan", "7pn3R6hPvE"), ("TravellerMan", "5EGWBMpuXq")]:
        source = "https://poly.pizza/m/" + model_id
        html = read(source).decode()
        if '"Licence":"CC0 1.0"' not in html or 'Quaternius' not in html:
            raise ValueError(f"Unverified creator/license: {source}")
        url = re.search(r"https://static\.poly\.pizza/[a-z0-9-]+\.glb", html).group()
        promote(url, f"living/characters/{name}.glb", source, "Quaternius")
    MANIFEST.write_text(json.dumps({"version": 1, "files": sorted(entries.values(), key=lambda e: e["path"])}, indent=2) + "\n")
    report = []
    for entry in entries.values():
        if not entry["path"].endswith((".glb", ".gltf")):
            continue
        doc = model_json((ASSETS / entry["path"]).read_bytes())
        report.append({"path": entry["path"], "skins": len(doc.get("skins", [])),
                       "clips": [a.get("name") for a in doc.get("animations", [])],
                       "nodes": [n.get("name") for n in doc.get("nodes", [])],
                       "bounds": [(a.get("min"), a.get("max")) for a in doc.get("accessors", []) if a.get("type") == "VEC3" and "min" in a]})
    out = ROOT / "target/living-assets-audit.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Verified {len(entries)} files; inspection report: {out}")


if __name__ == "__main__":
    main()
