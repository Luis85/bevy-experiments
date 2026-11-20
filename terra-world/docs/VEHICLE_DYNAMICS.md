# Vehicle support, loading and handling

Terra's road vehicles use a bounded surface bicycle model shared by the local
authority, network host and owner presentation prediction. The 0.9 increment
connects that model to loaded terrain at four wheel positions, source surface
tags, actual passengers and attached trailer mass. The native demo derives
independent wheel travel from the same loaded terrain. This is useful working
reference behavior for a game; it is not a six-degree-of-freedom rigid-body or
production tire simulation.

## Sampled support and source material

`terra_core::sample_vehicle_ground(position, heading, wheelbase, track, sample)`
queries front-left, front-right, rear-left and rear-right support. Coordinates
remain geographic `f64`; each offset is transformed in a local east/up/south
frame. Positive pitch means the vehicle points uphill; positive roll means the
right side has higher ground. The result contains mean height, pitch, roll,
individual heights, the residual from the fitted chassis plane, and materials.
Multi-axle vehicles use equivalent front/rear support for handling. Their native
visual wheels sample each actual axle separately.

The local adapter uses `terra_bevy::environment::vehicle_ground_sample`; the
host uses `WorldEnvironment::vehicle_ground_sample` and `ResidentSurface`.
Unavailable wheel samples, non-finite heights, unsupported water and slopes
outside the reference drivable limit fail closed. An unloaded wheel cannot
silently become level pavement. Existing complete vehicle-envelope sweeps,
height-aware source-building clearance and living-body contacts still admit
every candidate path. Missing coverage stops motion without invented impact
damage or odometer distance.

Source roads take material precedence over underlying grass/forest/farm
polygons. At stacked road crossings, the supported top reference bridge deck
supplies material. Explicit OSM `surface` tags select an authored coefficient;
untagged tracks and paths use earth, and other untagged source roads use the
reference paved-road default. Explicit unrecognized tags remain unclassified.
The rendered authored settlement road ribbons supply paved material only after
a valid terrain sample established coverage. They never manufacture terrain
height or allow travel over unsupported water.

| Surface category | Reference grip scale |
| --- | ---: |
| Paved | 1.00 |
| Gravel / compacted | 0.76 |
| Earth / dirt | 0.68 |
| Unclassified ground | 0.65 |
| Grass | 0.60 |
| Sand | 0.48 |
| Mud | 0.38 |
| Ice / snow | 0.16 |

These are authored game tuning, not surveyed friction measurements. Four-wheel
material grip is averaged, multiplied by existing weather grip, cross-slope
normal-load reduction and a bounded roughness factor. At 0.30 m residual or
more, roughness reduces the reference contact factor to 0.55. The driving solver
applies longitudinal-slope normal-load reduction separately. Source data that
does not describe small road irregularities cannot provide them; no random
bumps are inserted and labeled as observations.

Custom host environments retain a default neutral paved sample through their
existing validated movement/height query. Providers that know material or
support exclusions should override the method. The supplied resident-source
provider does so. Keep host and client data revisions and source geometry
consistent when predicting source-fed motion.

## Forces, brakes and effective load

`step_vehicle` retains finite validated steps from zero through 0.25 physical
seconds. Engine force, service brakes, steering actuator, gear and RPM remain
the bounded reference model. Available engine/brake force is capped by sampled
grip and normal load. The actual longitudinal tire force used in a step reduces
the remaining cornering budget through a shared force-circle calculation. Hard
braking therefore reduces available yaw instead of granting full cornering and
full braking independently. The lateral slip animation/velocity law remains an
authored approximation.

Service braking approaches zero without numerical sign oscillation. With
sufficient grip it holds an uphill or downhill stationary vehicle; releasing it
allows gravity to roll the vehicle downhill. A steep low-grip slope can exceed
service brake traction. The explicit parked state is a gameplay immobilization
rule: local and host adapters clear velocity and handling before stepping, and
do not charge moving distance or running fuel. It is not a simulated mechanical
parking-brake assembly.

`TransportWorld::vehicle_load(id)` derives carried load without new saved state:

| Component | Current reference mass policy |
| --- | --- |
| Cargo | Actual vehicle inventory units × 10 kg |
| Passengers | Actual onboard private-job and company riders × 75 kg |
| Trailer | Attached empty trailer: 600 kg + capacity units × 4 kg |

Reservations waiting at pickup contribute no passengers or cargo. All towed
cargo already lives in the towing vehicle's one authoritative inventory, so a
trailer contributes only its empty mass. Broken owner/attachment references fail
closed. These authored constants are exported for integration and described as
game policy, not real vehicle specifications or item densities.

`TransportWorld::vehicle_dynamics_config(id)` combines this load with the
vehicle's model, upgrades and condition. The local controlled adapter and host
both use it, and the host's private motion snapshot carries the same effective
configuration. Heavier loads change acceleration and brake response; recorded
distance fuel consumption uses that same total load. `VehicleState::dynamics_config`
remains the base model/upgrade/cargo helper for inspection; it cannot inspect
external passenger assignments or attached trailers on its own.

No suspension state or duplicated load totals are serialized. Existing saves
retain their kinematics and derive load from their existing cargo, riders and
trailer references after restore. New commerce freight variants contribute
cargo, not imaginary passengers.

## Safe vehicle dismount

`SandboxState::exit_vehicle_with_ground` plans an authoritative placement beside
the stopped owned vehicle. Clients request `ExitVehicle`; they do not supply an
exit coordinate. The native command dispatcher supplies resident terrain,
water/slope and source-building admission. The network dispatcher uses its
authoritative human movement/coverage provider. The plain SDK `apply` command
uses the same planner with its existing level-ground assumption; terrain-backed
integrations should use the explicit ground callback.

The bounded search checks twelve nearby door, side and end alternatives derived
from the actual vehicle dimensions and heading. The selected avatar footprint
must be completely supported and clear of every resident body, including people
and dormant owners. The occupied vehicle is ignored only while crossing its own
interior; the final avatar must stand outside it. Each exit corridor samples the
rotated avatar's centre, four corners and four edge midpoints at intervals no
larger than 0.25 m, with continuous body sweeps between supported samples.
Authored facilities and structures also obstruct the corridor. A dry endpoint
cannot authorize a path through an unloaded patch, a wall or another vehicle.

Both physical and saved handling speed must be at most 0.5 m/s. Ground must begin
within one metre of the vehicle's current base, corner heights must remain
within 0.5 m of the footprint centre, and adjacent corridor samples may differ
by at most 0.4 m. Terrain providers still enforce their own walking slope and
source rules. Planning requires the complete regional world; the native and
network command paths hydrate it before admission.

If every candidate is unsupported or occupied, the command gives an actionable
parking/terrain message and leaves position, possession, parking, cargo, fuel
and money unchanged. On success the avatar takes the vehicle's heading at the
admitted ground position, control returns to the avatar, and the vehicle parks
with zero velocity and engine RPM. This is bounded gameplay placement, not a
door or seated-character animation.

### Garage delivery placement

Purchases and rentals use `SandboxState::deliver_vehicle_with_ground` in the
native and network authorities. The shared planner tries at most 33 nearby
positions: the garage centre and eight directions at 4, 8, 12 and 16 m. The
actual vehicle footprint, with a 1.1 m walking strip around it, must be clear of
all resident bodies and authored facilities/structures. A grid with at most
0.5 m spacing checks resident support and model-height source clearance across
the complete parking pad; support may vary by at most one metre from its centre.
The supplied source providers retain their ordinary vehicle water and slope
rules. This is bounded reference parking, not a sourced parking-lot map.

The planner runs before the existing purchase/rental transaction. No usable bay
means no new unit ID, charge, fuel transfer or rental bond. Repeated deliveries
choose distinct usable spaces instead of stacking onto the starter rover at the
garage centre. Existing parked/saved vehicles are never relocated. Plain SDK
commands use the same occupancy and parking rules on assumed level ground;
source-backed integrations must supply the resident support callback.

## Prediction and native presentation

`ClientPrediction::preview_with_surface` queries support at each replayed
vehicle pose, including turns and material transitions. It retains the existing
0.25-second prediction horizon and validated resolver. Missing support stops
the preview and zeroes its transient velocity. The legacy fixed-surface
`preview` convenience method remains available. Neither method commits private
inventories, authority, fuel, odometer, saved positions or other actors' contacts.

The native chassis aligns to the shared four-wheel support plane. Each visible
wheel then samples terrain at its actual position on the interpolated body.
Residual height drives `terra_core::suspension_spring`, an exact critically damped
presentation spring with ±0.35 m travel and bounded timestep/velocity. Constant
targets produce matching results under different frame subdivisions. The
wheel's rolling and steering rotations remain separate from travel. A true
pause freezes the spring; missing terrain preserves its last supported travel
rather than guessing a new surface. Model replacement still rebuilds only the
vehicle's own wheels and lamps, preserving game-owned attachments.

This visual suspension does not exert gameplay forces, lift the body into the
air or simulate spring mass. It is terrain-driven presentation with explicit
limits. Autonomous road vehicles now use the same bounded handling model and
four-wheel support contract as manual control and its owner preview.

## Autonomous driving and recovery

Autonomous drivers produce `DriveInput`: throttle, service brake and steering.
The shared `drive_actor` integrator calculates every candidate pose. Road speed
limits, upcoming corners, final-stop braking, queue clearance and intersection
stop lines constrain the controller's requested speed; they never directly
write a waypoint position or rotate a stopped body to face its destination.
Following gaps constrain progress toward the leader, so a blocked follower can
still reverse away through the ordinary source, contact and stop-line checks.
The forward gap remains a hard admission bound toward that leader; allowing
reverse does not grant an intersection permit or a body-contact exemption.
Registered vehicles use their actual model, upgrades, condition, cargo,
passengers and trailer load. Unregistered traffic and couriers use the bounded
reference vehicle configuration and their existing contact dimensions.

The native and network authorities call
`advance_living_with_vehicle_environment` with actual wheel material samples,
weather grip and model-sized source-envelope admission. Each admitted physical
step is at most 100 ms, with at most fifty substeps for the public five-second
living timestep. The complete envelope is checked at quarter-metre translation
plus rotational travel, followed by dynamic-body contacts. Unknown terrain or
an unavailable wheel stops motion without distance, fabricated impact damage,
arrival credit or terrain fallback. Loading the missing terrain permits the
same order to continue.

Compatibility callers of `advance_living_with_environment` remain supported:
their movement resolver supplies wheel height and all envelope samples, with
neutral paved grip. Games whose source has different materials should use the
explicit vehicle-environment API. The callbacks must be resident/read-only;
they must not fetch network data inside a simulation step.

Ordinary forward route corners are guidance points. Final goals require a
low-speed arrival before the order completes. An intermediate reverse leg that
leads into a forward leg uses the same nearer arrival and low-speed checks;
the controller brakes before consuming that point instead of undoing a retreat
while still reversing. Low-speed reverse also handles nearby rear-quarter
targets that would otherwise require a forward loop into a held stop line.
Separate entry and continuation thresholds keep steering changes from flipping
the requested direction each frame.

Stationary-body detours remain bounded route suggestions. Both proposed legs
must pass the complete source envelope, geometric traffic clip and body sweep,
with exact endpoint checks. A supported corner alone does not establish a
usable approach. Planning uses the controller's reverse-facing choice instead
of sweeping an unnecessary half-turn for a rear leg. Actual movement still
passes normal bounded steering and all live admission checks.
An obstructed company driver retains the existing timed blocked-route state;
clearing an obstruction and resuming never bypasses physics or grants a fare.

If another stopped body blocks both original one-body proposals, the driver
can widen a proposal around a small encountered cluster. At most four stopped
bodies contribute bounds; moving bodies retain their ordinary stop/yield
priority. The two outer corners and the reconnection to the original target
must all pass source envelopes, stop-line clipping and body sweeps. Corners
remain within 64 m of the car and the complete proposal within 160 m. A shared
budget admits at most 12 additional corridor attempts per vehicle update,
including all substeps and clearance retries. Rejected or exhausted proposals
retain the existing order. Every accepted pose still comes from `drive_actor`
with the same steering, braking, source, contact and arrival rules.

The original incremental one-body maneuver also checks the complete source
footprint of its reconnection, with a 160 m three-dimensional segment limit
checked before invoking any source callback. The complete original pose sweep
is divided into calls with at most 64 m of translation plus rotational travel
(0.1 m tolerance), each charged to the shared twelve-call allowance. This
respects the compatibility resolver’s per-call sampling budget; a rejected
later segment cannot commit the earlier parts. Its future body occupancy and
traffic reservation
are evaluated when each physical step reaches them, so transient future
occupancy does not prohibit a necessary present escape. No actual movement
bypasses live source, contact or stop-line admission. Cluster and narrow
proposals retain full traffic/body/source admission on every proposed leg.

When those wider proposals cannot fit, a parallel parked vehicle can admit a
narrower two-corner alternative based on projected body widths. Its lateral
margin is 0.8 m; the approach retains both body radii plus 3.5 m of longitudinal
room. Its complete three-leg route, source envelopes, contacts and stop-line
clips share the existing twelve extra-corridor allowance with cluster search
and the new one-body reconnection checks. Moving, routed and crosswise bodies
cannot seed this narrow proposal. The tested 9.5 m supported strip requires an
adequate approach. If coverage returns after a close approach, a straight
two-to-eighteen-metre reverse can supply nine metres before the first corner.
The complete retreat, two corners and reconnection must all pass admission
before this three-point maneuver is saved. The reverse uses normal braking and
low-speed arrival. Missing rear support or another blocking body remains a
reason to hold; no fallback shifts the vehicle into free space.

Only after admitting a new `Move` maneuver, consumed waypoints are removed from
the saved prefix. Unconsumed points and the final destination remain. This keeps
long blockage/retry cycles from exhausting the unchanged 129-point route cap
with historical retreats, while exact save/resume and patrol semantics remain.

Outward clearance retries can make room for moving pedestrians, wildlife and
stopped vehicles that still have active routes. For a vehicle `Move` or `Patrol`
order, a fully admitted short retreat can persist as a waypoint in the saved
route, preventing its next forward input from immediately undoing the maneuver.
For a stationary obstruction, both `Move` and `Patrol` orders can use a complete
admitted two-corner detour. A patrol saves those corners in its temporary path,
preserving its original circuit, cursor and direction; it proposes another
maneuver only after that prefix drains. If the initial turning corridor cannot
be admitted, a clear straight outward leg can persist instead. This creates physical turning room
before retrying the route; it does not relax the blocked rotating sweep or move
the obstacle.
A patrol consumes its temporary path from the front; both the controller and
junction reservations inspect that same path before the original patrol.
Crossing pedestrians likewise retain their chosen walking detour until its
corners are reached, preserving the original patrol cursor and direction.
The combined policy resolves the reference scene's repeated sidestep/forward
retry conflict while retaining ordinary physical motion and current occupancy
checks. See [Contacts and routing](CONTACTS_AND_ROUTING.md) for the bounded
planning distances, queue semantics and save-validation rules.

Registered vehicle handling remains in `VehicleState::dynamics`. NPC handling
is an additive, bounded map in the saved `TrafficControl`, with defaults for
older saves. A true pause preserves speed, steering and pose. Resumption uses
the saved handling state rather than resetting every autonomous vehicle's
steering each tick. Fuel and odometer accounting sum accepted curved substeps;
only a physical arrival can complete the existing funded deliveries.

Recovery is local and conservative. It can hold indefinitely when every
admissible corridor is occupied or unsupported; it does not search a global
navigation mesh, move an obstructing parked body, or guarantee traffic flow in
an arbitrary layout. Changing coverage or clearing an obstruction permits the
existing route/recovery rules to proceed with their original cargo and escrow.

The archived 0.10 occupied-exit save exposed a specific policy mismatch: a
patrol could retain a straight retreat but could not use the checked static
detour already available to a `Move` vehicle. It repeatedly backed clear and
retried the same stopped-body corner. Allowing that existing detour on a patrol's
temporary prefix lets the queued road user clear and the original loaded
courier return under unchanged traffic permits, source/body admission and
handling. Both original synthetic saves are retained byte-for-byte with hashes;
their regression requires the exact physical 12-wood return inside the original
120-second replay window, exact goods/money conservation and exact serialized
continuation. The original flat SDK fixtures return after 34.0 and 49.5 seconds
in the reviewed implementation. This establishes recovery for those saved cases,
not automatic clearance of arbitrary occupied layouts.

## Regression coverage and remaining work

Focused regression commands (use the repository's installed Rust toolchain):

```sh
cargo test -p terra-core --offline --locked vehicle
cargo test -p terra-sandbox --offline --locked --test vehicle_ground
cargo test -p terra-sandbox --offline --locked --test vehicle_exit --test garage_placement --test transport --test rentals
cargo test -p terra-sandbox --offline --locked --test autonomous_driving --test traffic --test living
cargo test -p terra-sandbox --offline --locked --test navigation_clusters -- --nocapture
cargo test -p terra-sandbox --offline --locked --test traffic_recovery --test walking_routes
cargo test -p terra-net --offline --locked --test spawn_placement --test owner_dormancy
cargo test -p terra-net --offline --locked --lib prediction::tests
cargo test -p terra-bevy --no-default-features --offline --locked vehicle
cargo test -p terra-demo --offline --locked vehicle_presentation::tests
```

The tests exercise uphill/downhill brake hold and release, low-grip slip,
combined braking/turning limits, rotated/date-line terrain sampling, missing and
rough wheel contacts, spring pause/subdivision behavior, source/bridge material
agreement between native and host queries, parked motion, loaded acceleration,
braking/fuel, pickup/delivery mass transitions and exact saved load resumption.
Native ECS tests verify wheel travel from loaded terrain, pause/missing-coverage
behavior, model replacement, public/manual steering and unchanged authority.
Dismount regressions cover complete and rotated footprint support, unsupported
corridors, dormant occupants, overlapping vehicle corridors, authored structures,
saved handling speed, date-line/polar placement and exact save/resume results.
Native and network command-dispatch tests reject missing source coverage, then
require a clear supported avatar placement after the same terrain becomes usable.
Garage tests cover purchased and rented delivery beside the starter crew, a
long bus and repeated rentals, full-pad support, blocked parking, subsequent
boarding/dismount and exact funds/fuel/bond conservation. The native and network
dispatch fixtures also reject garage delivery without resident coverage before
admitting it on restored source ground.

Full rigid-body contact impulses, dynamic spring forces/load transfer, tire
contact patches and slip curves, airborne/rollover motion, articulated trailer
axles and collision, per-item physical densities, drivetrain detail and
force-feedback/device certification remain production work. The existing
trailer follows the towing body kinematically; adding tare mass does not solve
trailer articulation. Collision admission limits are documented in
[Contacts and routing](CONTACTS_AND_ROUTING.md).

## Design references

The general separation of support queries, suspension/tire behavior and a
combined tire-force budget is consistent with the official
[NVIDIA PhysX Vehicles documentation](https://nvidia-omniverse.github.io/PhysX/physx/5.6.1/docs/Vehicles.html).
Terra does not embed PhysX or claim its solver behavior. Material names follow
the [OpenStreetMap `surface` key documentation](https://wiki.openstreetmap.org/wiki/Key:surface);
the coefficients and simplifications above belong to this reference game.
