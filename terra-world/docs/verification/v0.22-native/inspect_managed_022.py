#!/usr/bin/env python3
"""Read a closed isolated native-review world; never repair or mutate its state."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sqlite3
import stat


def identity(path: Path) -> dict:
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"Expected a regular review file: {path}")
    before = path.stat()
    with path.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    after = path.stat()
    if (before.st_ino, before.st_size, before.st_mtime_ns) != (
        after.st_ino, after.st_size, after.st_mtime_ns
    ):
        raise ValueError(f"Review file changed while hashing: {path}")
    return {
        "path": str(path), "bytes": after.st_size, "sha256": digest,
        "unix_mode": oct(stat.S_IMODE(after.st_mode)),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--previous", type=Path)
    args = parser.parse_args()
    profile_path = args.profile.resolve(strict=True)
    profile = json.loads(profile_path.read_bytes())
    database = Path(profile["config"]["region_store"]).resolve(strict=True)
    paths = {
        "profile": profile_path,
        "witness": profile_path.with_name("world-started.json"),
        "lease": profile_path.with_name("world-session.lock"),
        "database": database,
    }
    before = {name: identity(path) for name, path in paths.items()}
    connection = sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)
    try:
        integrity = [row[0] for row in connection.execute("PRAGMA integrity_check")]
        generation, state_bytes, metadata_bytes = connection.execute(
            "SELECT generation, global_state, metadata FROM world WHERE singleton=1"
        ).fetchone()
        regions = connection.execute("SELECT count(*) FROM regions").fetchone()[0]
    finally:
        connection.close()
    state, metadata = json.loads(state_bytes), json.loads(metadata_bytes)
    player = str(metadata["player_id"])
    after = {name: identity(path) for name, path in paths.items()}
    if before != after:
        raise ValueError("Closed-world files changed during review")
    result = {
        "schema": 1, "release": "0.22.0",
        "observed_utc": datetime.now(timezone.utc).isoformat(),
        "status": "passed" if integrity == ["ok"] and generation > 0
            and profile["checkpoint_present"] is True else "failed",
        "title": profile["title"], "checkpoint_present": profile["checkpoint_present"],
        "integrity": integrity, "generation": generation, "stored_regions": regions,
        "factions": state["factions"], "next_domain_id": state["next_id"],
        "player_id": int(player), "account": state["living"]["accounts"][player],
        "player_inventory": state["players"][player]["inventory"],
        "clock": metadata["clock"], "files": after, "files_unchanged_during_read": True,
        "scope": "Read-only inspection of a closed isolated native test world; SQLite integrity and observed saved values, not a power-loss or gameplay-balance qualification.",
    }
    if args.previous:
        prior = json.loads(args.previous.read_bytes())
        result["comparison"] = {
            "previous_record_sha256": identity(args.previous)["sha256"],
            "witness_unchanged": prior["files"]["witness"]["sha256"] == after["witness"]["sha256"],
            "lease_marker_unchanged": prior["files"]["lease"]["sha256"] == after["lease"]["sha256"],
            "wallet_unchanged": prior["account"]["money"] == result["account"]["money"],
            "inventory_unchanged": prior["player_inventory"] == result["player_inventory"],
            "checkpoint_generation_increased": generation > prior["generation"],
            "next_domain_id_not_rewound": state["next_id"] >= prior["next_domain_id"],
        }
    with args.output.open("x") as output:
        json.dump(result, output, indent=2)
        output.write("\n")
    print(json.dumps(result, indent=2))
    if result["status"] != "passed":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
