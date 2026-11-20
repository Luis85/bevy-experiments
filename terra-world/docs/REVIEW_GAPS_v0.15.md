# Open gaps and improvement follow-up — 0.15

Review date: **2026-10-29** (Europe/Berlin). This increment follows the accepted
[0.14 gap register](REVIEW_GAPS_v0.14.md). Its
[product review](PRODUCT_REVIEW_v0.14.md) and
[validation record](VALIDATION_v0.14.md) are preserved byte-for-byte. The stable
**G01–G18** identifiers originate in the
[0.12 open-gaps report](OPEN_GAPS_REPORT_v0.12.md).

**The final automated and native checks passed:** 34 gates, 860 distinct Rust
tests, 929 passing executions, zero failed/ignored tests and 356 unchanged inputs.
Three native confirmation processes used the exact accepted executable and
exited cleanly. The [release record](release-verification-v0.15.json) binds the
[gate evidence](verification/v0.15-final/gates.json),
[native review](native-v15-verification.json),
[Linux package confirmation](desktop-package-v0.15.json) and
[preservation comparison](verification/v0.15-final/baseline-comparison.json).
These results establish the stated reference scope; the complete remaining-gap
register below remains the product work contract.

## Outcome and scope

This pass closes three concrete gaps in the regional reference experience:
autonomous vehicles can propose a bounded physical detour around a small group
of stopped bodies; artists have an original, animated compact-car example with
model-specific pack targeting; and players can back up, verify and recover
supported local menu worlds from the launcher. A cross-review and polishing
pass addresses stale paid actions, completed-result discoverability, competing
driving overlays and current documentation drift.

These additions use the existing economy, ownership, movement and persistence
contracts. Domain save version **1**, regional schema **1**, wire protocol
**10** and TLS **terra-world/10** remain unchanged. Asset schema remains **1**,
but the optional `vehicle_model` target and `vehicle_motion` policy require the
0.15 reader; older readers reject those new features. Existing kind/species
packs retain their normalized identity. See [Compatibility](COMPATIBILITY.md).

The product remains a regional SDK and playable reference game. This report
does not close broader production-art, career-balance, accessibility, hardware,
Windows, WAN, deployment or global-architecture work through a local example.
Each remaining condition appears in the full register below.

## Implemented improvements

### Physical detours around a small stopped group — G05

The existing one-body detour remains the first option. If another stopped body
blocks that corridor, a bounded fallback can consider a cluster of two to four
bodies. Corners remain within 64 metres; the complete reconnecting route remains
within 160 metres. At most 12 extra corridor admissions are shared across a
vehicle update's substeps and retries. All three legs, including the connection
back to the original goal, undergo the existing full-envelope terrain, traffic
and body-sweep checks. Moving, duplicate, unknown, self and excessive members
are rejected. The 129-waypoint limit, patrol behavior and ordinary handling and
arrival rules remain in force.

The regression uses a 60-metre journey blocked by three parked bodies on authored
flat SDK support. The 0.14 implementation advanced only 7.30 metres in 120
physical seconds. The revised case reaches its goal in 14.5 physical seconds
over 75.49 metres, with a maximum 0.800000002-metre step and 3.37-degree heading change.
The baseline uses unbounded flat support; the final fixture additionally bounds
the supported region. It verifies clear body sweeps, supported complete
envelopes, unchanged parked bodies, conserved goods and money, bounded source
work, a valid complete state and 143 exact resumed steps. These are fixture
measurements, not a dense-road, hardware-performance or profitable-company
claim.

Trace: [autonomous movement](../crates/terra-sandbox/src/living/autonomy.rs),
[contact planning](../crates/terra-sandbox/src/living/contacts.rs),
[cluster regressions](../crates/terra-sandbox/tests/navigation_clusters.rs),
[Contacts and routing](CONTACTS_AND_ROUTING.md) and
[Vehicle dynamics](VEHICLE_DYNAMICS.md).

### An original vehicle model and reproducible pack — G09

The optional Comet example contains an exterior, transparent windows, four empty
seats, dashboard, four wheels and front steering pivots. Its original GLB has
named `parked` and `rolling` clips. A checked-in Python generator reproduces the
model and associated example outputs; the authoring recipe composes Comet with
the five unchanged wildlife models through the existing validated publishing
path. The standalone pack targets the actual Compact model. The combined direct
manifest retains wildlife without copying or changing its original GLBs.

`VehicleModel` targeting takes precedence over the generic Vehicle binding.
Remote appearance can use the existing public model field without reading an
owner's private inventory. Wheel phase uses signed admitted travel, the declared
wheel radius and binding scale, so forward/reverse motion is reflected and a
stop retains phase. Steering follows authoritative manual dynamics or measured
heading change. Pause freezes clips, blending and the wheel rig. Rig hierarchy,
channel, scale and aggregate-work checks prevent unsupported transforms and
unbudgeted work. Procedural fallback remains visible until a replacement is
ready; first-person hides the exterior and retains the existing cockpit.

Garage now explains an imported fixed livery and prevents a paid native paint
command for that appearance. It checks the selected actual vehicle and rejects
stale displayed context before dispatch. Normal procedural paint still uses the
ordinary five-credit command. Gameplay paint state and ownership rules are not
changed merely because one client selects a presentation pack.

This is one representative original vehicle, not a complete production fleet.
Human/customization rigs, occupied seats, boarding, attachments, audio, broader
DCC presets, production LOD/material work and target-GPU qualification remain
open. The original wildlife pack remains the default.

Trace: [pack admission](../crates/terra-bevy/src/asset_pack.rs),
[runtime animation](../crates/terra-bevy/src/render_animation.rs),
[original Comet](../crates/terra-bevy/assets/transport-pack/README.md),
[recipe](../crates/terra-bevy/assets/transport-authoring.recipe.json),
[Garage](../crates/terra-demo/src/driving.rs),
[Authoring](ASSET_AUTHORING.md) and [Asset packs](ASSET_PACKS.md).

### Native backup, verified review and a new playable copy — G16

**Your worlds → Make backup** creates a supported local regional recovery set.
**Backups and recovery → Verify** checks the complete retained bundle before
opening a named review. **Recover as new world** restores into a new managed
directory and leaves opening that world as a separate action. This route remains
reachable when a damaged original profile is absent from Your worlds.

A private wrapper retains the world title, character customization, quality,
start hour, original creation time, checkpoint generation and expected bundle
identity. Its bounded canonical metadata lives outside the immutable backend
bundle. The same backend verifies and restores the exact database, catalog,
configuration, prepared-source and supported political inputs. The restored
profile is published only after the complete recovered world validates.
Deliberately politics-free worlds retain that setting.

One background worker handles creation, verification or restore. Browsing stays
available; another operation or world launch waits for completion. The browser
inspects at most 256 directories and shows at most 24 entries. The exact newly
completed backup or world is pinned within those bounds, so a full recent list
or a backwards system clock cannot hide the promised Verify/Open action.
Selection, listing, metadata and review identities are rechecked before a
confirmation can act. Failed work cannot publish an incomplete playable world
or overwrite an existing recovery set.

The managed-world rehearsal deletes its original world and input tree before
restoring generation 2. It checks the complete archive, two principals, living
bodies, distant resources, exact source/political data, character/presentation,
absolute rebased paths, discoverable profile, retained original backup and a
second backup of the recovered world. The larger backend recovery rehearsal
remains a separate gate. Synthetic resource extent does not establish terrain
coverage at every resource location.

This is local recovery for supported reference-host worlds. It does not provide
offsite storage, encryption, automatic retention, signing or service RPO/RTO.
Server, live, historical and custom-catalog worlds still need deliberate
dependency and metadata policies. Native wrappers and bare operator bundles
have distinct entry points, documented in the recovery guide.

Trace: [launcher recovery](../crates/terra-demo/src/launcher/recovery.rs),
[recovery regressions](../crates/terra-demo/src/launcher/recovery_tests.rs),
[launcher flow](../crates/terra-demo/src/launcher/mod.rs),
[Game flow](GAME_FLOW.md) and [Recovery bundles](RECOVERY_BUNDLES.md).

## Cross-review and polishing corrections

| Finding | Applied correction | Product consequence |
|---|---|---|
| A completed recovery could be dropped by a bounded rescan, particularly with 24 future-dated entries. | Retain and revalidate the exact completed world or backup, then pin it within the existing list bound. | The promised Open or Verify action remains reachable. |
| Imported fixed-livery models could still offer a paid paint change with no visible result. | Resolve the selected actual appearance, explain the limitation and reject stale paint context before producing the command. | The player is not charged for a change this client cannot display. |
| At 150% text, an open World drawer covered the driving card while fragments of its text protruded beside the panel. | Apply the existing route-map visibility rule to both floating driving cards while World, Inspector or Performance is open. | The panels no longer compete with a partially visible driving card. Actual ECS visibility regressions and final native confirmation pass at 1100 × 700 with 150% text. |
| Current guides omitted Company or the new recovery screens. | Reconcile the activity cycle, launcher navigation and design contract with code. | Instructions match the controls the player receives. |
| Historical measurements linked to a mutable current validation page. | Point 0.12 measurements and its incomplete-attempt anchor to the preserved 0.12 validation record. | A later release cannot silently change the evidence supporting an old claim. |
| Unchanged asset schema could be mistaken for backward support of new fields. | State the 0.15 reader requirement separately from gameplay and old-pack compatibility. | Developers can distinguish stable old packs from newly introduced optional features. |

Native interaction findings and final acceptance are recorded in the
[current product review](PRODUCT_REVIEW.md) and [validation](VALIDATION.md).

The final native review also retained a nonblocking usability finding: repeated
restores have distinct saved identities but the same visible title/subtitle.
The new copy is pinned and focused; clearer labels or renaming remain G16 work.
The accepted recovery comparison verified every database row/column and all
character/settings/source inputs before opening, then the recovered world
reached 49 ready terrain chunks. All 42 previous files remained unchanged.

## Complete remaining-gap register

“Scoped improvement” means the concrete behavior above is implemented and
verified within its recorded boundary. It does not mean the wider product area
is finished. Priority is conditional on the intended release and deployment.

| ID / product area | Current position | Remaining closure criteria |
|---|---|---|
| **G01 — Documentation** | Current guides reconciled with recovery, vehicle packs and historical evidence; release maintenance continues. | Keep compatibility, current-guide/link, artifact and claim checks in every release; preserve original historical values. |
| **G02 — Windows delivery** | Platform execution remains open. **P1 for Windows players.** | Run Windows CI and a clean-machine creation/play/save/reopen/recovery journey. Declare OS/GPU/runtime prerequisites and signing, installer, update and removal policy. |
| **G03 — Career progression** | Existing priced career cases retained; broader balance remains open. **P1 for a career release.** | Test additional beginner and advanced strategies, setbacks, living/labor/capital costs, treasury replenishment and longer sessions. Retain finite funding and managed-intervention limits. |
| **G04 — Continuing goals** | Company guidance retained; recovery provides a clearer continuation path. **P1 for sustained play.** | Observe uncoached first-session and returning-player Supply, Home/Community and Road/Company journeys; measure misunderstood costs, stalls and recovery success, then add sustained content where needed. |
| **G05 — Navigation/recovery** | Scoped improvement: bounded physical detours around a small stopped group. **P1 for denser regions.** | Exercise multiple moving obstacles, narrow roads, blocked junctions, alternate exits and missing/restored terrain. Require bounded physical progress or an explicit value-preserving recovery action across a declared geometry matrix. |
| **G06 — Community lifecycle** | Accepted departure/removal/handover behavior retained. **P1 for long-lived groups.** | Define unavailable-leader succession, dissolution, territory transfer/reclaim and treasury policy; verify offline permission and durable-value behavior. |
| **G07 — Rendering qualification** | Wheel/steering work joins admission budgets; hardware qualification remains open. **P1 for performance promises.** | Profile CPU/GPU frame tails, actual residency and loading peaks on chosen hardware. Budget direct game-owned overrides or require validated metadata. Import limits are not GPU measurements. |
| **G08 — Accessibility/devices** | Recovery focus, scrolling and 150% text plus the corrected driving overlays pass final native confirmation at 1100 × 700. Complete accessibility qualification remains open. | Qualify full keyboard/controller/wheel journeys, reconnect and device ordering, assistive access, contrast, motion comfort and localization with representative users. |
| **G09 — Production content** | Scoped improvement: original Comet exterior/empty cabin, model-specific targeting, motion-driven wheels and repeatable six-model authoring. | Produce human/customization rigs, occupied seats, boarding, attachments and audio; broaden DCC presets, licensing and production LOD/material budgets; qualify transitions and target devices. |
| **G10 — Regional data** | Original connected prepared inputs retained. | Select licensed shipping data and reproducible ingestion; resolve NoData, datum, topology, complex relations and bridge/underpass geometry; verify actual coverage and provenance. |
| **G11 — Trusted delivery** | Immutable local outputs and recovery identities remain integrity mechanisms. | Define signing/trust, release discovery, authenticated transfer, interruption/resume, installed-version selection and rollback. Hashes and author declarations do not authenticate a publisher. |
| **G12 — Content migration** | Recovery deliberately restores exact content; migration remains open. | Define ID/geometry/catalog conflicts affecting plots, resources, homes and obligations. Supply deliberate migrations with backups, rollback and exact validation. |
| **G13 — Public service controls** | Open. **Conditional P1 for Internet hosting.** | Provide account enrollment/recovery, authenticated operator access, durable moderation/audit policy and abuse controls; validate actual server capabilities and account recovery. |
| **G14 — WAN and capacity** | Finite local workloads remain the acceptance boundary. **Conditional P1.** | Measure sustained representative clients under delay/loss and TCP head-of-line stalls, plus long soaks; derive supported populations and frame/acknowledgement targets from those results. |
| **G15 — Storage stalls** | Native maintenance runs on one bounded worker; authority storage performance remains open. | Measure transition-pause distributions and tails; reduce clone/install/mutation stalls while preserving complete validation and durable acknowledgements. Final maintenance may still wait on storage. |
| **G16 — Operational recovery** | Scoped improvement: native verified backup/review/restore, exact character/settings handoff and reachable recovered worlds. **Conditional P1 for deployment.** | Distinguish repeatedly restored copies with a visible creation/checkpoint suffix or rename action; the exact new result is currently pinned/focused. Set retention, confidentiality, offsite copies, recovery owners and RPO/RTO; rehearse real transfer and reopening in the target deployment. Extend server/live/historical/custom-catalog policy deliberately. |
| **G17 — Observability** | Retained diagnostics and recovery errors remain available; no new external monitoring service. | Add deployment retention, crash diagnostics, external audit retention, actionable alerts and ownership; qualify failure detection instead of treating stale or absent samples as healthy. |
| **G18 — Global architecture** | Conditional expansion; no global-scale claim. | Establish real population/data requirements before authority partitions, cross-shard ownership/economy and adaptive content delivery. Regional paging remains a single-authority technique. |

## Recommended next increment

1. Observe uncoached first and returning sessions on the supported native build.
   Prioritize sustained goals, recovery comprehension and money/obligation
   explanations from those observations rather than inventing another progression
   layer without evidence.
2. Expand the autonomous geometry matrix to narrow roads, moving blockage and
   intersections, with complete envelope support, bounded work and retained
   obligations as acceptance conditions.
3. Produce one representative human/customization and seated/boarding handoff
   through the validated pipeline, then qualify its visibility, transitions,
   loading peaks and hardware budget.
4. Execute the clean-machine Windows and target-device journeys. If Internet
   hosting is the intended next release, public controls, WAN capacity and a
   deployment recovery rehearsal take precedence over broader world scale.

The [product review](PRODUCT_REVIEW.md), [validation](VALIDATION.md),
[desktop delivery guide](DESKTOP_DELIVERY.md) and original numbered register
provide the supporting contract and evidence boundaries.
