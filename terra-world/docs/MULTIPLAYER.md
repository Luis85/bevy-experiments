# Multiplayer host and integration guide

`terra-net` is an executable authoritative reference, with a standalone host,
terminal clients, and the desktop Bevy adapter. It supports shared movement,
inventories, land, structures, housing, crafting, crops, resources, faction
roles, possessed/RTS units, needs, money, shops, production and delivery cargo. Source terrain and political geography are versioned separately from
mutable gameplay. The current transport is bounded TCP JSON for a small trusted
session; Internet account hosting and competitive action networking need further
infrastructure.

## Start a shared world

Run these commands from the repository root in separate terminals:

```sh
cargo run -p terra-net --bin terra-server -- --save saves/server.json
cargo run -p terra-demo -- --connect 127.0.0.1:7777 --session saves/alice-session.json
cargo run -p terra-demo -- --connect 127.0.0.1:7777 --session saves/bob-session.json
```

The default host is Munich, 48.137° N / 11.575° E, with the explicitly synthetic
`terra-procedural-v2` surface, seed 42, level 15 and 65 samples per tile edge.
Movement and living state advance at 20 Hz. The host sends principal actor poses
at 10 Hz and living sandbox views at up to 5 Hz. The desktop interpolates
validated poses without speculative local movement.

The executable enables an authored settlement by default. Each new player gets
one principal identity, a 250.00-credit starting wallet and five owned bodies:
avatar, two crew members, rover and survey helicopter. The identity and wallet
stay the same when possessing a different body. **F1** provides RTS selection
and orders; **F2/F3** possess a selected body in third/first person. **WASD** then
sends view-relative velocity intent and **R/F** controls aircraft altitude.
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

The server defaults to loopback. `--bind 0.0.0.0:7777` opts into a LAN listener;
clients then use that machine's address. There is no TLS, account login, capability
recovery, or server-identity verification. A bearer capability provides ownership
continuity only while the transport, client file, and server save are protected.
The reference does not claim secure public Internet hosting.

## Save and resume the host

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

The save is a size-limited JSON snapshot with same-directory atomic replacement,
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

A buy command has this shape; obtain `shop_id` from the current authoritative
view rather than assuming IDs are stable across scenarios:

```json
{"type":"living","action":{"type":"buy","shop_id":1000000,"item":"bread","amount":1}}
```

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
neighborhood and neighboring procedural tiles, with 256 resident/queued source
tiles, two workers and two
integrated completions per loop. Its `ResidentSurface` cache is independently
bounded at 512 chunks. Historical packs are bounded and loaded before listening;
every contained tile is registered.

The default library environment is explicitly flat until replaced. The CLI
uses `ResidentSurface` for source geometry and the procedural function or pack
for base height/coverage. Unknown resident surface data blocks movement and
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

The built-in reference server supports procedural and historical-pack manifests.
The desktop intentionally rejects `--connect` together with `--live` or
`--heightmap`: mutable public APIs and arbitrary local rasters are not silently
accepted as synchronized multiplayer base data. A game can prepare equivalent
immutable chunks and extend the host provider/manifest policy deliberately.

`NetClient::connect_with_manifest` performs a three-second startup connection;
`send` and `poll` are then nonblocking. The Bevy plugin runs initial connection and
identity loading on a worker, locks out local mutation before the first fixed
tick, drops stale actor replicas on disconnect, and reports handshake or snapshot
silence after ten seconds. It does not switch a disconnected multiplayer session
into a writable offline world.

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
| Catch-up work | At most 8 fixed steps; dropped time is reported |
| Persistent domain entities | 10,000 per bounded domain collection |
| Server save | 32 MiB maximum |

Actor interest uses great-circle distance and always includes the owned actor.
Sandbox interest follows the active body's trusted position, independently of
spectator camera focus. Active and original-avatar bodies receive priority in
a living view. The 512-body cap may stop new living players before the separate
128-player storage ceiling is reached. Snapshots explicitly report truncation. Collection caps prevent
unbounded growth; they are not a world-scale database or a pagination API.

The current host is one process. Actor interest scans resident actors per client.
There is no spatial partition server, migration/handoff, replication delta codec,
prediction/reconciliation loop, rollback, lag compensation or encrypted channel.
Native interpolation smooths received positions; it does not change those
authority or latency limits.
TCP may delay later snapshots behind lost packets. Crop/resource advancement
uses one deterministic weather sample at spawn, not a regional physical weather
simulation. A larger service needs admission/account policy, durable region
storage, spatial indexing, authoritative regional environment services, and a
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
