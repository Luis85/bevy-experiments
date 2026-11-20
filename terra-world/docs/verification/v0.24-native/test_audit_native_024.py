#!/usr/bin/env python3
"""Temporary structural fixtures for the native audit; no native execution claims.

These generated images/process records exist only in a TemporaryDirectory.
Successful structural checks remain pending, never accepted. The accepted-mode
missing-receipt case uses the real shared validator, without a fake canonical
receipt or a stubbed acceptance success.
"""
from __future__ import annotations

from contextlib import ExitStack
from copy import deepcopy
from datetime import datetime, timedelta, timezone
import importlib.util
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

from PIL import Image

sys.dont_write_bytecode = True
SCRIPT = Path(__file__).with_name("audit_native_024.py")
SPEC = importlib.util.spec_from_file_location("terra_native_audit_fixture_024", SCRIPT)
audit = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(audit)


class NativeAuditTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="terra-native-audit-fixture-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.native = self.root / "docs/verification/v0.24-native"
        self.native.mkdir(parents=True)
        self.source_auditor = self.root / "docs/verification/v0.24-delivery/package_source_024.py"
        self.source_auditor.parent.mkdir(parents=True)
        shutil.copyfile(audit.SOURCE_AUDITOR, self.source_auditor)
        shutil.copyfile(SCRIPT, self.native / SCRIPT.name)
        self.frozen = {"fixture.rs": audit.sha(b"synthetic source fixture; not compiled")}
        self.digest = audit.canonical_digest(self.frozen)
        self.binary = self.root / "synthetic-unexecuted-binary"
        self.binary.write_bytes(b"synthetic binary fixture; never executed")
        self.name = "confirmation-fixture"
        self.stem = "80-confirmation-fixture"
        self.png = self.native / (self.stem + ".png")
        Image.new("RGB", (9, 7), (21, 43, 65)).save(self.png)
        self.image = {"name": self.png.name, "sha256": audit.sha(self.png.read_bytes()),
                      "bytes": self.png.stat().st_size}
        self.log = self.native / (self.name + ".log")
        self.log.write_bytes(b"synthetic process log; no process was launched\n")
        now = datetime.now(timezone.utc)
        at = lambda seconds: (now - timedelta(seconds=seconds)).isoformat()
        self.start = {
            "schema": 1, "release": "0.24.0", "phase": "confirmation", "pid": 34567,
            "command": [str(self.binary), "--fixture-only"], "working_directory": str(self.root),
            "started_utc": at(60), "binary_sha256": audit.sha(self.binary.read_bytes()),
            "source_input_count": len(self.frozen), "source_inputs_sha256": self.digest,
            "expected_interruption": False, "trace_file": None,
        }
        self.process = {**self.start, "exit_code": 0, "finished_utc": at(10), "seconds": 50.0,
                        "source_inputs_unchanged": True, "finished_source_inputs_sha256": self.digest,
                        "log_sha256": audit.sha(self.log.read_bytes())}
        self.capture = {
            "schema": 1, "release": "0.24.0", "capture_name": self.stem,
            "capture_started_utc": at(20), "captured_utc": at(19), "window_id": "123456",
            "window_pid": 34567, "window_pid_before": 34567, "window_pid_after": 34567,
            "window_pid_status": "available", "window_pid_error": None,
            "window_pixels": [9, 7], "image": self.image,
        }
        self.registry = {
            "schema": 1, "release": "0.24.0", "reviewed_utc": at(5),
            "reviewer": "synthetic fixture; no visual qualification",
            "processes": [{"name": self.name, "phase": "confirmation", "pid": 34567,
                           "captures": [{"stem": self.stem, "pixels": [9, 7], "sha256": self.image["sha256"]}]}],
        }
        self.registry_path = self.native / "synthetic-reviewed-registry.json"
        self.write_records()
        self.patches = ExitStack()
        self.addCleanup(self.patches.close)
        for key, value in {
            "ROOT": self.root, "NATIVE": self.native, "VERIFY_ROOT": self.root,
            "ACCEPTANCE": self.root / "docs/verification/v0.24-release-acceptance",
            "SOURCE_AUDITOR": self.source_auditor, "__file__": str(self.native / SCRIPT.name),
            "source_inputs": lambda: deepcopy(self.frozen),
        }.items():
            self.patches.enter_context(patch.object(audit, key, value))

    def write(self, path, value):
        path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")

    def write_records(self):
        self.write(self.native / (self.name + "-start.json"), self.start)
        self.write(self.native / (self.name + "-process.json"), self.process)
        self.write(self.native / (self.stem + "-capture.json"), self.capture)
        self.write(self.registry_path, self.registry)

    def test_complete_native_fixture_remains_pending_and_never_authorizes_release(self):
        result = audit.audit("pending", self.registry_path)
        self.assertEqual(result["status"], "pending_canonical_acceptance")
        self.assertIs(result["authorizes_release"], False)
        self.assertIs(result["acceptance"]["terminal_receipt_validated"], False)
        self.assertEqual(result["capture_count"], 1)
        self.assertEqual(result["captures"][0]["image_sha256"], self.image["sha256"])

    def test_accepted_mode_requires_the_actual_canonical_receipt(self):
        with self.assertRaisesRegex((OSError, ValueError), "completed-gates"):
            audit.audit("accepted", self.registry_path)

    def test_schema_types_reject_booleans_in_registry_and_process_start(self):
        for schema in [True, 1.0, "1"]:
            with self.subTest(registry_schema=schema):
                self.write(self.registry_path, {**self.registry, "schema": schema})
                with self.assertRaises(ValueError):
                    audit.audit("pending", self.registry_path)
        self.write(self.registry_path, self.registry)
        self.write(self.native / (self.name + "-start.json"), {**self.start, "schema": True})
        with self.assertRaisesRegex(ValueError, "mistyped direct launch"):
            audit.audit("pending", self.registry_path)

    def test_elapsed_duration_must_agree_with_wall_lifetime_within_two_seconds(self):
        for seconds in [0.000001, 47.999, 52.001]:
            with self.subTest(rejected_seconds=seconds):
                self.write(self.native / (self.name + "-process.json"), {**self.process, "seconds": seconds})
                with self.assertRaisesRegex(ValueError, "durations differ"):
                    audit.audit("pending", self.registry_path)
        for seconds in [48.0, 52.0]:
            with self.subTest(within_tolerance_seconds=seconds):
                self.write(self.native / (self.name + "-process.json"), {**self.process, "seconds": seconds})
                result = audit.audit("pending", self.registry_path)
                self.assertEqual(result["status"], "pending_canonical_acceptance")
                self.assertIs(result["authorizes_release"], False)
                self.assertEqual(result["duration_tolerance_seconds"], 2.0)

    def test_future_completion_is_rejected_even_when_its_duration_agrees(self):
        future = datetime.now(timezone.utc) + timedelta(days=1)
        duration = (future - datetime.fromisoformat(self.start["started_utc"])).total_seconds()
        self.write(self.native / (self.name + "-process.json"),
                   {**self.process, "finished_utc": future.isoformat(), "seconds": duration})
        with self.assertRaisesRegex(ValueError, "completion is in the future"):
            audit.audit("pending", self.registry_path)

    def test_registry_cannot_promote_baseline_candidate_or_duplicate_captures(self):
        mutations = [
            lambda r: r["processes"][0].update(phase="baseline"),
            lambda r: r["processes"][0].update(phase="candidate"),
            lambda r: r["processes"][0]["captures"][0].update(stem="02-baseline-rejected"),
            lambda r: r["processes"][0]["captures"][0].update(stem="10-candidate-fixture"),
            lambda r: r["processes"][0]["captures"].append(deepcopy(r["processes"][0]["captures"][0])),
            lambda r: r["processes"].append(deepcopy(r["processes"][0])),
            lambda r: r["processes"][0]["captures"][0].update(sha256="missing"),
            lambda r: r["processes"][0]["captures"][0].update(pixels=[True, 7]),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(case=index):
                value = deepcopy(self.registry)
                mutate(value)
                self.write(self.registry_path, value)
                with self.assertRaises(ValueError):
                    audit.audit("pending", self.registry_path)

    def test_process_source_binary_exit_or_direct_identity_changes_reject(self):
        mutations = [
            {"exit_code": -9}, {"exit_code": False}, {"source_inputs_unchanged": False},
            {"finished_source_inputs_sha256": "0" * 64}, {"source_inputs_sha256": "0" * 64},
            {"source_input_count": True}, {"binary_sha256": "0" * 64},
            {"expected_interruption": True}, {"trace_file": "trace.log"},
            {"trace_sha256": "0" * 64}, {"release": "0.23.0"}, {"seconds": 0},
        ]
        for changes in mutations:
            with self.subTest(changes=changes):
                value = {**self.process, **changes}
                self.write(self.native / (self.name + "-process.json"), value)
                with self.assertRaises(ValueError):
                    audit.audit("pending", self.registry_path)

    def test_both_pid_observations_dimensions_and_lifetimes_are_required(self):
        mutations = [
            {"window_pid_after": 34568}, {"window_pid_before": None},
            {"window_pid_status": "unavailable"}, {"window_pid_error": "property unavailable"},
            {"window_pixels": [7, 9]}, {"image": {**self.image, "sha256": "0" * 64}},
            {"capture_started_utc": self.start["started_utc"].replace("+00:00", "")},
            {"captured_utc": (datetime.now(timezone.utc) + timedelta(days=1)).isoformat()},
            {"release": "0.23.0"},
        ]
        for changes in mutations:
            with self.subTest(changes=changes):
                self.write(self.native / (self.stem + "-capture.json"), {**self.capture, **changes})
                with self.assertRaises(ValueError):
                    audit.audit("pending", self.registry_path)

    def test_exact_image_log_and_current_binary_bytes_are_bound(self):
        for path in [self.png, self.log, self.binary]:
            with self.subTest(path=path.name):
                original = path.read_bytes()
                path.write_bytes(original + b"changed")
                try:
                    with self.assertRaises(ValueError):
                        audit.audit("pending", self.registry_path)
                finally:
                    path.write_bytes(original)

    def test_source_change_during_audit_rejects(self):
        changed = {**self.frozen, "unexpected.rs": "0" * 64}
        with patch.object(audit, "source_inputs", side_effect=[deepcopy(self.frozen), changed]):
            with self.assertRaisesRegex(ValueError, "inputs changed"):
                audit.audit("pending", self.registry_path)

    def test_registry_change_after_initial_read_is_detected(self):
        original = audit.source_auditor.digest_regular
        changed = False

        def mutate_after_registry_read(path):
            nonlocal changed
            if path == self.binary and not changed:
                changed = True
                self.write(self.registry_path, {**self.registry, "reviewer": "changed after read"})
            return original(path)

        with patch.object(audit.source_auditor, "digest_regular", side_effect=mutate_after_registry_read):
            with self.assertRaisesRegex(ValueError, "evidence changed"):
                audit.audit("pending", self.registry_path)

    def test_cli_pending_exits_two_and_never_replaces_a_prior_report(self):
        output = self.native / "pending-fixture.json"
        argv = [str(SCRIPT), "--phase", "pending", "--registry", str(self.registry_path), "--output", str(output)]
        with patch.object(sys, "argv", argv):
            self.assertEqual(audit.main(), 2)
        before = output.read_bytes()
        self.assertFalse(json.loads(before)["authorizes_release"])
        with patch.object(sys, "argv", argv):
            self.assertEqual(audit.main(), 1)
        self.assertEqual(output.read_bytes(), before)

    def test_cli_missing_acceptance_retains_failure_without_authorization(self):
        output = self.native / "rejected-fixture.json"
        argv = [str(SCRIPT), "--phase", "accepted", "--registry", str(self.registry_path), "--output", str(output)]
        with patch.object(sys, "argv", argv):
            self.assertEqual(audit.main(), 1)
        result = json.loads(output.read_bytes())
        self.assertEqual(result["status"], "failed")
        self.assertIs(result["authorizes_release"], False)


if __name__ == "__main__":
    unittest.main(verbosity=2)
