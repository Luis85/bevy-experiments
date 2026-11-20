# Motor Town benchmark and release acceptance

Terra implementation reviewed for v0.12 on **2026-10-28**. The external benchmark
record below remains the earlier review's source record, not a new market audit.

Terra now has a connected, playable road-transport reference: create a character,
buy or rent a vehicle, drive with persistent handling and fuel, accept funded
passenger or freight work, visit real stops, earn credits, and operate a company.
The 0.10 increment adds continuing state-derived goals, shared autonomous vehicle
handling, validated animated wildlife packs, connected prepared terrain and
verified larger-world backup/recovery. Productive private depots, finite retail,
physical warehouse work, material/load support, management/RTS input and
connection recovery remain part of the same reference.
The 0.12 review adds complete recorded company cash flows and an honest Finances
view, measured contributed-versus-purchased-stock careers, supported nearby
walking, recovery of the exact retained traffic queues, aggregate model admission
and prepared-source distribution. The [gap register](REVIEW_GAPS_v0.12.md)
separates these corrections from campaign balance, physical handling fidelity,
content production and hardware/deployment qualification.

## Benchmark and meaning of the status labels

The benchmark scope recorded by the earlier product review uses the
developer/publisher's [official Steam product
page](https://store.steampowered.com/app/1369670/Motor_Town_Behind_The_Wheel/).
Its advertised scope includes a day/night driving world, a vehicle-and-trailer
library exceeding 50 entries, passenger and freight work, specialist services,
vehicle purchases and rentals, parts, racing, commercial fleets, multiplayer,
and several input-device families. The Key Features list recorded by that review
supplies the vehicle-count benchmark; the older Early Access answer on the same
page used a lower count. This update reconciles Terra's implementation with that
recorded scope; it does not independently inventory the other game's assets or
claim a new audit of all its released features.

The developer's [2024-10-31 fleet update](https://steamcommunity.com/games/1369670/announcements/detail/4463723296443072757)
and [2025-06-28 depot update](https://steamcommunity.com/games/1369670/announcements/detail/532102384016425868)
also establish commercial fleet operation as substantial product scope. Terra now
has finite company accounts, hired AI drivers, custom passenger/freight routes,
private storage/service depots and prepaid rentals. Player employment, richer
depot markets, individual service schedules and long-duration commercial balance
remain open.
This comparison uses official material, not community guides or an assumption
that a similarly named command implements the same experience.

- **Implemented reference** means a bounded end-to-end path exists in Terra's
  native demo and authoritative domain. It still needs game-specific content,
  balancing and broader product validation.
- **Partial** means useful mechanics exist but a material part of the expected
  experience is absent or simplified.
- **Open** means no complete player-facing implementation exists. A data type,
  rendering placeholder, extension point or unrelated existing system does not
  change that status.

There is no percentage-complete score: an additional vehicle mesh does not have
the same engineering or product weight as reliable wheel support, articulated
trailer physics, commercial fleet accounting or regional server handoff.

## What to play in this increment

Run `cargo run -p terra-demo` from the repository root. **New world** leads through
character creation, world settings and a source/persistence review into a loading
screen that checks actual readiness. **Continue** resumes a managed world.
**Multiplayer** accepts a reference host and optional verified TLS configuration.
The [game-flow guide](GAME_FLOW.md) explains managed worlds, restore behavior,
source presets, private session identities and recovery errors.

For a first transport session:

1. Create a **Living frontier** world. Wait for its terrain and settlement to
   finish preparing. The existing utility rover is registered as an owned pickup.
2. Open **Driving [V]**, select the pickup and choose **Enter vehicle** while
   nearby. A starter road license is already held. Vehicle entry validates
   ownership, qualification, distance and stopped motion.
3. Browse a passenger lift or a freight offer compatible with the pickup and
   accept it. The panel shows the pickup, destination, contents, reward and
   current stage. A reward is reserved from the service treasury; freight comes
   from an existing producer or shop.
4. Follow the marked stop, come to rest and use **E** to collect. Drive to the
   next stop, stop again and use **E** to deliver. Completion pays the reserved
   reward once and advances driver progress. The small north-up map and marked
   route use the loaded directed road graph. Missing or disconnected roads have
   an explicit direct fallback; a known access or direction restriction holds
   automatic routing instead of inventing a permitted connection. Manual
   navigation is advisory; it does not enforce road law.
5. Open **Garage [G]** and mark the garage. Nearby service provides refueling,
   repair, paint and engine/brake upgrades. Qualification and vehicle purchases
   have distinct prices and requirements. The UI displays the next requirement.
6. Review **Rent before you buy** for an itemized fee, fuel and security bond.
   Prepaid time can be renewed; returning requires the empty stopped vehicle at
   its original garage. An expired loaded rental retains its job and bond until
   renewed. See [TRANSPORT.md](TRANSPORT.md) for the physical return conditions.
7. Try a cargo trailer with a compatible owned vehicle, or choose the settlement
   time trial when a course is available. The reference trial starts at the
   garage. Gates advance automatically in order when physical travel and timing
   conditions are met.
8. Checkpoint through the session menu or **F5**, then resume and inspect the
   vehicle, driver, money and job stage. **F9** deliberately restores the latest
   successful checkpoint and discards later changes; use it only when intended.

The scenario uses finite funds, station fuel, cargo inventories and passenger
queues. Source coverage and available roads determine which facilities and
course are possible. Available jobs are an authored reference set, not an
unlimited procedural career. A loaded cancelled or expired job must be returned
to its pickup. Reopening a world does not refill its treasury or fuel.

### Native controls

| Entry point | Current behavior |
| --- | --- |
| **V / G** | Open driving work or garage services. |
| **W / S** in a road vehicle | Apply forward/reverse demand through the driving solver; direction changes include deceleration. |
| **A / D**, **Shift**, **Space** | Steer, service brake, and held handbrake. |
| **P** | Toggle the persistent parking brake. Entering a vehicle releases it; exiting parks the vehicle. |
| **L**, **Z / B**, **H** | Headlights, left/right indicators, and hazards. |
| **C**, **F2 / F3** | Change chase/cockpit view, or possess the selected owned actor in third/first person. |
| **E** | Contextual collection, delivery or return at an accepted job's stop. |
| **F1** | Release possession and return to RTS selection and orders. |
| **Escape** | Cancel text/placement/political drafting first; otherwise open the session menu. Local play pauses, while a multiplayer host continues. |
| **F10 / F11 / F12** | World settings, inspector visibility, and measured performance diagnostics. |

The listed keys are defaults. **Controls** supports fifteen remappable actions,
mouse/gamepad sensitivity, invert-Y, deadzones, text scaling, first-person field
of view and optional driving camera auto-center. Gamepad focus navigation and
a bounded text keyboard cover launcher, creation, settings and session menus.
The Bevy/Gilrs wheel/joystick profile adds device/axis selection, separate or
combined pedals, asymmetric/inverted calibration and neutral rearming after
focus loss or disconnect. RTS selection, orders, management and construction
now have controller paths with explicit focus. Atlas editing retains
mouse/keyboard. Virtual-device tests do not certify physical hardware;
raw HID, force feedback, gear-shifter mapping and broader assistive technology
remain open. See [CONTROLS.md](CONTROLS.md).

## Company play

At a garage, open **Fleet [Y]**, name a company for 20 personal credits, deposit
company funds and hire a named driver for 10 company credits. In **Routes**, draft
the local passenger loop or choose distinct stops yourself, then create the
route. In **Drivers**, assign an owned empty parked vehicle and the saved route;
your active body must be another actor. Start the service and watch physical
progress, reserved fares, deliveries and payroll. An ordinary pause completes
the loaded leg before stopping; **Hold vehicle here** stops it immediately while
preserving its load. Release only an empty paused vehicle.

Fleet wages use physical time; finite stock/queue shortages wait without payroll.
A blocked route eventually parks without draining wages. Resume after clearing
the obstruction. Roadside service requires the owner to approach the stopped
vehicle and afford the quoted fee; it does not teleport the vehicle or cancel
cargo. Offline companies freeze, including loaded cargo and economic reservations.
The **Recovery** tab can return a load to its original pickup or hand it over to
a nearby stopped, empty replacement and unassigned relief employee. The walking
owner must be nearby for handover. Goods, passengers, escrow, pickup time and
admitted travel proof remain attached to one commitment; no command teleports a
vehicle or grants a second reward. A physically impossible route can still need
the player to clear an obstruction or bring a relief vehicle. A full source may
defer the return without deleting its cargo.

At a garage, the **Depot** tab opens a private bay for 25 company credits. It
stores up to 2,000 mixed item units and removes the roadside callout fee at that
garage; fuel and repairs still cost real resources and money. Opening a depot
enables finite household purchases from nearby shops using paid wages. Employee
dismissal settles remaining earned money into the departed-household ledger.
**Fund public services** explicitly contributes company cash to the service
treasury. These loops connect production, retail demand, payroll and public
funding without an infinite replenishment grant. They do not establish indefinite
economic balance or individual passenger schedules. The
[company guide](COMPANIES.md) lists exact prerequisites, caps and ledger rules.

## Driving and vehicle coverage

| Capability | Status | Terra behavior and material limits |
| --- | --- | --- |
| Geographic exploration and changing environment | Implemented reference | WGS84 positions, streamed terrain, heightmap support, day/night and configurable world layers already share the transport world. Synthetic scenario content is explicitly identified. Geographic availability does not imply complete source coverage or finished roads everywhere. |
| Vehicle ownership and entry/exit | Implemented reference | Private ownership, nearby entry/exit, qualifications, walking avatar restoration and persistent selection. Generic possession uses the same license rule. Taking control clears old motion/control intent. |
| Road handling | Partial | Local authority, server, autonomous drivers and owner preview share a bounded surface bicycle model with sampled four-wheel support, source material grip, combined braking/cornering limits and actual cargo/passenger/trailer load. Fuel and damage constrain operation. Dynamic suspension forces, contact-patch tires, a detailed drivetrain and rollover simulation remain open. See [VEHICLE_DYNAMICS.md](VEHICLE_DYNAMICS.md). |
| Collision and terrain support | Partial | Resident terrain/source admission is followed by continuous oriented-body sweeps against other living bodies. Translation, conservative turns, overpasses and initial overlap escape are covered. Contacts stop/yield and bounded manual impacts damage condition; rigid-body impulses, dynamic suspension forces and articulated trailer contacts remain open. |
| Vehicle content | Partial | Nine original archetypes: compact, pickup, taxi, bus, box truck, semi tractor, tow truck, ambulance and fire engine. They have separate dimensions and specifications, original procedural silhouettes and eight paint choices. The much larger benchmark asset library is open. |
| Cockpit/chase presentation | Partial | Both control views, a camera-space dashboard, speed/gear/fuel/condition HUD, terrain alignment, independently rolling/steering wheels, lamps and bounded nearby headlights exist. Each visible wheel samples terrain and uses a bounded visual spring; it does not exert gameplay suspension forces. Detailed animated cabins, mirrors and finished vehicle audio remain open. |
| Qualifications and driver progress | Implemented reference | Completed jobs, XP, level, earnings, distance and paid passenger/heavy/emergency licenses persist. The demo progression is an explicit reference rule set, not a reproduction of another game's balance. |
| Driver needs | Implemented reference | Moving a possessed registered vehicle fatigues the parked avatar in physical time. Its food, water and energy constrain the manual driving cap; autonomous vehicles keep their mechanical cap. A parked Rest order cannot recover energy while driving. Food and water retain the existing calendar-time rules; exit the vehicle to eat or drink. Suggested guidance prioritizes critical needs and links to the existing park/exit, consumption or rest flow. |
| Purchase, resale and servicing | Implemented reference | Finite-wallet transactions, condition-based resale, station fuel, refueling, repair, paint and three engine/brake upgrade levels. Physical service range and stopped motion are required. |
| Detailed parts and tuning | Partial | Upgrade levels alter actual handling configuration. Parts inventory, component wear, engine temperature, configurable drivetrain/tire setups and inspection workflows are open. |
| Rentals and leasing | Partial | Itemized prepaid fee/fuel/bond, renewal, expiration, physical return and atomic damage/fuel/bond settlement exist. Loaded work and escrow survive expiration. Manufactured finite rental fleets, mileage leases, insurance and automatic repossession remain open. |
| Vehicle signals and navigation | Partial | Lamps/signals, advisory map/bearings and right-side routes share a directed graph with supported access, one-way, numeric speed and layer/bridge/tunnel rules. Known restrictions fail closed; disconnected/no-road fallbacks remain explicit. Physical intersection queues coordinate automatic traffic, with authored direction/stop markings. Full legal lane/turn relations, signals, enforcement and horn/audio remain open. |
| Input-device breadth | Partial | Remappable keyboard controls, ordinary gamepad movement/look, controller menu focus/text entry, text/FOV/motion preferences and a saved Bevy/Gilrs wheel/joystick axis profile with separate/combined pedal calibration exist. Wheel input is bounded and requires neutral after focus loss/disconnect. Management focus, text drafting, controller RTS cursor/orders and building placement are implemented; atlas drawing remains mouse/keyboard. Physical certification, raw HID, shifter mapping, force feedback and broader accessibility remain open. |

## Work, logistics, competition and long-term play

| Capability | Status | Terra behavior and material limits |
| --- | --- | --- |
| Passenger lifts, taxi and bus work | Implemented reference | Finite waiting queues, eligible vehicle/seat checks, real pickup and destination stages, deadlines, return handling and funded payment. Individual riders do not yet animate boarding or occupy rendered seats. |
| Custom scheduled passenger routes | Partial | Company owners author named loops of 2–8 distinct stops, choose passenger count and assign a qualified owned or rented vehicle/driver. Each leg uses finite queues, travel and escrow. Household transit spending contributes real issuer funding but does not create passengers. Individual recurrent rider schedules, clock timetables, service-quality scoring and in-place editing of loaded routes remain open. |
| Freight deliveries | Implemented reference | Actual goods move from producer/shop stock into reservation, assigned vehicle cargo, then destination stock. Existing NPC reservations cannot be taken by player jobs. Pickup and delivery require the assigned active vehicle to arrive and stop. |
| Trailers and cargo handling | Partial | Owned cargo trailers, stopped nearby hitching, capacity increase, loaded-detach restrictions, persistent attachment and public attached-trailer appearance. Following is kinematic; articulation, pallet/loading equipment, mass distribution and trailer braking remain open. |
| Recovery work | Partial | A qualified tow vehicle can complete a staged, funded recovery dispatch. There is no broken target vehicle, winch, physical hookup or tow-contact simulation. |
| Medical work | Partial | An ambulance can carry the reserved passengers of a medical dispatch. Treatment, injuries, ambulance equipment and individual casualty behavior are open. |
| Fire response | Partial | Fire-service jobs deliver real water cargo to a station. There is no spreading fire, hose system or extinguishing mechanic. |
| Police and other specialist careers | Open | No traffic enforcement, pursuit, fines, arrest lifecycle or complete additional specialist job chains. A vehicle class alone would not complete those careers. |
| Settlement industry and traffic | Partial | Production, shops, funded supply work and physical couriers share real endpoints. Paired loading-bay reservations and physical-time FIFO intersection permits address distinct conflicts; exit checks, bounded approach requeue and actual occupancy prevent invented passage. Source speed limits cap automatic vehicles. Full lane/turn laws, signals and general congestion/deadlock recovery remain open. |
| Company ownership and hired labor | Partial | Separate finite accounts, up to four employees/eight routes, payroll, loaded hold/return/handover, dismissal, private depot storage/service, household retail demand and explicit public funding are connected. Rental vehicles can serve qualified work. Cargo/proof and wallets survive failure, dormancy and restore. Player employment, broader depot market models, individual staff activity and recurrent schedules remain open. |
| Time trials | Implemented reference | Ordered geographic checkpoints, positive physical elapsed time, mileage evidence, one gate per authority tick and persistent personal bests. Repeated stationary commands cannot advance a run. |
| Competitive races and autocross | Open | No complete event lobby, grid/start procedure, synchronized competitors, stewarding, route authoring or autocross rules. |
| Online rankings | Open | Personal bests are private world state. There is no ranked backend, signed result submission, moderation or competitive anti-cheat service. |
| Ongoing economic career | Partial | State-derived supply, home, community and road goals connect jobs, trade, gathering, crafting, farming, construction, territory progression and company work. Wage-funded household purchases consume real shop stock and transit spending funds the issuer; production replenishes through existing recipes/resources. Explicit contributions and finite service/rental costs transfer existing money. Broader demand models, productive fleet/depot specialization and long-duration balance evidence remain open. |

Housing, building/destruction, farming, survival needs, factions, territorial
claims, historical packs and politics remain Terra systems in their own right.
Their presence expands the framework; it does not compensate for unfinished
driving features. See [gameplay](GAMEPLAY.md), [transport contracts](TRANSPORT.md)
and [architecture](ARCHITECTURE.md) for those system boundaries.

## Multiplayer, persistence and framework status

The implemented reference runs through an authoritative host. Clients submit
driving intent rather than speed, fuel, odometer or raw geographic state. The
host validates possession, ownership, sequence freshness, physical movement and
command conditions. An old client cannot drive a registered vehicle by sending
a nonzero generic velocity command. Idle controls brake, and releasing or
repossessing a vehicle does not reuse an earlier throttle command.
Atlas entry and absent local manual input also brake a possessed vehicle.
Successful restore adopts the saved possessed body into chase view once without
overriding subsequent RTS or camera choices. True local pause preserves the
paused simulation. Missing vehicle-envelope coverage stops movement without
being treated as a damaging impact.

Transport views expose another visible vehicle's model, paint, lamps, steering
and attached trailer while withholding its fuel, cargo, condition, driver
progress and wallet. Cosmetics are bounded and validated against visible living
bodies. A detached foreign trailer is omitted. Interest eviction removes its
dependent appearance; client appearance caches cannot be saved as full authority.

Vehicle configuration, handling, ownership, rental time/bond, cargo, escrow, job
stage, fuel, condition, licenses, races, company/depot ledgers and profile data
survive complete saves and regional checkpoints. New transport fields are
additive defaults, and the existing item
catalog identity is preserved. Regional storage runs normal checkpoint/hydrate
work on one bounded worker with authority barriers and an explicit final drain.
Offline principals, their owned bodies, referenced homes, needs and funded work
can now freeze and leave active pages. Authenticated resume hydrates before
activation; host restart makes disconnected identities dormant. The 0.8 merge
certifies only unchanged dependency-closed cold resource/source/tombstone groups,
while current global ledgers and actual dependent cold records remain validated.
Equal guarded page payloads reuse their existing bytes. Untrusted decode and
explicit restore still receive complete canonical validation. Authority waits
behind the bounded worker barrier; this is one authority, not distributed
simulation. See [regional persistence](REGIONAL_PERSISTENCE.md).

Optional TLS is implemented with explicit CA trust, server-name verification
and protocol negotiation. Resume capabilities remain withheld until verification
completes; a TLS failure has no plaintext fallback. This secures the connection
when configured. Trusted host APIs now suspend/reactivate an exact saved
principal and rotate its resume capability, committing before disconnect or
policy replacement. A bounded audit tail records those actions without bearer
secrets. **Resume-only admission** closes new guest registration before hydration
or allocation. These are embedding APIs and a host startup policy; real-person
enrollment/recovery, operator authentication, moderation services and a production
identity system remain the game's responsibility. See [operations](OPERATIONS.md).

The reference server defaults to **16 clients**, a **20 Hz** authority tick and
**10 Hz** snapshots. Those are configuration defaults in
[server.rs](../crates/terra-net/src/server.rs), not measured capacity promises.
The sandbox retains its **512-body aggregate limit**, including cold regional
reservations. Transport has separate bounded ledgers; their exact limits are in
[TRANSPORT.md](TRANSPORT.md). No claim of MMORPG concurrency follows from large
geographic coordinates, a configurable actor budget or a regional database.

The client uses private completed-fixed-step input acknowledgements and bounded
owner display prediction/reconciliation, introduced with protocol **6**, alongside
remote pose smoothing. [Compatibility](COMPATIBILITY.md) lists the current wire contract.
The client replays a maximum 250 ms/32-frame history through shared dynamics and
local terrain/body admission. Missing coverage, stale authority, body/control
changes or oversized history clear or stop the preview. Only the owned display
pose and camera use it; cargo, fuel, reach, travel proof and economic completion
remain received authority. The [prediction guide](CLIENT_PREDICTION.md) explains
the integration and failure behavior. Internet latency/jitter acceptance,
rollback combat, distributed regional ownership and representative population
capacity remain open.

## Evidence from this increment

[VALIDATION.md](VALIDATION.md) records the completed release gates and exact counts.
Tests establish bounded correctness properties, not parity or production scale.
Historical 0.7 gates remain in [VALIDATION_v0.7.md](VALIDATION_v0.7.md); their
counts and protocol-5 behavior are not current measurements. The separately
versioned 0.8 reports below are historical; [VALIDATION.md](VALIDATION.md) gives
the exact current gates and new proof.

| Evidence | Behaviors covered |
| --- | --- |
| [Company domain tests](../crates/terra-sandbox/tests/companies.rs), [rental tests](../crates/terra-sandbox/tests/rentals.rs) and [transport tests](../crates/terra-sandbox/tests/transport.rs) | Separate wallets, wages and escrow; finite queue/stock reservation; physical delivery/return proof; loaded hold/handover; employee dismissal and household demand; depot transfers; rental expiry/renewal/refunds; rejection rollback, dormancy, ownership/privacy and durable state. |
| [Ground contacts and routing](CONTACTS_AND_ROUTING.md) and [traffic tests](../crates/terra-sandbox/tests/traffic.rs) | Sweeps, support/underpass clearance, rotation, overlap escape, vertical separation, right-side directed routes, access/speed rules, FIFO crossing progress, blocked exits, approach requeue, persistent occupancy and loaded courier progress. |
| [Real TCP driving tests](../crates/terra-net/tests/driving.rs) and [loopback tests](../crates/terra-net/tests/loopback.rs) | Authority, sequence replay rejection, owner-only company/vehicle information, public appearance, RTS/possession and reconnect. |
| [Regional persistence tests](../crates/terra-net/tests/regional_persistence.rs), [0.8 regional probe](regional-probe-v0.8.json), [TCP session report](regional-session-load-tcp-v0.8.json) and [TLS session report](regional-session-load-tls-v0.8.json) | Certified environmental scopes are compared with full merges; unchanged bytes are reused under guarded proofs. Real socket churn/restart, durable commands, offline ownership, funded cargo, induced storage failure and retry have finite recorded workloads. Exact counts and exclusions are in [SCALING.md](SCALING.md) and validation; these are not production capacity certification. |
| [TLS tests](../crates/terra-net/tests/tls_transport.rs), [operator tests](../crates/terra-net/tests/operator_controls.rs) and [prediction tests](../crates/terra-net/tests/prediction.rs) | Verified encrypted resume, protocol 10 / `terra-world/10` rejection boundaries, transactional exact-principal controls, resume-only admission, private completed-step acknowledgement and bounded replay/reconciliation. |
| [Input tests](../crates/terra-demo/src/player_input.rs), [wheel tests](../crates/terra-demo/src/wheel_input.rs), [accessibility](../crates/terra-demo/src/accessibility.rs), [session flow](../crates/terra-demo/src/flow.rs) and [vehicle presentation](../crates/terra-demo/src/vehicle_presentation.rs) | Rebinding, controller focus/text drafts, disabled controls/focus loss, text scaling, menu handoffs, virtual separate/combined wheel calibration, safe release/rearm and manual/autonomous appearance. |

Native menu, company, driving, minimum-size controls and checkpoint/resume
screens provide separate presentation evidence in the validation guide. No
physical gamepad, steering wheel, Windows/macOS build, target gaming GPU or
large concurrent population was certified by this release.

## Next priorities and acceptance gates

These are future release gates, not completed capabilities or promised dates.
Choose target hardware, player population, network conditions and content budget
before accepting performance or scale claims.

| Priority | Required increment | Acceptance evidence before claiming it complete |
| --- | --- | --- |
| 1. Driving feel and physical interaction | Extend continuous stop/yield body admission with tire/suspension/drivetrain behavior, rigid-body impulses, articulated trailers and controllable recovery equipment. | Shared local/server scenarios for slopes, bridge edges, tight turns, reversing a trailer, obstacles, impacts and low grip. No authority bypass, fuel/odometer divergence or tunneling within the declared speed envelope. Recorded handling baselines and playtesting on every supported input family. |
| 2. Sustainable commercial play | Build on completed hold/return/handover, dismissal, private depots, household spending and prepaid rentals with player employment, individual rider schedules, fleet specialization and demonstrated balance. Private depot markets/transfers and a finite six-session conservation fixture are now implemented. | Extend the conservation/lifecycle regressions into long multi-session play through scarcity, full sources/destinations, stranded work, lease expiry, staff changes, reconnect and restore. A new player can progress through careers without developer fund injections; real replenishment and finite spending remain explicit. |
| 3. Finished job experiences | Individual passenger boarding, loading interactions, physical tow work, medical/fire/police mechanics and richer progression. | Each advertised career has a discoverable start, required equipment, meaningful actions, failure/recovery, authority validation, rewards and durable completion. Specialist labels without those steps remain partial. |
| 4. Competitive driving | Event lifecycle, route authoring, competitor state, fair start/finish decisions and ranked results. | Host-adjudicated replay and checkpoint validation; duplicate, stale, disconnected and tampered submissions cannot produce valid ranked results. Rules, moderation and dispute recovery are defined before publishing a leaderboard. |
| 5. Native usability and accessibility | Build on controller menu/text flows, remapping, text/FOV preferences and calibrated wheel/joystick profiles with shifter support, broader assistive technology, localization and physical hardware validation. Controller management/RTS flows are now implemented; atlas drawing remains a separate input path. | First-time players complete creation, a delivery, servicing, saving and resuming using each claimed device. Test keyboard-only navigation, visible focus, contrast, text scale, ultrawide/small windows, motion comfort and recoverable error states. Virtual-device success alone is insufficient. |
| 6. Production art and audio | Extend the validated immutable GLB pack and original wildlife example with a larger vehicle library, detailed cabins, character/vehicle animation, materials, surface effects and sound. | Asset licensing/provenance, LOD and material budgets, stable streaming, controlled overdraw and measured GPU/frame-time results on named hardware. Review day, night, rain, dense streets and cockpit views; a high-detail still image is insufficient. |
| 7. Multiplayer latency and trust | Validate the bounded owner preview under declared network conditions; integrate real-person enrollment/recovery, authenticated administration, abuse policy and game-specific lag compensation. | Test latency/jitter/loss, reconnect during funded work, stale input, malicious bounds and visibility changes. Preserve authority-only movement/economy and completed-step acknowledgements. Exercise suspension/rotation failures and resume-only provisioning; trusted APIs and TLS alone do not supply an account service. |
| 8. Regional authority and operations | Extend conservative certified environmental groups to independently provable dependency classes; establish explicit regional ownership, safe cross-host handoff, migrations and operations. | Preserve full-decode verification and differential merge evidence. Demonstrate one writer per entity, no duplicate cargo/payment at handoff, fault-injected restart recovery and compatible migration. Measure clone/install/authority barriers as well as worker time; long travel must retain profiles, claims, employees and deliveries. |
| 9. Measured population scale | Representative bots/players, dense-interest traffic, persistent changes and long-duration load on fixed budgets. | Publish hardware, source coverage, population mix, duration, tick p50/p95/p99, bytes per client, storage latency, memory plateau, visible truncation and error rates. Measure overload and recovery; increasing caps alone fails this gate. |
| 10. Data and historical coverage | Licensed, versioned geographic/historical packs with explicit gaps and gameplay-ready collision/navigation. | Source attribution, coverage checks, deterministic identities, era consistency and tested boundaries. Provider data must not silently turn an unavailable road, building or historical feature into an asserted fact. |

Priorities 1–5 develop the transport game requested by the benchmark. Priorities
6–10 determine whether it can become a polished reusable world platform at a
declared production scale. Both tracks need measured acceptance; one cannot be
inferred from progress in the other.

### New 0.9 acceptance evidence

The warehouse lifecycle fixture uses six resumed sessions, 24 physical deliveries,
scarcity, fuel recovery, payroll funding, cancellation and exact conservation.
It reports a narrow operating balance and excludes startup/harvest cost; it is
not proof of a profitable full career. Shared support tests compare host/local
terrain and source grip, carried loads, missing wheel coverage and slope holding.
Native springs do not alter authority. Delayed/fragmented TCP and TLS fixtures
exercise queue pressure, private state convergence and resume. Explicit semantic
bundles are accepted only with matching source identity. Management context tests
cover actual actions, text, scroll reveal and blocked atlas/world inputs.
These close specific reference gaps while the production acceptance gates above
remain applicable.


### 0.10 acceptance additions

The [guidance](GUIDANCE.md), [asset-pack](ASSET_PACKS.md),
[autonomous-handling](VEHICLE_DYNAMICS.md), [prepared-source](SOURCE_BUNDLES.md)
and [operations](OPERATIONS.md) guides describe the new reusable contracts.
The current [product review](PRODUCT_REVIEW.md) records native findings and
remaining balance limits; [VALIDATION.md](VALIDATION.md) records the final
source/build evidence. These additions do not change the partial status of
finished specialist careers, physical trailer handling, production art/audio,
physical-device coverage or demonstrated MMO capacity.
