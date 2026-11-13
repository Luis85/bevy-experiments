"""Self-tests for scripts/measure_field_luminance.py on generated PNGs.

Run: python -I -B scripts/tests/test_measure_field_luminance.py
"""
from pathlib import Path
import importlib.util
import json
import struct
import tempfile
import unittest
import zlib

_SPEC = importlib.util.spec_from_file_location(
    "measure_field_luminance",
    Path(__file__).resolve().parent.parent / "measure_field_luminance.py")
mfl = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(mfl)

W, H = 200, 120
#: The field in points; at ui_scale 1 and windows_scale 2 it is the 160x80 px centre.
ARENA = [10.0, 10.0, 90.0, 50.0]


def png(path, pixel, width=W, height=H, alpha=False, filter_kind=0):
    """A PNG whose pixel (x, y) is pixel(x, y) -> (r, g, b)."""
    bpp = 4 if alpha else 3
    raw, previous = bytearray(), bytes(width * bpp)
    for y in range(height):
        line = bytearray()
        for x in range(width):
            line += bytes(pixel(x, y)) + (b"\xff" if alpha else b"")
        if filter_kind == 2:
            coded = bytes((a - b) & 255 for a, b in zip(line, previous))
        elif filter_kind == 1:
            coded = bytes((line[i] - (line[i - bpp] if i >= bpp else 0)) & 255
                          for i in range(len(line)))
        else:
            coded = bytes(line)
        raw += bytes([filter_kind]) + coded
        previous = bytes(line)

    def chunk(kind, body):
        return (struct.pack(">I", len(body)) + kind + body
                + struct.pack(">I", zlib.crc32(kind + body)))

    header = struct.pack(">IIBBBBB", width, height, 8, 6 if alpha else 2, 0, 0, 0)
    Path(path).write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
                           + chunk(b"IDAT", zlib.compress(bytes(raw))) + chunk(b"IEND", b""))


def sidecar(path, lighting=None, arena=ARENA):
    facts = {"version": 1, "arena_rect": arena, "ui_scale": 1.0, "windows_scale": 2.0,
             "lighting": lighting, "storm": False, "reduced_effects": False}
    Path(path).write_text(json.dumps(facts), encoding="utf-8")


def grey(value):
    return lambda x, y: (value, value, value)


def field_only(value, outside=255):
    """`value` inside the field's pixels, `outside` elsewhere."""
    return lambda x, y: (value,) * 3 if 20 <= x < 180 and 20 <= y < 100 else (outside,) * 3


class Case(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def still(self, name, pixel, lighting=None, **kwargs):
        path = self.dir / f"{name}.png"
        png(path, pixel, **kwargs)
        sidecar(path.with_suffix(".json"), lighting)
        return str(path)

    def frames(self, pixels):
        out = []
        for index, pixel in enumerate(pixels):
            path = self.dir / f"frame-{index:04}.png"
            png(path, pixel)
            out.append(str(path))
        sidecar(self.dir / "motion.json")
        return out, str(self.dir / "motion.json")


class Luminance(Case):
    def test_relative_luminance_is_wcag(self):
        self.assertAlmostEqual(mfl.relative_luminance(255, 255, 255), 1.0, places=6)
        self.assertEqual(mfl.relative_luminance(0, 0, 0), 0.0)
        self.assertAlmostEqual(mfl.relative_luminance(255, 0, 0), 0.2126, places=6)
        self.assertAlmostEqual(mfl.relative_luminance(128, 128, 128), 0.21586, places=4)

    def test_mean_measures_only_the_inset_field(self):
        # White outside the field must not count: the sidecar rect scales by 2.
        value, _ = mfl.still_mean(self.still("a", field_only(128)))
        self.assertAlmostEqual(value, 0.21586, places=4)

    def test_every_supported_png_encoding_reads_the_same(self):
        pixel = lambda x, y: ((x * 3) & 255, (y * 5) & 255, (x + y) & 255)
        plain = mfl.still_mean(self.still("p", pixel))[0]
        for name, kwargs in (("sub", {"filter_kind": 1}), ("up", {"filter_kind": 2}),
                             ("rgba", {"alpha": True})):
            self.assertAlmostEqual(mfl.still_mean(self.still(name, pixel, **kwargs))[0], plain,
                                   places=9, msg=name)

    def test_missing_sidecar_or_field_is_invalid(self):
        path = self.dir / "bare.png"
        png(path, grey(10))
        with self.assertRaises(mfl.Invalid):
            mfl.still_mean(str(path))
        sidecar(path.with_suffix(".json"), arena=None)
        with self.assertRaises(mfl.Invalid):
            mfl.still_mean(str(path))
        self.assertEqual(mfl.main(["mean", str(path)]), 2)


class NightGate(Case):
    def test_night_at_most_70_percent_of_dawn_passes(self):
        night, dawn = self.still("n", grey(90), "night"), self.still("d", grey(128), "dawn")
        result = mfl.measure_ratio(night, dawn)
        self.assertLess(result["ratio"], 0.70)
        self.assertEqual(result["failures"], [])
        self.assertEqual(mfl.main(["ratio", night, dawn]), 0)

    def test_a_bright_night_fails(self):
        night, dawn = self.still("n", grey(120), "night"), self.still("d", grey(128), "dawn")
        self.assertEqual(mfl.main(["ratio", night, dawn]), 1)

    def test_capture_must_agree_with_the_model(self):
        night, dawn = self.still("n", grey(90), "night"), self.still("d", grey(128), "dawn")
        ratio = mfl.measure_ratio(night, dawn)["ratio"]
        self.assertEqual(mfl.main(["ratio", night, dawn, "--model", f"{ratio + 0.02}"]), 0)
        self.assertEqual(mfl.main(["ratio", night, dawn, "--model", f"{ratio - 0.05}"]), 1)

    def test_swapped_captures_are_invalid(self):
        night, dawn = self.still("n", grey(90), "night"), self.still("d", grey(128), "dawn")
        self.assertEqual(mfl.main(["ratio", dawn, night]), 2)


class MotionGate(Case):
    def test_a_steady_sequence_passes(self):
        frames, facts = self.frames([grey(60), grey(61), grey(60)])
        self.assertEqual(mfl.main(["motion", "--sidecar", facts, *frames]), 0)

    def test_a_whole_field_flash_fails_the_mean_delta(self):
        frames, facts = self.frames([grey(60), grey(110), grey(60)])
        result = mfl.measure_motion(frames, facts)
        self.assertGreater(result["max_mean_delta"], 0.01)
        self.assertEqual(mfl.main(["motion", "--sidecar", facts, *frames]), 1)

    def test_a_local_flash_fails_the_block_delta_only(self):
        # One 32 px tile inside the field turns white: the field mean barely moves.
        spark = lambda x, y: (255,) * 3 if 60 <= x < 92 and 40 <= y < 72 else (60,) * 3
        frames, facts = self.frames([grey(60), spark])
        result = mfl.measure_motion(frames, facts, max_mean_delta=1.0)
        self.assertGreater(result["max_block_delta"], 0.1)
        self.assertEqual(len(result["failures"]), 1)
        self.assertIn("block delta", result["failures"][0])

    def test_one_frame_is_invalid(self):
        frames, facts = self.frames([grey(60)])
        self.assertEqual(mfl.main(["motion", "--sidecar", facts, *frames]), 2)


class StormGate(Case):
    def sequence(self, flashes, length=12):
        return self.frames([grey(110 if index in flashes else 60) for index in range(length)])

    def test_spaced_soft_flashes_pass(self):
        frames, facts = self.sequence({2, 9})
        result = mfl.measure_storm(frames, facts, min_peak_gap=5)
        self.assertEqual(result["peaks"], [2, 9])
        self.assertEqual(result["failures"], [])

    def test_flashes_too_close_fail(self):
        frames, facts = self.sequence({2, 5})
        self.assertEqual(mfl.main(["storm", "--sidecar", facts, "--min-peak-gap", "5", *frames]), 1)

    def test_a_hard_flash_fails_the_frame_delta(self):
        frames, facts = self.frames([grey(40), grey(160), grey(40)])
        result = mfl.measure_storm(frames, facts)
        self.assertGreaterEqual(result["max_frame_delta"], 0.2)
        self.assertTrue(result["failures"])


if __name__ == "__main__":
    unittest.main()
