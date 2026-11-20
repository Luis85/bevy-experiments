"""Contact sheet of the residents framed by capture_view_checks.py.

  python scripts/playtest/pose_sheet.py [--shots target/npc-out/view-checks]

Crops the middle of every `pose-<kind>-30m-a/b` and `pose-<kind>-14m` capture
(the camera is centred on the resident) and lays them side by side, the 30 m
crops enlarged four times with nearest-neighbour sampling so no detail is
invented. Writes `pose-sheet.png` beside the captures. Needs Pillow.
"""
import argparse
from pathlib import Path

from PIL import Image, ImageDraw

KINDS = ("idle", "carry", "chop", "hammer", "farm")
CELL = 320


def crop(path, half, centre=(640, 350)):
    image = Image.open(path).convert("RGB")
    box = (centre[0] - half, centre[1] - half, centre[0] + half, centre[1] + half)
    return image.crop(box).resize((CELL, CELL), Image.NEAREST)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--shots", type=Path, default=Path("target/npc-out/view-checks"))
    args = parser.parse_args()
    columns = (("30m-a", 40, "30 m, x4"), ("30m-b", 40, "30 m, 0.5 s later"), ("14m", 80, "14 m, x2"))
    kinds = [kind for kind in KINDS if (args.shots / f"pose-{kind}-30m-a-1280x720.png").is_file()]
    sheet = Image.new("RGB", (CELL * len(columns) + 90, (CELL + 4) * len(kinds) + 24), (16, 18, 22))
    draw = ImageDraw.Draw(sheet)
    for column, (_, _, title) in enumerate(columns):
        draw.text((94 + column * CELL, 6), title, fill=(235, 238, 228))
    for row, kind in enumerate(kinds):
        top = 24 + row * (CELL + 4)
        draw.text((8, top + CELL // 2), kind, fill=(242, 204, 115))
        for column, (suffix, half, _) in enumerate(columns):
            path = args.shots / f"pose-{kind}-{suffix}-1280x720.png"
            if path.is_file():
                sheet.paste(crop(path, half), (90 + column * CELL, top))
    out = args.shots / "pose-sheet.png"
    sheet.save(out)
    print(out, kinds)


if __name__ == "__main__":
    main()
