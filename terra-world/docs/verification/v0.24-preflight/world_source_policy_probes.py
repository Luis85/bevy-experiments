#!/usr/bin/env python3
"""Small independent fixtures for source-packer preservation policy.

These call isolated policy functions with synthetic inventories and a tiny ZIP
under a temporary directory. They never construct acceptance receipts, invoke
the complete packer, modify source files or qualify a delivered archive.
"""
from __future__ import annotations

import copy
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
import zipfile

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
HELPER = ROOT / "docs/verification/v0.24-delivery/package_source_024.py"
SPEC = importlib.util.spec_from_file_location("terra024_source_policy_review", HELPER)
assert SPEC is not None and SPEC.loader is not None
packer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(packer)


def record(data: bytes, mode: int = 0o644) -> dict:
    return {"sha256": packer.sha(data), "bytes": len(data), "mode": mode}


class PreservationPolicy(unittest.TestCase):
    def setUp(self) -> None:
        self.original = {
            "DELIVERY_MANIFEST.json": record(b"old manifest"),
            "docs/PRODUCT_REVIEW.md": record(b"old review"),
            "docs/VALIDATION.md": record(b"old validation"),
            "assets/preserved.txt": record(b"original asset", 0o600),
        }
        self.current = {
            packer.RELOCATIONS.get(name, name): dict(value)
            for name, value in self.original.items()
        }
        for old, retained in packer.HISTORICAL_COPIES.items():
            self.current[retained] = dict(self.original[old])
        self.allowed = {name: "Exact historical copy" for name in packer.HISTORICAL_COPIES.values()}
        self.prefixes = {"docs/verification/v0.24-": "New current evidence"}

    def check(self, current=None, allowed=None):
        return packer.preserved_files(
            self.current if current is None else current,
            self.original,
            self.allowed if allowed is None else allowed,
            self.prefixes,
        )

    def test_exact_preservation_and_current_evidence_are_admitted(self):
        self.current["docs/verification/v0.24-review/new.json"] = record(b"new evidence")
        files, mapping, changed, restored = self.check()
        self.assertEqual(mapping["DELIVERY_MANIFEST.json"], packer.RELOCATIONS["DELIVERY_MANIFEST.json"])
        self.assertEqual(changed, [])
        self.assertEqual(restored, [])
        self.assertEqual(files, self.current)

    def test_exact_declared_new_path_is_admitted(self):
        self.current["crates/terra-demo/src/new_review.rs"] = record(b"declared source")
        self.allowed["crates/terra-demo/src/new_review.rs"] = "Specific dispatched source path"
        self.check()

    def test_unclassified_new_path_is_rejected(self):
        self.current["assets/unreviewed.bin"] = record(b"surprise")
        with self.assertRaisesRegex(ValueError, "unclassified_new_paths"):
            self.check()

    def test_new_historical_namespace_is_rejected(self):
        self.current["docs/verification/v0.23-retroactive/new.json"] = record(b"retroactive")
        with self.assertRaisesRegex(ValueError, "unclassified_new_paths"):
            self.check()

    def test_original_missing_or_changed_without_allowance_is_rejected(self):
        missing = copy.deepcopy(self.current)
        del missing["assets/preserved.txt"]
        with self.assertRaisesRegex(ValueError, "missing_originals"):
            self.check(missing)
        changed = copy.deepcopy(self.current)
        changed["assets/preserved.txt"] = record(b"changed asset")
        with self.assertRaisesRegex(ValueError, "unexpected_changes"):
            self.check(changed)

    def test_relocated_manifest_cannot_be_changed_even_if_allowlisted(self):
        self.current[packer.RELOCATIONS["DELIVERY_MANIFEST.json"]] = record(b"changed manifest")
        self.allowed["DELIVERY_MANIFEST.json"] = "Must not override relocation retention"
        with self.assertRaisesRegex(ValueError, "unexpected_changes"):
            self.check()

    def test_historical_copy_must_match_original_bytes(self):
        self.current["docs/PRODUCT_REVIEW_v0.23.md"] = record(b"changed old review")
        with self.assertRaisesRegex(ValueError, "Historical current document is not exact"):
            self.check()

    def test_original_mode_restoration_is_explicit(self):
        self.current["assets/preserved.txt"]["mode"] = 0o644
        files, _, _, restored = self.check()
        self.assertEqual(files["assets/preserved.txt"]["mode"], 0o600)
        self.assertEqual(files["assets/preserved.txt"]["source_tree_mode"], 0o644)
        self.assertEqual(restored, ["assets/preserved.txt"])


class ReleaseHistoryPolicy(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory(prefix="terra024-policy-fixture-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name) / "source"
        (self.root / "docs").mkdir(parents=True)
        self.archive = Path(self.directory.name) / "fixture-only.zip"
        self.prior = {
            "CHANGELOG.md": b"# Changelog\n\n## 0.23\nOriginal release text.\n",
            "docs/RELEASE_HISTORY.md": b"# Releases\n\n## 0.23\nOriginal historical entry.\n",
        }
        self.current = {}
        with zipfile.ZipFile(self.archive, "x") as archive:
            for name, data in self.prior.items():
                archive.writestr(packer.PREFIX + name, data)
                current = b"# Current title\n\n## 0.24\nNew entry.\n\n" + data[data.index(b"## "):]
                (self.root / name).write_bytes(current)
                self.current[name] = record(current)

    def test_new_entries_with_exact_prior_entries_are_admitted(self):
        packer.verify_prior_history(self.root, self.archive, self.current)

    def test_lost_prior_history_is_rejected_even_with_current_inventory(self):
        truncated = b"# Changelog\n\n## 0.24\nOnly current release.\n"
        (self.root / "CHANGELOG.md").write_bytes(truncated)
        self.current["CHANGELOG.md"] = record(truncated)
        with self.assertRaisesRegex(ValueError, "Prior release entries are missing or changed"):
            packer.verify_prior_history(self.root, self.archive, self.current)

    def test_post_inventory_history_file_change_is_rejected(self):
        path = self.root / "CHANGELOG.md"
        path.write_bytes(b"New preamble\n" + path.read_bytes())
        with self.assertRaisesRegex(ValueError, "Prior release entries are missing or changed"):
            packer.verify_prior_history(self.root, self.archive, self.current)


if __name__ == "__main__":
    unittest.main(verbosity=2)
