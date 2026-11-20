"""Desktop extraction namespace regressions; no Rust, graphics or network required."""
from __future__ import annotations

import json
import os
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import package_desktop as packaging
import test_verify_desktop as execution_fixtures
from release_contract import GATE_NAMES, RUST_TEST_GATE_NAMES


class DesktopNamespaceTests(unittest.TestCase):
    def test_supported_names_retain_their_exact_spelling(self):
        names = ["terra-demo", "docs/PLAYTESTING.md", "evidence/01-native-build.log", "assets/comet.glb"]
        packaging.validate_namespace(names)
        self.assertEqual(packaging.portable_parts(names[1]), ("docs", "PLAYTESTING.md"))

    def test_rejects_windows_devices_even_with_extensions(self):
        for name in ["CON", "assets/Nul.glb", "COM1.txt", "lpt9", "COM¹.json", "conin$", "aux/data.json"]:
            with self.subTest(name=name), self.assertRaises(ValueError):
                packaging.validate_namespace([name])

    def test_rejects_windows_trimming_and_special_characters(self):
        for name in ["docs/name.", "docs /guide.md", "docs/name ", "a\x00b", "a\nb", "a\x7fb", "a<b", 'a"b', "a|b", "a?b", "a*b"]:
            with self.subTest(name=name), self.assertRaises(ValueError):
                packaging.validate_namespace([name])

    def test_rejects_case_aliases_and_file_directory_conflicts(self):
        for names in [["README.md", "readme.md"], ["evidence/X.log", "Evidence/x.LOG"], ["a", "a/b"], ["A/b", "a"]]:
            with self.subTest(names=names), self.assertRaises(ValueError):
                packaging.validate_namespace(names)

    def test_retains_traversal_and_noncanonical_path_rejection(self):
        for name in ["", ".", "../a", "a/../b", "a/./b", "a//b", "a/", "/absolute", "C:/a", "a\\b"]:
            with self.subTest(name=name), self.assertRaises(ValueError):
                packaging.validate_namespace([name])

    def fixture(self, root, names):
        (root / "scripts").mkdir()
        (root / "scripts/desktop-payload.json").write_text(json.dumps({"format": 1, "files": names}))
        for name in names:
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"reviewed fixture")
        evidence = root / "accepted"
        evidence.mkdir()
        source = b"{}"
        (evidence / "source-hashes.json").write_bytes(source)
        gates = []
        for name in GATE_NAMES:
            log = "build.log" if name == "native-build" else f"{name}.log"
            (evidence / log).write_bytes(b"accepted build")
            gates.append({"name": name, "exit_code": 0,
                          "command": ["cargo", "build"] if name == "native-build" else ["fixture", name],
                          "log": log, "log_sha256": packaging.sha(b"accepted build"),
                          "passed_test_executions": 1 if name in RUST_TEST_GATE_NAMES else 0,
                          "failed_tests": 0, "ignored_tests": 0})
        identities = [{"crate": "terra-demo", "harness": "terra_demo", "name": "fixture_passes"}]
        identity_bytes = json.dumps(identities).encode()
        (evidence / "distinct-tests.json").write_bytes(identity_bytes)
        binary = bytearray(64)
        binary[:6] = b"\x7fELF\x02\x01"
        struct.pack_into("<H", binary, 18, 62)
        executable = root / "accepted-native"
        executable.write_bytes(binary)
        report = {
            "toolchain": "rustc 1.97.1 (test-build)", "version": "0.23.0", "status": "passed",
            "source_inputs_unchanged": True, "published_reports_unchanged": True,
            "published_logs_unchanged": True, "distinct_passing_rust_tests": 1,
            "evidence_artifacts": {"source-hashes.json": packaging.sha(source),
                                   "distinct-tests.json": packaging.sha(identity_bytes)},
            "distinct_tests_by_crate": {"terra-demo": 1}, "gates": gates,
            "passing_rust_test_executions": len(RUST_TEST_GATE_NAMES),
            "native_executable": str(executable), "native_binary_sha256": packaging.sha(binary),
            "environment": {"os": "Linux"},
        }
        return evidence, report

    def run_package(self, root, evidence, report):
        gates = evidence / "gates.json"
        gates.write_text(json.dumps(report))
        with patch.object(packaging, "ROOT", root), patch.object(packaging, "source_inputs", return_value={}), \
             patch.object(packaging, "check_compatibility", return_value={"contract": {"release": report["version"], "rust_acceptance": "1.97.1"}}), \
             patch.object(packaging.platform, "system", return_value="Linux"):
            return packaging.package(gates, root / "release.zip")

    def current_fixture(self, root):
        evidence, report = self.fixture(root, ["docs/guide.md"])
        report["version"] = "0.24.0"
        report["completion_record"] = "completed-gates.json"
        report["environment"]["gate_output_limit_bytes"] = 4096
        for gate in report["gates"]:
            gate.update(timeout_seconds=5, timed_out=False,
                        execution=execution_fixtures.successful_execution((evidence / gate["log"]).read_bytes()))
        report["startup_probes"] = []
        for name in ("metadata", "toolchain"):
            log = f"startup-{name}.log"
            data = f"fixture {name}".encode()
            (evidence / log).write_bytes(data)
            report["evidence_artifacts"][log] = packaging.sha(data)
            report["startup_probes"].append({"name": name, "log": log, "log_sha256": packaging.sha(data),
                                             "exit_code": 0, "timed_out": False, "timeout_seconds": 5,
                                             "execution": execution_fixtures.successful_execution(data)})
        return evidence, report

    def test_current_package_requires_complete_gate_and_probe_lifetime_evidence(self):
        for mutation in ("none", "gate", "probes", "probe", "linux-unconfirmed", "log-count"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                evidence, report = self.current_fixture(root)
                if mutation == "gate":
                    del report["gates"][0]["execution"]
                elif mutation == "probes":
                    del report["startup_probes"]
                elif mutation == "probe":
                    del report["startup_probes"][0]["execution"]
                elif mutation == "linux-unconfirmed":
                    report["gates"][0]["execution"]["descendant_check"]["status"] = "unconfirmed"
                elif mutation == "log-count":
                    report["startup_probes"][0]["execution"].update(raw_output_bytes_observed=1, retained_log_bytes=1)
                (evidence / "completed-gates.json").write_text(json.dumps(report))
                if mutation == "none":
                    self.assertEqual(self.run_package(root, evidence, report)["status"], "passed")
                else:
                    with self.assertRaises(ValueError):
                        self.run_package(root, evidence, report)
                    self.assertFalse((root / "release.zip").exists())

    def test_publication_preserves_valid_bytes_and_manifest(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence, report = self.fixture(root, ["docs/guide.md"])
            result = self.run_package(root, evidence, report)
            self.assertEqual(result["status"], "passed")
            with zipfile.ZipFile(root / "release.zip") as archive:
                prefix = "terra-world-0.23.0-linux-x86_64/"
                self.assertEqual(archive.read(prefix + "docs/guide.md"), b"reviewed fixture")
                manifest = json.loads(archive.read(prefix + "package-manifest.json"))
                self.assertEqual(manifest["acceptance_record_sha256"], packaging.sha(archive.read(prefix + "evidence/gates.json")))
                self.assertIsNone(archive.testzip())

    def test_failed_terminal_publication_cannot_package_passed_progress_alias(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence, report = self.fixture(root, ["docs/guide.md"])
            report["completion_record"] = "completed-gates.json"
            with self.assertRaisesRegex(ValueError, "Missing"):
                self.run_package(root, evidence, report)
            self.assertFalse((root / "release.zip").exists())

    def test_timed_out_gate_cannot_package_even_with_a_zero_exit_alias(self):
        for value in [True, 1, "false", None]:
            with self.subTest(value=value), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                evidence, report = self.fixture(root, ["docs/guide.md"])
                report["gates"][0]["timed_out"] = value
                with self.assertRaisesRegex(ValueError, "timed-out"):
                    self.run_package(root, evidence, report)
                self.assertFalse((root / "release.zip").exists())

    def test_incomplete_execution_evidence_cannot_package_a_zero_exit_gate(self):
        valid = {"status": "passed", "failure_reason": None, "output_overflow": False,
                 "output_complete": True, "leader_exit_observed": True,
                 "descendant_check": {"status": "owned_process_group_absent"}}
        for change in [{"status": "failed"}, {"failure_reason": "output_limit"},
                       {"output_overflow": True}, {"output_complete": False},
                       {"leader_exit_observed": False},
                       {"descendant_check": {"status": "owned_process_group_present"}}]:
            with self.subTest(change=change), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                evidence, report = self.fixture(root, ["docs/guide.md"])
                report["gates"][0]["execution"] = {**valid, **change}
                with self.assertRaisesRegex(ValueError, "command execution"):
                    self.run_package(root, evidence, report)
                self.assertFalse((root / "release.zip").exists())

    def test_completed_receipt_must_match_alias_and_uses_fixed_safe_name(self):
        for name in ["../escape.json", None, "completed-gates.json"]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                evidence, report = self.fixture(root, ["docs/guide.md"])
                report["completion_record"] = name
                (evidence / "completed-gates.json").write_text('{"status":"running"}')
                with self.assertRaisesRegex(ValueError, "Completion"):
                    self.run_package(root, evidence, report)
                self.assertFalse((root / "release.zip").exists())

    def test_new_terminal_receipt_is_bound_and_retained_in_desktop_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence, report = self.fixture(root, ["docs/guide.md"])
            report["completion_record"] = "completed-gates.json"
            (evidence / "completed-gates.json").write_text(json.dumps(report))
            result = self.run_package(root, evidence, report)
            self.assertEqual(result["status"], "passed")
            with zipfile.ZipFile(root / "release.zip") as archive:
                prefix = "terra-world-0.23.0-linux-x86_64/evidence/"
                self.assertEqual(archive.read(prefix + "completed-gates.json"), archive.read(prefix + "gates.json"))

    def test_aliasing_payload_is_rejected_without_publication(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence, report = self.fixture(root, ["docs/guide.md", "docs/GUIDE.md"])
            with self.assertRaisesRegex(ValueError, "alias"):
                self.run_package(root, evidence, report)
            self.assertFalse((root / "release.zip").exists())

    def test_gate_log_cannot_shadow_an_acceptance_identity(self):
        for name in ["gates.json", "GATES.JSON", "source-hashes.json"]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                evidence, report = self.fixture(root, ["docs/guide.md"])
                report["gates"][0]["log"] = name
                with self.assertRaisesRegex(ValueError, "alias"):
                    self.run_package(root, evidence, report)
                self.assertFalse((root / "release.zip").exists())

    def test_duplicate_gate_logs_cannot_hide_an_omitted_log(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence, report = self.fixture(root, ["docs/guide.md"])
            report["gates"][1]["log"] = report["gates"][0]["log"]
            with self.assertRaisesRegex(ValueError, "alias"):
                self.run_package(root, evidence, report)
            self.assertFalse((root / "release.zip").exists())

    def test_artifact_cannot_shadow_report_or_gate_log(self):
        for name in ["gates.json", "GATES.JSON", "build.log", "BUILD.LOG", "SOURCE-HASHES.JSON"]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                evidence, report = self.fixture(root, ["docs/guide.md"])
                report["evidence_artifacts"][name] = packaging.sha(b"accepted build")
                with self.assertRaisesRegex(ValueError, "alias"):
                    self.run_package(root, evidence, report)
                self.assertFalse((root / "release.zip").exists())

    def test_rejects_incomplete_duplicate_reordered_and_unknown_gate_rosters(self):
        for change in ("native-only", "missing", "duplicate", "unknown", "reordered"):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                evidence, report = self.fixture(root, ["docs/guide.md"])
                if change == "native-only":
                    report["gates"] = [report["gates"][-1]]
                elif change == "missing":
                    report["gates"].pop(0)
                elif change == "duplicate":
                    report["gates"][1]["name"] = report["gates"][0]["name"]
                elif change == "unknown":
                    report["gates"][0]["name"] = "unknown-gate"
                else:
                    report["gates"][0], report["gates"][1] = report["gates"][1], report["gates"][0]
                with self.assertRaisesRegex(ValueError, "roster"):
                    self.run_package(root, evidence, report)
                self.assertFalse((root / "release.zip").exists())

    def test_rejects_missing_test_identity_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence, report = self.fixture(root, ["docs/guide.md"])
            del report["evidence_artifacts"]["distinct-tests.json"]
            with self.assertRaisesRegex(ValueError, "requires hashed distinct-tests"):
                self.run_package(root, evidence, report)
            self.assertFalse((root / "release.zip").exists())

    def test_rejects_inconsistent_test_identities_and_counts(self):
        for change in ("duplicate", "empty", "bad-shape", "bad-name", "total", "boolean-total", "boolean-crate-total", "crate-total"):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                evidence, report = self.fixture(root, ["docs/guide.md"])
                identities = json.loads((evidence / "distinct-tests.json").read_bytes())
                if change == "duplicate":
                    identities.append(dict(identities[0]))
                    report["distinct_passing_rust_tests"] = 2
                    report["distinct_tests_by_crate"] = {"terra-demo": 2}
                elif change == "empty":
                    identities = []
                elif change == "bad-shape":
                    identities = {"count": 1}
                elif change == "bad-name":
                    identities[0]["name"] = ""
                elif change == "total":
                    report["distinct_passing_rust_tests"] = 2
                elif change == "boolean-total":
                    report["distinct_passing_rust_tests"] = True
                elif change == "boolean-crate-total":
                    report["distinct_tests_by_crate"] = {"terra-demo": True}
                else:
                    report["distinct_tests_by_crate"] = {"terra-other": 1}
                identity_bytes = json.dumps(identities).encode()
                (evidence / "distinct-tests.json").write_bytes(identity_bytes)
                report["evidence_artifacts"]["distinct-tests.json"] = packaging.sha(identity_bytes)
                with self.assertRaisesRegex(ValueError, "[Ii]dentity|identities"):
                    self.run_package(root, evidence, report)
                self.assertFalse((root / "release.zip").exists())

    def test_rejects_untyped_missing_and_unsuccessful_gate_results(self):
        changes = [("exit_code", value) for value in (None, False, 0.0, "0", 1, -1)]
        changes += [(field, value) for field in ("passed_test_executions", "failed_tests", "ignored_tests")
                    for value in (None, False, 0.0, "0", -1)]
        changes += [("failed_tests", 1), ("ignored_tests", 1)]
        for field, value in changes:
            with self.subTest(field=field, value=value), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                evidence, report = self.fixture(root, ["docs/guide.md"])
                gate = report["gates"][0]
                if value is None:
                    gate.pop(field)
                else:
                    gate[field] = value
                with self.assertRaises(ValueError):
                    self.run_package(root, evidence, report)
                self.assertFalse((root / "release.zip").exists())

    def test_rejects_zero_execution_test_gate_and_phantom_non_test_executions(self):
        for name, passed in [(RUST_TEST_GATE_NAMES[0], 0), ("native-build", 1)]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                evidence, report = self.fixture(root, ["docs/guide.md"])
                gate = next(gate for gate in report["gates"] if gate["name"] == name)
                gate["passed_test_executions"] = passed
                report["passing_rust_test_executions"] = sum(gate["passed_test_executions"] for gate in report["gates"])
                with self.assertRaises(ValueError):
                    self.run_package(root, evidence, report)
                self.assertFalse((root / "release.zip").exists())

    def test_rejects_missing_untyped_and_inconsistent_execution_total(self):
        for value in (None, False, 14.0, "14", -1, 999):
            with self.subTest(value=value), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                evidence, report = self.fixture(root, ["docs/guide.md"])
                if value is None:
                    report.pop("passing_rust_test_executions")
                else:
                    report["passing_rust_test_executions"] = value
                with self.assertRaisesRegex(ValueError, "execution total"):
                    self.run_package(root, evidence, report)
                self.assertFalse((root / "release.zip").exists())

    def test_rejects_more_distinct_identities_than_passing_executions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence, report = self.fixture(root, ["docs/guide.md"])
            count = report["passing_rust_test_executions"] + 1
            identities = [{"crate": "terra-demo", "harness": "terra_demo", "name": f"test_{index}"}
                          for index in range(count)]
            data = json.dumps(identities).encode()
            (evidence / "distinct-tests.json").write_bytes(data)
            report["evidence_artifacts"]["distinct-tests.json"] = packaging.sha(data)
            report["distinct_passing_rust_tests"] = count
            report["distinct_tests_by_crate"] = {"terra-demo": count}
            with self.assertRaisesRegex(ValueError, "identity evidence"):
                self.run_package(root, evidence, report)
            self.assertFalse((root / "release.zip").exists())

    @unittest.skipUnless(os.name == "posix", "Real symlink creation is qualified on POSIX")
    def test_source_payload_cannot_traverse_symlink(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence, report = self.fixture(root, ["docs/guide.md"])
            (root / "docs/guide.md").rename(root / "guide.md")
            (root / "docs").rmdir()
            (root / "docs").symlink_to(root, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, "symlink"):
                self.run_package(root, evidence, report)
            self.assertFalse((root / "release.zip").exists())


if __name__ == "__main__":
    unittest.main()
