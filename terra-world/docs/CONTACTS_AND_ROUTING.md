# Ground-body contacts and road routing

Terra's reference authorities now admit ground-body movement against other
resident bodies. Registered cars use their vehicle model's dimensions. Ordinary
traffic has a bounded reference vehicle shape, and people, creatures and animals
have compact body footprints. Vehicles interact with those footprints; people
and animals do not collide with each other in this reference implementation.

## Authority and reuse

`terra_core::sweep_body_contact` supplies a geodetic oriented-prism sweep.
Translation is continuous, so a fast body cannot jump across a thin obstacle
between sampled endpoints. Turning uses at most 90 conservative angular
intervals, each expanded for its rotational excursion. The vertical interval
separates traffic on a bridge from bodies below it. Inputs, dimensions and
ground-sweep distances have finite bounds, and geodetic seams use a local WGS84
frame rather than a longitude subtraction.

`terra_sandbox::LivingContacts` maps living bodies to those shapes and owns the
geographic broad phase. It is built once for a population step and incrementally
relocated after each accepted body move. Later bodies see the accepted position
of earlier bodies, including across an index-cell boundary. A sparse population
does not require comparing every moved actor with all previous movers.

The shared admission is connected to autonomous traffic, couriers, wildlife,
owned RTS orders, local manual driving/walking and the host's controlled living
bodies. Occupied walking avatars are excluded while their owner possesses a
road vehicle. Frozen but resident offline bodies remain static obstacles; bodies
in evicted regions have no resident physical shape. Games inserting independent
host actor records outside the living-world model supply their physical
collision policy through their environment integration.

Terrain and source-building admission happens before body contacts. Unknown
terrain remains a streaming boundary and causes no invented impact damage. A
new manual vehicle impact stops motion at a small clearance, clears handling
momentum and applies bounded condition loss from closing speed. An already
overlapping spawn/legacy placement can move away when its endpoint overlap does
not deepen. Stationary contact and repeated low-speed pressure do not repeatedly
charge crash damage. Autonomous units yield at a blocked motion rather than
receiving a fabricated physical impulse.

### New multiplayer placement and saved positions

New principals are prepared transactionally. A bounded search around the host's
arrival area validates the complete starter crew, including all rover wheel and
body-envelope samples, source/facility clearance and existing resident bodies.
People are included in spawn overlap checks even though their ordinary walking
contacts are permissive. Dormant resident owners also occupy their saved space.
A valid avatar point alone cannot authorize an unsupported rover or an
intersecting crew. Exhausting the search returns an actionable registration
error and leaves player IDs, capabilities, inventories and existing bodies
unchanged.

Authenticated resume keeps existing saved positions, homes, paths and headings.
The handshake does not run a fresh spawn search or relocate a saved body to
newly convenient ground. Unavailable terrain stops subsequent movement until
support is present; it does not rewrite the saved world's placement.

### Grounded vehicle exits

Exiting uses a bounded authority-owned dismount planner rather than placing the
avatar at the vehicle's centre. It checks complete support for the rotated human
footprint and sweeps each supported corridor segment against all other resident
bodies. People and dormant owners count as exit obstacles even where ordinary
walking contacts are permissive. Only the occupied vehicle is ignored inside
the corridor; it remains an obstacle at the final placement. Existing overlap
with another body cannot use the movement escape exception to authorize a new
dismount.

The planner also checks source geometry, authored facility/structure footprints
and unavailable terrain. No candidate means no position, control or parking
change. Regional hosts hydrate before planning so cold bodies cannot disappear
from exit admission. The exact API, search bounds and ground contract are in
[Vehicle dynamics](VEHICLE_DYNAMICS.md#safe-vehicle-dismount).

Purchased and rented vehicles use a similar bounded garage-delivery admission.
The new model and a surrounding walking strip need clear resident support and
occupancy before any funds, fuel, bond or ID changes. A garage marker occupied
by the starter rover therefore produces a nearby clear parking bay instead of
another body at the same centre. Exhausted or unloaded parking fails atomically;
the search does not relocate existing vehicles or expand beyond the garage's
normal interaction reach.

### Construction around occupied bodies

Building admission uses `SandboxState::construction_obstruction` and the same
model dimensions as movement contacts. `terra_core::body_overlaps_column` tests
the oriented body against the proposed structure's circular footprint and
vertical interval. A small fence cannot be placed through a long bus's nose
merely because its centre is several metres away. The narrow sides of a vehicle
remain available when the actual footprints are clear. Human and animal
reference dimensions are shared with contact admission too.

The authoritative `Build` command performs this check before changing inventory,
structure IDs or ownership. It applies to other owners' bodies and to parked,
rented, company and resident dormant vehicles. The native placement preview uses
the same query and identifies the obstructing actor. Hosts hydrate their command
dependencies before admission; a client preview cannot authorize a build through
an actor outside its received view. Terrain, property permissions and existing
structure/facility checks remain separate requirements. Existing-overlap escape
rules apply to movement only and are unchanged by construction admission.

### Vertical clearance and thin source geometry

The reference source collider now compares the actual vehicle model height with
an elevated building part's explicit `min_height`. A low car may pass below a
ceiling that stops a bus. `terra_core::has_underpass_clearance` uses terrain at the
building footprint, the body's current support altitude, its complete height,
and a 0.15 m reference margin. Source way geometry with altitude zero never turns
that terrain-relative underside into an absolute world elevation. Missing or
invalid underside metadata remains solid; unspecified vehicle height uses a
conservative eight-metre envelope rather than assuming a small car.

The Bevy API is `environment::source_building_at_with_clearance`. The headless
`ResidentSurface` supplies matching `building_at_with_clearance` and
`vehicle_support_with_clearance` queries. The network `WorldEnvironment` trait's
new method defaults to the old support method so existing custom providers still
compile; a provider with overhead geometry should override it. Local and host
manual vehicle envelopes pass their model's actual height, and client prediction
uses that same height-aware source policy. Generic kind-only autonomous resolvers
remain conservative when a particular model height is unavailable.

Local and host vehicle-envelope sweeps now bound both translation and corner arc
travel to 0.25 m per sample. At most 512 intervals are admitted; an oversized
request stops rather than silently undersampling. This closes the old one-metre
sampling gap for thin source walls. The 0.2 m support radius overlaps neighboring
samples, and ordinary ground/water/coverage checks remain required. This is still
reference geometric admission, not a rigid-body collision response.

This is a kinematic stop-and-yield policy. The shared
[vehicle support model](VEHICLE_DYNAMICS.md) now samples four wheel heights and
materials, uses load-aware handling and drives native visual wheel springs.
Contact admission still does not supply rigid-body impulses, crumpling, tire
contact patches, dynamic suspension forces, rollover, pedestrian injuries,
aircraft or ship contacts, or trailer articulation. A game-owned physics adapter
can replace the reference policy without accepting client-supplied positions.

## Routes that remain usable with physical bodies

Road access projects start and destination onto the nearest road segment before
using the bounded connected graph. Starting halfway along a long road no longer
requires driving back to its nearest endpoint. Disconnected graphs retain a
direct fallback; this is not a claim that an unobserved connecting road exists.

`LivingWorld::route_between` retains the shared center route.
`LivingWorld::vehicle_route_between` now uses the directed, grade-aware graph
and adds a reference right-hand offset on roads wide enough for two ordinary
cars. It keeps the final work stop exact. The autopilot, owned vehicle orders,
settlement couriers and company drivers use the same route. Authored facility
detours still pass through terrain admission.

`LivingWorld::vehicle_route_plan` exposes `RoadPlanKind`: `Connected`,
`DirectNoRoads`, `DirectDisconnected`, `BlockedAccess`, `BlockedDirection`, or
`Unavailable`. The two direct kinds are explicitly off-road reference fallback,
not evidence of an observed connecting street. A known one-way or access
restriction never becomes a direct wrong-way fallback. The compatibility
`vehicle_route_between` API holds at `[from]` for a blocked plan; integrations
that need a player-facing reason should inspect the plan before placing an order.
The native navigation panel does this: access, direction and unavailable-data
failures show distinct reasons and clear the route line. Direct fallback labels
also distinguish missing roads from disconnected roads.

Routine residents and owned walking orders use `pedestrian_route_between`, an
authored road-edge offset that is explicitly not sourced sidewalk geometry.
Its authored-facility admission uses the existing walking-body clearance;
vehicle routes continue to use vehicle clearance.
People and wildlife can make a bounded sideways step around a vehicle when
their route is blocked. The trial keeps the physical speed/time budget and
passes terrain and body admission. This also keeps an older saved centerline
patrol from permanently bunching pedestrians against a parked vehicle.

### Supported shortcuts for owned walks

Native and network `MoveUnits` dispatch use
`SandboxState::move_units_with_environment`. This retains the ordinary group,
ownership, licensing and two-kilometre order checks, then removes redundant
walking waypoints when complete supported corridors are available. If the direct
proposal encounters a stopped vehicle, the planner can use two local avoidance
corners around that body. Nearby goals can therefore avoid a long perimeter-road
excursion even when a parked vehicle blocks the straight line. Vehicle routing
and routine resident patrols retain their existing route policies.

A shortcut spans at most 160 metres and must save more than two metres of route.
The planner considers at most eight later targets per leg and shares a maximum
of 16,384 source queries across the whole group command. It samples the same
axis-aligned walking footprint at its center, corners and edge midpoints every
25 centimetres. Support must stay within 0.5 metres across the footprint and
0.4 metres between samples; an initial height mismatch over one metre rejects
the proposal. Source coverage, water, slope and source-building admission remain
the host resolver's responsibility. Authored facilities, structures and living
body sweeps are also checked along the supported corridor.

A body detour considers both sides of the first encountered vehicle in stable
shorter-first order. Every leg, including the return from the second corner to
the original target, passes the same complete corridor checks. The entire
three-leg replacement must be at most 160 metres, save more than two metres,
fit inside the remaining command query budget and leave room for the untouched
remainder under the existing 129-waypoint save limit. The planner does not recurse
around another obstacle, invent corners for unavailable terrain or rewrite
routes around moving traffic. Another body, a structure, missing footprint
support or a budget limit can reject a proposed side; if neither side is
admissible, the previous route remains available.

The callback may supply a supported height but cannot slide the route: horizontal
differences beyond a one-micrometre numerical tolerance reject the proposal.
Admitted waypoints retain their exact latitude/longitude and use actual ground
height. The ordinary command destination and all arrival rules remain unchanged.
Planning updates the path only; the fixed-step controller still performs and
admits each physical step. Missing support, blocked corridors or an exhausted
planning budget retain the existing road/facility route for that leg. This is a
bounded local improvement, not a globally shortest-path or navigation-mesh claim.
SDK integrations can call the same environment adapter; plain `apply(MoveUnits)`
retains its existing source-independent route proposal.

The [walking-route regressions](../crates/terra-sandbox/tests/walking_routes.rs)
cover a parked-body bypass with bounded physical steps and save/resume before its
corners are consumed, an occupied first side, moving traffic, missing corner
support and full-group query exhaustion. A separate fixture uses the exact
horizontal walker/pickup poses and road topology from the 0.12 native market
review on explicitly flat supported ground. That fixture isolates the local
geometry; it does not by itself qualify the real source terrain or establish a
general shortest-path guarantee.

The [prepared-source replay example](../crates/terra-net/examples/walking_replay.rs)
accepts a complete regional export, its exact prepared-source manifest and an
owned walker with an existing Move order. It reissues that destination and
measures route reduction, source/body admission, bounded movement, goods/cash
conservation and exact save/resume continuation. It reads the export without
rewriting it and prints aggregate checks rather than private world records:

```sh
cargo run -p terra-net --example walking_replay -- saved-walk.region.json crates/terra-stream/data/heightmap-connected/manifest.json 1000050
```

Use the unit ID and source manifest belonging to the supplied export. The probe
requires a strictly shorter route and physical arrival within 120 seconds;
calendar delta is zero so the measurement isolates movement and existing
delivery transfers. Its scoped result complements native interaction review.

Traffic patrols follow directed lane loops. An open two-way source road returns
in the opposite lane, while a closed ring wraps to its next point rather than
reversing into following traffic. An open one-way route runs only in its permitted
direction and stops at its end; it does not fabricate an illegal return lane.
Narrow roads retain a single center path. Stopped physical bodies can still
obstruct a narrow road; neither traffic nor a player may phase through them to
force progress.

## Source rules and explicit junctions

`RoadRoute.rules` is additive, with backward-compatible two-way/unrestricted
settings for older authored roads. `RoadRules::from_properties` retains supported
OSM-style `oneway`, mode-specific access, numeric `maxspeed`, `layer`, `bridge`,
`tunnel`, and roundabout direction information. Numeric km/h and mph are accepted;
unknown and conditional speed values remain unknown. Reversible/alternating
one-way values need a signal policy and are closed to automatic reference routing
instead of guessed as two-way. Footpaths/cycleways/steps and explicit private or
prohibited motor access are excluded unless a more specific permitted mode tag
supplies access. These rules are a bounded navigation subset, not complete
jurisdictional road law or a permission system for private roads.

`RoadNetwork` compiles directed segments with start/goal projection onto segment
interiors. At-grade geometric crossings are split into shared vertices; merging
requires compatible source structure/layer and less than 1.5 m vertical
separation. A bridge/tunnel `layer` is a relative topology rule and never becomes
an invented road elevation. A production provider should preserve true junction
and turn-restriction identities, bridge approaches, and measured deck geometry.
The normalized reference format still cannot encode every OSM relation or legal
turn. Incomplete disconnected geometry remains explicit.

The graph allows at most 8,192 vertices/segments and 256 junctions, in addition to
the existing 32 roads × 64 source points and 129 route points. Exhausting a graph
budget returns `Unavailable` and holds an automatic route. Cheap segment bounds
avoid expensive geodetic intersection work for disjoint geometry. A private,
immutable compiled graph cache is reused by the physical traffic step and rebuilt
when road geometry or rules change. The cache is not saved or replicated.

Source conventions: [OSM oneway](https://wiki.openstreetmap.org/wiki/Key:oneway),
[OSM maxspeed](https://wiki.openstreetmap.org/wiki/Key:maxspeed), and
[OSM layer](https://wiki.openstreetmap.org/wiki/Key:layer).

## Physical intersection coordination

Each vehicle requests its nearest upcoming junction. Requests receive monotonic
FIFO tickets, with stable actor ID order breaking simultaneous arrivals. Only one
vehicle receives a junction permit at a time. A blocked exit does not acquire the
box; the oldest request with a physically usable exit can proceed, while older
blocked requests retain their order. Moving bodies obey a geometric stop line
before their own front clearance enters the conflict area. The final candidate
is checked again so a local stationary-body detour cannot cut through a box for
which it has no permit.

Exit checks include reference spacing beyond the hard contact footprints. A
queued vehicle can therefore block a turn's required exit clearance while a
short motion on the approach is still body-clear. The waiting request retains
its ticket and cargo; clearing an earlier maneuver does not promise that every
downstream turn is available. Such a hold grants no delivery or return credit.

The projected body uses the heading of its actual outgoing route segment. An
oriented footprint test compares it with each other vehicle's current heading,
preserving the reference 0.75 m longitudinal and 0.35 m lateral margins and
three-metre grade-separation rule. This avoids both missing a perpendicular
vehicle beside the future nose and applying a long vehicle's length sideways
into a clear lane. The reservation is still only an exit-availability hint;
every actual turn passes ordinary continuous contact and terrain admission.

Permits are traffic scheduling hints, never collision authority. Terrain,
buildings, swept body contacts, fuel and vehicle limits still decide every actual
move. Parked bodies inside a junction block new entry. A manually controlled
vehicle's actual position and short physical velocity projection also block
automatic admission; the reference does not force the player to obey a traffic
signal. Numeric source limits cap automatic driving; manual vehicle dynamics
remain authority-owned and those road limits are advisory.

An approach holder that makes no physical progress for eight seconds, or fails
to enter within a thirty-second approach lease, rejoins the queue tail. A vehicle
already inside must physically clear: its occupancy never disappears on timeout.
This prevents a blocked approach from monopolizing an otherwise usable junction
without allowing traffic to phase through a stopped car. Moving away, holding or
stopping a route, dormancy and despawn remove obsolete requests on the next
physical step. Narrow-road deadlocks, blocked intersections and unavailable
terrain can still require player intervention. Company payroll retains its
bounded no-progress hold policy rather than charging indefinitely in such a queue.

`LivingWorld.traffic` persists physical elapsed time, tickets and bounded queue
state. Calendar acceleration does not shorten approach timeouts. There are at
most 512 pending requests in total. Stale request IDs can remain in a saved
regional ledger while their bodies are cold or removed; they grant no authority
and are pruned against actual active bodies before any automatic movement.
Serialization never restores a physical permission without recomputing current
occupancy and route eligibility. FIFO state and deterministic motion survive a
save/resume, while private geometry caches are excluded from semantic equality.
`LivingMetrics` exposes grants, wait observations, timed-out approach requeues and
blocked exits as work counters, not measured capacity promises.

Owned movement orders and couriers can plan a bounded two-corner detour around
a stationary body ahead of them. Corners include turning clearance and require
supported terrain. Both proposed legs pass the full source envelope, geometric
stop-line clip and body sweep before insertion. The complete reconnecting source
envelope is checked as well. For this original incremental one-body maneuver,
future traffic permissions and body occupancy are evaluated during physical
progress, rather than requiring the future connection to be vacant now. This
keeps an occupied future waypoint or changing reservation from invalidating a
necessary present escape. Every actual connecting step still passes the live
source, stop-line and body checks; a future moving obstruction can hold it. The
reconnecting segment is limited to 160 m of three-dimensional travel before any
source callback runs, including at the maximum five-second update. Its complete
pose envelope is split into calls with at most 64 m of translation plus rotational
travel (0.1 m tolerance); each call consumes the same shared allowance. This
keeps supported 128–160 m reconnections within the compatibility resolver’s
512-interval per-call limit without admitting an unchecked prefix. A failed
later segment commits none of the proposed maneuver. Each resolved endpoint
must remain within five centimetres horizontally of its proposal; a sliding resolver
cannot authorize another corridor. A reverse leg uses the same facing decision
as the controller, without adding an unnecessary half-turn to its planning
sweep. These remain navigation suggestions: every actual steered step passes
the complete live admission again. A parked body behind a vehicle does not send
the route back around that body when only rear clearance is needed.

When both original one-body proposals fail, a second stopped body encountered
by a corridor sweep can seed a **bounded cluster fallback**. It expands the
route-aligned outer corners around at most four encountered stopped bodies,
retaining the existing complete-body and turning clearance. It admits all
three legs, including the connection back to the original target, before
inserting the two corners. A moving member cannot seed or extend this static
proposal. Missing source support or a held intersection boundary stops that
proposal; neither is treated as a body to route around.

Cluster corners must remain within **64 m** of the current position and the
complete three-leg proposal within **160 m**. Across all physical substeps and
clearance retries of one vehicle update, the cluster fallback, narrow fallback
and added one-body reconnection checks share at most **12 additional corridor
admission attempts**. The original two one-body approach legs retain their
existing allowance. One-body and cluster proposals keep priority, side ordering
is stable, and no wider population search runs.
The existing 129-point route limit and one-pending-maneuver patrol policy remain
in force. Exhausted or rejected proposals leave the ordinary route and its
obligations intact. These are bounds on additional planning work, not a claim
that every four-body arrangement has an admissible passage.

If those wide proposals fail, a final bounded alternative can use a narrower
corridor beside **one parallel parked vehicle**. It uses the actual projected
half-width of the obstruction, half the moving vehicle's width and a 0.8 m
lateral margin. Longitudinal approach/exit room still includes both enclosing
body radii plus 3.5 m. Moving, routed, crossing, unknown or self bodies cannot
seed this alternative. Both corners stay within 64 m and the full route within
160 m; all three complete envelopes, contacts and stop-line clips must pass the
shared allowance before insertion. If the vehicle arrived too close before
coverage returned, one additional straight reverse approach can provide nine
metres before the first corner. It must retreat between two and eighteen metres
while nearly aligned with the route; its complete four-leg proposal must pass
the same shared allowance and 64/160 m bounds. Only then are the retreat and two
corners committed. The reverse leg uses the ordinary low-speed arrival and
direction-change rule. Actual movement still uses normal handling and live
admission. This admits the tested 9.5 m supported strip and its late recovery
without borrowing a wider area. Missing rear support, another occupying body or
an insufficient turning envelope still holds the route.

A newly admitted `Move` maneuver discards only the already-consumed path prefix
before inserting its waypoints. The unconsumed route and final destination
remain intact. Rejected proposals do not discard the prefix, the 129-point cap
still applies to the resulting route, and patrol circuit/cursor rules remain
unchanged. An unavailable flank can therefore wait through many clearance
attempts without permanently losing the ability to replan when support returns.

A tight corner or legacy overlap can trigger a forward or reverse clearance
attempt through the same throttle, steering and handling integrator. It is capped
at one metre per candidate and must increase separation from the actual
blocking pedestrian, wildlife or parked vehicle while the two enclosing body
circles plus a 0.25 m margin overlap. A moving pedestrian can therefore receive
room to pass without a vehicle advancing into it. Every candidate still passes
terrain, stop-line and full-body contacts.

If translation is denied, the valid bounded steering-centering intent can still
settle the wheels; a rejected turn cannot keep retrying a locked steering angle
forever. Once outside that finite envelope, clearance divergence stops. Moving
vehicles retain stop/yield priority. Following gaps bound forward progress
toward a leader; they do not forbid an otherwise admitted reverse away from it.
The destination stays unchanged, and no recovery assigns a target position or
instant heading.

When a vehicle `Move` or `Patrol` order needs to retreat from moving pedestrians,
wildlife or a stopped road user with an active route, an admitted clearance can
add one short waypoint (5–8 m, depending on vehicle length) to the saved path.
The complete proposed corridor first passes source, intersection and body
admission. Normal steering and all live admission checks still apply, and the
original route follows afterward. For parked vehicles and stationary people,
`Move` and `Patrol` orders can use a complete admitted static detour. If neither
turning corridor is admissible, an otherwise admitted straight outward clearance can
persist before the original route is retried. This prevents alternating a tiny
clearance step with the same blocked turn at a stationary avatar's corner;
source envelopes, body sweeps and the obstacle's position remain authoritative.

An intermediate reverse waypoint followed by a forward leg uses the final-goal
braking policy and the nearer, low-speed arrival check before it is consumed.
It cannot use the ordinary three-metre corner tolerance to finish a short
retreat early while still backing. Nearby rear-quarter targets also select
reverse before demanding a forward loop into an unreserved junction; separate
entry and continuation thresholds preserve direction hysteresis. These choices
change control intent, never traffic permission or contact admission.

A vehicle patrol stores one pending maneuver in the front-consumed `path`: a
clearance leg, two detour corners, or the narrow reverse approach plus its two
corners. Its original patrol points, cursor and
direction remain unchanged. A new maneuver is proposed only after that prefix
has drained, keeping repeated obstruction attempts bounded.
Waypoint advancement, next-corner braking and intersection requests all inspect
the same temporary prefix before the original patrol. Saved queues and handling
therefore resume the same maneuver, and another vehicle cannot acquire occupied
space until the holder physically clears it.

Walking people also retain a bounded two-corner avoidance route around a
stopped vehicle. Both proposed legs must resolve to their requested corners
and pass body sweeps before being saved; a sliding resolver cannot silently
approve a different corridor. A patrol consumes these temporary points from
the front of `path`, retaining its original patrol points, index and direction.
This lets crossing residents finish their sidestep instead of oscillating at a
vehicle's nose. Each walking patrol has at most one pending two-corner detour, and its
cursor remains bounded by the original patrol length as the temporary queue
drains. Save/resume preserves both that queue and the original patrol. Every
actual walking step still passes ordinary admission.

The path budget remains 129 points. A facility detour that would exceed this
budget becomes an unavailable plan and holds at the current position; it cannot
fall back to a direct shortcut through a one-way restriction. An impossible
detour leaves the vehicle stopped and the existing courier retry/physical-return
policy applies.

These are bounded local recovery policies. They do not provide a global
navigation mesh, reserve a complete future route, or guarantee that every
crowded arrangement can clear autonomously. A planned corridor may become
occupied or lose terrain coverage before the body reaches it; live admission
then holds the body. A parked vehicle in the only available escape corridor
remains an obstruction. Unblocking that space or explicitly resuming the
existing courier/company recovery flow may still be necessary, with cargo,
escrow and physical arrival requirements preserved.

Settlement dispatch also reserves both the pickup and destination service bay
together. Independent facility pairs can work at once, while opposing couriers
cannot each acquire one narrow bay and wait for the other. These bookings are
derived from durable delivery records and therefore survive save/restore without
another mutable reservation ledger. Completion, unloaded cancellation or a
completed physical return releases the booking. These facility bookings and
intersection permits solve different conflicts; neither grants ownership of a
public road or bypasses a blocked physical body.

## Road presentation

The native demo adds right-hand direction arrows and approach stop bars derived
from `RoadNetwork` and the saved source rules. One-way roads receive only the
permitted arrow direction; narrow two-way roads avoid contradictory overlapping
arrows. Authored asphalt has edge lines and two-way center dashes. Stop bars show
the reference scheduling boundary, not an assertion that a real traffic sign was
observed in open data. Tunnel paint is omitted rather than drawn on the terrain
above an unmodeled tunnel.

Paint follows loaded physical support heights and uses one cached mesh with a
4,096-primitive budget. Junction bars receive priority over decorative arrows.
Road/rule changes and changed resident source chunks invalidate the cache;
unchanged missing coverage does not rebuild an empty mesh each frame. These
original geometric assets improve legibility but do not certify AAA asset quality
or target-hardware frame rates.

## Regression coverage


`navigation_geometry` adds five bounded authored flat SDK cases. The original
0.15 driver remains 52.70 m from its 60 m destination after 120 physical seconds
on a 9.5 m supported strip with a parked vehicle 16 m ahead; 286.42 m of accepted
travel is spent on repeated retreats. The narrower fallback reaches the goal
in 17.4 physical seconds over 60.51 m in that same fixture. The retained failing
baseline is separate from release acceptance.

The matrix also holds a five-metre strip for 600 physical seconds and then
restores only the 9.5 m supported strip, rejects a detour whose final reconnection
crosses a missing six-metre strip, lets two moving crossing vehicles clear,
and exercises a supported 9.5 m junction with a blocked exit. In the junction
case, another exit completes while the blocked approach waits, and a deliberate
new order physically drives the obstruction away before the queue continues.
No obstruction is deleted or teleported as recovery.

Every case checks sampled complete envelopes, physical steps and heading
changes, all population sweeps in update order, conserved pocket money and
inventory, the path limit, exact save/resume and complete-state validation.
Machine-readable reports record total/max source samples, travel, elapsed
physical time and resumed steps. The three-body fixture ceiling is 32,768 source
samples per 100 ms update, not a global CPU or GPU capacity promise. Calendar
progress is frozen, so these cases do not establish payroll, fuel, economic
balance, terrain coverage or general road-network throughput. The missing-flank
case finishes 19.7 physical seconds after restoration, using an admitted reverse
approach. A separate patrol check preserves its original circuit through that
three-point maneuver, and a full 129-point unconsumed route cannot reclaim
capacity. An instrumented two-kilometre next-waypoint case uses twenty maximum
five-second updates and rejects reconnection-source calls longer than 160 m;
its finite callback ceiling is a fixture bound, not a promise about arbitrary
custom-provider execution time. A separate 155 m route around a stationary
human covers complete segmented reconnection, physical arrival, exact resume,
body sweeps and conserved carried goods/money. A missing strip in the final
source segment must reject the whole proposed maneuver. The unchanged
`company_depots` careers cover the related registered-vehicle regression with
physical deliveries, finite stock, cash and paid recovery. Unsupported smaller
strips or occupied rear approaches can still
require a new order or another explicit recovery.

`navigation_clusters` covers an authored flat SDK approach where three parked
vehicles block both one-body corridors. The original 0.14 driver had advanced
only 7.30 m along the 60 m route after 120 physical seconds, repeatedly
retaining retreats. The cluster fallback physically reaches the same final
goal in the measured case, keeping the parked bodies fixed and conserving all
inventories and money. It checks each body sweep and bounded step, admits
complete sampled source envelopes, validates the whole state, and requires
exact serialized continuation through the maneuver. Its
`NAVIGATION_CLUSTER_REPORT` output records actual elapsed time, distance,
largest step/heading change, path size and source-query work.

Companion cases withhold both supported flanks before restoring them, make
only the opposite flank available, retain a patrol's circuit and saved prefix,
and let a moving crossing vehicle clear through ordinary handling. Contact
tests reject moving, duplicate, unknown, self and excess cluster members.
These tests use synthetic flat SDK support and reference traffic bodies; they
do not establish licensed-world coverage, company profitability, target
hardware performance or a general traffic-flow guarantee. The original
occupied-exit and stopped-patrol fixtures remain separate unchanged evidence.

The focused tests cover continuous high-speed admission, adjacent lanes,
overpasses and vertical sweeps, turning corners, departure from an existing
overlap, finite extreme inputs, date-line/polar frames, occupied-avatar
exclusion, walking after vehicle exit, relocated index entries, sparse
512-body contact-query work, autonomous tunneling, manual local/host impact
behavior, turn clearance with forbidden terrain, and road-segment/lane routing.
The perpendicular-turn regression uses the bundled imported heightmap, a saved
loaded passenger trip, and an eastbound traffic nose beside a north-facing
pickup. It requires a physical delivery to Hillside, grounded bounded steps,
conserved company/issuer/payroll/escrow money, and the actual passenger transfer.
The TCP RTS flow now explicitly verifies that an order into standing starter
crew cannot force arrival, followed by a clear destination and save/resume.
The populated settlement regression
continues to require actual physical courier deliveries with traffic and
facility collision enabled. These are bounded correctness workloads, not a
realistic concurrent-player capacity benchmark.

The 0.10 recovery regressions retain the original 120-second populated-settlement
and 300-second finite-source workloads, with exact material conservation. Paired
one-step queue tests compare active and parked peers at identical poses while
requiring bounded acceleration and turning. A saved, aged junction reservation
requires its patrol holder to retreat physically before the waiting peer moves,
then both resume across a serialized checkpoint. A rear-quarter arrival test
keeps forward admission blocked while requiring a bounded reverse approach and
low-speed physical arrival on the permitted side.

`traffic_recovery.rs` additionally retains the exact synthetic saves behind the
archived 0.10 occupied-exit diagnostic, with their original hashes and baseline
results in `tests/fixtures/traffic-v0.10-provenance.json`. The 300-second rear-trap
save and 420-second occupied-exit save each receive the original 120-second
replay window. The regression requires the original loaded delivery to return
physically to its source, empty the courier and restore exactly its 12 wood.
The reviewed implementation returns those loads after 34.0 and 49.5 physical
seconds respectively. These fixtures retain their original flat SDK support
environment; native terrain behavior has separate admission/dispatch coverage.
Every step conserves each item and all 150,000 minor credits; all inventories
including the two SDK players contain 770 goods (the original diagnostic's 654
excluded their 116 goods). A checkpoint at 60 seconds must resume to exactly the
same final state. A separate unavailable-source replay requires the loaded
courier to wait without claiming delivery or return. The replay emits structured
`TRAFFIC_RECOVERY_REPORT` evidence only after these assertions pass.

The isolated patrol regression requires a stopped vehicle to remain an obstacle
while both detour sides lack support, then requires bounded physical passage and
identical saved-maneuver continuation when support becomes available. Walking
regressions cover a perimeter route shortened to its supported approach, thin
uncovered strips, missing corner support, parked bodies, shifted resolver output,
atomic ownership/distance rejection and the shared maximum-group query budget.
Native and network dispatch tests retain the road route without resident
coverage and admit its supported shortcut after coverage is restored.

The v0.8 traffic regressions also require a four-way simultaneous arrival to clear
in FIFO order, unchanged behavior after a serialized resume, safe blocked-exit
skipping, no-progress approach requeue, manual approaching-body priority,
100 seconds of unexpired parked junction occupancy, large-step stop-line
admission, numeric source speed limits, stale regional request pruning,
sequence saturation, compiled-topology invalidation, reverse one-way population
spacing on unequal segments and fail-closed facility-detour budget overflow.
The native guidance regression uses actual restricted domain routes and verifies
their captions and absence of a misleading route line. Source-clearance tests
cover car versus bus, elevated support, zero-elevation OSM geometry, unknown
heights and compatibility with an existing custom host support provider.
Construction regressions rent an actual bus for one player, attempt to build
through it from another player's permitted plot, and require unchanged inventory,
bond and body state on rejection. They also require a clear side placement to
succeed, reject an intersecting animal footprint, check rotated/vertically
separated geometry, and verify matching native placement feedback.
The dismount regressions require grounded, unoccupied avatar placement through
the ordinary native and network command dispatchers, atomic rejection of blocked
or unsupported exits, rotated footprint support and unchanged goods/fuel ledgers.
Projected junction-exit tests cover crosswise nose occupancy, a genuinely clear
side lane, unchanged parallel spacing margins and grade separation.
