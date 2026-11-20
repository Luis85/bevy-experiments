# Product review and improvement pass — 0.10

Review date: **2026-10-28**. This review covers the implemented SDK, reference
game, independent cross-review and connected development play. The previous
review is preserved in [PRODUCT_REVIEW_v0.9.md](PRODUCT_REVIEW_v0.9.md).
[VALIDATION.md](VALIDATION.md) is the final authority for release commands,
counts, source/executable identity and native confirmation. Development
observations below do not certify a later executable or replace that record.

## Product conclusion

Terra 0.10 makes the existing game easier to continue, extend and recover.
State-derived goals connect supply work, property, community and transport;
autonomous vehicles now use bounded physical handling; prepared terrain bundles
join through validated coverage; and a reproducible animated wildlife pack
exercises an explicit art-production contract. Verified backup and recovery
also preserve the complete bounded world when most records are dormant.

These changes strengthen a reusable SDK and playable reference game. They do
not establish AAA production quality, full rigid-body vehicle physics, global
terrain or historical coverage, a balanced indefinite career, or production
MMORPG capacity. The review separates demonstrated behavior from those larger
acceptance requirements.

## Player guidance and pacing

The Activities inspector now answers what to do next from the player's actual
inventory, contracts, transport assignments, permissions and progression.
Suggested journey, supply, home, community and road tracks can be revisited
without restarting a tutorial. The next action precedes longer explanation;
a persistent world banner retains the goal when immersive controls hide the
inspector. Restoring a save derives guidance from existing achievements and
obligations rather than maintaining another completion ledger.

Extended native play exposed a continued-play gap: the driver's water could
reach zero while Suggested journey kept promoting housing. Suggested now gives
critical needs priority at 20% or below, using the actual driver's avatar for a
registered vehicle. Drivers are directed to park and exit; on foot, guidance
uses carried consumables or existing acquisition, while energy opens the normal
Command rest controls. Explicit goal choices remain selected. Regression checks
verify real inventory consumption, unchanged work commitments and no automatic
vehicle exit or rest order.

Final native confirmation completed this entire urgent-water sequence on the
frozen executable: park, exit, travel to a real resource, gather one water and
drink it. Displayed water rose from 0% to 60%, then Suggested returned to the
housing goal. The existing 205.68-credit balance, 50 XP and neighborhood remained
intact after clean save-and-quit. The final evidence is recorded separately in
[native-v10-verification.json](native-v10-verification.json).

Cross-review closed three consequential errors. Acquisition now considers an
affordable stocked seller of the requested finished item before recursing into
missing recipe inputs. Road guidance opens the exact accepted job and assigned
vehicle, including after resume, instead of a previously browsed offer.
Resource recommendations inspect the nearest 128 candidates, using a lazy
spatial index of inaccessible plots, rather than multiplying every resource by
every plot. Failure text describes that bounded search, and a partial regional
view never proves that distant resources or infrastructure do not exist.

On activation, the UI recomputes the suggestion and compares its intent with
the displayed action. A changed command, destination or assignment requires
review; a stale click cannot silently become a different purchase or acceptance.
Travel uses ordinary movement commands and supported, unoccupied service
approaches. Review corrected stopping points inside building footprints and
vehicle approaches that checked only the center instead of wheel/body support.
See [GUIDANCE.md](GUIDANCE.md) and [GAME_FLOW.md](GAME_FLOW.md).

## Economy, ownership and continued play

Guidance explains and invokes existing costs, ownership checks, interaction
reach and finite rewards. It creates no tutorial money, duplicate reward,
free crafting input or bypass of protected territory. Inventory already held
can satisfy a request immediately; delivery still consumes those goods and
transfers the issuer's reserved payment exactly once. Choosing another track
does not cancel a loaded commitment.

Development native review completed a real accept → physical travel → delivery
journey for **four wood**. The displayed balance changed from **250.00 to
255.68 credits**, progression gained **40 XP**, and guidance advanced to a
community goal. This connects UI, movement, domain authority and feedback.
The final executable and capture qualification belong to
[VALIDATION.md](VALIDATION.md).

The six-session warehouse regression completes **24 physical deliveries** and
**7 save/resume checkpoints**, covering fuel service, depleted funds,
loaded return and finite stock while retaining exact cash/goods conservation.
That verifies commitments and recovery. In this fixture,
28.80 credits of goods revenue minus
15.49 of wages and 18.00 of paid
fuel service leaves an operating change of **-4.69 credits**,
excluding owner transfers. This is an explicit balance finding, not a
profitable-career claim. Its short synthetic routes, starting harvest and
authored prices do not establish long-career balance, startup-cost recovery or
an endgame. See [COMPANIES.md](COMPANIES.md).

## Autonomous driving and physical recovery

Autonomous intent now supplies throttle, brake and steering to the same bounded
vehicle integrator used for controlled driving. Four-wheel support, material
and weather grip, actual carried load, speed limits, corners and traffic
admission constrain movement. Navigation no longer assigns a waypoint pose or
snaps heading. Registered and ambient handling survive save/resume; accepted
curved substeps determine mileage and fuel. Missing support stops movement
without fabricated travel, arrival credit or collision damage.

Cross-review reproduced a career deadlock that shorter checks missed. An avatar
was ahead of the vehicle but behind the direction of its newly assigned goal.
A detour filter intended to avoid unnecessary rear-corner maneuvers rejected
this necessary turnaround. The correction recognizes obstructions ahead of
either the heading or goal while retaining rear clearance behavior. The exact
turnaround, rear-corner recovery and terrain-denial regressions pass together
with the longer career. Detours remain route suggestions: steering, terrain,
traffic and body sweeps still admit every movement.

The full procedural settlement check exposed another continuity defect: moving
pedestrians repeatedly undid their sidesteps and aimed back through a courier.
The correction retains a validated two-corner walking detour, including across
save/resume, while preserving the actual patrol or work destination. A new
order-aware cursor check prevents draining a temporary patrol queue from turning
an accepted save into invalid state. Queue gaps also admit a safe reverse away
from a leader. These changes retain source, contact and intersection checks;
they do not move bodies directly or extend the procedural delivery deadline.

The complete workspace run then found two related recovery gaps. Patrol vehicles
could undo a safe reverse on the next update, and a proposed detour could have
clear corners while crossing another body or a facility between them. Committed
clearance now covers stopped road users with active routes; controller and traffic
intent consume the same saved Patrol queue. Detours check both complete legs,
including the controller's reverse-facing rule. The finite-goods and populated
settlement regressions retain their original 300-second and 120-second simulated
limits, exact conservation, physical arrivals and facility collision checks.

The final real-terrain regression also caught a stationary-avatar corner where
an admitted outward movement was immediately undone by the original route.
Complete static detours retain priority; when neither complete detour admits,
the controller can preserve the already-admitted outward corridor. Captured
cases and deterministic resume pass, together with the unchanged 180-second
procedural delivery gate. No terrain, body or traffic admission rule was relaxed.

A captured older dense queue still reaches a legitimate occupied-exit hold after
its initial steering trap is cleared. The courier keeps its oldest request and
all 12 wood, but another queued vehicle remains inside the required outgoing
clearance. The diagnostic did not demonstrate an invalid scheduler rule or a
completed cargo return. This is an explicit local-recovery limit: the model does
not guarantee clearing every congested saved state. The
[diagnostic record](verification/v0.10/traffic-exit-limit.json) retains that scope.

This is reference handling with explicit bounds. It does not supply dynamic
suspension, articulated trailer bodies, production tire simulation, unrestricted
pathfinding or complete traffic-law behavior. The contracts and remaining
physics boundaries are in [VEHICLE_DYNAMICS.md](VEHICLE_DYNAMICS.md).

## Geography and source trust

New Heightmap valley worlds use two adjacent original prepared bundles.
Collections require connected shared edges, reject overlapping interiors and
validate every seam knot from both rasters, including mixed resolutions.
Exact rectangle-union coverage catches concave gaps and small holes; a bounding
box or a handful of successful height samples cannot invent missing ground.
The declared seam tolerance admits compatible data without repairing it.

Canonical child paths prevent escapes and duplicate entries. Content identity
covers source bytes and seam policy, independent of list order. Review also
closed a persistence mismatch: long or otherwise unsupported external feature
IDs are normalized into bounded gameplay identifiers while preserving exact
original provenance. Compatible short identifiers and legacy single-bundle
source behavior remain unchanged.

The included landscape is authored synthetic content near a real coordinate.
Its connected coverage is a working regional example, not surveyed geography,
a worldwide ingestion/distribution system, historical completeness or a policy
for disputed borders. See [SOURCE_BUNDLES.md](SOURCE_BUNDLES.md).

## Art production, animation and presentation

The new pack contract gives developers a reproducible export → validate →
install path. A headless `terra-assets validate` command reports identity and
budgets and rejects invalid input for CI. SHA-256-identified model bytes and
binding policy are installed as an immutable snapshot. Five original models—
deer, boar, hare, fox and ibex—include named idle, walk, run, feed and drink
clips and a deterministic generator.

Actual motion selects locomotion and playback speed. Explicit simulation or
virtual-time pause freezes both clip time and crossfade weight; calendar speed
remains independent of physical movement. Per-binding policies prevent shared
models from leaking animation settings. Procedural visuals remain until model,
dependencies and named hierarchy are ready, and remain available on failure.
Imported descendants inherit first-person/layer visibility. The default pack
binds wildlife and preserves character customization and saved authority.

Importer review closed unused-clip budget evasion, reused-sampler allocation
amplification and repeated scans through aliased accessors. Admission now counts
aggregate work before binary scans and allocation. Additional checks reject
malformed hierarchies, meshless scenes, invalid material ranges, skin indices
and weights, composed transform overflow and scale animation that could cross
zero. Basename paths work; nonregular files cannot block startup. STEP animation correctly samples
its final key, and diagnostics count fallback actors even when loading fails
before instantiation.

Development native diagnostics observed **64 ready model instances with zero
loading or fallback instances**. This demonstrates the installed example;
it does not measure target-GPU budgets, art direction, humanoid retargeting,
interiors, audio or unrestricted glTF support. Authoring and format boundaries
are explicit in [ASSET_PACKS.md](ASSET_PACKS.md).

Vehicle entry and exit still use instant reference-game possession and avatar
placement at the vehicle position. Final play verified ordinary outward movement
after dismount; seat, door and dismount animation remain presentation work.

## Accessibility and interface usability

The polish pass keeps the primary goal actionable at enlarged text sizes and
reduces navigation crowding in compact windows. Management focus reveals
available controls, and the measured wrapped goal/source banner reserves its
actual screen area so a click cannot issue a world order behind it. Diagnostic
identities are shortened for legibility while retaining full developer data.

Development play created **Mira Vale** with the **ochre** appearance, created a
connected Heightmap valley world, and verified persisted **150%** interface
settings. Character creation and customization remain part of the journey.
These observations and automated layout checks do not certify screen readers,
localization, physical controller/wheel hardware or all display configurations.
See [CONTROLS.md](CONTROLS.md) and the final native evidence.

## Runtime work and performance scope

The guidance search bounds resource candidates and indexes inaccessible plots.
Source-building queries also reject horizontal non-overlap before calculating
terrain heights for polygon vertices, preserving the tested height, underpass,
date-line and missing-coverage behavior while avoiding unnecessary work.
Complete detour admission adds geometric checks where a route is obstructed;
it remains bounded local planning on the authority thread. These changes and
the native telemetry identify concrete work, but software-Vulkan review and
correctness-test durations do not establish target-hardware frame budgets.
Representative populated-world and graphics profiling remains acceptance work.

## Multiplayer, privacy and admission

New multiplayer principals now require a fully supported, unoccupied placement
for their complete crew, checked against resident and dormant bodies. An
avatar-sized patch cannot partially create a crew with unsupported vehicles.
Failed admission/provisioning remains atomic. Reconnecting principals retain
their saved positions, even when terrain is temporarily unavailable; spawn
repair must not silently relocate persisted ownership.

Guidance operates on the permitted view and submits ordinary authoritative
commands. Presentation packs cannot change ownership, inventory, collision
support or admission. Existing private ledgers, capabilities and session
validation remain server concerns. These improvements do not establish public
account recovery, moderation, Internet reliability or player-capacity claims.
See [NETWORK_REPLICATION.md](NETWORK_REPLICATION.md).

## Operations and larger bounded worlds

Backup now verifies the copied snapshot before atomically publishing a new
exclusive destination. Recovery creates a new verified world path, preserving
incident files. Receipts identify the actual copied generation; maintenance
reads do not rewrite the source database. Bounded copy retries prevent endless
busy-source loops, while documentation distinguishes that deadline from full
validation and filesystem I/O.

The [0.10 recovery probe](recovery-verification-v0.10.json) preserves exact
state, private metadata, generation and dormant records across **1,027 nonempty
regions** and **6,144 resources**. Corrupt input produces no recovery output;
the backup source remains unchanged. The fixture has **47 living bodies**, no
connected clients, no renderer and no terrain import. It establishes larger
storage recovery, not populated-world throughput or distributed authority.
The existing payload/region limits and single-writer architecture remain.
See [OPERATIONS.md](OPERATIONS.md).

## SDK compatibility and remaining acceptance work

The reusable boundaries remain distinct: geographic sources, domain commands,
vehicle handling, storage, networking and optional Bevy presentation. Additive
handling defaults support compatible saved state. Domain save **1**, regional
schema **1**, wire protocol **7** and TLS ALPN **terra-world/7** remain; equal
version numbers do not certify arbitrary mixed executables. Catalog/source
identity checks still apply, and a connected set is a new source identity.
An appearance pack does not migrate authoritative world data.

The next product acceptance work is representative long-career pacing and
player studies; broader art/audio production with measured target hardware;
more complete physics and recovery scenarios; licensed geographic preparation
and distribution; and operational soaks with meaningful populated-world and
network workloads. Those require new evidence. Current release conclusions
rest on the source and bounded journeys recorded in
[VALIDATION.md](VALIDATION.md), with earlier release measurements kept historical.
