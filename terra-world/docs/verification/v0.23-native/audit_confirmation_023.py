#!/usr/bin/env python3
"""Validate the recorded 0.23 native confirmation and its accepted-source link.

Read-only inputs; Python 3.11+ and Pillow. Each output must be a new file. The
pending phase never authorizes a release. The accepted phase requires the real
successful terminal 44-gate receipt and rereads its complete evidence. Keep the
source tree, native executable and recorded captures idle during either audit.
This establishes local evidence consistency, not publisher authentication.
"""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
from io import BytesIO
import json
import os
from pathlib import Path
import re
import stat
import sys
import tomllib

from PIL import Image

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
NATIVE = Path(__file__).resolve().parent
ACCEPTANCE = ROOT / "docs/verification/v0.23-release-acceptance"
sys.path.insert(0, str(ROOT / "scripts"))
from release_contract import GATE_NAMES, validate_gate_results
from verify import ROOT as VERIFY_ROOT, source_inputs
from verify_desktop import portable_parts, validate_namespace

RELEASE = "0.23.0"
PROCESS_SPEC = {
    "confirmation-recovered": (14073, tuple(range(70, 77))),
    "confirmation-menu": (14392, (80, 81, 82)),
}
HASH = re.compile(r"[0-9a-f]{64}")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_digest(value) -> str:
    return sha(json.dumps(value, sort_keys=True, separators=(",", ":"),
                          ensure_ascii=True, allow_nan=False).encode("utf-8"))


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f"Duplicate JSON key: {key!r}")
        result[key] = value
    return result


def signature(info) -> tuple:
    return (info.st_dev, info.st_ino, info.st_mode, info.st_size,
            info.st_mtime_ns, info.st_ctime_ns)


def stable_read(path: Path, digest_only: bool = False):
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode), f"Expected regular non-link file: {path}")
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    with os.fdopen(descriptor, "rb") as stream:
        require(signature(os.fstat(stream.fileno())) == signature(before),
                f"File changed before reading: {path}")
        result = hashlib.file_digest(stream, "sha256").hexdigest() if digest_only else stream.read()
        after = os.fstat(stream.fileno())
    require(signature(after) == signature(before) == signature(path.lstat()),
            f"File changed during reading: {path}")
    return result


def timestamp(value: str) -> datetime:
    result = datetime.fromisoformat(value)
    require(result.tzinfo is not None, "Timestamp must include a timezone")
    return result


def contained(directory: Path, name: str) -> Path:
    parts = portable_parts(name)
    result = directory
    for part in parts:
        result = result / part
        require(not result.is_symlink(), f"Evidence path traverses a link: {name}")
    return result


def audit(phase: str) -> dict:
    require(VERIFY_ROOT.resolve() == ROOT, "Verification inputs belong to another checkout")
    frozen = source_inputs()
    source_digest = canonical_digest(frozen)
    evidence: dict[str, str] = {}

    def observed(path: Path) -> bytes:
        data = stable_read(path)
        key = path.relative_to(ROOT).as_posix()
        digest = sha(data)
        require(key not in evidence or evidence[key] == digest,
                f"Evidence changed between reads: {key}")
        evidence[key] = digest
        return data

    def decoded(path: Path):
        return json.loads(observed(path), object_pairs_hook=unique_object)

    accepted_map_bytes = observed(ACCEPTANCE / "source-hashes.json")
    accepted_map = json.loads(accepted_map_bytes, object_pairs_hook=unique_object)
    require(accepted_map == frozen and bool(frozen),
            "Current inputs differ from the canonical run's frozen map")

    capture_files = sorted(NATIVE.glob("*-confirmation-*-capture.json"))
    expected_indices = sorted(index for _, indices in PROCESS_SPEC.values() for index in indices)
    require([int(path.name.split("-", 1)[0]) for path in capture_files] == expected_indices,
            "Confirmation capture roster differs from the ten reviewed captures")
    by_index = {int(path.name.split("-", 1)[0]): path for path in capture_files}
    process_results = []
    capture_results = []
    binary_paths = {}
    binary_hashes = set()
    last_capture_time = None
    last_process_finish = None
    for name, (expected_pid, indices) in PROCESS_SPEC.items():
        start = decoded(NATIVE / f"{name}-start.json")
        process = decoded(NATIVE / f"{name}-process.json")
        require(isinstance(start, dict) and isinstance(process, dict), "Invalid process record")
        require({"schema", "release", "phase", "command", "working_directory", "started_utc",
                 "binary_sha256", "source_input_count", "source_inputs_sha256", "pid",
                 "trace_file", "expected_interruption"}.issubset(start),
                f"Start record is missing direct launch identity: {name}")
        require(all(process.get(key) == value for key, value in start.items()),
                f"Start and terminal process records differ: {name}")
        require(process.get("schema") == 1 and process.get("release") == RELEASE
                and process.get("phase") == "confirmation" and process.get("pid") == expected_pid
                and type(process.get("exit_code")) is int and process["exit_code"] == 0
                and process.get("expected_interruption") is False
                and process.get("trace_file") is None
                and process.get("working_directory") == str(ROOT),
                f"Expected a directly launched successful confirmation process: {name}")
        require(process.get("source_inputs_unchanged") is True
                and process.get("source_input_count") == len(frozen)
                and process.get("source_inputs_sha256") == source_digest
                and process.get("finished_source_inputs_sha256") == source_digest,
                f"Confirmation source digest differs from current canonical inputs: {name}")
        command = process.get("command")
        require(isinstance(command, list) and command and all(isinstance(part, str) for part in command)
                and Path(command[0]).is_absolute(), f"Invalid direct process command: {name}")
        if name == "confirmation-recovered":
            require("--world-profile" in command and "--menu" not in command,
                    "Recovered-world confirmation must open its recorded managed profile directly")
        else:
            require("--menu" in command, "Menu confirmation must directly launch the menu")
        executable = Path(command[0])
        executable_hash = stable_read(executable, digest_only=True)
        require(executable_hash == process.get("binary_sha256"),
                f"Current executable differs from confirmation binary: {name}")
        binary_paths[str(executable)] = executable_hash
        binary_hashes.add(executable_hash)
        log = observed(NATIVE / f"{name}.log")
        require(sha(log) == process.get("log_sha256"), f"Confirmation log hash differs: {name}")
        begun, finished = timestamp(process["started_utc"]), timestamp(process["finished_utc"])
        require(begun <= finished, f"Invalid process timestamps: {name}")
        last_process_finish = max(last_process_finish, finished) if last_process_finish else finished
        for index in indices:
            path = by_index[index]
            capture = decoded(path)
            image_name = path.name.removesuffix("-capture.json") + ".png"
            require(capture.get("schema") == 1 and capture.get("release") == RELEASE
                    and capture.get("capture_name") == image_name.removesuffix(".png")
                    and capture.get("window_pid_status") == "available"
                    and capture.get("window_pid") == expected_pid
                    and capture.get("window_pid_before") == expected_pid
                    and capture.get("window_pid_after") == expected_pid
                    and capture.get("window_pid_error") is None,
                    f"Capture is not bound to its direct confirmation PID: {image_name}")
            capture_begin = timestamp(capture["capture_started_utc"])
            capture_end = timestamp(capture["captured_utc"])
            require(begun <= capture_begin <= capture_end <= finished,
                    f"Capture falls outside its own process lifetime: {image_name}")
            require(last_capture_time is None or last_capture_time <= capture_begin,
                    f"Confirmation capture chronology differs: {image_name}")
            last_capture_time = capture_end
            image_bytes = observed(NATIVE / image_name)
            with Image.open(BytesIO(image_bytes)) as image:
                dimensions = list(image.size)
                require(image.format == "PNG", f"Expected PNG: {image_name}")
                image.verify()
            require(capture.get("window_pixels") == dimensions
                    and capture.get("image") == {"name": image_name, "sha256": sha(image_bytes),
                                                 "bytes": len(image_bytes)},
                    f"Capture image hash, size or dimensions differ: {image_name}")
            capture_results.append({"image": image_name, "receipt": path.name, "pid": expected_pid,
                                    "process": name, "pixels": dimensions,
                                    "image_sha256": sha(image_bytes),
                                    "captured_utc": capture["captured_utc"]})
        process_results.append({"name": name, "pid": expected_pid, "command": command,
                                "started_utc": process["started_utc"],
                                "finished_utc": process["finished_utc"], "exit_code": 0,
                                "log_sha256": sha(log), "binary_sha256": executable_hash,
                                "source_inputs_sha256": source_digest,
                                "capture_indices": list(indices)})
    require(len(binary_hashes) == 1, "Confirmation processes used different executables")
    binary_hash = next(iter(binary_hashes))

    acceptance = {"status": "pending_terminal_receipt", "terminal_receipt_validated": False}
    if phase == "accepted":
        receipt_bytes = observed(ACCEPTANCE / "completed-gates.json")
        receipt = json.loads(receipt_bytes, object_pairs_hook=unique_object)
        alias = observed(ACCEPTANCE / "gates.json")
        require(isinstance(receipt, dict) and alias == receipt_bytes
                and receipt.get("status") == "passed" and receipt.get("version") == RELEASE
                and receipt.get("completion_record") == "completed-gates.json"
                and all(receipt.get(key) is True for key in (
                    "source_inputs_unchanged", "published_reports_unchanged", "published_logs_unchanged")),
                "The actual successful terminal 0.23 receipt is required")
        gates = receipt.get("gates")
        require(isinstance(gates, list) and all(isinstance(gate, dict) for gate in gates)
                and len(gates) == len(GATE_NAMES) == 44, "Requires the canonical 44-gate roster")
        passing = validate_gate_results(gates)
        require(all(gate.get("timed_out") is False for gate in gates)
                and type(receipt.get("passing_rust_test_executions")) is int
                and receipt["passing_rust_test_executions"] == passing,
                "Canonical timeout or execution totals differ")
        require(type(receipt.get("source_inputs")) is int and receipt["source_inputs"] == len(frozen)
                and receipt.get("native_binary_sha256") == binary_hash,
                "Accepted source count or native binary differs from confirmation")
        require(timestamp(receipt["started_utc"]) >= last_process_finish
                and timestamp(receipt["finished_utc"]) >= timestamp(receipt["started_utc"]),
                "Expected the fresh canonical run after completed native confirmation")
        pinned = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
        require(re.fullmatch(r"rustc " + re.escape(pinned) + r" \(.+\)", receipt.get("toolchain", ""))
                is not None, "Accepted toolchain differs from pinned compiler")
        native = Path(receipt["native_executable"])
        require(native.is_absolute() and stable_read(native, digest_only=True) == binary_hash,
                "Accepted executable bytes differ from directly confirmed executable")
        binary_paths[str(native)] = binary_hash
        artifacts = receipt.get("evidence_artifacts")
        require(isinstance(artifacts, dict) and {"source-hashes.json", "distinct-tests.json"}.issubset(artifacts),
                "Canonical evidence map is incomplete")
        names = list(artifacts) + [gate.get("log") for gate in gates]
        require(all(isinstance(name, str) for name in names), "Canonical evidence names must be strings")
        validate_namespace(["completed-gates.json", "gates.json", *names])
        for name, expected in [*artifacts.items(), *((gate["log"], gate.get("log_sha256")) for gate in gates)]:
            require(isinstance(expected, str) and HASH.fullmatch(expected) is not None,
                    f"Invalid canonical evidence hash: {name}")
            require(sha(observed(contained(ACCEPTANCE, name))) == expected,
                    f"Canonical evidence differs from receipt: {name}")
        require(artifacts["source-hashes.json"] == sha(accepted_map_bytes),
                "Canonical terminal receipt does not bind the frozen source map")
        identities = decoded(ACCEPTANCE / "distinct-tests.json")
        require(isinstance(identities, list) and bool(identities)
                and all(isinstance(item, dict) and set(item) == {"crate", "harness", "name"}
                        and all(isinstance(value, str) and value for value in item.values())
                        for item in identities), "Invalid distinct test identities")
        keys = [(item["crate"], item["harness"], item["name"]) for item in identities]
        crate_counts = receipt.get("distinct_tests_by_crate")
        require(len(set(keys)) == len(keys) <= passing
                and type(receipt.get("distinct_passing_rust_tests")) is int
                and receipt["distinct_passing_rust_tests"] == len(keys)
                and isinstance(crate_counts, dict)
                and all(isinstance(name, str) and name and type(count) is int and count > 0
                        for name, count in crate_counts.items())
                and crate_counts == dict(Counter(key[0] for key in keys)),
                "Canonical distinct test counts differ")
        probes = receipt.get("startup_probes")
        require(isinstance(probes, list) and all(isinstance(probe, dict) for probe in probes)
                and [probe.get("name") for probe in probes] == ["metadata", "toolchain"],
                "Canonical startup probe roster differs")
        for probe in probes:
            require(type(probe.get("exit_code")) is int and probe["exit_code"] == 0
                    and probe.get("timed_out") is False
                    and isinstance(probe.get("log"), str) and probe["log"] in artifacts
                    and artifacts.get(probe.get("log")) == probe.get("log_sha256"),
                    "Canonical startup probe was unsuccessful or unbound")
        acceptance = {"status": "passed", "terminal_receipt_validated": True,
                      "receipt": "docs/verification/v0.23-release-acceptance/completed-gates.json",
                      "receipt_sha256": sha(receipt_bytes), "gates": len(gates),
                      "accepted_evidence_artifacts": len(artifacts),
                      "distinct_passing_rust_tests": len(keys),
                      "passing_rust_test_executions": passing,
                      "native_binary_sha256": binary_hash,
                      "source_manifest_sha256": sha(accepted_map_bytes)}

    require(source_inputs() == frozen, "Implementation inputs changed during native audit")
    for path, expected in binary_paths.items():
        require(stable_read(Path(path), digest_only=True) == expected,
                "Native executable changed during native audit")
    for name, expected in evidence.items():
        require(stable_read(ROOT / name, digest_only=True) == expected,
                f"Native or canonical evidence changed during audit: {name}")
    return {
        "schema": 1, "release": RELEASE, "phase": phase,
        "checked_utc": datetime.now(timezone.utc).isoformat(),
        "status": "passed" if phase == "accepted" else "pending_canonical_acceptance",
        "native_confirmation_checks": "passed", "capture_count": len(capture_results),
        "source_input_count": len(frozen), "source_inputs_sha256": source_digest,
        "source_manifest_sha256": sha(accepted_map_bytes),
        "source_manifest": "docs/verification/v0.23-release-acceptance/source-hashes.json",
        "native_binary_sha256": binary_hash, "processes": process_results,
        "captures": capture_results, "acceptance": acceptance,
        "evidence_sha256": dict(sorted(evidence.items())),
        "validator_sha256": stable_read(Path(__file__).resolve(), digest_only=True),
        "scope": "Exact recorded direct-process/capture association, current frozen implementation identity and executable bytes. Accepted phase additionally binds the real complete terminal gate evidence. No publisher authentication, device qualification or replay of all earlier candidate gameplay is asserted.",
        "candidate_limit": "Candidate gameplay PID 12147 retains window observations only; later confirmation does not invent its missing child process/binary/exit association.",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phase", choices=("pending", "accepted"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        require(not args.output.exists() and not args.output.is_symlink(), "Audit output must be new")
        result = audit(args.phase)
        with args.output.open("x", encoding="utf-8") as stream:
            json.dump(result, stream, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        print(json.dumps({key: result[key] for key in (
            "status", "native_confirmation_checks", "capture_count", "source_input_count",
            "source_inputs_sha256", "native_binary_sha256", "acceptance")}, indent=2))
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(json.dumps({"status": "failed", "error": str(error)}), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
