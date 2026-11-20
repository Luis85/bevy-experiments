"""Real ZIP extraction, admission ordering, mutation and failure-report checks."""
from __future__ import annotations

import contextlib
import io
import json
import os
from pathlib import Path
import stat
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch
import warnings
import zipfile

import test_verify_desktop as fixtures
import verify_desktop_archive as archive_check


PREFIX = "terra-world-0.23.0-linux-x86_64"


class DesktopArchiveTests(unittest.TestCase):
    def fixture(self, root):
        source = root / "source"
        source.mkdir()
        fixtures.ExtractedPackageTests().fixture(source)
        return source

    def pack(self, root, source, extra=(), overrides=None, prefix=PREFIX):
        archive = root / "desktop.zip"
        entries = [(f"{prefix}/{p.relative_to(source).as_posix()}", p.read_bytes(), None)
                   for p in sorted(source.rglob("*")) if p.is_file()]
        entries.extend(extra)
        overrides = overrides or {}
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)  # Deliberate duplicate fixtures.
            with zipfile.ZipFile(archive, "w") as bundle:
                for name, data, custom_mode in entries:
                    info = zipfile.ZipInfo(name)
                    info.create_system = 3
                    mode = 0o755 if name.endswith("/terra-demo") else 0o644
                    info.external_attr = (custom_mode if custom_mode is not None else stat.S_IFREG | mode) << 16
                    for key, value in overrides.get(name, {}).items():
                        setattr(info, key, value)
                    info.compress_type = zipfile.ZIP_DEFLATED
                    bundle.writestr(info, data)
        return archive

    def test_fresh_round_trip_checks_bytes_modes_and_archive_without_executing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = self.fixture(root)
            archive = self.pack(root, source)
            before = archive.read_bytes()
            with patch.object(archive_check.desktop, "smoke_help", side_effect=AssertionError("passive must not execute")):
                report = archive_check.verify_archive(archive)
            self.assertEqual(report["status"], "passed")
            self.assertTrue(report["fresh_extraction"])
            self.assertTrue(report["extracted_tree_unchanged"])
            self.assertTrue(report["archive_unchanged"])
            self.assertEqual(report["archive_entries"], len(list(source.rglob("*.*"))) + 1)
            self.assertEqual(report["package"]["smoke_help"]["status"], "not_requested")
            self.assertEqual(archive.read_bytes(), before)

    def test_smoke_is_only_invoked_after_passive_admission_and_tree_is_rechecked(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = self.fixture(root)
            archive = self.pack(root, source)
            observed = []

            def smoke(executable, target, expected_hash, timeout):
                checked, _ = archive_check.desktop.inspect_package(executable.parent)
                self.assertEqual(checked["native_binary_sha256"], expected_hash)
                self.assertEqual(target, "linux-x86_64")
                if os.name == "posix":
                    self.assertEqual(stat.S_IMODE(executable.stat().st_mode), 0o755)
                    self.assertEqual(stat.S_IMODE(executable.parent.parent.stat().st_mode), 0o700)
                observed.append(executable.parent)
                return {"status": "passed", "scope": "unit fixture; no executable was run"}

            with patch.object(archive_check.desktop, "smoke_help", side_effect=smoke):
                report = archive_check.verify_archive(archive, True)
            self.assertEqual(len(observed), 1)
            self.assertFalse(observed[0].exists(), "owned temporary extraction must be cleaned")
            self.assertEqual(report["package"]["smoke_help"]["status"], "passed")

    def test_corrupt_payload_never_reaches_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = self.fixture(root)
            (source / "docs/guide.md").write_bytes(b"changed after manifest")
            archive = self.pack(root, source)
            with patch.object(archive_check.desktop, "smoke_help", side_effect=AssertionError("must not execute")):
                with self.assertRaisesRegex(ValueError, "hash differs"):
                    archive_check.verify_archive(archive, True)

    def test_unsafe_names_aliases_and_multiple_roots_fail_before_extraction(self):
        names = [f"{PREFIX}/../escape", "/absolute", f"{PREFIX}/a\\b", f"{PREFIX}/NUL.txt",
                 f"{PREFIX}/docs/guide.md", f"{PREFIX}/docs/GUIDE.md", f"{PREFIX}/docs",
                 f"{PREFIX}/docs/name. ", "other-root/extra.txt", f"{PREFIX}/directory/"]
        for name in names:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                source = self.fixture(root)
                archive = self.pack(root, source, [(name, b"unexpected", None)])
                with patch.object(archive_check, "extract_checked", side_effect=AssertionError("must inspect first")):
                    with self.assertRaises(ValueError):
                        archive_check.verify_archive(archive)

    def test_links_special_files_and_unsafe_modes_are_rejected_before_extraction(self):
        for mode in [stat.S_IFLNK | 0o644, stat.S_IFIFO | 0o644, stat.S_IFDIR | 0o755,
                     stat.S_IFREG | 0o666, stat.S_IFREG | 0o4644, 0o644]:
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                source = self.fixture(root)
                archive = self.pack(root, source, [(f"{PREFIX}/extra", b"unexpected", mode)])
                with patch.object(archive_check, "extract_checked", side_effect=AssertionError("must inspect first")):
                    with self.assertRaisesRegex(ValueError, "regular file|permissions"):
                        archive_check.verify_archive(archive)

    def test_missing_executable_mode_and_nonunix_mode_metadata_are_rejected(self):
        for attributes in [{"external_attr": (stat.S_IFREG | 0o644) << 16}, {"create_system": 0}]:
            with self.subTest(attributes=attributes), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                source = self.fixture(root)
                archive = self.pack(root, source, overrides={f"{PREFIX}/terra-demo": attributes})
                with self.assertRaisesRegex(ValueError, "permissions|regular file"):
                    archive_check.verify_archive(archive)

    def test_central_directory_is_bounded_before_zipfile_allocates_members(self):
        for mutation in ("count", "bytes", "split", "offset"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                source = self.fixture(root)
                archive = self.pack(root, source)
                content = bytearray(archive.read_bytes())
                end = content.rfind(b"PK\x05\x06")
                if mutation == "count":
                    struct.pack_into("<HH", content, end + 8, 5000, 5000)
                elif mutation == "bytes":
                    struct.pack_into("<L", content, end + 12, archive_check.MAX_DIRECTORY_BYTES + 1)
                elif mutation == "split":
                    struct.pack_into("<H", content, end + 4, 1)
                else:
                    struct.pack_into("<L", content, end + 16, 0xFFFFFFFF)
                archive.write_bytes(content)
                with patch.object(archive_check.zipfile, "ZipFile", side_effect=AssertionError("must bound directory first")):
                    with self.assertRaises(ValueError):
                        archive_check.verify_archive(archive)

    def test_declared_payload_budgets_are_checked_before_extraction(self):
        for name, limit in [("MAX_FILE_BYTES", 32), ("MAX_TOTAL_BYTES", 128)]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                source = self.fixture(root)
                archive = self.pack(root, source)
                with patch.object(archive_check.desktop, name, limit), \
                     patch.object(archive_check, "extract_checked", side_effect=AssertionError("must bound first")):
                    with self.assertRaisesRegex(ValueError, "byte budget"):
                        archive_check.verify_archive(archive)

    def test_root_must_match_the_passively_verified_release_and_platform(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = self.fixture(root)
            archive = self.pack(root, source, prefix="wrong-release")
            with self.assertRaisesRegex(ValueError, "folder differs"):
                archive_check.verify_archive(archive)

    def test_post_execution_payload_and_archive_mutations_are_detected(self):
        for mutation in ("payload", "archive", "mode"):
            if mutation == "mode" and os.name != "posix":
                continue  # Mode application is explicitly platform-scoped.
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                source = self.fixture(root)
                archive = self.pack(root, source)

                def mutate(executable, *_):
                    if mutation == "payload":
                        (executable.parent / "docs/guide.md").write_bytes(b"changed")
                    elif mutation == "archive":
                        with archive.open("ab") as stream:
                            stream.write(b"changed")
                    else:
                        executable.chmod(0o644)
                    return {"status": "passed"}

                with patch.object(archive_check.desktop, "smoke_help", side_effect=mutate):
                    with self.assertRaisesRegex(ValueError, "hash differs|archive changed|permissions changed"):
                        archive_check.verify_archive(archive, True)

    def test_cli_retains_failed_crc_report_without_overwriting_earlier_results(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = self.fixture(root)
            archive = self.pack(root, source)
            content = bytearray(archive.read_bytes())
            header = content.find(b"PK\x01\x02")
            struct.pack_into("<L", content, header + 16, 0)
            archive.write_bytes(content)
            report = root / "check.json"
            args = ["verify_desktop_archive.py", "--archive", str(archive), "--report", str(report)]
            with patch.object(sys, "argv", args), contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(archive_check.main(), 1)
            first = report.read_bytes()
            self.assertEqual(json.loads(first)["status"], "failed")
            self.assertIn("CRC", json.loads(first)["error"])
            with patch.object(sys, "argv", args), contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(archive_check.main(), 1)
            self.assertEqual(report.read_bytes(), first)


if __name__ == "__main__":
    unittest.main()
