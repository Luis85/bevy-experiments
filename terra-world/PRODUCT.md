# Terra World

Terra is a reusable Rust/Bevy world SDK and a native playable reference game. It combines georeferenced streamed terrain, source features, persistent player changes, actor control and a bounded living settlement. Games can replace the renderer, providers and presentation while keeping the domain rules.

## Current iteration: a complete session and a connected driving economy

The 0.24 pass keeps useful funded work and reviewed operations coherent over
continued play. New supply offers fit actual demand and receiving capacity;
confirmed obsolete public offers return exact escrow to their original issuer
before retirement, while accepted promises retain their terms. Industry, Fleet
and building damage retain the reviewed target and operation. Protected output
identity works across supported platforms, canonical process evidence is bounded,
and CI qualifies the actual extracted desktop before upload.

See the [current gaps](docs/REVIEW_GAPS_v0.24.md),
[researched implementation plan](docs/IMPLEMENTATION_PLAN_v0.24.md) and
[Playtesting](docs/PLAYTESTING.md). Actual Windows/device/GPU qualification,
uncoached usability and public-service readiness retain distinct evidence.

The player can explore a larger landscape, select owned actors and issue RTS orders, then possess an actor in third or first person. Building, gathering, farming, trading and production use a shared player inventory and wallet. Individual bodies have position, needs and orders. Habitat populations, depleted/regrowing resource patches, residents, road traffic and couriers make the local settlement visibly active.

Activities should provide a clear next action: gather or process supplies, fulfill funded requests, establish a named neighborhood, build homes and farms, then qualify for village and city charters. Territorial membership must govern actual access. A failed operation must preserve goods and money and explain the unmet requirement.

The presentation foundation improves terrain detail, lighting and the path to game-owned
models while reducing repeated geographic work and network payload. Render
quality must be configurable. Possession should open the view without preventing
contextual interaction. A developer can inspect bounded stage timings and
replication counters without putting implementation statistics in the player's
main footer.

Scaling claims must name the workload. An index of 100,000 positions, a batch of
64 registered-player views, 16 connected TCP clients and 140 living bodies are
different measurements. Preserve gameplay and privacy invariants before raising
any cap. Durable regional storage now has a bounded single-authority implementation.
Distributed simulation remains a separate engineering project.

The first session should make five things immediately understandable: which control mode is active, which actor is selected, what it needs, which actions are available here, and whether the terrain is sourced or synthetic.

The first session begins in a native menu. Character creation, world source
and settings, review, loading, play and durable return form one continuous
journey. Road vehicles accept pedals and steering; transport jobs move real
goods/passengers and funded rewards through the existing economy. The Motor Town
comparison is a coverage benchmark, with explicit remaining physics, content,
services and hardware work. New presentation remains original Terra content.

Storage scheduling and worker transaction time are separate measurements.
Pending work must not look like a failed save; a committed acknowledgement must
not precede durability. Native and CLI multiplayer can use verified TLS, while
bounded owner display prediction is implemented; public accounts and
distributed authority remain separate systems.

## Authority and truth

Source terrain, map features and political geometry retain provenance. Authored population and economy are a scenario layer; their placement does not claim that real named people or businesses were observed. The bundled historical scenarios remain fictional. Imported heightmaps must state geographic bounds, elevation range and attribution. Multiplayer keeps server ownership of mutable state.

## Supported experience

Native desktop with mouse/keyboard, controller menu/management/RTS paths and calibrated wheel axes, 1440×900 reference with 1100×700 minimum. Linux/X11 is verified. This iteration is a playable vertical slice, not a scientific economy, traffic or vehicle dynamics model. All limitations and exact validation evidence belong in the release docs.

## Priorities

1. A continuous playable loop across possession, needs, nature, commerce, production and community progression.
2. Legible terrain, landmarks, actors and movement across a wider visible region.
3. Predictable controls, contextual actions and recoverable errors.
4. Reusable simulation contracts and bounded per-frame work.
5. Source provenance, durable ownership and verified persistence.

The 0.5 iteration adds opt-in regional checkpoint/resume. The user must know
which generation is durable, when a restore discards later changes, and how to
recover from a failed write. Dormant environmental records freeze; the retained
economy and owned bodies continue. Private host/session data must never appear
in player diagnostics. No UI should present a loaded subset as a whole-world
count, and no whole-world save may silently omit dormant records.

## 0.7: shared roads and a working fleet

Commercial transport must be a connected operating loop: register, fund, hire,
author a useful route, assign, observe, pause and release. Money, passengers,
cargo and payroll must balance across blocked roads, empty stocks, offline
owners and restored worlds. The Fleet panel explains the next action and names
the actual assigned vehicle and route independently of chooser selections.

Contacts must preserve population and delivery progress. Preventing overlaps
without resolving centerline conflicts is a regression. Physical admission,
reference lanes and bounded stalled-route recovery are reviewed together.
Offline owners leave active bodies and homes without draining needs or paying
employees; authenticated resume restores them before input is admitted.

Controls are user preferences, independent of characters and saves. The
keyboard editor and analog gamepad path share authoritative intent. The
reference adds readable road guidance, a north-up route map and rolling wheels;
it remains original geometric presentation, not a production asset library.

## 0.9: sources, productive depots and input continuity

An elevation bundle may carry explicit attributed GeoJSON. Its actual contents
participate in multiplayer and save identity. Source geometry must never be
invented from raster brightness or silently replaced under an older world.

Company depots are private inventory with reserved incoming capacity. Internal
transfers consume real payroll without public fares; shop delivery uses finite
buyer escrow and physical arrival. Reviews measure conserved goods and money
through depletion, recovery and resume, not an ever-growing synthetic treasury.

Four wheel support, source friction, carried load and bounded grip inform manual
authority and prediction. Independent visual wheel travel is presentation only.
The reference does not claim rigid-body suspension, physical trailers or hardware
certification. Slow readers defer optional replication without losing reliable
command outcomes or advancing an unseen baseline.

Management focus is an explicit input context. Native typing and controller
text entry must both work there. Exiting or opening a panel must not steer an
actor, finish an atlas polygon or replay a stale construction click.


## 0.10: continuous goals, shared motion and recoverable content

Player guidance is a read-only interpretation of the live world. It names one
useful next action and its actual cost or reward, uses current permissions and
never creates a second completion ledger. Accepting, travelling and delivering
remain separate actions. A stale suggestion cannot silently become a different
transaction. Missing regional data and bounded search scope stay explicit.

Autonomous vehicles feed bounded controls into the same handling model as owned
driving. Physical support, swept contacts, route restrictions, cargo and earned
travel remain authoritative. Safe new-principal placement checks the complete
crew and dormant occupancy; resume preserves saved positions.

Presentation packs are immutable, validated content. An unsuccessful model load
must leave a visible actor. Animation follows accepted motion and true pause,
and per-binding choices remain independent even when models are shared. The
bundled pack demonstrates five original wildlife species while preserving
character customization. It is an authoring contract, not a complete art library.

Prepared terrain collections prove their seams and connected union coverage.
They do not fill missing ground or silently migrate old world sources. Operators
can inspect, back up and recover complete regional state to a new destination;
large-world evidence includes dormant data and failure cases, with explicit caps.

## 0.11: reliable handoffs and reproducible acceptance

Guidance must preserve the concrete destination or seller it recommends. Urgent
needs should use available alternatives, including gathered food and finished
goods when crafting equipment is missing. Choosing a different workflow remains
an explicit player action; existing ownership, inventory and needs rules apply.

A financial action retains the total and selection actually displayed. Personal
market, depot and roadside service actions fail if the relevant quote changes
before acceptance. Host authority still checks stock, capacity, reach and both
accounts, with no mutation on rejection. A quote is an expectation, not a price
reservation or a second source of economy state.

Vehicle exits require safe body placement and a supported approach corridor.
The native and network hosts supply their actual terrain/source resolver; SDK
callers using the flat domain path retain its explicitly documented assumption.
Source absence cannot justify a teleport or a partially applied exit.

Content validation counts the rendered instances of mesh geometry, checks
texture decoding and rejects unsupported extensions. Runtime animation follows
elapsed virtual time, including slow frames. These are admission and correctness
bounds. Validated packs also have aggregate scene and retained-payload admission;
those estimates do not establish physical GPU memory use or final art quality.

Every release should retain fresh automated evidence and an honest gap register.
The checked-in verification runner freezes source inputs and records commands,
logs and exact test identities. Native journeys and larger performance claims
remain separately evidenced. See [the 0.12 gap register](docs/REVIEW_GAPS_v0.12.md).


## 0.12 continuation and production boundaries

Company decisions need reconciled costs as well as revenue. Finances separates
owner funding from earnings and identifies imported history and unpriced inputs.
Recorded acquisition payments remain exact. A finite company cash result does
not establish accrual profit or a complete cost basis for unpriced inputs/assets.
Ordinary walking may simplify a route only through supported, source-clear and
body-clear ground. Autonomous recovery must preserve original cargo, physical
arrival and permits; unresolved obstructions remain explicit holds.

Validated presentation packs use aggregate admission and retain procedural
visibility during deferral or failure. Prepared source distribution preserves
content identity, provenance and exact coverage in a new portable directory.
Unknown terrain remains unavailable even when a saved overlay page exists there.
Live recovery acceptance must join real sessions, private identity/sequence
continuity and dormant regional records, while labeling the terrain and hardware
actually exercised. Deployment scale and other physical devices require their
own evidence.

## 0.13: managed careers and durable communities

Local walking can propose bounded corners around a stopped body, but every
replacement leg must use supported, unoccupied ground. The actual movement
resolver retains authority. Reproducing a geometry case on flat support and
observing it on prepared terrain are separate acceptance steps.

Community membership must have an explicit lifecycle. The current leader may
hand over to an existing member, members may leave, and leaders may remove or
revoke invitations. Departure clears shared home access even while a member is
offline, while faction property and public funds remain communal. Leaders must
hand over before departure. Inactive-leader policy is deliberately unimplemented.

Route guidance explains a current first-departure receipt, supply/demand and
cost limits without promising a profit, travel duration or reserved resources.
Career evidence includes rental/refund costs, bounded scarcity and any manual
recovery, and does not turn managed operation into an unattended-autonomy claim.
Destructive confirmations bind the world/owner/target actually reviewed.

The source project includes its original gaps audit and a current status report.
Compatibility statements are checked against code. Desktop packaging retains
the accepted binary, curated content and its evidence; an unexecuted platform
workflow is not native platform qualification.

## 0.14: continuing services and complete local handoff

A returning owner should be told what the current company needs, which load is
committed, what continues to cost money, and where the relevant existing control
lives. Guidance derives from actual state and private ownership; it cannot
auto-resume, rewrite obligations or invent new cash/quest progress.

Artists need a repeatable export-to-validated-pack path with immutable accepted
bytes and actionable rejection messages. Operators need a recovery set that
retains the exact saved world, catalog, configuration and supported source
dependencies, then proves restore in a new location. Diagnostic history must
survive abrupt termination without labeling missing or stale evidence healthy.

## 0.15: reachable recovery and representative vehicle content

Supported menu worlds need a player-facing backup and recovery path. Backups
retain the character and presentation choices needed to continue, verify before
confirmation, and restore into a new managed world. A completed output must
remain reachable through its exact named action even when bounded recent lists
are full or the system clock moved backwards. Neither recovery nor browsing
may replace the only original or make an unreviewed world start automatically.

Autonomous vehicles may propose a bounded route around a small group of stopped
bodies. Every segment, including reconnection to the original goal, must be
supported and unoccupied. The shared handling model and movement resolver keep
authority; a detour cannot teleport a vehicle, change another body, erase a
load or bypass missing terrain. A measured fixture establishes that geometry's
progress, while dense roads, moving obstacles and unattended work still need
their own evidence.

The original Comet example extends the artist handoff to one vehicle model with
exterior, an empty cabin and wheels driven by actual admitted motion. Existing
wildlife and character customization remain intact. A fixed imported livery
must be disclosed before any paint charge. This example narrows production
content work; it does not establish a complete animated human, boarding, audio
or commercially licensed content library. The [0.15 gap register](docs/REVIEW_GAPS_v0.15.md)
retains explicit acceptance criteria for those areas.

## 0.16: recognizable commitments, people and recovery copies

Activities should let a player choose a goal directly and return to real accepted
work. The commitments briefing belongs to the current owner and names its view
scope. Deadlines use the world clock; gross rewards and complete missing-input
prices do not imply profit. A queued selection, expired/reassigned request or
restored world requires the player to review the new context before acting.

Constrained navigation must retain bounded physical progress or a clear hold.
Consumed waypoints cannot exhaust future recovery capacity after support returns.
New narrow/cluster proposals validate their whole planned corridor; an existing
incremental one-body maneuver validates its bounded reconnecting source envelope
and checks then-current traffic and bodies during every physical step. No detour
may erase a load, invent terrain or move a parked obstacle.

Representative human content must preserve saved customization per instance and
fit the actual vehicle in its seated pose. Named material roles, bounded clip and
allocation work, visible fallback, pause and camera transitions belong to the
asset contract. A known manually controlling driver is a presentation handoff
from existing authority; it is not a new passenger or physical boarding system.

Players may name a recovered copy before creation or rename an existing managed
world. The operation retains progress, sources, character and dates, rechecks
the complete selected profile, and keeps the result reachable. Names describe
separate world identities; they must never become keys that overwrite a copy.
The [current gap register](docs/REVIEW_GAPS_v0.16.md) retains broader closure
criteria and the evidence required for each product area.

## 0.19: integrity across SDK, presentation and delivery boundaries

A suspended owner's direct SDK settlement and controlled-motion intent must
reject before any mutation. Trusted host placement remains a separate API for
validated movement, provisioning and restore; it does not resume a session.

Regional storage admits the supported schema contract as well as valid payloads.
Keys and hidden automatic indexes must not change the meaning of an acknowledged
checkpoint, and duplicate regional identities cannot overwrite earlier state.
Valid existing schema-1 stores retain their backup/reopen/restore path.

An asynchronous terrain mesh belongs to its immutable source instance. A matching
tile, LOD and edit revision cannot justify displaying an evicted source over
replacement authoritative ground. Rejected results use existing bounded queues.

Long menu errors and paths must stay readable inside bounded content without
replacing footer action identity. Protocol diagnostics redact typed resume
capabilities while the private wire intentionally retains them. Complete release
receipts require explicit gate execution and consistent identities/counters;
byte consistency alone does not establish publisher trust or device readiness.

## 0.20: deliberate restore and bounded failure recovery

Native restore consent names a retained target, defaults to Cancel and binds a
single review identity. Confirmation cannot be reused for another review; queued
work rechecks context and target before installation. Trusted SDK Load remains
explicit. Dirty presentation work survives worker failure with bounded retries
and visible diagnostics. Storage hydration validates committed inputs even when
live state is already materialized, without replacing valid uncommitted edits.
Expired offers reject before mutation at the deadline. Network admission uses
shared checks and source expectations; normalized snapshots cannot bypass wire
replication. A release succeeds only after direct terminal receipt publication
and readback; packaging requires its matching completion record.
