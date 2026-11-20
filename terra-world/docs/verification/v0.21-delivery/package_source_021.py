#!/usr/bin/env python3
"""Build and re-read the complete 0.21 source archive after final acceptance.

This handoff helper lives outside the frozen implementation set. It requires
the actual completed receipt and compares every original ZIP file again.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import stat
import sys
import zipfile


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--original", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    if args.output.is_symlink() or args.report.is_symlink():
        raise ValueError("Output/report cannot be symlinks")
    output = args.output.resolve()
    report_path = args.report.resolve()
    if (output.exists() or report_path.exists() or output == report_path
            or output.is_relative_to(root) or report_path.is_relative_to(root)):
        raise ValueError("Use new output/report paths outside the source tree")
    sys.path.insert(0, str(root / "scripts"))
    from package_desktop import contained_file, validate_namespace
    from release_contract import validate_gate_results
    from verify import source_inputs

    acceptance_dir = root / "docs/verification/v0.21-final-acceptance"
    receipt_bytes = (acceptance_dir / "completed-gates.json").read_bytes()
    receipt = json.loads(receipt_bytes)
    if (receipt.get("status") != "passed" or receipt.get("version") != "0.21.0"
            or any(receipt.get(key) is not True for key in (
                "source_inputs_unchanged", "published_reports_unchanged", "published_logs_unchanged"
            ))):
        raise ValueError("Requires completed 0.21 acceptance")
    if (receipt.get("completion_record") != "completed-gates.json"
            or (acceptance_dir / "gates.json").read_bytes() != receipt_bytes):
        raise ValueError("Terminal receipt and progress alias differ")
    passing = validate_gate_results(receipt["gates"])
    if (type(receipt.get("passing_rust_test_executions")) is not int
            or receipt["passing_rust_test_executions"] != passing):
        raise ValueError("Receipt execution count differs")
    accepted_hashes = {
        "completed-gates.json": sha(receipt_bytes), "gates.json": sha(receipt_bytes)
    }
    for name, expected in receipt["evidence_artifacts"].items():
        if sha(contained_file(acceptance_dir, name).read_bytes()) != expected:
            raise ValueError(f"Accepted evidence changed: {name}")
        accepted_hashes[name] = expected
    for gate in receipt["gates"]:
        if sha(contained_file(acceptance_dir, gate["log"]).read_bytes()) != gate["log_sha256"]:
            raise ValueError(f"Accepted gate log changed: {gate['name']}")
        accepted_hashes[gate["log"]] = gate["log_sha256"]
    if not {"source-hashes.json", "distinct-tests.json"}.issubset(accepted_hashes):
        raise ValueError("Required hashed source/test evidence is missing")
    identities = json.loads((acceptance_dir / "distinct-tests.json").read_bytes())
    if (not isinstance(identities, list) or not identities
            or any(not isinstance(item, dict) or set(item) != {"crate", "harness", "name"}
                   or any(not isinstance(value, str) or not value for value in item.values())
                   for item in identities)):
        raise ValueError("Invalid distinct test identities")
    keys = [(item["crate"], item["harness"], item["name"]) for item in identities]
    if (len(set(keys)) != len(keys) or len(keys) > passing
            or type(receipt.get("distinct_passing_rust_tests")) is not int
            or receipt["distinct_passing_rust_tests"] != len(keys)
            or receipt.get("distinct_tests_by_crate") != dict(Counter(item[0] for item in keys))):
        raise ValueError("Receipt distinct test counts differ")
    if sha(Path(receipt["native_executable"]).read_bytes()) != receipt["native_binary_sha256"]:
        raise ValueError("Accepted executable changed")
    frozen = json.loads((acceptance_dir / "source-hashes.json").read_bytes())
    if frozen != source_inputs():
        raise ValueError("Implementation inputs changed after acceptance")

    preservation = json.loads(
        (root / "docs/verification/v0.21-review/preservation.json").read_bytes()
    )
    allowed = dict(preservation["expected_edit_allowlist"])
    allowed["docs/VALIDATION.md"] = "Final current execution/qualification handoff"
    allowed["scripts/desktop-payload.json"] = "Include 0.21 handoff and preserved 0.20 reports"

    files = {}
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root)
        if {".git", "target", "__pycache__"}.intersection(relative.parts):
            continue
        if path.suffix in {".pyc", ".pyo"}:
            continue
        if path.is_symlink():
            raise ValueError(f"Source archive rejects symlink: {relative}")
        if path.is_file():
            data = path.read_bytes()
            files[relative.as_posix()] = {
                "sha256": sha(data), "bytes": len(data),
                "mode": stat.S_IMODE(path.stat().st_mode),
            }
    validate_namespace(files)
    for name, expected in accepted_hashes.items():
        relative = (acceptance_dir / name).relative_to(root).as_posix()
        if files[relative]["sha256"] != expected:
            raise ValueError(f"Acceptance evidence changed during inventory: {name}")
    original_files = {}
    original_modes = {}
    with zipfile.ZipFile(args.original) as original:
        for item in original.infolist():
            if item.is_dir():
                continue
            if not item.filename.startswith("terra-world/"):
                raise ValueError("Unexpected original prefix")
            name = item.filename.removeprefix("terra-world/")
            if name in original_files:
                raise ValueError("Duplicate original entry")
            original_files[name] = sha(original.read(item))
            original_modes[name] = stat.S_IMODE(item.external_attr >> 16)
    missing = sorted(set(original_files) - set(files))
    changed = [name for name, digest in original_files.items()
               if name in files and files[name]["sha256"] != digest]
    unexpected = sorted(set(changed) - set(allowed))
    if missing or unexpected:
        raise ValueError({"missing_originals": missing, "unexpected_changes": unexpected})
    restored_original_modes = []
    for name, original_mode in original_modes.items():
        if files[name]["mode"] != original_mode:
            restored_original_modes.append(name)
            files[name]["source_tree_mode"] = files[name]["mode"]
        files[name]["mode"] = original_mode
    for old, archived in [
        ("docs/PRODUCT_REVIEW.md", "docs/PRODUCT_REVIEW_v0.20.md"),
        ("docs/VALIDATION.md", "docs/VALIDATION_v0.20.md"),
    ]:
        if files[archived]["sha256"] != original_files[old]:
            raise ValueError(f"Historical current document changed: {archived}")

    manifest = {
        "schema": 1, "release": "0.21.0", "root_prefix": "terra-world/",
        "scope": "Complete source handoff with original content and recorded current evidence. The generated manifest excludes itself.",
        "accepted_receipt_sha256": sha(receipt_bytes),
        "frozen_implementation_inputs": len(frozen),
        "native_binary_sha256": receipt["native_binary_sha256"],
        "original_archive_sha256": sha(args.original.read_bytes()),
        "original_files": len(original_files),
        "original_files_unchanged": len(original_files) - len(changed),
        "original_files_changed": {name: allowed[name] for name in sorted(changed)},
        "missing_original_files": missing,
        "unexpected_original_changes": unexpected,
        "mode_policy": "Original ZIP Unix modes preserved for all original files; new files retain source-tree modes.",
        "original_modes_restored_from_extraction": restored_original_modes,
        "source_files": files,
    }
    manifest_bytes = (json.dumps(manifest, indent=2) + "\n").encode()
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for name, expected in files.items():
            path = root / name
            if sha(path.read_bytes()) != expected["sha256"]:
                raise ValueError(f"Source changed during packaging: {name}")
            info = zipfile.ZipInfo.from_file(path, f"terra-world/{name}")
            info.create_system = 3
            info.external_attr = (stat.S_IFREG | expected["mode"]) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, path.read_bytes())
        info = zipfile.ZipInfo("terra-world/DELIVERY_MANIFEST.json")
        info.create_system = 3
        info.external_attr = (stat.S_IFREG | 0o644) << 16
        info.compress_type = zipfile.ZIP_DEFLATED
        archive.writestr(info, manifest_bytes)
    with zipfile.ZipFile(output) as archive:
        expected_names = {f"terra-world/{name}" for name in files}
        expected_names.add("terra-world/DELIVERY_MANIFEST.json")
        if set(archive.namelist()) != expected_names or len(archive.infolist()) != len(expected_names):
            raise ValueError("Archive member list differs")
        validate_namespace(archive.namelist())
        if archive.testzip() is not None:
            raise ValueError("Archive CRC verification failed")
        for name, expected in files.items():
            item = archive.getinfo(f"terra-world/{name}")
            if sha(archive.read(item)) != expected["sha256"]:
                raise ValueError(f"Packaged bytes differ: {name}")
            if stat.S_IMODE(item.external_attr >> 16) != expected["mode"]:
                raise ValueError(f"Packaged mode differs: {name}")
        if archive.read("terra-world/DELIVERY_MANIFEST.json") != manifest_bytes:
            raise ValueError("Archive manifest differs")
    if source_inputs() != frozen:
        raise ValueError("Implementation changed while packaging")
    result = {
        "status": "passed", "release": "0.21.0", "archive": str(output),
        "archive_bytes": output.stat().st_size, "archive_sha256": sha(output.read_bytes()),
        "archive_entries": len(files) + 1, "source_files": len(files),
        "original_files": len(original_files),
        "original_unchanged": len(original_files) - len(changed),
        "original_expected_changed": len(changed),
        "missing_original_files": missing, "unexpected_original_changes": unexpected,
        "all_archive_bytes_and_modes_verified": True,
        "original_unix_modes_preserved": True,
        "original_modes_restored_from_extraction": len(restored_original_modes),
        "accepted_evidence_bound_to_terminal_receipt": True,
        "frozen_inputs_unchanged": True, "archived_current_reports_exact": True,
        "accepted_receipt_sha256": sha(receipt_bytes),
        "scope": "Exact archive bytes, portable namespace, file modes and preservation against supplied source. Integrity checks do not authenticate a publisher or qualify target hardware.",
    }
    report_path.parent.mkdir(parents=True, exist_ok=True)
    with report_path.open("x") as report:
        json.dump(result, report, indent=2)
        report.write("\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
