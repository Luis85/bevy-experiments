"""Field luminance gates on review captures (design-vfx-lighting section 12, W5-09).

Standard library only. Run: python -I -B scripts/measure_field_luminance.py <command> ...

Every PNG is read with its capture sidecar: `<capture>.json` beside a still
(written by LASTLIGHT_REVIEW_CAPTURE). The sidecar gives the field as
`arena_rect` in points and the scales (`ui_scale`, `windows_scale`); the field
in pixels is their product.

A capture_motion export has no sidecar of its own: its manifest.json records
no arena_rect. For `motion` and `storm`, --sidecar is therefore the
capture_review sidecar `<mode>/<scene>[-small].json` of the same scene,
viewport and mode (normal or reduced) as the export; a Battle fixture's camera
fit does not move between the still and the frames. When the export's
manifest.json lies beside the first frame, its scene, reduced_effects, scale
and windows_scale must match the sidecar's scene, reduced_effects, ui_scale
and windows_scale, or the evidence is invalid.
Luminance is WCAG 2.x relative luminance (sRGB decode, 0.2126/0.7152/0.0722),
averaged over the field inset by 4% on every side.

Commands
  mean PNG...                         print the field mean of each still
  ratio NIGHT.png DAWN.png            night gate: mean(night) / mean(dawn) <= --max (0.70);
                                      with --model R also |ratio - R| <= --tolerance (0.03)
  motion --sidecar JSON FRAME.png...  reduced MOTION gate over consecutive frames:
                                      |delta mean| < --max-mean-delta (0.01) and no
                                      --block (32) px block delta > --max-block-delta (0.1)
  storm --sidecar JSON FRAME.png...   storm MOTION gate: frame delta < --max-frame-delta (0.2)
                                      and flash peaks at least --min-peak-gap (240) frames apart

Exit codes: 0 pass, 1 gate failed, 2 invalid evidence (unreadable PNG or sidecar).
"""
import argparse
import json
import struct
import sys
import zlib
from pathlib import Path

INSET = 0.04
#: A frame is a flash peak when its mean rises by at least this over the previous frame.
PEAK_RISE = 0.02


class Invalid(Exception):
    """Evidence that cannot be measured."""


def _linear(channel):
    c = channel / 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


_LINEAR = [_linear(v) for v in range(256)]


def relative_luminance(r, g, b):
    return 0.2126 * _LINEAR[r] + 0.7152 * _LINEAR[g] + 0.0722 * _LINEAR[b]


def read_png(path):
    """(width, height, rows) of an 8-bit RGB or RGBA non-interlaced PNG; rows are bytes of RGB(A)."""
    data = Path(path).read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise Invalid(f"{path}: not a PNG")
    at, header, idat = 8, None, []
    while at + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[at:at + 8])
        body = data[at + 8:at + 8 + length]
        at += 12 + length
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    if header is None:
        raise Invalid(f"{path}: no IHDR")
    width, height, depth, colour, _, _, interlace = header
    if depth != 8 or colour not in (2, 6) or interlace != 0:
        raise Invalid(f"{path}: only 8-bit RGB/RGBA non-interlaced PNGs are supported")
    bpp = 3 if colour == 2 else 4
    stride = width * bpp
    try:
        raw = zlib.decompress(b"".join(idat))
    except zlib.error as error:
        raise Invalid(f"{path}: {error}") from error
    if len(raw) != height * (stride + 1):
        raise Invalid(f"{path}: truncated image data")
    rows, previous = [], bytes(stride)
    for y in range(height):
        start = y * (stride + 1)
        kind, line = raw[start], bytearray(raw[start + 1:start + 1 + stride])
        if kind == 1:
            for i in range(bpp, stride):
                line[i] = (line[i] + line[i - bpp]) & 255
        elif kind == 2:
            for i in range(stride):
                line[i] = (line[i] + previous[i]) & 255
        elif kind == 3:
            for i in range(stride):
                left = line[i - bpp] if i >= bpp else 0
                line[i] = (line[i] + ((left + previous[i]) >> 1)) & 255
        elif kind == 4:
            for i in range(stride):
                a = line[i - bpp] if i >= bpp else 0
                b = previous[i]
                c = previous[i - bpp] if i >= bpp else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        elif kind != 0:
            raise Invalid(f"{path}: unknown PNG filter {kind}")
        previous = bytes(line)
        rows.append(previous)
    return width, height, rows, bpp


def read_sidecar(path):
    try:
        facts = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise Invalid(f"{path}: {error}") from error
    if isinstance(facts, dict) and "frames" in facts and not facts.get("arena_rect"):
        raise Invalid(f"{path}: a capture_motion manifest has no arena_rect; pass the "
                      "capture_review sidecar <mode>/<scene>[-small].json of the same scene, "
                      "viewport and mode as --sidecar")
    if not isinstance(facts, dict) or not facts.get("arena_rect"):
        raise Invalid(f"{path}: no arena_rect (not a battle capture)")
    return facts


#: (manifest key, sidecar key) pairs a motion export and its still sidecar must agree on.
_EXPORT_FACTS = (("scene", "scene"), ("reduced_effects", "reduced_effects"),
                 ("scale", "ui_scale"), ("windows_scale", "windows_scale"))


def motion_sidecar(frames, sidecar):
    """The still sidecar of a motion export, checked against the export's manifest.json.

    The manifest is read from beside the first frame when it is there; a key
    either file leaves out (or sets to null) is not compared.
    """
    facts = read_sidecar(sidecar)
    if not frames:
        return facts
    manifest_path = Path(frames[0]).parent / "manifest.json"
    if not manifest_path.is_file() or manifest_path.resolve() == Path(sidecar).resolve():
        return facts
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise Invalid(f"{manifest_path}: {error}") from error
    if not isinstance(manifest, dict):
        raise Invalid(f"{manifest_path}: not a capture_motion manifest")
    for export_key, still_key in _EXPORT_FACTS:
        export_value, still_value = manifest.get(export_key), facts.get(still_key)
        if export_value is None or still_value is None:
            continue
        if isinstance(export_value, (int, float)) and not isinstance(export_value, bool):
            agree = (isinstance(still_value, (int, float)) and not isinstance(still_value, bool)
                     and abs(float(export_value) - float(still_value)) <= 1e-3)
        else:
            agree = export_value == still_value
        if not agree:
            raise Invalid(f"{sidecar}: {still_key} {still_value!r} does not match the export's "
                          f"{export_key} {export_value!r} in {manifest_path}; pass the "
                          "capture_review sidecar of the same scene, viewport and mode")
    return facts


def field_px(facts, width, height):
    """The field in pixels (left, top, right, bottom), inset by 4% and clamped to the image."""
    scale = float(facts.get("ui_scale") or 1.0) * float(facts.get("windows_scale") or 1.0)
    left, top, right, bottom = (float(v) * scale for v in facts["arena_rect"])
    dx, dy = (right - left) * INSET, (bottom - top) * INSET
    box = (max(0, round(left + dx)), max(0, round(top + dy)),
           min(width, round(right - dx)), min(height, round(bottom - dy)))
    if box[2] <= box[0] or box[3] <= box[1]:
        raise Invalid(f"arena_rect {facts['arena_rect']} is outside the {width}x{height} image")
    return box


def luminance_rows(path, facts):
    """Per-pixel luminance of the inset field, as a list of rows."""
    width, height, rows, bpp = read_png(path)
    left, top, right, bottom = field_px(facts, width, height)
    out = []
    for y in range(top, bottom):
        line = rows[y]
        out.append([relative_luminance(line[x * bpp], line[x * bpp + 1], line[x * bpp + 2])
                    for x in range(left, right)])
    return out


def mean(grid):
    return sum(map(sum, grid)) / (len(grid) * len(grid[0]))


def block_means(grid, block):
    """Means of the full block x block tiles of `grid`, row by row."""
    out = []
    for y in range(0, len(grid) - block + 1, block):
        for x in range(0, len(grid[0]) - block + 1, block):
            out.append(sum(sum(row[x:x + block]) for row in grid[y:y + block]) / (block * block))
    return out


def still_mean(png, sidecar=None):
    facts = read_sidecar(sidecar or Path(png).with_suffix(".json"))
    return mean(luminance_rows(png, facts)), facts


def measure_ratio(night, dawn, maximum=0.70, model=None, tolerance=0.03):
    night_mean, night_facts = still_mean(night)
    dawn_mean, dawn_facts = still_mean(dawn)
    failures = []
    for name, facts, wanted in (("night", night_facts, "night"), ("dawn", dawn_facts, "dawn")):
        lighting = facts.get("lighting")
        if lighting is not None and lighting != wanted:
            raise Invalid(f"the {name} capture's sidecar says lighting {lighting!r}")
    if dawn_mean <= 0:
        raise Invalid("the dawn field is black")
    ratio = night_mean / dawn_mean
    if ratio > maximum:
        failures.append(f"night/dawn {ratio:.3f} > {maximum}")
    if model is not None and abs(ratio - model) > tolerance:
        failures.append(f"night/dawn {ratio:.3f} is not within {tolerance} of the model {model}")
    return {"night_mean": night_mean, "dawn_mean": dawn_mean, "ratio": ratio, "failures": failures}


def measure_motion(frames, sidecar, max_mean_delta=0.01, max_block_delta=0.1, block=32):
    facts = motion_sidecar(frames, sidecar)
    if len(frames) < 2:
        raise Invalid("a motion gate needs at least two frames")
    means, worst_mean, worst_block, failures = [], 0.0, 0.0, []
    previous = None
    for index, frame in enumerate(frames):
        grid = luminance_rows(frame, facts)
        blocks = block_means(grid, block)
        means.append(mean(grid))
        if previous is not None:
            if len(previous) != len(blocks):
                raise Invalid(f"{frame}: frame size changed")
            delta = abs(means[-1] - means[-2])
            tile = max((abs(a - b) for a, b in zip(blocks, previous)), default=0.0)
            worst_mean, worst_block = max(worst_mean, delta), max(worst_block, tile)
            if delta >= max_mean_delta:
                failures.append(f"frame {index}: mean delta {delta:.4f} >= {max_mean_delta}")
            if tile > max_block_delta:
                failures.append(f"frame {index}: block delta {tile:.4f} > {max_block_delta}")
        previous = blocks
    return {"means": means, "max_mean_delta": worst_mean, "max_block_delta": worst_block,
            "failures": failures}


def measure_storm(frames, sidecar, max_frame_delta=0.2, min_peak_gap=240):
    facts = motion_sidecar(frames, sidecar)
    if len(frames) < 2:
        raise Invalid("a motion gate needs at least two frames")
    means = [mean(luminance_rows(frame, facts)) for frame in frames]
    failures, peaks, worst = [], [], 0.0
    for index in range(1, len(means)):
        delta = means[index] - means[index - 1]
        worst = max(worst, abs(delta))
        if abs(delta) >= max_frame_delta:
            failures.append(f"frame {index}: delta {abs(delta):.4f} >= {max_frame_delta}")
        # One peak per flash: a rise that does not continue a rise.
        rising_before = index >= 2 and means[index - 1] - means[index - 2] >= PEAK_RISE
        if delta >= PEAK_RISE and not rising_before:
            peaks.append(index)
    for a, b in zip(peaks, peaks[1:]):
        if b - a < min_peak_gap:
            failures.append(f"flash peaks at frames {a} and {b} are under {min_peak_gap} apart")
    return {"means": means, "max_frame_delta": worst, "peaks": peaks, "failures": failures}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    c = commands.add_parser("mean")
    c.add_argument("png", nargs="+")
    c = commands.add_parser("ratio")
    c.add_argument("night")
    c.add_argument("dawn")
    c.add_argument("--max", type=float, default=0.70)
    c.add_argument("--model", type=float)
    c.add_argument("--tolerance", type=float, default=0.03)
    c = commands.add_parser("motion")
    c.add_argument("--sidecar", required=True)
    c.add_argument("--max-mean-delta", type=float, default=0.01)
    c.add_argument("--max-block-delta", type=float, default=0.1)
    c.add_argument("--block", type=int, default=32)
    c.add_argument("frames", nargs="+")
    c = commands.add_parser("storm")
    c.add_argument("--sidecar", required=True)
    c.add_argument("--max-frame-delta", type=float, default=0.2)
    c.add_argument("--min-peak-gap", type=int, default=240)
    c.add_argument("frames", nargs="+")
    args = parser.parse_args(argv)
    try:
        if args.command == "mean":
            for png in args.png:
                value, facts = still_mean(png)
                print(f"{png}\t{value:.4f}\t{facts.get('lighting')}")
            return 0
        if args.command == "ratio":
            result = measure_ratio(args.night, args.dawn, args.max, args.model, args.tolerance)
        elif args.command == "motion":
            result = measure_motion(args.frames, args.sidecar, args.max_mean_delta,
                                    args.max_block_delta, args.block)
        else:
            result = measure_storm(args.frames, args.sidecar, args.max_frame_delta,
                                   args.min_peak_gap)
    except Invalid as error:
        print(f"INVALID: {error}", file=sys.stderr)
        return 2
    failures = result.pop("failures")
    result.pop("means", None)
    print(json.dumps(result, sort_keys=True))
    for failure in failures:
        print(f"FAIL: {failure}", file=sys.stderr)
    print("PASS" if not failures else "FAIL")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
