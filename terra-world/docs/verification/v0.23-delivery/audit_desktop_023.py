#!/usr/bin/env python3
"""Independently extract and verify the actual accepted 0.23 Linux desktop ZIP.

Evidence-only helper: no Cargo invocation, source edit or in-package report.
The output directory must have no previous desktop-verification/passive/smoke
results. Extraction uses a new private temporary directory and retains it for
inspection. The known prepared-library wrapper is used only if ldd reports
unresolved host dependencies. This is not clean-machine certification.
"""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import stat
import subprocess
import sys
import tempfile
import time
import zipfile
import zlib

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
ACCEPTANCE = ROOT / "docs/verification/v0.23-release-acceptance"
WRAPPER = Path("/tmp/terra023-toolchain/run.py")
LIBRARIES = "/tmp/terra023-native-deps/sysroot/usr/lib/x86_64-linux-gnu"
PREFIX = "terra-world-0.23.0-linux-x86_64/"
sys.path.insert(0, str(ROOT / "scripts"))
from release_contract import validate_gate_results
from verify import source_inputs
from verify_desktop import validate_namespace


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def unique(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f"Duplicate JSON key: {key}")
        result[key] = value
    return result


def decode(data):
    return json.loads(data, object_pairs_hook=unique)


def record_file(path):
    return {"name": path.name, "bytes": path.stat().st_size, "sha256": digest(path)}


def snapshot(directory):
    result = {}
    for path in sorted(directory.rglob("*")):
        info = path.lstat()
        require(not path.is_symlink(), f"Extracted link found: {path}")
        if stat.S_ISDIR(info.st_mode):
            continue
        require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1,
                f"Extracted special or hard-linked file: {path}")
        result[path.relative_to(directory).as_posix()] = {
            "sha256": digest(path), "bytes": info.st_size,
            "mode": oct(stat.S_IMODE(info.st_mode)),
        }
    validate_namespace(result)
    return result


def run_logged(command, cwd, timeout=45):
    begun = time.monotonic()
    process = subprocess.Popen(command, cwd=cwd, stdin=subprocess.DEVNULL,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               start_new_session=os.name == "posix")
    timed_out = False
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        if os.name == "posix":
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        else:
            process.kill()
        stdout, stderr = process.communicate(timeout=3)
    return {"command": command, "working_directory": str(cwd),
            "exit_code": process.returncode, "timed_out": timed_out,
            "seconds": round(time.monotonic() - begun, 6)}, stdout, stderr


def verify(archive_path, output):
    receipt_bytes = (ACCEPTANCE / "completed-gates.json").read_bytes()
    receipt = decode(receipt_bytes)
    frozen_bytes = (ACCEPTANCE / "source-hashes.json").read_bytes()
    frozen = decode(frozen_bytes)
    require(receipt.get("status") == "passed" and receipt.get("version") == "0.23.0"
            and receipt.get("completion_record") == "completed-gates.json"
            and receipt_bytes == (ACCEPTANCE / "gates.json").read_bytes(),
            "Actual completed 0.23 acceptance is required")
    passing = validate_gate_results(receipt["gates"])
    require(len(receipt["gates"]) == 44 and passing == receipt["passing_rust_test_executions"]
            and source_inputs() == frozen
            and sha(frozen_bytes) == receipt["evidence_artifacts"]["source-hashes.json"],
            "Current implementation or canonical evidence differs")
    allowlist_bytes = (ROOT / "scripts/desktop-payload.json").read_bytes()
    allowlist = decode(allowlist_bytes)
    declared = allowlist["files"]
    require(allowlist.get("format") == 1 and len(declared) == len(set(declared)) == 180
            and frozen["scripts/desktop-payload.json"] == sha(allowlist_bytes),
            "Expected the accepted 180-entry payload allowlist")
    evidence_names = {"completed-gates.json", "gates.json", *receipt["evidence_artifacts"],
                      *(gate["log"] for gate in receipt["gates"])}
    expected_names = {"terra-demo", "start-terra.sh", "START_HERE.txt", "package-manifest.json",
                      *declared, *("evidence/" + name for name in evidence_names)}
    validate_namespace(expected_names)
    require(len(expected_names) == 266, "Unexpected expected package inventory size")
    archive_before = record_file(archive_path)
    private = Path(tempfile.mkdtemp(prefix="terra023-desktop-independent-"))
    require(stat.S_IMODE(private.stat().st_mode) == 0o700, "Extraction parent is not private")
    extracted = private / PREFIX.rstrip("/")
    extracted.mkdir(mode=0o700)
    entries = {}
    with zipfile.ZipFile(archive_path) as bundle:
        infos = bundle.infolist()
        names = [info.filename for info in infos]
        validate_namespace(names)
        require(len(names) == len(set(names)) == 266
                and set(names) == {PREFIX + name for name in expected_names},
                "ZIP inventory, prefix or uniqueness differs")
        for info in infos:
            name = info.filename.removeprefix(PREFIX)
            mode = stat.S_IMODE(info.external_attr >> 16)
            expected_mode = 0o755 if name in {"terra-demo", "start-terra.sh"} else 0o644
            require(not info.is_dir() and info.create_system == 3
                    and stat.S_ISREG(info.external_attr >> 16) and mode == expected_mode
                    and not (info.flag_bits & 1), f"Invalid ZIP type/mode/encryption: {name}")
        require(bundle.testzip() is None, "ZIP CRC validation failed")
        manifest_bytes = bundle.read(PREFIX + "package-manifest.json")
        manifest = decode(manifest_bytes)
        require(manifest.get("release") == "0.23.0" and manifest.get("platform") == "linux-x86_64"
                and manifest.get("build_profile") == "dev"
                and manifest.get("acceptance_record_sha256") == sha(receipt_bytes)
                and manifest.get("native_binary_sha256") == receipt["native_binary_sha256"]
                and set(manifest["files_sha256"]) == expected_names - {"package-manifest.json"},
                "Package manifest does not bind the exact acceptance/inventory")
        for info in infos:
            name = info.filename.removeprefix(PREFIX)
            destination = extracted / name
            destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            count = 0
            crc = 0
            hashed = hashlib.sha256()
            with bundle.open(info) as source, destination.open("xb") as target:
                for chunk in iter(lambda: source.read(1024 * 1024), b""):
                    target.write(chunk)
                    hashed.update(chunk)
                    count += len(chunk)
                    crc = zlib.crc32(chunk, crc)
            mode = stat.S_IMODE(info.external_attr >> 16)
            destination.chmod(mode)
            actual_hash = hashed.hexdigest()
            require(count == info.file_size and crc & 0xffffffff == info.CRC,
                    f"Extracted size/CRC differs: {name}")
            if name != "package-manifest.json":
                require(actual_hash == manifest["files_sha256"][name], f"Manifest byte hash differs: {name}")
            entries[name] = {"sha256": actual_hash, "bytes": count, "mode": oct(mode),
                             "zip_crc32": f"{info.CRC:08x}"}

    for name in declared:
        require(digest(ROOT / name) == entries[name]["sha256"], f"Declared source payload differs: {name}")
    for name in evidence_names:
        require(digest(ACCEPTANCE / name) == entries["evidence/" + name]["sha256"],
                f"Packaged acceptance evidence differs from actual accepted bytes: {name}")
    require((extracted / "evidence/completed-gates.json").read_bytes() == receipt_bytes
            and (extracted / "evidence/gates.json").read_bytes() == receipt_bytes
            and (extracted / "evidence/source-hashes.json").read_bytes() == frozen_bytes
            and entries["terra-demo"]["sha256"] == receipt["native_binary_sha256"],
            "Packaged terminal receipt, source map or native binary differs")
    before = snapshot(extracted)
    require(before == {name: {key: item[key] for key in ("sha256", "bytes", "mode")}
                       for name, item in entries.items()}, "Extracted inventory/modes differ before execution")

    library_run, libraries_stdout, libraries_stderr = run_logged(
        ["ldd", str(extracted / "terra-demo")], extracted, timeout=10)
    with (output / "desktop-host-libraries.log").open("xb") as stream:
        stream.write(libraries_stdout + libraries_stderr)
    missing = [line.strip() for line in (libraries_stdout + libraries_stderr).decode(errors="replace").splitlines()
               if "not found" in line]
    prepared = bool(missing or library_run["exit_code"] != 0 or library_run["timed_out"])
    if prepared:
        require(WRAPPER.is_file(), "Host dependencies are missing and the prepared wrapper is unavailable")
    verifier = str(extracted / "scripts/verify_desktop.py")
    results = {}
    for kind in ("passive", "smoke"):
        command = [sys.executable, "-B", verifier, "--directory", str(extracted)]
        if kind == "smoke":
            command += ["--smoke-help", "--timeout-seconds", "20"]
            if prepared:
                command = [sys.executable, "-B", str(WRAPPER), *command]
        execution, stdout, stderr = run_logged(command, extracted)
        result_path = output / f"desktop-{kind}.json"
        log_path = output / f"desktop-{kind}.stderr.log"
        with result_path.open("xb") as stream:
            stream.write(stdout)
        with log_path.open("xb") as stream:
            stream.write(stderr)
        observed = decode(stdout)
        require(execution["exit_code"] == 0 and not execution["timed_out"]
                and observed.get("status") == "passed"
                and observed.get("native_binary_sha256") == receipt["native_binary_sha256"]
                and observed.get("acceptance_record_sha256") == sha(receipt_bytes)
                and observed.get("files_verified") == 265,
                f"Shipped {kind} verification failed; retained raw result and stderr")
        if kind == "smoke":
            smoke = observed["smoke_help"]
            require(smoke.get("status") == "passed" and smoke.get("exit_code") == 0
                    and smoke.get("output_limit_bytes") == 65536
                    and 0 < smoke.get("output_bytes", 0) <= 65536
                    and smoke.get("descendant_check", {}).get("status") == "owned_process_group_absent",
                    "Finite Linux help workload did not finish with its owned group absent")
        else:
            require(observed.get("smoke_help", {}).get("status") == "not_requested",
                    "Passive verification unexpectedly executed the binary")
        results[kind] = {**execution, "result": record_file(result_path),
                         "stderr": record_file(log_path), "reported": observed}

    after = snapshot(extracted)
    require(after == before, "Extracted payload bytes, inventory or modes changed during verification")
    require(record_file(archive_path) == archive_before, "Original desktop ZIP changed during verification")
    require(source_inputs() == frozen and (ACCEPTANCE / "completed-gates.json").read_bytes() == receipt_bytes,
            "Frozen source or accepted receipt changed during verification")
    for name in declared:
        require(digest(ROOT / name) == entries[name]["sha256"], f"Declared source payload changed: {name}")
    return {
        "schema": 1, "release": "0.23.0", "status": "passed",
        "checked_utc": datetime.now(timezone.utc).isoformat(),
        "archive": {"path": str(archive_path), **archive_before},
        "package_prefix": PREFIX, "archive_entries": len(entries),
        "uncompressed_bytes": sum(item["bytes"] for item in entries.values()),
        "unix_mode_counts": dict(Counter(item["mode"] for item in entries.values())),
        "all_entries_regular_portable_unique": True,
        "zip_crc_and_streamed_crc_checks_passed": True,
        "all_manifest_hashes_and_sizes_matched": True,
        "declared_payload_entries": len(declared),
        "declared_payload_allowlist_sha256": sha(allowlist_bytes),
        "all_declared_payload_bytes_match_source": True,
        "accepted_evidence_files": len(evidence_names),
        "all_packaged_evidence_bytes_match_actual_canonical_files": True,
        "acceptance_record_sha256": sha(receipt_bytes),
        "native_binary_sha256": receipt["native_binary_sha256"],
        "canonical_gates": len(receipt["gates"]),
        "distinct_passing_rust_tests": receipt["distinct_passing_rust_tests"],
        "passing_rust_test_executions": passing,
        "source_inputs": len(frozen), "source_manifest_sha256": sha(frozen_bytes),
        "private_extraction_parent": str(private), "private_parent_mode": "0o700",
        "extracted_package": str(extracted),
        "environment": {
            "os": platform.system(), "architecture": platform.machine(), "python": platform.python_version(),
            "smoke_mode": "prepared_library_wrapper" if prepared else "current_host_environment",
            "host_library_check": {**library_run, "log": record_file(output / "desktop-host-libraries.log"),
                                   "missing_libraries": missing},
            "wrapper": str(WRAPPER) if prepared else None,
            "wrapper_sha256": digest(WRAPPER) if prepared else None,
            "runtime_library_prefix": LIBRARIES if prepared else None,
            "inherited_LD_LIBRARY_PATH": os.environ.get("LD_LIBRARY_PATH"),
            "effective_smoke_LD_LIBRARY_PATH": LIBRARIES + ":" + os.environ.get("LD_LIBRARY_PATH", "") if prepared else os.environ.get("LD_LIBRARY_PATH"),
            "wrapper_boundary": "The recorded existing wrapper prepends its isolated native library/bin and Cargo paths, sets its build-environment variables and disables bytecode writes. This audit invokes only Python, ldd and the shipped --help binary; no Cargo build or tests run." if prepared else "Current container dependencies only; no clean-machine claim.",
        },
        "executions": results,
        "all_extracted_bytes_modes_inventory_unchanged_afterward": True,
        "original_archive_unchanged_afterward": True,
        "implementation_inputs_and_canonical_receipt_unchanged": True,
        "entry_inventory": dict(sorted(entries.items())),
        "auditor_sha256": digest(Path(__file__).resolve()),
        "scope": "Exact independently inspected ZIP and extracted payload; shipped passive verifier and finite Linux command-line help. No native window, saved world, physical device, Windows, clean-machine install or publisher authentication is qualified by this run.",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    output = args.output_dir.resolve(strict=True)
    for name in ("desktop-verification.json", "desktop-passive.json", "desktop-passive.stderr.log",
                 "desktop-smoke.json", "desktop-smoke.stderr.log", "desktop-host-libraries.log"):
        require(not (output / name).exists(), f"Refusing to overwrite retained evidence: {name}")
    result = verify(args.archive.resolve(strict=True), output)
    with (output / "desktop-verification.json").open("x", encoding="utf-8") as stream:
        json.dump(result, stream, indent=2)
        stream.write("\n")
    print(json.dumps({key: result[key] for key in (
        "status", "archive", "archive_entries", "uncompressed_bytes", "unix_mode_counts",
        "declared_payload_entries", "accepted_evidence_files", "native_binary_sha256",
        "acceptance_record_sha256", "extracted_package", "environment")}, indent=2))


if __name__ == "__main__":
    main()
