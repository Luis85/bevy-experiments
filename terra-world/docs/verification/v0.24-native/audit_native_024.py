#!/usr/bin/env python3
"""Audit only explicitly reviewed final Terra 0.24 native confirmations.

Python 3.11+ and Pillow; read-only inputs and one exclusively new output. Supply
an actual reviewed registry with schema/release/reviewed_utc/reviewer and a
processes list. Each process supplies name, phase="confirmation", pid and captures;
each capture supplies stem, pixels=[width,height] and the reviewed image sha256.
No registry or capture is invented by this helper. Historical baseline and
candidate records are never promoted to final confirmation.

The accepted phase calls the complete current source-delivery receipt validator
and rereads all associated evidence. Missing canonical acceptance cannot pass.
Pending mode is explicitly non-authorizing, even when native checks succeed.
Keep the source, binary, registry and evidence idle while this audit runs.
Process wall-clock duration must agree with the recorded monotonic duration
within two seconds. This assumes UTC did not step relative to the monotonic
clock by more than that tolerance during the recorded process lifetime.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
from io import BytesIO
import json
import math
from pathlib import Path
import re
import sys
import tomllib

from PIL import Image

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
NATIVE = Path(__file__).resolve().parent
ACCEPTANCE = ROOT / "docs/verification/v0.24-release-acceptance"
SOURCE_AUDITOR = ROOT / "docs/verification/v0.24-delivery/package_source_024.py"
sys.path.insert(0, str(ROOT / "scripts"))
from package_desktop import contained_file
from release_contract import GATE_NAMES, validate_gate_results
from verify import ROOT as VERIFY_ROOT, source_inputs
from verify_desktop import portable_parts, validate_namespace
from verify_desktop_archive import publish_report

RELEASE = "0.24.0"
HASH = re.compile(r"[0-9a-f]{64}")
NAME = re.compile(r"confirmation-[a-z0-9]+(?:-[a-z0-9]+)*")
STEM = re.compile(r"[0-9]{2,3}-confirmation-[a-z0-9]+(?:-[a-z0-9]+)*")
MAX_JSON_BYTES = 1024 * 1024
MAX_NATIVE_FILE_BYTES = 64 * 1024 * 1024
MAX_LIFETIME_CLOCK_DRIFT_SECONDS = 2.0

# Reuse the reviewed 0.24 canonical validator and stable file readers. Its main
# function is not called, and importing it neither packages nor writes anything.
_spec = importlib.util.spec_from_file_location("terra_source_auditor_024", SOURCE_AUDITOR)
if _spec is None or _spec.loader is None:
    raise ImportError("The current source-delivery auditor is unavailable")
source_auditor = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(source_auditor)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_digest(value) -> str:
    return sha(json.dumps(value, sort_keys=True, separators=(",", ":"),
                          ensure_ascii=True, allow_nan=False).encode())


def decode(data: bytes):
    def invalid_constant(value):
        raise ValueError(f"Nonfinite JSON value: {value}")

    return json.loads(data, object_pairs_hook=source_auditor.unique_object,
                      parse_constant=invalid_constant)


def timestamp(value) -> datetime:
    require(isinstance(value, str), "Timestamp must be a string")
    parsed = datetime.fromisoformat(value)
    require(parsed.tzinfo is not None, "Timestamp must include a timezone")
    return parsed


def positive_int(value) -> bool:
    return type(value) is int and value > 0


def checked_registry(registry: dict) -> list[dict]:
    require(isinstance(registry, dict) and type(registry.get("schema")) is int
            and registry["schema"] == 1
            and registry.get("release") == RELEASE,
            "An explicit reviewed 0.24 native registry is required")
    timestamp(registry.get("reviewed_utc"))
    require(isinstance(registry.get("reviewer"), str) and registry["reviewer"].strip(),
            "The registry must name its visual reviewer")
    processes = registry.get("processes")
    require(isinstance(processes, list) and 1 <= len(processes) <= 32,
            "Registry must contain 1..32 explicitly reviewed processes")
    names, pids, stems, indices = set(), set(), set(), set()
    namespace = []
    for process in processes:
        require(isinstance(process, dict), "Registry process must be an object")
        name, pid = process.get("name"), process.get("pid")
        require(isinstance(name, str) and NAME.fullmatch(name) is not None
                and process.get("phase") == "confirmation" and positive_int(pid),
                "Only named direct final confirmation processes may be registered")
        require(name not in names and pid not in pids, "Registry process name/PID is repeated")
        names.add(name)
        pids.add(pid)
        namespace.extend([f"{name}-start.json", f"{name}-process.json", f"{name}.log"])
        captures = process.get("captures")
        require(isinstance(captures, list) and captures, "Every registered process needs a reviewed capture")
        for capture in captures:
            require(isinstance(capture, dict), "Registry capture must be an object")
            stem, pixels, digest = capture.get("stem"), capture.get("pixels"), capture.get("sha256")
            require(isinstance(stem, str) and STEM.fullmatch(stem) is not None,
                    "Only explicitly named final confirmation captures may be registered")
            index = int(stem.split("-", 1)[0])
            require(index >= 10 and stem not in stems and index not in indices,
                    "Baseline or duplicate confirmation capture identity")
            require(isinstance(pixels, list) and len(pixels) == 2
                    and all(positive_int(size) and size <= 16384 for size in pixels)
                    and pixels[0] * pixels[1] <= 64 * 1024 * 1024,
                    "Registry must record the actual reviewed image dimensions")
            require(isinstance(digest, str) and HASH.fullmatch(digest) is not None,
                    "Registry must bind the actual reviewed image bytes")
            stems.add(stem)
            indices.add(index)
            namespace.extend([f"{stem}-capture.json", f"{stem}.png"])
    require(len(stems) <= 256, "Reviewed capture roster exceeds its bounded size")
    validate_namespace(namespace)
    return processes


def audit(phase: str, registry_path: Path) -> dict:
    audit_started = datetime.now(timezone.utc)
    require(phase in {"pending", "accepted"}, "Unknown audit phase")
    require(VERIFY_ROOT.resolve() == ROOT.resolve(), "Verifier belongs to a different checkout")
    frozen = source_inputs()
    require(isinstance(frozen, dict) and frozen, "Current source inputs are missing")
    source_digest = canonical_digest(frozen)
    evidence: dict[str, str] = {}

    def observed(path: Path, limit: int = MAX_NATIVE_FILE_BYTES) -> bytes:
        name = path.absolute().relative_to(ROOT).as_posix()
        checked = contained_file(ROOT, name)
        require(checked.stat().st_size <= limit, f"Native evidence exceeds its byte budget: {name}")
        data, _ = source_auditor.read_regular(checked)
        require(len(data) <= limit, f"Native evidence exceeds its byte budget: {name}")
        digest = sha(data)
        require(name not in evidence or evidence[name] == digest,
                f"Evidence changed between reads: {name}")
        evidence[name] = digest
        return data

    def decoded(path: Path):
        return decode(observed(path, MAX_JSON_BYTES))

    registry_path = registry_path.absolute()
    require(registry_path.is_relative_to(NATIVE), "Reviewed registry must be in the current native evidence directory")
    registry = decoded(registry_path)
    specifications = checked_registry(registry)
    reviewed = timestamp(registry["reviewed_utc"])
    require(reviewed <= audit_started, "Visual review timestamp is in the future")
    observed(Path(__file__).resolve(), MAX_JSON_BYTES)
    observed(SOURCE_AUDITOR, MAX_JSON_BYTES)
    capture_results, process_results = [], []
    binary_paths: dict[str, str] = {}
    last_process_finish = None
    all_intervals = []
    for specification in specifications:
        name, pid = specification["name"], specification["pid"]
        start = decoded(NATIVE / f"{name}-start.json")
        process = decoded(NATIVE / f"{name}-process.json")
        require(isinstance(start, dict) and isinstance(process, dict), "Invalid native process record")
        required_start = {"schema", "release", "phase", "command", "working_directory", "started_utc",
                          "binary_sha256", "source_input_count", "source_inputs_sha256", "pid",
                          "trace_file", "expected_interruption"}
        require(required_start.issubset(start) and type(start.get("schema")) is int
                and start["schema"] == 1, f"Incomplete or mistyped direct launch record: {name}")
        require(all(process.get(key) == value for key, value in start.items()),
                f"Start and terminal process identity differ: {name}")
        require(type(process.get("schema")) is int and process["schema"] == 1
                and process.get("release") == RELEASE and process.get("phase") == "confirmation"
                and type(process.get("pid")) is int and process["pid"] == pid
                and type(process.get("exit_code")) is int and process["exit_code"] == 0
                and process.get("expected_interruption") is False
                and process.get("trace_file") is None and "trace_sha256" not in process
                and process.get("working_directory") == str(ROOT),
                f"Requires a directly launched clean final confirmation: {name}")
        require(process.get("source_inputs_unchanged") is True
                and type(process.get("source_input_count")) is int
                and process["source_input_count"] == len(frozen)
                and process.get("source_inputs_sha256") == source_digest
                and process.get("finished_source_inputs_sha256") == source_digest,
                f"Start/finish source identity differs from current inputs: {name}")
        command = process.get("command")
        require(isinstance(command, list) and command
                and all(isinstance(argument, str) and "\0" not in argument for argument in command)
                and Path(command[0]).is_absolute(), f"Invalid direct native command: {name}")
        executable = Path(command[0])
        binary_hash = source_auditor.digest_regular(executable)
        require(binary_hash == process.get("binary_sha256"), f"Recorded/current native binary differs: {name}")
        require(str(executable) not in binary_paths or binary_paths[str(executable)] == binary_hash,
                f"Native executable changed between processes: {name}")
        binary_paths[str(executable)] = binary_hash
        log = observed(NATIVE / f"{name}.log")
        require(sha(log) == process.get("log_sha256"), f"Native process log differs: {name}")
        begun, finished = timestamp(process["started_utc"]), timestamp(process["finished_utc"])
        seconds = process.get("seconds")
        require(begun < finished and type(seconds) in {int, float}
                and math.isfinite(seconds) and seconds > 0,
                f"Invalid direct process lifetime: {name}")
        require(finished <= audit_started, f"Native process completion is in the future: {name}")
        wall_seconds = (finished - begun).total_seconds()
        require(abs(wall_seconds - seconds) <= MAX_LIFETIME_CLOCK_DRIFT_SECONDS,
                f"Wall-clock and monotonic process durations differ by more than two seconds: {name}")
        last_process_finish = max(last_process_finish, finished) if last_process_finish else finished
        process_captures = []
        for approved in specification["captures"]:
            stem = approved["stem"]
            capture = decoded(NATIVE / f"{stem}-capture.json")
            require(isinstance(capture, dict) and type(capture.get("schema")) is int
                    and capture["schema"] == 1 and capture.get("release") == RELEASE
                    and capture.get("capture_name") == stem
                    and capture.get("window_pid_status") == "available"
                    and "window_pid_error" in capture and capture["window_pid_error"] is None
                    and all(type(capture.get(field)) is int and capture[field] == pid
                            for field in ("window_pid", "window_pid_before", "window_pid_after")),
                    f"Capture does not bind both observations to its direct process PID: {stem}")
            require(isinstance(capture.get("window_id"), str) and capture["window_id"].isdigit()
                    and int(capture["window_id"]) > 0,
                    f"Capture lacks a recorded native window identity: {stem}")
            capture_begin, capture_end = timestamp(capture["capture_started_utc"]), timestamp(capture["captured_utc"])
            require(begun <= capture_begin <= capture_end <= finished and capture_end <= reviewed,
                    f"Capture falls outside its process lifetime or post-capture review: {stem}")
            all_intervals.append((capture_begin, capture_end, stem))
            image_name = f"{stem}.png"
            image_bytes = observed(NATIVE / image_name)
            with Image.open(BytesIO(image_bytes)) as image:
                dimensions = list(image.size)
                require(image.format == "PNG" and dimensions == approved["pixels"],
                        f"Image format or reviewed dimensions differ: {image_name}")
                image.verify()
            image_record = {"name": image_name, "sha256": sha(image_bytes), "bytes": len(image_bytes)}
            require(capture.get("window_pixels") == dimensions and capture.get("image") == image_record
                    and image_record["sha256"] == approved["sha256"],
                    f"Image bytes, captured window or explicit review binding differ: {image_name}")
            capture_results.append({"image": image_name, "receipt": f"{stem}-capture.json", "pid": pid,
                                    "process": name, "pixels": dimensions, "image_sha256": sha(image_bytes),
                                    "capture_started_utc": capture["capture_started_utc"],
                                    "captured_utc": capture["captured_utc"]})
            process_captures.append(stem)
        process_results.append({"name": name, "pid": pid, "command": command,
                                "started_utc": process["started_utc"], "finished_utc": process["finished_utc"],
                                "seconds": seconds, "wall_seconds": wall_seconds,
                                "exit_code": 0, "log_sha256": sha(log), "binary_sha256": binary_hash,
                                "source_inputs_sha256": source_digest, "captures": process_captures})
    require(len(set(binary_paths.values())) == 1, "Final processes used different native executable bytes")
    binary_hash = next(iter(binary_paths.values()))
    ordered_intervals = sorted(all_intervals)
    require(all(earlier[1] <= later[0] for earlier, later in zip(ordered_intervals, ordered_intervals[1:])),
            "Final capture intervals overlap")

    acceptance = {"status": "pending_terminal_receipt", "terminal_receipt_validated": False}
    accepted = None
    acceptance_args = (ROOT, contained_file, validate_namespace, source_inputs,
                       validate_gate_results, GATE_NAMES)
    if phase == "accepted":
        accepted = source_auditor.accepted_state(*acceptance_args)
        require(accepted["frozen"] == frozen and accepted["native_binary_sha256"] == binary_hash,
                "Final native process/source identity differs from actual canonical acceptance")
        receipt_bytes = observed(ACCEPTANCE / "completed-gates.json")
        receipt = decode(receipt_bytes)
        require(sha(receipt_bytes) == accepted["receipt_sha256"], "Canonical receipt changed after validation")
        begun, finished = timestamp(receipt["started_utc"]), timestamp(receipt["finished_utc"])
        require(last_process_finish <= begun <= finished <= audit_started,
                "Final canonical acceptance must follow the completed native confirmations")
        pinned = tomllib.loads(observed(ROOT / "rust-toolchain.toml", MAX_JSON_BYTES).decode())["toolchain"]["channel"]
        require(isinstance(receipt.get("toolchain"), str)
                and re.fullmatch(r"rustc " + re.escape(pinned) + r" \(.+\)", receipt["toolchain"]) is not None,
                "Accepted compiler differs from the pinned toolchain")
        for name, expected in accepted["accepted_hashes"].items():
            if name in evidence:
                require(evidence[name] == expected, f"Native/canonical evidence binding differs: {name}")
            evidence[name] = expected
        binary_paths[accepted["native_executable"]] = binary_hash
        acceptance = {"status": "passed", "terminal_receipt_validated": True,
                      "receipt": (ACCEPTANCE / "completed-gates.json").relative_to(ROOT).as_posix(),
                      "receipt_sha256": accepted["receipt_sha256"], "gate_count": accepted["gate_logs"],
                      "evidence_artifact_count": accepted["evidence_artifacts"],
                      "distinct_passing_rust_tests": accepted["distinct_tests"],
                      "passing_rust_test_executions": accepted["passing_executions"],
                      "source_manifest_sha256": accepted["accepted_hashes"][(ACCEPTANCE / "source-hashes.json").relative_to(ROOT).as_posix()],
                      "native_binary_sha256": binary_hash}

    require(source_inputs() == frozen, "Current implementation inputs changed during native audit")
    for path, expected in binary_paths.items():
        require(source_auditor.digest_regular(Path(path)) == expected, "Native executable changed during audit")
    for name, expected in evidence.items():
        require(source_auditor.digest_regular(contained_file(ROOT, name)) == expected,
                f"Native or canonical evidence changed during audit: {name}")
    if accepted is not None:
        require(source_auditor.accepted_state(*acceptance_args) == accepted,
                "Canonical acceptance/evidence/source/executable changed during native audit")
    return {"schema": 1, "release": RELEASE, "phase": phase,
            "status": "passed" if phase == "accepted" else "pending_canonical_acceptance",
            "authorizes_release": phase == "accepted", "audit_started_utc": audit_started.isoformat(),
            "checked_utc": datetime.now(timezone.utc).isoformat(),
            "native_provenance_checks": "passed", "registry": registry_path.relative_to(ROOT).as_posix(),
            "registry_sha256": evidence[registry_path.relative_to(ROOT).as_posix()],
            "declared_visual_reviewer": registry["reviewer"], "declared_visual_reviewed_utc": registry["reviewed_utc"],
            "source_input_count": len(frozen), "source_inputs_sha256": source_digest,
            "native_binary_sha256": binary_hash, "capture_count": len(capture_results),
            "processes": process_results, "captures": sorted(capture_results, key=lambda item: item["capture_started_utc"]),
            "acceptance": acceptance, "evidence_sha256": dict(sorted(evidence.items())),
            "duration_tolerance_seconds": MAX_LIFETIME_CLOCK_DRIFT_SECONDS,
            "clock_assumption": "UTC and the monotonic clock are assumed not to drift or step apart by more than two seconds during each recorded native process. Larger disagreement rejects the record rather than interpreting its lifetime as proven.",
            "scope": "Recorded direct-process lifetime and before/after window PID association; exact reviewed image/log/binary bytes and agreeing start/finish/current source identity. Accepted mode additionally binds actual complete canonical evidence. This is not publisher authentication, an atomic filesystem snapshot, new visual inspection or physical-device qualification.",
            "historical_exclusion": "All baseline and candidate captures/processes are excluded from final acceptance. Their original records, including rejected images and known historical labeling errors, are left unchanged."}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phase", choices=("pending", "accepted"), required=True)
    parser.add_argument("--registry", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        require(not args.output.exists() and not args.output.is_symlink(), "Audit output must be new")
    except (OSError, ValueError) as error:
        print(json.dumps({"status": "failed", "authorizes_release": False, "error": str(error)}), file=sys.stderr)
        return 1
    try:
        result = audit(args.phase, args.registry)
    except (OSError, ValueError, KeyError, TypeError, RuntimeError) as error:
        result = {"schema": 1, "release": RELEASE, "phase": args.phase, "status": "failed",
                  "authorizes_release": False, "checked_utc": datetime.now(timezone.utc).isoformat(),
                  "registry": str(args.registry), "error": str(error)}
    try:
        publish_report(args.output, result)
    except (OSError, ValueError) as error:
        print(json.dumps({"status": "failed", "authorizes_release": False,
                          "error": f"Could not exclusively publish audit: {error}"}), file=sys.stderr)
        return 1
    print(json.dumps({key: result[key] for key in ("status", "authorizes_release", "capture_count", "acceptance", "error") if key in result}, indent=2))
    return 0 if result["status"] == "passed" else 2 if result["status"] == "pending_canonical_acceptance" else 1


if __name__ == "__main__":
    raise SystemExit(main())
