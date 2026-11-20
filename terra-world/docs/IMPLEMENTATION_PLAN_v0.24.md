# Terra World 0.24 — improvement and polishing plan

## Objective and pinned baseline

Deliver the next reusable world-framework and native-reference-game increment from the exact delivered **0.23.0**. The user authorizes product-wide review, primary research, dedicated subagent implementation, end-to-end execution and final polish. This plan is written before implementation dispatch on **30 October 2026**.

The original archive is pinned at SHA-256 `99d619069fc8f6275e625f40a7c4a916535ededcafb20f733f34ef0a3f361140` (313,526,188 bytes; 3,092 regular members). The current tree matched every original member except the generated root delivery manifest, which is preserved byte-for-byte under `verification/v0.24-review/baseline-delivery-manifest.json`. Original current review and validation are retained as `PRODUCT_REVIEW_v0.23.md` and `VALIDATION_v0.23.md`. All historical evidence/assets/data/examples/licenses are protected.

## Product outcome

A player should find useful funded work over an extended career, execute the operation and destination they reviewed, and retain reliable world data. A framework integrator should receive an accepted source and desktop artifact whose verification cannot succeed while ordinary owned work is still running.

The 0.24 increment closes concrete gaps in **supply-board turnover**, **Industry/Fleet/building-damage action intent**, **cross-platform protected file identity**, and **bounded release execution**. It preserves the native forest palette, existing interaction model, finite money, authoritative domain ownership, source/catalog identity, serialized command shapes and saved schemas.

## Review coverage and ownership

| Owner | Perspectives and implementation ownership |
|---|---|
| Player specialist | First/returning sessions, task discovery, action meaning, destructive consent, input/accessibility, native Industry/Fleet/Build/community presentation. Owns affected `terra-demo` native files and tests. |
| World specialist | Career goals, finite economy, escrow lifecycle, inventories/capacity, world simulation, deterministic authority, progression and SDK seams. Owns `terra-sandbox` supply/economy changes and focused continuation tests. |
| Reliability specialist | Persistence, file identity, concurrency, subprocess lifetime/output, portable tooling, CI and extracted desktop verification. Owns affected persistence files, shared tooling/tests and CI. |
| Integrator | Product breadth, research, policy decisions, plan, version integration, current docs/gaps, fresh native baseline/candidate/confirmation, canonical acceptance, preservation and handoff. |

Review also covers community governance, vehicle/navigation behavior, source/geography, rendering/performance, production content/audio, networking/privacy/security, operational recovery and distributed architecture. A lack of a new edit in an area is not evidence that its broader requirements are complete. All G01–G18 remain tracked in the new register with selected closures and the next required proof.

## Prioritized evidence

| Priority | Finding | Evidence and desired outcome |
|---|---|---|
| P1 | Expired public supply offers renew forever; obsolete requests hold scarce board slots and escrow. | `settlement/economy.rs`: unconditional expiry renewal. Retire only confirmed obsolete public offers after exact, cap-safe issuer credit. |
| P1 | Posting ignores receiving capacity; a one-item shortage posts at least four. | Same module: shortage clamp and missing admission. Post only a useful amount that the receiver can admit. |
| P1 | Industry/Fleet explicit missing selections may resolve to other loaded objects. | `interface.rs`, `fleet.rs` selectors. Preserve missing explicit selection and require deliberate browsing. |
| P1 | Routine Industry/Fleet operations can resolve targets/state after the player activated a different displayed operation. | Native queue/dispatch paths. Retain reviewed target, desired state and relevant amounts across activation and dispatch. |
| P1 | Building damage is destructive but unnamed and selects the nearest building at dispatch. | Build panel and `Action::Damage`. Provide exact target/consequence review with cancel/confirm and no hold-only requirement. |
| P1 | Canonical runner accepts a successful leader while an ordinary child still writes; output spooling/readback is unlimited. | Reliability reviewer reproduced a child writing after exit 0 was reported. Bound output, deadline, cleanup and owned-group completion. |
| P1 | Protected output identity checks deliberately omit hard links on non-Unix platforms; new paths through Windows junctions are inadequately normalized. | `terra-persistence::validate_output_path`. Use portable file identity and canonical existing ancestors. |
| P2 | CI packages/upload desktops without testing the extracted delivered tree. | CI workflow. Verify fresh extraction and startup before artifact upload on configured Linux and Windows jobs. |
| P2 | Selected faction can be silently replaced by the first membership. | Native dispatcher. Keep explicit unavailability until the player selects again. |

## Product policy: useful work and escrow

1. Demand comes from the existing producer/retail target rules and current observed stock. New offers use a useful capacity-fitting amount in **1..=12** with existing reward arithmetic.
2. An accepted contract is a commitment. Demand changes alone do not rewrite its amount, reward, owner or deadline and do not invalidate delivery. Existing expiry and return semantics release ownership before public maintenance.
3. Public offers with **confirmed obsolete demand** may retire. Retirement credits the exact retained escrow to the **original issuer**, using checked arithmetic and the existing money cap. Removal and credit are one prepared, all-or-nothing transition.
4. A missing issuer, unknown destination view or failed refund capacity does not justify deleting value. Retain the offer/escrow; expose actionable or explicit unavailable status. Never invent a refund destination or replenish money.
5. Still-needed expired offers renew under their existing identity. Retired IDs are never reused. Maintenance is deterministic and bounded by the existing 16-offer board.
6. Readiness and actual acceptance use the same derived public-demand rule; acceptance rechecks authority. Replica absence is unknown, not proof of global deletion.

This is an explicit Terra product decision. Research supports transactional/concurrency principles but does not dictate this game's exact lifecycle policy.

## Work packages and acceptance

### WP01 — useful supply posting

**Owner:** world. Share a demand/capacity query across posting and public lifecycle evaluation. Remove the minimum-four over-order and reject or reduce offers that cannot fit. Preserve accepted terms, reward arithmetic and finite money.

**Acceptance:** one-item shortages; aggregate producer capacity; full inventories; recipe changes/disabled services; deterministic useful amounts; no reserved reward for an impossible new offer.

### WP02 — public offer retirement and continued careers

**Owner:** world. Retire confirmed obsolete unclaimed offers only after exact issuer refund is possible. Keep cap-blocked refunds and unknown destinations recoverable; avoid permanent board starvation. Add a typed acceptance blocker and native-ready explanation.

**Acceptance:** saturated 16-slot board turns over after demand fills; original issuer credit and escrow conservation; refund cap then real spending/retry; accepted delivery remains honored; expiry/return; monotonic IDs; deterministic no-double-refund; competing admissions; disk save/restore continuation and validated state.

### WP03 — Industry reviewed operations

**Owner:** player. Preserve explicit facility identity and displayed desired pause/resume state. Name the operation and missing/ownership/reach recovery. Use existing native intent discipline through actual activation and queueing.

**Acceptance:** unchanged operation succeeds; missing/changed site, ownership, enabled state, body/player/restore changes and same-frame chooser/action reject before mutation. Current direct selection and deliberate browse still work.

### WP04 — Fleet reviewed operations

**Owner:** player. Preserve explicit driver/route/vehicle/stop/relief/depot selectors. Retain reviewed context for assignment, run/pause, hold/release, handover and depot stock actions; retain existing quote/destructive safeguards.

**Acceptance:** real Activate-to-dispatch regressions for changed/missing targets and same-frame selection; exact item/amount/direction; no changed cargo, money, employee assignment or running state on stale rejection; normal company journey remains usable.

### WP05 — named building damage and community selection

**Owner:** player. Prepare a named target and consequences for damage; explicit cancel/confirm in existing scroll/focus pattern with safe initial focus. Recheck exact structure, identity, ownership, eligibility and reach. Never replace an explicitly missing faction during routine dispatch.

**Acceptance:** cancel makes no mutation; confirmed intended target changes by the reviewed amount; vanished/replaced target, changed health/ownership/body/player/restore reject; keyboard/controller-style focus stays usable. No new wire/save schema.

### WP06 — protected file aliases across platforms

**Owner:** reliability. Replace Unix-only hard-link protection with portable existing-file identity, using the already locked dependency if suitable. Resolve existing path ancestors to protect new outputs through aliases/junctions, retaining explicit error behavior.

**Acceptance:** database, sidecar and managed-world protected files cannot be overwritten through distinct hard-link names; absent destinations under aliased parents resolve coherently; Linux tests run, Windows-specific fixtures are added to the existing Windows job; no actual Windows execution claim without its receipt.

### WP07 — bounded canonical gate processes

**Owner:** reliability. Stream logs under a generous explicit configurable maximum; bound leader execution, EOF and cleanup; require ordinary owned POSIX group completion. Preserve useful partial logs and typed failure reasons. Avoid unsafe unbounded final waits.

**Acceptance:** noisy, hanging, failed, successful, pipe-held and redirected-child programs; no success before owned work finishes; output and retained log bounds; subprocess/source receipts remain truthful. Document platform and deliberately detached-child limits.

### WP08 — extracted desktop qualification in CI

**Owner:** reliability. Add reusable safe ZIP extraction and exact-tree verification, invoking passive checks and the bounded startup smoke. Integrate both configured CI platforms before upload, retaining failed reports.

**Acceptance:** reject path traversal, namespace/duplicate/type surprises and mismatched content; preserve executable modes; correct packaged tree passes; no executing a binary before passive verification succeeds; actual Linux final artifact is verified by a fresh extraction. Windows CI wiring is not Windows execution evidence.

### WP09 — release integration and researched documentation

**Owner:** integrator. Version workspace/crates to 0.24.0, preserve existing external pins except an explicitly justified promotion of an already-locked identity dependency. Update README/product/design/compatibility/current review/guidance/recovery/delivery/validation and all carried gaps. Archive prior current reports exactly.

**Acceptance:** clear public API/compatibility/policy changes; no inflated claims; local links checked; historical file and mode inventory accounted for; primary-source decisions and remaining acceptance written.

### WP10 — independent cross-review and final polish

**Owner:** all, integrator coordinates. Rotate specialists across native/domain/reliability boundaries. Capture a fresh 0.23 baseline, one candidate native journey at 1440x900 and 1100x700 with 150% text, consolidate material corrections, and confirm once with direct executable/process provenance.

**Acceptance:** inspect every accepted image; distinguish screenshots from adversarial tests; retain failed observations; complete tasks and explicit blockers; source/binary/capture identities recorded. No claim of 200% or physical-device qualification.

### WP11 — canonical end-to-end acceptance

**Owner:** integrator. Run the canonical offline gate suite on the final implementation with exact log/artifact/source receipts and final native-confirmed executable. Resolve actual failures, rerunning only the affected work before final canonical acceptance.

**Acceptance:** every required gate passes, terminal receipt/source identity stable, no partial or failed receipt authorizes packaging. Finite workloads retain their stated scale and platform.

### WP12 — preserved, verified delivery

**Owner:** integrator plus independent reliability check. Package full source and Linux desktop only after acceptance; verify every included byte/mode and prior member, safely extract and smoke the desktop, preserve manifest history, save complete source/desktop/reports.

**Acceptance:** exact delivered archive identities; current source/evidence/executable match accepted receipts; zero missing or unexplained baseline changes; all intended artifacts available to the user. Source/desktop are the complete next increment, not a patch archive.

## Sequence and coordination

Review and baseline pin -> this plan -> independent owned implementation -> focused meaningful regression checks -> rotated review -> integrated build/native candidate -> one consolidated correction batch -> final confirmation -> canonical acceptance -> documentation/preservation freeze -> desktop/source packaging and independent readback -> saved handoff.

No agents edit the same owned file without a message/transfer. World announces public API variants before the player updates labels. Root owns version and current-doc changes; reliability may own its crate dependency edit with explicit coordination. Builds use the existing pinned toolchain/cache, with long cargo invocations coordinated rather than piled up behind a lock.

## Explicit remaining requirements

Real Windows/physical GPU/input/assistive qualification, uncoached measured careers, full 200% reflow, production art/audio/data licensing, asset-owning governance/migration, trusted signing/upgrades, public-service controls, WAN soaks, offsite durability and distributed authority remain in G01–G18. This iteration improves their foundation but does not substitute synthetic tests or CI configuration for missing real-world proof.

## Execution ledger

Implementation has been dispatched only after this plan was written. Results, corrections and actual acceptance are appended here as work completes; pending items are not recorded as passed.


### Implementation and focused acceptance

WP01–08 implementation is complete. Domain acceptance passed 83 tests across six supply/guidance suites; native acceptance passed 250 activation/dispatch and interface tests; persistence passed 53 tests; Python delivery/execution tooling passed 72 tests. These focused totals overlap the completed canonical run and must not be added to it. The sandbox and native receipts include unchanged input maps. Reliability receipts bind commands, complete logs and bounded execution metadata; final canonical acceptance binds the complete current implementation.

Independent reviews identified and closed rental countdown overbinding, incomplete new receipt admission, ambiguous Windows unresolved Unicode aliases, source additions after a preservation checkpoint and historical-entry changes after a checkpoint. Corrections retain finite escrow, accepted terms, explicit action intent and historical evidence. Failed attempts remain retained.

Specialist records: [player](verification/v0.24-review/player-experience.md), [world](verification/v0.24-review/world-framework.md), [reliability](verification/v0.24-review/reliability-delivery.md), [native cross-review](verification/v0.24-review/world-player-cross-review.md), [domain/tooling cross-review](verification/v0.24-review/player-cross-review.md), and [source packaging](verification/v0.24-review/source-packaging-review.md).

WP09 documentation integration and policy cross-review are complete. WP10 source-bound native confirmation is complete: the final two-label build matches the original clean building run exactly, and a fresh Fleet restore saved/quit with unchanged source. WP11 passed all 44 canonical gates, with 1,099 distinct Rust tests across 1,200 executions and a separate 72-test Python gate. There were no failed or ignored Rust tests. All 390 implementation inputs, published logs and reports remained unchanged.

The accepted native audit binds the actual successful terminal receipt to two clean final-executable processes and 18 individually inspected captures. See [Validation](VALIDATION.md) for exact timestamps, hashes, counts and workload limits.

WP12 artifact results are recorded in the separate [delivery record](verification/v0.24-delivery/README.md) and the external source-verification JSON. This keeps final archive byte identities outside their own payloads. The actual extracted-desktop verification and full source readback are the completion evidence; the implementation and focused tests alone cannot authorize delivery.


### Native journey and bounded final correction

The candidate completed supply delivery, named Industry pause/resume, building creation and damage review, company funding/recruitment, route assignment and service start/pause, followed by a saved exit. The actual saved checkpoints passed 20 independent selected checks; two restored copies passed a further 50 scoped state/identity checks. These counts are evidence checks, not Rust test executions. The company setup still requires manual garage navigation; that observed discovery gap remains in G04/G05.

The minimum-window copy correction shortened Review damage / Confirm damage. A subsequent visual assessment incorrectly reported non-button text loss after changing focus. Exact RGB comparisons disproved that finding: the original explanation and Build prose were present and identical across the cited frames. The finding was withdrawn, and the experimental rendering system and its test were removed to restore the exact input map from final build 01. All original observations, attempted tests/builds and the independent correction remain retained. No renderer change is part of the delivered increment. Canonical native acceptance remains the 250-test suite.
