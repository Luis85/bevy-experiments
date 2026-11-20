# Using Terra as an MMORPG world foundation

Terra World 0.12 is a reusable Rust/Bevy world SDK and a playable reference game.
It connects streamed geographic terrain and source semantics to persistent
construction, needs, trade, production, logistics, wildlife and community claims.
The 0.10 increment connects state-derived player guidance, immutable validated
animated model packs, shared autonomous handling, connected prepared terrain
and verified backup/recovery across a larger dormant world. Existing finite
warehouse logistics, directed roads, private replication, regional durability
and configurable input remain part of the same authority and data boundaries.
The 0.12 review adds reconciled company accounting, supported walking shortcuts,
exact recovery of retained traffic fixtures, aggregate imported-scene admission,
portable prepared-source tooling and a combined real-TLS/cold-world recovery
workload. It retains the reviewed quote, restore and placement boundaries from
0.11. See the [gap register](REVIEW_GAPS_v0.12.md) for implementation, executed
acceptance and the external qualifications that finite local work cannot establish.

Use [GAMEPLAY.md](GAMEPLAY.md) to play the reference loop,
[INTEGRATION.md](INTEGRATION.md) for the existing plugin/provider contracts, and
[LIVING_WORLD.md](LIVING_WORLD.md) for detailed living-world gameplay behavior.
This guide maps what can be reused, what a host must provide, and which current
limits must be redesigned before scaling. [SCALING.md](SCALING.md),
[NETWORK_REPLICATION.md](NETWORK_REPLICATION.md) and
[PERFORMANCE.md](PERFORMANCE.md) describe the measured workloads and diagnostics.

## What the foundation already connects

The default native reference requests a 49-chunk geographic neighborhood. The
optional [large-region preset](../configs/large-region.json) requests 121 chunks;
it does not change the default or increase the living population limits. The
reference can switch between an RTS camera and first/third-person control,
gather real persistent
resource stock, craft, trade, construct housing and crops, fulfill funded work,
and found a personal or faction community. Courier goods move through physical
bodies. Animals use persistent habitat metadata and shared movement checks.
Local and multiplayer commands use the same domain transactions.

Keep the three kinds of world truth explicit:

| Layer | Examples | Authority and update policy |
|---|---|---|
| Immutable source world | Height samples, map features, provenance, dated packs, political source claims | Licensed provider/prepared dataset; immutable revision and declared epoch |
| Authoritative gameplay state | Bodies, needs, resources, inventories, money, cargo, plots, structures, factions, territories, contracts | Local trusted host or server; validated commands and persistence |
| Presentation and input | Camera, selected unit, mesh LOD, grass detail, animal pose, panel state | Client/game view; cannot grant ownership or manufacture stock |

A real forest polygon can suggest a habitat, but it does not establish a deer
count. A mapped shop can suggest a service location, but it does not establish
opening hours, workers, inventory or credit balances. A historical pack can
provide a starting landscape; simulating forward does not reconstruct observed
history. Imported country borders do not confer gameplay property rights.

## Choose the integration surface

The crates are source dependencies in this workspace, not published crates.io
packages. Rust 1.99.1 and Bevy 0.20.0 are pinned by this release. Match the Bevy
version when using its adapter, or use the independent domain crates from a
separate application.

| Requirement | Existing seam | Your game's responsibility |
|---|---|---|
| Global positions and tiles | `terra_core::GeoPosition`, `LocalFrame`, `TileKey` | Coordinate/height-datum policy and geographic content placement |
| Source loading | `terra_stream::ChunkProvider`, `ChunkStreamer`; `TerraWorldPlugin::with_provider` | Licensed ingestion, immutable revisions, prepared tiles, service quotas and distribution |
| Historical/political layers | `HistoricalPack`, `HistoricalCatalog`, `PoliticsDataset` | Evidence-backed dates/coverage, uncertainty and scenario policies |
| World scheduling | `TerraWorldPlugin`, `WorldSet`, `WorldRuntime`, `ResidentChunks` | Camera/focus, host timestep and scheduling of custom systems |
| Body control and simulation | `TerraLivingPlugin`, `LivingSettings`, `LivingMotion`, `AutonomousVehicleEnvironment`; shared bounded driving | More complete AI, navigation, physical interactions and game-specific movement |
| Spatial queries and wildlife work | `GeoSpatialIndex`, `SandboxInterestIndex`, `EcologyScheduler`; Bevy `EcologyRuntime` | Trusted observer selection, index lifetime, decision budgets and durable regional ownership |
| Gameplay commands | `Command`, `LivingCommand`, `SettlementCommand`, `TransportCommand`, `CompanyCommand`; `SandboxQueue` | UI/controller intent, game policies and any additional environmental checks |
| Commercial simulation | Transport/company commands and advancement; rental offers/return quotes; private recipient views | Fleet content, player employment, individual passenger schedules, product balance and additional conservation tests |
| Road coordination | `RoadRules`, `RoadNetwork`, `RoadPlanKind`, `LivingWorld::vehicle_route_plan`, physical traffic reservations | Complete source junction/turn relations, legal lane policy, signals, congestion management and any manual enforcement |
| Items and crafting | `Catalog::from_json`, `add_player_with_inventory` | Balanced content, starter policy, new recipe/blueprint IDs and migrations |
| Wildlife/resources | `EcologyConfig`, `register_ecology_chunk`, environment-aware ecology seeding/advancement | Species/yield policy, habitat lifecycle and any scientific or archival data model |
| Territories and work | `SettlementWorld`, `settlement_readiness`, `advance_settlements`, derived `activity_guidance` | Faction lifecycle, broader governance, taxes/conquest or custom progression rules |
| Scene presentation | Optional `TerraRenderPlugin`, `GeoAnchor`, `RenderQualitySettings`, `ActorSceneOverrides`, validated immutable `AssetPackPlugin` and motion-driven clips | Broader licensed art/audio, animation authoring/retargeting, UI and target-GPU qualification |
| Multiplayer | `terra-net` host/session/environment boundaries, protocol-10 deltas/resync, verified optional TLS, private motion snapshots and `ClientPrediction` | Real-person accounts/recovery, certificate operations, representative network-condition acceptance, game-specific lag compensation and scaling |
| Host administration | Trusted principal provisioning, exact-principal status, suspension/reactivation and capability rotation; bounded audit; resume-only admission | Authenticated/authorized operator surface, private capability/recovery delivery, external audit/retention and abuse policy |
| Native input/accessibility | Remappable actions, controller menu/text flow, text/FOV/motion preferences and validated Bevy/Gilrs wheel profiles | Game-specific atlas input, localization, broader assistive technology, physical hardware certification and force feedback |
| Runtime evidence | `WorldDiagnostics`, `RenderTelemetry`, scheduler/replication counters | Representative workloads, hardware budgets and production observability |
| Persistence | `RegionStore`, `RegionWorker`, immutable validated partitions, certified environmental groups, guarded page-byte reuse, no-overwrite operations and complete JSON envelopes | Broader independently provable dependency classes, game-specific migrations, account/backup policy and regional authority handoff |

Core, streaming, history and sandbox rules do not require Bevy. Disable default
features on `terra-bevy` to use it headlessly. An integrating game can retain
geographic loading and domain state while replacing every mesh and camera.

### Start from a small host

The existing examples are finite entry points without a window:

```sh
cargo run -p terra-bevy --no-default-features --example headless
cargo run -p terra-bevy --no-default-features --example custom_provider
```

Install `TerraWorldPlugin` for streaming/environment state, and add
`TerraLivingPlugin` if the shared living/ecology/settlement adapter is useful.
Insert `LivingSettings` first. `seed_demo: false` prevents the authored reference
population from being created; the game then supplies validated state and its
own initialization policy. `simulate: false` lets a host replace local
advancement. Disabling collision checks requires a deliberate replacement
physical policy.

The adapter already schedules ecology decisions before shared living movement and
performs settlement maintenance after production. Do not call those domain
advancement methods a second time from another plugin. Network clients install
validated replicas rather than advancing an authoritative local economy.

`EcologyRuntime.scheduler` defaults to a 160 m near-observer radius, two physical
seconds between distant reconsiderations and at most sixteen far decisions per
normal behavior pass. Near or threatened wildlife retains priority; the default
behavior pass is one second, not a 20 Hz AI promise. Local observers are owned
body positions; the server uses owned bodies of connected principals. A free
camera cannot manufacture near-tier work. Motion, collision resolution, needs,
resource recovery, production and deliveries still advance on their ordinary
fixed steps. The scheduler is not an offscreen physics shortcut.

Reset the scheduler when replacing or loading a world, including a same-clock
load. The Bevy load path and server restore already do this; custom hosts must
also reset their runtime resource. A floating render-origin rebase must preserve
the scheduler: geographic positions and fairness remain valid. Runtime indexes,
decision cursors and timing windows are separate from saved authoritative state.

`TerraWorldPlugin` installs the app-wide Bevy fixed timestep from `fixed_hz`.
Coordinate it with your physics schedule. Local render axes are **east, up,
south**, whereas actor velocity arrays are **east, north, up**. Persistent and
networked positions stay geographic `f64`; local Bevy transforms are disposable
`f32` render representations.

### Preserve connected commercial obligations

Company cash, employee wallets, departed-household money, issuer treasury and
rental bonds are separate accounts. Payroll and household-spending totals are
historical receipts, not additional spendable money. A game adding ledgers or
reports must preserve that distinction. Freight leaves real producer/shop stock;
passengers come from finite queues; a delivery pays the reserved reward once.

The reference now supports immediate loaded holds, physical return to the original
pickup and nearby stopped handover to a relief crew and qualified empty vehicle.
Handover preserves one cargo/passenger/escrow commitment and the original timing
and admitted odometer proof. Full return destinations retain the commitment.
Rental expiration disables driving but retains the body, load and bond; renewal
charges real funds, and physical return settles damage and fuel under current
account/capacity checks. Dismissal settles earned wages before removing the
employee. Dormancy freezes this work instead of granting elapsed-wall-time income.

Private depots store real inventory and affect nearby service cost. Nearby shop
trades check the displayed quote against actual goods, company/shop money and
available depot space before committing. Private warehouse sale routes reserve
the buyer's payment at loading and earn it only after physical delivery; internal
depot transfers earn no public fare. Loaded work retains its return and arrival
space reservations, recovery choices and finite commitments.

A depot also enables bounded wage-funded household purchases: actual goods leave
shop stock, their prices enter the shop till, and a transit payment enters the
issuer treasury. Existing production replenishes through its ordinary recipes
and resources. Explicit company contributions can fund public services. None of
these paths provides an unlimited stock or money grant. A depot creates no
automatic profit: acquisition prices, buyer demand, fuel and payroll constrain
returns. These reference loops do not establish a balanced macroeconomy or
individual passenger schedules.

Use the same validated domain commands for local UI and remote clients. Preserve
the movement → physical arrival/settlement ordering and never advance commercial
work twice through another plugin. Private transport views contain only their
owner's ledgers; public vehicle appearance remains separate. See
[COMPANIES.md](COMPANIES.md) and [TRANSPORT.md](TRANSPORT.md) before replacing
pricing, payroll, production or the return policy.

### Customize content and presentation

Catalog JSON controls items, recipe quantities, blueprint costs/footprints and
crop timing. `EcologyConfig` is serializable and validated; it controls population
budgets, species activity/behavior parameters and natural-resource definitions.
Saved populations and definitions are checked together. Changing carrying
capacities or other saved structural policy is a migration, not a reason to
silently reset the world.

The reference territory tiers and supply economy are Rust domain rules. They are
clear extension points, but they are not advertised as a complete JSON-defined
quest framework, city-planning language or general production-node editor.

With the render feature, `terra_bevy::render::RenderQualitySettings` selects
`Performance`, `Balanced` (default) or `High`, plus independent atmosphere and
terrain-LOD switches. Profiles bound shadow, antialiasing, sky and ground-cover
costs. Distant terrain meshes reduce interior vertices while retaining every
source border sample; collision and terrain queries keep the full heightfield.
The optional 121-chunk preset broadens the source neighborhood, while these
profiles control its presentation. They make no frame-rate guarantee.

`terra_bevy::render::WorldDetailSettings` controls nearby grass/flower clusters,
resource visuals and settlement boundary cues.
The requested default is 640 ground-cover clusters within 96 m, with hard caps
of 2,048 and 180 m. Quality profiles further cap that request; Performance uses
at most 320 clusters within 64 m. Ground cover is one batched mesh and does not
add a collidable object per blade. `wildlife_mesh` and `natural_resource_mesh`
expose the procedural reference geometry.

`ActorSceneOverrides` installs game-owned `Handle<WorldAsset>` scenes by actor
kind or wildlife species; species-specific appearances take precedence.
`ActorSceneAppearance` defines scale, ground offset and forward-axis correction.
The body's root retains identity, geographic transform and possession/layer
visibility. A missing, loading, failed or meshless scene keeps the procedural
fallback; a ready scene replaces it, and removing the override restores it.
For a validated content workflow, `AssetPackPlugin` installs immutable GLB bytes
and per-binding clip policies over this hook. `terra-assets validate` enforces
geometry, hierarchy, texture, skin and animation budgets before runtime loading.
The default five-species wildlife pack has reproducible original models and
clips. See [ASSET_PACKS.md](ASSET_PACKS.md). Broader model licensing, equipment
sockets, retargeting and authored animation state graphs remain game work.
See [GRAPHICS_AND_MAPS.md](GRAPHICS_AND_MAPS.md) for exact profiles and the model
installation example.

The renderer is optional. Hiding a presentation layer does not remove a body,
release a claim, replenish a deposit or stop the host's economy. Keep gameplay
permissions in the authority layer even when the visual presentation changes.

## Current budgets are constraints, not scale claims

These bounds come from the current implementation. They are distinct from
measured operating capacity on a production server or target GPU.
The principal definitions are in the
[sandbox constants](../crates/terra-sandbox/src/lib.rs),
[living model](../crates/terra-sandbox/src/living/model.rs),
[ecology model](../crates/terra-sandbox/src/ecology/model.rs),
[settlement model](../crates/terra-sandbox/src/settlement/model.rs),
[view budgeting](../crates/terra-sandbox/src/state.rs) and
[renderer detail settings](../crates/terra-bevy/src/render_detail.rs).

| Area | Reference bound or default | Consequence for a larger game |
|---|---|---|
| Visible regional source terrain | Default 7 × 7 chunks; optional 11 × 11 preset; 65 × 65 samples each at one configured source level | Render mesh LOD is present; offline prepared-source distribution is implemented; globe-wide adaptive source levels/clipmaps, licensed global ingestion and network/CDN delivery remain future work |
| Source mesh work | At most two build jobs; bounded main-thread integration; under-two-million source-vertex regression for the default scene | Profile full scene, materials, shadows and your hardware; the counter excludes separate actors/overlay meshes |
| Nearby world detail | 640 requested default clusters; 2,048 maximum; 128 rendered resource nodes; profile-specific caps | Tune existing quality profiles on target hardware before changing instancing/culling or budgets |
| Living bodies | 512 total, including player-owned units and wildlife | Five bodies per player plus NPCs consume the same budget; this is not a 512-player claim |
| Production sites and roads | 32 living sites and 32 road routes; compiled graph bounded to 8,192 vertices/segments and 256 junctions | Directed/access rules and physical FIFO permits exist; larger routing graphs, complete turn/lane rules and offscreen simulation still need policy |
| Ecology | 32 habitats, 64 animals, 192 resource metadata entries, 256 seeded region keys | Active and dormant totals share these caps; paging reduces the active set without raising population admission |
| Community claims | 32 territories; four founded per player; 100/300/700 m tiers | Add region ownership and spatial queries before lifting global limits |
| Funded work | 16 contracts, three accepted per player, at most four newly posted per maintenance pass | A full mission/quest scheduler and large player market need a different workload model |
| Public works | 4,000 stored material units per territory | Define storage, logistics and withdrawal policies before generalizing shared warehouses |
| Registered principals/factions | At most 128 players, 64 factions, eight memberships/invitations per player | Storage/admission ceilings are not concurrent-player performance evidence |
| Base sandbox collections | 10,000 entries per bounded entity collection | Indexes, paging and certified unchanged environmental groups reduce repeated work; larger aggregate populations need broader dependency proofs and admission design |
| Client living view | Up to 64 bodies, eight shops, 16 sites, 16 deliveries and eight road routes before byte-budget reduction | Partial views must be explicit; do not infer global counts from one client |
| Network payload | 240 KiB reconstructed sandbox view budget; 256 KiB frame ceiling | Protocol 10 uses bounded interest, deltas, resync, optional verified TLS and private owner motion acknowledgements; distributed fanout and real-person accounts remain separate work |
| Update rates and owner preview | Up to 5 Hz living sandbox views and 10 Hz principal actor stream; owner display replay limited to 250 ms and 32 frames | Validate network conditions and game-specific lag compensation; a bounded display preview is not combat rollback or remote-body prediction |
| Source-surface server cache | 512 chunks | Route decisions and physical admission depend on resident coverage |
| Persistence | 16 MiB pure JSON; 48 MiB desktop JSON; 32 MiB server JSON; regional store 4,096 pages / 64 MiB payload | Ordinary merges certify only unchanged closed environmental groups and reuse equal page bytes; current ledgers/dependent records remain validated. Authority still pauses; clone/install and maintenance barriers remain. Larger services need broader proven transaction classes and migrations |

Unknown terrain rejects relevant movement and placement; it is not treated as
empty safe space. Resource/habitat registration stops when its budget is reached
instead of inventing a limitless world. Those behaviors are intentional for a
reviewable reference. Regional paging preserves depletion and reserves cold
capacity, with an explicit frozen-dormant policy; larger aggregate worlds need
a redesigned durable admission and offscreen simulation model.

### Historical fixtures and current measurement scope

The earlier [simulation probe](SCALING.md) compared byte-identical snapshots for 512
living bodies, 10,000 resources and 64 registered recipient views. On the recorded
development-profile run, scanning took 37,145 µs; indexed construction plus the
same snapshots took 11,448 µs. Those are batch views without network connections.
Its separate 100,000-point experiment measures index representation/query work,
not 100,000 fully simulated NPCs.

The earlier [replication probe](NETWORK_REPLICATION.md) exercised sixteen actual loopback
clients with 4,096 generic kinematic actor records and a separate 140-body living
domain. It recorded a 62.86% reduction in encoded replication bytes against
equivalent full views across the finite run. Generic records and living bodies
are different representations; the test does not establish thousands of full
NPCs or production concurrency. Both guides link the complete JSON reports and
state the workload, build and timing exclusions.

These earlier measurements remain historical fixture evidence; they are not a
rerun of every current subsystem. The [0.8 regional
report](regional-probe-v0.8.json) compares certified and full merge results and
records checkpoint stages and page reuse. The [TCP session
report](regional-session-load-tcp-v0.8.json) and [TLS session
report](regional-session-load-tls-v0.8.json) exercise actual finite socket sessions,
durable commands, overlapping/spread interests, ownership dormancy, churn,
restart, induced storage failure and retry. [SCALING.md](SCALING.md) describes
their precise workload and exclusions; [VALIDATION.md](VALIDATION.md) records
the final release gates and exact results. These accelerated fixtures do not
establish Internet behavior, long-duration memory plateaus, gaming-GPU frame
rates, distributed authority or a production concurrent-player target.

`WorldDiagnostics` exposes bounded local frame intervals and CPU-stage timings;
`EcologyScheduler::stats()` reports actual evaluated/deferred work, and the
network client exposes payload and socket counters separately. See
[PERFORMANCE.md](PERFORMANCE.md) before interpreting these as performance
evidence. Client timing is not server timing, and frame intervals are not GPU
timestamps.

## Concrete nonproduction boundaries

### World data and fidelity

Public adapters load configured elevation, normalized OpenStreetMap semantics
and current weather. They do not ingest every open dataset, all OSM relations,
interiors, satellite material imagery or observed populations. Heightmaps supply
elevation; the included PNG is synthetic. Provider heights retain their vertical
datum unless a host converts it. Historical examples are fictional fixtures,
with explicit dates and coverage.

Use a licensed offline ingestion pipeline and immutable packs for a shipped
world. Pin revisions, preserve attribution, define source conflicts and handle
missing coverage. Public endpoints are development integrations with their own
limits, not the asset CDN or availability guarantee for an online game.

### Graphics, movement and world activity

The native scene uses authored procedural assets, mipmapped material detail,
terrain LOD, atmospheric or compatibility lighting, vegetation detail, finite
original animated wildlife, procedural fallback and geographic anchors.
It is a playable visual reference, not a photorealistic production renderer.
Production art, animation rigs, audio, interiors, streaming texture systems,
occlusion, broader accessibility and platform-specific quality tuning remain
substantial content/engineering work.

Controlled and autonomous road vehicles use a shared surface bicycle model with acceleration,
braking, steering, grip, slope and gearbox state. Candidate movement is admitted
against resident terrain and a model-sized support envelope. This remains a
reference controller with sampled four-wheel support, surface-specific grip,
actual carried-load effects and independent visual wheel springs. It still lacks
physical suspension/contact-patch tire physics, articulated
trailer dynamics or crash rigid bodies. Other body kinds retain kinematic
movement; buoyancy, rotor aerodynamics and flight simulation remain game-owned.
Road routing now uses supported one-way, access, numeric speed, layer and
bridge/tunnel metadata. At-grade graph crossings require compatible source
structure and vertical separation; a source layer is not an invented elevation.
Known access/direction restrictions hold automatic routes. No-road and
disconnected-graph direct fallbacks remain explicit. Physical-time FIFO junction
permits check exits and requeue stalled approaches, while actual inside-box
occupancy never expires. Manual bodies also obstruct automatic entry, but the
reference does not force a player to obey a traffic signal or road speed limit.
Source-derived automatic speeds, direction arrows and reference stop bars make
the present policy inspectable. Full lane/turn relations, traffic lights,
enforcement and a general congestion/deadlock solver remain open.

Shared manual/host/predicted vehicle support checks include model height under
source `min_height` and bounded thin-wall envelope sampling. These are geometric
admission checks, not suspension or rigid-body response. Routing and courier
retries cannot promise navigation through arbitrary imported buildings/bridges;
an impossible physical path remains blocked. See
[CONTACTS_AND_ROUTING.md](CONTACTS_AND_ROUTING.md). Wildlife
behavior is illustrative. There is no animal combat, hunting, taming, ecological
food web or long-term population science.

### Input and accessibility

The launcher, character/world creation and session menus support gamepad focus
navigation and a bounded text-draft keyboard. Saved preferences include remapped
actions, sensitivity/deadzones, text scaling, first-person field of view and
driving camera auto-center. Disabled controls and focus-loss paths have explicit
handling; a text draft does not overwrite a concurrent external edit silently.

The opt-in wheel/joystick profile selects a Bevy/Gilrs device and reported axes,
calibrates asymmetric/inverted steering and separate or combined pedals, and
requires neutral after focus loss, disconnection or invalid input. Runtime raw
values and unfinished calibration do not enter persistence. Virtual-device tests
exercise those paths, not actual hardware compatibility. F6/right-stick management
focus, private text drafts and a bounded terrain cursor now cover management,
RTS orders and construction. Atlas polygon editing retains mouse/keyboard. Raw
HID, shifter mapping, force
feedback, physical device/OS certification, localization and broader assistive
technology remain game/platform work. See [CONTROLS.md](CONTROLS.md).

### Persistent gameplay and economy

Player construction is an overlay. Permitted tree-point edits use tombstones;
whole sourced buildings/roads/water and arbitrary terrain are not generally
editable. There are no voxel excavations, structural fracture, hostile raiding
or automatic city generation.

Prices respond to stock, contracts hold funded rewards, productive residents can
receive funded pay, and company/depot households consume real stock using earned
wages. Private depot transfers reserve capacity, move physical stock and pay
earned wages without public fares. Depot-to-shop delivery holds finite buyer
escrow until arrival; nearby warehouse retail uses current validated quotes.
A six-session conservation fixture covers depletion and recovery, not calibrated
long-term career profitability. Rental/service fees and explicit public contributions transfer existing
money into issuer funding; they do not mint new money. These mechanics conserve actual items
and credit transfers, but they do not constitute a macroeconomic model, unlimited
market or proven long-duration commercial balance.
Faction territories enforce present roles. Members can leave, leaders can remove
members, and leadership can be handed to an existing member. Automatic
inactive-leader succession, dissolution, conquest, diplomacy, elections, taxes
and general treasury withdrawal remain open.

### Network and operations

The reference uses single-process framed TCP with optional verified TLS and saved
resume capabilities. Protocol 10 sends recipient-specific keyframes and revision
deltas, validates reconstruction against the same count/privacy/byte bounds and
requests fresh baselines on revision gaps. Queue headroom defers optional
replication without advancing an unsent baseline; reliable response admission
is bounded separately. Actual delayed/fragmented TCP/TLS fixtures exercise
convergence, reconnect and privacy, without certifying Internet service.
Immutable geographic indexes are
shared across each broadcast. These changes reduce repeated queries and measured
payloads within the bounded host. TLS verifies the CA and server name and does
not fall back to plaintext; non-loopback plaintext CLI joins require an explicit
opt-in.

Private `MotionSnapshot` messages acknowledge only input included in a completed
server fixed step. `ClientPrediction` reconciles the locally controlled display
body through shared dynamics and a bounded replay history, subject to loaded
terrain/building/body admission. Invalid snapshots, stale authority, missing
coverage, body/control transfer and excessive history stop or clear prediction.
Only the owned mesh/camera preview changes; gameplay positions, money, fuel,
cargo, interaction reach and travel proof remain received authority. This is not
remote-player prediction, whole-world rollback, combat lag compensation or a
guarantee under Internet latency. See [CLIENT_PREDICTION.md](CLIENT_PREDICTION.md).

Trusted host methods can inspect an exact principal, suspend/reactivate access
and rotate a private resume capability. With regional storage, validation and
durable commit precede live policy replacement and peer disconnection; failure
preserves the previous capability and world. A bounded audit tail omits bearer
secrets. `--resume-only` rejects new guests before hydration or allocation, but
is a startup policy rather than saved-world account enrollment. The embedding
application must authenticate its operators, provision principals and verify
real-person recovery through a private channel. These methods expose no public
administration endpoint and do not supply a moderation/account service. See
[OPERATIONS.md](OPERATIONS.md).

Transactional regional SQLite checkpoints, complete JSON exports, verified
archive/JSON conversion and backups cover one local authority. Immutable,
catalog-pinned proofs certify only wholly cold dependency groups consisting of
unchanged resource/source/tombstone records without natural-resource ecology
metadata. Current global ledgers and actual cold bodies, cargo, homes, farms and
ecology remain in the merged validation scope. Exact equal page bytes avoid
repeat encoding/SQL binding under generation/schema/data-version guards and an
in-transaction write-epoch check. Untrusted decode still reconstructs and fully
validates canonical state; a forged projection cannot stand in for actual cargo.

Ordinary storage jobs have bounded submission and polling, but authority waits
while a job owns the transition. Main-thread cloning/installation and maintenance
barriers still cost time. More independently provable record classes,
cross-authority handoff, distributed transactions, backup retention, schema
migrations and recovery drills remain part of a game's live-service work. See
[REGIONAL_PERSISTENCE.md](REGIONAL_PERSISTENCE.md).

The validation record reports the checks and native environments actually
exercised. Linux/Mesa software rendering evidence is not a gaming-GPU benchmark,
proof of Windows/macOS support or a server load test. A minimum supported native
window is a product target, not a claim of complete screen-reader, controller or
keyboard-only accessibility. See [VALIDATION.md](VALIDATION.md) and
[PRODUCT_REVIEW.md](PRODUCT_REVIEW.md) for current evidence and remaining gaps.

## Practical expansion order

1. **Pick a licensed regional vertical slice.** Select the terrain/feature epoch,
   player activities and target hardware. Establish source provenance and a
   measurable quality/frame-time budget.
2. **Replace reference content deliberately.** Keep commands and identity
   boundaries while adding production assets, physics/navigation, animation and
   a coherent game's inventory/skills/combat policies.
3. **Scale durable regional state.** Build on certified unchanged environmental
   groups, guarded page reuse and the worker by proving additional dependency
   classes. Keep differential full-merge checks and complete untrusted decode.
   Measure clone/install and authority-wait time alongside worker savings; test
   owner dormancy with every new game-owned dependency and preserve identity,
   money, cargo and depletion invariants.
4. **Build the Internet multiplayer boundary.** Operate verified TLS certificates,
   provision real-person accounts/recovery and authenticated administration,
   and define abuse policies around the existing trusted host controls. Validate
   the bounded owner predictor under declared latency/jitter and representative
   gameplay. Adapt interest/delta/resync and any game-specific lag compensation
   to the chosen transport guarantees.
5. **Partition simulation only after measuring it.** Choose ownership/handoff
   rules for bodies, contracts and cargo crossing regions. Avoid double-paying
   escrow or duplicating inventory during handoff and restart.
6. **Add evidence-backed specialist layers.** Historical populations, climate,
   agriculture or traffic models need their own provenance, uncertainty and
   validation. Keep authored gameplay assumptions visible.

The useful starting point is the connected, inspectable game loop and reusable
boundaries already present. Raising numeric caps alone does not complete the
production systems above.


### Historical 0.7 boundary additions

The following records what 0.7 introduced. Current additions and remaining
limits are described above; the archived [0.7 validation](VALIDATION_v0.7.md) is
the evidence for that earlier release.

- A company owns cash, routes and contracts; the existing player still owns the
  vehicles. Wages and fares are explicit transfers with bounded ledgers. Private
  replicas include only the requesting owner's commercial data.
- Contacts admit occupied bodies consistently for local, remote and autonomous
  motion. The translation sweep prevents tunneling within declared bounds;
  angular envelopes are conservative. Reference lanes restore useful traffic
  progression after occupancy became physical. Full impulse/tire/suspension,
  articulated-trailer and road-rule solvers were still game-provider work in
  that release. The current bounded source-rule/intersection subset is described
  above; full legal/physical completeness remains open.
- Offline owners freeze and release body/home residency. Reconnect hydration is
  behind capability, duplicate-connection and sequence checks. The source and
  item identities are unchanged; whole-world exports reassemble dormant pages.
- Input bindings, analog gamepad axes, route guidance/map and wheel animation
  stay presentation policy. They do not alter the authoritative motion contract.
  At that release, keyboard/mouse operated menus and RTS selection; 0.8 adds
  controller menu/text flows and configurable wheel axes while retaining
  mouse/keyboard RTS and management.

These changes close specific operational gaps. They do not turn the aggregate
512-body/128-player bounded authority into measured MMORPG capacity. Publish
representative workloads and failure/recovery evidence before raising that claim.

## 0.9 integration additions

See [SOURCE_BUNDLES.md](SOURCE_BUNDLES.md) for validated GeoJSON normalization,
coverage and content identity; [VEHICLE_DYNAMICS.md](VEHICLE_DYNAMICS.md) for sampled
wheel support and authored coefficients; [COMPANIES.md](COMPANIES.md) for private
cargo and reservation invariants; and [OPERATIONS.md](OPERATIONS.md) for trusted
closed-host provisioning. Current proof is listed in [VALIDATION.md](VALIDATION.md).
The separately versioned 0.8 timing reports remain historical measurements.
