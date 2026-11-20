# Independent 0.24 source-preservation policy and checkpoint

**Status: passed_checkpoint.** This is an active-tree checkpoint, not final source/archive acceptance.

Baseline: exact delivered 0.23.0 source ZIP, 313,526,188 bytes and 3,092 regular members. SHA-256: `99d619069fc8f6275e625f40a7c4a916535ededcafb20f733f34ef0a3f361140`.

The edit allowlist is derived from the integrator's explicit assignments and specialist ownership. It is never populated from whatever changed. All other original files must remain byte-exact; only the prior root manifest has an explicit relocation.

## Observed preservation

| Result | Files |
|---|---:|
| exact at original path | 3045 |
| exact at retained path | 1 |
| expected modified members | 46 |
| unexpected modified members | 0 |
| missing members | 0 |
| new paths observed | 516 |
| unclassified new paths | 0 |

Original Unix mode differences: **0**. Nonregular paths: **0**. Files that changed during their own hash: **0**.

## Protected original groups

| Group | Original files | Byte-exact |
|---|---:|---:|
| runtime assets including recipes and generators | 24 | 24 |
| source data | 70 | 70 |
| configuration examples | 9 | 9 |
| sdk examples | 14 | 14 |
| authoring sources and artifacts | 10 | 10 |
| historical verification evidence | 2208 | 2208 |
| historical screenshots | 497 | 497 |
| versioned root documents and receipts | 130 | 130 |
| licenses and notices | 3 | 3 |
| all prior members outside explicit edit allowlist | 3046 | 3046 |

Groups overlap. Every prior member is also independently recorded in the JSON companion; broad group names do not authorize modifications.

## Retained current documents and manifest

| Prior path | Retained path | Exact bytes and mode |
|---|---|---|
| `docs/PRODUCT_REVIEW.md` | `docs/PRODUCT_REVIEW_v0.23.md` | Yes |
| `docs/VALIDATION.md` | `docs/VALIDATION_v0.23.md` | Yes |
| `DELIVERY_MANIFEST.json` | `docs/verification/v0.24-review/baseline-delivery-manifest.json` | Yes |

The old root manifest can be removed after this exact-copy proof. The new source packer refuses a stale root manifest and generates a new one only for the actual completed 0.24 acceptance.

## Release history

- `CHANGELOG.md`: complete prior text retained verbatim: **False**; every prior release entry retained verbatim: **True**. New sections may be inserted after the document title; this distinction is recorded explicitly.
- `docs/RELEASE_HISTORY.md`: complete prior text retained verbatim: **False**; every prior release entry retained verbatim: **True**. New sections may be inserted after the document title; this distinction is recorded explicitly.

## Explicit edit policy

| Path | Dispatched reason |
|---|---|
| `.github/workflows/ci.yml` | Reliability WP07–08: bounded subprocess evidence, complete current receipt admission, safe extraction/passive-before-smoke and CI-before-upload plus actual adversarial tests. |
| `CHANGELOG.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `Cargo.lock` | Integrator WP09: workspace0.24 versions; reliability WP06 adds only the exact already-locked same-file dependency edge. No external package version changes. |
| `Cargo.toml` | Integrator WP09: workspace0.24 versions; reliability WP06 adds only the exact already-locked same-file dependency edge. No external package version changes. |
| `DESIGN.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `PRODUCT.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `README.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `crates/terra-demo/src/building_review.rs` | Player WP03–05: exact operational selections and stable reviewed context, named production and damage consent/focus, meaningful native activation/continuation regressions and aligned supply fixture demand. No serialized protocol/save changes. |
| `crates/terra-demo/src/command_input.rs` | Player WP03–05: exact operational selections and stable reviewed context, named production and damage consent/focus, meaningful native activation/continuation regressions and aligned supply fixture demand. No serialized protocol/save changes. |
| `crates/terra-demo/src/communities.rs` | Player WP03–05: exact operational selections and stable reviewed context, named production and damage consent/focus, meaningful native activation/continuation regressions and aligned supply fixture demand. No serialized protocol/save changes. |
| `crates/terra-demo/src/fleet.rs` | Player WP03–05: exact operational selections and stable reviewed context, named production and damage consent/focus, meaningful native activation/continuation regressions and aligned supply fixture demand. No serialized protocol/save changes. |
| `crates/terra-demo/src/fleet_operations.rs` | Player WP03–05: exact operational selections and stable reviewed context, named production and damage consent/focus, meaningful native activation/continuation regressions and aligned supply fixture demand. No serialized protocol/save changes. |
| `crates/terra-demo/src/frontier.rs` | Player WP03–05: exact operational selections and stable reviewed context, named production and damage consent/focus, meaningful native activation/continuation regressions and aligned supply fixture demand. No serialized protocol/save changes. |
| `crates/terra-demo/src/industry.rs` | Player WP03–05: exact operational selections and stable reviewed context, named production and damage consent/focus, meaningful native activation/continuation regressions and aligned supply fixture demand. No serialized protocol/save changes. |
| `crates/terra-demo/src/interaction.rs` | Player WP03–05: exact operational selections and stable reviewed context, named production and damage consent/focus, meaningful native activation/continuation regressions and aligned supply fixture demand. No serialized protocol/save changes. |
| `crates/terra-demo/src/interface.rs` | Player WP03–05: exact operational selections and stable reviewed context, named production and damage consent/focus, meaningful native activation/continuation regressions and aligned supply fixture demand. No serialized protocol/save changes. |
| `crates/terra-demo/src/main.rs` | Player WP03–05: exact operational selections and stable reviewed context, named production and damage consent/focus, meaningful native activation/continuation regressions and aligned supply fixture demand. No serialized protocol/save changes. |
| `crates/terra-demo/src/native_operation_tests.rs` | Player WP03–05: exact operational selections and stable reviewed context, named production and damage consent/focus, meaningful native activation/continuation regressions and aligned supply fixture demand. No serialized protocol/save changes. |
| `crates/terra-demo/src/reviewed_action.rs` | Player WP03–05: exact operational selections and stable reviewed context, named production and damage consent/focus, meaningful native activation/continuation regressions and aligned supply fixture demand. No serialized protocol/save changes. |
| `crates/terra-persistence/Cargo.toml` | Reliability WP06: exact same-file1.0.6 edge, portable protected identity and conservative unresolved Windows aliases with platform/pure regression fixtures. |
| `crates/terra-persistence/src/lib.rs` | Reliability WP06: exact same-file1.0.6 edge, portable protected identity and conservative unresolved Windows aliases with platform/pure regression fixtures. |
| `crates/terra-persistence/src/tests.rs` | Reliability WP06: exact same-file1.0.6 edge, portable protected identity and conservative unresolved Windows aliases with platform/pure regression fixtures. |
| `crates/terra-sandbox/README.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `crates/terra-sandbox/src/guidance.rs` | World WP01–02: useful demand/capacity, exact public escrow retirement and accepted promise preservation, lazy readiness, real conservation/continuation and valid-demand fixture updates. |
| `crates/terra-sandbox/src/settlement/commands.rs` | World WP01–02: useful demand/capacity, exact public escrow retirement and accepted promise preservation, lazy readiness, real conservation/continuation and valid-demand fixture updates. |
| `crates/terra-sandbox/src/settlement/demand.rs` | World WP01–02: useful demand/capacity, exact public escrow retirement and accepted promise preservation, lazy readiness, real conservation/continuation and valid-demand fixture updates. |
| `crates/terra-sandbox/src/settlement/economy.rs` | World WP01–02: useful demand/capacity, exact public escrow retirement and accepted promise preservation, lazy readiness, real conservation/continuation and valid-demand fixture updates. |
| `crates/terra-sandbox/src/settlement/mod.rs` | World WP01–02: useful demand/capacity, exact public escrow retirement and accepted promise preservation, lazy readiness, real conservation/continuation and valid-demand fixture updates. |
| `crates/terra-sandbox/src/settlement/supply.rs` | World WP01–02: useful demand/capacity, exact public escrow retirement and accepted promise preservation, lazy readiness, real conservation/continuation and valid-demand fixture updates. |
| `crates/terra-sandbox/tests/guidance_commitments.rs` | World WP01–02: useful demand/capacity, exact public escrow retirement and accepted promise preservation, lazy readiness, real conservation/continuation and valid-demand fixture updates. |
| `crates/terra-sandbox/tests/supply_readiness.rs` | World WP01–02: useful demand/capacity, exact public escrow retirement and accepted promise preservation, lazy readiness, real conservation/continuation and valid-demand fixture updates. |
| `crates/terra-sandbox/tests/supply_turnover.rs` | World WP01–02: useful demand/capacity, exact public escrow retirement and accepted promise preservation, lazy readiness, real conservation/continuation and valid-demand fixture updates. |
| `docs/ARCHITECTURE.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `docs/COMPANIES.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `docs/COMPATIBILITY.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `docs/CONTROLS.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `docs/DESKTOP_DELIVERY.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `docs/GAME_FLOW.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `docs/GUIDANCE.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `docs/IMPLEMENTATION_PLAN_v0.24.md` | Integrator new current plan, primary research and complete G01–G18 continuation. |
| `docs/MULTIPLAYER.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `docs/PLAYTESTING.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `docs/PRODUCT_REVIEW.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `docs/PRODUCT_REVIEW_v0.23.md` | Exact retained copies of delivered0.23 current documents; no content or mode edits allowed. |
| `docs/RELEASE_HISTORY.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `docs/RESEARCH_v0.24.md` | Integrator new current plan, primary research and complete G01–G18 continuation. |
| `docs/REVIEW_GAPS_v0.24.md` | Integrator new current plan, primary research and complete G01–G18 continuation. |
| `docs/SETTLEMENTS_AND_ECONOMY.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `docs/VALIDATION.md` | Integrator WP09–12: current product/policy/API/compatibility/research/actual-evidence handoff and exact retained historical entries. No fabricated qualification. |
| `docs/VALIDATION_v0.23.md` | Exact retained copies of delivered0.23 current documents; no content or mode edits allowed. |
| `scripts/desktop-payload.json` | Integrator WP09/WP12: add current reports, archived0.23 current docs and the standalone archive verifier, retaining all previous payload entries. |
| `scripts/package_desktop.py` | Reliability WP07–08: bounded subprocess evidence, complete current receipt admission, safe extraction/passive-before-smoke and CI-before-upload plus actual adversarial tests. |
| `scripts/release_contract.py` | Reliability WP07–08: bounded subprocess evidence, complete current receipt admission, safe extraction/passive-before-smoke and CI-before-upload plus actual adversarial tests. |
| `scripts/test_package_desktop.py` | Reliability WP07–08: bounded subprocess evidence, complete current receipt admission, safe extraction/passive-before-smoke and CI-before-upload plus actual adversarial tests. |
| `scripts/test_verify_desktop.py` | Reliability WP07–08: bounded subprocess evidence, complete current receipt admission, safe extraction/passive-before-smoke and CI-before-upload plus actual adversarial tests. |
| `scripts/test_verify_desktop_archive.py` | Reliability WP07–08: bounded subprocess evidence, complete current receipt admission, safe extraction/passive-before-smoke and CI-before-upload plus actual adversarial tests. |
| `scripts/test_verify_execution.py` | Reliability WP07–08: bounded subprocess evidence, complete current receipt admission, safe extraction/passive-before-smoke and CI-before-upload plus actual adversarial tests. |
| `scripts/verify.py` | Reliability WP07–08: bounded subprocess evidence, complete current receipt admission, safe extraction/passive-before-smoke and CI-before-upload plus actual adversarial tests. |
| `scripts/verify_desktop.py` | Reliability WP07–08: bounded subprocess evidence, complete current receipt admission, safe extraction/passive-before-smoke and CI-before-upload plus actual adversarial tests. |
| `scripts/verify_desktop_archive.py` | Reliability WP07–08: bounded subprocess evidence, complete current receipt admission, safe extraction/passive-before-smoke and CI-before-upload plus actual adversarial tests. |

New evidence is confined to `docs/verification/v0.24-` paths. Exact historical-copy checks still apply inside that namespace. No old verification file, runtime asset, source data, recipe, generator, SDK example, or license receives a broad edit exception.

## Authorized policy amendments


## Findings

- Unexpected edits: none.
- Missing originals: none.
- Unclassified additions: none.
- Nonregular paths: none.
- Changed during hashing: none.
- Mode differences: none.

## Reproduce after documents settle

```sh
python3 docs/verification/v0.24-review/audit_preservation.py \
  --baseline /workspace/scratch/96f1d170dd13/baseline/terra-world-0.23.0.zip
```

The [machine-readable policy and member evidence](preservation.json) uses the existing preservation schema. The [independent baseline inventory](baseline-inventory.json) records every ZIP member's digest, size and mode. Final packaging must independently re-read source, baseline, accepted receipt/evidence and every output ZIP member.

## Limits

- The active working tree is not an atomic final-source snapshot; later authorized changes require a fresh inventory or final packer check.
- New evidence paths identify additions and do not endorse test, screenshot or release success.
- An allowed path is not permission for unrelated semantic edits; reasons are limited to the explicit work package and owner assignment.
- Source content preservation does not establish publisher authentication, target-platform support or successful canonical acceptance.
- The old root delivery manifest may remain temporarily; remove it only after verifying its exact retained copy. The packer requires it absent before generating the new manifest.

Observation: 2026-10-30T13:42:49.600508+00:00 to 2026-10-30T13:42:53.030571+00:00.
