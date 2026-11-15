"""Self-tests for scripts/compare_captures.py (W7-11, design-qa-captures 1.12) on throwaway capture runs:
a wave-0 style before folder (PNGs only), its supplement and an after run with captures.json and
scenes.tsv.

Run: python -I -B scripts/tests/test_compare_captures.py
"""
from pathlib import Path
import contextlib
import hashlib
import importlib.util
import io
import json
import struct
import tempfile
import unittest
import zlib

_SPEC = importlib.util.spec_from_file_location(
    "compare_captures", Path(__file__).resolve().parent.parent / "compare_captures.py")
compare_captures = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(compare_captures)

SCENES_HEADER = "name\tkind\tmotion\tperf\tpractice\tprimary\tbefore_proxy\tsince_wave\n"


def png_bytes(width: int, height: int, payload: bytes) -> bytes:
    """A PNG signature and IHDR (all compare_captures reads), then `payload` so files differ."""
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    chunk = struct.pack(">I", len(ihdr)) + b"IHDR" + ihdr + struct.pack(">I", zlib.crc32(b"IHDR" + ihdr))
    return b"\x89PNG\r\n\x1a\n" + chunk + payload


class CompareCapturesTests(unittest.TestCase):
    def setUp(self):
        self._directory = tempfile.TemporaryDirectory()
        self.root = Path(self._directory.name).resolve()
        self.before = self.root / "production-before"
        self.after = self.root / "production-after/final"
        self.entries = []

    def tearDown(self):
        self._directory.cleanup()

    def png(self, folder: Path, mode: str, stem: str, payload: bytes, scale: float = 2.0) -> bytes:
        width, height = (1050, 700) if stem.endswith("-small") else (1440, 900)
        data = png_bytes(int(width * scale), int(height * scale), payload)
        path = folder / mode / f"{stem}.png"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return data

    def after_capture(self, mode: str, scene: str, size: str, payload: bytes, error=None,
                      write: bool = True) -> None:
        stem = scene + ("-small" if size == "small" else "")
        data = self.png(self.after, mode, stem, payload) if write else None
        self.entries.append({
            "scene": scene, "mode": mode, "size": size, "file": f"{mode}/{stem}.png",
            "sha256": hashlib.sha256(data).hexdigest() if data else None, "error": error,
        })

    def write_after(self, scale=2, proxies: dict | None = None) -> None:
        self.after.mkdir(parents=True, exist_ok=True)
        (self.after / "captures.json").write_text(json.dumps(
            {"version": 1, "windows_scale": scale, "captures": self.entries}), encoding="utf-8")
        lines = [SCENES_HEADER] + [f"{name}\tStill\tfalse\tfalse\tfalse\t\t{proxy}\t1\n"
                                   for name, proxy in (proxies or {}).items()]
        (self.after / "scenes.tsv").write_text("".join(lines), encoding="utf-8")

    def run_main(self, *extra: str) -> tuple[int, str]:
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = compare_captures.main([str(self.before), str(self.after), *extra])
        return code, out.getvalue()

    def statuses(self) -> dict:
        rows = compare_captures.pair_runs(self.before, self.after)
        return {(row["mode"], row["scene"], row["size"]): (row["before_scene"], row["status"]) for row in rows}

    def test_pairs_by_name_and_before_proxy_and_reports_each_status(self):
        self.png(self.before, "normal", "battle", b"battle v0")
        self.png(self.before, "normal", "operations-small", b"operations v0")
        self.png(self.before, "normal", "menu", b"menu")
        self.after_capture("normal", "battle", "large", b"battle v1")
        self.after_capture("normal", "operations-observer", "small", b"observer")
        self.after_capture("normal", "menu", "large", b"menu")
        self.after_capture("normal", "camp-home", "large", b"camp")
        self.write_after(proxies={"operations-observer": "operations"})
        self.assertEqual(self.statuses(), {
            ("normal", "battle", "large"): ("battle", "changed"),
            ("normal", "operations-observer", "small"): ("operations", "changed"),
            ("normal", "menu", "large"): ("menu", "unchanged"),
            ("normal", "camp-home", "large"): ("", "missing"),
        })
        code, out = self.run_main()
        self.assertEqual(code, 0, out)
        self.assertIn("PASS: 4 after captures: 2 changed, 1 unchanged, 1 missing, 0 failed", out)

    def test_wave0_before_scale_is_inferred_from_png_width_and_must_equal_the_after_scale(self):
        self.png(self.before, "normal", "battle", b"v0", scale=2.0)
        self.after_capture("normal", "battle", "large", b"v1")
        self.write_after(scale=1.5)
        code, out = self.run_main()
        self.assertEqual(code, 2, out)
        self.assertIn("windows_scale differs: before 2, after 1.5", out)
        self.write_after(scale=2)
        self.assertEqual(self.run_main()[0], 0)

    def test_a_before_run_whose_stills_disagree_on_scale_is_refused(self):
        self.png(self.before, "normal", "battle", b"v0", scale=2.0)
        self.png(self.before, "normal", "menu", b"v0", scale=1.0)
        self.after_capture("normal", "battle", "large", b"v1")
        self.write_after()
        code, out = self.run_main()
        self.assertEqual(code, 2)
        self.assertIn("before stills disagree on windows_scale", out)

    def test_the_supplement_is_merged_and_wins_over_the_wave0_still(self):
        self.png(self.before, "normal", "progressed-battle", b"broken v0")
        supplement = self.before / "supplement"
        self.png(supplement, "normal", "progressed-battle", b"fixed v0")
        self.png(supplement, "normal", "journey-victory", b"victory v0")
        (supplement / "captures.json").write_text(json.dumps({"version": 1, "windows_scale": 2, "captures": []}),
                                                  encoding="utf-8")
        self.after_capture("normal", "progressed-battle", "large", b"fixed v0")
        self.after_capture("normal", "journey-victory", "large", b"victory v1")
        self.write_after()
        self.assertEqual(self.statuses(), {
            ("normal", "progressed-battle", "large"): ("progressed-battle", "unchanged"),
            ("normal", "journey-victory", "large"): ("journey-victory", "changed"),
        })

    def test_a_failed_after_capture_exits_1(self):
        self.png(self.before, "normal", "battle", b"v0")
        self.after_capture("normal", "battle", "large", b"v1")
        self.after_capture("reduced", "battle", "large", b"", error="exit code 3", write=False)
        self.write_after()
        self.assertEqual(self.statuses()[("reduced", "battle", "large")], ("", "failed"))
        code, out = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("FAIL: reduced/battle.png: exit code 3", out)

    def test_reduced_stills_report_whether_they_equal_their_normal_twin(self):
        self.png(self.before, "normal", "menu", b"menu")
        self.png(self.before, "reduced", "menu", b"menu")
        self.after_capture("normal", "menu", "large", b"menu")
        self.after_capture("reduced", "menu", "large", b"menu")
        self.after_capture("normal", "storm", "large", b"storm flash")
        self.after_capture("reduced", "storm", "large", b"storm static")
        self.write_after()
        rows = {(row["mode"], row["scene"]): row["identical_to_normal"]
                for row in compare_captures.pair_runs(self.before, self.after)}
        self.assertEqual(rows, {("normal", "menu"): None, ("reduced", "menu"): True,
                                ("normal", "storm"): None, ("reduced", "storm"): False})
        code, out = self.run_main()
        self.assertIn("reduced identical to normal: 1 of 2", out)

    def test_html_writes_a_side_by_side_sheet_with_relative_paths(self):
        self.png(self.before, "normal", "battle", b"v0")
        self.after_capture("normal", "battle", "large", b"v1")
        self.after_capture("normal", "a&b", "large", b"odd")
        self.write_after()
        sheet = self.root / "sheets/contact.html"
        code, out = self.run_main("--html", str(sheet))
        self.assertEqual(code, 0, out)
        html = sheet.read_text(encoding="utf-8")
        self.assertIn('src="../production-before/normal/battle.png"', html)
        self.assertIn('src="../production-after/final/normal/battle.png"', html)
        self.assertIn("a&amp;b", html)
        self.assertNotIn("a&b", html)
        self.assertLess(html.index("battle"), html.index("a&amp;b"), "changed pairs first, then missing")

    def test_an_after_run_without_captures_json_is_invalid(self):
        self.after.mkdir(parents=True)
        code, out = self.run_main()
        self.assertEqual(code, 2)
        self.assertIn("captures.json", out)


if __name__ == "__main__":
    unittest.main()
