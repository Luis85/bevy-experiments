"""Self-tests for scripts/check_capture_findings.py (W7-11, design-qa-captures 1.12) on a throwaway
workspace: a plan file with a capture-findings ledger, evidence PNGs under target/production-after and a
docs/screenshots/production pair folder.

Run: python -I -B scripts/tests/test_check_capture_findings.py
"""
from pathlib import Path
import hashlib
import importlib.util
import io
import contextlib
import os
import tempfile
import unittest

_SPEC = importlib.util.spec_from_file_location(
    "check_capture_findings", Path(__file__).resolve().parent.parent / "check_capture_findings.py")
check_capture_findings = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(check_capture_findings)

HEADER = ("| ID | Wave | Scene | Size | Mode | Finding | Severity | Owner | Status | Fix commit | "
          "After evidence | Deferral |\n"
          "|---|---|---|---|---|---|---|---|---|---|---|---|\n")
EVIDENCE = "target/production-after/w9/normal/battle-small.png"
FRAMES = "target/production-after/w9/motion/boss-phase-normal-normal"


def row(cells: dict) -> str:
    base = {
        "id": "CF-9-01", "wave": "9", "scene": "battle", "size": "both", "mode": "both",
        "finding": "The banner covers a lane", "severity": "minor", "owner": "vfx-lighting",
        "status": "open", "commit": "", "evidence": "", "deferral": "",
    }
    base.update(cells)
    order = ["id", "wave", "scene", "size", "mode", "finding", "severity", "owner", "status",
             "commit", "evidence", "deferral"]
    return "| " + " | ".join(base[key] for key in order) + " |\n"


class CheckCaptureFindingsTests(unittest.TestCase):
    def setUp(self):
        self._directory = tempfile.TemporaryDirectory()
        self.root = Path(self._directory.name).resolve()
        self.evidence = self.write_png(EVIDENCE, b"after battle")
        self.frame = self.write_png(FRAMES + "/frame-0000.png", b"frame 0")
        self.write_png(FRAMES + "/frame-0022.png", b"frame 22")
        self.plan = self.root / "docs/plans/plan.md"
        self.plan.parent.mkdir(parents=True, exist_ok=True)
        self._saved_target = os.environ.pop("CARGO_TARGET_DIR", None)

    def tearDown(self):
        if self._saved_target is not None:
            os.environ["CARGO_TARGET_DIR"] = self._saved_target
        self._directory.cleanup()

    def write_png(self, relative: str, payload: bytes) -> str:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        data = b"\x89PNG\r\n\x1a\n" + payload
        path.write_bytes(data)
        return hashlib.sha256(data).hexdigest()[:12]

    def write_plan(self, rows: str, header: str = HEADER) -> None:
        self.plan.write_text(
            "# Plan\n\n## 10. Ledger\n\n<!-- capture-findings:begin -->\n" + header + rows
            + "<!-- capture-findings:end -->\n\nAfter the table.\n", encoding="utf-8")

    def check(self, final: bool = False) -> tuple[int, list[str]]:
        problems, counts = check_capture_findings.check(self.plan, self.root, final=final, git=False)
        return len(problems), problems

    def fixed(self, **cells) -> str:
        base = {"status": "fixed", "commit": "299470a", "evidence": f"{EVIDENCE}#{self.evidence}"}
        base.update(cells)
        return row(base)

    def make_pairs(self, size: int = 10) -> Path:
        folder = self.root / "docs/screenshots/production"
        folder.mkdir(parents=True, exist_ok=True)
        for name in ["before-battle-small.png", "after-battle-small.png"]:
            (folder / name).write_bytes(b"x" * size)
        for name in ["captures-before.json", "captures-after.json"]:
            (folder / name).write_text("{}", encoding="utf-8")
        return folder

    # Vocabulary and table shape.
    def test_a_well_formed_ledger_passes(self):
        self.write_plan(row({}) + self.fixed(id="CF-9-02")
                        + row({"id": "CF-9-03", "status": "deferred",
                               "deferral": "approved by user 2026-11-14"}))
        self.assertEqual(self.check(), (0, []))

    def test_a_plan_without_the_markers_fails(self):
        self.plan.write_text("# Plan\n\nNo ledger here.\n", encoding="utf-8")
        count, problems = self.check()
        self.assertEqual(count, 1)
        self.assertIn("capture-findings:begin", problems[0])

    def test_a_changed_header_fails(self):
        self.write_plan(row({}), header=HEADER.replace("| Owner |", "| Who |"))
        count, problems = self.check()
        self.assertEqual(count, 1)
        self.assertIn("header", problems[0])

    def test_every_vocabulary_column_is_checked(self):
        self.write_plan(row({"size": "1440x900"}) + row({"id": "CF-9-02", "size": "1050x700@1.35"})
                        + row({"id": "CF-9-03", "size": "1050x700"})
                        + row({"id": "CF-9-04", "mode": "dark"})
                        + row({"id": "CF-9-05", "severity": "critical"})
                        + row({"id": "CF-9-06", "status": "closed"}))
        count, problems = self.check()
        self.assertEqual(count, 4, problems)
        joined = "\n".join(problems)
        for word in ["CF-9-03: Size", "CF-9-04: Mode", "CF-9-05: Severity", "CF-9-06: Status"]:
            self.assertIn(word, joined)

    def test_ids_are_unique_well_formed_and_match_their_wave(self):
        self.write_plan(row({}) + row({}) + row({"id": "CF-9-1"}) + row({"id": "CF-8-04"})
                        + row({"id": "CF-9-05", "owner": "", "finding": ""}))
        count, problems = self.check()
        joined = "\n".join(problems)
        self.assertIn("CF-9-01: duplicate ID", joined)
        self.assertIn("CF-9-1: ID must read CF-<wave>-<nn>", joined)
        self.assertIn("CF-8-04: Wave 9 differs from the ID's wave 8", joined)
        self.assertIn("CF-9-05: no Owner", joined)
        self.assertIn("CF-9-05: no Finding", joined)
        self.assertEqual(count, 5, problems)

    def test_a_short_row_fails(self):
        self.write_plan("| CF-9-01 | 9 | battle | both | both | short |\n")
        count, problems = self.check()
        self.assertEqual(count, 1)
        self.assertIn("12 cells", problems[0])

    # Fixed rows.
    def test_a_fixed_row_needs_hex_commits(self):
        self.write_plan(self.fixed(commit="") + self.fixed(id="CF-9-02", commit="299470a, xyz1234")
                        + self.fixed(id="CF-9-03", commit="12ab")
                        + self.fixed(id="CF-9-04", commit="bbaa01c, d74a13e"))
        count, problems = self.check()
        joined = "\n".join(problems)
        self.assertIn("CF-9-01: a fixed row needs a Fix commit", joined)
        self.assertIn("CF-9-02: Fix commit 'xyz1234'", joined)
        self.assertIn("CF-9-03: Fix commit '12ab'", joined)
        self.assertEqual(count, 3, problems)

    def test_a_fixed_row_needs_existing_evidence_with_a_matching_hash(self):
        self.write_plan(self.fixed(evidence="")
                        + self.fixed(id="CF-9-02", evidence=f"{EVIDENCE}#000000000000")
                        + self.fixed(id="CF-9-03", evidence="target/production-after/w9/normal/gone.png#"
                                     + "0" * 12)
                        + self.fixed(id="CF-9-04", evidence=EVIDENCE)
                        + self.fixed(id="CF-9-05", evidence="screenshots/after.png"))
        count, problems = self.check()
        joined = "\n".join(problems)
        self.assertIn("CF-9-01: a fixed row needs After evidence", joined)
        self.assertIn(f"CF-9-02: {EVIDENCE} hashes to {self.evidence}, not 000000000000", joined)
        self.assertIn("CF-9-03: evidence not found: target/production-after/w9/normal/gone.png", joined)
        self.assertIn(f"CF-9-04: {EVIDENCE} needs its #<sha12>", joined)
        self.assertIn("CF-9-05: evidence must be docs/screenshots/production/after-*.png or "
                      "target/production-after/...png#<sha12>", joined)
        self.assertEqual(count, 5, problems)

    def test_a_bare_frame_name_reads_from_the_previous_evidence_folder(self):
        frame22 = hashlib.sha256(b"\x89PNG\r\n\x1a\nframe 22").hexdigest()[:12]
        evidence = f"{FRAMES}/frame-0000.png#{self.frame}, frame-0022.png#{frame22}"
        self.write_plan(self.fixed(evidence=evidence)
                        + self.fixed(id="CF-9-02", evidence="frame-0000.png#" + self.frame))
        count, problems = self.check()
        self.assertEqual(count, 1, problems)
        self.assertIn("CF-9-02: frame-0000.png has no earlier evidence folder", problems[0])

    def test_committed_after_pairs_count_as_evidence_without_a_hash(self):
        folder = self.make_pairs()
        digest = hashlib.sha256((folder / "after-battle-small.png").read_bytes()).hexdigest()[:12]
        self.write_plan(self.fixed(evidence="docs/screenshots/production/after-battle-small.png")
                        + self.fixed(id="CF-9-02", evidence="docs/screenshots/production/after-battle-small.png#"
                                     + digest)
                        + self.fixed(id="CF-9-03", evidence="docs/screenshots/production/before-battle-small.png"))
        count, problems = self.check()
        self.assertEqual(count, 1, problems)
        self.assertIn("CF-9-03: evidence must be", problems[0])

    def test_target_evidence_follows_cargo_target_dir(self):
        elsewhere = tempfile.TemporaryDirectory()
        self.addCleanup(elsewhere.cleanup)
        moved = Path(elsewhere.name) / "production-after/w9/normal/moved.png"
        moved.parent.mkdir(parents=True)
        moved.write_bytes(b"moved")
        digest = hashlib.sha256(b"moved").hexdigest()[:12]
        self.write_plan(self.fixed(evidence=f"target/production-after/w9/normal/moved.png#{digest}"))
        self.assertEqual(self.check()[0], 1)
        os.environ["CARGO_TARGET_DIR"] = elsewhere.name
        try:
            self.assertEqual(self.check(), (0, []))
        finally:
            del os.environ["CARGO_TARGET_DIR"]

    # Deferrals.
    def test_a_deferral_reads_approved_by_user_and_an_iso_date(self):
        self.write_plan(row({"status": "deferred"})
                        + row({"id": "CF-9-02", "status": "deferred", "deferral": "approved 2026-11-14"})
                        + row({"id": "CF-9-03", "status": "deferred",
                               "deferral": "approved by user 2026-13-40"})
                        + row({"id": "CF-9-04", "deferral": "approved by user 2026-11-14"}))
        count, problems = self.check()
        joined = "\n".join(problems)
        self.assertIn("CF-9-01: a deferred row needs Deferral 'approved by user YYYY-MM-DD'", joined)
        self.assertIn("CF-9-02: Deferral must read 'approved by user YYYY-MM-DD'", joined)
        self.assertIn("CF-9-03: Deferral date 2026-13-40 is not a date", joined)
        self.assertIn("CF-9-04: only a deferred row carries a Deferral", joined)
        self.assertEqual(count, 4, problems)

    # --final.
    def test_final_fails_on_an_open_row_and_without_the_pair_folder(self):
        self.write_plan(row({}) + self.fixed(id="CF-9-02"))
        self.assertEqual(self.check(), (0, []))
        count, problems = self.check(final=True)
        joined = "\n".join(problems)
        self.assertIn("CF-9-01: open", joined)
        self.assertIn("docs/screenshots/production is missing", joined)
        self.assertEqual(count, 2, problems)

    def test_final_passes_with_every_row_closed_and_the_pairs_in_budget(self):
        self.make_pairs()
        self.write_plan(self.fixed() + row({"id": "CF-9-02", "status": "deferred",
                                            "deferral": "approved by user 2026-11-14"}))
        self.assertEqual(self.check(final=True), (0, []))

    def test_final_fails_over_40_mb_and_on_an_unpaired_still_or_a_missing_index(self):
        folder = self.make_pairs()
        self.write_plan(self.fixed())
        with open(folder / "after-big.png", "wb") as handle:
            handle.truncate(check_capture_findings.PAIR_BUDGET_BYTES)
        (folder / "captures-after.json").unlink()
        count, problems = self.check(final=True)
        joined = "\n".join(problems)
        self.assertIn("over the 40,000,000 B budget", joined)
        self.assertIn("after-big.png has no before-big.png", joined)
        self.assertIn("captures-after.json is missing", joined)
        self.assertEqual(count, 3, problems)

    # Command line.
    def test_main_prints_pass_or_each_problem_and_exits_0_or_1(self):
        self.write_plan(row({}))
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = check_capture_findings.main([str(self.plan), "--root", str(self.root), "--no-git"])
        self.assertEqual(code, 0)
        self.assertIn("PASS: 1 capture findings (1 open, 0 fixed, 0 deferred)", out.getvalue())
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = check_capture_findings.main([str(self.plan), "--final", "--root", str(self.root),
                                                "--no-git"])
        self.assertEqual(code, 1)
        self.assertIn("FAIL: CF-9-01: open", out.getvalue())

    def test_the_default_root_is_the_folder_above_docs(self):
        self.assertEqual(check_capture_findings.default_root(self.plan), self.root)


if __name__ == "__main__":
    unittest.main()
