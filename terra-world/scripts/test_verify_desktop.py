"""Extracted-package and real bounded smoke-process regressions."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import platform
import struct
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import verify_desktop as desktop


def sha(data):
    return hashlib.sha256(data).hexdigest()


def successful_execution(data, process_platform="posix", limit=4096):
    """Structural receipt fixture, not a claim that a native workload ran."""
    return {"status": "passed", "failure_reason": None, "process_platform": process_platform,
            "output_limit_bytes": limit, "raw_output_bytes_observed": len(data),
            "retained_log_bytes": len(data), "output_overflow": False, "output_complete": True,
            "leader_exit_observed": True,
            "descendant_check": {"status": "owned_process_group_absent" if process_platform == "posix" else "unconfirmed"},
            "cleanup": {"status": "not_needed"}, "scope": "structural unit fixture"}


class ExtractedPackageTests(unittest.TestCase):
    def fixture(self, root):
        binary = bytearray(64)
        binary[:6] = b"\x7fELF\x02\x01"
        struct.pack_into("<H", binary, 18, 62)
        files = {"terra-demo": bytes(binary), "docs/guide.md": b"Reviewed instructions",
                 "evidence/build.log": b"accepted build", "evidence/source-hashes.json": b"{}"}
        receipt = {
            "status": "passed", "version": "0.23.0", "completion_record": "completed-gates.json",
            "native_binary_sha256": sha(binary), "source_inputs_unchanged": True,
            "published_reports_unchanged": True, "published_logs_unchanged": True,
            "gates": [{"name": "native-build", "exit_code": 0, "timed_out": False,
                       "failed_tests": 0, "ignored_tests": 0,
                       "log": "build.log", "log_sha256": sha(files["evidence/build.log"])}],
            "evidence_artifacts": {"source-hashes.json": sha(b"{}")},
        }
        record = json.dumps(receipt).encode()
        files.update({"evidence/gates.json": record, "evidence/completed-gates.json": record})
        for name, data in files.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        (root / "terra-demo").chmod(0o755)
        manifest = {"release": "0.23.0", "platform": "linux-x86_64", "build_profile": "dev",
                    "native_binary_sha256": sha(binary), "acceptance_record_sha256": sha(record),
                    "files_sha256": {name: sha(data) for name, data in files.items()}}
        self.write_manifest(root, manifest)
        return manifest, receipt

    def write_manifest(self, root, manifest):
        (root / desktop.MANIFEST).write_text(json.dumps(manifest))

    def rewrite_receipt(self, root, manifest, receipt):
        data = json.dumps(receipt).encode()
        for name in ("evidence/gates.json", "evidence/completed-gates.json"):
            (root / name).write_bytes(data)
            manifest["files_sha256"][name] = sha(data)
        manifest["acceptance_record_sha256"] = sha(data)
        self.write_manifest(root, manifest)

    def current_fixture(self, root):
        manifest, receipt = self.fixture(root)
        manifest["release"] = receipt["version"] = "0.24.0"
        receipt["environment"] = {"os": "Linux", "gate_output_limit_bytes": 4096}
        for gate in receipt["gates"]:
            gate["timeout_seconds"] = 5
            gate["execution"] = successful_execution((root / "evidence" / gate["log"]).read_bytes())
        receipt["startup_probes"] = []
        for name in ("metadata", "toolchain"):
            log = f"startup-{name}.log"
            data = f"fixture {name}".encode()
            (root / "evidence" / log).write_bytes(data)
            manifest["files_sha256"][f"evidence/{log}"] = sha(data)
            receipt["evidence_artifacts"][log] = sha(data)
            receipt["startup_probes"].append({"name": name, "log": log, "log_sha256": sha(data),
                                              "exit_code": 0, "timed_out": False, "timeout_seconds": 5,
                                              "execution": successful_execution(data)})
        self.rewrite_receipt(root, manifest, receipt)
        return manifest, receipt

    def test_current_complete_execution_and_probe_evidence_passes_with_exact_log_counts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.current_fixture(root)
            result, _ = desktop.inspect_package(root)
            self.assertEqual(result["status"], "passed")
            self.assertEqual(result["release"], "0.24.0")

    def test_current_receipts_cannot_omit_or_forge_execution_and_startup_proofs(self):
        mutations = ("gate-omitted", "gate-partial", "probes-omitted", "probe-omitted", "probe-partial",
                     "linux-unconfirmed", "zero-with-overflow", "raw-boolean", "raw-over-limit",
                     "retained-count", "limit-mismatch", "policy-missing", "missing-reason")
        for mutation in mutations:
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                manifest, receipt = self.current_fixture(root)
                execution = receipt["gates"][0]["execution"]
                if mutation == "gate-omitted":
                    del receipt["gates"][0]["execution"]
                elif mutation == "gate-partial":
                    receipt["gates"][0]["execution"] = {"status": "passed"}
                elif mutation == "probes-omitted":
                    del receipt["startup_probes"]
                elif mutation == "probe-omitted":
                    receipt["startup_probes"].pop()
                elif mutation == "probe-partial":
                    del receipt["startup_probes"][0]["execution"]
                elif mutation == "linux-unconfirmed":
                    execution["descendant_check"]["status"] = "unconfirmed"
                elif mutation == "zero-with-overflow":
                    execution["output_overflow"] = True
                elif mutation == "raw-boolean":
                    execution["raw_output_bytes_observed"] = True
                elif mutation == "raw-over-limit":
                    execution["raw_output_bytes_observed"] = execution["retained_log_bytes"] = 4097
                elif mutation == "retained-count":
                    execution["raw_output_bytes_observed"] = execution["retained_log_bytes"] = 1
                elif mutation == "limit-mismatch":
                    execution["output_limit_bytes"] = 8192
                elif mutation == "policy-missing":
                    del receipt["environment"]["gate_output_limit_bytes"]
                else:
                    del execution["failure_reason"]
                self.rewrite_receipt(root, manifest, receipt)
                with self.assertRaises(ValueError):
                    desktop.inspect_package(root)

    def test_passive_verification_binds_exact_bytes_without_executing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, _ = self.fixture(root)
            with patch.object(desktop.subprocess, "Popen", side_effect=AssertionError("must not execute")):
                report, executable = desktop.inspect_package(root)
            self.assertEqual(report["status"], "passed")
            self.assertEqual(report["files_verified"], len(manifest["files_sha256"]))
            self.assertEqual(report["smoke_help"], {"status": "not_requested"})
            self.assertEqual(executable, root / "terra-demo")

    def test_missing_corrupt_and_unlisted_payloads_fail(self):
        for mutation in ("missing", "corrupt", "unlisted"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                self.fixture(root)
                if mutation == "missing":
                    (root / "docs/guide.md").unlink()
                elif mutation == "corrupt":
                    (root / "docs/guide.md").write_bytes(b"changed")
                else:
                    (root / "unreviewed.dll").write_bytes(b"extra")
                with self.assertRaisesRegex(ValueError, "inventory differs|hash differs"):
                    desktop.inspect_package(root)

    def test_unsafe_and_aliased_manifest_paths_fail_before_reads(self):
        for bad in ("../outside", "C:/file", "docs/NUL.txt", "docs/guide.md ", "DOCS/extra.md",
                    "docs/guide.md/child", "package-manifest.json"):
            with self.subTest(bad=bad), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                manifest, _ = self.fixture(root)
                manifest["files_sha256"][bad] = "a" * 64
                self.write_manifest(root, manifest)
                with self.assertRaises(ValueError):
                    desktop.inspect_package(root)

    def test_duplicate_json_keys_and_invalid_hashes_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, _ = self.fixture(root)
            (root / desktop.MANIFEST).write_text('{"files_sha256":{},"files_sha256":{}}')
            with self.assertRaisesRegex(ValueError, "Duplicate JSON key"):
                desktop.inspect_package(root)
            for value in (None, True, "A" * 64, "123", 1):
                with self.subTest(value=value):
                    manifest["files_sha256"]["terra-demo"] = value
                    self.write_manifest(root, manifest)
                    with self.assertRaisesRegex(ValueError, "SHA-256"):
                        desktop.inspect_package(root)

    def test_links_and_reparse_points_are_rejected_without_following(self):
        # Reparse classification is exercised even where symlink creation needs
        # Windows privileges. Actual Unix links are exercised when supported.
        fake = type("Metadata", (), {"st_mode": 0o100644, "st_file_attributes": 0x400})()
        self.assertTrue(desktop.is_link(fake))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            if os.name == "posix":
                (root / "docs/guide.md").unlink()
                (root / "docs/guide.md").symlink_to(root / "evidence/build.log")
                with self.assertRaisesRegex(ValueError, "Linked"):
                    desktop.inspect_package(root)
                (root / "docs/guide.md").unlink()
                (root / "docs").rmdir()
                (root / "docs").symlink_to(root / "evidence", target_is_directory=True)
                with self.assertRaisesRegex(ValueError, "Linked"):
                    desktop.inspect_package(root)

    def test_declared_architecture_and_binary_association_must_match(self):
        for mutation in ("platform", "binary", "receipt"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                manifest, receipt = self.fixture(root)
                if mutation == "platform":
                    binary = bytearray(128)
                    binary[:2] = b"MZ"
                    struct.pack_into("<I", binary, 60, 64)
                    binary[64:68] = b"PE\0\0"
                    struct.pack_into("<H", binary, 68, 0x8664)
                    (root / "terra-demo").write_bytes(binary)
                    manifest["files_sha256"]["terra-demo"] = sha(binary)
                    manifest["native_binary_sha256"] = sha(binary)
                elif mutation == "binary":
                    manifest["native_binary_sha256"] = "a" * 64
                else:
                    receipt["native_binary_sha256"] = "b" * 64
                    self.rewrite_receipt(root, manifest, receipt)
                self.write_manifest(root, manifest)
                with self.assertRaisesRegex(ValueError, "identity differs|accepted package"):
                    desktop.inspect_package(root)

    def test_hard_linked_payload_and_oversized_receipt_alias_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "package"
            root.mkdir()
            manifest, _ = self.fixture(root)
            outside = root.parent / "outside.md"
            outside.write_bytes((root / "docs/guide.md").read_bytes())
            (root / "docs/guide.md").unlink()
            os.link(outside, root / "docs/guide.md")
            with self.assertRaisesRegex(ValueError, "Hard-linked"):
                desktop.inspect_package(root)
            (root / "docs/guide.md").unlink()
            (root / "docs/guide.md").write_bytes(outside.read_bytes())
            alias = b" " * 4097
            (root / "evidence/gates.json").write_bytes(alias)
            manifest["files_sha256"]["evidence/gates.json"] = sha(alias)
            self.write_manifest(root, manifest)
            with patch.object(desktop, "MAX_MANIFEST_BYTES", 4096):
                with self.assertRaisesRegex(ValueError, "JSON record exceeds"):
                    desktop.inspect_package(root)

    def test_false_terminal_or_gate_success_is_rejected(self):
        for mutation in ("running", "flags", "timeout", "exit", "missing-log", "artifact",
                         "boolean-counter", "float-counter", "missing-counter", "execution"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                manifest, receipt = self.fixture(root)
                if mutation == "running":
                    receipt["status"] = "running"
                elif mutation == "flags":
                    receipt["published_logs_unchanged"] = 1
                elif mutation == "timeout":
                    receipt["gates"][0]["timed_out"] = True
                elif mutation == "exit":
                    receipt["gates"][0]["exit_code"] = False
                elif mutation == "missing-log":
                    receipt["gates"][0].update(log="missing.log", log_sha256=None)
                elif mutation == "artifact":
                    receipt["evidence_artifacts"]["source-hashes.json"] = "b" * 64
                elif mutation == "boolean-counter":
                    receipt["gates"][0]["failed_tests"] = False
                elif mutation == "float-counter":
                    receipt["gates"][0]["ignored_tests"] = 0.0
                elif mutation == "execution":
                    receipt["gates"][0]["execution"] = {
                        "status": "failed", "failure_reason": "output_limit",
                        "output_overflow": True, "output_complete": False,
                        "leader_exit_observed": True,
                        "descendant_check": {"status": "owned_process_group_absent"},
                    }
                else:
                    del receipt["gates"][0]["ignored_tests"]
                self.rewrite_receipt(root, manifest, receipt)
                with self.assertRaises(ValueError):
                    desktop.inspect_package(root)

    def test_terminal_alias_and_manifest_size_are_checked(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, _ = self.fixture(root)
            (root / "evidence/gates.json").write_bytes(b"{}")
            manifest["files_sha256"]["evidence/gates.json"] = sha(b"{}")
            self.write_manifest(root, manifest)
            with self.assertRaisesRegex(ValueError, "Completion receipt differs"):
                desktop.inspect_package(root)
            with patch.object(desktop, "MAX_MANIFEST_BYTES", 20):
                with self.assertRaisesRegex(ValueError, "exceeds"):
                    desktop.inspect_package(root)


class DesktopSmokeTests(unittest.TestCase):
    def smoke_fixture(self, root, text):
        executable = root / "terra-demo"
        executable.write_text("inspected test executable")
        executable.chmod(0o755)
        helper = root / "process.py"
        helper.write_text(text)
        native_popen = subprocess.Popen

        def launch(command, **kwargs):
            if command[0] == "taskkill":
                return native_popen(command, **kwargs)
            self.assertEqual(command, [str(executable), "--help"])
            return native_popen([sys.executable, str(helper)], **kwargs)

        target = "windows-x86_64" if platform.system() == "Windows" else "linux-x86_64"
        return executable, target, launch

    def test_explicit_smoke_runs_and_requires_expected_help(self):
        for output, success in (("Terra World\\nUsage: terra-demo", True), ("wrong application", False)):
            with self.subTest(output=output), tempfile.TemporaryDirectory() as temporary:
                executable, target, launch = self.smoke_fixture(Path(temporary), f"print({output!r})\n")
                with patch.object(desktop.subprocess, "Popen", side_effect=launch):
                    if success:
                        result = desktop.smoke_help(executable, target, desktop.digest(executable), 5)
                        self.assertEqual(result["status"], "passed")
                        self.assertEqual(result["exit_code"], 0)
                    else:
                        with self.assertRaisesRegex(ValueError, "expected bounded help"):
                            desktop.smoke_help(executable, target, desktop.digest(executable), 5)

    def test_real_timeout_never_reports_success(self):
        with tempfile.TemporaryDirectory() as temporary:
            executable, target, launch = self.smoke_fixture(Path(temporary), "import time\ntime.sleep(30)\n")
            with patch.object(desktop.subprocess, "Popen", side_effect=launch):
                with self.assertRaisesRegex(ValueError, "timed out"):
                    desktop.smoke_help(executable, target, desktop.digest(executable), .05)

    def test_output_limit_is_enforced_while_the_process_is_running(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            late = root / "must-not-reach.txt"
            script = (
                "import os, time\nfrom pathlib import Path\n"
                "os.write(1, b'Terra World\\nUsage: noisy-help\\n')\n"
                "for _ in range(1024):\n    os.write(1, b'x' * 4096)\n"
                f"time.sleep(2)\nPath({str(late)!r}).write_text('still running')\n"
            )
            executable, target, launch = self.smoke_fixture(root, script)
            started = time.monotonic()
            # A passive byte check is allowed to use files; startup capture must
            # not rely on a grow-until-exit temporary spool anymore.
            with patch.object(desktop.subprocess, "Popen", side_effect=launch), \
                    patch.object(tempfile, "TemporaryFile", side_effect=AssertionError("no disk spool")):
                with self.assertRaisesRegex(ValueError, "65536-byte output limit"):
                    desktop.smoke_help(executable, target, desktop.digest(executable), 10)
            self.assertLess(time.monotonic() - started, 8)
            self.assertFalse(late.exists())

    def test_exact_output_boundary_and_nonzero_exit_are_distinct(self):
        for size, exit_code, accepted in ((desktop.MAX_HELP_BYTES, 0, True),
                                          (desktop.MAX_HELP_BYTES + 1, 0, False),
                                          (64, 7, False)):
            with self.subTest(size=size, exit_code=exit_code), tempfile.TemporaryDirectory() as temporary:
                script = (
                    "import os, sys\nheader=b'Terra World\\nUsage: boundary\\n'\n"
                    f"os.write(1, header + b'x' * ({size} - len(header)))\n"
                    f"sys.exit({exit_code})\n"
                )
                executable, target, launch = self.smoke_fixture(Path(temporary), script)
                with patch.object(desktop.subprocess, "Popen", side_effect=launch):
                    if accepted:
                        result = desktop.smoke_help(executable, target, desktop.digest(executable), 5)
                        self.assertEqual(result["output_bytes"], desktop.MAX_HELP_BYTES)
                        self.assertEqual(result["output_limit_bytes"], desktop.MAX_HELP_BYTES)
                    else:
                        reason = "output limit" if size > desktop.MAX_HELP_BYTES else "exit 7"
                        with self.assertRaisesRegex(ValueError, reason):
                            desktop.smoke_help(executable, target, desktop.digest(executable), 5)

    def test_leader_exit_with_descendant_held_pipe_has_a_finite_deadline(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            ready = root / "descendant.pid"
            exited = root / "leader-finished.txt"
            descendant = (
                "import os, time\nfrom pathlib import Path\n"
                f"Path({str(ready)!r}).write_text(str(os.getpid()))\n"
                "time.sleep(30)\n"
            )
            script = (
                "import subprocess, sys, time\nfrom pathlib import Path\n"
                f"subprocess.Popen([sys.executable, '-c', {descendant!r}])\n"
                "deadline = time.monotonic() + 5\n"
                f"while not Path({str(ready)!r}).is_file() and time.monotonic() < deadline:\n"
                "    time.sleep(.01)\n"
                "print('Terra World\\nUsage: descendant-test', flush=True)\n"
                f"Path({str(exited)!r}).write_text('normal leader exit')\n"
            )
            executable, target, launch = self.smoke_fixture(root, script)
            started = time.monotonic()
            try:
                with patch.object(desktop.subprocess, "Popen", side_effect=launch):
                    with self.assertRaisesRegex(ValueError, "timed out.*output completion"):
                        desktop.smoke_help(executable, target, desktop.digest(executable), 2)
                self.assertTrue(exited.is_file(), "fixture must actually exit its leader")
                self.assertTrue(ready.is_file())
                self.assertLess(time.monotonic() - started, 16)
            finally:
                # Windows taskkill cannot prove cleanup after the leader exits.
                # This fixture knows its own child's PID and explicitly cleans
                # it; ordinary smoke verification makes no equivalent claim.
                if ready.is_file():
                    pid = int(ready.read_text())
                    if os.name == "nt":
                        subprocess.run(["taskkill", "/PID", str(pid), "/T", "/F"],
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
                    else:
                        try:
                            os.kill(pid, 9)
                        except ProcessLookupError:
                            pass

    def test_redirected_descendant_cannot_pass_a_posix_workload_check(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            ready = root / "redirected.pid"
            descendant = (
                "import os, time\nfrom pathlib import Path\n"
                f"Path({str(ready)!r}).write_text(str(os.getpid()))\n"
                "time.sleep(30)\n"
            )
            script = (
                "import subprocess, sys, time\nfrom pathlib import Path\n"
                f"subprocess.Popen([sys.executable, '-c', {descendant!r}], "
                "stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)\n"
                "deadline = time.monotonic() + 5\n"
                f"while not Path({str(ready)!r}).is_file() and time.monotonic() < deadline:\n"
                "    time.sleep(.01)\n"
                "print('Terra World\\nUsage: redirected-child', flush=True)\n"
            )
            executable, target, launch = self.smoke_fixture(root, script)
            try:
                with patch.object(desktop.subprocess, "Popen", side_effect=launch):
                    if os.name == "posix":
                        with self.assertRaisesRegex(ValueError, "left an owned process group"):
                            desktop.smoke_help(executable, target, desktop.digest(executable), 5)
                    else:
                        # The portable Windows path explicitly does not assert
                        # descendant quiescence from successful leader/EOF alone.
                        result = desktop.smoke_help(executable, target, desktop.digest(executable), 5)
                        self.assertEqual(result["descendant_check"]["status"], "unconfirmed")
                self.assertTrue(ready.is_file())
                if sys.platform.startswith("linux"):
                    pid = int(ready.read_text())
                    deadline = time.monotonic() + 2
                    while True:
                        try:
                            state = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[0]
                        except FileNotFoundError:
                            break
                        if state == "Z":
                            break  # Terminated; its external parent must reap it.
                        if time.monotonic() >= deadline:
                            self.fail("redirected fixture descendant remained live after group cleanup")
                        time.sleep(.01)
            finally:
                if ready.is_file():
                    pid = int(ready.read_text())
                    if os.name == "nt":
                        subprocess.run(["taskkill", "/PID", str(pid), "/T", "/F"],
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
                    else:
                        try:
                            os.kill(pid, 9)
                        except ProcessLookupError:
                            pass

    def test_mismatch_and_bad_deadlines_do_not_execute(self):
        with tempfile.TemporaryDirectory() as temporary:
            executable, target, _ = self.smoke_fixture(Path(temporary), "raise AssertionError()\n")
            with patch.object(desktop.subprocess, "Popen", side_effect=AssertionError("must not execute")):
                with self.assertRaisesRegex(ValueError, "changed"):
                    desktop.smoke_help(executable, target, "b" * 64, 1)
                other = "linux-x86_64" if target == "windows-x86_64" else "windows-x86_64"
                with self.assertRaisesRegex(ValueError, "operating system"):
                    desktop.smoke_help(executable, other, desktop.digest(executable), 1)
                for timeout in (0, -1, float("inf"), float("nan")):
                    with self.subTest(timeout=timeout), self.assertRaisesRegex(ValueError, "positive and finite"):
                        desktop.smoke_help(executable, target, desktop.digest(executable), timeout)


if __name__ == "__main__":
    unittest.main()
