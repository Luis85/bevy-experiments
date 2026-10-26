#!/usr/bin/env python3
"""Scoped image regressions for the supplied meadow render-check scenarios.

Run: python3 tools/verify_captures.py validation/native
Requires Pillow. These color checks complement CPU/GPU state validation and
visual inspection; pixels do not measure individual visible-unit counts.
"""

import argparse
import json
from pathlib import Path

from PIL import Image


def inspect(path: Path, require_teams: bool) -> dict:
    with Image.open(path) as source:
        image = source.convert("RGB")
        width, height = image.size
        roi = (0, 180, width, height - 120)
        cropped = image.crop(roi)
        pixels = (
            cropped.get_flattened_data()
            if hasattr(cropped, "get_flattened_data")
            else cropped.getdata()
        )
        blue = red = meadow = 0
        for r, g, b in pixels:
            blue += b >= 120 and b > r * 1.25 and g > r * 1.15
            red += r >= 150 and r > g * 1.25 and r > b * 1.35
            meadow += r >= 90 and g >= r * 0.9 and r > b * 1.15 and g > b * 1.15
        return {
            "path": str(path),
            "roi": roi,
            "blue_family_pixels": blue,
            "red_family_pixels": red,
            "meadow_pixels": meadow,
            "passed": meadow >= 50_000 and (not require_teams or (blue >= 1_000 and red >= 1_000)),
        }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path, nargs="?", default=Path("validation/native"))
    args = parser.parse_args()
    cases = ["million"] + [
        f"categories/{category}"
        for category in (
            "creature", "vehicle", "building", "air_unit", "sea_unit",
            "underground_unit", "space_unit",
        )
    ]
    results = []
    for case in cases:
        path = args.root / case / "render-check.png"
        try:
            results.append(inspect(path, require_teams=case == "million"))
        except (OSError, ValueError) as error:
            results.append({"path": str(path), "passed": False, "error": str(error)})
    print(json.dumps(results, indent=2))
    return 0 if all(result["passed"] for result in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
