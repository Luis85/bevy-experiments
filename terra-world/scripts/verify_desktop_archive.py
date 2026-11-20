#!/usr/bin/env python3
"""Verify a delivered desktop ZIP through a fresh private extraction.

Python 3.11+, standard library only. Passive inspection never executes payloads.
--smoke-help explicitly runs the accepted executable after passive verification.
The archive and extracted tree must remain unchanged. This checks consistency,
not publisher authenticity, native-window behavior or target-device support.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import math
import os
from pathlib import Path
import stat
import struct
import tempfile
import zipfile
import zlib

import verify_desktop as desktop


MAX_ARCHIVE_BYTES = 4 * 1024**3 - 1
MAX_DIRECTORY_BYTES = 16 * 1024 * 1024
COPY_BLOCK_BYTES = 1024 * 1024
SCOPE = "Fresh bounded extraction, exact package consistency and optional command-line startup; no publisher, window, GPU or device qualification."


def bounded_directory(archive: Path) -> int:
    """Bound central-directory allocation before ZipFile builds member objects.

    Our desktop packager uses one ordinary directory with at most 4097 files.
    ZIP64 directory counts/offsets and split archives exceed this contract.
    Individual ZIP64 size fields are still checked against the payload limits.
    """
    with archive.open("rb") as stream:
        stream.seek(0, os.SEEK_END)
        length = stream.tell()
        stream.seek(max(0, length - 65557))
        tail = stream.read(65557)
    position = len(tail)
    while True:
        position = tail.rfind(b"PK\x05\x06", 0, position)
        if position < 0:
            raise ValueError("Desktop ZIP has no complete terminal directory record")
        if position + 22 <= len(tail):
            record = struct.unpack_from("<4s4H2LH", tail, position)
            if position + 22 + record[-1] == len(tail):
                break
    _, disk, directory_disk, disk_count, count, size, offset, _ = record
    if disk or directory_disk or disk_count != count:
        raise ValueError("Split desktop ZIP archives are not supported")
    if not 1 <= count <= desktop.MAX_FILES + 1 or size > MAX_DIRECTORY_BYTES:
        raise ValueError("Desktop ZIP central directory exceeds its bounded file/metadata budget")
    directory_end = length - len(tail) + position
    if offset == 0xFFFFFFFF or size == 0xFFFFFFFF or offset + size != directory_end:
        raise ValueError("Desktop ZIP directory offsets are unsupported or inconsistent")
    return count


def checked_members(bundle: zipfile.ZipFile, expected_count: int) -> tuple[str, list[tuple[zipfile.ZipInfo, str, int]], int]:
    entries = bundle.infolist()
    if len(entries) != expected_count:
        raise ValueError("Desktop ZIP directory count differs from its actual inventory")
    desktop.validate_namespace([entry.orig_filename for entry in entries])
    prefix = None
    members = []
    total = 0
    for entry in entries:
        if entry.orig_filename != entry.filename:
            raise ValueError("Desktop ZIP contains a truncated member name")
        parts = desktop.portable_parts(entry.filename)
        if len(parts) < 2:
            raise ValueError("Desktop ZIP needs one containing package folder")
        if prefix is None:
            prefix = parts[0]
        if parts[0] != prefix:
            raise ValueError("Desktop ZIP contains multiple package roots")
        name = "/".join(parts[1:])
        mode = entry.external_attr >> 16
        if entry.create_system != 3 or stat.S_IFMT(mode) != stat.S_IFREG or entry.is_dir():
            raise ValueError(f"Desktop ZIP member is not a declared regular file: {name}")
        expected_mode = 0o755 if name in {"terra-demo", "start-terra.sh"} else 0o644
        if stat.S_IMODE(mode) != expected_mode:
            raise ValueError(f"Desktop ZIP member has unexpected permissions: {name}")
        if entry.flag_bits & 1 or entry.compress_type not in {zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED}:
            raise ValueError(f"Desktop ZIP member uses unsupported encryption/compression: {name}")
        total += entry.file_size
        if not 0 <= entry.file_size <= desktop.MAX_FILE_BYTES or total > desktop.MAX_TOTAL_BYTES:
            raise ValueError("Desktop ZIP exceeds its file or aggregate uncompressed byte budget")
        members.append((entry, name, expected_mode))
    return prefix, members, total


def extract_checked(bundle: zipfile.ZipFile, root: Path, members: list[tuple[zipfile.ZipInfo, str, int]]) -> None:
    for entry, name, mode in members:
        destination = root.joinpath(*desktop.portable_parts(name))
        destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        written = 0
        # Exclusive output below an owned private root. Namespace/type checks
        # precede all extraction; no archive pathname is handed to extractall.
        with bundle.open(entry, "r") as source, destination.open("xb") as output:
            while True:
                data = source.read(min(COPY_BLOCK_BYTES, entry.file_size - written + 1))
                if not data:
                    break
                written += len(data)
                if written > entry.file_size:
                    raise ValueError(f"Desktop ZIP member expands beyond its declared size: {name}")
                output.write(data)
        if written != entry.file_size:
            raise ValueError(f"Desktop ZIP member has incomplete bytes: {name}")
        if os.name == "posix":
            destination.chmod(mode)


def check_modes(root: Path, members: list[tuple[zipfile.ZipInfo, str, int]]) -> str:
    if os.name != "posix":
        return "archive_modes_validated; Unix mode application is not applicable on this host"
    for _, name, expected_mode in members:
        if stat.S_IMODE((root / name).stat().st_mode) != expected_mode:
            raise ValueError(f"Extracted desktop permissions changed: {name}")
    return "every extracted regular-file mode matches the archive"


def verify_archive(archive: Path, smoke: bool = False, timeout: float = 20.0) -> dict:
    if not math.isfinite(timeout) or timeout <= 0:
        raise ValueError("Smoke timeout must be positive and finite")
    archive = archive.absolute()
    before = archive.lstat()
    if desktop.is_link(before) or not stat.S_ISREG(before.st_mode):
        raise ValueError("Desktop archive must be a regular file, not a link")
    if not 1 <= before.st_size <= MAX_ARCHIVE_BYTES:
        raise ValueError("Desktop ZIP exceeds its compressed byte budget")
    count = bounded_directory(archive)
    archive_hash = desktop.digest(archive)
    with zipfile.ZipFile(archive) as bundle:
        prefix, members, total = checked_members(bundle, count)
        with tempfile.TemporaryDirectory(prefix="terra-desktop-check-") as temporary:
            private = Path(temporary)
            if os.name == "posix":
                private.chmod(0o700)
            root = private / prefix
            root.mkdir(mode=0o700)
            extract_checked(bundle, root, members)
            result, executable = desktop.inspect_package(root)
            if prefix != f"terra-world-{result['release']}-{result['platform']}":
                raise ValueError("Desktop ZIP folder differs from its verified release/platform")
            mode_status = check_modes(root, members)
            if smoke:
                result["smoke_help"] = desktop.smoke_help(executable, result["platform"],
                                                         result["native_binary_sha256"], timeout)
            # Startup must not silently modify/add payload files. Recheck even
            # without execution, independently of the ZIP decompressor's CRC.
            again, _ = desktop.inspect_package(root)
            if any(again[key] != result[key] for key in again if key != "smoke_help"):
                raise ValueError("Extracted desktop identity changed during verification")
            check_modes(root, members)
    after = archive.lstat()
    identity = lambda value: (value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns)
    if identity(before) != identity(after) or desktop.digest(archive) != archive_hash:
        raise ValueError("Desktop archive changed during verification")
    return {
        "status": "passed", "checked_utc": datetime.now(timezone.utc).isoformat(),
        "archive_name": archive.name, "archive_bytes": before.st_size,
        "archive_sha256": archive_hash, "archive_unchanged": True,
        "archive_entries": count, "uncompressed_bytes": total, "package_folder": prefix,
        "fresh_extraction": True, "extracted_tree_unchanged": True,
        "extraction_access": "owner-only Unix parent (0700)" if os.name == "posix" else "platform temporary directory; Windows ACLs are not qualified",
        "mode_check": mode_status, "package": result, "scope": SCOPE,
    }


def publish_report(path: Path, report: dict) -> None:
    """Retain one new result, including failures; never overwrite earlier proof."""
    path.parent.mkdir(parents=True, exist_ok=True)
    data = (json.dumps(report, indent=2) + "\n").encode()
    staged = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".terra-archive-report-", delete=False) as output:
            staged = Path(output.name)
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
        os.link(staged, path)
        if path.read_bytes() != data:
            raise ValueError("Archive-check report readback differs from captured bytes")
    finally:
        if staged is not None:
            staged.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--smoke-help", action="store_true")
    parser.add_argument("--timeout-seconds", type=float, default=20.0)
    parser.add_argument("--report", type=Path, help="New JSON file outside the archive; failures are retained too")
    args = parser.parse_args()
    try:
        if args.report is not None and (args.report.exists() or args.report.is_symlink()):
            raise ValueError("Archive-check report must be a new path")
        report = verify_archive(args.archive, args.smoke_help, args.timeout_seconds)
    except (OSError, ValueError, KeyError, TypeError, struct.error, zipfile.BadZipFile, zlib.error, RuntimeError) as error:
        report = {"status": "failed", "checked_utc": datetime.now(timezone.utc).isoformat(),
                  "archive_name": args.archive.name, "error": str(error), "scope": SCOPE}
    if args.report is not None:
        try:
            publish_report(args.report, report)
        except (OSError, ValueError) as error:
            report["status"] = "failed"
            report["report_error"] = str(error)
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
