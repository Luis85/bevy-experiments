# Multiplayer host and integration guide

`terra-net` is an executable authoritative reference, with a standalone host,
terminal clients, and the desktop Bevy adapter. It supports shared movement,
inventories, land, structures, housing, crafting, crops, resources, faction
roles, possessed/RTS units, needs, wildlife, natural resources, named territories,
funded supply work, money, shops, production, delivery cargo, owned vehicles,
transport jobs, trailers and driver progression. Source terrain and political
geography are versioned separately from mutable gameplay. The transport is
bounded TCP JSON with optional verified TLS. Internet account hosting and
competitive action networking need further infrastructure.

## Start a shared world

Terra 0.24 uses **wire protocol 11**, retained from 0.22. It includes safe dissolution of unused solo communities alongside the existing community membership and
leadership commands. Protocol 9 introduced private company accounting and
the bounded `AccountingFull` hold status.
It retains the earlier private company-ledger, funded depot/freight and
completed-step movement-acknowledgement semantics. Update host and clients together. Earlier protocols
are rejected before allocating a principal or resume capability; TLS requires
the matching `terra-world/11` ALPN. Revisioned actor/sandbox updates replace unconditional full-view
transmission. This boundary is separate from source identity and validated
migration of earlier saved worlds.

Run these commands from the repository root in separate terminals:

```sh
cargo run -p terra-net --bin terra-server -- --save saves/server.json
cargo run -p terra-demo -- --connect 127.0.0.1:7777 --session saves/alice-session.json
cargo run -p terra-demo -- --connect 127.0.0.1:7777 --session saves/bob-session.json
```

The default host is Munich, 48.137° N / 11.575° E, with the explicitly synthetic
`terra-procedural-v2` surface, seed 42, level 15 and 65 samples per tile edge.
Movement and living state advance at 20 Hz. The host sends principal actor poses
at 10 Hz and living sandbox views at up to 5 Hz. These streams now carry complete
keyframes or smaller entity deltas. The desktop still receives reconstructed,
validated views, interpolates remote poses and optionally replays bounded input
for its owned display body. The preview never writes authoritative gameplay or
save state. See [client prediction](CLIENT_PREDICTION.md) for the 250 ms horizon,
missing-terrain stop, body-switch resets and measured limitations.

The executable enables an authored settlement by default. Each new player gets
one principal identity, a 250.00-credit starting wallet and five owned bodies:
avatar, two crew members, rover and survey helicopter. The identity and wallet
stay the same when possessing a different body. **F1** provides RTS selection
and orders; **F2/F3** possess a selected body in third/first person. **WASD** then
sends view-relative velocity intent for ordinary bodies and **R/F** controls aircraft altitude.
Registered vehicles instead use **W/S** for throttle/reverse, **A/D** for steering,
**Shift** for braking and **Space** for the handbrake. Pedal intent is bounded and
sequenced; the host owns vehicle dynamics, collision admission, fuel and wear.
**V/G** open transport work and the garage, **E** serves the current job stop,
and **C** changes the first/third-person camera. Driver needs come from the same
player's avatar, which must exit the vehicle to eat, drink or rest.
RTS air orders are horizontal destinations with cruise clearance; there is no
three-dimensional flight-route planner.
Changing camera GPS remains spectator interest, not a player teleport.

`--no-living` opts out of creating a settlement for a new host. It does not erase
living state restored from a save. In that legacy host mode, arrow keys retain
the original authoritative-player movement while WASD pans the camera. The
reusable `Server::bind` API does not seed a demo automatically; call
`enable_living_demo` when desired.

The desktop keeps its server address and a random resume capability in the
session file. Use a separate file for each player and server. Restarting the
client with that file resumes its original inventory, faction role, claims, and
home, money, active body and owned crew. An anonymous client gets a new identity. Corrupt, invalid, or differently
pinned identity files produce a visible error instead of silently abandoning an
existing player. Capability writes use private temporary files and atomic
replacement; capabilities are never printed.

The server defaults to loopback. Use a verified TLS listener for a remote host:

```sh
cargo run -p terra-net --bin terra-server --features tls -- \
  --bind 0.0.0.0:7777 --tls-cert /absolute/server.pem --tls-key /absolute/server-key.pem \
  --region-store saves/server.sqlite
cargo run -p terra-demo -- --connect 203.0.113.10:7777 \
  --tls-ca /absolute/issuer-ca.pem --tls-server-name example.net \
  --session saves/alice-session.json
```

Replace the example IP with the server's real address. The certificate must cover the verified server name and chain to the configured
CA. The client sends its Hello/resume capability only after verification and exact
`terra-world/11` ALPN negotiation. There is no plaintext fallback. Public test
certificates in the test fixtures are unsuitable for a deployed server.
Non-loopback plaintext requires explicit `--allow-insecure-remote` on the host
and client; use it only for an intentionally trusted test environment.
TLS provides channel confidentiality and server identity, while the resume
capability provides ownership continuity. Public account login, capability
recovery, abuse controls and distributed session ownership remain game/backend
responsibilities. Keep client identity files and server saves private.

## Save and resume the host

### Transactional regional storage

For the regional lifecycle, start and resume with the same command:

```sh
cargo run -p terra-net --bin terra-server -- \
  --region-store saves/server.sqlite
```

The host binds the store to its complete source manifest, catalog, host format,
and source tile level. It restores the last committed clock, player identities,
resume capabilities, inventories, wallets and command sequences before admitting
clients. Source registration hydrates persisted mutations before any provider
can seed resources. Depleted deposits and removed trees therefore retain their
state when a tile returns. Each resident source tile also retains the regions
containing its feature anchors, even when an imported feature starts beyond the
tile boundary. Shared anchors remain active until all contributing source tiles
evict, so a visible removed tree cannot reappear just because its anchor page
would otherwise sleep.

A checkpoint commits all regional records, the global ownership/economy ledger,
and private host metadata in one SQLite transaction. Only after that commit does
the runtime remove dormant records from the active sandbox. The reference host
checkpoints after input batches that admit players or apply sandbox commands,
after each source residency batch, every 30 seconds during otherwise stable play,
and on clean shutdown. Commands hydrate the complete logical world first so
housing, workbench, placement and global capacity rules include dormant records.
Its periodic console status reports committed
generation, active/dormant regions and record counts. Resume capabilities are
excluded from these diagnostics.

Geographic pages can hold environmental features, resource depletion, ambient
bodies, wildlife and eligible constructed assets. Principals, inventory/money,
factions, land rights, settlements, markets, production and funded delivery work
remain in the global ledger. Connected owners keep their bodies and homes resident.
On disconnect, owned bodies/homes can retire into geographic pages while private
reference descriptors preserve their account/ownership dependencies. Even if
another observer keeps the region loaded, offline owned bodies, needs, company
payroll and personal transport timers freeze. Resume hydrates before reactivating
the existing owner, without elapsed-wall-time catch-up or stale pedal input.
A cold region freezes its local simulation;
returning to it does not grant a wall-clock harvest or regenerate consumed goods.
Existing aggregate world limits still apply, and immutable source geometry has
its own separate cache.

An optimistic generation check rejects a stale second writer. A failed commit
does not retire live state or replace a committed generation. Player admission
and sandbox success responses are flushed only after their input-batch commit
succeeds. A failed checkpoint is reported in `ServerReport::persistence_error`;
queued responses are held, new input is not consumed, and simulation does not
advance until a retry succeeds. The reference executable stops with the error;
an embedding host may resolve the cause and retry. Ordinary storage work runs on
one bounded worker job; the simulation authority waits for its result while
nonblocking polls continue. Complete dependency validation still occurs on that
worker. Snapshot cloning and installation remain calling-thread work, and
startup, explicit exports and final flush are blocking boundaries. Movement and passive simulation snapshots can still be newer than
the most recent checkpoint, and an abrupt stop can lose those later changes.
It is not a distributed shard or a per-tick durability service.

A transient lock or storage condition can be retried on the same host after it
is resolved. A generation conflict requires stopping the competing authority and
restarting this `Server` from the latest store; the SDK does not replace a live
connected world with another generation. Clients resume with their committed
capabilities and command sequences.

Use `--save saves/export.json` alongside `--region-store` to produce a complete
portable server snapshot, including dormant records, on the normal save interval
and clean shutdown. The JSON export remains subject to its 32 MiB limit.
Export paths cannot alias the live database or its WAL, SHM or journal companions;
the CLI checks this before opening the world, and `save_world` enforces it for SDK
hosts as well.
`Server::backup_region_store` creates a consistent online SQLite backup at a new
destination, including committed WAL contents. Checkpoint first to include current
in-memory changes. You can also back up an export or the closed SQLite database;
preserve its private permissions because
both contain resume capabilities. The host does not print those capabilities.

`--load` and `--region-store` are separate CLI restore modes. Use the explicit
`--import-json` operation or `terra-worldctl` for no-overwrite conversion; see
[OPERATIONS.md](OPERATIONS.md). To import a legacy
JSON snapshot through a game host, call `Server::load_world`, then
`Server::open_region_store` on a new store, then `Server::checkpoint_regions`.
Upgrade an earlier reference catalog before attachment if needed, by enabling
the current authored living demo before opening the new store.
An existing store restores its own state. Catalog changes and JSON replacement
are rejected after attachment; migrate through an explicit complete export.
Attach before clients connect. API hosts should use `try_register_chunk` to
surface activation failures, `evict_chunk` to remove source interest, and
`checkpoint_regions` at their own batch boundary. `region_store_status` exposes
counts without private host metadata.

### Portable JSON snapshots

With `--save`, the host writes every 30 seconds and on Ctrl+C or normal bounded
shutdown. Restart with the same data settings:

```sh
cargo run -p terra-net --bin terra-server -- \
  --load saves/server.json --save saves/server.json
```

A server save contains the manifest, clock, NPCs, sandbox state, faction roles,
living units/needs/orders, accounts/money, shops, production, courier cargo,
catalog, new-player equipment policy, resume-capability ownership mapping and
last sandbox sequence for each player. Restoring existing living accounts does
not grant starter money or populate a second settlement. Source chunk bytes and political polygons
stay in their separate caches or packs. Load validates the complete state before
replacing the running world. A different world seed, pack revision, epoch, terrain
layout, political revision, or incompatible schema is rejected.

The JSON save is a size-limited snapshot with same-directory atomic replacement,
not a transaction journal. Abrupt termination can lose changes after the latest
save. Offline wall-clock time is not automatically simulated on restart. Back up
the host save and protect it as an identity-bearing file.

## Historical terrain and political geography

The included era examples are explicitly fictional test scenarios. They exercise
the same import, rendering, authority, and persistence path as supplied historical
datasets; they are not reconstructions of Munich in those years.

```sh
cargo run -p terra-net --bin terra-server -- \
  --world-pack crates/terra-history/data/synthetic-era-b/manifest.json \
  --epoch 2000-01-01 \
  --politics crates/terra-history/data/synthetic-politics-eras.geojson \
  --save saves/era-b-server.json

cargo run -p terra-demo -- \
  --world-pack crates/terra-history/data/synthetic-era-b/manifest.json \
  --epoch 2000-01-01 \
  --politics crates/terra-history/data/synthetic-politics-eras.geojson \
  --connect 127.0.0.1:7777 --session saves/era-b-alice.json
```

`--epoch` accepts signed Unix seconds or `YYYY-MM-DD`. The host requires a pack
when an epoch is supplied and requires an explicit epoch for a pack. It infers the
pack's uniform level and square grid resolution; explicit incompatible settings
are errors. Spawn must be covered by terrain. Player movement stops at uncovered
terrain, and there is no fallthrough to current data or synthetic terrain.

The selected data epoch stays frozen in the manifest. Game time advances from
that starting date; it does not select a new geographic pack automatically. The
political dataset is queried against simulation time by the viewer and can contain
dated changes. Its revision must match on every client. If current political
geometry is supplied without a historical terrain pack, the starting reference
calendar is advanced into that dataset's interval when necessary; this does not
turn procedural terrain into historical evidence.

The client sends its expected source identity in `Hello`. The server compares
provider, revision, seed, level, grid, epoch, and political revision before
allocating a saved player. The viewer checks the returned manifest again before
enabling controls. The host's `world_id` identifies its save/session namespace;
multiple hosts can use the same source data. Pack files are distributed separately
and should use immutable content-derived revision IDs.

## Sandbox, living and faction authority

### Frontier commands and saves

Ecology and settlement advancement run only on the host in a network session. New habitat bodies require resident terrain and clear authored/source footprints. The client renders species, behavior, resource depletion and territory boundaries from validated snapshots. It does not generate local wildlife, award itself experience or compute a successful delivery.

Settlement commands use the same envelope and per-principal sequence as building and trade:

```json
{"type":"settlement","action":{"type":"accept_contract","contract_id":2000000}}
```

Use an offer ID from the current view; this numeric example is illustrative. The host validates the recipient's accepted work, deadline, inventory and 20 m three-dimensional reach when fulfilling. Rewards were reserved from a finite market till when posted. Snapshots include public offers and only the recipient's accepted jobs/progress. Wildlife and resource metadata must reference corresponding visible bodies/nodes. Truncation is reported across all layers, and hidden private route/cargo/pocket state is not restored from client snapshots.

Named claims, public stocks/treasuries, ecology metadata, depletion, price anchors, escrow, deadlines and progress persist in the host save. New fields default when absent. An earlier living world receives additive nearby ecology without moving existing actors or resetting stock. Exact published v0.1/v0.2 reference catalogs can receive the new builtin content; a customized catalog is retained exactly. Manifest and catalog consistency checks still apply. Offline wall time does not advance deadlines or populations automatically.

Every network sandbox command is applied as the principal assigned to that
socket. When living state is enabled, that principal's server-computed interaction
position follows its active owned body. A command cannot supply a different caller,
rewrite an inventory, or teleport a player. The domain enforces a 20 m three-
dimensional interaction range, ownership, inventory costs, placement overlap, and
bounded damage/gather/craft quantities. Claim, build and plant targets also need
to be within 3 m of known authoritative support. The reference CLI additionally
rejects water and slopes above the placement limit. Building/crop footprint
checks also reject occupied source/authored geometry through its admission
policy.

Possession and RTS commands validate body ownership. A group order validates all
requested IDs before changing any unit. Needs and speed limits are computed on
the host. A client cannot submit money, item prices, shop inventory, production
outputs or a completed delivery. Buy/sell transfers validate finite stock,
customer/till balances and 20 m three-dimensional reach before updating either
side. Production consumes catalog items; courier pickup/dropoff transfers
actual goods after accepted movement. The first player owns the demo production
sites; later players cannot pause someone else's site.

The public living view contains only the requesting player's account/wallet.
NPC pocket money, cargo inventory and internal route arrays are removed from
replication. Visible delivery descriptions report progress, not a writable cargo
ledger. `LivingView::validate_for` checks partial views before installation;
full-world save validation uses the stronger ownership and reference invariants.

Faction roles are saved and replicated. Leaders invite players, grant builder or
member roles, and transfer their own plots to their faction. Builders can mutate
shared land and assets. Members may use shared housing and workbenches without
receiving construction or destruction rights. The original creator remains on
an asset for audit, but a demoted builder cannot bypass faction permissions by
pointing to their creator ID. Political countries and player factions remain
separate concepts; an authored political feature may refer to a gameplay faction.

Each player's sandbox sequence is independent from movement sequencing and
survives reconnects. Older or duplicate requests are rejected without repeating
their effect. Rule failures consume the accepted sequence too. Client UI displays
rejections and uses subsequent authoritative snapshots as its state.

Terminal automation can submit one command file:

```json
{"type":"craft","recipe":"planks","count":3}
```

```sh
cargo run -p terra-net --bin terra-client -- \
  --session saves/alice-session.json --command craft.json --seconds 2
```

Living commands use the same outer tagged command with a nested action. For
example, this command releases direct control while retaining the active body:

```json
{"type":"living","action":{"type":"release"}}
```

A reviewed personal purchase has this shape. Obtain `shop_id`, the item price
and quantity from the current authoritative view; the total below is an
illustrative **500 minor credits (5.00 credits)**, not a fixed bread price:

```json
{"type":"living","action":{"type":"trade_quoted","shop_id":1000000,"item":"bread","amount":1,"buy":true,"quoted_total":500}}
```

Use `buy:false` for a sale and retain the displayed sale total. The server rejects
a changed total before money or goods move, then the client must review a fresh
quote. IDs are scenario-specific, and command replay protection still applies.
The native UI uses the read-only `personal_trade_quote` preflight and this command.
Legacy `buy`/`sell` actions with `shop_id`, `item` and `amount` remain accepted,
but deliberately trade at the authority's current price at execution; they do
not preserve a price previously shown to the player.

The client exits unsuccessfully if its requested command is rejected or never
confirmed. `--expect-manifest FILE` pins a terminal client to an exact
`BaseWorldManifest` JSON, useful in automated scenarios and deployment checks.

## Integrate another game

The library has no Bevy dependency. Use `default-features = false` when the
standalone server executable is not needed.

```rust,ignore
let mut host = terra_net::Server::bind(config)?;
host.set_environment(Box::new(MyResidentEnvironment::new()));
host.set_catalog_and_starting_inventory(my_catalog, starter_items)?;
host.register_chunk(&normalized_chunk);
// Optional authored settlement; a custom catalog must include its required items.
host.enable_living_demo(&nearby_features)?;
let report = host.update(actual_elapsed_seconds);
```

`WorldEnvironment` supplies synchronous resident terrain/water sampling,
coverage, movement resolution and placement admission. It must not perform HTTP.
Its `register_chunk` and `evict_chunk` hooks let a host maintain collision/source
geometry as its source working set changes. `Server::register_chunk` also
registers immutable source metadata and resources in the mutable overlay without
refilling depletion or resurrecting tombstones. Custom catalogs get only
supported resource items.

The reference CLI streams the union of active player centers, the spawn
neighborhood and neighboring procedural or explicitly imported heightmap tiles,
with 256 resident/queued source
tiles, two workers and two
integrated completions per loop. Its `ResidentSurface` cache is independently
bounded at 512 chunks. Historical packs are bounded and loaded before listening;
every contained tile is registered.

The default library environment is explicitly flat until replaced. The CLI
uses `ResidentSurface` for source geometry and the procedural function, historical
pack or imported heightmap for base height/coverage. Vehicle wheel samples retain
the source road/land material; model-height clearance passes through the same
reference wrapper as ordinary movement. Unknown resident surface data blocks movement and
placement. Ground actors reject water and excessive slope (44° for biological
ground actors, 32° for vehicles); source building footprints are checked along a
bounded swept step. Bridges supply decks over water. Ships require water and air
bodies maintain at least 3 m terrain clearance. Shared authored-facility and
player-structure footprints use swept disks on the server. Failed movement
cannot complete a delivery.

These are conservative reference movement rules, with local sliding around
obstacles. They are not a complete rigid-body engine, building-volume collider,
flight model or general navigation mesh. Keep local and server policies aligned
when adding a provider or custom controller.

The reference server supports procedural, historical-pack and explicitly imported
heightmap manifests. A heightmap host and native client must load the same local
bundle and use matching seed, tile level, grid resolution and optional political
revision:

```sh
cargo run -p terra-net --bin terra-server --locked -- \
  --heightmap crates/terra-stream/data/heightmap-demo/manifest-v2.json \
  --level 15 --resolution 33 --seed 42 --save saves/heightmap-host.json
cargo run -p terra-demo --locked -- \
  --config configs/heightmap-multiplayer.json \
  --connect 127.0.0.1:7777 --session saves/heightmap-player.json
```

In the native Join menu, `configs/heightmap-multiplayer.json` provides these
matching source settings through the optional WorldConfig field.

The source identity uses `terrain_provider = "heightmap"` and
`HeightmapProvider::source_revision()`. Legacy raster-only manifests retain their
published revision for save compatibility. A manifest with explicit semantic
GeoJSON binds its canonical manifest, PNG and GeoJSON into the composite source
revision; editing the attached geometry changes admission/save identity even if
the declared revision text is unchanged. No map bytes travel through the gameplay
wire: distribute and verify the local bundle separately. The host registers
covered spawn neighbours before living seeding, then streams only fully covered
tiles through the ordinary bounded source worker. Missing coverage never becomes
procedural terrain. `--heightmap` cannot be combined with `--world-pack` or
`--epoch`; the raster has no invented historical date.

The desktop continues to reject `--connect` together with `--live`: mutable
public APIs are not silently treated as synchronized immutable multiplayer data.
Prepare an explicit versioned source bundle to host such data.

`NetClient::connect_with_manifest` performs a three-second startup connection;
`send` and `poll` are then nonblocking. The Bevy plugin runs initial connection and
identity loading on a worker, locks out local mutation before the first fixed
tick, drops stale actor replicas on disconnect, and reports handshake or snapshot
silence after ten seconds. It does not switch a disconnected multiplayer session
into a writable offline world.

## Revisioned replication and observability

Actor and sandbox streams each maintain an independent per-connection revision.
A keyframe has no base revision. A delta names the exact prior revision and
contains changed entities, departed-interest IDs, and an explicit collection
order when membership or ordering changes. An interest removal means “remove
this object from this replica,” not “delete it from the saved world.”

The host chooses a delta only when its encoded frame is smaller than a full
keyframe. It sends a full refresh at least every ten fixed-step seconds, independent of calendar speed and
on a new connection/resync. Unchanged actor views still send tick/time heartbeats.
The byte ceiling, public-view truncation, command sequencing and private wallet/
progress filtering remain in force. Per-peer caches contain only that peer's
bounded public view; they are transient and never saved as world authority.

`NetClient::poll` reconstructs wire updates atomically and returns the existing
`ServerMessage::Snapshot` / `SandboxSnapshot` API. Invalid shapes, conflicting
IDs, invalid recipients, inconsistent ecology references, and oversized
reconstructed views are rejected before installing a baseline. A missing
baseline triggers one pending `Resync` request. The host permits one resync per
real second; both streams send fresh keyframes. Queue admission must succeed
before a request becomes pending. A rate rejection schedules one retry after a
one-second client cooldown instead of leaving recovery stuck. Reconnect starts with empty
baselines and restores gameplay identity through the existing private session
capability, independently of revision counters.

`NetClient::replication_stats()` reports keyframes/deltas, changed/removed
entities, encoded replication bytes and the full-snapshot byte equivalent.
`transport_stats()` separately reports actual bytes read/written on the socket
and current/peak application pending bytes, excluding OS socket buffers.
`resync_pending()` exposes recovery status. `ServerReport.replication` reports
per-update payload counters and optional-view backpressure deferrals.
`deferred_command_batches` reports retained input waiting for response headroom;
`max_peer_pending_bytes` / `peak_peer_pending_bytes` expose queue pressure.
`interest_candidates_tested` counts exact-distance
principal AOI candidates. The standalone host aggregates these in its periodic
log. These are measured counters, not a promise about supported concurrent users.

A slow reader defers optional view construction before a baseline revision
advances. A single pending sandbox-refresh flag coalesces skipped views into the
latest eligible state after drain; already queued frames preserve their revision
chain. Optional traffic leaves 64 KiB of reliable reserve, and each request waits
until one maximum-size response fits. The 32-message per-peer inbox and existing
read/write budgets remain bounded, while other peers retain their allowances.
The queue cap and idle/malformed-peer policies remain in force.

The host also uses the transient `EcologyScheduler`. Connected principals' owned
bodies receive near-observer priority; nearby physical threats retain immediate
behavior checks. Far AI decisions use a bounded cadence, while living motion,
needs, production, cargo and currency still advance on their normal schedule.
`Server::set_ecology_schedule` configures that host policy and
`ecology_schedule_stats()` exposes evaluated/deferred work. Loading a world
resets the scheduler even when the restored clock is equal to the old clock.

See [NETWORK_REPLICATION.md](NETWORK_REPLICATION.md) for wire details, failure
behavior and the finite loopback load harness.

## Budgets and current deployment limits

| Resource | Policy |
|---|---|
| Active connections | 16 default; configurable up to 128 |
| Persisted players | 128 per world, including disconnected players |
| Server actor records | 10,000 default; configurable up to 100,000 |
| Living bodies | 512 total, including residents and all owned crews; admission reserves all five starter units |
| Group order | 1–32 unique owned bodies; destinations within 2 km |
| Actors per snapshot | 128 default; configurable up to 512 |
| Nearby sandbox entries | Up to 64 per collection, 5 km maximum radius |
| Initial living view | At most 64 bodies, 8 shops, 16 sites, 16 deliveries and 8 roads |
| Complete sandbox view | At most 240 KiB serialized; further explicit truncation protects the frame budget |
| Faction view | Up to 8 rosters; each roster/invite list at most 128 |
| TCP frame body / queued output | 256 KiB / 512 KiB per connection |
| I/O / decoded messages | 64 KiB per poll or flush; 32 messages per update |
| Message / sandbox rates | 60/s burst 120; 10/s burst 20 |
| Handshake / idle deadlines | 10 s / 30 s |
| Input expiry | Velocity stops after 1 s without fresh intent |
| Replication refresh / resync | Keyframe within 10 fixed-step seconds; at most 1 explicit resync per real second |
| Catch-up work | At most 8 fixed steps; dropped time is reported |
| Persistent domain entities | 10,000 per bounded domain collection |
| Server save | 32 MiB maximum |

Actor interest uses great-circle distance and always includes the owned actor.
Sandbox interest follows the active body's trusted position, independently of
spectator camera focus. Active and original-avatar bodies receive priority in
a living view. The 512-body cap may stop new living players before the separate
128-player storage ceiling is reached. Snapshots explicitly report truncation. Collection caps prevent
unbounded growth; they are not a world-scale database or a pagination API.

The current host is one process. Immutable spherical spatial indices are rebuilt
once per replication fanout and reused for all clients; sandbox indices are
rebuilt only on sandbox-view passes. Queries retain exact great-circle admission
and deterministic ownership/distance/ID selection. Bounded entity deltas reduce
repeated static state. There is no distributed partition server, migration/handoff, whole-world rollback or lag-compensated combat. Optional verified TLS protects the transport. Native interpolation and bounded owner prediction/reconciliation improve display response without changing gameplay authority; see [client prediction](CLIENT_PREDICTION.md).
TCP may delay later snapshots behind lost packets. Crop/resource advancement
uses one deterministic weather sample at spawn, not a regional physical weather
simulation. A larger service needs admission/account policy, durable region
storage, durable region ownership, authoritative regional environment services, and a
transport selected for its gameplay latency requirements.

## Reproduce the network checks

```sh
cargo test -p terra-net --no-default-features
cargo build -p terra-net --bins
python3 crates/terra-net/tests/process_smoke.py
```

The process runner looks in `target/debug` by default. If you set
`CARGO_TARGET_DIR` or use release binaries, pass their exact paths:

```sh
python3 crates/terra-net/tests/process_smoke.py \
  --server /path/to/terra-server --client /path/to/terra-client
```

The tests use local loopback sockets and temporary files. They cover bounded
framing, two-client movement, interest filtering, replay rejection, terrain
coverage, malformed-peer isolation, faction promotion/demotion, custom catalogs,
resume identity, manifest preflight, save validation, maximum snapshot size,
finite trade, possession, RTS orders and living persistence.
The separate process smoke runs the actual host/client binaries, restarts two
players, then connects a historical/political client and rejects incompatible
pack, epoch, and politics revisions. It does not contact public data providers.

## Trusted provisioning and conditioned transport acceptance

Resume-only hosts can create new dormant principals through
`Server::provision_principal` / `provision_principal_for` without opening guest
registration. The bounded operation uses the active starter policy, validates a
candidate, and commits ownership plus its private capability atomically when
regional storage is attached. A failed writer does not consume an ID or change
cold residency. The embedding game authorizes the caller and privately delivers
the returned `SessionIdentity`; public account enrollment remains separate.
See [Operations](OPERATIONS.md#provision-a-new-principal-without-opening-guests).

`cargo run -p terra-net --example network_conditions --locked -- tls` runs a
finite actual-socket delay/jitter/fragmentation and paused-reader fixture, with
a healthy observer, exact drain convergence, private finite inventory, company
filtering, host restart and replay-safe resume. `tcp` runs the same scenario
without encryption. See [the condition bounds and interpretation](NETWORK_REPLICATION.md#reproduce-delayed-fragmented-and-stalled-connections).
