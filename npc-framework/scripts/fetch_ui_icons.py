"""Fetch the interface icons (Maki and Temaki, CC0), verify them and build the atlases.

Run from any directory:
  python -I scripts/fetch_ui_icons.py [--download-dir DIR] [--offline]

What it does, in order:
  1. Downloads the two licence texts and the 92 SVG sources by their pinned raw
     URLs into a download directory (a new temporary folder unless one is given;
     never the script folder). Nothing downloaded is executed or imported.
  2. Refuses to go on unless both licence texts say "CC0 1.0 Universal".
  3. Checks every file against `assets/ui-icons-manifest.json` when that exists
     (a changed upstream file is an error, not an update), then copies the
     sources unchanged to `assets/ui/icons/src/{maki,temaki}/`.
  4. Rasterises each source to white on transparent at 32 and 64 px cells with a
     1 px (2 px) margin and packs two atlases of 10 columns.
  5. Writes the manifest: path, url, source, author, license, bytes and sha256 per
     file, the atlas cell of each icon and the hashes of the two PNGs.

`--offline` skips the network and rebuilds the atlases and the manifest from the
sources already under `assets/ui/icons/src/`.

Rasteriser: a small even-odd/non-zero scanline fill in this script, and a PNG
writer on `zlib`; standard library only, so it runs under `python -I` with no
packages. The spec names the `resvg` command-line tool as the alternative; it
is not installed on the build machine and the sources need nothing more than
filled paths. The script stops on any SVG feature it does not implement (strokes,
transforms, gradients, text) instead of drawing it wrongly.
"""
from pathlib import Path
import argparse
import hashlib
import json
import math
import re
import shutil
import struct
import tempfile
import urllib.request
import xml.etree.ElementTree as ElementTree
import zlib

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "assets"
MANIFEST = ASSETS / "ui-icons-manifest.json"
ICONS = ASSETS / "ui" / "icons"
# A generic client name: no project or personal detail leaves the machine.
HEADERS = {"User-Agent": "python-urllib (icon fetch)"}
COLUMNS = 10
SAMPLES = 8
MAX_BYTES = 64 * 1024

SETS = {
    "maki": {
        "source": "https://github.com/mapbox/maki",
        "author": "Mapbox and contributors",
        "version": "8.0.0",
        "commit": "070ed0af1012da8dee83593bb0a33e4592ed68f5",
        "raw": "https://raw.githubusercontent.com/mapbox/maki/",
        "license_file": "LICENSE.txt",
        "license_path": "licenses/Maki-8.0.0-CC0.txt",
        "icons": [
            "aquarium", "arrow", "bakery", "beer", "caution", "circle", "circle-stroked",
            "clothing-store", "cross", "diamond", "farm", "fence", "ferry", "garden", "harbor",
            "hardware", "heart", "home", "hospital", "information", "library", "lodging",
            "logging", "marker", "mountain", "paint", "park", "restaurant", "roadblock", "shoe",
            "shop", "slaughterhouse", "square", "star", "triangle", "viewpoint", "village",
            "warehouse", "waste-basket", "water", "windmill",
        ],
    },
    "temaki": {
        "source": "https://github.com/rapideditor/temaki",
        "author": "Rapid editor contributors",
        "version": "5.13.0",
        "commit": "ff12eb30f7f91b4c0e17594f8c548be29aa03f4b",
        "raw": "https://raw.githubusercontent.com/rapideditor/temaki/",
        "license_file": "LICENSE.md",
        "license_path": "licenses/Temaki-5.13.0-CC0.txt",
        "icons": [
            "anvil", "anvil_and_hammer", "barn", "binoculars", "boulder1", "bow_and_arrow",
            "bread", "brick_trowel", "briefcase", "bulb", "bulletin_board", "cabin", "campfire",
            "clock", "compass", "dagger", "fireplace", "fishing_pier", "garden_bed", "grapes",
            "grass", "handbag", "hotpot", "ice_fishing", "island_trees_building", "lock", "meat",
            "mineshaft_cage", "money_hand", "needle_and_spool", "pedestrian", "pick_hammer",
            "plant", "portrait", "room", "row_houses", "scaffold", "silo", "sleep_shelter",
            "snow", "speaker", "spice_bottle", "storage", "tools", "toolbox", "town_hall",
            "tree_needleleaved", "tree_stump", "vase", "well_pump_manual", "windsock",
        ],
    },
}


def fetch(url):
    request = urllib.request.Request(url, headers=HEADERS)
    with urllib.request.urlopen(request, timeout=45) as response:
        data = response.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise ValueError(f"{url}: larger than {MAX_BYTES} bytes")
    return data


def require_cc0(name, text):
    head = "\n".join(text.strip().splitlines()[:6])
    if "CC0 1.0 Universal" not in head:
        raise SystemExit(f"{name}: the licence text does not open with 'CC0 1.0 Universal'. "
                         "Stop and report; do not use this set.")
    return next(line.strip() for line in text.splitlines() if "CC0 1.0 Universal" in line)


# --- SVG subset -------------------------------------------------------------------------------

NUMBER = re.compile(r"[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?")
TOKEN = re.compile(r"[MmLlHhVvCcSsQqTtAaZz]|[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?")
ARITY = {"M": 2, "L": 2, "H": 1, "V": 1, "C": 6, "S": 4, "Q": 4, "T": 2, "A": 7, "Z": 0}
ALLOWED = {"svg", "path", "rect", "circle", "ellipse", "polygon", "g", "title", "desc"}
REFUSED = ("stroke", "transform", "filter", "mask", "clip-path", "style", "opacity", "fill-opacity")


def arc_flags(tokens, index):
    """Arc flags may be written without separators ("a1 1 0 011 1")."""
    out = []
    while len(out) < 2:
        token = tokens[index]
        if token in ("0", "1"):
            out.append(float(token))
            index += 1
        elif token and token[0] in "01" and len(token) > 1 and "." not in token[:1]:
            out.append(float(token[0]))
            tokens[index] = token[1:]
        else:
            raise ValueError(f"bad arc flag {token!r}")
    return out, index


def cubic(p0, p1, p2, p3, steps=16):
    return [tuple((1 - t) ** 3 * a + 3 * (1 - t) ** 2 * t * b + 3 * (1 - t) * t * t * c + t ** 3 * d
                  for a, b, c, d in zip(p0, p1, p2, p3))
            for t in (step / steps for step in range(1, steps + 1))]


def quadratic(p0, p1, p2, steps=12):
    return [tuple((1 - t) ** 2 * a + 2 * (1 - t) * t * b + t * t * c for a, b, c in zip(p0, p1, p2))
            for t in (step / steps for step in range(1, steps + 1))]


def arc(p0, rx, ry, rotation, large, sweep, p1):
    """Endpoint to centre parameterisation (SVG 1.1, appendix F.6)."""
    if p0 == p1:
        return []
    rx, ry = abs(rx), abs(ry)
    if rx == 0 or ry == 0:
        return [p1]
    phi = math.radians(rotation)
    cos, sin = math.cos(phi), math.sin(phi)
    dx, dy = (p0[0] - p1[0]) / 2, (p0[1] - p1[1]) / 2
    x1, y1 = cos * dx + sin * dy, -sin * dx + cos * dy
    scale = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry)
    if scale > 1:
        rx, ry = rx * math.sqrt(scale), ry * math.sqrt(scale)
    numerator = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1
    denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1
    factor = math.sqrt(max(numerator / denominator, 0.0)) * (-1 if large == sweep else 1)
    cxp, cyp = factor * rx * y1 / ry, -factor * ry * x1 / rx
    cx = cos * cxp - sin * cyp + (p0[0] + p1[0]) / 2
    cy = sin * cxp + cos * cyp + (p0[1] + p1[1]) / 2

    def angle(ux, uy, vx, vy):
        value = math.atan2(ux * vy - uy * vx, ux * vx + uy * vy)
        return value

    start = angle(1, 0, (x1 - cxp) / rx, (y1 - cyp) / ry)
    delta = angle((x1 - cxp) / rx, (y1 - cyp) / ry, (-x1 - cxp) / rx, (-y1 - cyp) / ry)
    if not sweep and delta > 0:
        delta -= 2 * math.pi
    elif sweep and delta < 0:
        delta += 2 * math.pi
    steps = max(4, int(abs(delta) / (math.pi / 16)) + 1)
    points = []
    for step in range(1, steps + 1):
        theta = start + delta * step / steps
        x, y = rx * math.cos(theta), ry * math.sin(theta)
        points.append((cos * x - sin * y + cx, sin * x + cos * y + cy))
    points[-1] = p1
    return points


def path_polygons(d):
    tokens = TOKEN.findall(d)
    polygons, current = [], []
    position = start = (0.0, 0.0)
    control = None
    command = None
    index = 0
    while index < len(tokens):
        if tokens[index].isalpha():
            command = tokens[index]
            index += 1
        elif command is None:
            raise ValueError("path data starts with a number")
        elif command in "Mm":
            # Further pairs after a moveto are linetos.
            command = "L" if command == "M" else "l"
        upper, relative = command.upper(), command.islower()
        if upper == "Z":
            if current:
                polygons.append(current)
            current, position, control = [], start, None
            continue
        if upper == "A":
            head = [float(tokens[index + offset]) for offset in range(3)]
            flags, after = arc_flags(tokens, index + 3)
            tail = [float(tokens[after]), float(tokens[after + 1])]
            values, index = head + flags + tail, after + 2
        else:
            count = ARITY[upper]
            values = [float(token) for token in tokens[index:index + count]]
            if len(values) != count:
                raise ValueError(f"short arguments for {command}")
            index += count

        def point(x, y):
            return (position[0] + x, position[1] + y) if relative else (x, y)

        if upper == "M":
            if current:
                polygons.append(current)
            position = start = point(values[0], values[1])
            current, control = [position], None
        elif upper == "L":
            position = point(values[0], values[1])
            current.append(position)
            control = None
        elif upper == "H":
            position = (position[0] + values[0] if relative else values[0], position[1])
            current.append(position)
            control = None
        elif upper == "V":
            position = (position[0], position[1] + values[0] if relative else values[0])
            current.append(position)
            control = None
        elif upper in "CS":
            if upper == "C":
                first, second, end = point(*values[0:2]), point(*values[2:4]), point(*values[4:6])
            else:
                first = ((2 * position[0] - control[1][0], 2 * position[1] - control[1][1])
                         if control and control[0] == "C" else position)
                second, end = point(*values[0:2]), point(*values[2:4])
            current.extend(cubic(position, first, second, end))
            position, control = end, ("C", second)
        elif upper in "QT":
            if upper == "Q":
                first, end = point(*values[0:2]), point(*values[2:4])
            else:
                first = ((2 * position[0] - control[1][0], 2 * position[1] - control[1][1])
                         if control and control[0] == "Q" else position)
                end = point(*values[0:2])
            current.extend(quadratic(position, first, end))
            position, control = end, ("Q", first)
        elif upper == "A":
            end = point(values[5], values[6])
            current.extend(arc(position, values[0], values[1], values[2], bool(values[3]),
                               bool(values[4]), end))
            position, control = end, None
        if not current:
            current = [position]
    if current:
        polygons.append(current)
    return [polygon for polygon in polygons if len(polygon) >= 3]


def ellipse_polygon(cx, cy, rx, ry, steps=64):
    return [(cx + rx * math.cos(2 * math.pi * step / steps), cy + ry * math.sin(2 * math.pi * step / steps))
            for step in range(steps)]


def shapes(svg_bytes, name):
    """[(polygons, even_odd)] in viewBox units, and the viewBox size."""
    text = svg_bytes.decode("utf-8")
    if "<!DOCTYPE" in text or "<!ENTITY" in text:
        raise ValueError(f"{name}: document type declarations are not accepted")
    root = ElementTree.fromstring(text)
    box = [float(value) for value in NUMBER.findall(root.get("viewBox", ""))]
    if len(box) != 4 or box[0] != 0 or box[1] != 0 or box[2] != box[3]:
        raise ValueError(f"{name}: unexpected viewBox {root.get('viewBox')!r}")
    out = []
    for element in root.iter():
        tag = element.tag.rsplit("}", 1)[-1]
        if tag not in ALLOWED:
            raise ValueError(f"{name}: unsupported element <{tag}>")
        for attribute in REFUSED:
            if element.get(attribute) not in (None, "none"):
                raise ValueError(f"{name}: unsupported attribute {attribute} on <{tag}>")
        if element.get("fill") == "none":
            continue
        even_odd = "evenodd" in (element.get("fill-rule", ""), element.get("clip-rule", ""))
        number = lambda key, default=0.0: float(element.get(key, default))  # noqa: E731
        if tag == "path":
            out.append((path_polygons(element.get("d", "")), even_odd))
        elif tag == "rect":
            if element.get("rx") or element.get("ry"):
                raise ValueError(f"{name}: rounded <rect> is not implemented")
            x, y, w, h = number("x"), number("y"), number("width"), number("height")
            out.append(([[(x, y), (x + w, y), (x + w, y + h), (x, y + h)]], even_odd))
        elif tag == "circle":
            out.append(([ellipse_polygon(number("cx"), number("cy"), number("r"), number("r"))], even_odd))
        elif tag == "ellipse":
            out.append(([ellipse_polygon(number("cx"), number("cy"), number("rx"), number("ry"))], even_odd))
        elif tag == "polygon":
            values = [float(value) for value in NUMBER.findall(element.get("points", ""))]
            out.append(([list(zip(values[0::2], values[1::2]))], even_odd))
    if not any(polygons for polygons, _ in out):
        raise ValueError(f"{name}: nothing to draw")
    return out, box[2]


def coverage(polygons, even_odd, scale, margin, cell):
    """Covered sub-samples per pixel (0..SAMPLES*SAMPLES) of one shape in a `cell` square."""
    size = cell * SAMPLES
    counts = [[0] * cell for _ in range(cell)]
    edges = []
    for polygon in polygons:
        points = [((x * scale + margin) * SAMPLES, (y * scale + margin) * SAMPLES) for x, y in polygon]
        for (x0, y0), (x1, y1) in zip(points, points[1:] + points[:1]):
            if y0 != y1:
                edges.append((x0, y0, x1, y1))
    for row in range(size):
        y = row + 0.5
        crossings = []
        for x0, y0, x1, y1 in edges:
            if (y0 <= y < y1) or (y1 <= y < y0):
                crossings.append((x0 + (y - y0) * (x1 - x0) / (y1 - y0), 1 if y1 > y0 else -1))
        crossings.sort()
        winding = 0
        line = counts[row // SAMPLES]
        for (x, direction), (following, _) in zip(crossings, crossings[1:]):
            winding += direction
            inside = winding % 2 == 1 if even_odd else winding != 0
            if not inside:
                continue
            left, right = max(int(math.ceil(x - 0.5)), 0), min(int(math.ceil(following - 0.5)), size)
            for column in range(left, right):
                line[column // SAMPLES] += 1
    return counts


def rasterise(svg_bytes, name, cell):
    """Alpha (0..255) rows of one icon."""
    margin = cell // 32
    drawn, view = shapes(svg_bytes, name)
    scale = (cell - 2 * margin) / view
    full = SAMPLES * SAMPLES
    alpha = [[0] * cell for _ in range(cell)]
    for polygons, even_odd in drawn:
        counts = coverage(polygons, even_odd, scale, margin, cell)
        for y in range(cell):
            for x in range(cell):
                value = round(min(counts[y][x], full) * 255 / full)
                if value > alpha[y][x]:
                    alpha[y][x] = value
    if max(max(row) for row in alpha) < 128:
        raise ValueError(f"{name}: rasterised to nothing")
    return alpha


def png(width, height, rows):
    """An 8-bit RGBA PNG from rows of RGBA bytes."""
    def chunk(kind, data):
        return (struct.pack(">I", len(data)) + kind + data
                + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF))

    raw = b"".join(bytes([0]) + bytes(row) for row in rows)
    signature = bytes([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
    return (signature + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def atlas(sources, cell):
    """(width, height, PNG bytes): white pixels, the icon in the alpha channel."""
    rows = math.ceil(len(sources) / COLUMNS)
    width, height = COLUMNS * cell, rows * cell
    alpha = [[0] * width for _ in range(height)]
    for index, (name, data) in enumerate(sources):
        icon = rasterise(data, name, cell)
        top, left = index // COLUMNS * cell, index % COLUMNS * cell
        for y in range(cell):
            alpha[top + y][left:left + cell] = icon[y]
    pixels = [[channel for value in row for channel in (255, 255, 255, value)] for row in alpha]
    return width, height, png(width, height, pixels)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--download-dir", type=Path, default=None,
                        help="where downloads land before they are verified (default: a new temporary folder)")
    parser.add_argument("--offline", action="store_true",
                        help="rebuild atlases and manifest from the committed sources; no network")
    args = parser.parse_args()
    known = {}
    if MANIFEST.exists():
        known = {entry["path"]: entry for entry in json.loads(MANIFEST.read_text(encoding="utf-8"))["files"]}
    downloads = None
    if not args.offline:
        downloads = args.download_dir or Path(tempfile.mkdtemp(prefix="ui-icons-"))
        downloads = downloads.resolve()
        if Path(__file__).resolve().parent in (downloads, *downloads.parents):
            raise SystemExit("The download directory must be outside the script directory.")
        downloads.mkdir(parents=True, exist_ok=True)
        if any(downloads.iterdir()):
            raise SystemExit(f"{downloads} is not empty; give a new directory.")
    files, sources, licence_lines = [], [], {}

    def take(set_name, upstream, relative):
        info = SETS[set_name]
        url = f"{info['raw']}{info['commit']}/{upstream}"
        target = ASSETS / relative
        if args.offline:
            data = target.read_bytes()
        else:
            data = fetch(url)
            staged = downloads / set_name / Path(upstream).name
            staged.parent.mkdir(parents=True, exist_ok=True)
            staged.write_bytes(data)
        digest = hashlib.sha256(data).hexdigest()
        if relative in known and known[relative]["sha256"] != digest:
            raise SystemExit(f"{relative} differs from the manifest; review upstream before updating.")
        if target.exists() and target.read_bytes() != data:
            raise SystemExit(f"{relative} differs from the committed file; review before replacing.")
        files.append(dict(path=relative, url=url, source=info["source"], author=info["author"],
                          license="CC0-1.0", bytes=len(data), sha256=digest))
        return data, target

    # Licences first: nothing else is kept unless both are CC0.
    staged_licences = []
    for set_name, info in SETS.items():
        data, target = take(set_name, info["license_file"], info["license_path"])
        licence_lines[set_name] = require_cc0(set_name, data.decode("utf-8"))
        staged_licences.append((target, data))
    staged_sources = []
    for set_name, info in SETS.items():
        for icon in info["icons"]:
            data, target = take(set_name, f"icons/{icon}.svg", f"ui/icons/src/{set_name}/{icon}.svg")
            sources.append((f"{set_name}/{icon}", data))
            staged_sources.append((target, data))
    # Rasterise before anything is written, so a bad source leaves the tree as it was.
    sheets = {cell: atlas(sources, cell) for cell in (32, 64)}
    for target, data in staged_licences + staged_sources:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    atlases = []
    for cell, (width, height, data) in sheets.items():
        (ICONS / f"icons-{cell}.png").write_bytes(data)
        atlases.append(dict(path=f"ui/icons/icons-{cell}.png", cell=cell, columns=COLUMNS,
                            width=width, height=height, bytes=len(data),
                            sha256=hashlib.sha256(data).hexdigest(),
                            change="rasterised to white on transparent by scripts/fetch_ui_icons.py"))
    manifest = dict(
        version=1,
        sets={name: dict(source=info["source"], version=info["version"], commit=info["commit"],
                         author=info["author"], license="CC0-1.0", license_line=licence_lines[name])
              for name, info in SETS.items()},
        atlases=atlases,
        cells={name: index for index, (name, _) in enumerate(sources)},
        files=files,
    )
    MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    total = sum(entry["bytes"] for entry in files) + sum(entry["bytes"] for entry in atlases)
    print(json.dumps(dict(ok=True, icons=len(sources), licence_lines=licence_lines,
                          download_dir=str(downloads) if downloads else None,
                          bytes_total=total + MANIFEST.stat().st_size), indent=2))
    if downloads and args.download_dir is None:
        shutil.rmtree(downloads, ignore_errors=True)


if __name__ == "__main__":
    main()
