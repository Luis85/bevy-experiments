#!/usr/bin/env python3
"""Associate native UI observations with the accepted 0.22 executable.

This is evidence generation, not a test runner or replacement acceptance record.
It requires the actual completed fresh run and an identical native executable.
The only allowed post-native source change is the retained two-byte correction
to explicit protocol expectations inside the existing test module.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import sys
from datetime import datetime, timezone


ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_digest(value: object) -> str:
    return digest(json.dumps(value, sort_keys=True, separators=(",", ":"),
                             ensure_ascii=True, allow_nan=False).encode("utf-8"))


def read_json(path: Path) -> tuple[dict, bytes]:
    data = path.read_bytes()
    return json.loads(data), data


def main() -> None:
    if not __debug__:
        raise RuntimeError("Run without -O or PYTHONOPTIMIZE; evidence checks require assertions")
    sys.dont_write_bytecode = True
    sys.path.insert(0, str(ROOT / "scripts"))
    from release_contract import validate_gate_results
    from verify import source_inputs

    accepted_dir = ROOT / "docs/verification/v0.22-release-acceptance"
    failed_dir = ROOT / "docs/verification/v0.22-final-acceptance"
    receipt, receipt_bytes = read_json(accepted_dir / "completed-gates.json")
    assert receipt["status"] == "passed" and receipt["version"] == "0.22.0"
    assert all(receipt[key] is True for key in (
        "source_inputs_unchanged", "published_reports_unchanged", "published_logs_unchanged"))
    assert receipt["completion_record"] == "completed-gates.json"
    assert (accepted_dir / "gates.json").read_bytes() == receipt_bytes
    assert len(receipt["gates"]) == 44
    assert validate_gate_results(receipt["gates"]) == receipt["passing_rust_test_executions"]

    failed, failed_bytes = read_json(failed_dir / "completed-gates.json")
    assert failed["status"] == "failed"
    old_map, old_map_bytes = read_json(failed_dir / "source-hashes.json")
    new_map, new_map_bytes = read_json(accepted_dir / "source-hashes.json")
    assert digest(old_map_bytes) == failed["evidence_artifacts"]["source-hashes.json"]
    assert digest(new_map_bytes) == receipt["evidence_artifacts"]["source-hashes.json"]
    assert new_map == source_inputs()
    assert len(old_map) == len(new_map) == receipt["source_inputs"] == 378
    assert set(old_map) == set(new_map)
    changed = sorted(name for name in old_map if old_map[name] != new_map[name])
    assert changed == ["crates/terra-net/src/protocol.rs"]

    native, native_bytes = read_json(HERE / "confirmation-managed-reopen-process.json")
    assert native["phase"] == "confirmation" and native["exit_code"] == 0
    assert native["source_inputs_unchanged"] is True
    assert (native["source_inputs_sha256"] == native["finished_source_inputs_sha256"]
            == canonical_digest(old_map))

    old_protocol = (HERE / "native-confirmed-protocol.rs").read_bytes()
    new_protocol = (ROOT / changed[0]).read_bytes()
    assert digest(old_protocol) == old_map[changed[0]]
    assert digest(new_protocol) == new_map[changed[0]]
    assert old_protocol.count(b'"protocol": 10') == 2
    assert old_protocol.replace(b'"protocol": 10', b'"protocol": 11') == new_protocol
    boundary = old_protocol.index(b"#[cfg(test)]")
    offsets = [i for i, (left, right) in enumerate(zip(old_protocol, new_protocol))
               if left != right]
    assert len(old_protocol) == len(new_protocol) and len(offsets) == 2
    assert all(i > boundary for i in offsets)
    assert old_protocol[:boundary] == new_protocol[:boundary]

    executable = Path(receipt["native_executable"])
    with executable.open("rb") as stream:
        binary_hash = hashlib.file_digest(stream, "sha256").hexdigest()
    assert binary_hash == receipt["native_binary_sha256"] == native["binary_sha256"]

    result = {
        "schema": 1,
        "release": "0.22.0",
        "status": "passed",
        "checked_utc": datetime.now(timezone.utc).isoformat(),
        "scope": "Same native executable, with one independently reviewed test-only input correction; not a reconstructed acceptance receipt or a new UI execution.",
        "accepted_receipt": str((accepted_dir / "completed-gates.json").relative_to(ROOT)),
        "accepted_receipt_sha256": digest(receipt_bytes),
        "retained_failed_receipt_sha256": digest(failed_bytes),
        "native_process_record_sha256": digest(native_bytes),
        "native_process_pid": native["pid"],
        "native_process_finished_utc": native["finished_utc"],
        "native_process_exit_code": native["exit_code"],
        "accepted_and_native_binary_sha256": binary_hash,
        "native_executable": str(executable),
        "binary_identical": True,
        "source_input_count": len(new_map),
        "native_source_map_sha256": canonical_digest(old_map),
        "accepted_source_map_sha256": canonical_digest(new_map),
        "changed_inputs": changed,
        "old_protocol_sha256": digest(old_protocol),
        "new_protocol_sha256": digest(new_protocol),
        "changed_byte_offsets": offsets,
        "production_prefix_bytes": boundary,
        "production_prefix_sha256": digest(old_protocol[:boundary]),
        "production_prefix_unchanged": True,
        "all_other_source_inputs_unchanged": True,
        "only_explicit_test_protocol_expectations_changed": True,
    }
    output = HERE / "confirmation-input-association.json"
    with output.open("x", encoding="utf-8") as stream:
        json.dump(result, stream, indent=2)
        stream.write("\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
