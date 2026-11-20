# Product review and improvement pass — 0.12

Review date: **2026-10-28**. **Status: all 20 frozen-source automated gates and
final native acceptance passed.** This review separates current automated
results, pre-polish candidate observations and final native confirmation. The
[0.11 product review](PRODUCT_REVIEW_v0.11.md) and
[0.11 validation](VALIDATION_v0.11.md) preserve their original evidence.

The pass began with the six acceptance areas in the
[documentation-led gap register](REVIEW_GAPS_v0.12.md), followed their causes
through code, and used parallel economy, navigation and content/source agents.
Independent reviews then examined financial presentation, authority adapters,
asset retirement, source distribution and verification evidence. The current
[validation record](VALIDATION.md) contains the final source/binary identity,
commands and executed automated results.

## Product outcome and acceptance status

The reference game now explains more of what continued play costs, permits
supported shortcuts on clear ordinary walking routes, and allows the reproduced
autonomous queues to clear through physical movement. Game developers gain
aggregate imported-model admission and a verified portable source handoff.
Operators gain one workload
that combines actual client sessions, prepared terrain, dormant storage and
failure/recovery checks. Fleet management has a compact section chooser and an
explicit cash-report view. These changes address concrete breaks in the existing
economy, ownership, movement and presentation rules.

The [complete final run](verification/v0.12-final/gates.json) passed all **20
gates**, with **747 distinct Rust tests**, zero failed/ignored cases and **327
unchanged source inputs**. Every retained log and accepted report matched its
captured hash. The paired cash results, exact saved-traffic recoveries, scene
degradation, portable source roundtrip and combined operational workload below
were rerun in that sequence. The [native acceptance record](native-v12-verification.json)
confirms three clean sessions of the polished gate-built executable, with 14
captures and exact save/reopen continuation. Earlier screenshots, test totals and
hardware observations are not relabeled as current results.

## A01 — Company economics that explain the player's money

Revenue and payroll alone could not explain a company's cash result. Setup,
stock acquisition, fuel, roadside service and owner support could occur outside
the reported delivery window. A player could see a completed route and matching
ending cash without understanding that one supply strategy returned substantially
less money to its owner.

The persisted company report now separates operating, capital and financing
movements, reconciles recorded totals to opening/current cash, and retains the
latest 64 monetary entries. Deposits add company cash without becoming earnings.
Attributable personal registration and assigned-vehicle costs are disclosed
separately. The UI labels known owner funding **before withdrawals**, matching
its gross-funding definition. Categories and recent entries explain where the
money went. [Company accounting](COMPANIES.md) documents the contract and the
[Finances implementation](../crates/terra-demo/src/fleet.rs) presents it.

Compatible older companies begin from their actual imported balance. Their
historical revenue/payroll counters remain identified as earlier history; the
report does not reconstruct missing expenses. Contributed goods and a personal
vehicle's earlier capital/input costs remain explicitly unpriced. Validation
covers the durable journal and totals before cash, stock or escrow mutation.
Lifetime counter exhaustion holds work or rejects the transaction, preserving
exact records. The bounded recent list can roll forward while lifetime totals
remain intact; those are different limits.

### Fresh paired-career result

Both [career scenarios](../crates/terra-sandbox/tests/company_depots.rs) completed
six sessions, 24 physical deliveries and seven in-career restores, followed by
an exact regional accounting roundtrip. Each sold 96 water over 2,967.45 physical
seconds. The complete fixture conserved 13,900.00 credits, 224 water and 40 bread,
including household consumption. These are current finite domain results from
the frozen release run, not a rendered or campaign-length economy measurement.

| Cash observation, in credits | Owner-contributed harvest | Stock purchased at retail |
| --- | ---: | ---: |
| Goods sale receipts | 28.80 | 28.80 |
| Stock acquisition payment | Unknown; 96 contributed units | 76.80 |
| Payroll | 15.49 | 15.49 |
| Recovery fuel / roadside callout | 3.00 / 15.00 | 3.00 / 15.00 |
| Recruitment | 10.00 | 10.00 |
| Operating cash result since registration | **−14.69** | **−91.49** |
| Depot access capital payments | 50.00 | 50.00 |
| Cash result before owner financing | **−64.69** | **−141.49** |
| Personally paid registration | 20.00 | 20.00 |
| Result after known owner-paid costs | **−84.69** | **−161.49** |
| Owner deposits / withdrawals | 205.00 / 126.38 | 205.00 / 49.58 |
| Final company cash | 13.93 | 13.93 |

The identical final company cash hides a meaningful difference: the company
buying stock returns **76.80 fewer credits** to its owner. The persisted report
makes that difference visible. Retail acquisition alone pays 76.80 credits for
goods sold for 28.80, a 48.00-credit merchandise shortfall before paid work or
travel. Prices, wages and deadlines were not adjusted to make this example
profitable.

Both original delivery-window fields remain **−4.69 credits**, because setup and
acquisition happened before that window. The fixture deliberately begins with
0.04 litres of fuel to require one funded recovery: 3.00 credits purchases two
actual litres and 15.00 pays the callout. No repair was purchased in these runs.
The callout is a stress condition, not a recurring-service requirement inferred
from an ordinary route. Recruitment, depot access and registration add 80.00
credits of known setup outlay. Contributed harvest labor, vehicle capital and
prepaid fuel remain unpriced, so these cash reports do not establish all-in
profit or campaign-length balance.

Final evidence:
[contributed career](verification/v0.12-final/career-report.json) and
[purchased career](verification/v0.12-final/purchased-career-report.json). Both
persisted reports reconcile exact lifetime totals while retaining 64 recent
entries; their monetary event counts are 1,570 and 1,571 respectively. The comparison and its recording-period
qualifications are detailed in [COMPANIES.md](COMPANIES.md).

## A02 — Useful walks and physical autonomous recovery

The docs/code review found pedestrian routes could retain redundant road legs
where a supported, clear shortcut exists. The new authority adapter accepts the
ordinary owned movement intent, then removes redundant pedestrian route legs only
where the complete local corridor is
supported and clear. The shared command budget bounds support queries; candidate
and distance limits bound shortcut search. Terrain support, body sweeps and
source/facility clearance remain mandatory. Unknown ground or an obstructed
corridor retains the original route. The planner changes waypoints; actual steps
still use the normal movement resolver and speed. This makes the existing
Go-to-market and continued-play goals more practical without changing their
reach, inventory or ownership requirements.

The candidate native pickup-to-market approach was about 77 m away but retained
a 261.8 m route. Geometry review confirmed that the direct shortcut candidates
crossed the parked pickup. Keeping the existing route respected the clearance
rules. This observation does not demonstrate a native walking-distance reduction;
the shorter unobstructed routes remain focused automated evidence. The implemented
planner removes valid redundant legs within its bounds; the candidate review did
not establish a new route around every obstructing corner.

A separate saved queue exposed a policy mismatch: patrol vehicles lacked the
checked temporary detour already available to Move vehicles. Patrol now uses
that same supported detour while retaining its cursor, junction rules and contact
checks. The original saved fixture and original delivery remain the test subject.

| Historical fixture | Physical return time in final run | Replay window |
| --- | ---: | ---: |
| 300-second rear-trap save | **34.0 seconds** | 120 seconds |
| 420-second occupied-exit save | **49.5 seconds** | 120 seconds |

Each replay verifies return of the original 12-wood load to its original source,
an empty courier at the transfer, exact per-item and money conservation on every
step, and identical save/resume continuation. The fixtures conserve 770 item units
and 1,500.00 credits. The unchanged 120-second window gives the result its scope;
a separate unsupported-source case must continue holding its load. This closes
the reproduced queue, while arbitrary dense road networks and fully blocked
exits still require scenario-specific acceptance.

See [routing](CONTACTS_AND_ROUTING.md), [vehicle dynamics](VEHICLE_DYNAMICS.md)
and the [historical regressions](../crates/terra-sandbox/tests/traffic_recovery.rs).
The [final traffic recovery report](verification/v0.12-final/traffic-recovery.json)
records both exact replays.

## A03 — Content that degrades within explicit scene limits

Per-model validation did not prevent an accepted model from multiplying costs
across many actors, or account for outgoing assets while their replacements
loaded. The validated pack renderer now admits aggregate actor-scene reservations
before creating replacements. Quality presets bound instances, expanded geometry,
primitives, animated nodes/channels, retained texture-slot pixels, concurrent
model loads and estimated payload.

Selection favors the visible active body, owned bodies and nearby actors. Stable
distance bands, incumbency and hysteresis reduce churn. Deferred, distant or
failed imports retain procedural presentation on the same authoritative body.
A selected model loads lazily; installation alone does not request every model.
Per-binding animation remains independent for actors sharing immutable model data.

Reservations include outgoing scene instances through teardown and every retained
model while loading or while supported glTF/PBR subassets remain alive. This
prevents a camera move from spending an outgoing model's allowance prematurely.
Accounting also includes texture-slot aliases and vertex expansion from generated
flat normals. Lower budgets can require asynchronous retirement. If immutable
pack snapshots alone exceed a lowered byte limit, retirement cannot satisfy it;
the limit must rise or the application must restart with a smaller pack.

Developer diagnostics expose selected/reserved totals, deferred bodies,
loading/retiring models and estimated-byte peaks. Direct game-owned scene overrides
without validated metadata remain explicitly unmetered. These counters and
estimates do not measure actual GPU allocation, driver reclamation, shadow-pass
cost or frame time. Target-device profiling remains necessary.

The original five-species animated wildlife pack, authoring script, character
customization and procedural fallbacks remain available. Original GLB bytes and
pack identity are unchanged. [ASSET_PACKS.md](ASSET_PACKS.md) states the exact
presets, accounting formula and format profile. The
[final asset acceptance report](verification/v0.12-final/asset-pack.json) accepted
all five models. The complete Bevy suites exercised asynchronous retention,
failed lazy loads, priority replacement, budget changes and actual
scene-to-fallback restoration. Model acceptance and these tests do not establish
target-GPU performance.

## A04 — A source package developers can validate and hand off

Adjacent bundles already checked seams and exact regional coverage. The missing
workflow was a portable, inspectable handoff that preserved identity and declared
what the receiving developer actually had.

`terra-sources inspect/export/verify/install` now inventories referenced members,
validates them with the native/host importer, and carries declared extent,
per-layer provenance/licensing, exact file hashes and missing-tile results.
Export/install require a new directory. Copied manifests keep their relative
paths, and the copied provider must retain its exact source identity. A complete
canonical package index is published last. Existing saved revisions and
configurations are not rewritten.

The final CLI roundtrip used the unchanged authored connected valley: two
bundles, seven source files, 115,316 bytes and 81 fully covered level-15 tiles at
resolution 65. Export, verify and install produced identical inspection content
and package-index bytes. Package identity is distinct from saved-world source
identity and can be supplied as a trusted expected digest at admission.

Coverage checks every intersecting tile, so a partly covered outer tile remains
missing. Dense grayscale has no NoData-mask or sentinel interpretation. Developers
must prepare genuinely missing regions explicitly. The tool rejects oversized
inventories, forged aggregate declarations, traversal, links, special files,
platform-invalid names, case collisions and reserved publication aliases before
admission. Inputs and output directories must remain quiescent during the operation.
A content hash provides integrity; publisher trust and actual redistribution
rights remain the receiving developer's responsibility.

[SOURCE_BUNDLES.md](SOURCE_BUNDLES.md) contains the working commands and SDK API.
The accepted [inspection](verification/v0.12-final/source-inspection.json),
[export](verification/v0.12-final/source-export.json),
[installation](verification/v0.12-final/source-install.json) and
[verification](verification/v0.12-final/source-verify.json) reports preserve the
same source and package identities. This
workflow distributes the supplied authored region; it does not create licensed
worldwide terrain or establish surveyed/historical accuracy.

## A05 — Live sessions and recovery in one declared workload

The completed integrated acceptance workload combines actual TLS clients with prepared
terrain and dormant mutable storage. It keeps safely admitted crew placement;
the earlier flat fixture had collapsed each owner's bodies onto one point.
The final run used **16 local clients, 182 living bodies (80 owned), 5,248 resource
nodes and 25 resident source chunks**. Eight churn cycles contained 400 active
and 200 half-disconnected fixed steps per cycle, and 1,024 distant cold regions
holding 4,096 depleted resource records. Those cold overlays exercise storage;
they do not extend the local terrain's coverage or simulate a populated world in
every region.

The run verified exact disconnected/resumed/restarted owner state,
128 durable crafting acknowledgements, actual ping timings, a failed SQLite write
with unchanged authority/tick/generation, and a verified backup reopened with
exact full state and private metadata. The same authenticated principals and
command sequences resumed from the recovered database. Removing a source tile
stopped physical movement; restoring it restored movement. The **4,840 fixed
steps completed in 131.962 wall seconds**, including those interruption checks.

Advancing host updates measured **46.691 ms p95 / 57.483 ms p99**. The 128 durable
acknowledgements measured **108.879 ms p95**; 6,464 local TLS pings measured
**58.887 ms p95**. Storage paused authority on 2,119 update calls. Verified recovery
preserved generation **165** across **1,051 nonempty regions**, followed by
authenticated continuation to generation **169**. Process RSS increased from
**36.04 to 100.27 MiB** across the measured admission/recovery scope.

The accepted [combined sessions](verification/v0.12-final/regional-sessions.json)
and separate [storage recovery](verification/v0.12-final/recovery.json) reports
retain their actual scopes. The latter has zero clients, no renderer and no
terrain import. [OPERATIONS.md](OPERATIONS.md) and [SCALING.md](SCALING.md)
document reproduction and the distributions. RSS includes the host and local
client fixtures in one process; these finite accelerated samples do not establish
a long-duration memory plateau, Internet service level or player capacity.

## A06 — Management that fits the current interface

The Fleet panel's shared summary and navigation could consume much of a small
window before the selected task appeared. The implementation now uses a compact
chooser, a shorter shared summary and visible selection across six sections.
Switching sections resets the unrelated scroll offset, while rescaling preserves
scroll position. The existing inspector width, terrain hit-test boundary,
keyboard focus system and forest palette remain the design context.

Finances leads with the result before owner funding and after known personal
costs. It then explains cash movement, owner funding, categories, coverage and
recent entries. Legacy opening balances and unpriced inputs are visible alongside
the result. This gives continued fleet play a concrete financial feedback loop
without inventing a complete asset valuation.

### Candidate native review and resulting polish

A 0.12 candidate session at **1100 × 700 and 150% text** exercised the compact
six-section chooser and actual company transactions. Its pre-polish executable
SHA-256 was `bf6817f0ed7ddb449f378347da2814afe56f430a50206eba31879001adc62626`.
Registration cost **20.00 personal credits**, a **50.00-credit deposit** moved
money into the company, and hiring cost **10.00 company credits**. The resulting
balances were **131.88 personal / 40.00 company credits**. Finances showed
**−10.00 before owner funding** and **−30.00 after known owner-paid costs**,
correctly separating the deposit from the recruitment and registration costs.

The same review found repeated empty Finances details before registration.
The polished view now gives one registration prompt and an immediate **Manage
company** action. Detailed cash, funding, category, coverage and recent-entry
groups appear only after a company exists; the empty shared summary is shorter.

Returning from F2 possession to F1 command mode at a heading of **280.3°** also
left the body offscreen because the overview offset retained the possession yaw.
The body-focused return to RTS now restores zero yaw with its overview offset
and pitch. A location/atlas Release keeps its explicit spectator destination and
orientation. This is a presentation correction; identity, possession authority
and physical state keep their existing rules.

The candidate session saved and quit cleanly at **generation 286**, with
**57 ready imported actors and seven distance-deferred procedural fallbacks**;
no model, world or simulation errors were reported. Those observations predate
the final polish and are development evidence. The subsequent complete demo
test run passed **102 tests, zero failed and zero ignored**. Final native review
then confirmed the polished empty-state keyboard action, body-focused camera
handoff, compact Finances layout and exact save/reopen continuity on the frozen
executable. Its separate evidence is recorded below; no previous release capture
is used to validate the new layout.

## Cross-review corrections and evidence integrity

Independent review found and closed defects beyond the initial feature list:
portable source names could collide on another platform; a forged index could
defer aggregate rejection until after excessive reads; an expected storage failure
could hide another authority error; and the funding label could be mistaken
for net owner investment. The accounting hot path also avoids cloning an unchanged
journal on sub-credit payroll ticks while retaining immediate capacity holds.

The verification runner captures command output privately and atomically publishes
logs and accepted JSON. Recovery/session sidecars must agree with captured stdout.
Published report and log hashes are checked independently before acceptance, and
source hashes must remain unchanged. Interrupted commands now stop their owned
process group before cleanup; the actual descendant-cleanup check passed. The
runner preserves distinct test identities and the exact Cargo executable artifact.
These checks make evidence reviewable without substituting for the final run.

Two earlier attempts remain separately archived: a build stopped when the
incremental cache exhausted disk space, and a subsequent run caught two household
fixtures that changed company cash/wages without recording known payroll. The
fixtures now record that payment explicitly; depot capacity setups use real
withdrawal/deposit commands. Production validation was not weakened. The
[validation record](VALIDATION.md#earlier-incomplete-attempts) distinguishes both
failed attempts from the complete fresh sequence.

## Final automated and native evidence

The [gate result](verification/v0.12-final/gates.json),
[327-input source manifest](verification/v0.12-final/source-hashes.json) and
[747 distinct test identities](verification/v0.12-final/distinct-tests.json)
establish the current automated result. The exact Cargo artifact SHA-256 is
`4b1a7ae522a51e321dad33fbfb7e3b83f098c3dd3d1b025825f39034ada2de95`;
[VALIDATION.md](VALIDATION.md) records commands, timings and provenance.

**Final native acceptance passed.** The [native record](native-v12-verification.json)
retains 14 captures from three clean processes of this exact executable; all 327
source inputs stayed unchanged. Linux X11/Xvfb with software Vulkan/llvmpipe,
keyboard/mouse input and 150% text supported the reviewed 1440 × 900 and
1100 × 700 windows. The concise empty state exposed **Manage company** without
scrolling, showed keyboard focus and opened Company. Six-section navigation,
scroll behavior and body-focused RTS framing were confirmed.

Final Finances inspection of the saved paused company confirmed **40.00 company
credits**, **131.88 personal credits**, **−10.00 before owner funding** and
**−30.00 after known owner-paid costs**. Its three-entry journal retained the
20.00 registration, 50.00 deposit and 10.00 recruitment amounts. These final
checks reviewed persisted accounting; the original transaction clicks remain
candidate-session evidence. Activities retained 50 XP, one completed contract
and the housing goal.

The empty-world session exited at generation **25**. The company session exited
at **320**, reopened to checkpoint **341**, and exited again at **346**, all with
code zero. All 12 named identity, inventory, accounting, progression, ownership
and world continuity checks passed. The original 0.11 database was unchanged.
Five models produced **57 ready actors / seven distant fallbacks** in the reviewed
scene and **59 / five** in resume telemetry; selection varies with distance.
No model, world, simulation or persistence errors were reported. These native
checks establish the inspected interface and continuation behavior, while
physical GPU frame time, other operating systems and controller/wheel or
screen-reader qualification remain unmeasured.

## Remaining external qualification

Campaign-length balance still needs representative supply, demand, utilization,
capital and labor valuation. General traffic acceptance needs additional dense
networks and obstruction scenarios. Geographic expansion needs actual licensed
datasets, preparation/update policies and explicit saved-world compatibility.
Deployment claims need sustained active populations, Internet conditions and
host measurements. Hardware GPUs, physical controllers/wheels, screen readers
and non-Linux platforms need direct testing on those devices and environments.

The implemented controls, reproducible cases and developer workflows make those
next assessments concrete. Their bounded evidence retains its scope throughout
this review.
