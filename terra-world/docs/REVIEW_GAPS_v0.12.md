# Documentation-led gap register — 0.12

Review date: **2026-10-28**. This pass begins with the six remaining acceptance
areas in the [0.11 gap register](REVIEW_GAPS_v0.11.md), traces their concrete
causes in code, and gives each change a behavioral or operational check.
The [0.11 product review](PRODUCT_REVIEW_v0.11.md) and
[validation](VALIDATION_v0.11.md) remain historical evidence.

The [final automated run](verification/v0.12-final/gates.json) passed all **20
gates**, with **747 distinct Rust tests**, **327 unchanged inputs** and matching
published log/report hashes. Final native acceptance also passed in three clean
sessions of the exact polished executable, with 14 retained captures and exact
save/reopen checks. Candidate observations below retain their pre-polish scope.

## Findings and implemented scope

| Area | Finding from the docs and code | Correction and evidence boundary |
| --- | --- | --- |
| A01 — Company economics | Revenue and payroll did not reconcile all company cash movements. The earlier delivery-window result omitted setup, recruitment and acquisition costs. | Persisted categorized cash records, imported opening balances, owner-cost attribution and Fleet Finances; paired measured careers using contributed and purchased stock. This establishes cash accounting, not an entire campaign's balance or unknown capital valuation. |
| A02 — Navigation and autonomous continuation | Ordinary walks could follow a long road perimeter for a nearby destination. Patrols lacked the safe local detour already available to Move vehicles, trapping an actual saved courier queue. | Bounded supported walking shortcuts in native/server command dispatch; the same checked temporary detour for patrols. Exact historical replay cases require physical return, conservation and identical save/resume continuation. |
| A03 — Aggregate content admission | Per-model import limits did not bound the total visible scene or account for resources still loading or retiring. | Configurable aggregate validated actor-scene reservations, lazy model loading, stable priority, bounded concurrent loads, procedural fallback and measured diagnostic counters. Estimated payload bounds are separate from physical GPU memory and hardware frame-rate qualification. |
| A04 — Prepared-source distribution | Adjacent bundles were validated, but developers lacked a reproducible portable handoff with declared coverage and provenance. | Offline inspect/export/verify/install CLI and SDK, deterministic package index, exact tile coverage and exclusive publication. Authored demonstration data does not establish licensed worldwide terrain coverage. |
| A05 — Operational recovery | Current backup correctness and older real-session measurements were separate; the live fixture also collapsed each crew's bodies onto one point. | Real TLS sessions on prepared terrain with safe crew placement, large cold-overlay storage, actual writer failure, terrain interruption/restoration, verified backup and authenticated continuation. Finite local measurements retain their actual workload and host limits. |
| A06 — Product accessibility and qualification | Fleet summary/navigation consumed the small window, selected sections were not highlighted, and section switches could retain an unrelated scroll offset. Candidate play also exposed repeated empty finance details and a body-focused RTS return retaining possession yaw. | Compact section chooser, concise empty state with a direct company action, explicit finance coverage and a consistent body-focused overview. Final native review confirmed the fixes at 1100 × 700 with 150% text, including keyboard action and exact save/reopen continuity. Physical controller/wheel, other OS and representative GPU qualification remain separate. |

## Traceability

### A01 — Reconciled cash and honest career results

`TransportCompany::accounting_report()` separates operating, capital and
financing flows. Exact lifetime category totals reconcile to opening/current
company cash; the newest 64 monetary entries remain available. Known personally
paid registration and costs for currently assigned vehicles are explicit and do
not change company cash. Owner deposits are financing, never revenue. In-kind
stock and preassignment vehicle costs have no invented price.

Absent accounting in a compatible older save begins at its actual imported
balance, with earlier revenue/payroll retained as historical counters. Complete
save and regional validation reject malformed records. Preparation occurs before
cash/stock/escrow changes. Counter exhaustion holds work or rejects the transaction;
it cannot saturate, reset history, fabricate a refund or permit unrecorded wages.
A full monetary history can still refund a load already at pickup directly from
escrow; distant paid travel requires accounting capacity. The declared lifetime
limit requires a compatible migration before further cash postings.

Trace: [accounting](../crates/terra-sandbox/src/transport/company/accounting.rs),
[company commands](../crates/terra-sandbox/src/transport/company/commands.rs),
[simulation](../crates/terra-sandbox/src/transport/company/simulation.rs),
[Finances](../crates/terra-demo/src/fleet.rs),
[accounting regressions](../crates/terra-sandbox/tests/company_accounting.rs),
and [paired careers](../crates/terra-sandbox/tests/company_depots.rs).
[Companies](COMPANIES.md) explains categories, legacy history and the distinction
between a delivery window, since-registration cash and complete profit.

The final [contributed](verification/v0.12-final/career-report.json) and
[purchased-stock](verification/v0.12-final/purchased-career-report.json) careers
each completed six sessions, 24 deliveries and seven restores over 2,967.45 physical
seconds. Results before financing were −64.69 / −141.49 credits; after known
personally paid costs they were −84.69 / −161.49. Both final wallets were 13.93
credits because purchased input returned 76.80 fewer credits to the owner. Exact
lifetime accounting and conservation passed; unknown input/labor/capital costs
remain unpriced.

### A02 — Shorter valid walks and exact queue recovery

The authority adapter accepts the existing owned `MoveUnits` intent first, then
removes redundant pedestrian route legs only through a completely checked local
corridor. A command has a shared 16,384 support-query limit, at most eight shortcut
candidates per leg and a 160 m shortcut distance. Nine support points at 0.25 m
intervals, source/facility checks and body sweeps preserve admission. Unknown,
sliding or insufficient support retains the original route. Planning changes no
position or money; actual movement still uses the normal resolver and speed.

The candidate native pickup-to-market approach retained a 261.8 m route for a
goal about 77 m away. Geometry review found the direct shortcut candidates crossed
the parked pickup, so the retained route respected clearance. This case is not
evidence of measured native walking reduction or a general corner-routing planner.
Shorter supported corridors are covered by the focused domain/adapter regressions.

Patrol vehicles use the existing supported temporary detour prefix while keeping
the patrol cursor and all junction/contact rules. Two original 0.10 saved queue
fixtures retain their exact provenance. Each checks the original delivery's
return event and source transfer, an empty courier at transfer, exact per-item
conservation on every step and save/resume equivalence inside the unchanged
120-second replay. A separate unsupported-source case must remain held.

The [final replay report](verification/v0.12-final/traffic-recovery.json) records
physical returns at 34.0 seconds for the rear trap and 49.5 seconds for the
occupied exit, preserving 770 item units and 1,500.00 credits in both cases.

Trace: [walking](../crates/terra-sandbox/src/living/walking.rs),
[autonomy](../crates/terra-sandbox/src/living/autonomy.rs),
[native dispatch](../crates/terra-bevy/src/sandbox.rs),
[host dispatch](../crates/terra-net/src/server.rs),
[walking regressions](../crates/terra-sandbox/tests/walking_routes.rs),
and [historical traffic regressions](../crates/terra-sandbox/tests/traffic_recovery.rs).
This resolves the reproduced queue; it does not claim every possible road network
or traffic obstruction is solvable without intervention.

### A03 — Bounded imported presentation

`ActorSceneBudget` supplies quality defaults or explicit game limits. Selection
prioritizes visible active/owned actors, then stable distance bands and IDs, with
incumbency/hysteresis. Geometry and animation count per admitted instance;
texture slots and model payload count once per retained model. Pack snapshots,
loading reservations and retiring resources remain charged until the asset
lifecycle permits release. Lowering a limit may require an explicitly reported
drain before new resources fit. If immutable pack snapshots alone exceed the
byte limit, that floor persists until the limit is raised or a smaller pack is
installed on restart.

The importer accounts for aliased texture slots and generated-normal geometry.
Per-binding animation policy remains separate when multiple actors share one
model. Failed, distant, hidden or budget-deferred replacements retain the existing
visibility/fallback contract. Arbitrary game-owned scene overrides are reported
as unmetered rather than given invented geometry counts.

Trace: [scene admission](../crates/terra-bevy/src/render_budget.rs),
[pack installation](../crates/terra-bevy/src/render_pack.rs),
[scene lifecycle](../crates/terra-bevy/src/render_scenes.rs),
[import validation](../crates/terra-bevy/src/asset_pack.rs),
and [diagnostics](../crates/terra-demo/src/performance.rs).
See [asset packs](ASSET_PACKS.md) for exact limits and lifecycle semantics.

### A04 — Portable prepared sources

`terra-sources` inspects the referenced file graph, provenance and coverage;
exports only those files with a bounded canonical index; verifies exact bytes;
and installs to a new directory while retaining source identity after rebasing.
Coverage includes partial edge tiles as missing unless completely covered.
Dense grayscale input has no inferred NoData mask; the workflow says so explicitly.

Validation bounds index/member/aggregate size before expensive work, rejects
traversal, links, portable-name collisions and reserved publication names, and
requires the expected package/source identity when supplied. A content hash is
not a publisher signature. Input/output directories must remain quiescent during
installation; the tool is not an adversarial concurrent-filesystem sandbox.

Trace: [distribution SDK](../crates/terra-stream/src/source_distribution.rs),
[CLI](../crates/terra-stream/src/bin/terra-sources.rs),
and [source workflow](SOURCE_BUNDLES.md). The connected original fixture has two
child bundles and 81 fully covered level-15 tiles; distribution does not expand it.
The [final inspection](verification/v0.12-final/source-inspection.json) records
seven files and 115,316 bytes. The export, installation and verification reports
retain identical inspection content and exact source/package identities.

### A05 — Sessions and recovery in the same workload

The extended `regional_session_load` example uses the same resident source
surface as the host and leaves real new-principal placements intact. Its optional
cold fixture stores four depleted records per distant region; these overlays
exercise dormancy and recovery without implying terrain coverage at their locations.

Acceptance checks ownership/inventory/cargo while disconnected, authenticated
resume and restart, a failed SQLite write with unchanged authority/tick/generation,
source removal with stopped movement and restoration with resumed movement,
verified backup bytes/generation and exact full host/private metadata on recovery.
The recovered host must admit the same capabilities and durable command sequences.
The host includes storage-completion failure in its domain-error count. Only the
exact injected lock failure at an idle barrier is allowed; unrelated/additional
errors or authority work during that failure reject.
Actual ping and durable-command acknowledgement distributions are reported.

The [accepted final workload](verification/v0.12-final/regional-sessions.json)
used 16 local TLS clients, 182 living bodies (80 owned), 5,248 resource records,
25 resident source chunks and 1,024 cold regions. All 4,840 fixed steps completed
in 131.962 wall seconds. The 128 durable acknowledgements measured 108.879 ms p95;
6,464 pings measured 58.887 ms p95. Exact backup/recovery preserved generation 165
across 1,051 regions, then authenticated continuation reached generation 169.
These are finite local measurements, with no renderer or Internet conditions.

Trace: [live workload](../crates/terra-net/examples/regional_session_load.rs),
[prepared surface](../crates/terra-net/examples/support/prepared_session_surface.rs),
[storage recovery](../crates/terra-persistence/examples/recovery_probe.rs),
and [release verifier](../scripts/verify.py). See [Operations](OPERATIONS.md) and
[Scaling](SCALING.md) for reproduction, scope and measured results.

### A06 — Readable management and current native evidence

Finances leads with the result before owner funding and after known personal
costs. Detailed category, coverage and recent-entry explanations follow. The
compact chooser uses existing buttons/focus behavior without changing the
300-pixel inspector or terrain hit-test boundary. Section switches reveal their
beginning; rescaling preserves scroll. Summary and selected-state changes retain
the established forest palette and prototype art.

The pre-polish candidate review at 1100 × 700 and 150% text exercised the compact
six-section chooser and real registration, funding and hiring. Spending 20.00
personal credits to register, depositing 50.00 and spending 10.00 company credits
to hire left 131.88 personal / 40.00 company credits; Finances correctly showed
−10.00 before owner funding and −30.00 after known owner-paid costs.

Review found that an unregistered company repeated empty finance sections. The
polished view gives one registration prompt with **Manage company**, hides the
detailed groups until a company exists, and shortens the empty shared summary.
A separate F2→F1 return at heading 280.3° left the body offscreen. Body-focused
RTS/Release now restores zero yaw with the overview offset/pitch; an explicit
location/atlas spectator destination and orientation remain intact.

The candidate saved and quit at generation 286, with 57 ready imported actors,
seven distance-deferred fallbacks and no reported model/world/simulation errors.
The complete final demo gate passed 102 tests with zero failures or ignored
cases. The separate [final native acceptance](native-v12-verification.json)
passed with three clean processes of the gate-built executable and 14 captures,
at 1100 × 700 and 1440 × 900 with 150% text. It confirmed the empty-state keyboard
action, compact navigation, body-focused RTS framing and the saved company's
40.00 / 131.88 balances and −10.00 / −30.00 cash results. The original registration,
deposit and hiring clicks remain pre-polish candidate evidence.

All 12 named continuity checks passed across company exit generation 320,
reopened checkpoint 341 and second exit 346; the empty-world session exited at
25. The original 0.11 database remained unchanged. The reviewed scene had five
models, 57 ready actors and seven distant fallbacks; resume telemetry ended at
59 ready and five distant fallbacks. All sessions exited with code zero and no
model/world/simulation/persistence errors. This was Linux software Vulkan with
keyboard/mouse input, not physical GPU or cross-platform qualification.

Trace: [Fleet](../crates/terra-demo/src/fleet.rs),
[layout](../crates/terra-demo/src/interface.rs),
[camera handoff](../crates/terra-demo/src/control.rs), and [game flow](GAME_FLOW.md).
The native record names the executable, source, window sizes, text scale and
actual save/reopen observations. Virtual input tests cannot certify a physical
wheel/controller or a platform unavailable in the review environment.

## Independent review corrections

Reviewers worked across ownership boundaries after implementation. Source review
caught Windows-invalid names, case aliases of reserved publication names and
forged size declarations that could defer aggregate rejection until after excessive
reads. Those paths now reject during preflight. Operations review caught an
expected storage failure hiding another authority error; acceptance now permits
only the exact injected lock failure with one counted error and no authority
work, rejecting other errors. Finance review clarified that known owner funding is gross
before withdrawals. The accounting hot path avoids cloning an unchanged journal
on sub-credit ticks while retaining immediate capacity holds.

An integrated run also caught household test fixtures that authored wages without
recording the known payroll payment. The fixtures now include that receipt and
use valid cash transfers for depot capacity setup. Production validation stayed
intact. This failed attempt and an earlier disk-cache exhaustion are archived
separately in [validation](VALIDATION.md#earlier-incomplete-attempts); neither
contributes results to the final run.

## Evidence and remaining qualification

The current [validation record](VALIDATION.md) and
[product review](PRODUCT_REVIEW.md) contain the completed fresh automated and
[final native evidence](native-v12-verification.json), separately identifying
the pre-polish candidate review. This register does not reuse earlier release
counts or measurements as current results.

The remaining external work is explicit: licensed real-world dataset preparation
and distribution policy; campaign-length balance with game-owned capital/labor
valuation; arbitrary dense road networks; long-duration Internet/host performance;
and physical GPU, controller/wheel, screen-reader and non-Linux qualification.
Those need actual datasets, devices or deployment environments. Implemented
budgets and finite reference workloads must not be labeled as those certifications.
