# Independent source preservation audit — 0.22 prepackaging

This is a **prepackaging content audit of an active working tree**, not a final archive acceptance receipt. The integrator must repeat it on the completed source archive. The audit read the original ZIP and current files independently; it changed only these preservation reports.

## Result

The exact delivered 0.21.0 archive contains **2,376 files**. At the observation above, **2,324** remained byte-identical at their original paths, **1** remained byte-identical at its explicitly retained path, and **51** had changes covered by the explicit implementation allowlist. There were **0 missing**, **0 unexpectedly modified** prior files and **0 unclassified new paths**.

Archive SHA-256: cf93495f77d4ec6e2e52ce538d38465f11c1d1797a0cf36d7d731dea857679e9. Archive size: 230,308,401 bytes. Every recorded baseline member hash and byte count was independently checked against that ZIP; no baseline inventory mismatch or duplicate archive member was found.

The JSON companion contains the complete path-to-reason expected_edit_allowlist, per-member before/observed hashes and sizes, relocation proof, new-path classification and metadata differences. It includes pending current release documentation edits explicitly, rather than silently treating future writes as preservation failures.

## Protected content

Counts below overlap: authoring files are also assets, and the final row includes every prior member outside the explicit edit allowlist. All nonempty protected groups were byte-identical.

| Group | Prior files | Exact content |
|---|---:|---:|
| runtime assets including recipes and generators | 24 | 24 |
| source data | 70 | 70 |
| configuration examples | 9 | 9 |
| sdk examples | 14 | 14 |
| authoring sources and artifacts | 10 | 10 |
| historical verification evidence | 1,510 | 1,510 |
| historical screenshots | 314 | 314 |
| versioned root documents and receipts | 120 | 120 |
| licenses and notices | 3 | 3 |
| all prior members outside explicit edit allowlist | 2,319 | 2,319 |

## Exact retained originals

| Delivered path | Retained path | SHA-256 matches |
|---|---|---|
| docs/PRODUCT_REVIEW.md | docs/PRODUCT_REVIEW_v0.21.md | Yes |
| docs/VALIDATION.md | docs/VALIDATION_v0.21.md | Yes |
| DELIVERY_MANIFEST.json | docs/verification/v0.22-review/baseline-delivery-manifest.json | Yes |

The complete previous text of CHANGELOG.md and docs/RELEASE_HISTORY.md was also checked for verbatim retention in their current versions. Current results: CHANGELOG.md: True, docs/RELEASE_HISTORY.md: True.

## Filesystem metadata caveat

**669 working-tree permission modes differ from original ZIP metadata.** The observed normalization is old Unix 0600 to working-tree 0644 for historical evidence; content is unchanged. The integrator confirmed this came from extracting 0.21 and will preserve original ZIP modes in the final source packer, including explicitly relocated material. This audit does not call working-tree metadata exact. The JSON records every affected member and flags any other mode transition separately.

## Explicitly expected source and current-document changes

Each row names an exact path, with its dispatched work-package reason. New 0.22 verification files are separately confined to the new versioned evidence prefix. No asset, data, configuration, old screenshot, old receipt or historical versioned document is broadly allowlisted.

| Path | Reason | Observed |
|---|---|---|
| CHANGELOG.md | WP08-WP09: append the completed 0.22 increment and its verified limits; retain earlier release history. | exact |
| Cargo.lock | WP08: keep workspace package versions aligned with 0.22.0. | expected_edit |
| Cargo.toml | WP08: advance workspace release to 0.22.0 without changing the Rust MSRV. | expected_edit |
| DESIGN.md | WP01-WP06/WP09: describe authoritative actor context, bounded community lifecycle, active ecology observers and honest persistence contracts. | expected_edit |
| PRODUCT.md | WP08-WP09: update current product overview, completed increment and remaining goals. | expected_edit |
| README.md | WP08-WP09: current release entry point, player/integrator guidance and evidence links. | expected_edit |
| crates/terra-bevy/src/living.rs | WP04: exclude dormant-owned bodies from native ecology observers and exercise the real adapter. | expected_edit |
| crates/terra-bevy/src/network.rs | WP05: reject uncertain multiplayer identity publication before connection admission; focused regression. | expected_edit |
| crates/terra-bevy/src/recovery.rs | WP05: structural published/uncertain recovery outcomes and checked payload/final-marker synchronization. | expected_edit |
| crates/terra-bevy/src/recovery_tests.rs | WP05: prove retained complete outputs after final-marker uncertainty and unchanged originals. | expected_edit |
| crates/terra-bevy/src/sandbox.rs | WP05: use the shared private publisher for native snapshots without erasing existing validation. | expected_edit |
| crates/terra-demo/src/actor_summary.rs | WP01: new bounded read-only actor/needs/consumption-target resolver with focused regressions. | new |
| crates/terra-demo/src/command_input.rs | WP09: native capture57 exposed Confirm-first management entry; prefer Cancel only, defer while unavailable, and preserve activation consumption, deliberate navigation, expiry and return-to-origin with actual F6 regression. | authorized after observation; final comparison pending |
| crates/terra-demo/src/communities.rs | WP03/WP09: reviewed unused-faction dissolution, exact displayed-review token binding, Cancel-first layout and regressions. | expected_edit |
| crates/terra-demo/src/control.rs | WP02/WP09: remove obsolete RestSelected intent after direct owned-ID Rest dispatch and fix screenshot48 selection toast using actual primary/current principal ownership; command targets unchanged. | authorized after observation; final comparison pending |
| crates/terra-demo/src/flow.rs | WP05-WP06: honest save/exit disposition and managed lease lifetime across final metadata persistence. | expected_edit |
| crates/terra-demo/src/interaction.rs | WP01-WP02: bind consumption to the displayed active body and snapshot owned group-order targets at dispatch. | expected_edit |
| crates/terra-demo/src/interface.rs | WP01-WP03/WP09: actor context, readable labels, displayed target/confirmation binding and actual native system regressions. | expected_edit |
| crates/terra-demo/src/launcher/model.rs | WP05-WP06: shared metadata publication, checked parent creation and cooperative managed-world lease. | expected_edit |
| crates/terra-demo/src/launcher/recovery.rs | WP05: preserve published inner recovery output across wrapper errors and flush new managed parents. | expected_edit |
| crates/terra-demo/src/launcher/recovery_tests.rs | WP05: nested marker/new-parent fault regressions and retained original-state checks. | expected_edit |
| crates/terra-demo/src/main.rs | WP01/WP06: register actor-summary module and retain managed lease through native session ownership. | expected_edit |
| crates/terra-demo/src/output.rs | WP06: protect the stable managed lease path and aliases from unrelated outputs. | expected_edit |
| crates/terra-demo/src/telemetry_capture.rs | WP05-WP06: checked telemetry publication and managed-path protection. | expected_edit |
| crates/terra-net/README.md | WP03/WP08: document matched 0.22 protocol 11 clients and hosts. | expected_edit |
| crates/terra-net/src/lib.rs | WP05: propagate structural publication errors through the host API. | expected_edit |
| crates/terra-net/src/protocol.rs | WP03/WP08: deliberately advance wire protocol to 11 for the exhaustive DissolveFaction command. | expected_edit |
| crates/terra-net/src/server.rs | WP05: host snapshots use the shared private publisher while retaining complete-state validation. | expected_edit |
| crates/terra-net/src/session.rs | WP05: private resume identity publication has explicit durability outcomes and tests. | expected_edit |
| crates/terra-net/tests/faction_lifecycle_tls.rs | WP03: authenticated dissolution, private views, replay refusal, identity and sequence save/resume coverage. | expected_edit |
| crates/terra-persistence/src/lib.rs | WP05: propagate structural publication errors through persistence APIs. | expected_edit |
| crates/terra-persistence/src/operations.rs | WP05: publish verified SQLite backups/exports through the shared contract and check newly created ancestors. | expected_edit |
| crates/terra-persistence/src/tests.rs | WP05: reopen uncertain backup output and verify exact checkpoint/original conservation. | expected_edit |
| crates/terra-sandbox/src/commands.rs | WP03: add the explicit authoritative DissolveFaction command. | expected_edit |
| crates/terra-sandbox/src/factions.rs | WP03: read-only eligibility and sole-leader asset-free dissolution; preserve all other domain values. | expected_edit |
| crates/terra-sandbox/src/lib.rs | WP05: expose shared publication module and structured SandboxError conversion. | expected_edit |
| crates/terra-sandbox/src/persistence.rs | WP05: preserve validation/size bounds and use explicit checked snapshot publication. | expected_edit |
| crates/terra-sandbox/src/publication.rs | WP05: new shared private publication, checked parent/tree synchronization and fault tests. | new |
| crates/terra-sandbox/tests/faction_dissolution.rs | WP03: new domain, cold-state merge, conservation, refusal and save/load acceptance. | new |
| crates/terra-sandbox/tests/faction_lifecycle.rs | WP03: wire round-trip includes the new exhaustive command variant. | expected_edit |
| docs/COMPATIBILITY.md | WP03/WP06/WP08: current version/protocol/source compatibility and cooperative lease limits. | expected_edit |
| docs/CONTROLS.md | WP02: explain actor targeting, existing selection shortcuts and needs context. | expected_edit |
| docs/DESKTOP_DELIVERY.md | WP07-WP08: independent extracted-package verifier, trust boundary and release instructions. | expected_edit |
| docs/GAMEPLAY.md | WP01-WP03: accurate actor/consumption targeting, unused-community policy and bound review behavior. | expected_edit |
| docs/GAME_FLOW.md | WP01-WP03/WP05-WP06: current native journey, readable context, review and progress-safety behavior. | expected_edit |
| docs/IMPLEMENTATION_PLAN_v0.22.md | WP00-WP09: new researched dispatch plan and explicit closeout status. | new |
| docs/INTEGRATION.md | WP03-WP06/WP08: current command/observer/publication/lease integration contracts. | expected_edit |
| docs/LIVING_WORLD.md | WP01/WP04: truthful actor needs context and native active-observer policy. | expected_edit |
| docs/MULTIPLAYER.md | WP03/WP08: protocol 11 and matched-client authority expectations. | expected_edit |
| docs/NETWORK_REPLICATION.md | WP03/WP08: protocol 11 and explicit prior-wire incompatibility. | expected_edit |
| docs/OPERATIONS.md | WP05-WP06/WP08: checked publication, recovery guidance, cooperative ownership and current protocol. | expected_edit |
| docs/PLAYTESTING.md | WP08-WP09: current actor/community/save acceptance routes and remaining qualification. | exact |
| docs/PRODUCT_REVIEW.md | WP08-WP09: current 0.22 all-perspective synthesis; exact old document is retained separately. | expected_edit |
| docs/PRODUCT_REVIEW_v0.21.md | WP08: exact byte-preserved copy of the delivered 0.21 current product review. | new |
| docs/RECOVERY_BUNDLES.md | WP05: explain published uncertainty, nested final markers, checked parents and retained output. | expected_edit |
| docs/REGIONAL_PERSISTENCE.md | WP05: verified committed-snapshot publication and retained uncertain outputs. | expected_edit |
| docs/RELEASE_HISTORY.md | WP08-WP09: retain previous history and append missing/current increment summaries. | expected_edit |
| docs/RESEARCH_v0.22.md | WP00-WP09: new primary-source research synthesis and applied decisions. | new |
| docs/REVIEW_GAPS_v0.22.md | WP09: new complete gap register with scoped closures and remaining owners. | new |
| docs/SETTLEMENTS_AND_ECONOMY.md | WP03: no-assets dissolution, global/cold reference proof and conservation scope. | expected_edit |
| docs/VALIDATION.md | WP08-WP09: current execution/evidence receipt; exact old document is retained separately. | exact |
| docs/VALIDATION_v0.21.md | WP08: exact byte-preserved copy of the delivered 0.21 current validation receipt. | new |
| scripts/desktop-payload.json | WP07-WP08: include independent verifier and new current/historical documentation without dropping prior runtime content. | expected_edit |
| scripts/package_desktop.py | WP07: reuse portable-path/architecture admission and ship standalone verification instructions. | expected_edit |
| scripts/test_package_desktop.py | WP07: bounded package-verifier integration regressions if the current increment requires them. | exact |
| scripts/test_verify_desktop.py | WP07: new independent verifier tamper, alias, inventory, executable and smoke tests. | new |
| scripts/verify_desktop.py | WP07: new standard-library extracted-package consistency verifier. | new |

## Scope and follow-up

Observation window: 2026-10-29T20:53:47.524287+00:00 to 2026-10-29T20:53:49.251889+00:00. No baseline file changed during its individual hash read. Files may have changed after that observation because authors and tests remain active.

The report verifies preservation and expected edit scope. It does not certify semantic correctness of an allowlisted source change, treat new test logs as passes, or qualify hardware, Windows, WAN or power-loss behavior. The source archive must be assembled after implementation and evidence are frozen, then independently compared again with this allowlist and the original archive modes.

Unexpected modified paths: none.
Missing prior paths: none.
Unclassified additions: none.

## Authorized scope amendment after this observation

The integrator authorized a bounded native correction after capture 57 showed
that F6 focused Confirm despite Cancel-first visual order. The exact
crates/terra-demo/src/command_input.rs path is now allowlisted for Cancel-only
new-review handoff, delayed readiness and actual F6 focus regression. Original
per-file observation hashes remain unchanged; this amendment does not pretend
the later source was already audited. Final archive comparison is still required.

The same consolidated batch extends the existing control.rs allowance to the
obsolete RestSelected intent removal after direct owned-ID dispatch and the
capture 48 primary-selection/ownership toast correction. Command targets are
unchanged; final archive verification must check the complete amended source.
