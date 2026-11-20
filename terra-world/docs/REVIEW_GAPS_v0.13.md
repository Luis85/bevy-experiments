# Open gaps and improvement follow-up — 0.13

Review date: **2026-10-28**. This report follows the independent
[0.12 open-gaps report](OPEN_GAPS_REPORT_v0.12.md). Its unchanged
[PDF](reports/Terra-World-0.12-Open-Gaps-Report.pdf) is included in the project,
alongside the archived [0.12 product review](PRODUCT_REVIEW_v0.12.md) and
[validation record](VALIDATION_v0.12.md). The original **G01–G18** identifiers
remain stable. Historical findings describe the accepted 0.12 baseline; this
report records what changed and what still needs work.

**Final release acceptance:** all **22 release gates passed**, with **785 distinct Rust tests**, zero failed or ignored tests and **338 unchanged source inputs**. Three clean final native sessions confirmed the accepted executable, including saved-world reopening and community restoration; two separately labeled candidate sessions retain the preceding visual review. The final source-backed walking replay also passed. The [desktop record](desktop-v13-verification.json) identifies the Linux package and its separate extracted-folder smoke result; Windows was not executed.


The simulation measurements below were repeated against the frozen release.
Candidate native observations and final confirmations remain separately labeled.
The final acceptance record identifies the exact source and executable. Refer to the
[release record](release-verification-v0.13.json),
[gates](verification/v0.13-final/gates.json),
[native record](native-v13-verification.json), [validation](VALIDATION.md) and
[product review](PRODUCT_REVIEW.md) for the final acceptance boundary.

## Outcome and product scope

This pass improves the regional SDK and reference game's everyday decisions,
navigation, group ownership and developer handoff. Three implementation agents
worked on walking, community lifecycle and commercial planning. Independent
reviews checked authority, offline state, cost accounting, retained UI selections,
documentation and distribution. Native review then exercised the integrated
management flows, including a **1100×700 window at 150% text**.

The resulting scope closes identifiable defects and supplies a cash-positive finite
beginner business example. It does not establish a complete consumer game,
indefinitely sustainable economy, Windows hardware qualification, production
Internet service or worldwide simulation. Production art, regional data,
accessibility and operational work remain visible in the register below.

## Improvements and evidence

### Supported walking and useful vehicle orders

Owned pedestrians can propose two corners around a stopped body when that
bypass improves the existing route. The entire new bypass must remain within
the **160 m local envelope** and **129-waypoint limit**, pass source, body,
structure and facility checks, and share the command's **16,384 support-query
budget**. Sampling remains at 0.25 m with nine footprint support points. Failed
or exhausted proposals retain the old route proposal; ordinary movement still
checks each actual step. Planning never moves the actor.

The source-backed replay of the preserved market approach reduced its planned
route from **261.806 m to 75.345 m**, using **2,817 source queries**. The character
reached the same goal through normal physical movement in **18.9 seconds**.
The replay checked support, contact, step bounds and value conservation, and
an exact save/resume continuation. This is a measured local obstruction case,
not a shortest-path or arbitrary-obstacle guarantee. The exact saved-case
replay is distinct from native interface observations.

Review also found that parked vehicles accepted an RTS move while their brake
prevented progress. An accepted explicit `MoveUnits` order now releases the
parking brake only after the entire selected group passes validation. Rejected
groups retain every order and brake. Exit and company release still park the
vehicle; fuel, condition, rental expiry, assignment, terrain and collision
constraints continue to govern its motion.

Trace: [walking planner](../crates/terra-sandbox/src/living/walking.rs),
[walking regressions](../crates/terra-sandbox/tests/walking_routes.rs),
[saved-case replay](../crates/terra-net/examples/walking_replay.rs),
[RTS command implementation](../crates/terra-sandbox/src/living/commands.rs),
[vehicle-order regressions](../crates/terra-sandbox/tests/rts_vehicle_orders.rs),
[routing contract](CONTACTS_AND_ROUTING.md) and [transport](TRANSPORT.md).

### Community lifecycle and reliable confirmations

Members can leave, leaders can remove members or revoke invitations, invitees
can decline, and a leader can hand leadership to an existing member. A leader
must hand over before leaving; the former leader becomes a Builder. Departure
clears faction-home access, including offline and cold references, while
preserving personal property, other memberships, shared assets, public stock
and treasury. Creator IDs confer no permission after a role changes. Invalid
actions reject before mutation.

The native panel shows the actual invitation and a paged roster. Consequence
reviews bind the player, restore revision, faction roster and selected property.
Leave, removal, handover and permanent property sharing cannot silently retarget
an earlier confirmation. Fleet confirmations likewise retain the reviewed
driver, route, depot, trip or funding amount through selection changes and
restores. Management focus moves to a newly opened review without activating it.

Native candidate review confirmed that Tab navigation to **Cancel** preserved
leader 1. Confirmed handover changed leadership to player 2 and left the former
leader a Builder; F5/F9 restored those roles. The review also retained 150% text
through controls and save/quit. These are specific observed paths, not complete
keyboard-only or assistive-technology qualification.

The separate two-principal TLS journey exposed a replica-validation mismatch:
nearby visible faction property intentionally exposes its bounded public roster,
but a departing nonmember's client rejected it. Validation now follows the
authority's relevance rule, still rejects unrelated rosters and retains private
inventory boundaries. Tests cover membership transitions and host save/restart.

Trace: [domain lifecycle](../crates/terra-sandbox/src/factions.rs),
[domain regressions](../crates/terra-sandbox/tests/faction_lifecycle.rs),
[TLS journey](../crates/terra-net/tests/faction_lifecycle_tls.rs),
[replica validation](../crates/terra-net/src/replication.rs),
[native community flow](../crates/terra-demo/src/communities.rs),
[Fleet](../crates/terra-demo/src/fleet.rs) and
[community rules](SETTLEMENTS_AND_ECONOMY.md).

### Commercial decisions and a measured beginner strategy

Fleet explains the selected route's **first departure** before dispatch: full-load
receipt, visible stock or passenger queue, and payroll-only time coverage. A
warehouse sale compares the buyer's price with a named visible stocked retailer,
including that seller's limited quantity. Unknown data remains unknown. The
preview neither reserves demand/funding nor promises a journey time or profit.

The passenger quantity selector now reaches **three**, matching the compact
rental used by the novice career, and also reaches the larger supported loads.
Native candidate review confirmed the three-passenger choice and correct quote
at 1100×700/150% text, with readable scrolling. This closes a practical mismatch
between an available domain strategy and the native controls.

The finite career starts with the ordinary **250-credit wallet**, deposits
**100 credits once**, hires one driver and rents a Comet hatchback. It uses the
unchanged Town–Hillside roads, queues and prices, with no later funding grant.
The **3,600-second service window** plus final delivery and physical rental
return took **3,649.5 physical seconds**, completing **65 paid deliveries**.

| Measured cash movement | Credits |
|---|---:|
| Public fares | 821.60 |
| Registration and recruitment | −30.00 |
| Payroll | −70.71 |
| Initial lease and renewals | −23.40 |
| Fuel payment net of return refund | −2.38 |
| Actual rental-return damage charge | −0.63 |
| **All measured cash costs** | **127.12** |
| **Combined personal/company cash gain** | **+694.48** |

Six exact ten-minute save/resume checkpoints and a final regional accounting
round trip check continuation. There were **five explicit ten-second wait/resume
interventions** after route holds. The ecology clock stayed at zero, and living
costs and the owner's management time remain unpriced. Passengers circulate
within finite queues and fares come from finite public funding. This is a
managed-service measurement, not unattended-autonomy or human-playtest evidence.

The two loss-making warehouse careers remain unchanged. A successful passenger
strategy does not erase their input costs or prove every business profitable.
Trace: [planning API](../crates/terra-sandbox/src/transport/company/planning.rs),
[planning regressions](../crates/terra-sandbox/tests/company_planning.rs),
[career measurement](../crates/terra-sandbox/tests/company_career.rs),
[company guide](COMPANIES.md) and [continuing guidance](GUIDANCE.md).

### Documentation and desktop delivery

Current guides now describe wire protocol **10**, TLS **terra-world/10**, six
Fleet sections and the implemented community lifecycle. Domain save and regional
schema remain **1**. The [compatibility checker](../scripts/check_compatibility.py)
reads source constants, workspace/lock versions and the pinned toolchain; it
checks selected handoff statements while preserving historical versions. Manual
review also corrected narrative claims outside that checker.

The [desktop packager](../scripts/package_desktop.py) requires passed local gates,
unchanged source inputs, the recorded compiler and exact accepted executable.
A curated [payload list](../scripts/desktop-payload.json) includes original
runtime content, licenses and current guides. Accepted logs and reports retain
their hashes; ZIP contents are checked before publication to a new destination.
Launchers establish the working directory and separate user-data paths.

Linux and Windows CI jobs are configured. **Windows was not executed in this
pass.** The final acceptance record above identifies the Linux extracted-folder
smoke outcome. System graphics/input/C runtime libraries are not bundled; the optimized
dev-profile package does not establish release-profile performance, universal
Linux portability or a signed Windows installer. See
[Desktop delivery](DESKTOP_DELIVERY.md) and [Compatibility](COMPATIBILITY.md).

## Full gap register and completion criteria

“Scoped correction” identifies the specific defect or demonstrated case. A
broader baseline gap remains open where its original completion criteria still
require content, policy, hardware or sustained evidence. P1 is a gate for the
named audience or promise; public-service priorities are conditional on hosting.

| ID | Status after this pass | Remaining closure criteria |
|---|---|---|
| **G01 — Documentation** | Scoped correction: current handoff and lifecycle drift corrected. | Keep compatibility checks and manual current-guide review in release acceptance. Preserve historical values and verify new feature/navigation claims when they change. |
| **G02 — Windows delivery** | Packaging tooling improved; platform qualification open. **P1 for Windows players.** | Execute Windows CI and a clean-machine startup, creation, play, save/reopen and update/removal journey. Declare supported OS/GPU prerequisites; decide signing and installer/update policy. |
| **G03 — Career progression** | One priced managed beginner strategy measured; broad balance open. **P1 for a career release.** | Establish additional beginner/advanced strategies through scarcity and setbacks, include or explicitly exclude living/labor/capital costs, and measure treasury replenishment and longer player sessions. |
| **G04 — Continuing goals** | Decision guidance and reachable quantities improved; content/player evidence open. **P1 for sustained play.** | Define first-session and returning-player journeys; observe uncoached supply, home/community and road/company play. Close measured stalls, misunderstood costs and recovery failures with explicit targets. |
| **G05 — Navigation/recovery** | Scoped walking case and parked RTS-order defect corrected; general routing open. **P1 for denser regions.** | Exercise multiple/moving obstacles, narrow roads, blocked junctions, alternate exits and missing/restored terrain. Require physical progress or a usable value-preserving recovery action within declared bounds. |
| **G06 — Community lifecycle** | Explicit departure/removal/handover implemented; broader governance open. **P1 for long-lived groups.** | Define unavailable-leader succession, dissolution, territory transfer/reclaim and treasury policy. Implement only the intended transitions, including offline permissions and durable value preservation. |
| **G07 — Rendering qualification** | Open; bounded validated-asset admission retained. **P1 for performance promises.** | Profile CPU/GPU frame tails, actual residency and loading peaks on chosen hardware with representative scenes. Budget direct game-owned scene overrides or require validated metadata. |
| **G08 — Accessibility/devices** | Selected focus, cancellation, scrolling and enlarged-text paths reviewed; broader acceptance open. | Qualify complete supported keyboard/controller/wheel journeys, reconnect and device ordering. Test assistive access, contrast, motion comfort and localization with representative users. |
| **G09 — Production content** | Open; validated animated wildlife example retained. | Deliver representative human/customization, vehicle/cabin and boarding/seated content with audio, export presets, licensing and LOD/material budgets. Verify transitions, first-person visibility and required attachment workflows. |
| **G10 — Regional data** | Open; connected authored prepared bundles retained. | Choose licensed shipping-region inputs and reproducible ingestion. Resolve NoData, datum, topology, complex relations and bridge/underpass geometry; test real coverage and provenance. |
| **G11 — Trusted delivery** | Open; immutable offline distribution retained. | Define publisher trust/signing, release discovery, authenticated transfer, interruption/resume, installed-version selection and rollback. Content hashes alone do not establish publisher identity. |
| **G12 — Content migration** | Open; strict saved-source/catalog rejection retained. | Define ID and geometry conflicts affecting plots, resources, homes and obligations. Provide deliberate source/catalog migrations, backup, rollback and exact post-migration validation. |
| **G13 — Public service controls** | Open. **Conditional P1 for Internet hosting.** | Supply account enrollment/recovery, authenticated operator access, durable moderation/audit policy and abuse controls. Verify authorization and recovery beyond private resume capabilities. |
| **G14 — WAN and capacity** | Open beyond finite local workloads. **Conditional P1 for hosting promises.** | Measure representative clients under sustained WAN delay/loss, TCP head-of-line stalls and long soaks. Declare population and frame/acknowledgement targets from those results. |
| **G15 — Storage stalls** | Open; durable acknowledgement barrier retained. | Measure transition pause distributions and tails; reduce main-thread clone/install and mutation stalls while preserving complete validation and acknowledgement after durability. |
| **G16 — Operational recovery** | Open for deployment. **Conditional P1.** | Define retention, confidentiality, offsite copies, recovery owners and RPO/RTO. Retain source/catalog/configuration dependencies and rehearse restore and supported migrations against those targets. |
| **G17 — Observability** | Open; graceful-exit native telemetry retained. | Add periodic retained metrics, crash diagnostics, external audit retention, actionable alerts and incident triage. A missing post-crash export is not evidence of a healthy run. |
| **G18 — Global architecture** | Conditional expansion; no new scale claim. | Set actual population/data requirements before adding authority partitions, cross-shard ownership/economy and adaptive global content delivery. Regional paging is not distributed simulation. |

## Recommended next pass

First qualify the intended regional player release: run the supported Windows
hardware journey, define performance targets, and observe new and returning
players using the now-priced compact career and community controls. Prioritize
failures by blocked progress, lost understanding or unsafe recovery, retaining
the exact worlds and conditions that reproduce them.

Next complete one representative production human/vehicle/boarding slice and one
licensed regional dataset. Those concrete inputs should determine importer,
animation, navigation and rendering changes, rather than expanding every format
or simulation feature speculatively.

If public hosting is selected, treat G13–G17 as a separate acceptance programme
with named operational owners, capacity targets and rehearsed restoration.
Expand G18 only when measured regional requirements exceed the current single
authority and bounded population/content contracts. The original baseline and
this follow-up remain the audit trail for that decision.
