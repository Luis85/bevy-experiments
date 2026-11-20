# Documentation-led gap register — 0.11

Review date: **2026-10-28**. This register traces product promises in the
documentation through the domain, native client, host and verification code.
It records the concrete corrections in the 0.11 source and the larger
acceptance work that remains outside those corrections.

**Fresh automated verification and scoped native confirmation are complete:**
12 passing gates, 700 distinct Rust tests and 313 unchanged source inputs. The
[0.11 validation record](VALIDATION_v0.11.md) and [gate report](verification/v0.11/gates.json)
retain the commands, complete logs, source manifest and executable identity.
The [native record](native-v11-verification.json) and
[release verification record](release-verification-v0.11.json) retain the observed
journey on that executable. The closing evidence summary separates those native
observations from automated regressions. Neither establishes the broader
acceptance claims in A01–A06, which remain open.

The [0.10 product review](PRODUCT_REVIEW_v0.10.md) and
[0.10 validation](VALIDATION_v0.10.md) remain historical evidence. Their test
counts, career measurements, screenshots and procedural-run provenance must not
be relabeled as 0.11 results.

## Priorities and scope

**P1** identifies a correctness, admission, authority or evidence defect that
can invalidate the affected flow. **P2** identifies a concrete continuation or
presentation defect that makes a valid flow misleading or unnecessarily blocks
it. Both groups below have implementation changes in this pass. The separate
remaining-acceptance section describes claims that those changes do not establish.

| ID | Priority | Affected flow | Reviewed gap |
| --- | --- | --- | --- |
| G01 | P1 | Activities → Market → buy/sell | The named seller and visible price were not carried through to authority. |
| G02 | P1 | F9 restore → next command/input | Actions queued for the old world could execute after successful replacement. |
| G03 | P1 | Drive → park → exit; buy/rent → enter/exit | Center placement and incomplete future-body checks could admit overlap or leave no usable exit. |
| G04 | P1 | Export GLB → validate → install | Header-only image checks, undeclared extension payloads and repeated mesh nodes bypassed parts of the declared import contract. |
| G05 | P1 | Verify → retain → review release evidence | Empty selected tests, inferred binary paths and incomplete failure finalization could produce misleading evidence. |
| G06 | P2 | Low food → obtain food → consume | Bread/recipe assumptions could hide a directly available meal. |
| G07 | P2 | Stranded company vehicle → paid service | Ineligible or fuel-empty service selection could hide valid repair recovery. |
| G08 | P2 | Earned wages → household purchase | An unusable nearest shop suppressed an eligible local exchange. |
| G09 | P2 | Animate during variable rendering cadence | Slow frames lost clip time; held/STEP rotation keys were not consistently normalized. |
| G10 | P2 | Open a connected prepared world | Child-center probing could reject valid coverage supplied jointly by several bundles. |

## Concrete corrections and traceability

### G01 — Preserve the seller and the reviewed personal-trade total

[GUIDANCE.md](GUIDANCE.md) promises that a displayed action is rechecked before
execution, while [SETTLEMENTS_AND_ECONOMY.md](SETTLEMENTS_AND_ECONOMY.md) describes
prices that change with stock. The old personal `Buy`/`Sell` commands carried no
price expectation. Generic Market opening also lost the seller selected by
guidance when several shops overlapped. A player could review one offer and
reach a different seller or authority price.

`GuideAction::OpenMarket { shop_id }` now preserves seller identity. The native
Market retains the rendered quote, quantity, seller, player and restore revision;
a missing selected seller does not silently fall back to another shop.
`LivingCommand::TradeQuoted` carries the exact total. Its shared read-only
`personal_trade_quote` preflight and execution check quantity, 3D reach, stock,
funds and receiving limits before committing. A stale total rejects atomically.
Legacy `Buy`/`Sell` remain explicit current-price APIs for embedding games.

Trace: [guidance](../crates/terra-sandbox/src/guidance.rs),
[trade authority](../crates/terra-sandbox/src/living/commands.rs),
[native Market](../crates/terra-demo/src/market.rs).
Regressions: [personal trades](../crates/terra-sandbox/tests/personal_trade.rs),
the Market module's rendered-price/context tests, overlapping-seller tests in
[guidance](../crates/terra-sandbox/tests/guidance.rs), and
`quoted_personal_trade_over_tcp_rejects_unseen_total_and_prevents_replay` in
[TCP loopback](../crates/terra-net/tests/loopback.rs).

The additive command is accompanied by wire protocol **8** and TLS ALPN
`terra-world/8`; older protocols reject during admission. Domain save version
one remains unchanged. See [protocol](../crates/terra-net/src/protocol.rs),
[TLS configuration](../crates/terra-net/src/tls.rs) and
[legacy admission tests](../crates/terra-net/tests/frontier_protocol.rs).

### G02 — Treat successful restore as a boundary for old input

[OPERATIONS.md](OPERATIONS.md) describes restore as an authority replacement.
Checking a displayed quote's restore revision is insufficient once a valid
command is already queued: the same shop and price can also exist in the saved
world, including under a different selected principal.

Successful local JSON or regional Load now retires the remaining
`SandboxQueue` intents and ends that dispatch pass. The native control layer
also retires its bounded control/UI backlogs and transient drag, interaction,
order and placement state once when the player/restore context changes.
Failed restore preserves actions for the still-current authority; ordinary
world revisions and newly issued actions do not trigger this discard.

Trace and regressions: [sandbox dispatch](../crates/terra-bevy/src/sandbox.rs),
including `successful_json_restore_retires_quoted_trade_and_control_from_previous_principal`,
the regional equivalent and failed-load preservation; and
[native control](../crates/terra-demo/src/control.rs), including
`restore_retires_native_backlogs_and_transient_orders_without_eating_new_actions`.
Focused native backlog and changed-principal restore checks passed before the
integrated release run; final source evidence is linked in the validation record.

### G03 — Admit a complete dismount and usable vehicle delivery position

The historical reference behavior placed an exiting avatar at the vehicle
center. That does not establish a supported, unoccupied exit beside a rotated
vehicle. The traffic review also found that checking an exit's dimensions
without its outgoing orientation could miss a crosswise nose or falsely block
lateral clearance. Buying or renting directly at a garage center could create
an overlapping vehicle before the first drive.

Grounded dismount now plans a complete avatar footprint and corridor, including
other bodies, dormant bodies, structures and source support, before changing
possession or parking state. Traffic exit admission uses the actual projected
outgoing orientation. Purchase/rental delivery now searches bounded nearby
garage bays for the model footprint and walking clearance, with terrain,
body, structure and facility checks before the financial transaction.
Unsupported or occupied placement leaves the previous state intact.

Trace: [dismount](../crates/terra-sandbox/src/transport/boarding.rs),
[garage placement](../crates/terra-sandbox/src/transport/parking.rs),
[body admission](../crates/terra-sandbox/src/living/contacts.rs),
[traffic admission](../crates/terra-sandbox/src/living/traffic.rs), and the
[native](../crates/terra-bevy/src/sandbox.rs)/[host](../crates/terra-net/src/server.rs)
dispatch adapters. Regressions:
[vehicle exits](../crates/terra-sandbox/tests/vehicle_exit.rs),
[garage placement](../crates/terra-sandbox/tests/garage_placement.rs),
[traffic](../crates/terra-sandbox/tests/traffic.rs), plus adapter dispatch tests.
Focused domain, native and TCP purchase/rental placement checks passed before the
integrated release run. Safe rejection of an occupied exit remains required behavior.

### G04 — Validate the payload and the instantiated scene

[ASSET_PACKS.md](ASSET_PACKS.md) declares a bounded, extension-free GLB profile
with preflight failure before installation. Merely recognizing PNG dimensions
did not establish that compressed image data could decode. Undeclared extension
payloads escaped declaration-list checks, and repeated scene nodes could draw
one mesh many times while staying within declared-mesh counts.

Preflight now decodes every embedded PNG with bounded dimensions, aggregate
pixels and decoder allocations, rejects extension payloads throughout the
document while leaving `extras` opaque, and counts instantiated scene geometry
as well as declared mesh geometry. These checks retain the immutable validated
bytes and existing renderer fallback contract; they do not add world-wide
graphics admission.

Trace: [asset importer](../crates/terra-bevy/src/asset_pack.rs).
Regressions in [asset-pack tests](../crates/terra-bevy/src/asset_pack_tests.rs):
`embedded_png_payloads_are_decoded_and_corruption_is_rejected_in_preflight`,
`undeclared_extension_payloads_are_rejected_but_extras_remain_opaque`, and
`repeated_mesh_nodes_count_toward_instantiated_geometry_budgets`.

### G05 — Make fresh verification reproducible and failures inspectable

Release review needs evidence tied to the inputs and executable actually
checked. The shipped shell sequence omitted asset acceptance, recovery, native
artifact identity and source continuity. Cross-review of the new runner then
caught reporting defects: a selected test that matches zero cases is not a career measurement;
an assumed `target/debug` path does not identify a custom-target build; and an
exception during final hashing must not leave a completed run labeled running.
Filtering absolute path components also previously omitted source files when a
checkout's parent directory happened to be named `target`.

The checked-in [verifier](../scripts/verify.py) runs the complete gate sequence
fresh, including the procedural integration test, without importing a previous
result or skipping that case. It requires nonempty identified test execution,
the exact career test and one valid measurement, hashes the native executable
reported by Cargo, and checks the source manifest before/after the run.
Exclusive report-directory claims, report-relative log references, startup
diagnostics and failed finalization protect reviewable evidence.

The first integrated attempt correctly rejected an incomplete Bevy output log
despite its zero command exit status. Its
[failed evidence record](verification/v0.11-incomplete-capture/README.md) is
retained separately. Command output is now captured privately, parsed and hashed
from those exact bytes, then published atomically. Finalization verifies that
published logs still match their captured hashes. Interrupted commands retain
their available output; atomic JSON publication keeps status records complete.
Every release gate ran again after this change and passed.

Trace: [shell entry point](../scripts/verify.sh) and
[CI wrapper](../.github/workflows/ci.yml). Isolated mocked subprocess/temporary
fixture review covered zero/missing career records, custom-target versus stale
binary selection, source-read failure, startup failure, source-path exclusions,
portable log references and competing directory claims. Those checks establish
runner behavior only. The separate fresh full-gate output establishes the
automated 0.11 result recorded above.

### G06 — Choose an available food path and retain recipe alternatives

The Suggested journey's urgent-needs promise did not hold when bread had no
available supply but berries or cooked fish did. Holding all recipe ingredients
also caused an early workbench suggestion even when a finished item was sold
nearby and processing was unavailable.

Guidance now compares the supported foods in the current catalog/view and
prefers ready actions and direct supplies before ingredient preparation.
Missing/distant workbenches remain fallbacks after viable finished-item sellers.
The bounded resource search, real inventory, permissions and normal commands
remain authoritative; no food is created or consumed merely by opening a goal.

Trace: [GUIDANCE.md](GUIDANCE.md),
[acquisition logic](../crates/terra-sandbox/src/guidance.rs), and the hungry-food,
finished-item/workbench and overlapping-market cases in
[guidance tests](../crates/terra-sandbox/tests/guidance.rs).

### G07 — Offer eligible roadside recovery, including repair alone

The Fleet recovery flow could select a facility that the roadside authority
would not accept, or block repair when no fuel could be supplied. Native offers
now select an eligible Garage and can quote repair with zero litres. The
displayed service still uses the existing company quote, finite fuel, reach,
ownership and cash checks; it does not move the vehicle or erase its load.

Trace: [Fleet offer and displayed quote](../crates/terra-demo/src/fleet.rs),
[roadside authority](../crates/terra-sandbox/src/transport/company/roadside.rs),
and `roadside_selects_an_eligible_garage_and_can_offer_repairs_when_fuel_is_empty`
in the Fleet module. The recovery contract remains documented in
[COMPANIES.md](COMPANIES.md).

### G08 — Select a feasible household purchase before choosing nearest

Household circulation selected the nearest stocked shop before checking the
complete item-and-pass price and receiving cash capacity. An unaffordable or
capped shop therefore suppressed a valid purchase at another local shop.

Selection now filters eligible exchanges first and then chooses nearest, for
both current and departed employee wallets. A capped service treasury still
blocks the entire exchange. Stock, cash, recorded wage spending and consumed
material retain their exact accounting; prices and wages were not retuned.

Trace: [household economy](../crates/terra-sandbox/src/transport/company/economy.rs),
[household regressions](../crates/terra-sandbox/tests/households.rs), and
[COMPANIES.md](COMPANIES.md). Tests cover fallback, blocked alternatives,
conservation and save/resume validation.

### G09 — Keep animation time consistent across render cadence

Clamping a slow render frame's animation delta lost time relative to physical
movement. STEP and held final rotation keys also needed the same unit-quaternion
contract as interpolated keys. The animator now advances by the full virtual-time
delta with a double-precision phase accumulator and normalizes all applied
rotations. Existing pause and independent per-binding policies remain in place.

Trace and regressions: [animation](../crates/terra-bevy/src/render_animation.rs),
especially `animation_time_and_blends_match_across_slow_and_fast_render_frames`
and `step_and_held_final_rotation_keys_apply_unit_quaternions`.
[ASSET_PACKS.md](ASSET_PACKS.md) records the supported behavior. This corrects
timing and transforms; it does not demonstrate target-device frame budgets.

### G10 — Find a valid start tile in the connected coverage union

Adjacent bundles can jointly cover a usable tile even when every child-center
candidate is unusable. Startup probing those centers alone could reject an
otherwise valid collection. `covered_focus` now has a bounded fallback over
source-boundary candidate tiles and checks their exact combined coverage,
including across the date line. Missing coverage is still rejected, never filled.

Trace: [SOURCE_BUNDLES.md](SOURCE_BUNDLES.md) and
[heightmap collections](../crates/terra-stream/src/heightmap_set.rs), including
`covered_focus_finds_joint_coverage_when_every_child_center_is_unusable` and the
existing concave-union/subpixel-gap regressions. This extends valid regional
startup selection; it does not enlarge the authored source data.

## Larger acceptance work that remains open

### A01 — Complete expense accounting and long-career balance

The [company model](../crates/terra-sandbox/src/transport/company/model.rs)
retains revenue, employee money and spending, but not a complete expense history
for fuel, service, acquisition and setup. It cannot supply a defensible all-in
profit report from those fields alone. The historical finite-career result in
[COMPANIES.md](COMPANIES.md) is a balance finding under its stated starting
capital, harvest, route and price assumptions, not proof of an indefinite career.

Acceptance requires persisted, reconciled expense history and representative
careers that account for setup, inventory acquisition, service, leases, payroll
and owner transfers separately. Fresh finite conservation/recovery evidence is
necessary but insufficient to establish profitable progression or setup-cost
recovery. This pass does not adjust prices to force a fixture profit.

### A02 — Recovery in dense navigation and saved queues

Shared vehicle handling, local detours, projected exits and safe garage bays
remain bounded local mechanisms. They do not guarantee clearing arbitrary
congested saved states. The
[historical occupied-exit diagnostic](verification/v0.10/traffic-exit-limit.json)
has not been established as solved by this pass.

The current native Market and pickup walking approaches completed, but used
perimeter detours. Ordinary route efficiency remains part of this acceptance
work alongside congested-queue recovery.

Acceptance requires replaying representative dense saved states with unchanged
terrain, contact, traffic and physical-delivery rules, documenting whether a
vehicle completes, waits legitimately or needs an explicit player recovery
action. New tests must retain meaningful time bounds and exact load/payment
accounting. See [CONTACTS_AND_ROUTING.md](CONTACTS_AND_ROUTING.md) and
[VEHICLE_DYNAMICS.md](VEHICLE_DYNAMICS.md).

### A03 — Per-model limits versus total scene and hardware budgets

The importer now bounds both declared and instantiated geometry **within one
model**, and performs bounded PNG decoding. Repeating accepted actors can still
multiply animation, draw, texture, shadow and scene costs. Decoder allocation
limits exclude other loaded models, mipmaps and GPU resources.

Acceptance requires representative visible populations and quality settings on
the intended devices, with measured CPU/GPU frame time, resident memory, loading
and fallback behavior. Define total scene limits or degradation policy from
those measurements before claiming a world-wide graphics budget. The five-model
example remains a working authoring/validation path, not a complete production
art library. See [ASSET_PACKS.md](ASSET_PACKS.md) and [PERFORMANCE.md](PERFORMANCE.md).

### A04 — Prepared regional coverage versus global source coverage

Connected raster sets validate seams, identity and exact coverage of supplied
regional data. The bundled source is authored synthetic content. Matching host
and client identity does not download missing source files or establish surveyed
accuracy, global ingestion/distribution, historical completeness or a policy for
disputed geography.

Acceptance for a larger world requires a declared source extent, licensing and
provenance, actual coverage/NoData checks, distribution and update policy, and
explicit compatibility or migration behavior for saved worlds. Do not replace
source identity to force new data onto an old save. See
[SOURCE_BUNDLES.md](SOURCE_BUNDLES.md) and [DATA.md](DATA.md).

### A05 — Representative live population and operational recovery

One bounded authority, regional cold-state preservation, localhost protocol
checks and the 1,027-nonempty-region recovery workload do not establish a concurrent
player capacity or production operational service level. Index-only point
counts, full simulated actors, live clients and storage regions are different
workloads. A generic archive also leaves host metadata validation to its adapter.

Acceptance requires representative active/dormant populations and session churn,
traffic/economy load, queue/latency and memory measurements, plus exercised
checkpoint failure, restart and backup recovery with private identity and complete
state preserved. Record the hardware, workload and timing scope. The current
generation/checksum protections do not provide distributed authority handoff.
See [SCALING.md](SCALING.md), [OPERATIONS.md](OPERATIONS.md) and
[recovery probe](../crates/terra-persistence/examples/recovery_probe.rs).

### A06 — Actual platforms, accessibility and physical input hardware

Automated layout/input checks and prior native captures do not establish screen
reader support, localization, all display scales, controller/wheel hardware or
target-GPU performance. Earlier software-rendered observations remain scoped to
their recorded environment.

Acceptance requires the complete create/resume → needs/Activities → Market →
drive/park/exit → save/restore journey on the intended platforms, including compact
windows, enlarged text, keyboard focus and the physical input devices being
claimed. Capture failures and corrections against the final executable. See
[CONTROLS.md](CONTROLS.md), [GAME_FLOW.md](GAME_FLOW.md) and
[PERFORMANCE.md](PERFORMANCE.md).

## Completed release-review evidence

1. **Fresh automated gates and source freeze.** The checked-in verifier completed
   all 12 gates with 700 distinct passing Rust tests and 313 unchanged source
   inputs. The [0.11 validation record](VALIDATION_v0.11.md) links complete logs, distinct
   test identities, source hashes, the fresh career measurement, recovery output,
   asset acceptance and native artifact identity. Historical results and the
   retained incomplete-capture attempt are excluded from this passing run.
2. **Native executable and display scope.** The
   [native record](native-v11-verification.json) and
   [release verification record](release-verification-v0.11.json) identify the
   frozen executable by SHA-256
   `a1e3391a81327c2a05b9e31bd442d8e9cb27b0b99cd09b6bc1f8e84cc5f15107`.
   The journey resumed a copy of the fictional 0.10 Riverbend Commons world as
   Mira Vale, at 1440×900 and then 1100×700 with 150% text. The asset diagnostic
   reported 64 ready, zero fallbacks and zero errors. These observations
   apply to the recorded environment, not all platforms or graphics hardware.
3. **Observed native transactions, movement and continuity.** Market GoTo reached
   an 11 m approach. Buying five water for the displayed 4.10 credits and selling
   one for 0.30 credits reconciled wallet and shop/player inventory exactly;
   keyboard focus was visible. The player walked within the 20 m vehicle-entry
   range, entered the pickup, drove briefly with W/Shift, and exited 2.09 m beside
   the parked vehicle with zero motion. F5, F9 and session Restore were exercised.
   Keyboard Save & Quit and a second Save & Quit after reopening both exited
   successfully, with checkpoints 209 and 230. Resume preserved the 201.88
   credit wallet, four water, profile, pose, progress (50 XP, one contract and one
   settlement), vehicle and source identity exactly.
4. **Explicit acceptance boundaries.** Food alternatives, failed-restore handling,
   roadside service, and garage purchase/rental placement are covered by fresh
   automated regressions; this record does not claim they were clicked through
   during the native journey. The [current product review](PRODUCT_REVIEW.md)
   retains these distinctions. Earlier release records remain historical, and
   A01–A06 stay open because this finite run does not satisfy their broader
   acceptance requirements.
