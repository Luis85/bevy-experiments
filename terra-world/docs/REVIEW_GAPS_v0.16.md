# Improvement and open gaps report — 0.16

Review date: **2026-10-29**. This increment follows the accepted
[0.15 gap register](REVIEW_GAPS_v0.15.md). The previous
[product review](PRODUCT_REVIEW_v0.15.md) and
[validation](VALIDATION_v0.15.md) remain preserved byte-for-byte. Stable
**G01–G18** identifiers originate in the
[0.12 open-gaps report](OPEN_GAPS_REPORT_v0.12.md).

**Automated and native acceptance passed.** All **43 final gates** passed on **366 unchanged implementation inputs**, with **904 distinct Rust tests**, **990 passing test executions**, zero failed and zero ignored. Four native confirmations used the exact accepted executable and exited 0. [Validation](VALIDATION.md) records the commands, measurements and limits; [native](native-v16-verification.json), [desktop](desktop-package-v0.16.json) and [release](release-verification-v0.16.json) receipts bind the completed results.

## What this increment changes

This pass addresses four concrete problems in the playable regional reference:
remembering accepted work and its costs, recovering physically from constrained
vehicle routes, carrying character customization into an animated model, and
distinguishing recovered copies of the same world. A separate subagent reviews
each implementation before integration. Native interaction and delivery review
then exercise their combined behavior.

The implementation uses the existing finite economy, ownership, physical
movement and persistence rules. Domain save version **1**, regional schema
**1**, wire protocol **10** and TLS **terra-world/10** remain unchanged. Asset
schema remains **1**, but the optional human animation and driver-seat fields
require a **0.16 reader**. Absent fields retain older normalized pack identities.
World names use existing profile metadata. See [Compatibility](COMPATIBILITY.md).

This is a regional SDK and reference game. The broader product conditions in
the complete register remain open where the available evidence does not meet
them. A local example, agent-operated play review or finite fixture cannot
establish uncoached usability, career balance, hardware performance, public
deployment readiness or global scale.

## Implemented improvements

### Explicit goals and continuing commitments — G03 / G04

Players can choose directly among the six existing goals. The expandable
chooser avoids stepping through unrelated goals to reach the desired one.
The owner-specific **Your commitments** briefing gathers accepted supply work,
personal funds, road assignments, company cash and loaded obligations, and
rental bonds. Its entries respect the existing bounds and do not expose another
owner's private obligations. The briefing is derived from authoritative state;
it creates no second saved progression ledger or reward source.

An explicitly selected accepted supply request retains priority. Otherwise the
next accepted request is the earliest world-clock deadline, with stable ID as
the tie-breaker. Review opens the exact displayed request as a separate action
from travel and transactions. Guidance and noticeboard actions compare the
full displayed request, principal and restore context before acting. A second
queued action waits for a refreshed display after selection or review changes.

Supply advice compares the gross reward with the current price of all missing
units from a seller that stocks that quantity. It shows the missing-input shortfall and
whether the personal wallet can fund it. Existing carried goods and travel are
not assigned invented costs, and the text makes clear that the comparison is neither a
profit promise nor a reservation. Deadlines use hours, minutes or seconds as
appropriate; expired work cannot be delivered through the UI and retains the
existing value-preserving return action.

The deterministic continuation journey accepts three requests, explicitly
returns one, lets one expire, and physically completes one paid delivery. It
rebuilds guidance after resume and checks the obligations and ownership rules.
Its 86,401 seconds of calendar advancement are separate from 70.8 physical
seconds of walking. This tests continuity through a declared world-day window;
it is not a claim of a full day of human play or balanced sustained progression.

Trace: [guidance](../crates/terra-sandbox/src/guidance.rs),
[commitments](../crates/terra-sandbox/src/guidance/commitments.rs),
[continuation regression](../crates/terra-sandbox/tests/guidance_commitments.rs),
[native handoff](../crates/terra-demo/src/frontier.rs),
[player guide](GUIDANCE.md) and
[independent review](verification/v0.16-review/guidance-cross-review.md).

### Bounded recovery through constrained terrain — G05

The previous release could repeatedly oscillate behind a parallel parked
reference vehicle on a 9.5-metre supported strip. The new fallback uses the body's actual lateral
extent and admits the full proposed corridor. A late approach can reverse
physically before taking the two corners and reconnecting to the route. Source
support, traffic and body sweeps cover every new narrow/cluster leg, including
the reconnection. A failed proposal preserves the original route and value.

The additional admission budget remains **12 per vehicle update**, shared
across substeps and retries. Accepted maneuvers can reclaim consumed Move path
prefixes while keeping unconsumed work and the destination. Patrol continuation
and exact resume remain covered. The legacy one-body route also checks the full
source envelope of its reconnection, with a **160-metre** limit before querying
source support. The complete reconnecting envelope is divided into source calls
bounded by 64 metres of translation plus rotation (0.1 metre tolerance), each
charged to the shared twelve-call allowance. All segments must pass before any
route change; this preserves the existing callback budget on longer career trips. Moving, routed and crosswise obstacles cannot seed the new
narrow proposal. The legacy maneuver's future traffic/body occupancy remains checked during each
physical step; pre-admitting all future occupancy changed established courier
behavior and was not adopted.

The declared geometry matrix includes a parked body on a narrow corridor,
temporarily unavailable then restored flank support, unavailable reconnection,
two moving crossings and a blocked narrow junction. It checks physical arrival,
motion bounds, supported complete envelopes, clear sweeps, conserved goods and
money, bounded source work, complete-state validity and exact resumed steps.
It does not establish general dense-road throughput or arbitrary intersection
handling. The original loaded-courier rear-trap and occupied-exit records remain
separate acceptance cases.

These are authored flat SDK cases with a frozen calendar. The blocked junction
preserves the waiting route and permits an unrelated exit; an explicit new Move
order then moves the blocker physically, allowing the original queue to resume.
The 600-second unavailable-flank case includes repeated safe clearances before
support returns, rather than a stationary wait. Conservation covers inventories
and pocket money in these fixtures, not operating fuel costs or profitability.
Unsupported rear approaches, insufficient turning room, exhausted route capacity
and reconnections beyond the local bound can still hold an order.

Trace: [autonomy](../crates/terra-sandbox/src/living/autonomy.rs),
[contact admission](../crates/terra-sandbox/src/living/contacts.rs),
[geometry regressions](../crates/terra-sandbox/tests/navigation_geometry.rs),
[Contacts and routing](CONTACTS_AND_ROUTING.md),
[Vehicle dynamics](VEHICLE_DYNAMICS.md) and
[implementation review](verification/v0.16-review/navigation-implementation.md).

### A customized animated person and an explicit driver pose — G09

The original **Fieldhand** example provides idle, walking, running and seated
clips. All three saved body styles and the existing skin, hair and outfit
choices retain their meanings. Three distinct, validated material roles receive
per-instance colors; one actor cannot recolor another through a shared source
material. Profile changes reuse those handles and scene teardown releases them.
Actual admitted movement drives locomotion. Pause freezes animation and blend;
an actual occupancy change applies a discrete standing/seated handoff.

The importer validates the full admitted hierarchy and every clip, including
unused clips. Material roles must be present, unique and used, and cannot carry
base-color textures, vertex colors or emissive presentation that would defeat
the declared customization. Runtime readiness checks the actual role primitives
before replacing the visible procedural fallback. Admission includes seated
animation work and an estimated **12 KiB** material allowance per human; this is
a work estimate, not a GPU measurement.

An explicit driver-seat point belongs to a particular vehicle-model binding.
The reference adapter seats only a known manually controlling owner whose
registered active vehicle and avatar agree. Public remote appearance is not
treated as occupancy information. The visual follows the interpolated,
terrain-aligned vehicle without moving the saved authoritative avatar. First
person hides the rider and exterior. Exit and control release retain existing
authority semantics; RTS/autopilot does not gain persistent physical occupancy.

While the selected cabin replacement is deferred or failed, the procedural car
stays visible and its rider is hidden: that older closed car does not fit the
seated human. Once the cabin is ready, the customized procedural human remains
visible until its own imported replacement is ready. This makes vehicle and
human readiness separate, explicit presentation decisions.

The composed example uses five unchanged wildlife models, Fieldhand and an
original Comet cabin variant. The variant creates room for the seated human
while preserving the earlier Comet input exactly. A standard-library generator
reproduces five outputs; the seven-model authoring recipe exercises immutable,
content-identified publication and subsequent validation. The existing woodland
pack remains the default.

Independent geometry review found and corrected roof, floor, opaque-shell and
hand-placement conflicts across the body styles. Final native review separately
confirmed the visible fit, first-person hiding and vehicle handoff for one saved
appearance. The larger restored scene reached its 64 imported-instance limit:
nine other eligible actors and 18 distant actors retained procedural presentation.
Selected and reserved work remained within every declared limit; no import,
runtime or persistence error was reported. This is an articulated node model,
with fixed hair geometry. It is
not a humanoid retargeting workstation, a complete production catalog or a
physical/timed boarding system.

Trace: [original character example](../crates/terra-bevy/assets/character-pack/README.md),
[pack admission](../crates/terra-bevy/src/asset_pack.rs),
[runtime character presentation](../crates/terra-bevy/src/render_character.rs),
[authoring](ASSET_AUTHORING.md), [asset contract](ASSET_PACKS.md) and
[implementation review](verification/v0.16-review/content-implementation.md).

### Names for recovered copies and existing worlds — G16

Verified recovery review includes an editable **Recovered world name** before
the background restore begins. The progress message starts with that selected,
trimmed name and then identifies the source backup/checkpoint. Restore still
creates a separate managed directory, publishes only a complete validated
profile, and leaves opening the result as a separate player action.

**Your worlds → Rename** changes the selected profile title. It rechecks the
complete displayed profile, selection revision and direct managed-world path
before atomic replacement. Invalid names, stale/cancelled actions, symlink and
hardlink aliases and external paths are rejected. Checkpoints, backups, source
inputs, character choices and creation/last-played times remain exact. Renaming
an unplayed world does not invent a checkpoint. The result uses the existing
bounded list pin/focus so the named world remains reachable.

World entries also show creation time to seconds in UTC. Duplicate names and
equal timestamps are allowed: the managed directory remains identity. The
regression deliberately creates equal timestamps, restores two copies with
different names, renames one, compares the whole retained tree and confirms
that only the intended `world.json` title changed. Fresh relisting preserves
the names without relying on the temporary pin.

Trace: [launcher](../crates/terra-demo/src/launcher/mod.rs),
[recovery](../crates/terra-demo/src/launcher/recovery.rs),
[regressions](../crates/terra-demo/src/launcher/recovery_tests.rs),
[recovery guide](RECOVERY_BUNDLES.md), [game flow](GAME_FLOW.md) and
[independent review](verification/v0.16-review/recovery-cross-review.md).

## Cross-review and polishing findings

| Finding | Applied correction | Product consequence |
|---|---|---|
| A queued selection change could retarget the next accepted-request review before its new label appeared. | Invalidate the commitments snapshot when selection changes; regress both queued action orders. | Review remains tied to what the player actually saw. |
| A narrow-road car could oscillate instead of passing a parked body. | Admit a bounded route based on the real lateral body extent, including a reverse approach and rejoin. | The declared narrow-road case can make physical progress while preserving support and values. |
| Full release tests found two warehouse careers stalled after six deliveries: a reconnection fit the 160 m route bound but exceeded one terrain callback's existing work limit. | Validate the complete envelope in bounded segments, charging each to the same allowance; reject the whole proposal if any segment fails. | Longer existing routes retain physical progress without weakening terrain checks or career assertions. |
| A source-only legacy rejoin check could query an arbitrarily distant next waypoint. | Reject reconnections over 160 metres before invoking the source callback. | The added validation cannot turn a large route segment into unbounded source work. |
| Human standing proportions and the original car shell did not provide a usable seated cabin. | Add a separate original cabin variant, correct seated joints and inspect actual transformed geometry for all three styles. | The example demonstrates a deliberate model-to-vehicle fit while preserving the prior asset. |
| Human ground offset could shift a seated hip away from the seat anchor. | Rebase seated posture around the declared hip; retain ground offset for standing presentation. | Imported and fallback occupancy agree on the seat location. |
| The original procedural car could not fit the seated rider during cabin loading or failure. | Keep the car fallback visible and defer its rider until the cabin replacement is active; then allow the human fallback independently. | Loading cannot expose a person through the closed car's roof or floor. |
| Imported material colors interpreted existing sRGB palette values as linear light. | Use the same sRGB-to-linear conversion as procedural mesh vertices, and compare actual fallback colors in the regression. | The visible model replacement retains the selected colors. |
| Emissive customization materials could wash out selected skin, hair or outfit colors. | Reject those role policies during admission. | The three declared color roles remain meaningful. |
| A paused newly loaded human could remain in a rest pose. | Initialize the proper idle or seated pose without advancing subsequent paused animation. | Paused loading and restoration show an intentional posture. |
| Native Review changed instructions above the scrolled viewport; the supply paragraph repeated purchase details. | Reveal the inspector after an explicit Review/goal choice and show one gross reward/XP/full missing-quantity comparison. Automatic refresh preserves scroll. | A deliberate action has a visible result and understandable cost. |
| Exit selected a different owned vehicle; driving help repeated hardcoded keys; route distance lacked a label. | Preserve the displayed car through Exit, use configured movement help and label remaining route distance. | Re-entry beside the same car is reachable and driving information is consistent. |
| Recovery progress named only the source backup after the user selected a different destination name. | Lead with the chosen copy name, followed by source/checkpoint. | The background operation confirms the player's edit. |
| Source packaging needed to preserve original inputs, historical records and executable modes. | Independently compare the accepted baseline and strengthen final archive/mode and no-replace publication checks. | The deliverable retains its evidence and runnable permissions. |

Final native findings and the resulting correction batch are recorded in the
[current product review](PRODUCT_REVIEW.md); final commands, counts, receipts and
scope are recorded in [Validation](VALIDATION.md). The independent
[release engineering review](verification/v0.16-review/release-engineering-review.md)
records the baseline, dependency, payload and temporary packaging-helper checks.

The final native review retains one minor copy follow-up: re-entering the Comet
can show “Comet hatchback rental · Worker” while the control line correctly says
DRIVING. Vehicle identity, ownership and re-entry work correctly. A future status
label cleanup can make that description more consistent; it is not treated as
completed polish or as a blocking navigation defect.

## Complete remaining-gap register

**Scoped improvement** means a concrete condition was implemented and verified
within a declared boundary. It does not mean the wider product area is complete.
Priorities depend on the intended release and deployment.

| ID / product area | Current position | Remaining closure criteria |
|---|---|---|
| **G01 — Documentation** | Current goal, character, naming, compatibility and evidence guides are reconciled. Release maintenance continues. | Keep compatibility, current-guide/link, artifact and claim checks in every release; preserve historical values. |
| **G02 — Windows delivery** | Platform execution remains open. **P1 for Windows players.** | Run Windows CI and a clean-machine creation/play/save/reopen/recovery journey. Declare OS/GPU/runtime prerequisites and signing, installer, update and removal policy. |
| **G03 — Career progression** | Scoped improvement: full missing-input purchase comparison and an accepted-work continuation case; priced career records retained. **P1 for a career release.** | Test beginner/advanced strategies, setbacks, living/labor/capital costs, treasury replenishment and longer sessions. Retain finite funding and managed-intervention limits. |
| **G04 — Continuing goals** | Scoped improvement: six explicit goals, owner commitments, earliest-deadline default, exact review handoff and resume continuity. **P1 for sustained play.** | Observe uncoached first and returning Supply, Home/Community and Road/Company journeys; measure misunderstood costs, stalls and recovery success, then add sustained content where evidence calls for it. |
| **G05 — Navigation/recovery** | Scoped improvement: narrow support, reverse approach, restored flank, unavailable reconnection, moving crossings and a blocked junction in a bounded geometry matrix. **P1 for denser regions.** | Extend geometry, traffic density and long-running interactions. Require bounded physical progress or explicit value-preserving recovery across declared cases; current finite scenarios do not qualify arbitrary junctions. |
| **G06 — Community lifecycle** | Existing departure/removal/handover behavior retained. **P1 for long-lived groups.** | Define unavailable-leader succession, dissolution, territory transfer/reclaim and treasury policy; verify offline permissions and durable values. |
| **G07 — Rendering qualification** | Human clips, role materials and instance lifetimes join admission checks. Hardware qualification remains open. **P1 for performance promises.** | Profile CPU/GPU frame tails, actual residency and loading peaks on chosen hardware. Budget direct game-owned overrides or require validated metadata; import limits are not GPU measurements. |
| **G08 — Accessibility/devices** | Local native review passed the observed goal, commitments and recovery-name flows at 1100 × 700 with 150% text, including scrolling, wrapping and visible actions. Complete accessibility qualification remains open. | Qualify full keyboard/controller/wheel journeys, reconnect/device ordering, assistive access, contrast, motion comfort and localization with representative users. |
| **G09 — Production content** | Scoped improvement: original animated customized human, private per-instance tint roles, known-driver seated presentation and reproducible seven-model composition. | Add retargeting/skinning workflows, facial/equipment work, physical/timed boarding, remote occupancy/passengers, audio, broader DCC presets, licensing and production LOD/material budgets; qualify target devices. |
| **G10 — Regional data** | Original connected prepared inputs retained. | Select licensed shipping data and reproducible ingestion; resolve NoData, datum, topology, complex relations and bridge/underpass geometry; verify coverage and provenance. |
| **G11 — Trusted delivery** | Immutable local outputs and recovery identities remain integrity mechanisms. | Define signing/trust, release discovery, authenticated transfer, interruption/resume, installed-version selection and rollback. Hashes and author declarations do not authenticate publishers. |
| **G12 — Content migration** | Recovery restores exact content; no content migration was introduced. | Define ID/geometry/catalog conflicts affecting plots, resources, homes and obligations. Supply deliberate migrations with backups, rollback and exact validation. |
| **G13 — Public service controls** | Open. **Conditional P1 for Internet hosting.** | Provide account enrollment/recovery, authenticated operator access, durable moderation/audit policy and abuse controls; validate server capabilities and account recovery. |
| **G14 — WAN and capacity** | Finite local workloads remain the acceptance boundary. **Conditional P1.** | Measure sustained representative clients under delay/loss and TCP head-of-line stalls and run long soaks; derive supported populations and frame/acknowledgement targets from results. |
| **G15 — Storage stalls** | Native maintenance retains one bounded worker; storage performance remains open. | Measure transition-pause distributions/tails; reduce clone/install/mutation stalls while preserving complete validation and durable acknowledgements. Final maintenance can still wait on storage. |
| **G16 — Operational recovery** | Scoped improvement: editable recovery name, safe persistent rename, visible creation timestamp and exact metadata-only regression. **Conditional P1 for deployment.** | Set retention, confidentiality, offsite copies, recovery owners and RPO/RTO; rehearse transfer/reopening in the target deployment. Extend server/live/historical/custom-catalog policy deliberately. |
| **G17 — Observability** | Existing diagnostics and recovery errors retained; no external monitoring service added. | Add deployment retention, crash diagnostics, external audit retention, actionable alerts and ownership; qualify failure detection rather than treating stale/absent samples as healthy. |
| **G18 — Global architecture** | Conditional expansion; no global-scale claim. | Establish population/data requirements before authority partitions, cross-shard ownership/economy and adaptive delivery. Regional paging remains a single-authority technique. |

## Next evidence-led increment

1. Run uncoached first and returning sessions on the supported native build,
   concentrating on commitments, costs, continuing goals and recovery naming.
   Use observed stalls and misunderstandings to select the next content work.
2. Execute clean-machine Windows and target GPU/device journeys. Establish
   frame-time, loading and residency targets from measured hardware results.
3. Extend the declared traffic geometry and human-content matrices where
   observed failures or intended production assets demand them.
4. If Internet hosting is the next release goal, prioritize public controls,
   sustained WAN/capacity measurement and an actual deployment recovery rehearsal
   before expanding world scale.
