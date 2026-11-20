#!/usr/bin/env python3
"""Inspect an extracted Terra desktop package without Cargo or a source checkout.

Python 3.11+, standard library only. Passive verification never runs the game.
--smoke-help explicitly runs the verified executable's --help on a matching host.
Hashes establish package consistency, not publisher authenticity or GPU support.
Keep the extracted folder unchanged while this check runs. JSON is printed to
stdout; redirect it outside the package to retain a report.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import platform
import re
import signal
import stat
import struct
import subprocess
import threading
import time


MAX_MANIFEST_BYTES = 8 * 1024 * 1024
MAX_FILES = 4096
MAX_FILE_BYTES = 4 * 1024**3
MAX_TOTAL_BYTES = 16 * 1024**3
MAX_HELP_BYTES = 64 * 1024
MAX_EXECUTION_OUTPUT_BYTES = 512 * 1024 * 1024
MANIFEST = "package-manifest.json"
SCOPE = (
    "Exact extracted-file consistency and accepted-binary association. "
    "Not publisher authentication, target-device qualification or a power-loss guarantee."
)


def portable_parts(name: str) -> tuple[str, ...]:
    relative = PurePosixPath(name)
    if (not name or not relative.parts or "\\" in name or ":" in name or relative.is_absolute()
            or name != relative.as_posix() or any(p in {".", ".."} for p in relative.parts)):
        raise ValueError(f"Unsafe payload path: {name!r}")
    devices = {"con", "prn", "aux", "nul", "conin$", "conout$"}
    devices.update(f"{prefix}{digit}" for prefix in ("com", "lpt") for digit in "123456789¹²³")
    for part in relative.parts:
        if (part.endswith((".", " ")) or any(ord(c) < 32 or ord(c) == 127 or c in '<>"|?*' for c in part)
                or part.split(".", 1)[0].casefold() in devices):
            raise ValueError(f"Non-portable desktop payload path: {name!r}")
    return relative.parts


def validate_namespace(names) -> None:
    entries: dict[tuple[str, ...], str] = {}
    spellings: dict[tuple[str, ...], tuple[str, ...]] = {}
    for name in names:
        parts = portable_parts(name)
        key = tuple(part.casefold() for part in parts)
        if key in entries:
            raise ValueError(f"Desktop payload paths alias: {entries[key]} and {name}")
        for length in range(1, len(parts) + 1):
            previous = spellings.setdefault(key[:length], parts[:length])
            if previous != parts[:length]:
                raise ValueError(f"Desktop payload directory spellings alias: {'/'.join(previous)} and {name}")
        entries[key] = name
    for key, name in entries.items():
        if any(key[:length] in entries for length in range(1, len(key))):
            raise ValueError(f"Desktop payload file is also a directory: {name}")


def executable_platform(data: bytes) -> str:
    if data[:4] == b"\x7fELF" and len(data) > 20:
        if data[4:6] != b"\x02\x01" or struct.unpack_from("<H", data, 18)[0] != 62:
            raise ValueError("Only little-endian x86-64 Linux executables are packaged")
        return "linux-x86_64"
    if data[:2] == b"MZ" and len(data) >= 64:
        offset = struct.unpack_from("<I", data, 60)[0]
        if offset + 6 > len(data) or data[offset:offset + 4] != b"PE\0\0":
            raise ValueError("Invalid Windows executable header")
        if struct.unpack_from("<H", data, offset + 4)[0] != 0x8664:
            raise ValueError("Only x86-64 Windows executables are packaged")
        return "windows-x86_64"
    raise ValueError("Native artifact is not a supported Linux/Windows executable")


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"Duplicate JSON key: {key!r}")
        result[key] = value
    return result


def execution_required(release: str) -> bool:
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", release) if isinstance(release, str) else None
    if match is None:
        raise ValueError("Acceptance release must be a numeric major.minor.patch version")
    # Older deliveries did not record this evidence. Their inspection remains
    # explicitly legacy; current/future releases cannot omit the new contract.
    return tuple(map(int, match.groups())) >= (0, 24, 0)


def validate_command_execution(record: dict, *, required: bool, platform: str | None,
                               output_limit_bytes: int | None, log_bytes: int | None = None) -> None:
    execution = record.get("execution")
    if execution is None and not required and "execution" not in record:
        return
    if not isinstance(execution, dict):
        raise ValueError("Complete command execution evidence is required")
    if required and (type(output_limit_bytes) is not int or not 1 <= output_limit_bytes <= MAX_EXECUTION_OUTPUT_BYTES):
        raise ValueError("Current command execution requires its bounded output policy")
    limit = execution.get("output_limit_bytes")
    raw = execution.get("raw_output_bytes_observed")
    retained = execution.get("retained_log_bytes")
    expected_platform = {"Linux": "posix", "Darwin": "posix", "Windows": "nt"}.get(platform)
    if expected_platform is None:
        raise ValueError("Command execution requires a recorded supported host platform")
    expected_descendants = "owned_process_group_absent" if expected_platform == "posix" else "unconfirmed"
    if (execution.get("status") != "passed" or "failure_reason" not in execution
            or execution.get("failure_reason") is not None
            or execution.get("process_platform") != expected_platform
            or execution.get("output_overflow") is not False
            or execution.get("output_complete") is not True
            or execution.get("leader_exit_observed") is not True
            or execution.get("cleanup") != {"status": "not_needed"}
            or not isinstance(execution.get("descendant_check"), dict)
            or execution["descendant_check"].get("status") != expected_descendants
            or type(limit) is not int or not 1 <= limit <= MAX_EXECUTION_OUTPUT_BYTES
            or (output_limit_bytes is not None and limit != output_limit_bytes)
            or type(raw) is not int or not 0 <= raw <= limit
            or type(retained) is not int or retained != raw
            or (log_bytes is not None and retained != log_bytes)):
        raise ValueError("Incomplete or failed command execution cannot authorize publication")


def validate_receipt_execution(receipt: dict, log_sizes: dict[str, int] | None = None) -> None:
    required = execution_required(receipt.get("version"))
    environment = receipt.get("environment", {})
    platform = environment.get("os") if isinstance(environment, dict) else None
    limit = environment.get("gate_output_limit_bytes") if isinstance(environment, dict) else None
    if required and (type(limit) is not int or not 1 <= limit <= MAX_EXECUTION_OUTPUT_BYTES):
        raise ValueError("Current acceptance requires its bounded command output policy")
    gates = receipt.get("gates", [])
    probes = receipt.get("startup_probes", [])
    if required and (not isinstance(probes, list) or len(probes) != 2
                     or not all(isinstance(probe, dict) for probe in probes)
                     or [probe.get("name") for probe in probes] != ["metadata", "toolchain"]):
        raise ValueError("Current acceptance requires both ordered startup probes")
    for record in [*gates, *probes]:
        if not isinstance(record, dict):
            raise ValueError("Command evidence must be an object")
        if required or "execution" in record:
            if (type(record.get("exit_code")) is not int or record["exit_code"] != 0
                    or record.get("timed_out") is not False
                    or type(record.get("timeout_seconds")) not in {int, float}
                    or not math.isfinite(record["timeout_seconds"]) or record["timeout_seconds"] <= 0):
                raise ValueError("Current command evidence is incomplete or unsuccessful")
            log = record.get("log")
            if not isinstance(log, str):
                raise ValueError("Current command evidence has no log")
            portable_parts(log)
            if log_sizes is not None and log not in log_sizes:
                raise ValueError("Current command log is absent from delivered evidence")
            validate_command_execution(record, required=required, platform=platform,
                                       output_limit_bytes=limit,
                                       log_bytes=log_sizes.get(log) if log_sizes is not None else None)
    if required:
        validate_namespace([record["log"] for record in [*gates, *probes]])
        for probe in probes:
            if (probe["log"] != f"startup-{probe['name']}.log"
                    or receipt.get("evidence_artifacts", {}).get(probe["log"]) != probe.get("log_sha256")):
                raise ValueError("Startup probe log is not bound to accepted evidence")


def read_json(path: Path) -> tuple[dict, bytes]:
    if path.stat().st_size > MAX_MANIFEST_BYTES:
        raise ValueError(f"JSON record exceeds {MAX_MANIFEST_BYTES} bytes: {path.name}")
    with path.open("rb") as stream:
        data = stream.read(MAX_MANIFEST_BYTES + 1)
    if len(data) > MAX_MANIFEST_BYTES:
        raise ValueError(f"JSON record exceeds size limit: {path.name}")
    value = json.loads(data, object_pairs_hook=unique_object)
    if not isinstance(value, dict):
        raise ValueError(f"JSON record must be an object: {path.name}")
    return value, data


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def is_link(metadata) -> bool:
    # Junctions/reparse points are not reliably represented as POSIX symlinks.
    return stat.S_ISLNK(metadata.st_mode) or bool(
        getattr(metadata, "st_file_attributes", 0)
        & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
    )


def file_path(root: Path, name: str) -> Path:
    current = root
    parts = portable_parts(name)
    for index, part in enumerate(parts):
        current = current / part
        metadata = current.lstat()
        if is_link(metadata):
            raise ValueError(f"Linked payload path is not supported: {name}")
        if index < len(parts) - 1 and not stat.S_ISDIR(metadata.st_mode):
            raise ValueError(f"Payload parent is not a directory: {name}")
    if not stat.S_ISREG(metadata.st_mode):
        raise ValueError(f"Payload is not a regular file: {name}")
    if getattr(metadata, "st_nlink", 1) > 1:
        raise ValueError(f"Hard-linked payload aliases another path: {name}")
    return current


def inventory(root: Path) -> set[str]:
    files = set()
    pending = [root]
    directories = 0
    while pending:
        directory = pending.pop()
        directories += 1
        if directories > MAX_FILES * 4:
            raise ValueError("Extracted package contains too many directories")
        with os.scandir(directory) as entries:
            for entry in entries:
                metadata = entry.stat(follow_symlinks=False)
                relative = Path(entry.path).relative_to(root).as_posix()
                portable_parts(relative)
                if is_link(metadata):
                    raise ValueError(f"Linked payload path is not supported: {relative}")
                if stat.S_ISDIR(metadata.st_mode):
                    pending.append(Path(entry.path))
                elif stat.S_ISREG(metadata.st_mode):
                    if getattr(metadata, "st_nlink", 1) > 1:
                        raise ValueError(f"Hard-linked payload aliases another path: {relative}")
                    files.add(relative)
                    if len(files) > MAX_FILES + 1:
                        raise ValueError("Extracted package contains too many files")
                else:
                    raise ValueError(f"Payload is not a regular file: {relative}")
    validate_namespace(files)
    return files


def inspect_package(directory: Path) -> tuple[dict, Path]:
    root = directory.absolute()
    if is_link(root.lstat()) or not root.is_dir():
        raise ValueError("Package directory must be a real directory, not a link")
    manifest_path = file_path(root, MANIFEST)
    manifest, manifest_bytes = read_json(manifest_path)
    files = manifest.get("files_sha256")
    if not isinstance(files, dict) or not 1 <= len(files) <= MAX_FILES:
        raise ValueError("Manifest must list a bounded nonempty file inventory")
    if not all(isinstance(name, str) and isinstance(value, str)
               and re.fullmatch(r"[0-9a-f]{64}", value) for name, value in files.items()):
        raise ValueError("Manifest file hashes must be lowercase SHA-256 values")
    validate_namespace([*files, MANIFEST])
    actual = inventory(root)
    expected = set(files) | {MANIFEST}
    if actual != expected:
        raise ValueError(f"Package inventory differs; missing={sorted(expected - actual)[:8]}, "
                         f"unlisted={sorted(actual - expected)[:8]}")
    total = 0
    for name, expected_hash in files.items():
        path = file_path(root, name)
        before = path.stat()
        total += before.st_size
        if before.st_size > MAX_FILE_BYTES or total > MAX_TOTAL_BYTES:
            raise ValueError("Package exceeds file or total byte limit")
        if digest(path) != expected_hash:
            raise ValueError(f"Payload hash differs: {name}")
        after = path.stat()
        if (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) != (
                after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns):
            raise ValueError(f"Payload changed during verification: {name}")
    target = manifest.get("platform")
    if target not in {"linux-x86_64", "windows-x86_64"}:
        raise ValueError("Unsupported package platform")
    binary_name = "terra-demo.exe" if target == "windows-x86_64" else "terra-demo"
    executable = file_path(root, binary_name)
    with executable.open("rb") as stream:
        actual_target = executable_platform(stream.read(1024 * 1024))
    if actual_target != target or files.get(binary_name) != manifest.get("native_binary_sha256"):
        raise ValueError("Manifest platform or native binary identity differs")
    receipt, receipt_bytes = read_json(file_path(root, "evidence/completed-gates.json"))
    _, alias_bytes = read_json(file_path(root, "evidence/gates.json"))
    if (alias_bytes != receipt_bytes
            or hashlib.sha256(receipt_bytes).hexdigest() != manifest.get("acceptance_record_sha256")):
        raise ValueError("Completion receipt differs from package acceptance identity")
    if (receipt.get("status") != "passed" or receipt.get("completion_record") != "completed-gates.json"
            or receipt.get("version") != manifest.get("release")
            or receipt.get("native_binary_sha256") != manifest.get("native_binary_sha256")
            or receipt.get("source_inputs_unchanged") is not True
            or receipt.get("published_reports_unchanged") is not True
            or receipt.get("published_logs_unchanged") is not True):
        raise ValueError("Completion receipt does not describe the accepted package")
    gates = receipt.get("gates")
    if not isinstance(gates, list) or not gates or not all(
        isinstance(gate, dict) and type(gate.get("exit_code")) is int
        and gate["exit_code"] == 0 and gate.get("timed_out", False) is False
        and type(gate.get("failed_tests")) is int and gate["failed_tests"] == 0
        and type(gate.get("ignored_tests")) is int and gate["ignored_tests"] == 0
        for gate in gates
    ):
        raise ValueError("Completion receipt contains incomplete or failed gates")
    validate_receipt_execution(receipt, {
        name.removeprefix("evidence/"): file_path(root, name).stat().st_size
        for name in files if name.startswith("evidence/")
    })
    # Link every claimed gate/report byte to the independently hashed inventory.
    evidence_names = ["evidence/completed-gates.json", "evidence/gates.json"]
    for gate in gates:
        name = gate.get("log")
        if not isinstance(name, str):
            raise ValueError("Gate has no log path")
        evidence_names.append("evidence/" + name)
        if evidence_names[-1] not in files or files[evidence_names[-1]] != gate.get("log_sha256"):
            raise ValueError(f"Gate log is not bound to the manifest: {name}")
    artifacts = receipt.get("evidence_artifacts")
    if not isinstance(artifacts, dict) or "source-hashes.json" not in artifacts:
        raise ValueError("Completion receipt has no source identity evidence")
    for name, expected_hash in artifacts.items():
        if not isinstance(name, str) or not isinstance(expected_hash, str):
            raise ValueError("Evidence paths and hashes must be strings")
        evidence_names.append("evidence/" + name)
        if evidence_names[-1] not in files or files[evidence_names[-1]] != expected_hash:
            raise ValueError(f"Evidence is not bound to the manifest: {name}")
    validate_namespace(evidence_names)
    if manifest_path.read_bytes() != manifest_bytes or inventory(root) != expected:
        raise ValueError("Package inventory or manifest changed during verification")
    result = {
        "status": "passed", "release": manifest["release"], "platform": target,
        "build_profile": manifest.get("build_profile"), "files_verified": len(files),
        "payload_bytes": total, "manifest_sha256": hashlib.sha256(manifest_bytes).hexdigest(),
        "native_binary_sha256": manifest["native_binary_sha256"],
        "acceptance_record_sha256": manifest["acceptance_record_sha256"],
        "smoke_help": {"status": "not_requested"}, "scope": SCOPE,
    }
    return result, executable


def stop_smoke_process(process: subprocess.Popen) -> str:
    """Bounded cleanup of the process group/tree this smoke invocation owns.

    POSIX descendants that deliberately create another session are outside the
    owned group. Windows taskkill cannot confirm descendants after their leader
    has already exited; keep that limitation explicit instead of claiming proof.
    """
    cleanup = "leader termination requested; descendant cleanup unconfirmed"
    if os.name == "posix":
        try:
            os.killpg(process.pid, signal.SIGKILL)
            cleanup = "owned process group termination requested"
        except ProcessLookupError:
            cleanup = "owned process group no longer exists"
        except OSError:
            cleanup = "owned process group termination unconfirmed"
    elif os.name == "nt":
        try:
            subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                           timeout=10, check=True)
            cleanup = "owned process tree termination requested"
        except (OSError, subprocess.SubprocessError):
            pass
    if process.poll() is None:
        try:
            process.kill()
        except ProcessLookupError:
            pass
        except OSError:
            cleanup += "; leader termination unconfirmed"
    try:
        process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        cleanup += "; leader exit unconfirmed"
    return cleanup


def smoke_help(executable: Path, target: str, expected_hash: str, timeout: float) -> dict:
    if not math.isfinite(timeout) or timeout <= 0:
        raise ValueError("Smoke timeout must be positive and finite")
    host = platform.system()
    if host != ("Windows" if target == "windows-x86_64" else "Linux"):
        raise ValueError("Smoke execution requires the package's operating system")
    if platform.machine().lower() not in {"x86_64", "amd64"}:
        raise ValueError("Smoke execution requires an x86-64 host")
    if digest(executable) != expected_hash:
        raise ValueError("Native executable changed before smoke execution")
    if host == "Linux" and not os.access(executable, os.X_OK):
        raise ValueError("Executable permission is missing; restore its executable flag first")
    started = time.monotonic()
    process = subprocess.Popen(
        [str(executable), "--help"], cwd=executable.parent, stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, bufsize=0,
        start_new_session=os.name == "posix",
        creationflags=getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0) if os.name == "nt" else 0,
    )
    data = bytearray()
    finished = threading.Event()
    overflow = threading.Event()
    read_errors: list[Exception] = []

    def capture() -> None:
        try:
            while True:
                # Retain at most the accepted limit plus its first overflow byte.
                # A pipe supplies backpressure; there is no disk spool or queue.
                chunk = process.stdout.read(min(8192, MAX_HELP_BYTES + 1 - len(data)))
                if not chunk:
                    break
                data.extend(chunk)
                if len(data) > MAX_HELP_BYTES:
                    overflow.set()
                    break
        except (OSError, ValueError) as error:
            read_errors.append(error)
        finally:
            process.stdout.close()
            finished.set()

    # A descendant can retain stdout even after the leader exits. The reader is
    # daemonized and its completion shares the same deadline as the child, so a
    # platform that cannot terminate that descendant cannot hang verification.
    reader = threading.Thread(target=capture, name="terra-help-output", daemon=True)
    try:
        reader.start()
        while process.poll() is None or not finished.is_set():
            if overflow.is_set():
                raise ValueError(f"Native --help exceeded its {MAX_HELP_BYTES}-byte output limit")
            remaining = timeout - (time.monotonic() - started)
            if remaining <= 0:
                raise ValueError(f"Smoke help timed out after {timeout:g}s, including output completion")
            time.sleep(min(0.01, remaining))
        if overflow.is_set():
            raise ValueError(f"Native --help exceeded its {MAX_HELP_BYTES}-byte output limit")
        if read_errors:
            raise ValueError(f"Native --help output could not be read: {read_errors[0]}")
        code = process.returncode
        if code != 0 or b"Terra World" not in data or b"Usage:" not in data:
            raise ValueError(f"Native --help did not return the expected bounded help (exit {code})")
        if os.name == "posix":
            try:
                os.killpg(process.pid, 0)
            except ProcessLookupError:
                descendant_check = {
                    "status": "owned_process_group_absent",
                    "scope": "The owned group no longer exists; deliberately detached descendants are outside this check.",
                }
            except OSError as error:
                raise ValueError(f"Native --help process-group completion could not be checked: {error}") from error
            else:
                # EOF alone does not prove completion: a background child may
                # redirect both streams. A surviving owned group is a failed
                # smoke workload, followed by bounded cleanup in the handler.
                raise ValueError("Native --help left an owned process group after leader exit and output closure")
        else:
            descendant_check = {
                "status": "unconfirmed",
                "scope": "This platform check establishes leader exit and output closure only; descendants with redirected streams are not enumerated or proven stopped.",
            }
    except BaseException as error:
        cleanup = stop_smoke_process(process)
        if reader.ident is not None:
            reader.join(timeout=1)
        if not finished.is_set():
            cleanup += "; output closure unconfirmed"
        if isinstance(error, ValueError):
            raise ValueError(f"{error}; {cleanup}") from None
        raise
    finally:
        # FileIO is unbuffered: closing it does not wait for a buffered reader's
        # lock. Any still-blocked daemon reader retains only the bounded buffer.
        process.stdout.close()
    return {"status": "passed", "exit_code": code,
            "seconds": round(time.monotonic() - started, 6),
            "output_sha256": hashlib.sha256(data).hexdigest(), "output_bytes": len(data),
            "output_limit_bytes": MAX_HELP_BYTES,
            "descendant_check": descendant_check,
            "scope": "Command-line startup only; no window, saved world or device was exercised."}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, default=Path(__file__).absolute().parents[1])
    parser.add_argument("--smoke-help", action="store_true")
    parser.add_argument("--timeout-seconds", type=float, default=20.0)
    args = parser.parse_args()
    try:
        if not math.isfinite(args.timeout_seconds) or args.timeout_seconds <= 0:
            raise ValueError("Smoke timeout must be positive and finite")
        result, executable = inspect_package(args.directory)
        if args.smoke_help:
            result["smoke_help"] = smoke_help(executable, result["platform"],
                                               result["native_binary_sha256"], args.timeout_seconds)
        result["checked_utc"] = datetime.now(timezone.utc).isoformat()
        print(json.dumps(result, indent=2))
        return 0
    except (OSError, ValueError, KeyError, TypeError, struct.error) as error:
        print(json.dumps({"status": "failed", "error": str(error), "scope": SCOPE}, indent=2))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
