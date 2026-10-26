# terra-net

A working authoritative multiplayer reference for georeferenced games. The
library owns connections, server ticks, movement validation, interest-filtered
snapshots, persistent sandbox commands and resume ownership. It has no Bevy
dependency and can run beside a headless Bevy server or another host loop.
Consumers needing only the protocol library can use `default-features = false`
to omit the standalone server's streaming/history and Ctrl+C adapter dependencies.
Normal workspace commands include the reference server by default.
See [the multiplayer guide](../../docs/MULTIPLAYER.md) for desktop clients,
historical packs, political revisions, operator behavior, and extension examples.

## Run two clients

From the workspace root, start the server and clients in separate terminals:

```sh
cargo run -p terra-net --bin terra-server -- --save saves/server.json
cargo run -p terra-net --bin terra-client -- --seconds 60 --east 2 --session saves/alice.json
cargo run -p terra-net --bin terra-client -- --seconds 60 --north 2 --session saves/bob.json
```

The server binds `127.0.0.1:7777`, starts near Munich (48.137, 11.575), simulates
at 20 Hz and publishes actor snapshots at 10 Hz. Its synthetic terrain uses the
same seed 42, level 15, resolution 33 surface as the desktop demo. `--lat`, `--lon`,
`--seed`, `--level` and `--resolution` configure these settings.
`--bind 0.0.0.0:7777` explicitly exposes it to the LAN. The
clients report their assigned actor IDs and received snapshot counts. A
deterministic bounded run uses `--ticks 100` on the server.

Use Ctrl+C for a clean server shutdown; with `--save`, it saves on shutdown and
every 30 seconds. Abrupt termination can lose work since the last save. Restart:

```sh
cargo run -p terra-net --bin terra-server -- --load saves/server.json --save saves/server.json
```

Reuse each client's `--session` file to recover its position, inventory, claims,
housing and command sequence. An anonymous reconnect receives a new player ID
and cannot inherit an old player's property. Session files contain random
256-bit bearer capabilities; the CLI pins them to the server address and the
save includes their ownership mapping. Protect these files. This protocol has
no TLS, account service, password recovery or secure Internet identity layer.

## Send a sandbox command

The reference client accepts one command JSON file with `--command PATH`.
For example, `craft.json` can contain:

```json
{"type":"craft","recipe":"planks","count":3}
```

```sh
cargo run -p terra-net --bin terra-client -- --session saves/alice.json --command craft.json
```

The server supplies the player ID and trusted position to `terra-sandbox`.
Clients submit intent, never their inventory or an authoritative player
position. Sandbox rule validation handles reach, ownership, costs, collisions
between authored structures, bounded command quantities, housing, crops and
resource changes. Each player's sandbox sequence survives saves and reconnects;
duplicate or older requests cannot repeat a craft, gather or build operation.

The starter inventory, recipes and blueprint costs come from the sandbox
catalog. A host can call `Server::set_catalog_and_starting_inventory` to install
a game catalog and its trusted initial equipment together, or `set_catalog` to
retain the current starter policy. Both are validated before mutation and are
included in saves. Existing data must remain compatible with the replacement.

Faction commands run through the same authority and replay checks. Leaders
invite players, grant roles, and transfer personally owned plots. Builders can
mutate shared assets; members can use shared housing/workbenches. Asset creator
IDs remain audit data and do not bypass the current faction role after demotion.

## Embed in a host

```rust,no_run
use terra_net::{Server, ServerConfig};
let mut server = Server::bind(ServerConfig::default())?;
// In your host loop: pass actual elapsed seconds, not a guessed frame count.
let report = server.update(1.0 / 60.0);
assert_eq!(report.domain_errors, 0);
# Ok::<(), terra_net::NetError>(())
```

Integrate the geographic world through two seams:

- `Server::register_chunk(&WorldChunk)` registers resource/feature metadata in
  the persistent sandbox overlay. Reloading source chunks does not refill
  resources or restore removed objects.
- `Server::set_environment(Box<dyn WorldEnvironment>)` supplies resident terrain
  and water heights. The standalone executable uses the same explicitly
  procedural surface as the offline viewer. `Server::bind` itself installs
  `FlatEnvironment` until the host replaces it. Movement is kinematic. A game supplies terrain
  support, collision, navigation, flight and marine physics as appropriate.

Environment queries execute on the server thread and must never perform
blocking network requests. The streaming layer should populate a resident
cache before queries. Store authoritative geographic positions as `f64` and
convert snapshots into each viewer's own floating origin when rendering.

`NetClient::connect` performs a startup connection with a three-second timeout.
`send` and `poll` thereafter are nonblocking. Poll regularly to flush pending
writes. `connect_with_token` resumes a saved player. `connect_with_manifest`
also checks source compatibility before allocating a new saved identity.
`SessionIdentity` provides bounded, address-pinned, private atomic file storage.
Welcome supplies the next
valid sandbox sequence. A viewer should interpolate snapshots, display command
rejections, and reconcile local prediction with the authoritative position.

## Protocol and authority

Frames are a four-byte big-endian unsigned length followed by UTF-8 JSON.
Protocol version 1 uses tagged snake-case message types. The decoder supports
partial headers, partial bodies and several frames in one TCP read.

| Direction | Message | Meaning |
|---|---|---|
| Client → server | `hello` | Protocol, interest, optional resume capability and expected data manifest |
| Client → server | `move` | Sequence and bounded ENU velocity intent |
| Client → server | `focus` | Spectator interest and bounded radius; never a teleport |
| Client → server | `sandbox` | Independent sequence plus a validated sandbox command |
| Client → server | `ping` | Liveness round trip |
| Server → client | `welcome` | Identity, immutable manifest, clock, owned actor and resume capability |
| Server → client | `snapshot` | Authoritative actor states, tick, time and explicit truncation |
| Server → client | `sandbox_snapshot` | Own inventory and bounded nearby sandbox overlay |
| Server → client | `sandbox_result` / `sandbox_rejected` | Command outcome tied to its sequence |
| Server → client | `error` | Validation/protocol error |

The immutable `BaseWorldManifest` carries a world ID, seed, source revision,
terrain provider, chunk level, terrain resolution, optional frozen data epoch,
and optional political dataset revision. Saves must match the host manifest.
Source compatibility checks do not depend on the host's save/session world ID.
A production deployment
should use a verified world-pack content hash; the reference manifest is
`procedural-v1`. Data downloads are a separate path from mutable gameplay
replication.

The server gives each connection one owned actor. Other actors can be inserted
by trusted host code. Movement limits depend on actor kind and prevent ground
actors from requesting vertical flight. Velocity intent expires after one
second without fresh input. Surface and air altitude constraints use the host
environment. Calendar speed is independent from physical actor movement.
Claim, build and plant commands must target a known surface within 3 meters of
the authoritative terrain height; this prevents unsupported airborne placements.
The standalone executable streams the union of player-neighbour tiles with
bounded workers and registers two explicitly authored
gameplay deposits: wood six meters east of spawn and stone six meters west.
Wood regenerates; the stone deposit is finite. Loaded saves retain depletion.

Actor AOI uses great-circle distance, including the antimeridian, and always
includes the client's own actor. Snapshots select the nearest eligible actors
within the configured capacity and report truncation. Sandbox views are centered
on the player's authoritative position, capped at 5 km, with at most 64 entries
per collection, plus at most eight relevant faction rosters. They are sent immediately after commands and periodically at
approximately one second; spectator focus affects actor views only.

## Resource limits

| Limit | Default / hard limit |
|---|---|
| Connected clients | 16 default, at most 128 |
| Saved players, including disconnected owners | 128 |
| Resident server actors | 10,000 default, at most 100,000 |
| Actor states per snapshot | 128 default, at most 512 |
| Frame body | 256 KiB |
| Pending output per connection | 512 KiB |
| I/O per poll / flush | 64 KiB |
| Messages consumed per peer per update | 32 |
| Protocol rate | 60 messages/s, burst 120 |
| Sandbox mutation rate | 10 commands/s, burst 20 |
| Unfinished handshake / idle connection | 10 s / 30 s |
| Fixed-step catch-up | 8 steps; excess time is reported |
| Server save | 32 MiB, strict schema and cross-reference validation |

Slow consumers exceeding their output budget are disconnected. There are no
unbounded per-client message queues. `ServerReport` exposes dropped time,
connection drops, commands consumed, snapshot counts and domain errors.

## Current scope

The reference is a single-process session server. Actor AOI currently scans
resident actors per client; large populations require a spatial index and
simulation partitions. It does not implement multi-server handoff, rollback,
lag compensation, client prediction, transport encryption, bandwidth-efficient
deltas, persistence journaling or geographic database distribution. TCP can add
head-of-line blocking under packet loss; fast action games should keep these
authority/domain contracts and replace the transport with an appropriate
reliable/unreliable channel system.

Persistent crop/resource advancement uses the server's accelerated world clock
and a deterministic weather sample at the configured spawn. Regional ecological
and weather-driven simulation belongs in a hosting world's regional systems.
Offline elapsed-time progression is not automatic: loading resumes the saved
simulation time. Base terrain/OSM bytes are not duplicated into server saves.

## Verification

```sh
cargo test -p terra-net
```

Tests exercise fragmented/coalesced frames, oversized input, unknown fields,
geographic AOI, fixed-step budgets, two real loopback clients, authoritative
movement, personal/faction ownership, role demotion, repeated commands,
save/restart/resume, custom catalogs, manifest preflight, maximum snapshot size,
capability-file storage and malformed-peer isolation. They do not require
Internet access or public data services. After `cargo build -p terra-net --bins`,
run `python3 crates/terra-net/tests/process_smoke.py` to exercise the actual
executables, including historical packs and mismatched political revisions.
