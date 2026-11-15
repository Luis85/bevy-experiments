"""Pair a capture_review.ps1 run with the before set (W7-11, design-qa-captures 1.12).

Standard library only. Run:
  python -I -B scripts/compare_captures.py target/production-before target/production-after/<run> [--html out.html]

<after> is a capture_review.ps1 run: captures.json (version 1, read for each capture's file, sha256 and
error, and the run's windows_scale) and scenes.tsv (the binary's --list-scenes, read for before_proxy).
<before> is the wave-0 before set: its own captures.json when it has one, else every <mode>/<scene>[-small].png
under it. Its supplement (<before>/supplement, or --supplement DIR) is merged in and wins where both hold a
still. A before run without captures.json records no windows_scale, so it is inferred from each PNG's width
(1440 or 1050 times the scale; the wave-0 stills read 2).

Each after capture is paired by (mode, scene, size): with the before still of the same name, else with that
of its before_proxy (operations-observer pairs with operations). A pair is `changed` or `unchanged` (the
same SHA-256), `missing` (no before still) or `failed` (the after capture has an error or no PNG). A reduced
capture also reports whether it is byte-identical to its normal twin (identical_to_normal). Pairs are
comparable only at equal windows_scale: a run whose stills disagree, or a before and after at different
scales, is refused.

--html writes a side-by-side contact sheet (failed, changed, missing, then unchanged pairs) whose image
paths are relative to the sheet.

Exit codes: 0 pass, 1 an after capture failed, 2 invalid evidence (no captures.json, mismatched scales).
"""
import argparse
import hashlib
import html
import json
import os
import struct
import sys
import urllib.parse
from pathlib import Path

MODES = ("normal", "reduced")
WIDTHS = {"large": 1440, "small": 1050}
ORDER = {"failed": 0, "changed": 1, "missing": 2, "unchanged": 3}


class InvalidEvidence(Exception):
    pass


def png_width(path: Path) -> int | None:
    with open(path, "rb") as handle:
        header = handle.read(24)
    if len(header) < 24 or header[:8] != b"\x89PNG\r\n\x1a\n" or header[12:16] != b"IHDR":
        return None
    return struct.unpack(">I", header[16:20])[0]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def read_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, ValueError) as error:
        raise InvalidEvidence(f"cannot read {path}: {error}") from error


def format_scale(scale: float) -> str:
    return f"{scale:g}"


def scan_run(folder: Path) -> tuple[dict, float | None]:
    """{(mode, scene, size): path} of a before folder, and its windows_scale."""
    stills = {}
    index = folder / "captures.json"
    recorded = None
    if index.is_file():
        report = read_json(index)
        recorded = report.get("windows_scale")
        for entry in report.get("captures", []):
            path = folder / entry["file"]
            if entry.get("error") is None and path.is_file():
                stills[(entry["mode"], entry["scene"], entry["size"])] = path
    for mode in MODES:
        for path in sorted((folder / mode).glob("*.png")) if (folder / mode).is_dir() else []:
            stem = path.stem
            size = "small" if stem.endswith("-small") else "large"
            scene = stem[: -len("-small")] if size == "small" else stem
            stills.setdefault((mode, scene, size), path)
    if recorded is not None:
        return stills, float(recorded)
    inferred = set()
    for (_, _, size), path in stills.items():
        width = png_width(path)
        if width:
            inferred.add(round(width / WIDTHS[size], 3))
    if len(inferred) > 1:
        raise InvalidEvidence(f"before stills disagree on windows_scale in {folder}: "
                              + ", ".join(format_scale(s) for s in sorted(inferred)))
    return stills, (inferred.pop() if inferred else None)


def before_proxies(after: Path) -> dict:
    listing = after / "scenes.tsv"
    if not listing.is_file():
        return {}
    lines = listing.read_text(encoding="utf-8-sig").splitlines()
    header = lines[0].split("\t") if lines else []
    if "name" not in header or "before_proxy" not in header:
        return {}
    name, proxy = header.index("name"), header.index("before_proxy")
    proxies = {}
    for line in lines[1:]:
        cells = line.split("\t")
        if len(cells) > max(name, proxy) and cells[proxy]:
            proxies[cells[name]] = cells[proxy]
    return proxies


def pair_runs(before: Path, after: Path, supplement: Path | None = None) -> list[dict]:
    """One row per after capture, in capture order. Raises InvalidEvidence."""
    index = after / "captures.json"
    if not index.is_file():
        raise InvalidEvidence(f"{index} is missing: <after> must be a capture_review.ps1 run")
    report = read_json(index)
    after_scale = report.get("windows_scale")
    stills, before_scale = scan_run(before)
    supplement = supplement if supplement is not None else before / "supplement"
    if supplement.is_dir():
        extra, extra_scale = scan_run(supplement)
        if extra_scale is not None and before_scale is not None and extra_scale != before_scale:
            raise InvalidEvidence(f"before stills disagree on windows_scale: {before} "
                                  f"{format_scale(before_scale)}, {supplement} {format_scale(extra_scale)}")
        stills.update(extra)
        before_scale = before_scale if before_scale is not None else extra_scale
    if after_scale is not None and before_scale is not None and float(after_scale) != before_scale:
        raise InvalidEvidence(f"windows_scale differs: before {format_scale(before_scale)}, after "
                              f"{format_scale(float(after_scale))}; pairs are comparable only at equal scale")
    proxies = before_proxies(after)
    entries = report.get("captures", [])
    hashes = {}
    for entry in entries:
        path = after / entry["file"]
        if entry.get("error") is None and path.is_file():
            hashes[(entry["mode"], entry["scene"], entry["size"])] = entry.get("sha256") or sha256(path)
    rows = []
    for entry in entries:
        key = (entry["mode"], entry["scene"], entry["size"])
        path = after / entry["file"]
        row = {"mode": key[0], "scene": key[1], "size": key[2], "after": path, "before": None,
               "before_scene": "", "status": "failed", "identical_to_normal": None,
               "error": entry.get("error") or (None if path.is_file() else "no PNG")}
        if key[0] == "reduced" and key in hashes and ("normal", key[1], key[2]) in hashes:
            row["identical_to_normal"] = hashes[key] == hashes[("normal", key[1], key[2])]
        if row["error"] is None:
            for scene in (key[1], proxies.get(key[1])):
                if scene and (key[0], scene, key[2]) in stills:
                    row["before"] = stills[(key[0], scene, key[2])]
                    row["before_scene"] = scene
                    break
            if row["before"] is None:
                row["status"] = "missing"
            else:
                row["status"] = "unchanged" if sha256(row["before"]) == hashes[key] else "changed"
        rows.append(row)
    return rows


def still_name(row: dict) -> str:
    return f"{row['mode']}/{row['scene']}{'-small' if row['size'] == 'small' else ''}.png"


def write_html(rows: list[dict], out: Path) -> None:
    out.parent.mkdir(parents=True, exist_ok=True)

    def image(path) -> str:
        if path is None:
            return "<td class=none>no still</td>"
        relative = Path(os.path.relpath(Path(path).resolve(), out.parent.resolve())).as_posix()
        source = html.escape(urllib.parse.quote(relative), quote=True)
        return f'<td><a href="{source}"><img src="{source}" loading="lazy" alt=""></a></td>'

    body = []
    for row in sorted(rows, key=lambda r: ORDER[r["status"]]):
        twin = {True: "identical to normal", False: "differs from normal", None: ""}[row["identical_to_normal"]]
        label = html.escape(still_name(row))
        before = html.escape(row["before_scene"] or "-")
        body.append(f"<tr class={row['status']}><th>{label}<br><small>{row['status']} · before {before}"
                    f"{' · ' + twin if twin else ''}</small></th>{image(row['before'])}"
                    f"{image(None if row['status'] == 'failed' else row['after'])}</tr>")
    out.write_text(
        "<!doctype html>\n<html lang=en><head><meta charset=utf-8><title>Capture pairs</title><style>"
        "body{font:14px sans-serif;background:#111;color:#ddd;margin:16px}"
        "table{border-collapse:collapse;width:100%}th,td{border:1px solid #333;padding:4px;vertical-align:top}"
        "th{text-align:left;width:16em}img{width:100%;height:auto}.changed th{color:#efb552}"
        ".failed th,.missing th{color:#ff7a6a}.none{color:#888}</style></head><body>\n"
        "<table><tr><th>Still</th><th>Before</th><th>After</th></tr>\n" + "\n".join(body)
        + "\n</table></body></html>\n", encoding="utf-8")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("before", type=Path, help="the before set (target/production-before)")
    parser.add_argument("after", type=Path, help="a capture_review.ps1 run")
    parser.add_argument("--supplement", type=Path, help="the supplemental before set (default <before>/supplement)")
    parser.add_argument("--html", type=Path, help="write a side-by-side contact sheet here")
    args = parser.parse_args(argv)
    try:
        rows = pair_runs(args.before, args.after, args.supplement)
    except InvalidEvidence as error:
        print(f"INVALID: {error}")
        return 2
    print("mode\tsize\tscene\tbefore\tstatus\tidentical_to_normal")
    for row in rows:
        twin = "" if row["identical_to_normal"] is None else str(row["identical_to_normal"]).lower()
        print(f"{row['mode']}\t{row['size']}\t{row['scene']}\t{row['before_scene'] or '-'}\t{row['status']}\t{twin}")
    if args.html:
        write_html(rows, args.html)
        print(f"Contact sheet: {args.html}")
    counts = {status: sum(1 for row in rows if row["status"] == status) for status in ORDER}
    reduced = [row for row in rows if row["identical_to_normal"] is not None]
    summary = (f"{len(rows)} after captures: {counts['changed']} changed, {counts['unchanged']} unchanged, "
               f"{counts['missing']} missing, {counts['failed']} failed; reduced identical to normal: "
               f"{sum(1 for row in reduced if row['identical_to_normal'])} of {len(reduced)}")
    failed = [row for row in rows if row["status"] == "failed"]
    for row in failed:
        print(f"FAIL: {still_name(row)}: {row['error']}")
    print(("FAIL: " if failed else "PASS: ") + summary)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
