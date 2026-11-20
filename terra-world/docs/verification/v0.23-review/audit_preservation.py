#!/usr/bin/env python3
"""Independently compare the active 0.23 tree with the exact delivered 0.22 ZIP.

Only named, dispatched paths receive an edit allowance. This helper writes
current review evidence; it does not repair source files, edit older evidence,
or qualify a final archive. Run again after the current documents settle.
"""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import zipfile


BASELINE_SHA256 = "54ba5420db937056ba62e1f7861308a4ea540112f52ae500af10e92e897a739e"
BASELINE_BYTES = 274052010
BASELINE_MEMBERS = 2790
PREFIX = "terra-world/"
EVIDENCE = "docs/verification/v0.23-review"
EXCLUDED = {".git", "target", "__pycache__"}
REPORTS = {f"{EVIDENCE}/preservation.json", f"{EVIDENCE}/preservation.md"}
RELOCATIONS = {
    "DELIVERY_MANIFEST.json": f"{EVIDENCE}/baseline-delivery-manifest.json",
}
RETAINED = {
    "docs/PRODUCT_REVIEW.md": "docs/PRODUCT_REVIEW_v0.22.md",
    "docs/VALIDATION.md": "docs/VALIDATION_v0.22.md",
    **RELOCATIONS,
}

# This list was assembled from the integrator's explicit assignments and the
# three specialists' declared ownership. Observed changes never add entries.
ALLOWED = {
    "Cargo.toml": "Integrator WP08: advance workspace package version to 0.23.0; preserve pinned external dependencies.",
    "Cargo.lock": "Integrator WP08: align workspace lockfile package versions with 0.23.0, without dependency churn.",
    "README.md": "Integrator WP08-WP10: current 0.23 overview, entry points, acceptance limits and complete delivery links.",
    "PRODUCT.md": "Integrator WP08: current product scope and reviewed 0.23 behavior, preserving explicit capability limits.",
    "CHANGELOG.md": "Integrator WP08: add the 0.23 increment while retaining every prior release entry verbatim.",
    "docs/PRODUCT_REVIEW.md": "Integrator WP08-WP10: current all-perspective 0.23 review; exact prior report retained as PRODUCT_REVIEW_v0.22.md.",
    "docs/VALIDATION.md": "Integrator WP10: actual current 0.23 execution and qualification; exact prior report retained as VALIDATION_v0.22.md.",
    "docs/COMPATIBILITY.md": "Integrator WP08: 0.23 package/public API compatibility and unchanged save/wire boundaries.",
    "docs/MULTIPLAYER.md": "Integrator WP08 compatibility gate: identify Terra 0.23 as retaining wire 11 from 0.22; preserve protocol constants and behavior.",
    "docs/RELEASE_HISTORY.md": "Integrator WP08: archive prior overview context while preserving every existing release entry verbatim.",
    "docs/PLAYTESTING.md": "Integrator WP09: current 0.23 journey checks, evidence paths and explicit human/device validation limits if required.",
    "docs/GUIDANCE.md": "Integrator WP03-WP04 integration: document shared supply readiness, bounded whole-exchange acquisition and recovery guidance if required.",
    "scripts/desktop-payload.json": "Integrator WP08-WP10: add 0.23 plan/research/gaps and exact archived 0.22 review/validation to the desktop document payload; retain all prior entries.",
    "crates/terra-demo/src/actor_summary.rs": "Player WP01: present the exact owned active person used by quick consumption actions.",
    "crates/terra-demo/src/frontier.rs": "Player WP01-WP03: bind displayed journey/request context through activation and use shared supply-readiness explanations and refusal; WP09 screenshot-led plural and readable item-label polish.",
    "crates/terra-demo/src/flow.rs": "Player WP09 consolidated native polish: describe effective atmospheric-sky state in pause/settings when Performance quality keeps sky off, preserving the stored preference and rendering policy.",
    "crates/terra-demo/src/interaction.rs": "Player WP01-WP02: reject stale queued owner/body/restore/target context without retargeting commands.",
    "crates/terra-demo/src/interface.rs": "Player WP01-WP02: retain displayed Nature/territory target identity and show concrete action/recovery context.",
    "docs/CONTROLS.md": "Player WP01-WP02: explain named body actions, displayed targets and stale-action refresh behavior.",
    "docs/GAME_FLOW.md": "Player WP01-WP03: document reviewed-action identity, unavailable supply states and continued goal flows.",
    "crates/terra-demo/src/launcher/model.rs": "Reliability WP05: share the stable managed-world lease with metadata maintenance and protect rename publication.",
    "crates/terra-demo/src/launcher/ui.rs": "Player WP09 consolidated native polish: explain effective sky behavior in launcher settings/review after independent fresh screenshots showed stored preference and Performance quality appeared contradictory.",
    "crates/terra-demo/src/launcher/recovery.rs": "Reliability WP06: hold managed maintenance ownership through native backup publication while preserving typed recovery outcomes.",
    "crates/terra-demo/src/launcher/recovery_tests.rs": "Reliability WP05-WP06: exercise real process/session contention, clean/crash release and publication/retention boundaries.",
    "scripts/verify_desktop.py": "Reliability WP07: bound child help-output capture and inherited-pipe lifetime, with finite cleanup and existing payload integrity checks.",
    "scripts/test_verify_desktop.py": "Reliability WP07: meaningful overflow, descendant, deadline and bounded-cleanup regression fixtures.",
    "docs/DESKTOP_DELIVERY.md": "Reliability WP07: document bounded desktop smoke behavior, evidence and platform limitations.",
    "docs/RECOVERY_BUNDLES.md": "Reliability WP05-WP06: distinguish coordinated native maintenance from explicit backend online backup and retained recovery results.",
    "crates/terra-sandbox/README.md": "World WP03-WP04: document typed readiness/public framework limits and correct the stale statement about existing safe solo dissolution.",
    "crates/terra-sandbox/src/guidance.rs": "World WP03-WP04: truthful supply readiness, dormant-owner recovery, expiry filtering and achievable complete inventory exchanges.",
    "crates/terra-sandbox/src/settlement/commands.rs": "World WP03: share acceptance/return preparation and commit a completely prepared supply delivery without changing economic policy.",
    "crates/terra-sandbox/src/settlement/mod.rs": "World WP03: export the read-only supply readiness API and typed blockers from the settlement module.",
    "docs/ARCHITECTURE.md": "World WP03: describe the shared pure-domain preparation seam and authoritative execution boundary.",
    "docs/SETTLEMENTS_AND_ECONOMY.md": "World WP03-WP04: describe supply readiness, explicit value-preserving recovery and focused conservation fixtures.",
    "crates/terra-sandbox/src/settlement/supply.rs": "World WP03 new source: typed read-only readiness and private prepared delivery values shared with actual commands.",
    "crates/terra-sandbox/tests/supply_readiness.rs": "World WP03-WP04 new integration tests: blockers, conservation, partial views, competing capacity and finite failure/recovery/save/restore.",
    "crates/terra-sandbox/tests/guidance_capacity.rs": "World WP04 new integration tests: complete multi-output exchange, feasible smaller batches and credible recovery alternatives.",
    "docs/IMPLEMENTATION_PLAN_v0.23.md": "Integrator new current plan: dispatched work packages, acceptance criteria, scope and delivery sequence.",
    "docs/RESEARCH_v0.23.md": "Integrator new current research: primary-source patterns, actual application and explicit non-goals.",
    "docs/REVIEW_GAPS_v0.23.md": "Integrator new current gap register: scoped closures, retained G01-G18 limits and next required proof.",
    "docs/PRODUCT_REVIEW_v0.22.md": "Exact retained copy of the delivered 0.22 current product review; no content edits allowed to this new historical copy.",
    "docs/VALIDATION_v0.22.md": "Exact retained copy of the delivered 0.22 current validation; no content edits allowed to this new historical copy.",
}
NEW_PREFIXES = {
    "docs/verification/v0.23-": "New 0.23 review, candidate/native, acceptance and delivery evidence only; no pre-existing historical path replacement.",
}
AMENDMENTS = [{
    "path": "docs/MULTIPLAYER.md",
    "authorization": "Integrator explicitly added this original path after the initial preservation checkpoint.",
    "reason": "The compatibility gate requires the current-release phrase; 0.23 retains wire 11 from 0.22 with unchanged protocol constants.",
    "evidence": "docs/verification/v0.23-preflight/compatibility.json",
}, {
    "path": "crates/terra-demo/src/flow.rs",
    "authorization": "Integrator explicitly authorized the player's consolidated native polish after independent fresh candidate screenshots.",
    "reason": "Clarify effective atmospheric-sky state when Performance quality overrides the retained preference; no rendering-policy change.",
    "evidence": "docs/verification/v0.23-native/README.md",
}, {
    "path": "crates/terra-demo/src/launcher/ui.rs",
    "authorization": "Integrator explicitly authorized the player's consolidated native polish after independent fresh candidate screenshots.",
    "reason": "Clarify effective sky state in launcher settings/review without changing stored settings or rendering behavior.",
    "evidence": "docs/verification/v0.23-native/README.md",
}, {
    "path": "crates/terra-demo/src/frontier.rs",
    "authorization": "Integrator explicitly broadened this already allowed player-owned file for the same consolidated native polish.",
    "reason": "Correct singular/plural counts and show readable item names in the existing journey text.",
    "evidence": "docs/verification/v0.23-native/README.md",
}]


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def signature(info: os.stat_result) -> tuple:
    return (info.st_dev, info.st_ino, info.st_mode, info.st_size,
            info.st_mtime_ns, info.st_ctime_ns)


def stable_digest(path: Path) -> tuple[str, os.stat_result]:
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode):
        raise ValueError(f"Nonregular path: {path}")
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    with os.fdopen(descriptor, "rb") as stream:
        if signature(os.fstat(stream.fileno())) != signature(before):
            raise RuntimeError(f"Path changed before hashing: {path}")
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
        after = os.fstat(stream.fileno())
    if signature(after) != signature(before) or signature(path.lstat()) != signature(before):
        raise RuntimeError(f"Path changed during hashing: {path}")
    return digest, before


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--baseline", type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    baseline = args.baseline.resolve(strict=True)
    output = root / EVIDENCE
    output.mkdir(parents=True, exist_ok=True)
    started = datetime.now(timezone.utc).isoformat()
    archive_hash, archive_stat = stable_digest(baseline)
    if archive_hash != BASELINE_SHA256 or archive_stat.st_size != BASELINE_BYTES:
        raise ValueError("Baseline bytes/hash do not match the exact delivered 0.22 archive")

    original = {}
    old_history = {}
    with zipfile.ZipFile(baseline) as archive:
        entries = archive.infolist()
        duplicate_members = sorted(name for name, count in Counter(e.filename for e in entries).items() if count > 1)
        if duplicate_members or len(entries) != BASELINE_MEMBERS:
            raise ValueError("Unexpected baseline member count or duplicate names")
        for item in entries:
            name = item.filename.removeprefix(PREFIX)
            parts = PurePosixPath(name).parts
            mode = item.external_attr >> 16
            if (not item.filename.startswith(PREFIX) or not parts
                    or PurePosixPath(name).is_absolute() or ".." in parts
                    or "\\" in name or item.create_system != 3
                    or not stat.S_ISREG(mode) or item.is_dir()):
                raise ValueError(f"Invalid baseline member: {item.filename}")
            data = archive.read(item)
            if len(data) != item.file_size:
                raise ValueError(f"Baseline size mismatch: {name}")
            original[name] = {"sha256": sha(data), "bytes": len(data), "mode": oct(stat.S_IMODE(mode))}
            if name in ("CHANGELOG.md", "docs/RELEASE_HISTORY.md"):
                old_history[name] = data
    original = dict(sorted(original.items()))
    inventory_path = f"{EVIDENCE}/baseline.json"
    inventory = {
        "baseline": "0.22.0", "archive_sha256": archive_hash,
        "archive_bytes": archive_stat.st_size,
        "scope": "Fresh independent ZIP member inventory. No claim about initial extraction or final archive qualification.",
        "files": original,
    }
    (root / inventory_path).write_text(json.dumps(inventory, indent=2) + "\n")
    independently_read = json.loads((root / inventory_path).read_text())["files"]

    evidence = {}
    missing, nonregular, changed_during, unexpected, modes = [], [], [], [], []
    for name, expected in original.items():
        current = RELOCATIONS.get(name, name)
        record = {
            "archive_sha256": expected["sha256"], "archive_bytes": expected["bytes"],
            "archive_mode": expected["mode"], "current_path": current,
        }
        try:
            observed, info = stable_digest(root / current)
        except FileNotFoundError:
            missing.append(name)
            record["classification"] = "missing"
        except ValueError:
            nonregular.append(current)
            record["classification"] = "nonregular"
        except RuntimeError:
            changed_during.append(current)
            record["classification"] = "changed_during_hash"
        else:
            record.update(observed_sha256=observed, observed_bytes=info.st_size,
                          observed_mode=oct(stat.S_IMODE(info.st_mode)))
            if observed == expected["sha256"] and info.st_size == expected["bytes"]:
                record["classification"] = "exact_relocated" if name in RELOCATIONS else "exact"
            elif name in ALLOWED and name not in RELOCATIONS:
                record.update(classification="expected_edit", reason=ALLOWED[name])
            else:
                record["classification"] = "unexpected_edit"
                unexpected.append(name)
            if record["observed_mode"] != expected["mode"]:
                modes.append({"path": name, "current_path": current,
                              "archive_mode": expected["mode"], "observed_mode": record["observed_mode"]})
        evidence[name] = record

    retained = []
    for name, current in RETAINED.items():
        digest, info = stable_digest(root / current)
        retained.append({
            "archive_path": name, "retained_path": current,
            "sha256": digest, "matches_archive": digest == original[name]["sha256"],
            "mode_matches_archive": oct(stat.S_IMODE(info.st_mode)) == original[name]["mode"],
        })

    new_paths, unclassified = [], []
    def inaccessible(error: OSError) -> None:
        raise error
    for directory, children, names in os.walk(root, followlinks=False, onerror=inaccessible):
        parent = Path(directory)
        retained_children = []
        for child in sorted(children):
            if child in EXCLUDED:
                continue
            path = parent / child
            if not stat.S_ISDIR(path.lstat().st_mode):
                nonregular.append(path.relative_to(root).as_posix())
            else:
                retained_children.append(child)
        children[:] = retained_children
        for filename in sorted(names):
            path = parent / filename
            name = path.relative_to(root).as_posix()
            if path.suffix in {".pyc", ".pyo"} or name in original or name in REPORTS:
                continue
            if not stat.S_ISREG(path.lstat().st_mode):
                nonregular.append(name)
            reason = ALLOWED.get(name)
            if reason is None:
                reason = next((value for prefix, value in NEW_PREFIXES.items() if name.startswith(prefix)), None)
            if reason is None:
                unclassified.append(name)
            new_paths.append({"path": name, "reason": reason or "No declared allowance; needs independent disposition."})

    history = {}
    for name, old in old_history.items():
        current = (root / name).read_bytes()
        first_entry = re.search(rb"(?m)^## ", old)
        history[name] = {
            "entire_prior_text_retained_verbatim": old in current,
            "prior_release_entries_retained_verbatim": bool(first_entry and old[first_entry.start():] in current),
            "interpretation": "A new section may be inserted after the title. Every prior release entry must remain one exact contiguous suffix; title/preamble edits are reported separately.",
        }

    selectors = {
        "runtime_assets_including_recipes_and_generators": lambda p: "/assets/" in f"/{p}",
        "source_data": lambda p: "/data/" in f"/{p}",
        "configuration_examples": lambda p: p.startswith("configs/"),
        "sdk_examples": lambda p: "/examples/" in f"/{p}",
        "authoring_sources_and_artifacts": lambda p: p.endswith(("generate.py", "generate_features.py", "authoring.recipe.json")) or p == "docs/ASSET_AUTHORING.md",
        "historical_verification_evidence": lambda p: p.startswith("docs/verification/"),
        "historical_screenshots": lambda p: p.startswith(("docs/screenshots/", "docs/verification/")) and Path(p).suffix.lower() in {".png", ".jpg", ".jpeg", ".webp"},
        "versioned_root_documents_and_receipts": lambda p: len(PurePosixPath(p).parts) == 2 and p.startswith("docs/") and re.search(r"(?:_|-)v[0-9]", p) is not None,
        "licenses_and_notices": lambda p: p in {"LICENSE-APACHE", "LICENSE-MIT", "THIRD_PARTY_NOTICES.md"},
        "all_prior_members_outside_explicit_edit_allowlist": lambda p: p not in ALLOWED,
    }
    groups = {}
    for label, select in selectors.items():
        selected = [p for p in original if select(p)]
        nonexact = [p for p in selected if evidence[p]["classification"] not in {"exact", "exact_relocated"}]
        groups[label] = {"files": len(selected), "exact_content": len(selected) - len(nonexact), "nonexact_paths": nonexact}
    if stable_digest(baseline)[0] != archive_hash:
        raise ValueError("Baseline changed while auditing")
    classification_counts = Counter(record["classification"] for record in evidence.values())
    failed = bool(unexpected or missing or nonregular or changed_during or modes or unclassified
                  or any(not r["matches_archive"] or not r["mode_matches_archive"] for r in retained)
                  or any(not value["prior_release_entries_retained_verbatim"] for value in history.values()))
    result = {
        "schema_version": 1,
        "scope": "Independent active-tree preservation checkpoint against every regular member of the exact 0.22 source archive; explicit dispatched edit policy, no final archive qualification.",
        "reviewer": "World/framework specialist, independent of release packaging",
        "started_utc": started, "finished_utc": datetime.now(timezone.utc).isoformat(),
        "working_tree_active": True, "final_archive_qualified": False,
        "status": "findings" if failed else "passed_checkpoint",
        "archive": {"path": str(baseline), "bytes": archive_stat.st_size, "sha256": archive_hash,
                    "matches_recorded_baseline": True, "non_directory_members": len(original), "duplicate_members": duplicate_members},
        "baseline_inventory": {"path": inventory_path, "independently_checked_against_zip": independently_read == original,
                               "content_mismatches": [p for p in original if independently_read.get(p) != original[p]],
                               "path_set_matches": set(independently_read) == set(original)},
        "expected_edit_allowlist": dict(sorted(ALLOWED.items())),
        "expected_relocations": RELOCATIONS, "expected_new_evidence_prefixes": NEW_PREFIXES,
        "summary": {"exact_at_original_path": classification_counts["exact"],
                    "exact_at_retained_path": classification_counts["exact_relocated"],
                    "expected_modified_members": classification_counts["expected_edit"],
                    "unexpected_modified_members": len(unexpected), "missing_members": len(missing),
                    "new_paths_observed": len(new_paths), "unclassified_new_paths": len(unclassified)},
        "unexpected_modified_paths": unexpected, "missing_paths": missing,
        "nonregular_paths": sorted(set(nonregular)), "changed_during_individual_hash": changed_during,
        "retained_originals": retained, "release_history_retention": history, "protected_groups": groups,
        "metadata_caveat": {
            "working_tree_mode_difference_count": len(modes),
            "observed_pairs": [{"archive_mode": a, "observed_mode": b, "files": count}
                               for (a, b), count in sorted(Counter((m["archive_mode"], m["observed_mode"]) for m in modes).items())],
            "integrator_disposition": "No extraction normalization exception is granted. Any observed difference requires independent disposition.",
            "final_archive_requirement": "Preserve the exact baseline Unix modes through the explicit relocation; preserve modes on archived current reports and independently re-read the final ZIP.",
            "unexpected_mode_differences": modes, "mode_differences": modes,
        },
        "new_paths": sorted(new_paths, key=lambda item: item["path"]), "unclassified_new_paths": sorted(unclassified),
        "expected_edits_not_yet_observed": sorted(p for p in ALLOWED if p in original and evidence[p]["classification"] == "exact"),
        "excluded_transient_directories": sorted(EXCLUDED), "excluded_transient_suffixes": [".pyc", ".pyo"],
        "reports_created_after_inventory": sorted(REPORTS),
        "member_evidence": evidence,
        "limits": [
            "The active working tree is not an atomic final-source snapshot; later authorized changes require a fresh inventory or final packer check.",
            "New evidence paths identify additions and do not endorse test, screenshot or release success.",
            "An allowed path is not permission for unrelated semantic edits; reasons are limited to the explicit work package and owner assignment.",
            "Source content preservation does not establish publisher authentication, target-platform support or successful canonical acceptance.",
            "The old root delivery manifest may remain temporarily; remove it only after verifying its exact retained copy. The packer requires it absent before generating the new manifest.",
        ],
        "allowlist_amendments_after_observation": AMENDMENTS,
    }
    (output / "preservation.json").write_text(json.dumps(result, indent=2) + "\n")
    lines = ["# Independent 0.23 source-preservation policy and checkpoint", "",
             f"**Status: {result['status']}.** This is an active-tree checkpoint, not final source/archive acceptance.", "",
             f"Baseline: exact delivered 0.22.0 source ZIP, {BASELINE_BYTES:,} bytes and {len(original):,} regular members. SHA-256: `{archive_hash}`.", "",
             "The edit allowlist is derived from the integrator's explicit assignments and specialist ownership. It is never populated from whatever changed. All other original files must remain byte-exact; only the prior root manifest has an explicit relocation.", "",
             "## Observed preservation", "", "| Result | Files |", "|---|---:|"]
    lines += [f"| {label.replace('_', ' ')} | {count} |" for label, count in result["summary"].items()]
    lines += ["", f"Original Unix mode differences: **{len(modes)}**. Nonregular paths: **{len(nonregular)}**. Files that changed during their own hash: **{len(changed_during)}**.", "",
              "## Protected original groups", "", "| Group | Original files | Byte-exact |", "|---|---:|---:|"]
    lines += [f"| {label.replace('_', ' ')} | {value['files']} | {value['exact_content']} |" for label, value in groups.items()]
    lines += ["", "Groups overlap. Every prior member is also independently recorded in the JSON companion; broad group names do not authorize modifications.", "",
              "## Retained current documents and manifest", "", "| Prior path | Retained path | Exact bytes and mode |", "|---|---|---|"]
    lines += [f"| `{item['archive_path']}` | `{item['retained_path']}` | {'Yes' if item['matches_archive'] and item['mode_matches_archive'] else 'NO — finding'} |" for item in retained]
    lines += ["", "The old root manifest can be removed after this exact-copy proof. The new source packer refuses a stale root manifest and generates a new one only for the actual completed 0.23 acceptance.", "",
              "## Release history", ""]
    for name, value in history.items():
        lines.append(f"- `{name}`: complete prior text retained verbatim: **{value['entire_prior_text_retained_verbatim']}**; every prior release entry retained verbatim: **{value['prior_release_entries_retained_verbatim']}**. New sections may be inserted after the document title; this distinction is recorded explicitly.")
    lines += ["", "## Explicit edit policy", "", "| Path | Dispatched reason |", "|---|---|"]
    lines += [f"| `{name}` | {reason} |" for name, reason in sorted(ALLOWED.items())]
    lines += ["", "New evidence is confined to `docs/verification/v0.23-` paths. Exact historical-copy checks still apply inside that namespace. No old verification file, runtime asset, source data, recipe, generator, SDK example, or license receives a broad edit exception.", "",
              "## Authorized policy amendments", ""]
    lines += [f"- `{item['path']}`: {item['authorization']} {item['reason']} Supporting receipt: `{item['evidence']}`." for item in AMENDMENTS]
    lines += ["",
              "## Findings", ""]
    findings = {"Unexpected edits": unexpected, "Missing originals": missing,
                "Unclassified additions": unclassified, "Nonregular paths": sorted(set(nonregular)),
                "Changed during hashing": changed_during,
                "Mode differences": [m["path"] for m in modes]}
    lines += [f"- {label}: " + (", ".join(f"`{p}`" for p in paths) or "none") + "." for label, paths in findings.items()]
    lines += ["", "## Reproduce after documents settle", "", "```sh",
              "python3 docs/verification/v0.23-review/audit_preservation.py \\",
              "  --baseline /workspace/scratch/96f1d170dd13/baseline/terra-world-0.22.0.zip", "```", "",
              "The [machine-readable policy and member evidence](preservation.json) uses the existing preservation schema. The [independent baseline inventory](baseline.json) records every ZIP member's digest, size and mode. Final packaging must independently re-read source, baseline, accepted receipt/evidence and every output ZIP member.", "",
              "## Limits", ""]
    lines += ["- " + limit for limit in result["limits"]]
    lines += ["", f"Observation: {started} to {result['finished_utc']}.", ""]
    (output / "preservation.md").write_text("\n".join(lines))
    print(json.dumps({"status": result["status"], "summary": result["summary"],
                      "unexpected_modified_paths": unexpected, "unclassified_new_paths": unclassified,
                      "mode_differences": len(modes), "retained_originals": retained}, indent=2))
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
