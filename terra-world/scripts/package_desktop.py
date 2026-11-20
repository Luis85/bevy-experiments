#!/usr/bin/env python3
"""Package the exact accepted native executable with its original runtime data.

Usage: python3 scripts/package_desktop.py --gates PATH/completed-gates.json --output NEW.zip
The completed local release gates, platform and executable must match. This does
not certify GPU/device performance, sign a Windows binary or run a native UI.
The output is published only to a new filename after complete ZIP validation.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import stat
import tempfile
import zipfile

from check_compatibility import check as check_compatibility
from release_contract import validate_gate_results
from verify import ROOT, source_inputs
from verify_desktop import (executable_platform, portable_parts, validate_namespace,
                            execution_required, validate_receipt_execution)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def contained_file(directory: Path, name: str) -> Path:
    path = directory.joinpath(*portable_parts(name))
    current = path
    while current != directory:
        if current.is_symlink():
            raise ValueError(f"Payload path must not traverse a symlink: {name}")
        current = current.parent
    if not path.is_file() or not path.resolve().is_relative_to(directory.resolve()):
        raise ValueError(f"Missing or escaped payload file: {name}")
    return path


def package(gates_path: Path, output: Path) -> dict:
    gates_path = gates_path.resolve(strict=True)
    output = output.absolute()
    if output.exists() or output.is_symlink():
        raise ValueError("Output must be a new path; existing archives are never replaced")
    if output.suffix.lower() != ".zip":
        raise ValueError("Output must end in .zip")
    contract = check_compatibility()["contract"]
    report_bytes = gates_path.read_bytes()
    report = json.loads(report_bytes)
    completion_bytes = None
    if "completion_record" in report:
        # New runners name a separately published terminal record. A passed
        # progress alias cannot authorize release after that publication failed.
        if report["completion_record"] != "completed-gates.json":
            raise ValueError("Completion record must be completed-gates.json")
        completion_bytes = contained_file(gates_path.parent, "completed-gates.json").read_bytes()
        if completion_bytes != report_bytes:
            raise ValueError("Completion record differs from supplied acceptance receipt")
    compiler = re.fullmatch(r"rustc (\S+) \(.+\)", report.get("toolchain", ""))
    if compiler is None or compiler.group(1) != contract["rust_acceptance"]:
        raise ValueError("Recorded compiler differs from the pinned acceptance toolchain")
    if (report.get("status") != "passed" or report.get("version") != contract["release"]
            or report.get("source_inputs_unchanged") is not True
            or report.get("published_reports_unchanged") is not True
            or report.get("published_logs_unchanged") is not True):
        raise ValueError("Packaging requires completed, unchanged gates for this release")
    gates = report.get("gates")
    if not isinstance(gates, list) or not all(isinstance(gate, dict) for gate in gates):
        raise ValueError("Acceptance gates must be a complete list of gate records")
    passing_executions = validate_gate_results(
        gates, require_execution=execution_required(report.get("version")),
        platform=report.get("environment", {}).get("os"),
        output_limit_bytes=report.get("environment", {}).get("gate_output_limit_bytes"),
    )
    if (type(report.get("passing_rust_test_executions")) is not int
            or report["passing_rust_test_executions"] != passing_executions):
        raise ValueError("Recorded Rust execution total differs from acceptance gate counts")
    source_path = gates_path.parent / "source-hashes.json"
    source_bytes = source_path.read_bytes()
    if report.get("evidence_artifacts", {}).get("source-hashes.json") != sha(source_bytes):
        raise ValueError("Source manifest does not match its accepted gate record")
    before = source_inputs()
    if json.loads(source_bytes) != before:
        raise ValueError("Current source differs from the accepted executable inputs")
    evidence = {"gates.json": report_bytes, "source-hashes.json": source_bytes}
    if completion_bytes is not None:
        evidence["completed-gates.json"] = completion_bytes
    log_names = [gate["log"] for gate in report.get("gates", [])]
    artifact_names = [name for name in report.get("evidence_artifacts", {}) if name != "source-hashes.json"]
    validate_namespace([*evidence, *log_names, *artifact_names])
    for gate in report.get("gates", []):
        if gate.get("exit_code") != 0 or gate.get("failed_tests") or gate.get("ignored_tests"):
            raise ValueError("An acceptance gate was not fully successful")
        data = contained_file(gates_path.parent, gate["log"]).read_bytes()
        if sha(data) != gate.get("log_sha256"):
            raise ValueError(f"Gate log changed: {gate['name']}")
        evidence[gate["log"]] = data
    for name, expected in report.get("evidence_artifacts", {}).items():
        data = contained_file(gates_path.parent, name).read_bytes()
        if sha(data) != expected:
            raise ValueError(f"Gate evidence changed: {name}")
        evidence[name] = data
    validate_receipt_execution(report, {name: len(data) for name, data in evidence.items()})
    identities_bytes = evidence.get("distinct-tests.json")
    if identities_bytes is None:
        raise ValueError("Executed Rust acceptance requires hashed distinct-tests.json evidence")
    identities = json.loads(identities_bytes)
    if (not isinstance(identities, list) or not identities
            or not all(isinstance(item, dict)
                       and set(item) == {"crate", "harness", "name"}
                       and all(isinstance(value, str) and value for value in item.values())
                       for item in identities)):
        raise ValueError("Invalid distinct Rust test identity evidence")
    identity_keys = [(item["crate"], item["harness"], item["name"]) for item in identities]
    recorded_count = report.get("distinct_passing_rust_tests")
    crate_counts = report.get("distinct_tests_by_crate")
    if (len(set(identity_keys)) != len(identity_keys)
            or type(recorded_count) is not int or recorded_count != len(identity_keys)
            or recorded_count > passing_executions
            or not isinstance(crate_counts, dict)
            or not all(isinstance(name, str) and name and type(count) is int and count > 0
                       for name, count in crate_counts.items())
            or crate_counts != dict(Counter(item[0] for item in identity_keys))):
        raise ValueError("Distinct Rust test identity evidence differs from recorded counts")
    native = [g for g in gates if g["name"] == "native-build"]
    if len(native) != 1 or report.get("distinct_passing_rust_tests", 0) < 1:
        raise ValueError("Native build and executed Rust acceptance are required")
    executable = Path(report["native_executable"])
    binary = executable.read_bytes()
    if sha(binary) != report.get("native_binary_sha256"):
        raise ValueError("Executable differs from the accepted native artifact")
    target = executable_platform(binary)
    expected_os = "Windows" if target.startswith("windows") else "Linux"
    if (report.get("environment", {}).get("os") != expected_os
            or platform.system() != expected_os):
        raise ValueError("Package platform must match the machine that executed its gates")
    profile = "release" if "--release" in native[0]["command"] else "dev"
    prefix = f"terra-world-{contract['release']}-{target}"
    files: dict[str, bytes] = {"terra-demo.exe" if expected_os == "Windows" else "terra-demo": binary}
    # Keep only reviewed paths used by presets/content and named guides. A new
    # JSON beside a fixture is not automatically approved for distribution.
    payload = json.loads((ROOT / "scripts/desktop-payload.json").read_bytes())
    names = payload.get("files", [])
    if (payload.get("format") != 1 or not names or not all(isinstance(n, str) for n in names)
            or len(names) != len(set(names))):
        raise ValueError("Invalid or duplicate desktop payload allowlist")
    for name in names:
        if name in files or name.startswith("evidence/") or name in {"START_HERE.txt", "package-manifest.json", "Start-Terra.cmd", "start-terra.sh"}:
            raise ValueError(f"Payload collides with a generated file: {name}")
        files[name] = contained_file(ROOT, name).read_bytes()
    if expected_os == "Windows":
        files["Start-Terra.cmd"] = (
            '@echo off\r\nsetlocal\r\ncd /d "%~dp0"\r\n'
            'if not defined LOCALAPPDATA (\r\n  echo LOCALAPPDATA is unavailable. Choose --user-data explicitly.\r\n  exit /b 1\r\n)\r\n'
            '"%~dp0terra-demo.exe" --menu --user-data "%LOCALAPPDATA%\\TerraWorld" %*\r\n'
            'exit /b %ERRORLEVEL%\r\n'
        ).encode()
        start = "Start-Terra.cmd"
        saves = r"%LOCALAPPDATA%\TerraWorld"
    else:
        files["start-terra.sh"] = (
            '#!/bin/sh\nset -eu\ncd -- "$(dirname -- "$0")"\n'
            'exec ./terra-demo --menu --user-data "${XDG_DATA_HOME:-$HOME/.local/share}/terra-world" "$@"\n'
        ).encode()
        start = "./start-terra.sh"
        saves = "${XDG_DATA_HOME:-$HOME/.local/share}/terra-world"
    files["START_HERE.txt"] = (
        f"Terra World {contract['release']} | {target}\n\n"
        f"Extract the complete folder, then run {start}. Keep the data directories alongside the executable.\n"
        f"The launcher stores worlds, preferences and private identities under {saves}.\n"
        "Keep this extracted folder in a writable location; default direct-play cache/save paths use the working directory.\n"
        "For command-line play, run terra-demo[.exe] from this folder and choose --user-data and save paths explicitly.\n\n"
        f"This package contains the exact {profile}-profile executable from the included accepted gate record.\n"
        "Automated acceptance does not certify native window/input/GPU behavior on every device.\n"
        "Read docs/DESKTOP_DELIVERY.md for qualification, controls, save/upgrade and recovery guidance.\n"
        "Start a review with docs/PLAYTESTING.md: first-session route, optional local reports, save/reopen checks and issue template.\n"
        "Current player guides are included. Developer source links, historical captures and earlier evidence archives refer to the companion complete source project.\n"
        "Before launch, use Python 3.11+: python scripts/verify_desktop.py\n"
        "For a bounded command-line startup check on this OS: python scripts/verify_desktop.py --smoke-help\n"
        "Keep the package unchanged during verification; save verifier output outside this folder.\n"
        "These checks establish byte consistency, not publisher trust or hardware qualification.\n"
        "See docs/COMPATIBILITY.md before connecting to a host. Windows signing and an installer are separate work.\n"
    ).encode()
    for name, data in evidence.items():
        files[f"evidence/{name}"] = data
    manifest = {
        "release": contract["release"], "platform": target, "build_profile": profile,
        "compatibility": contract, "native_binary_sha256": sha(binary),
        "acceptance_record_sha256": sha(report_bytes),
        "files_sha256": {name: sha(content) for name, content in sorted(files.items())},
        "scope": "Accepted executable and original runtime data. No saved worlds, tokens or device certification.",
        "runtime": "System graphics/input/C runtime libraries are not bundled. Qualify the extracted package on its intended OS, GPU and input devices.",
    }
    files["package-manifest.json"] = (json.dumps(manifest, indent=2) + "\n").encode()
    validate_namespace(files)
    if source_inputs() != before:
        raise ValueError("Source changed while preparing the package")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".terra-package-", dir=output.parent) as temporary:
        archive = Path(temporary) / "desktop.zip"
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as bundle:
            for name, data in sorted(files.items()):
                entry = zipfile.ZipInfo(f"{prefix}/{name}", date_time=(2026, 1, 1, 0, 0, 0))
                entry.create_system = 3
                mode = 0o755 if name in {"terra-demo", "start-terra.sh"} else 0o644
                entry.external_attr = (stat.S_IFREG | mode) << 16
                entry.compress_type = zipfile.ZIP_DEFLATED
                bundle.writestr(entry, data)
        with zipfile.ZipFile(archive) as bundle:
            if bundle.testzip() is not None or len(bundle.namelist()) != len(files):
                raise ValueError("Packaged ZIP failed integrity validation")
            for name, data in files.items():
                if bundle.read(f"{prefix}/{name}") != data:
                    raise ValueError(f"Packaged bytes changed: {name}")
        # Same-filesystem publication cannot replace an existing archive.
        os.link(archive, output)
    return {"status": "passed", "output": str(output), "release": contract["release"],
            "platform": target, "build_profile": profile, "files": len(files),
            "bytes": output.stat().st_size, "sha256": sha(output.read_bytes()),
            "native_binary_sha256": sha(binary)}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gates", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        print(json.dumps(package(args.gates, args.output), indent=2))
    except (OSError, ValueError, KeyError, zipfile.BadZipFile) as error:
        parser.exit(1, f"Desktop packaging failed: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
