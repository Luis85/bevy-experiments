# Terra World 0.22 — researched improvement and polishing plan

Baseline: the delivered **0.21.0** source archive, SHA-256
`cf93495f77d4ec6e2e52ce538d38465f11c1d1797a0cf36d7d731dea857679e9`.
All 2,375 materialized source/evidence files matched that archive before edits;
its generated delivery manifest is retained as baseline metadata. Review and
dispatch began on **29 October 2026**. This plan is written before implementation
dispatch. Completion and evidence are recorded in the closeout section and
[Validation](VALIDATION.md), never inferred from this plan.

## Outcome and product constraints

Deliver the next usable Rust/Bevy framework increment and native reference game.
Keep the georeferenced world, authoritative simulation, finite economy, owned
bodies, source provenance and original forest-green presentation. Improve the
complete player journey and progress safety while retaining reproducible source,
runtime content, historical decisions and acceptance evidence.

The increment answers three concrete player questions: **who am I directing and
what do they need; how can I undo an unused community choice safely; and can I
trust that a save or recovered world is ready?** Integrators also receive an
independent way to inspect an extracted desktop package. No second quest ledger,
duplicate inventory, invented funding, automatic source migration or speculative
distributed rewrite is introduced.

## Review and ownership

Three dedicated specialists reviewed the existing product and researched primary
sources. The integrator reviews cross-module contracts, networking, scope,
release tooling, evidence and the actual native experience.

| Owner | Product perspectives | Review artifact |
|---|---|---|
| Player specialist | First/returning play, interaction, accessibility, controls, information hierarchy, feedback, comprehension | [Player experience](verification/v0.22-review/player-experience.md) |
| World/framework specialist | Domain authority, community lifecycle, finite economy, ecology, navigation, terrain/render boundaries, SDK extensibility | [World/framework](verification/v0.22-review/world-framework.md) |
| Reliability specialist | Persistence, recovery, privacy, concurrency, storage/platform contracts, operations and support | [Reliability/delivery](verification/v0.22-review/reliability-delivery.md) |
| Integrator | Product fit, cross-module consistency, transport/admission, deployment, QA, packaging, preservation, native evidence | Current review, research synthesis and validation |

The review distinguishes source findings, research guidance, actual execution,
rendered observations and remaining human/hardware qualification. Baseline native
captures come from this run of the accepted 0.21 binary, not historical images.
The complete G01–G18 register remains the coverage map; a scoped closure does not
claim the whole area is finished.

## Applied research

| Primary guidance | Decision applied in this increment |
|---|---|
| Microsoft Xbox Accessibility Guidelines 101, 103, 109, 112 and 114 | Keep current actor/goal context visible, name needs beyond color, make selection versus action targets explicit, preserve focus and wrap enlarged/long content. This does not assert XAG compliance. |
| Official Bevy fixed-timestep example | Keep physical/economic authority in its existing update policy; presentation and ecology-adapter fixes do not create a second clock. |
| PlayFab idempotent-transaction guidance | Preserve Terra's own sequence and reviewed-identity discipline; repeated or stale community intents must not target another entity or manufacture value. No PlayFab dependency is added. |
| Cargo SemVer guidance | Treat the new exhaustive command variant as an SDK source change. Advance the wire protocol deliberately while keeping unchanged stored schemas explicit. |
| Linux fsync documentation, Rust File and Microsoft file-lock/publication documentation | Separate visible publication from confirmed synchronization; use a retained nonblocking OS lock rather than a PID-file convention. Do not claim Unix semantics qualify Windows power-loss behavior. |
| SQLite WAL and online-backup documentation | Retain committed-generation validation and SQLite snapshot backup. Do not replace a supported backup with raw active-file copying. |
| GitHub security guidance | Keep least-privilege CI and distinguish local hash consistency from trusted publisher authentication; platform execution remains a separate proof. |

Exact source links and interpretation are in the specialist reports and the
release research synthesis. Research informs the design; regressions and native
inspection establish only the implemented and observed conditions.

## Dispatched implementation packages

### WP01 — truthful actor context

**Owner: player specialist.** Introduce a bounded read-only actor-summary model
shared by the HUD and Command inspector. A missing selection must never silently
fall back to a different active actor. Distinguish empty, one owned, inspect-only,
vehicle/nonbiological, multiple, missing and remote-received states.

For groups, label minima across selected owned biological bodies as **lowest**;
exclude vehicles and foreign bodies from that aggregate. Only associate avatar
needs with a vehicle when it is the exact owned, active, admitted possessed
vehicle. A single public foreign body may be inspected with explicit ownership
wording; no mutation authority is gained.

Acceptance: actual resolver and update-system tests cover those states, stale
IDs, pending possession, unrelated vehicles, missing avatars and partial views.
Text and bars agree. No source/save/wire mutation comes from this presentation.

### WP02 — HUD, action-target and accessibility polish

**Owner: player specialist.** Reuse the existing scaled footer, keeping loading,
storage failure, toast and input guidance priorities. Make actor/needs context
available during default Activities and immersive control. Preserve the shared
world-hit/viewport boundary. Keep the detailed Command context and explain that
consumption affects the active body whereas Rest/Stop concerns owned selection.

Add word-or-character wrapping to in-world labels, bound long compact names,
retain full names in inspection, and document existing X/Ctrl+A shortcuts.
Controls remain authoritative; no group-eating action is invented.

Acceptance: actual label/action/need-bar systems agree after context changes;
the main activity and confirmation remain reachable at 1100×700/150% and
1440×900. Preserve focus, modal ownership and text size. Native visual checks
use one batched inspection, one correction batch and at most one confirmation.

### WP03 — safe dissolution of an unused solo community

**Owner: world/framework specialist; interface integration coordinated with
player specialist.** Add a named `DissolveFaction` command and a shared readiness
query. Eligibility: active principal is the sole current member and Leader;
there is no faction-owned plot or settlement territory. Explicitly revoke its
outstanding invitations. Never refund invented funds, transfer assets, remove
another principal, or rewind IDs.

Global plots/territories cover valid cold property through existing ownership
validation. A remote client cannot infer complete eligibility from an absent
nearby asset: the server always rechecks authoritative state. The native review
names the exact faction and consequences, retains player/restore/roster context,
and supports cancel plus stale-review rejection.

Acceptance: successful empty-solo lifecycle and slot reuse; no ID reuse; outsider,
invitee, dormant, multi-member, unknown and repeated operations reject safely;
far plots/territories and cold shared property block; wallets, goods, stock and other communities
are conserved. Cover serialization/restore, regional retention, network command
authority and native confirmation. Wire **11** / ALPN **terra-world/11** is a
deliberate compatibility change; save **1**, regional **1**, asset **1** remain.

### WP04 — dormant ecology observer consistency

**Owner: world/framework specialist.** Prove the native/SDK discrepancy and
exclude dormant-owned bodies from native near-ecology observers, preserving
active-owner behavior and the server's connected-owner policy.

Acceptance: a headless adapter fixture demonstrates dormancy does not activate
or accelerate nearby ecology, active owners still do, and authored/background
simulation behavior is otherwise retained. Do not change saved positions,
population placement, rates or cold-region policy.

### WP05 — explicit durable-publication outcomes

**Owner: reliability specialist.** Add one dependency-light publication contract
at an existing shared layer. Distinguish **not published** from **published,
durability unconfirmed**. Keep private same-directory staging, exclusive temp
creation, file sync, replace/no-clobber intent, `.` for relative basenames and
explicit Unix parent sync. Do not add an external dependency merely for this.

Integrate domain/native saves, host saves, private session identity, launcher
metadata, backups/recovery and telemetry. Recovery wrappers must preserve final
visible validated artifacts after uncertain publication; only owned unpublished
temporary trees may be cleaned. Synchronize completed payload trees before the
final completion marker. Propagate the stage structurally, without parsing error
prose. User-facing copy must not report rollback or successful saving falsely.

Acceptance: real filesystem fault injection before/after the publication
boundary, both replace and no-clobber, prior-byte preservation, exact new-byte
retention, 0600 under isolated permissive umask, relative paths and collisions.
Nested recovery faults retain independently verifiable final artifacts. Native
save-and-exit and network identity admission do not claim success on uncertainty.
Name the narrower non-Unix contract and physical power-loss qualification gap.

### WP06 — one cooperative writer per managed world

**Owner: reliability specialist.** Claim a stable private per-profile file with
`File::try_lock` before initialization and hold its handle through game shutdown
and final metadata persistence. Reject a concurrent managed session promptly.
Never unlink the lock to unlock it; an unlocked existing file is not stale
ownership. Keep the 0.21 witness's separate initialization-continuity purpose.

Protect the lease path, its aliases and ancestors from screenshot/political/
telemetry outputs at preflight and actual publication. Preserve private files
and existing first-start semantics. Older executables/direct SDK writers and
arbitrary external replacement do not participate in this cooperative protocol.

Acceptance: real subprocess contention, separate-world concurrency, clean and
abrupt holder release, repeated open, no progress/profile/witness mutation on
contention, output protection and ownership through final metadata save. Windows
API mapping is research, not a substitute for executing these tests on Windows.

### WP07 — independent desktop-package inspection

**Owner: integrator.** Ship a standard-library-only extracted-package verifier,
independent of Cargo and the source checkout. Validate portable/unique paths,
regular non-symlink files, exact inventory/hashes, manifest and accepted binary
association, and supported architecture. Offer an explicit finite `--help`
smoke check on the matching host, separate from passive verification.

Acceptance: genuine extracted payload passes; corrupted/missing/unlisted/aliased
paths, unsafe names and mismatched executable/receipt fail; passive verification
does not run a binary; smoke failure/timeout is not reported as success. Include
tool and instructions in the curated payload, exercise them on the final Linux
archive, and retain Windows-specific execution as an open gate. A self-contained
manifest establishes consistency, not publisher authenticity.

### WP08 — compatibility and release integration

**Owner: integrator.** Advance all workspace packages to **0.22.0**, protocol to
11 only for WP03, and update current compatibility/networking/delivery guides.
Preserve external dependency versions unless a concrete finding requires one.
Wire rejection stays explicit, TLS has no plaintext fallback, source/catalog
identity remains mandatory, and existing JSON/regional worlds remain schema 1.

Keep the exact canonical gate roster unless a new check needs explicit roster
admission. Register new meaningful Python tests in the existing tooling gate.
Run focused checks after stable owner edits, then one fresh canonical acceptance
from frozen implementation inputs, with immutable terminal receipt and exact
source/log/report/binary identities. Resolve real failures before finalization.

### WP09 — cross-review, complete native journey and final polish

**Owners: all, orchestrated by integrator.** Rotate specialists across boundaries:
player reviews community consequences; world reviews durability/value-preserving
cleanup; reliability reviews package admission and player target ownership.
Root integrates findings, including error and recovery behavior.

Capture actual native baseline and improved RTS/Activities, selected group,
empty/inspect-only, Command, possession, community review, pause, save and reopen.
Use isolated test worlds. Check 1100×700 at 150% plus reference window. Inspect
captured images before accepting them. Correct material findings together,
rebuild, confirm, and stop optional polishing once accepted. Software-rendered
Linux evidence does not certify GPUs, physical devices or Windows.

### WP10 — product closeout and preserved handoff

**Owner: integrator with owner doc contributions.** Update README, PRODUCT,
CHANGELOG, current review, full G01–G18 register, validation and affected guides.
Archive the exact prior current review/validation; preserve original assets,
source data, authoring inputs, history and receipts. Report implemented closure,
evidence limits and next priorities separately.

Deliver complete source ZIP, accepted Linux desktop ZIP, research/review, this
plan, current gaps, validation and archive verification. Bind archive inventory
and native binary to the fresh terminal receipt, independently read back archive
bytes/modes, and save the deliverables. A Windows binary must never be labeled
accepted without its own executed platform evidence.

## Sequence and change control

1. Preserve and verify 0.21 baseline; review/research in parallel; capture current
   native evidence; record this plan.
2. Dispatch WP01–02, WP03–04 and WP05–06 to their specialist owners; root executes
   WP07–08 integration. Distinct-file ownership is the default. Shared main/lib/
   interface/server touch points are coordinated by narrow changes.
3. Owners run meaningful focused regressions in a coordinated build slot and
   record raw results. Root schedules Cargo work to avoid competing builds.
4. Cross-review every package, resolve findings, run native batched inspection
   and complete the single correction/confirmation round.
5. Freeze implementation inputs; run canonical acceptance. Documentation may
   record results afterward; any implementation change requires appropriate
   fresh acceptance. Package only the final accepted binary/evidence.
6. Verify source preservation and package extraction; complete docs and deliver.

If investigation disproves a candidate, record the evidence and remove that
change rather than manufacturing work. A newly exposed material correctness
defect may expand a package with recorded rationale and acceptance criteria.
All selected packages must complete before claiming the next increment shipped.

## Explicit remaining qualification

Physical Windows/GPU/device/assistive testing, uncoached long careers, production
art/audio and licensed regional data, populated-community dissolution/succession,
content migration, signed updates, Internet accounts/moderation, WAN/soaks,
storage tail budgets, offsite RPO/RTO and distributed authority remain distinct
acceptance work. Maintain all G01–G18 entries and concrete next proof. This is a
substantial framework increment, not a claim of finished AAA/MMORPG production.

## Closeout

**Implementation, peer review, bounded native confirmation and canonical
acceptance are complete; WP01–WP09 are accepted.** The separate
[delivery record](verification/v0.22-delivery/README.md) and paired source-verification
JSON record artifact acceptance outcomes.

The successful [terminal receipt](verification/v0.22-release-acceptance/completed-gates.json)
covers all **44 canonical gates**, **1,043 distinct Rust tests**, **1,144 Rust test
executions** and **46 Python tests**, with **zero failed or ignored tests and zero
timeouts**. The run began **29 October 2026 at 22:04:10 UTC** and finished at
**22:15:38 UTC**; all **378 implementation inputs**, published reports and logs
remained unchanged. Receipt SHA-256:
`7992f2fb0821faaa1f72d96f0d5b69f0fea18cd3acbc593214109dc1db893085`.

| Package | Status and delivered result | Accepted evidence / artifact record |
|---|---|---|
| **WP01 — actor context** | Accepted. Shared read-only summaries distinguish owned, inspect-only, empty, unavailable, group, vehicle and admitted-driver context. Biological group minima and partial views are explicit. | [Player implementation and resolver/system review](verification/v0.22-review/player-experience.md); native group, empty and driver captures recorded in the [native evidence](verification/v0.22-native/README.md). |
| **WP02 — HUD and action targets** | Accepted. Activities and immersive views retain actor needs; consumption binds displayed identity and rechecks current eligibility; Stop/Rest retain explicit owned IDs. The native ownership-toast correction is confirmed at 1100×700/150%. | [Final native confirmation](verification/v0.22-review/player-experience.md#final-native-confirmation--passed-within-the-observed-scope); accepted demo gate [227 passed](verification/v0.22-release-acceptance/demo.log). |
| **WP03 — unused solo community** | Accepted. Named dissolution rechecks sole-leader membership and global property, revokes invitations, preserves value and monotonic IDs, and rejects stale reviews. Native review enters on Cancel and permits deliberate confirmation. | [Domain/transport implementation and native lifecycle](verification/v0.22-review/world-framework.md); [independent community review](verification/v0.22-review/world-cross-review.md); accepted [headless gate](verification/v0.22-release-acceptance/headless.log). |
| **WP04 — dormant ecology** | Accepted. Native observers exclude dormant owners; resuming an owner restores eligibility without changing body retention or simulation rates. | Actual `advance_population` dormancy/resumption regression is included in the accepted [140-test Bevy render gate](verification/v0.22-release-acceptance/bevy-render.log). |
| **WP05 — publication outcomes** | Accepted. Shared typed publication stages, checked new-parent synchronization and nested recovery retention reach save, identity, backup, metadata and diagnostics paths. Uncertain publication cannot acknowledge successful native exit or identity admission. | [Reliability implementation/fault evidence](verification/v0.22-review/reliability-delivery.md); [ancestor-publication peer finding and closure](verification/v0.22-review/reliability-cross-review.md); accepted headless, Bevy and demo gates. |
| **WP06 — managed-world ownership** | Accepted. A retained OS lease covers initialization through final metadata; contention and output aliases are rejected while the independent first-start witness is preserved. | [Lease/process regression evidence](verification/v0.22-review/reliability-delivery.md); accepted demo gate and native [closed-world save comparison](verification/v0.22-native/confirmation-managed-save-evidence.json). |
| **WP07 — desktop inspection** | Accepted implementation and peer correction. Passive source-free inspection checks exact payload and receipt association; explicit bounded smoke mode remains separate. Hard-link, bounded-JSON and strict-counter findings are closed. | [Independent verifier review](verification/v0.22-review/delivery-cross-review.md); accepted [46-test Python gate](verification/v0.22-release-acceptance/desktop-packaging-tests.log). Actual extracted-package outcomes are recorded under WP10. |
| **WP08 — release integration** | Accepted. Workspace 0.22.0, wire/ALPN 11 and unchanged stored-schema versions are documented. The two stale test-only protocol expectations are corrected; all canonical gates passed. | [Compatibility contract](COMPATIBILITY.md); [exact test-only source review](verification/v0.22-review/test-only-source-review.md); successful [native/source association](verification/v0.22-native/confirmation-input-association.json). |
| **WP09 — cross-review and native polish** | Accepted. Specialists rotated across authority, publication, targeting and packaging. One consolidated native correction batch closed ownership feedback, default Cancel focus and singular community copy; independent confirmation observed create/cancel/confirm/save/reopen behavior. | [Native journey and scope](verification/v0.22-native/README.md); [clean confirmation process](verification/v0.22-native/confirmation-managed-reopen-process.json) and saved generation 60, empty factions, next ID 3 with unchanged wallet/inventory/witness/lease. |
| **WP10 — preserved handoff** | Documentation and packaging implementation complete; final archive outcomes are recorded in [delivery record](verification/v0.22-delivery/README.md) and paired source-verification JSON. | [Source-packer review](verification/v0.22-review/source-package-cross-review.md). Archive inventory, byte/mode readback, preservation, extraction and delivery evidence are recorded separately from implementation acceptance. |

The first canonical attempt is retained as a
[failed terminal receipt](verification/v0.22-final-acceptance/completed-gates.json),
with its [headless failure log](verification/v0.22-final-acceptance/headless.log).
The existing handshake fixture still expected protocol 10 after production had
advanced to 11. Only the two expected JSON literals inside `#[cfg(test)]` changed
from 10 to 11—**two bytes total**. Independent comparison confirms that every
production-prefix byte and all assertions remain intact. The
[focused fixture rerun passed](verification/v0.22-preflight/protocol-fixture-confirmation.log).
The failed run was neither reused nor relabeled; the fresh complete run has its
own directory and source identity.

A second attempt is retained at `verification/v0.22-confirmed-acceptance`.
It ended without a terminal receipt after 42 completed zero-exit gates. The
[interruption record](verification/v0.22-review/acceptance-interruption.json)
retains the observed progress/log/source identities and the unknown cause.
Its partial evidence cannot authorize packaging. All 44 gates were rerun and
passed in the fresh `v0.22-release-acceptance` directory without changing
implementation inputs.

The [final association check](verification/v0.22-native/confirmation-input-association.json)
passed: the canonical rebuild and the actually inspected native process share
executable SHA-256
`34dc6fe5c906d556fcf182968df0a92742079008c5eb26cc94d652a69a041cd6`.
The source inventory differs only by the independently reviewed two-byte test
correction; all production-prefix bytes and the other 377 inputs are unchanged.
This binds the retained native confirmation to the accepted executable without
relabeling either earlier run.

Consult the separate delivery record and paired verification JSON for actual
archive names, counts, hashes, complete byte/mode readback, preservation against
all 2,376 baseline members, extracted-package outcomes and delivery. The explicit
qualification limits above remain in force.
