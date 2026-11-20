# Product review and improvement pass — 0.11

Review date: **2026-10-28**. This pass began with the product, design, integration,
operations and previous release documentation, then traced the stated behavior
through domain rules, native UI, network authority, rendering, sources and
persistence. Three subagents owned economy/guidance, vehicles/placement, and
content/source/operations; independent cross-reviews followed integration.

[VALIDATION.md](VALIDATION.md) records the final commands, exact source and binary
identity, test counts and native observations. The detailed
[gap register](REVIEW_GAPS_v0.11.md) connects each finding to source and regression
coverage. The [0.10 review](PRODUCT_REVIEW_v0.10.md) and
[0.10 validation](VALIDATION_v0.10.md) preserve the previous release's evidence.

## Outcome

The main improvement is reliability at the boundaries between already-built
systems. A goal now keeps the seller it recommends; a financial action keeps the
price shown; a successful restore retires old input; and a vehicle transition
requires usable physical space. Asset validation now checks what a renderer will
actually instantiate and decode. These changes make the existing reference game
more coherent without changing its currency, ownership, save authority or source
identity to manufacture progress.

This is **0.11.0**, with **wire protocol 8** and **TLS `terra-world/8`**. The new
personal quoted-trade command requires matching clients and hosts. Domain save
version **1** and regional database schema **1** remain; compatible prior worlds
still pass their ordinary catalog, source and complete-state validation.

## Player guidance and continued play

The documented promise was a useful next action derived from current world
state. Code review found two ways that promise failed: urgent food guidance could
commit to bread while another meal was available, and held ingredients could
force an unavailable workbench path before considering a finished-item seller.
The planner now considers supported foods and usable alternatives within its
bounded loaded-world scope. Gathering, crafting, buying and consuming still use
the existing inventory, ownership, needs and reach rules.

A second gap occurred after a correct recommendation. Generic Market opening
selected a nearby shop again and could lose the seller that actually had stock.
`GuideAction::OpenMarket { shop_id }` now carries the seller through the native
handoff. The action's intent comparison includes that identifier. An integration
regression follows a recommendation past an unusable nearer shop, opens the
selected seller and constructs a trade for that same shop.

Ordinary Go to market and Go to production now share the guided approach planner.
They require loaded ground and body clearance, including vehicle dimensions and
heading, instead of using an arbitrary offset from a service point. These are
bounded physical approaches; unavailable terrain and obstructed routes remain
honest blockers.

## Economy, ownership and financial presentation

Market rows show **total prices for the selected quantity**, seller stock,
carried stock and concrete reasons an action is unavailable. The player can
explicitly cycle sellers. If a selected seller disappears, the panel requires a
new choice instead of redirecting a purchase silently.

The UI retains the quote shown when labels refresh. Input checks its player,
restore serial, seller and quantity, then compares the current authoritative
preflight. The new `TradeQuoted` command carries the exact expected total into
authority. The host checks the price again before mutation, alongside stock,
payer funds, receiving-account limits, inventory capacity, quantity and 3D reach.
This closes both the render-to-click and enqueue-to-acceptance windows. Rejection
preserves goods, cash and metrics. Existing SDK `Buy` and `Sell` retain their
explicit current-price behavior for embedders.

Depot trades and roadside service already had authoritative quotes, but their UI
recomputed the quote on click. They now retain the displayed quote too. Roadside
selection filters for actual supplying Garages; an ineligible nearby Depot no
longer hides a valid provider. A needed repair can be offered with zero litres
when fuel is unavailable. The price and service contents remain visible and
must match at acceptance.

Household demand now checks complete affordability and the receiving shop's
account capacity before choosing the nearest eligible seller. A blocked local
shop can no longer suppress a valid alternative. The service treasury and both
employee-wallet lifecycles retain their exact conservation checks.

No prices, wages, rewards or deadlines were adjusted to improve a test outcome.
The fresh finite-career measurement is reported in the validation record. The
runtime still lacks a complete expense history for acquisition, setup, fuel and
service, so a displayed all-in profit figure would be unsupported.

## Vehicles, traffic and recovery

The old exit command placed the avatar at the vehicle center. Safe dismount now
searches twelve bounded side/end positions and checks the rotated avatar
footprint along the supported corridor. It considers living bodies, authored
structures and facility footprints, and requires a complete authoritative world.
The native and network hosts supply actual resident terrain/source support.
Moving, unsupported or boxed-in exits reject atomically; the player remains in
the same vehicle with its state intact.

This stricter check exposed a connected creation defect: purchases and rentals
also appeared at the garage center, overlapping the starter rover or one another.
Both paths now search at most 33 nearby parking candidates, with model-sized
support and a walking clearance strip. Admission happens before funds, station
fuel, bond, identifiers or bodies can change. Normal entry and dismount are
covered through both native and TCP command dispatch. Plain SDK calls retain
the documented flat-ground assumption; terrain-backed hosts use the grounded APIs.

Two geometric regressions also proved that outgoing intersection checks used
incompatible body orientations: one missed a crosswise collision, and one blocked
a clear lateral position. The corrected check uses the outgoing heading and
rotated body footprints while preserving margins and grade separation.

The handling model remains a bounded reference. Safe waiting at a genuinely
occupied exit is necessary, and these fixes do not prove unrestricted dense-road
navigation, full tire/suspension behavior, articulated-trailer physics or an
automatic escape from every queue.

## Content production, animation and terrain

The importer previously accepted PNG headers without proving that the compressed
pixels could decode. It now decodes accepted textures during preflight, with
dimensions, aggregate pixels and allocation bounds applied first. Actual glTF
extension payloads are rejected even when undeclared; opaque `extras` metadata
remains available.

Declared mesh totals alone did not bound a scene containing hundreds of copies
of the same mesh. Acceptance now reports declared and instantiated geometry
separately, and enforces the scene vertex/triangle ceilings across reachable
nodes. Unused content still participates in the relevant import budgets.

Animation no longer discards elapsed time above 100 ms. Slow and fast frame
partitions produce matching clip phase and blend progression without replaying
missed frames. Applied STEP and final held quaternion keys are normalized just
like interpolated keys. Pause, per-binding policies, procedural fallback and
first-person inheritance remain covered by the renderer tests.

Automatic prepared-source focus now considers bounded tile candidates beside
bundle boundaries when child centers fail. This admits a full tile covered by a
valid adjacent union, including across the dateline. Exact union coverage still
rejects holes. Explicit and saved focus, original raster/semantic data, wildlife
GLBs and their content identities remain unchanged.

These are stronger content admission contracts, not a world-level graphics budget,
an art-complete production library, licensed global coverage or historical truth.

## Restore safety and operational reproducibility

Cross-review found that checking a UI quote's restore serial was insufficient:
`Load` followed by a queued valid purchase could still spend in the restored world,
including a different saved principal with matching IDs and prices. A successful
local load now retires the remaining `SandboxQueue` and ends that command pass.
Failed restore preserves the existing world and continuation behavior.

The native client separately retires old UI/control backlogs and transient
interaction, drag, order and placement state when authority context changes.
Its regression uses 33 actions in each queue, exceeding the 16-action drain,
and covers same-player restore and principal replacement. Initial startup,
ordinary revisions and deferred camera adoption keep their intended behavior.
The network bridge and persisted host replay sequences are unchanged.

The original verification shell script omitted asset acceptance, the larger-world
recovery workload, native build identity and source continuity. The new
`python3 scripts/verify.py` runs the full suite freshly and preserves command
logs, distinct test identities, source hashes, career measurements and the exact
Cargo executable artifact. It rejects empty selected tests, changed source,
nonempty evidence destinations and incomplete career reports. Independent review
also exercised custom target paths, startup/finalization failures and concurrent
output claims. CI uses this same runner and retains its evidence.
The first integrated attempt stopped on an incomplete Bevy log. The hardened
runner captures output privately, publishes complete logs/status atomically and
verifies published log hashes before acceptance. The failed record is retained;
the final verification ran every command again and passed.

The storage probe remains a bounded storage/recovery workload with zero clients.
It checks complete dormant/private state, generation, corruption rejection and
source preservation. It does not measure a populated world's simulation or
promise distributed recovery or a latency bound for filesystem operations.

## Presentation, accessibility and acceptance boundaries

The forest-green reference interface, character customization, enlarged text,
activity chooser and visible keyboard focus remain the established design.
Market information now explains the actual transaction and its blocking reason.
Existing scrolling accommodates the longer rows; the final native record
identifies the sizes, scale and flows actually inspected.

### Final native observations

The exact frozen 0.11 executable reopened a copy of the prior acceptance world,
preserving Mira Vale's original customization and Willow Brook neighborhood.
At 1440×900 and then 1100×700 with 150% text, the reviewed Market approach
completed 11 metres from the seller. Buying five water units for the displayed
4.10 credits and selling one for 0.30 credits reconciled both sides of the
exchange exactly. Enabled actions, unavailable reasons, quantity totals,
scrolling and visible keyboard focus remained readable. The compact session
menu also scrolled its focused Save and quit action fully into view.

Ordinary walking brought Mira into boarding range. After a short straight drive
and braking, Exit on foot placed her beside the parked pickup, approximately
2.09 metres from its centre. Checkpoint restoration retained the next housing
goal. Two clean exits and a reopen preserved the 201.88-credit balance, four
water units, exact character and vehicle poses, owned vehicle state, 50 XP,
completed contract, neighborhood and source identity. Both sessions reported
64 ready wildlife instances, no fallback instances and no world, simulation or
persistence errors. The [native record](native-v11-verification.json) contains
the capture hashes, selected state comparisons and process/telemetry evidence.

Both walking approaches completed through the bounded road planner; their
perimeter detours remain a navigation-efficiency limitation. This final session
did not repeat character/world creation, dense traffic recovery or every new
branch. Food alternatives, stale trade rejection, failed restore, roadside
selection and garage delivery are established by the fresh automated cases.
The native environment was Linux/X11 with software Vulkan and keyboard/mouse;
its observations do not establish hardware GPU performance or other platforms.

Current automated and native evidence supports the implemented reference, with
its declared bounds. Further acceptance still needs long-career balance and a
complete expense ledger; dense navigation scenarios; total-scene GPU/streaming
budgets; licensed source coverage; representative concurrent populations and
fault recovery; and actual target-platform, controller, assistive-technology and
hardware checks. The gap register gives concrete evidence needed for each claim.
