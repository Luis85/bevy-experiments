# Independent 0.23 source-preservation policy and checkpoint

**Status: passed_checkpoint.** This is an active-tree checkpoint, not final source/archive acceptance.

Baseline: exact delivered 0.22.0 source ZIP, 274,052,010 bytes and 2,790 regular members. SHA-256: `54ba5420db937056ba62e1f7861308a4ea540112f52ae500af10e92e897a739e`.

The edit allowlist is derived from the integrator's explicit assignments and specialist ownership. It is never populated from whatever changed. All other original files must remain byte-exact; only the prior root manifest has an explicit relocation.

## Observed preservation

| Result | Files |
|---|---:|
| exact at original path | 2755 |
| exact at retained path | 1 |
| expected modified members | 34 |
| unexpected modified members | 0 |
| missing members | 0 |
| new paths observed | 300 |
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
| historical verification evidence | 1914 | 1914 |
| historical screenshots | 431 | 431 |
| versioned root documents and receipts | 125 | 125 |
| licenses and notices | 3 | 3 |
| all prior members outside explicit edit allowlist | 2756 | 2756 |

Groups overlap. Every prior member is also independently recorded in the JSON companion; broad group names do not authorize modifications.

## Retained current documents and manifest

| Prior path | Retained path | Exact bytes and mode |
|---|---|---|
| `docs/PRODUCT_REVIEW.md` | `docs/PRODUCT_REVIEW_v0.22.md` | Yes |
| `docs/VALIDATION.md` | `docs/VALIDATION_v0.22.md` | Yes |
| `DELIVERY_MANIFEST.json` | `docs/verification/v0.23-review/baseline-delivery-manifest.json` | Yes |

The old root manifest can be removed after this exact-copy proof. The new source packer refuses a stale root manifest and generates a new one only for the actual completed 0.23 acceptance.

## Release history

- `CHANGELOG.md`: complete prior text retained verbatim: **False**; every prior release entry retained verbatim: **True**. New sections may be inserted after the document title; this distinction is recorded explicitly.
- `docs/RELEASE_HISTORY.md`: complete prior text retained verbatim: **False**; every prior release entry retained verbatim: **True**. New sections may be inserted after the document title; this distinction is recorded explicitly.

## Explicit edit policy

| Path | Dispatched reason |
|---|---|
| `CHANGELOG.md` | Integrator WP08: add the 0.23 increment while retaining every prior release entry verbatim. |
| `Cargo.lock` | Integrator WP08: align workspace lockfile package versions with 0.23.0, without dependency churn. |
| `Cargo.toml` | Integrator WP08: advance workspace package version to 0.23.0; preserve pinned external dependencies. |
| `PRODUCT.md` | Integrator WP08: current product scope and reviewed 0.23 behavior, preserving explicit capability limits. |
| `README.md` | Integrator WP08-WP10: current 0.23 overview, entry points, acceptance limits and complete delivery links. |
| `crates/terra-demo/src/actor_summary.rs` | Player WP01: present the exact owned active person used by quick consumption actions. |
| `crates/terra-demo/src/flow.rs` | Player WP09 consolidated native polish: describe effective atmospheric-sky state in pause/settings when Performance quality keeps sky off, preserving the stored preference and rendering policy. |
| `crates/terra-demo/src/frontier.rs` | Player WP01-WP03: bind displayed journey/request context through activation and use shared supply-readiness explanations and refusal; WP09 screenshot-led plural and readable item-label polish. |
| `crates/terra-demo/src/interaction.rs` | Player WP01-WP02: reject stale queued owner/body/restore/target context without retargeting commands. |
| `crates/terra-demo/src/interface.rs` | Player WP01-WP02: retain displayed Nature/territory target identity and show concrete action/recovery context. |
| `crates/terra-demo/src/launcher/model.rs` | Reliability WP05: share the stable managed-world lease with metadata maintenance and protect rename publication. |
| `crates/terra-demo/src/launcher/recovery.rs` | Reliability WP06: hold managed maintenance ownership through native backup publication while preserving typed recovery outcomes. |
| `crates/terra-demo/src/launcher/recovery_tests.rs` | Reliability WP05-WP06: exercise real process/session contention, clean/crash release and publication/retention boundaries. |
| `crates/terra-demo/src/launcher/ui.rs` | Player WP09 consolidated native polish: explain effective sky behavior in launcher settings/review after independent fresh screenshots showed stored preference and Performance quality appeared contradictory. |
| `crates/terra-sandbox/README.md` | World WP03-WP04: document typed readiness/public framework limits and correct the stale statement about existing safe solo dissolution. |
| `crates/terra-sandbox/src/guidance.rs` | World WP03-WP04: truthful supply readiness, dormant-owner recovery, expiry filtering and achievable complete inventory exchanges. |
| `crates/terra-sandbox/src/settlement/commands.rs` | World WP03: share acceptance/return preparation and commit a completely prepared supply delivery without changing economic policy. |
| `crates/terra-sandbox/src/settlement/mod.rs` | World WP03: export the read-only supply readiness API and typed blockers from the settlement module. |
| `crates/terra-sandbox/src/settlement/supply.rs` | World WP03 new source: typed read-only readiness and private prepared delivery values shared with actual commands. |
| `crates/terra-sandbox/tests/guidance_capacity.rs` | World WP04 new integration tests: complete multi-output exchange, feasible smaller batches and credible recovery alternatives. |
| `crates/terra-sandbox/tests/supply_readiness.rs` | World WP03-WP04 new integration tests: blockers, conservation, partial views, competing capacity and finite failure/recovery/save/restore. |
| `docs/ARCHITECTURE.md` | World WP03: describe the shared pure-domain preparation seam and authoritative execution boundary. |
| `docs/COMPATIBILITY.md` | Integrator WP08: 0.23 package/public API compatibility and unchanged save/wire boundaries. |
| `docs/CONTROLS.md` | Player WP01-WP02: explain named body actions, displayed targets and stale-action refresh behavior. |
| `docs/DESKTOP_DELIVERY.md` | Reliability WP07: document bounded desktop smoke behavior, evidence and platform limitations. |
| `docs/GAME_FLOW.md` | Player WP01-WP03: document reviewed-action identity, unavailable supply states and continued goal flows. |
| `docs/GUIDANCE.md` | Integrator WP03-WP04 integration: document shared supply readiness, bounded whole-exchange acquisition and recovery guidance if required. |
| `docs/IMPLEMENTATION_PLAN_v0.23.md` | Integrator new current plan: dispatched work packages, acceptance criteria, scope and delivery sequence. |
| `docs/MULTIPLAYER.md` | Integrator WP08 compatibility gate: identify Terra 0.23 as retaining wire 11 from 0.22; preserve protocol constants and behavior. |
| `docs/PLAYTESTING.md` | Integrator WP09: current 0.23 journey checks, evidence paths and explicit human/device validation limits if required. |
| `docs/PRODUCT_REVIEW.md` | Integrator WP08-WP10: current all-perspective 0.23 review; exact prior report retained as PRODUCT_REVIEW_v0.22.md. |
| `docs/PRODUCT_REVIEW_v0.22.md` | Exact retained copy of the delivered 0.22 current product review; no content edits allowed to this new historical copy. |
| `docs/RECOVERY_BUNDLES.md` | Reliability WP05-WP06: distinguish coordinated native maintenance from explicit backend online backup and retained recovery results. |
| `docs/RELEASE_HISTORY.md` | Integrator WP08: archive prior overview context while preserving every existing release entry verbatim. |
| `docs/RESEARCH_v0.23.md` | Integrator new current research: primary-source patterns, actual application and explicit non-goals. |
| `docs/REVIEW_GAPS_v0.23.md` | Integrator new current gap register: scoped closures, retained G01-G18 limits and next required proof. |
| `docs/SETTLEMENTS_AND_ECONOMY.md` | World WP03-WP04: describe supply readiness, explicit value-preserving recovery and focused conservation fixtures. |
| `docs/VALIDATION.md` | Integrator WP10: actual current 0.23 execution and qualification; exact prior report retained as VALIDATION_v0.22.md. |
| `docs/VALIDATION_v0.22.md` | Exact retained copy of the delivered 0.22 current validation; no content edits allowed to this new historical copy. |
| `scripts/desktop-payload.json` | Integrator WP08-WP10: add 0.23 plan/research/gaps and exact archived 0.22 review/validation to the desktop document payload; retain all prior entries. |
| `scripts/test_verify_desktop.py` | Reliability WP07: meaningful overflow, descendant, deadline and bounded-cleanup regression fixtures. |
| `scripts/verify_desktop.py` | Reliability WP07: bound child help-output capture and inherited-pipe lifetime, with finite cleanup and existing payload integrity checks. |

New evidence is confined to `docs/verification/v0.23-` paths. Exact historical-copy checks still apply inside that namespace. No old verification file, runtime asset, source data, recipe, generator, SDK example, or license receives a broad edit exception.

## Authorized policy amendments

- `docs/MULTIPLAYER.md`: Integrator explicitly added this original path after the initial preservation checkpoint. The compatibility gate requires the current-release phrase; 0.23 retains wire 11 from 0.22 with unchanged protocol constants. Supporting receipt: `docs/verification/v0.23-preflight/compatibility.json`.
- `crates/terra-demo/src/flow.rs`: Integrator explicitly authorized the player's consolidated native polish after independent fresh candidate screenshots. Clarify effective atmospheric-sky state when Performance quality overrides the retained preference; no rendering-policy change. Supporting receipt: `docs/verification/v0.23-native/README.md`.
- `crates/terra-demo/src/launcher/ui.rs`: Integrator explicitly authorized the player's consolidated native polish after independent fresh candidate screenshots. Clarify effective sky state in launcher settings/review without changing stored settings or rendering behavior. Supporting receipt: `docs/verification/v0.23-native/README.md`.
- `crates/terra-demo/src/frontier.rs`: Integrator explicitly broadened this already allowed player-owned file for the same consolidated native polish. Correct singular/plural counts and show readable item names in the existing journey text. Supporting receipt: `docs/verification/v0.23-native/README.md`.

## Findings

- Unexpected edits: none.
- Missing originals: none.
- Unclassified additions: none.
- Nonregular paths: none.
- Changed during hashing: none.
- Mode differences: none.

## Reproduce after documents settle

```sh
python3 docs/verification/v0.23-review/audit_preservation.py \
  --baseline /workspace/scratch/96f1d170dd13/baseline/terra-world-0.22.0.zip
```

The [machine-readable policy and member evidence](preservation.json) uses the existing preservation schema. The [independent baseline inventory](baseline.json) records every ZIP member's digest, size and mode. Final packaging must independently re-read source, baseline, accepted receipt/evidence and every output ZIP member.

## Limits

- The active working tree is not an atomic final-source snapshot; later authorized changes require a fresh inventory or final packer check.
- New evidence paths identify additions and do not endorse test, screenshot or release success.
- An allowed path is not permission for unrelated semantic edits; reasons are limited to the explicit work package and owner assignment.
- Source content preservation does not establish publisher authentication, target-platform support or successful canonical acceptance.
- The old root delivery manifest may remain temporarily; remove it only after verifying its exact retained copy. The packer requires it absent before generating the new manifest.

Observation: 2026-10-30T09:19:22.001547+00:00 to 2026-10-30T09:19:24.399884+00:00.
