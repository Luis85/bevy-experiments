#!/usr/bin/env python3
"""Check the current handoff against compiled compatibility constants.

Historical changelogs and versioned reviews are intentionally not rewritten.
Python 3.11+, no third-party dependencies. Run with --write to update only the
marked table in docs/COMPATIBILITY.md after an intentional version change.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]
START = "<!-- compatibility:generated:start -->"
END = "<!-- compatibility:generated:end -->"


def constant(path: str, name: str) -> int:
    matches = re.findall(rf"pub const {name}: \w+ = (\d+);", (ROOT / path).read_text())
    if len(matches) != 1:
        raise ValueError(f"Expected exactly one {name} in {path}")
    return int(matches[0])


def contract() -> dict:
    package = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]
    protocol = constant("crates/terra-net/src/protocol.rs", "PROTOCOL_VERSION")
    return {
        "release": package["version"],
        "wire_protocol": protocol,
        "tls_alpn": f"terra-world/{protocol}",
        "domain_save": constant("crates/terra-sandbox/src/lib.rs", "SAVE_VERSION"),
        "regional_schema": constant("crates/terra-persistence/src/lib.rs", "STORE_SCHEMA_VERSION"),
        "rust_minimum": package["rust-version"],
        "rust_acceptance": tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"],
    }


def table(value: dict) -> str:
    labels = (
        ("Release", "release"), ("Wire protocol", "wire_protocol"),
        ("TLS ALPN", "tls_alpn"), ("Domain save", "domain_save"),
        ("Regional database schema", "regional_schema"),
        ("Minimum Rust", "rust_minimum"), ("Acceptance toolchain", "rust_acceptance"),
    )
    return "\n".join([START, "| Contract | Current value |", "|---|---|"] +
                     [f"| {label} | `{value[key]}` |" for label, key in labels] + [END])


def check(write: bool = False) -> dict:
    value = contract()
    path = ROOT / "docs/COMPATIBILITY.md"
    content = path.read_text()
    expression = re.compile(re.escape(START) + r".*?" + re.escape(END), re.DOTALL)
    if len(expression.findall(content)) != 1:
        raise ValueError("Compatibility guide must have exactly one generated table")
    expected = expression.sub(lambda _: table(value), content)
    if write:
        path.write_text(expected)
    elif content != expected:
        raise ValueError("Compatibility table differs from code; review the change and run --write")
    protocol = value["wire_protocol"]
    series = value["release"].rsplit(".", 1)[0]
    statements = {
        "README.md": f"**Wire protocol {protocol}** and TLS **{value['tls_alpn']}**",
        "crates/terra-net/README.md": f"Protocol version {protocol} uses",
        "docs/MULTIPLAYER.md": f"Terra {series} uses **wire protocol {protocol}**",
        "docs/NETWORK_REPLICATION.md": f"# Network protocol {protocol}, replication and transport",
        "docs/INTEGRATION.md": f"**wire protocol {protocol}**",
        "docs/OPERATIONS.md": f"Current clients and hosts require protocol {protocol}",
    }
    checked = {}
    for relative, statement in statements.items():
        data = (ROOT / relative).read_bytes()
        if statement not in data.decode("utf-8"):
            raise ValueError(f"Current compatibility statement is stale: {relative}")
        checked[relative] = hashlib.sha256(data).hexdigest()
    packages = tomllib.loads((ROOT / "Cargo.lock").read_text())["package"]
    members = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["members"]
    for member in members:
        name = tomllib.loads((ROOT / member / "Cargo.toml").read_text())["package"]["name"]
        locked = [p for p in packages if p["name"] == name and "source" not in p]
        if len(locked) != 1 or locked[0]["version"] != value["release"]:
            raise ValueError(f"Workspace lock version differs for {name}")
    checked[path.relative_to(ROOT).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    return {"status": "passed", "contract": value, "workspace_packages": len(members),
            "current_documents_sha256": checked,
            "scope": "Current handoff statements and workspace lock; historical versions preserved."}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    try:
        print(json.dumps(check(args.write), indent=2))
    except (OSError, ValueError, KeyError) as error:
        parser.exit(1, f"Compatibility check failed: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
