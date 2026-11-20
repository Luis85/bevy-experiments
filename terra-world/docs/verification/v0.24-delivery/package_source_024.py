#!/usr/bin/env python3
"""Build and independently re-read the complete accepted Terra 0.24 source ZIP.

This evidence-only helper does not modify implementation inputs or historical
helpers. Keep the source tree and accepted executable idle while it runs. Both
output paths must be new and outside the source tree. A success report is written
only after full archive, baseline, current-source and acceptance revalidation.
Hashes establish consistency, not publisher authentication or an atomic live
filesystem snapshot. A failed run may leave its exclusively created output ZIP;
retain that failure separately and use new paths for a corrected run.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import zipfile


RELEASE = "0.24.0"
BASELINE_SHA256 = "99d619069fc8f6275e625f40a7c4a916535ededcafb20f733f34ef0a3f361140"
BASELINE_FILES = 3092
PREFIX = "terra-world/"
MANIFEST = "DELIVERY_MANIFEST.json"
RELOCATIONS = {
    MANIFEST: "docs/verification/v0.24-review/baseline-delivery-manifest.json",
}
HISTORICAL_COPIES = {
    "docs/PRODUCT_REVIEW.md": "docs/PRODUCT_REVIEW_v0.23.md",
    "docs/VALIDATION.md": "docs/VALIDATION_v0.23.md",
}
EXCLUDED_DIRECTORIES = {".git", "target", "__pycache__"}
EXCLUDED_SUFFIXES = {".pyc", ".pyo"}
SHA256 = re.compile(r"[0-9a-f]{64}")


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"Duplicate JSON key: {key!r}")
        result[key] = value
    return result


def decode(data: bytes):
    return json.loads(data, object_pairs_hook=unique_object)


def signature(info: os.stat_result) -> tuple:
    return (info.st_dev, info.st_ino, info.st_mode, info.st_size,
            info.st_mtime_ns, info.st_ctime_ns)


def read_regular(path: Path) -> tuple[bytes, os.stat_result]:
    """Read one regular file and reject replacement/change during that read."""
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode):
        raise ValueError(f"Expected a regular non-symlink file: {path}")
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    with os.fdopen(descriptor, "rb") as stream:
        opened = os.fstat(stream.fileno())
        if signature(opened) != signature(before):
            raise ValueError(f"File changed before reading: {path}")
        data = stream.read()
        after = os.fstat(stream.fileno())
    if (signature(after) != signature(before)
            or signature(path.lstat()) != signature(before)
            or len(data) != before.st_size):
        raise ValueError(f"File changed while reading: {path}")
    return data, before


def digest_regular(path: Path) -> str:
    """Stream large ZIPs/binaries while checking their file identity."""
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode):
        raise ValueError(f"Expected a regular non-symlink file: {path}")
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    with os.fdopen(descriptor, "rb") as stream:
        if signature(os.fstat(stream.fileno())) != signature(before):
            raise ValueError(f"File changed before hashing: {path}")
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
        after = os.fstat(stream.fileno())
    if signature(after) != signature(before) or signature(path.lstat()) != signature(before):
        raise ValueError(f"File changed while hashing: {path}")
    return digest


def source_inventory(root: Path, validate_namespace) -> dict:
    def unreadable_directory(error: OSError) -> None:
        raise error

    files = {}
    for directory, children, names in os.walk(
            root, followlinks=False, onerror=unreadable_directory):
        parent = Path(directory)
        retained = []
        for name in sorted(children):
            if name in EXCLUDED_DIRECTORIES:
                continue
            path = parent / name
            if not stat.S_ISDIR(path.lstat().st_mode):
                raise ValueError(f"Source directory must not be a symlink: {path}")
            retained.append(name)
        children[:] = retained
        for name in sorted(names):
            path = parent / name
            if path.suffix in EXCLUDED_SUFFIXES:
                continue
            relative = path.relative_to(root).as_posix()
            data, info = read_regular(path)
            files[relative] = {
                "sha256": sha(data), "bytes": len(data),
                "mode": stat.S_IMODE(info.st_mode),
            }
    files = dict(sorted(files.items()))
    validate_namespace(files)
    if MANIFEST in files:
        raise ValueError("Old root delivery manifest must be relocated before packaging")
    return files


def baseline_inventory(original: Path, validate_namespace) -> dict:
    if digest_regular(original) != BASELINE_SHA256:
        raise ValueError("Baseline is not the exact delivered 0.23.0 archive")
    files = {}
    with zipfile.ZipFile(original) as archive:
        entries = archive.infolist()
        if any(item.is_dir() for item in entries):
            raise ValueError("The pinned baseline must contain only its recorded files")
        if len(entries) != BASELINE_FILES:
            raise ValueError("Baseline member count differs from 3,092 files")
        validate_namespace(item.filename for item in entries)
        for item in entries:
            if not item.filename.startswith(PREFIX):
                raise ValueError(f"Unexpected baseline prefix: {item.filename}")
            name = item.filename.removeprefix(PREFIX)
            if name in files or not name:
                raise ValueError(f"Duplicate/empty baseline path: {name}")
            mode = item.external_attr >> 16
            if item.create_system != 3 or not stat.S_ISREG(mode):
                raise ValueError(f"Nonregular baseline member: {name}")
            data = archive.read(item)
            if len(data) != item.file_size:
                raise ValueError(f"Baseline size differs: {name}")
            files[name] = {
                "sha256": sha(data), "bytes": len(data),
                "mode": stat.S_IMODE(mode),
            }
    if digest_regular(original) != BASELINE_SHA256:
        raise ValueError("Baseline changed while its members were being read")
    if MANIFEST not in files:
        raise ValueError("Baseline delivery manifest is missing")
    return dict(sorted(files.items()))


def accepted_state(root: Path, contained_file, validate_namespace,
                   source_inputs, validate_gate_results, gate_names) -> dict:
    directory = root / "docs/verification/v0.24-release-acceptance"
    receipt_bytes, _ = read_regular(contained_file(directory, "completed-gates.json"))
    receipt = decode(receipt_bytes)
    if (not isinstance(receipt, dict) or receipt.get("status") != "passed"
            or receipt.get("version") != RELEASE
            or any(receipt.get(key) is not True for key in (
                "source_inputs_unchanged", "published_reports_unchanged", "published_logs_unchanged"
            )) or receipt.get("completion_record") != "completed-gates.json"):
        raise ValueError("Requires the actual successful completed 0.24 acceptance")
    alias, _ = read_regular(contained_file(directory, "gates.json"))
    if alias != receipt_bytes:
        raise ValueError("Terminal receipt and progress alias differ")
    gates = receipt.get("gates")
    if (not isinstance(gates, list) or not all(isinstance(gate, dict) for gate in gates)
            or len(gate_names) != 44 or len(gates) != 44):
        raise ValueError("Requires the canonical 44-gate acceptance roster")
    passing = validate_gate_results(gates, require_execution=True,
                                   platform=receipt.get("environment", {}).get("os"),
                                   output_limit_bytes=receipt.get("environment", {}).get("gate_output_limit_bytes"))
    if any(gate.get("timed_out") is not False for gate in gates):
        raise ValueError("Every 0.24 gate must explicitly report no timeout")
    if (type(receipt.get("passing_rust_test_executions")) is not int
            or receipt["passing_rust_test_executions"] != passing):
        raise ValueError("Receipt Rust execution total differs")

    hashes = {"completed-gates.json": sha(receipt_bytes), "gates.json": sha(receipt_bytes)}
    artifacts = receipt.get("evidence_artifacts")
    if not isinstance(artifacts, dict):
        raise ValueError("Missing accepted evidence artifact map")
    for name, expected in [*artifacts.items(), *((g.get("log"), g.get("log_sha256")) for g in gates)]:
        if (not isinstance(name, str) or not isinstance(expected, str)
                or SHA256.fullmatch(expected) is None or name in hashes):
            raise ValueError(f"Invalid/duplicate accepted evidence binding: {name!r}")
        data, _ = read_regular(contained_file(directory, name))
        if sha(data) != expected:
            raise ValueError(f"Accepted evidence changed: {name}")
        hashes[name] = expected
    validate_namespace(hashes)
    if not {"source-hashes.json", "distinct-tests.json"}.issubset(hashes):
        raise ValueError("Required hashed source/test evidence is missing")
    probes = receipt.get("startup_probes")
    if (not isinstance(probes, list) or not all(isinstance(probe, dict) for probe in probes)
            or [probe.get("name") for probe in probes] != ["metadata", "toolchain"]):
        raise ValueError("Required startup probe roster differs")
    for probe in probes:
        if (type(probe.get("exit_code")) is not int or probe["exit_code"] != 0
                or probe.get("timed_out") is not False
                or hashes.get(probe.get("log")) != probe.get("log_sha256")):
            raise ValueError("Startup probe failure or unbound log cannot authorize packaging")

    from verify_desktop import validate_receipt_execution
    validate_receipt_execution(receipt, {name: contained_file(directory, name).stat().st_size for name in hashes})

    frozen_bytes, _ = read_regular(contained_file(directory, "source-hashes.json"))
    frozen = decode(frozen_bytes)
    if (not isinstance(frozen, dict) or not frozen
            or any(not isinstance(name, str) or not isinstance(value, str)
                   or SHA256.fullmatch(value) is None for name, value in frozen.items())
            or type(receipt.get("source_inputs")) is not int
            or receipt["source_inputs"] != len(frozen)
            or frozen != source_inputs()):
        raise ValueError("Frozen implementation manifest/count differs from current inputs")
    if sha(frozen_bytes) != hashes["source-hashes.json"]:
        raise ValueError("Frozen input evidence changed between reads")
    identity_bytes, _ = read_regular(contained_file(directory, "distinct-tests.json"))
    identities = decode(identity_bytes)
    if (sha(identity_bytes) != hashes["distinct-tests.json"]
            or not isinstance(identities, list) or not identities
            or any(not isinstance(item, dict) or set(item) != {"crate", "harness", "name"}
                   or any(not isinstance(value, str) or not value for value in item.values())
                   for item in identities)):
        raise ValueError("Invalid or changed distinct test identities")
    keys = [(item["crate"], item["harness"], item["name"]) for item in identities]
    crate_counts = receipt.get("distinct_tests_by_crate")
    if (len(set(keys)) != len(keys) or len(keys) > passing
            or type(receipt.get("distinct_passing_rust_tests")) is not int
            or receipt["distinct_passing_rust_tests"] != len(keys)
            or not isinstance(crate_counts, dict)
            or any(not isinstance(name, str) or type(count) is not int or count <= 0
                   for name, count in crate_counts.items())
            or crate_counts != dict(Counter(key[0] for key in keys))):
        raise ValueError("Receipt distinct test counts differ")
    native_name = receipt.get("native_executable")
    native_hash = receipt.get("native_binary_sha256")
    if (not isinstance(native_name, str) or not Path(native_name).is_absolute()
            or not isinstance(native_hash, str) or SHA256.fullmatch(native_hash) is None
            or digest_regular(Path(native_name)) != native_hash):
        raise ValueError("Current accepted native executable differs")
    return {
        "receipt_sha256": sha(receipt_bytes), "native_binary_sha256": native_hash,
        "native_executable": native_name, "frozen": frozen,
        "passing_executions": passing, "distinct_tests": len(keys),
        "evidence_artifacts": len(artifacts), "gate_logs": len(gates),
        "accepted_hashes": {(directory / name).relative_to(root).as_posix(): digest
                            for name, digest in hashes.items()},
    }


def preserved_files(raw_files: dict, original_files: dict, allowed: dict,
                    new_prefixes: dict) -> tuple:
    mapping = {old: RELOCATIONS.get(old, old) for old in original_files}
    if len(set(mapping.values())) != len(mapping):
        raise ValueError("Baseline relocation aliases another original member")
    missing = sorted(old for old, current in mapping.items() if current not in raw_files)
    changed = sorted(old for old, current in mapping.items()
                     if current in raw_files
                     and raw_files[current]["sha256"] != original_files[old]["sha256"])
    unexpected = sorted(old for old in changed if old not in allowed or old in RELOCATIONS)
    if missing or unexpected:
        raise ValueError({"missing_originals": missing, "unexpected_changes": unexpected})
    unclassified = sorted(name for name in raw_files
                          if name not in mapping.values() and name not in allowed
                          and not any(name.startswith(prefix) for prefix in new_prefixes))
    if unclassified:
        raise ValueError({"unclassified_new_paths": unclassified})
    files = {name: dict(record) for name, record in raw_files.items()}
    restored_modes = []
    for old, current in mapping.items():
        mode = original_files[old]["mode"]
        if files[current]["mode"] != mode:
            restored_modes.append(current)
            files[current]["source_tree_mode"] = files[current]["mode"]
        files[current]["mode"] = mode
    for old, archived in HISTORICAL_COPIES.items():
        if (archived not in files
                or files[archived]["sha256"] != original_files[old]["sha256"]
                or files[archived]["bytes"] != original_files[old]["bytes"]):
            raise ValueError(f"Historical current document is not exact: {archived}")
        if files[archived]["mode"] != original_files[old]["mode"]:
            files[archived]["source_tree_mode"] = files[archived]["mode"]
        files[archived]["mode"] = original_files[old]["mode"]
    return files, mapping, changed, sorted(restored_modes)


def verify_prior_history(root: Path, original: Path, raw_files: dict) -> None:
    """Apply historical-entry policy to current bytes, not an earlier checkpoint."""
    with zipfile.ZipFile(original) as archive:
        for name in ("CHANGELOG.md", "docs/RELEASE_HISTORY.md"):
            prior = archive.read(PREFIX + name)
            first_entry = re.search(rb"(?m)^## ", prior)
            current, _ = read_regular(root / name)
            if (sha(current) != raw_files[name]["sha256"] or first_entry is None
                    or prior[first_entry.start():] not in current):
                raise ValueError(f"Prior release entries are missing or changed: {name}")


def verify_archive(output: Path, files: dict, manifest_bytes: bytes, validate_namespace) -> str:
    before_hash = digest_regular(output)
    with zipfile.ZipFile(output) as archive:
        expected_names = {PREFIX + name for name in files} | {PREFIX + MANIFEST}
        if set(archive.namelist()) != expected_names or len(archive.infolist()) != len(expected_names):
            raise ValueError("Archive member list differs")
        validate_namespace(archive.namelist())
        for name, expected in files.items():
            item = archive.getinfo(PREFIX + name)
            mode = item.external_attr >> 16
            data = archive.read(item)  # ZipFile also checks the CRC on the complete read.
            if (sha(data) != expected["sha256"] or len(data) != expected["bytes"]
                    or item.file_size != expected["bytes"]):
                raise ValueError(f"Packaged bytes differ: {name}")
            if (item.create_system != 3 or not stat.S_ISREG(mode)
                    or stat.S_IMODE(mode) != expected["mode"]):
                raise ValueError(f"Packaged file type/mode differs: {name}")
        item = archive.getinfo(PREFIX + MANIFEST)
        if (archive.read(item) != manifest_bytes or item.create_system != 3
                or (item.external_attr >> 16) != (stat.S_IFREG | 0o644)):
            raise ValueError("Generated archive manifest bytes/type/mode differ")
    after_hash = digest_regular(output)
    if before_hash != after_hash:
        raise ValueError("Output archive changed during member verification")
    return after_hash


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--original", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    if not root.is_dir():
        raise ValueError("Source root must be a directory")
    if args.output.is_symlink() or args.report.is_symlink() or args.original.is_symlink():
        raise ValueError("Baseline/output/report must not be symlinks")
    original = args.original.resolve(strict=True)
    output = args.output.resolve()
    report_path = args.report.resolve()
    if (output.exists() or report_path.exists() or output == report_path
            or output.is_relative_to(root) or report_path.is_relative_to(root)
            or original.is_relative_to(root) or output.suffix.lower() != ".zip"):
        raise ValueError("Use an external baseline and exclusive new output ZIP/report outside the source tree")

    sys.dont_write_bytecode = True
    sys.path.insert(0, str(root / "scripts"))
    from package_desktop import contained_file
    from release_contract import GATE_NAMES, validate_gate_results
    from verify import ROOT, source_inputs
    from verify_desktop import validate_namespace
    if ROOT.resolve() != root:
        raise ValueError("Imported verification module belongs to another source root")

    acceptance_args = (root, contained_file, validate_namespace, source_inputs,
                       validate_gate_results, GATE_NAMES)
    accepted = accepted_state(*acceptance_args)
    original_files = baseline_inventory(original, validate_namespace)
    preservation_path = root / "docs/verification/v0.24-review/preservation.json"
    preservation_bytes, _ = read_regular(preservation_path)
    preservation = decode(preservation_bytes)
    if (not isinstance(preservation, dict)
            or preservation.get("status") != "passed_checkpoint"
            or not isinstance(preservation.get("archive"), dict)
            or preservation["archive"].get("sha256") != BASELINE_SHA256
            or preservation["archive"].get("non_directory_members") != BASELINE_FILES
            or preservation.get("expected_relocations") != RELOCATIONS):
        raise ValueError("Preservation policy is not bound to this baseline and relocation")
    allowed = preservation.get("expected_edit_allowlist")
    if (not isinstance(allowed, dict)
            or any(not isinstance(name, str) or not isinstance(reason, str) or not reason.strip()
                   for name, reason in allowed.items())):
        raise ValueError("Expected edit allowlist must contain named reasons")
    validate_namespace(allowed)
    new_prefixes = preservation.get("expected_new_evidence_prefixes")
    if (not isinstance(new_prefixes, dict)
            or set(new_prefixes) != {"docs/verification/v0.24-"}
            or any(not isinstance(reason, str) or not reason.strip()
                   for reason in new_prefixes.values())):
        raise ValueError("Expected new evidence namespace policy differs")
    raw_files = source_inventory(root, validate_namespace)
    for name, digest in accepted["accepted_hashes"].items():
        if name not in raw_files or raw_files[name]["sha256"] != digest:
            raise ValueError(f"Acceptance evidence changed during source inventory: {name}")
    if raw_files[preservation_path.relative_to(root).as_posix()]["sha256"] != sha(preservation_bytes):
        raise ValueError("Preservation policy changed during source inventory")
    files, mapping, changed, restored_modes = preserved_files(
        raw_files, original_files, allowed, new_prefixes)
    verify_prior_history(root, original, raw_files)
    manifest = {
        "schema": 1, "release": RELEASE, "root_prefix": PREFIX,
        "scope": "Complete source handoff. The generated root manifest excludes itself; the exact prior manifest is retained at its named historical path.",
        "accepted_receipt_sha256": accepted["receipt_sha256"],
        "frozen_implementation_inputs": len(accepted["frozen"]),
        "accepted_evidence_artifacts": accepted["evidence_artifacts"],
        "accepted_gate_logs": accepted["gate_logs"],
        "native_binary_sha256": accepted["native_binary_sha256"],
        "original_archive_sha256": BASELINE_SHA256,
        "original_files": len(original_files),
        "original_files_unchanged": len(original_files) - len(changed),
        "original_files_changed": {name: allowed[name] for name in changed},
        "original_file_mapping": mapping,
        "original_relocations": RELOCATIONS,
        "historical_current_copies": HISTORICAL_COPIES,
        "missing_original_files": [], "unexpected_original_changes": [],
        "mode_policy": "All prior ZIP Unix modes survive through the explicit path mapping; archived current reports also retain prior modes. Other new files retain source-tree modes.",
        "original_modes_restored_from_extraction": restored_modes,
        "source_files": files,
    }
    manifest_bytes = (json.dumps(manifest, indent=2) + "\n").encode()
    output.parent.mkdir(parents=True, exist_ok=True)
    report_path.parent.mkdir(parents=True, exist_ok=True)
    with output.open("xb") as destination:
        with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
            for name, expected in files.items():
                data, info = read_regular(root / name)
                raw = raw_files[name]
                if (sha(data) != raw["sha256"] or len(data) != raw["bytes"]
                        or stat.S_IMODE(info.st_mode) != raw["mode"]):
                    raise ValueError(f"Source changed during packaging: {name}")
                member = zipfile.ZipInfo.from_file(root / name, PREFIX + name)
                member.create_system = 3
                member.external_attr = (stat.S_IFREG | expected["mode"]) << 16
                member.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(member, data)
            member = zipfile.ZipInfo(PREFIX + MANIFEST)
            member.create_system = 3
            member.external_attr = (stat.S_IFREG | 0o644) << 16
            member.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(member, manifest_bytes)
        destination.flush()
        os.fsync(destination.fileno())

    archive_hash = verify_archive(output, files, manifest_bytes, validate_namespace)
    # Re-read every source member, not just the smaller frozen implementation
    # set: documentation, historical evidence and newly added files matter too.
    if source_inventory(root, validate_namespace) != raw_files:
        raise ValueError("Complete current source inventory changed during packaging")
    if baseline_inventory(original, validate_namespace) != original_files:
        raise ValueError("Actual baseline inventory changed during packaging")
    if accepted_state(*acceptance_args) != accepted:
        raise ValueError("Acceptance receipt, evidence, implementation or native executable changed")
    # Verify the entire archive again after rereading both source trees and the
    # external native executable. Do not rely on an earlier output hash alone.
    if verify_archive(output, files, manifest_bytes, validate_namespace) != archive_hash:
        raise ValueError("Output archive changed during final source/baseline checks")
    result = {
        "status": "passed", "release": RELEASE, "archive": str(output),
        "archive_bytes": output.stat().st_size, "archive_sha256": archive_hash,
        "archive_entries": len(files) + 1, "source_files": len(files),
        "original_archive_sha256": BASELINE_SHA256,
        "original_files": len(original_files),
        "original_unchanged": len(original_files) - len(changed),
        "original_expected_changed": len(changed),
        "original_relocations": RELOCATIONS,
        "missing_original_files": [], "unexpected_original_changes": [],
        "all_archive_bytes_and_modes_verified": True,
        "original_unix_modes_preserved": True,
        "original_modes_restored_from_extraction": len(restored_modes),
        "accepted_evidence_bound_to_terminal_receipt": True,
        "frozen_inputs_unchanged": True, "complete_source_inventory_reread": True,
        "actual_baseline_reread": True, "accepted_native_reread": True,
        "archived_current_reports_exact": True,
        "accepted_receipt_sha256": accepted["receipt_sha256"],
        "frozen_implementation_inputs": len(accepted["frozen"]),
        "accepted_evidence_artifacts": accepted["evidence_artifacts"],
        "accepted_gate_logs": accepted["gate_logs"],
        "native_binary_sha256": accepted["native_binary_sha256"],
        "canonical_gates": len(GATE_NAMES),
        "distinct_passing_rust_tests": accepted["distinct_tests"],
        "passing_rust_test_executions": accepted["passing_executions"],
        "scope": "Exact observed archive bytes, portable paths, Unix modes, full source/baseline preservation and accepted-binary association. This is not publisher authentication, an atomic concurrent filesystem snapshot, power-loss certification or target-hardware qualification.",
    }
    with report_path.open("x", encoding="utf-8") as report:
        json.dump(result, report, indent=2)
        report.write("\n")
        report.flush()
        os.fsync(report.fileno())
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
