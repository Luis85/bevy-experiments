# Transport domain and driving reference

This increment adds a reusable, authority-owned road transport domain to
`terra-sandbox`. It is an authored reference game loop attached to the existing
geographic world and economy. It does not assert feature parity with Motor Town,
AAA production readiness, or an unbounded MMO simulation.

## Playable progression

The existing utility rover becomes an owned pickup when the transport scenario
is enabled. A new driver starts with a road license and the existing account
balance. The garage sells nine original vehicle archetypes: compact, pickup,
taxi, city bus, box truck, tractor, recovery truck, ambulance, and fire engine.

Use a pickup for passenger lifts and small freight contracts. A completed
contract unlocks purchase of the passenger license; three completed contracts
unlock the heavy-goods license; five unlock the emergency-service license.
Licenses and vehicles have separate prices. The garage supports refueling,
condition repair, eight paint choices, and three engine/brake upgrade levels.
Selling requires an empty, unassigned vehicle, with the player outside it and
the trailer detached. Resale is discounted and depends on condition.

Passenger services have finite waiting queues. Outbound and inbound services
move people between facilities without creating new passengers. Freight moves
existing catalog goods between actual settlement producer/shop inventories.
The reference offers include timber for the sawmill, irrigation water, bakery
flour, and bulk water. Specialized dispatch offers also cover recovery, medical
transport, and fire-station supplies. Recovery is a staged dispatch contract;
there is no winch, damaged target vehicle, or towing rigid-body simulation.
Fire service transports real water cargo; it does not extinguish simulated fires.

The cargo trailer increases compatible vehicles' capacity and follows its
towing vehicle geodetically. Attachment requires ownership, proximity and a
stopped vehicle. A loaded or assigned trailer cannot be detached. This is a
reference hitch/capacity model, without articulation or trailer tire dynamics.

Time trials use an ordered, closed sequence of separated geographic checkpoints.
Each gate requires physical proximity, positive elapsed time, and new vehicle
mileage since the previous gate. Repeated commands at a stationary checkpoint
cannot advance the course. Results are personal bests; races do not mint money.

## Prepaid rentals and physical return (0.8)

At **Garage → Rent before you buy**, inspect the selected model's five-minute
fee, security bond and supplied fuel before renting. A compact rental costs
**61.80 personal credits** initially: 1.80 fee, 45.00 bond and 15.00 for 10 litres.
The pickup costs **77.40 credits**: 2.40 fee, 60.00 bond and the same fuel amount.
Both fit the authored new-player wallet. Higher vehicle classes still require
normal licenses. Rental bodies consume the same player, total vehicle and
aggregate resident-plus-cold body capacities as purchases.

`RentVehicle` supplies **300 physical seconds**, **10 litres** from the chosen
garage's finite tank, and a bond equal to **25% of model purchase price**. The
five-minute fee is **1% of purchase price**. Fees and fuel payments enter the
service treasury; the bond is held separately in `VehicleRental.bond_escrow`,
never counted as treasury money.

**Renew selected rental** adds another 300 seconds for the same model fee, from
any location, up to one prepaid physical hour. Time freezes while the owner is
dormant and is not multiplied by the calendar. Expiry stops authoritative motion
and preserves the body, cargo, jobs, company assignment, fare and security bond.
The Garage and vehicle labels explain the renewal path. Rental fleets suspend
payroll while expired. Renewing does not teleport or automatically cancel a job.

Drive back to the **original supplying garage**, stop within 20 metres, exit,
unload/finish or return funded work, release company assignment and detach any
trailer before returning. **Return empty rental** shows the current damage, bond
and fuel breakdown and asks for confirmation. Authority recomputes the refund
and rejects a changed quoted total before mutation:

- Damage withheld is the lesser of the bond and 45% of purchase price multiplied
  by missing condition, rounded up to a minor credit.
- Refund the remaining bond from escrow.
- Return all remaining fuel to the supplying garage and refund it at 1.50 credits
  per litre, rounded down to a minor credit, from the finite service treasury.
- Prepaid time is not refunded. Return removes the empty vehicle/body exactly
  once; it does not sell a rental for its purchase/resale value.

Insufficient refund funds, a full personal wallet or full garage tank reject the
return atomically, preserving the body and bond. Mechanical rental upgrades are
not offered; fuel, repair, paint, signals and compatible trailer use retain their
ordinary requirements. Renting and returning without use loses the rental fee;
there is no fuel or bond duplication. The reference dealership still creates its
bounded vehicle equipment as authored scenario stock; this is not a finite
manufactured rental-fleet inventory, automatic repossession, mileage lease,
insurance or credit system.

`VehicleState.rental` is optional and defaults to `None` for earlier saves. Only
the owner's private vehicle view includes prepaid time and bond. Public
`VehicleAppearance` contains neither. [Rental tests](../crates/terra-sandbox/tests/rentals.rs)
cover conserved money/fuel, quotes, repeated return, expiry, cold dormancy,
forgery rejection, privacy and expired loaded company recovery.

## Authority contract

```rust
// After population placement has been resolved against resident terrain:
state.ensure_transport_demo(&catalog)?;

// A trusted principal may install cosmetic identity before population exists.
state.set_character_profile(player, CharacterProfile {
    name: "Mara Rivera".into(), body_style: 1, skin: 3, hair: 2, outfit: 4,
})?;

// Untrusted clients send intent through the existing command envelope.
state.apply(player, Command::Transport {
    action: TransportCommand::AcceptJob { job_id, vehicle_id },
}, &catalog)?;

// After authority-controlled terrain/collision validation and movement:
state.record_vehicle_motion(player, vehicle_id, distance_m, seconds, impact_speed)?;
```

`VehicleState::dynamics_config()` supplies model-specific mass, wheelbase,
power, brake force, grip, cargo and upgrade effects. Local authority, host and
private prediction use `TransportWorld::vehicle_dynamics_config(vehicle_id)` to
also include actual onboard passengers and attached trailer tare, without
counting trailer cargo twice. Reservations waiting at pickup add no mass.
`VehicleState::speed_limit_m_s()` also accounts for fuel, condition, parking
brake and the existing bounded road-actor capability. Both local and server
adapters must resolve the candidate geographic path before committing motion.
Clients never submit odometer, condition, fuel, speed configuration, raw handling
state or authoritative coordinates as transport commands.

`VehicleKinematics` persists signed speed, lateral speed, steering, yaw rate,
RPM and gear. The reference core model is a surface bicycle model. Four loaded
wheel samples supply source material, pitch, cross-slope and roughness. Braking
and cornering share a bounded contact-force budget; missing wheel coverage stops
motion. Native wheels show independently sampled, damped travel. See
[Vehicle dynamics](VEHICLE_DYNAMICS.md) for the shared APIs, tuning, regression
checks and the limits of this model. A full rigid-body and tire solver remains
game-owned work.

`advance_living(real_dt, sim_dt, ...)` advances the transport clock exactly once
using physical seconds, and accounts fuel/wear/distance for registered vehicles
under RTS orders or autopilot. The manual driving adapter accounts only the
possessed vehicle. Accelerated calendar time does not shorten a driving deadline
or improve a race time. The existing nominal NPC traffic and hauler speeds are
unchanged. Empty fuel, severe damage, expired rentals and an engaged parking
brake stop both manual and RTS operation. An accepted explicit RTS move releases
the parking brake on its registered vehicles, after the entire selected group
passes ownership, licensing, company-assignment and distance checks. Rejected
orders leave every brake and order unchanged. Entering or starting road autopilot
also releases the brake; exiting and releasing a company vehicle park it until
a new driving instruction. Generic possession enforces the same vehicle license
as entering through the transport command. RTS movement still uses the ordinary
fuel, rental, handling, terrain and collision limits.

The parked walking avatar remains the driver's needs record. Moving a possessed
registered vehicle consumes 0.00035 avatar energy per physical second, including
when an earlier order left that avatar resting. Food and water continue their
existing calendar-time decay. `SandboxState::vehicle_speed_limit_m_s()` combines
license and mechanical limits with the present driver's existing needs factor
(35–100%); `VehicleState::speed_limit_m_s()` remains the mechanical limit.
Authorities use the state helper for manual driving. Unpossessed RTS/autopilot
vehicles do not inherit the owner's parked-avatar fatigue. Stopping allows the
normal avatar recovery rules to operate; eating and drinking require leaving
the vehicle and controlling a person.

Opening the political atlas sends protective brake input. A possessed local
vehicle also brakes if the presentation supplies no active manual input, so
stored momentum cannot freeze during a control/view transition. A true local
pause still freezes physical simulation. Successful restores reconcile the
saved possessed body into third-person presentation once; ordinary state
revisions do not undo a player's later RTS or first-person choice. That restore
serial is runtime-only, not part of a saved world's gameplay data.

Swept vehicle admission distinguishes missing source coverage from a known
physical obstruction. An unloaded corner of the vehicle's support envelope
stops movement without collision damage; a covered solid obstruction retains
the impact/condition rule.

## Economic and material invariants

Transport commands prepare a bounded candidate of the affected player, living
and transport ledgers, validate it, and commit together. They do not clone the
terrain, source geometry, resource-node working set, or dormant regional pages.

1. Accepting a job debits its reward from the finite issuer treasury into escrow.
   Freight is removed from existing source stock into a reservation. Stock
   already reserved for an NPC courier is unavailable. Passenger reservations
   debit the facility's waiting queue.
2. Pickup requires the assigned, active vehicle to be stopped at the pickup
   facility. Goods move from the reservation into vehicle cargo. Required
   vehicle model, license, cargo capacity and passenger seats are checked.
3. Completion requires that same vehicle to be stopped at the destination.
   Goods move into its existing producer/shop inventory; passengers enter the
   destination queue; the escrow moves to the driver's account exactly once.
4. Cancelling before pickup returns reserved goods, passengers and reward.
   Cancelling after pickup requires physical return to the original facility.
   Loaded jobs that expire enter return mode. Expired uncollected reservations
   refund automatically; an exceptional full account/inventory retains escrow
   and retries without dropping any value.
5. Vehicle purchases, sales, refueling, repairs, upgrades, paint and licenses
   transfer credits between the driver's existing wallet and the finite service
   treasury. Included delivery fuel comes from garage stock; selling returns
   remaining fuel. Consumption and wear occur only through authority accounting.

The initial transport scenario supplies explicit finite service capital and
garage fuel. Seeding is idempotent: restoration, chunk streaming and later joins
do not refill those stocks or reset vehicles, races, licenses or job progress.
The preexisting starter vehicle fuel is authored starting equipment. The
reference dealership does not model vehicle manufacturing, replacement-parts
stock or a commercial credit market.

## Persistence, boundedness and privacy

`SandboxState.transport` and `SandboxView.transport` are additive serde-default
fields. Earlier saves load with transport disabled and no newly minted capital.
The existing builtin item catalog and its identity are unchanged. A game can
enable the reference scenario explicitly after loading a prior world, or author
its own validated transport records.

Vehicle ownership, physical cargo, reward escrow, profiles, licenses, mileage,
fuel, condition, customization, hitch references, job phases, deadlines, races,
personal bests and handling state are durable. Transport remains in the regional
global ledger for company/vehicle metadata and funded commitments. Offline
owned bodies can retire into cold pages; validated private cold references keep
those commitments coherent. Global metrics count transport metadata, while
retired bodies count as dormant regional records.
Buying a vehicle checks the existing aggregate 512-body capacity, including
cold regional reservations; the new system does not increase that cap.

Additional transport limits are 128 registered vehicles in the world, 12 per
player, 24 facilities, 32 offers, two accepted jobs per player, 32 trailers and
eight race courses. All domain loops are bounded. Geographic movement and local
commands remain constrained by existing resident-terrain admission.

Recipient views include only that player's private driver progress, owned
vehicle state, accepted jobs and personal race results. Other players'
accepted offers are omitted. Public profiles contain bounded cosmetics only.
Public `VehicleAppearance` contains model, paint, lights, indicator, steering
and a hitch reference for visible owned bodies. An attached trailer is public
when its towing body is visible; an unhitched trailer is visible only to its
owner. Other players' wallets, licenses, fuel, condition,
cargo and progress are absent. Appearance owner and body IDs must match the
living interest view. Budget trimming removes dependent appearances, accepted
jobs and races together with omitted bodies. Client reconstruction keeps a
nonserialized appearance cache and cannot be saved as authoritative full state.

Numeric owner keys in public profile maps use a bounded custom deserializer so
they also work inside serde's internally tagged network envelopes. Duplicate,
noncanonical or excess owner keys are rejected.

## Motor Town reference coverage

The comparison uses the developer/publisher's [official Steam product page](https://store.steampowered.com/app/1369670/Motor_Town_Behind_The_Wheel/),
reviewed 2026-10-27. Its advertised breadth includes extensive vehicle content,
tire physics and assist systems, passenger/freight work, special services,
vehicle purchase/rental and parts, racing and leaderboards, company routes and
hired drivers, multiplayer, and multiple input devices. A finite reference
implementation is not equivalent to that released game's accumulated content
and behavior.

| Area | This increment | Remaining product work |
| --- | --- | --- |
| Road handling | Shared bounded acceleration/braking/steering; four-wheel source support and material grip; onboard/trailer load; braking/turning force budget; independent visual wheel travel | Dynamic suspension forces, tire contact patches, drivetrain detail, impulse/crash response and physical wheel/force-feedback certification |
| Vehicle content | Nine original domain archetypes with distinct specifications | Larger vehicle library, detailed interiors, skeletal animation and production assets; shared native wheels now roll and steer |
| Passenger work | Finite queues, lift/taxi/bus eligibility, real pickup/destination, funded rewards | Individual boarding animations and schedules; companies now author multi-stop routes |
| Freight | Actual producer/shop goods, reservations, vehicle/trailer capacity, arrival and return workflows | Container and pallet handling, articulated trailers, complex loading equipment |
| Garage and progression | Ownership, finite purchases/sales, prepaid rental/bond/renew/return workflow, licenses, fuel, condition, paint, engine/brake levels | Finite manufactured rental fleets, mileage leases, detailed parts inventories, tuning UI, inspections |
| Special service | Recovery dispatch, ambulance passengers, fire-service water cargo | Physical recovery/winch mechanics, medical treatment, fire response, policing |
| Racing | Ordered course, physical mileage, physical-time results, personal bests | Competitive race events, autocross authoring, online leaderboards |
| Company operation | Finite account, employee hire/dismissal, paid household retail demand, private warehouse markets and physical sale/transfer routes, buyer escrow, storage reservations, return/handover, public funding and payroll | Playable employment, insurance, individual passenger schedules and production-server career balance |
| Networking | Authority and interest replication with private ledgers, public appearances, verified TLS, bounded owner prediction/reconciliation and trusted principal access controls | Public account enrollment/recovery, Internet latency/loss qualification, distributed region handoff and measured population-scale operation |

The implementation tests focus on value conservation, physical arrival,
authorization, atomic rejection, duplicate payout prevention, deadlines,
privacy, regional capacity and exact save restoration. They establish those
specific behaviors; they do not establish production-scale concurrency or
complete Motor Town parity.

The detailed [Motor Town benchmark and release acceptance](MOTORTOWN_PARITY.md)
maps current native entry points to implemented, partial and open capabilities,
with explicit next gates for commercial gameplay, AAA presentation and MMORPG
operation.


## Occupancy, commercial routes and presentation

[Companies](COMPANIES.md) expose the same command boundary through Fleet [Y].
Assigned vehicles cannot be possessed, ordered manually, sold or reused for jobs
until the driver is paused, unloaded and released. Employees have the bounded
road/passenger/heavy qualification policy documented there; emergency service
remains outside their contract. Loaded fares/cargo and fractional wages survive
pause, hold/return/handover, insufficient funds, blocked routes, expired rental time,
dormant owners and complete saves. Private warehouse sale routes reserve real shop
purchase money; internal transfers receive no fare. Garage/freight bays protect
loaded return and private arrival capacity, and nearby markets trade actual goods
with quote-checked company funds. The [company guide](COMPANIES.md) explains these
workflows, recovery choices and private depot/household economy.

[Contacts and routing](CONTACTS_AND_ROUTING.md) describe continuous translation
admission against occupied bodies, conservative angular bounds, bridge-height
separation, initial-overlap escape and shared local/host/autonomous integration.
Route access uses segment projection; reference right-side lanes and directed
patrols prevent the former centerline conflicts. Body contact is still a bounded
kinematic stop/admission policy, without impulse exchange, crushing, dynamic
suspension forces or articulated trailers. Derived terrain support and visual
wheel springs do not turn that admission policy into rigid-body physics.

The native demo caches directed source-road guidance. Missing roads or an
unrestricted disconnected graph can use a labeled direct bearing. Known access
or direction restrictions and unavailable or over-budget routes show their
reason and draw no forbidden shortcut. A north-up map shows roads, stops, traffic and
the route. These are advisory reference paths, not turn-by-turn legal lane
navigation. Wheels inherit the smoothed body pose, roll with signed movement and
steer with available authoritative handling. Custom game-owned actor scenes hide
the procedural wheels. [Controls](CONTROLS.md) documents editable keys and the
analog gamepad path plus configurable calibrated Bevy/Gilrs wheel/joystick axes.
Virtual-device tests cover input safety; physical device compatibility and force
feedback remain open.
