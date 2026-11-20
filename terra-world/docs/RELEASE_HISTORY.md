# Earlier Terra World release overviews

These overviews preserve earlier README release summaries. Their versioned claims and
links describe earlier increments; consult the current review and validation for
the delivered release. The complete change history remains in [CHANGELOG](../CHANGELOG.md).

## New in 0.23

Native actions retain the person, resource, territory or supply request
actually displayed when an action is chosen. Changed context requires a fresh
review. Food and water controls name the active person, and missing explicit
targets cannot silently become a different nearby object.

Supply work uses one shared readiness model for guidance, native controls and
authoritative admission. Capacity, expired offers, missing receiving data and
other blockers explain how to recover while preserving goods and funded rewards.
Gathering and crafting guidance considers the complete inventory exchange and
chooses feasible batches or useful alternatives. Inactive owners receive a
resume instruction instead of actions they cannot execute.

Managed rename and regional backup now participate in the world's cooperative
lease, protecting metadata and refusing active-session conflicts before backup
output is created. Desktop startup inspection bounds output while the process
runs and detects remaining POSIX background children. Existing private save,
recovery-retention, solo-community and actor-context behavior is preserved.

| Start with | Purpose |
|---|---|
| [Product review](PRODUCT_REVIEW_v0.23.md) | Findings, product perspectives and implemented decisions |
| [Research](RESEARCH_v0.23.md) | Primary-source patterns, tradeoffs and evidence limits |
| [Implementation plan](IMPLEMENTATION_PLAN_v0.23.md) | Dispatched packages, owners and acceptance criteria |
| [Full gap register](REVIEW_GAPS_v0.23.md) | Scoped closures and remaining product/platform work |
| [Validation](VALIDATION_v0.23.md) | Fresh test, native and delivery evidence with limits |
| [Playtesting](PLAYTESTING.md) | First-session route, recovery checks and issue template |
| [Desktop delivery](DESKTOP_DELIVERY.md) | Build, package, launch and platform qualification |

Save 1, regional schema 1 and asset schema 1 remain unchanged.
**Wire protocol 11** and TLS **terra-world/11** remain unchanged from 0.22.
Upgrade clients and hosts together. [Earlier release overviews](RELEASE_HISTORY.md)
and the [changelog](../CHANGELOG.md) retain prior milestones.


## New in 0.22

The current actor and its needs remain visible during Activities and possession.
Groups show explicitly labeled lowest needs; vehicles show only a valid current
driver association. Native actions retain the displayed actor/review context and
reject stale confirmation. Unused solo communities can be dissolved when they
have no shared land or settlement, without moving goods or money.

Saving and recovery distinguish failure before publication from visible output
whose durability could not be confirmed. Completed recovery output is retained
for inspection. Managed worlds hold one cooperative writer lease through final
metadata publication, so a second participating session cannot overwrite the
same world's progress. Dormant bodies no longer activate native ecology work.
A standalone desktop verifier checks a clean extracted package and optionally
performs a bounded command-line startup check.

| Start with | Purpose |
|---|---|
| [Product review](PRODUCT_REVIEW_v0.22.md) | Findings, product perspectives and implemented decisions |
| [Research](RESEARCH_v0.22.md) | Primary-source patterns, tradeoffs and evidence limits |
| [Implementation plan](IMPLEMENTATION_PLAN_v0.22.md) | Dispatched packages, owners and acceptance criteria |
| [Full gap register](REVIEW_GAPS_v0.22.md) | Scoped closures and remaining product/platform work |
| [Validation](VALIDATION_v0.22.md) | Fresh test, native and delivery evidence with limits |
| [Playtesting](PLAYTESTING.md) | First-session route, recovery checks and issue template |
| [Desktop delivery](DESKTOP_DELIVERY.md) | Build, package, launch and platform qualification |

Save 1, regional schema 1 and asset schema 1 remain unchanged.
**Wire protocol 11** and TLS **terra-world/11** add safe unused-community dissolution.
Upgrade clients and hosts together. [Earlier release overviews](RELEASE_HISTORY.md)
and the [changelog](../CHANGELOG.md) retain prior milestones.

## New in 0.20

This increment closes reviewed player-flow, mesh-recovery, multiplayer, gameplay,
storage and release-validation gaps. See [the plan](IMPLEMENTATION_PLAN_v0.20.md),
[current gaps](REVIEW_GAPS_v0.20.md) and [validation](VALIDATION.md)
for delivered scope, executed checks and remaining qualification.

## New in 0.19

This gap-closing pass strengthens dormant-player command authority, regional
storage integrity, resume-token privacy, source-bound terrain presentation,
menu notice reachability and complete release-evidence admission. The original
world, content, progression and current compatibility contracts are retained.

See the [implementation and gap-closing plan](IMPLEMENTATION_PLAN_v0.19.md),
[current review and gaps](REVIEW_GAPS_v0.19.md),
[product review](PRODUCT_REVIEW.md) and [validation](VALIDATION.md)
for implementation details, newly executed evidence and remaining acceptance work.
Windows execution, physical devices, measured GPUs, uncoached play and Internet
service qualification remain separate requirements.

## New in 0.18

This defect-closing pass strengthens four boundaries: poisoned storage workers
require validated reopen, rejected assigned-vehicle services cannot leave phantom
business expenses, standard gamepads require neutral controls after a focus or
device handoff, and desktop archives reject paths that alias on Windows.

See the [implementation plan](IMPLEMENTATION_PLAN_v0.18.md),
[current review and gaps](REVIEW_GAPS_v0.18.md),
[product review](PRODUCT_REVIEW.md) and [validation](VALIDATION.md)
for the exact changes, executed checks and remaining qualification work.
Save 1, regional schema 1, wire 10 and asset schema 1 remain unchanged.
Windows execution, physical input devices, GPU performance, long uncoached play
and public deployment remain separate acceptance work.

## New in 0.17

This pass prepares the regional reference for repeatable playtesting and further
iteration. The [playtest guide](PLAYTESTING.md) supplies a first-session
route, save/reopen checks, a focused product matrix and a concise issue template.
The [0.17 improvement and open-gap report](REVIEW_GAPS_v0.17.md),
[product review](PRODUCT_REVIEW.md) and [validation](VALIDATION.md)
record what changed and what was actually verified.

- **Recover clearly from a stopped storage worker.** An uncertain final
  operation stays blocked until the world is reopened and its committed state
  validated. Pending work, ordinary retryable errors and terminal worker loss
  now have distinct guidance, including an actionable Close failed world exit.
- **Collect local playtest evidence from the menu.** Settings offers an explicit,
  default-Off diagnostic choice. The bounded report records real-time samples,
  pause/input state and committed checkpoint status. The session menu and F12
  show actual publication, age and failures; Settings names the retained file.
  Single-writer ownership protects a shared destination, including after a crash.
- **Keep driving actions tied to the reviewed context.** Interact follows the
  possessed vehicle's accepted assignment even when another offer was browsed.
  Rental returns retain the displayed vehicle, garage and complete quote, and
  reject stale or repeated confirmation. Contextual key hints follow rebinding,
  and re-entry reports the vehicle consistently.

Save **1**, regional schema **1**, wire **10** and asset schema **1** remain
unchanged. The optional human/seat fields introduced in 0.16 keep their existing
reader requirement. The diagnostic envelope retains its existing schema and
adds observational session fields; it grants no authority and contains no saved
world. See [Compatibility](COMPATIBILITY.md) and
[Diagnostics](DIAGNOSTICS.md). Windows execution, physical hardware,
uncoached sustained play and deployment qualification remain explicit acceptance
work; the included Linux build is a review package.

## Foundations from 0.16

This increment connects continuing goals, constrained vehicle navigation,
customized human presentation and recognizable recovery copies. The
[0.16 improvement and gap report](REVIEW_GAPS_v0.16.md),
[product review](PRODUCT_REVIEW_v0.16.md) and [validation](VALIDATION_v0.16.md)
record implementation, review corrections and the exact acceptance boundary.

- **Find and resume your commitments.** Activities has direct goal choices and
  a live owner-only briefing for accepted supply, road/company assignments and
  rental bonds. Supply prioritizes deadlines, explains gross rewards and missing
  input costs, and retains the exact request shown before an action.
- **Recover from constrained roads.** New bounded maneuvers handle supported
  narrow passages and regain approach room after prolonged blockage. Consumed
  waypoints no longer exhaust the route limit. Physical handling, live collisions,
  source support and the original cargo/goal remain authoritative.
- **Load a customized animated person.** The original Fieldhand example retains
  body, skin, hair and outfit choices through per-instance appearance, with named
  idle, walking, running and seated clips. The optional composed pack demonstrates
  a known manual driver's visual seat handoff with a separate original Comet cabin
  variant and the unchanged wildlife models.
  See the [character example](../crates/terra-bevy/assets/character-pack/README.md).
- **Recognize your recovered worlds.** Choose a name in the recovery review, or
  use Rename in Your worlds. Creation times help identify copies; the actual
  separate world identity and gameplay files stay intact.

The existing world save and regional schema remain **1**, with **wire protocol 10**.
New optional human/seat pack fields require the **0.16** reader; older pack identities
are retained. [Compatibility](COMPATIBILITY.md) separates those contracts.
The default woodland pack remains; the new seven-model pack is an explicit demo
option. Hardware, Windows, uncoached player studies and deployment qualification
remain separately stated in the gap report.

## Foundations from 0.15

This increment makes blocked roads, model delivery and saved-world recovery more
usable. The [0.15 improvement and gap report](REVIEW_GAPS_v0.15.md),
[product review](PRODUCT_REVIEW_v0.15.md) and [validation](VALIDATION_v0.15.md)
separate implemented behavior from the platforms and workloads actually checked.

- **Navigate a cluster of parked vehicles.** Autonomous road vehicles can try a
  bounded passage around several stopped bodies when the existing one-body
  maneuver fails. Every leg still requires terrain, traffic and contact
  admission; movement uses the shared handling model. Moving bodies and
  unavailable terrain do not become static shortcuts.
- **Load a model for one vehicle type.** The original Comet example combines
  exterior, cabin and animated wheels with a vehicle-model-specific pack target.
  Wheel rolling follows signed admitted movement; steering and pause remain
  presentation-only. A ready-to-load combined demo pack and an authoring recipe
  retain the original wildlife models and character customization.
- **Back up and recover through the native menu.** Your worlds exposes supported
  checkpoint backups and a Backups view. Review the named checkpoint before
  restoring into a separate managed world. A bounded background operation keeps
  the menu responsive; incomplete or changed copies cannot silently become a
  launchable world. Existing bundles, worlds and saved authority stay separate.

For 0.15, all 34 final gates passed with 860 distinct Rust tests and no failed or ignored
tests. Three native confirmations used the accepted executable. The
[release record](release-verification-v0.15.json) binds automated, native,
extracted Linux package and preservation evidence; the Linux artifact is a
review build, with platform/hardware qualification still stated separately.

The [0.14 review](PRODUCT_REVIEW_v0.14.md),
[validation](VALIDATION_v0.14.md) and [gap register](REVIEW_GAPS_v0.14.md)
remain unchanged historical evidence. Current save/schema/protocol contracts
are in [Compatibility](COMPATIBILITY.md).

## Foundations from 0.14

This increment connects returning-player guidance, model authoring and local
world recovery. The [0.14 improvement and gap report](REVIEW_GAPS_v0.14.md)
retains the original G01–G18 register; the [product review](PRODUCT_REVIEW_v0.14.md)
and [validation record](VALIDATION_v0.14.md) state what was actually exercised.

- **Continue a company with a clear next step.** Activities gains a live Company
  goal that names current service blockers, retained commitments and the relevant
  Fleet destination. Displayed handoffs retain their reviewed service identity;
  existing command permissions, costs and physical recovery rules still apply.
- **Build a pack from an artist's exports.** `terra-assets inspect` lists actual
  GLB clips and budgets. `build` reads an editable recipe, snapshots and validates
  all source bytes, computes content-identified paths, and publishes a new pack.
  The [authoring guide](ASSET_AUTHORING.md) includes a working original
  wildlife recipe. Character customization and original pack identities remain.
- **Recover the complete local world.** New `terra-worldctl` bundle operations
  capture a committed database, pinned catalog, resolved configuration and local
  prepared-source dependencies, then verify and restore to a new location.
  See [Recovery bundles](RECOVERY_BUNDLES.md) for supported sources and the
  explicit local-host scope.
- **Retain evidence before exit.** Optional periodic native diagnostics use a
  bounded background writer and retain 16 successful captures across runs.
  Timestamps and running/exit status distinguish an interrupted session from a
  graceful exit. See [Diagnostics](DIAGNOSTICS.md).

The unchanged [0.13 review](PRODUCT_REVIEW_v0.13.md),
[validation](VALIDATION_v0.13.md) and
[gap report](REVIEW_GAPS_v0.13.md) remain the preceding release record.

## Foundations from 0.13

This follow-up uses the [independent open-gaps report](OPEN_GAPS_REPORT_v0.12.md)
to improve daily play, community ownership and developer handoff. The original
[PDF report](reports/Terra-World-0.12-Open-Gaps-Report.pdf) is included in the
project, with a [0.13 status report](REVIEW_GAPS_v0.13.md) tracking all 18 gaps.

- **Walk around a parked obstacle.** Owned pedestrians can use two checked
  local corners when a stopped body blocks an otherwise useful shortcut.
  Support, facilities, structures, body sweeps, waypoint limits and a shared
  query budget still govern the complete route. Unsupported ground retains the
  earlier route; movement remains physical.
- **Move a parked vehicle through RTS orders.** An accepted move releases its
  parking brake after the full selected group validates. Fuel, condition, rental
  expiry, company assignment, terrain and contacts still constrain the vehicle.
- **Manage a community over time.** Members can leave; leaders can remove
  members, revoke invitations and hand leadership to an existing member.
  Departures revoke shared access and faction homes, including offline homes,
  while shared property and public funds stay with the faction. The roster,
  named invitations and consequence review explain the action being taken.
- **Understand a commercial route before dispatch.** Fleet shows current
  first-departure receipts, visible stock or queues, payroll-only coverage and
  the difference between a warehouse buyer's price and a visible retail input.
  It does not reserve funding or hide unknown acquisition, vehicle and labor
  costs. The finite passenger-career measurement includes real rental and
  refund accounting; its traffic interventions and limits remain explicit.
  The native quantity picker reaches the three-seat compact and 30-seat bus.
- **Keep confirmations tied to what was reviewed.** Community changes and
  destructive Fleet actions retain their player, restore context and target.
  A changed world or selection requires a new review. Management focus follows
  a newly opened community review so its confirmation remains reachable.
- **Hand off a checked release.** A canonical compatibility guide is checked
  against code and the workspace lock. Desktop packaging uses the exact
  accepted executable, a curated original-content list, licenses, launchers and
  hashed verification evidence. CI is configured for Linux and Windows;
  Windows execution and hardware qualification remain separate, uncompleted work.

See [the current product review](PRODUCT_REVIEW.md),
[validation](VALIDATION.md), [compatibility](COMPATIBILITY.md) and
[desktop delivery](DESKTOP_DELIVERY.md) for actual acceptance and remaining
limits. The [0.12 review](PRODUCT_REVIEW_v0.12.md) and
[0.12 validation](VALIDATION_v0.12.md) preserve the previous release.

## Foundations from 0.12

This follow-up traces the remaining documented gaps through actual gameplay,
content tooling and host recovery:

- **Read a complete cash report.** Fleet → Finances separates operating
  receipts/payments, setup, owner funding and known personal business costs.
  Persisted category totals reconcile to company cash; a bounded recent journal
  explains transactions. Older worlds disclose their imported opening balance
  and unknown earlier costs. Stock supplied without a price and vehicle costs
  before assignment are identified rather than treated as free profit.
- **Take a supported walking route.** Ordinary ground orders can remove long
  road-perimeter detours only after checking the complete local corridor against
  loaded source support, structures and bodies. Missing support retains the
  original route. Patrol vehicles can use the same checked temporary detours as
  other autonomous vehicles, allowing the exact saved traffic-jam fixtures to
  recover their original loads without moving through an obstacle.
- **Bound the total imported scene.** Validated model packs load lazily under
  configurable instance, geometry, animation, texture and estimated resident-byte
  budgets. Current, loading and retiring resources remain charged; actors keep
  their procedural appearance while a model is unavailable or deferred. F12 and
  telemetry explain selection, fallback and draining.
- **Distribute prepared terrain reproducibly.** `terra-sources inspect`,
  `export`, `verify` and `install` produce a self-contained, content-identified
  directory with provenance and exact tile coverage. Unsafe paths, unexpected
  bytes and existing destinations reject before publication;
  `--require-complete` also rejects incomplete requested coverage. The original connected demonstration retains its source identity.
- **Recover a populated local session with a large cold world.** The release
  workload combines actual verified TLS clients, prepared terrain, disconnected
  owners, dormant overlay regions, a real SQLite writer failure and a verified
  backup. It checks authenticated resume, command sequences, all private state
  and exact cold records, and reports acknowledgement latency separately.
- **Keep management usable in a small window.** Fleet has six clearly selected
  sections, a compact chooser when space is tight and a shorter shared summary.
  Switching sections reveals their beginning; changing text size preserves the
  reader's position. An empty finance report leads to company setup, and returning
  from possession frames the selected character in the RTS view.

The [0.12 gap register](REVIEW_GAPS_v0.12.md),
[product review](PRODUCT_REVIEW_v0.12.md) and
[validation record](VALIDATION_v0.12.md) distinguish implemented corrections,
executed acceptance and external data/hardware qualification. Run
`python3 scripts/verify.py` for the reproducible automated release gates.
The [0.11 review](PRODUCT_REVIEW_v0.11.md) and
[0.11 validation](VALIDATION_v0.11.md) remain historical records.

## Reliability foundations from 0.11

This documentation-led review closes gaps in guidance, transaction handoffs,
vehicle placement and content admission:

- **Find a usable next step.** Urgent food guidance considers available foods,
  nearby sellers and usable workbenches. A recommended market remains selected
  when the player opens it. Market and production travel share the guidance
  planner's supported, unoccupied approach checks.
- **Approve the price shown.** Personal market rows show the total for the
  selected quantity, stock, carried goods and concrete availability reasons.
  Personal, depot and roadside actions retain the displayed quote and reject a
  changed price or selection. The host checks personal trade totals atomically.
- **Leave a vehicle safely.** Dismounting requires a stopped vehicle and a
  supported, unoccupied avatar footprint with a clear corridor beside it.
  Failed exits preserve possession and vehicle state. Traffic exit checks use
  the outgoing vehicle orientation and both bodies' rotated footprints.
  Purchases and rentals receive a supported, clear parking bay before funds,
  fuel or rental bonds change.
- **Restore a clean input context.** Successful local restore retires commands,
  queued UI actions and transient placement/interaction state from the previous
  world. Failed restore preserves continuation under the existing authority.
- **Catch broken or excessive assets before play.** Validation decodes PNG
  textures, rejects undeclared extension payloads and counts instantiated scene
  geometry as well as declared meshes. Animation retains elapsed time at slow
  frame rates and normalizes accepted rotation keys.
- **Use valid connected coverage.** Automatic prepared-map focus can find a
  complete tile covered by adjacent bundles even when no child center works.
  Every candidate still passes the exact union-coverage check.
- **Reproduce the release checks.** `python3 scripts/verify.py` runs the complete
  automated suite, examples, asset acceptance, larger-world recovery probe and
  native build. It records commands, logs, distinct tests and source hashes.

The [0.11 gap register](REVIEW_GAPS_v0.11.md),
[0.11 product review](PRODUCT_REVIEW_v0.11.md) and
[0.11 validation record](VALIDATION_v0.11.md) separate fixes, executed evidence and
remaining acceptance work. The previous release's
[review](PRODUCT_REVIEW_v0.10.md) and
[validation](VALIDATION_v0.10.md) remain archived.

## Integrated foundations from 0.10

The preceding release connected sustained player goals, animated content, autonomous
handling, adjacent prepared landscapes and verified operational recovery:

- **Keep a useful next step in view.** Activities [J] derives supply, home,
  community and road goals from current inventory, permissions, progress and
  accepted jobs. Its action uses the ordinary command rules and the exact
  transaction shown. Guidance updates after saves, restores and other actions;
  the compact world banner keeps the next goal visible during possession.
  Suggested journey prioritizes urgent water, food and rest needs, including
  the actual driver's needs while a vehicle is active.
- **Supply a validated animated model pack.** The default game includes five
  original animated woodland species. Immutable GLB packs carry content hashes,
  bounded geometry/animation budgets and per-binding gait policies. Procedural
  actors stay visible until replacement scenes are ready. Character appearance
  and saved authority remain independent of this optional presentation layer.
- **Use the same bounded handling for autonomous vehicles.** Road traffic,
  ordered vehicles and commercial drivers generate pedal and steering intent
  for the shared driving model. Four-wheel support, swept body admission,
  physical turns, fuel and actual travelled distance apply. Saved steering and
  safe crew placement carry through regional paging and authenticated resume.
- **Join prepared terrain across real seams.** A collection of adjacent raster
  and semantic bundles validates connected coverage, exact edge relationships,
  mixed-resolution seam samples and bounded provenance. Holes and unsupported
  ground remain unavailable. New Heightmap valley worlds use the original
  two-bundle example; older worlds retain their pinned sources.
- **Recover a larger stored world deliberately.** Online backups validate the
  copied snapshot, publish only to a new path and report the generation actually
  copied. `terra-worldctl recover` creates a verified new database from a backup.
  A reproducible 1,027-region probe checks complete dormant state, private
  metadata, corruption rejection and source preservation.
- **Keep controls usable in a compact window.** At enlarged text sizes the
  inspector collapses its page grid behind an explicit activity chooser, keeping
  the current action reachable. Keyboard focus, footer spacing, source-banner
  hit testing and diagnostic identity labels follow the same layout boundaries.

Start with [player guidance](GUIDANCE.md), [asset packs](ASSET_PACKS.md),
[source bundles](SOURCE_BUNDLES.md), [vehicle dynamics](VEHICLE_DYNAMICS.md)
and [operations](OPERATIONS.md). The [product review](PRODUCT_REVIEW.md)
and [validation record](VALIDATION.md) distinguish implemented behavior,
executed evidence and remaining production work. The
[0.9 review](PRODUCT_REVIEW_v0.9.md) preserves earlier release scope.

**Wire protocol 10** and TLS **terra-world/10** require matching client and host
binaries. Protocol 10 adds explicit community membership and leadership commands;
protocol 9 introduced private cash records and accounting-capacity holds. See
[Compatibility](COMPATIBILITY.md) for the checked current contract. Domain save
version **1** and regional schema **1** remain. Compatible
previous saves load through additive defaults and unchanged source/catalog checks.
New semantic terrain bundles use a distinct source identity; they do not silently
replace the terrain of an existing world.

Earlier foundations remain integrated: complete character/world/session flow,
construction and housing, needs and farming, wildlife and resources, factions
and territorial progression, rentals and fleet recovery, finite production and
household demand, source-aware roads, owner display prediction, and transactional
regional checkpoint/resume. See the [0.8 review](PRODUCT_REVIEW_v0.8.md) for
that release's scope and measurements. Current population budgets and the
single-authority architecture remain explicit.

## New in 0.21

This pass improves loading/recovery controls, isolates on-screen text editing,
and explains each session's actual save behavior. It fixes supply-board
maintenance with distinct merchant assortments, initializes clipped polar
neighborhoods, targets terrain updates to affected source chunks, and validates
native regional activation through the existing storage worker. JSON snapshot
replacement keeps owner-only Unix permissions. Verification commands have
explicit deadlines and retain timeout evidence. Managed worlds now retain an
immutable launch record before persistence starts, so interrupted launches with
missing required progress produce recovery guidance instead of a fresh world.
