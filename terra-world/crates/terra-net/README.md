# terra-net

A working authoritative multiplayer reference for georeferenced games. The
library owns connections, server ticks, movement validation, interest-filtered
snapshots, persistent sandbox commands and resume ownership. It has no Bevy
dependency and can run beside a headless Bevy server or another host loop.
Consumers needing only the protocol library can use `default-features = false`
to omit the standalone server's streaming/history and Ctrl+C adapter dependencies
and the optional TLS dependency; add `features = ["tls"]` to retain verified TLS.
The library still includes its regional-storage dependency and bundled SQLite;
this feature switch does not produce a transport-only build.
Normal workspace commands include the reference server and TLS support by default.
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
at 20 Hz and publishes actor snapshots at 10 Hz. The reference server's synthetic
terrain defaults to seed 42, level 15 and 65 samples per tile edge. Desktop clients
adopt the host's source manifest when joining. `--lat`, `--lon`,
`--seed`, `--level` and `--resolution` configure these settings.
Non-loopback listeners require TLS configuration or an explicit
`--allow-insecure-remote` choice, as described below. The clients report their
assigned actor IDs and received snapshot counts. A
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
save includes their ownership mapping. Keep these files private. Verified TLS
protects the capability in transit; account enrollment, password/capability
recovery and distributed identity services remain the hosting game's responsibility.

## Use a verified remote connection

Supply a certificate chain and matching private key on the server. Clients need
an explicit trusted CA and the expected certificate hostname or IP SAN:

```sh
cargo run -p terra-net --bin terra-server -- \
  --bind 0.0.0.0:7777 --region-store saves/server.sqlite \
  --tls-cert certificates/server-chain.pem --tls-key certificates/private-key.pem

cargo run -p terra-net --bin terra-client -- \
  --connect 192.0.2.10:7777 --tls-ca certificates/trusted-ca.pem \
  --tls-server-name world.example --session saves/alice.json

cargo run -p terra-demo -- \
  --connect 192.0.2.10:7777 --tls-ca certificates/trusted-ca.pem \
  --tls-server-name world.example --session saves/alice-native.json
```

The address and hostname above are placeholders. Use the actual server address
and a name covered by its certificate. Rustls verifies the chain and server name;
the client withholds its Hello/resume capability until verification and exact
`terra-world/11` ALPN negotiation finish. TLS early data is disabled, and failures
do not fall back to plaintext. `NetClient::is_encrypted()` reports a completed
verified handshake. Public test certificates and keys must never be deployed.

Plaintext loopback remains available for local development. Deliberate
non-loopback plaintext testing requires `--allow-insecure-remote` on both the
reference listener and client. The native launcher exposes the same explicit
choice. Library embedders own their deployment policy and can use
`ServerTlsConfig::from_pem_files`, `Server::bind_tls`,
`ClientTlsConfig::from_ca_pem_file` and `NetClient::connect_tls`.
See [Operations](../../docs/OPERATIONS.md#tls-transport-and-player-capabilities)
for configuration bounds and private-file handling.

## Run a persistent regional world

Create and resume with the same command:

```sh
cargo run -p terra-net --bin terra-server -- --region-store saves/server.sqlite
```

The SQLite store commits the global gameplay ledger, geographic pages and private
host identity metadata together. Depletion, destruction, ownership, money and
command sequences survive restart. Source registration restores old mutations
before seeding, then dependency-safe regions can leave the active simulation.
The host checkpoints joins and sandbox mutation/source batches, every 30 seconds
and on clean exit. Ordinary `Move`, `Drive`, `Focus` and `Ping` messages do not
trigger a checkpoint or full-world hydration for each input.
Player admission and sandbox success responses wait for their batch commit;
movement and passive simulation can remain newer than the last checkpoint.

`--save saves/export.json` can additionally export a complete portable server
snapshot, including dormant records. This full JSON export is synchronous
maintenance work, separate from the background checkpoint path. Export
destinations must be separate from the database and its SQLite companion files,
including file aliases. CLI `--load` and `--region-store` are separate restore
modes. Use `--import-json OLD.json --region-store NEW.sqlite` for validated
one-time migration, or `--region-store EXISTING.sqlite --export-json NEW.json`
for a maintenance export. Both exit before simulation and require new output
paths. Supply the original world/source flags. See
[Operations](../../docs/OPERATIONS.md) for inspection, online backup and conversion,
and [durable regional state](../../docs/REGIONAL_PERSISTENCE.md) for residency,
identity and recovery rules.

Attach with `Server::open_region_store` before clients connect. Use
`request_region_checkpoint` for ordinary background requests, `persistence_pending`
and `region_store_status` for progress, and `checkpoint_regions` for an explicit
synchronous maintenance/final barrier. `backup_region_store` creates a consistent
new backup. Use `prepare_source_batch` to request background activation; retain
the bounded source event batch until it returns ready, then use
`try_register_chunk`/`try_evict_chunk` and request the batch's checkpoint.
A failed storage barrier holds pending success responses and stops authoritative input
and simulation; inspect `ServerReport::persistence_error` and resolve or stop the
host. Ordinary hydration/checkpoints run through one bounded worker; poll
`ServerReport::persistence_pending` and `storage_work_ms` without blocking the
host loop. Complete dependency validation still runs on the worker, while
snapshot cloning and completion installation remain main-thread costs. Fixed
authority work waits during the transition and does not catch up storage wait
time afterward. Explicit startup, restore, export and final flush remain
blocking maintenance barriers. Aggregate
world caps remain unchanged.

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

- `Server::try_register_chunk(&WorldChunk)` registers resource/feature metadata in
  the persistent sandbox overlay. Reloading source chunks does not refill
  resources or restore removed objects. `register_chunk` is the compatibility
  wrapper that reports failures to standard error.
- `Server::set_environment(Box<dyn WorldEnvironment>)` supplies resident terrain
  and water heights. The standalone executable defaults to the same explicitly
  procedural surface as the offline viewer and accepts explicit historical packs
  or matching immutable heightmap/semantic bundles through `--heightmap`. `Server::bind` itself installs
  `FlatEnvironment` until the host replaces it. Ordinary bodies use kinematic
  movement; registered road vehicles use the shared reference pedal/steering
  dynamics. The vehicle envelope checks model-sized support points, provider
  coverage and finite returned surfaces. Missing coverage stops the vehicle
  without charging collision wear. A game supplies its terrain, rigid-body
  collision, navigation, flight and marine physics as appropriate.

Environment queries execute on the server thread and must never perform
blocking network requests. The streaming layer should populate a resident
cache before queries. Store authoritative geographic positions as `f64` and
convert snapshots into each viewer's own floating origin when rendering.

`NetClient::connect` performs a startup connection with a three-second timeout.
`send` and `poll` thereafter are nonblocking. Poll regularly to flush pending
writes. `connect_with_token` resumes a saved player. `connect_with_manifest`
also checks source compatibility before allocating a new saved identity.
`SessionIdentity` provides bounded, address-pinned, private atomic file storage.
`poll` atomically reconstructs the wire deltas into the established `Snapshot`
and `SandboxSnapshot` messages; the game still validates its source and catalog
policy. `replication_stats()` exposes payload reductions and keyframe/delta counts;
`transport_stats()` measures actual socket bytes. A missing baseline requests a
bounded resync. Welcome supplies the next valid sandbox sequence. A viewer should interpolate snapshots, display command
rejections, and reconcile local prediction with the authoritative position.

## Protocol and authority

Frames are a four-byte big-endian unsigned length followed by UTF-8 JSON.
Protocol version 11 uses tagged snake-case message types. See the
[current compatibility contract](../../docs/COMPATIBILITY.md) for matching
executable, TLS and save requirements. The decoder supports
partial headers, partial bodies and several frames in one TCP read.

| Direction | Message | Meaning |
|---|---|---|
| Client → server | `hello` | Protocol, interest, optional resume capability and expected data manifest |
| Client → server | `move` | Sequence and bounded ENU velocity intent |
| Client → server | `drive` | Shared movement sequence plus normalized throttle, brake, steering and handbrake; no coordinates |
| Client → server | `focus` | Spectator interest and bounded radius; never a teleport |
| Client → server | `sandbox` | Independent sequence plus a validated sandbox command |
| Client → server | `resync` | Request complete actor/sandbox baselines; rate limited |
| Client → server | `ping` | Liveness round trip |
| Server → client | `welcome` | Identity, immutable manifest, clock, owned actor and resume capability |
| Server → client | `actor_update` | Revisioned keyframe or entity delta, tick, time and explicit truncation |
| Server → client | `motion_state` | Recipient-only controlled body pose/configuration and movement sequence from a completed host step, for bounded display prediction |
| Server → client | `sandbox_update` | Revisioned recipient-private sandbox keyframe or entity delta |
| Server → client | `sandbox_result` / `sandbox_rejected` | Command outcome tied to its sequence |
| Server → client | `pong` | Echoed nonce and current host tick |
| Server → client | `error` | Validation/protocol error |

The immutable `BaseWorldManifest` carries a world ID, seed, source revision,
terrain provider, chunk level, terrain resolution, optional frozen data epoch,
and optional political dataset revision. Saves must match the host manifest.
Source compatibility checks do not depend on the host's save/session world ID.
A production deployment
should use a verified world-pack content hash; the reference manifest is
`terra-procedural-v2`. Data downloads are a separate path from mutable gameplay
replication.

The server gives each connection one owned actor. Other actors can be inserted
by trusted host code. Movement limits depend on actor kind and prevent ground
actors from requesting vertical flight. Velocity intent expires after one
second without fresh input. Registered road vehicles reject nonzero `move` and
require `drive`; zero `move` is an explicit stop. Drive requires possession of
the owner's active registered vehicle, shares replay protection with movement,
and is cleared on successful control transitions. Stale pedal input brakes the
vehicle. Licensing, fuel, condition, parking brake and the present driver's needs
constrain the host's admitted motion. The host computes fuel, mileage and wear.
Surface and air altitude constraints use the host environment. Calendar speed
is independent from physical actor movement; world pause stops physical motion
and transport clocks.
Claim, build and plant commands must target a known surface within 3 meters of
the authoritative terrain height; this prevents unsupported airborne placements.
The standalone executable streams the union of player-neighbour tiles with
bounded workers (coverage-filtered for imported heightmaps) and registers two explicitly authored
gameplay deposits: wood six meters east of spawn and stone six meters west.
Wood regenerates; the stone deposit is finite. Loaded saves retain depletion.

Actor AOI uses great-circle distance, including the antimeridian, and always
includes the client's own actor. Snapshots select the nearest eligible actors
within the configured capacity and report truncation. Sandbox views are centered
on the player's authoritative position, capped at 5 km, with at most 64 entries
in the principal spatial collections, plus at most eight relevant faction
rosters and the transport domain's smaller collection bounds. Public vehicle
appearance and attached trailers follow visible living bodies. Full vehicle
records, driver progress, assigned transport work and personal bests are private
to their owner; another player's wallet, fuel and cargo are omitted. They are
sent immediately after commands and periodically at
up to five times per second in a living world (once per second otherwise); spectator focus affects actor views only.

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
| Complete server JSON export | 32 MiB, strict schema and cross-reference validation |
| Regional checkpoint payload | 64 MiB; each serialized document at most 32 MiB |
| Stored regional pages | 4,096; lower aggregate gameplay caps still apply |

Slow consumers defer optional snapshot construction before its baseline advances;
one pending sandbox refresh coalesces skipped views after the queue drains.
Already queued revision chains are preserved. Optional traffic leaves 64 KiB of
control reserve; reliable requests stay in the bounded inbox until one maximum
response fits. A peer still exceeding a hard limit or its idle deadline can be
disconnected. There are no unbounded per-client message queues. `ServerReport` exposes dropped time,
connection drops, commands consumed, snapshot/command deferrals, current/peak
peer output bytes, domain errors and regional
checkpoint failures.

## Current scope

The reference is a single-process session server. Shared spherical actor and
sandbox spatial indices serve interest queries; per-peer revisioned keyframes
and entity deltas avoid retransmitting unchanged entities. Opt-in regional
storage pages mutable environmental records while keeping ownership and economic
dependencies authoritative. A bounded worker handles ordinary storage requests;
verified TLS protects reference clients when enabled. Larger populations still
require selective durable queries, account-service storage and simulation partitions.
Disconnected owners now freeze and retire body/home region pins, while saved
principals, accounts and transport ledgers still count against bounded global caps.
Bounded owner display prediction/reconciliation is included; see
[CLIENT_PREDICTION.md](../../docs/CLIENT_PREDICTION.md). It does not implement
multi-server handoff, whole-world rollback, combat lag compensation, public
account services, application command journals or geographic database
distribution. TCP can add
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
Internet access or public data services. Regional tests add real SQLite restart,
dormant depletion/tombstone round trips, stale-writer rejection, metadata guards
and a real write-lock failure proving success responses wait for commit. Driving
tests cover pedal authority, control ownership, stale input, public appearance
and private financial records, transport restart and unloaded envelope coverage.
Company command/delta tests check recipient privacy and transfer replay. A cold
owner fixture retires real bodies and housing, restarts the host, and resumes
the same identity, inventory, account values, needs and controlled-body heading.
TLS tests verify trusted commands/resume, reject untrusted CAs or names before
admission, isolate plaintext peers on a TLS listener, and enforce configuration
bounds and exact protocol ALPN. These are correctness cases, not Internet
deployment or capacity tests.
After `cargo build -p terra-net --bins`,
run `python3 crates/terra-net/tests/process_smoke.py` to exercise the actual
executables, including historical packs and mismatched political revisions.


The [replication design and finite load harness](../../docs/NETWORK_REPLICATION.md)
records exact bounds, failure semantics and how to run a measured workload. Its
v0.4 performance results remain explicitly archived; they predate current
driving, TLS and regional storage and must not be presented as fresh measurements.

The [0.7 headless gate](../../docs/headless-verification-v0.7.json) records 86
distinct `terra-net` Rust tests within the complete headless workspace gate.
The [0.7 server and operations smoke](../../docs/server-operations-v0.7.json)
verifies actual JSON/SQLite, portable archive and backup roundtrips; no-overwrite
and database-alias guards; private output permissions; remote plaintext argument
policy; and localhost TLS listener startup. These process checks are not added
to the Rust test count; the TLS Rust suite verifies actual handshakes.

### Archived 0.7 session and protocol additions

Protocol 5 and exact `terra-world/5` TLS ALPN include private transport-company
views. Real TCP tests exercise company command/delta privacy, transfer replay,
obsolete-protocol rejection and cold-owner resume across a database restart.
Unauthenticated or already-replayed commands do not activate cold pages.

Disconnect clears stale physical control and pauses personal bodies, needs,
company payroll and transport timers. Owned homes/bodies can leave the working
set; small private descriptors preserve validation references. Reconnect hydrates
before waking the same owner, without wall-clock catch-up. Host restart marks all
saved owners dormant because no socket survives the process. Other world economy
work continues under the existing policy. See the [regional lifecycle](../../docs/REGIONAL_PERSISTENCE.md#owner-disconnect-cold-records-and-resume).

## Operator and enrollment boundary

Trusted Rust hosts can provision a new dormant principal, inspect exact principal
status, suspend/readmit an existing principal, rotate its bearer capability and
inspect a bounded secret-free action audit. Regional commits complete before live capabilities/connections change; a
failed write retains the old session and policy. `--resume-only` (or
`ServerConfig.allow_new_players = false`) rejects fresh guest registration before
hydration while allowing valid unsuspended saved capabilities. The startup policy
is deployment configuration, not copied from saves. `provision_principal` and
`provision_principal_for` create one bounded candidate with the current starter
policy and return a private `SessionIdentity`; with regional storage it commits
before live installation. An in-memory host must save before private delivery. These controls do not perform
real-person enrollment or external account recovery. See [Operations](../../docs/OPERATIONS.md).

The finite `regional_session_load` example combines real TCP/TLS clients with
living owners, regional persistence, half-session churn, repeated host restarts
and a forced storage-lock failure/retry. It reports exact private-state checks,
actual socket bytes, host-update tails, authority waits and process RSS samples;
no bearer secret or private fixture save is emitted. It does not establish a
production concurrency or long-duration memory capacity.

The [final 0.8 headless gate](../../docs/headless-verification-v0.8.json) records
**100 distinct `terra-net` tests** within 391 headless Rust cases. Five operator
regressions exercise durable suspension/readmission and rotation, a failed writer
without policy/session changes, exact-ID admission, bounded secret-free audit,
explicit recovery endpoints and resume-only rejection before hydration. The
network suite also covers private movement acknowledgements, regional restart,
TLS identity validation, replay handling and model-sized underpass contacts.
Source and fixture hashes remained unchanged across that complete final gate;
targeted reruns are not added to the count.

The `network_conditions` example and shared integration tests exercise actual TCP
and TLS through an opaque finite relay with 25 ms scheduled one-way delay,
0–15 ms ordered jitter, bounded write fragments and a paused receiver. They
require host snapshot deferral, healthy-peer service, exact current-view drain,
private company/inventory continuity, durable restart and stale-command rejection.
Run `cargo run -p terra-net --example network_conditions --locked -- tls` (or
`tcp`). JSON output reports measured queue/byte/deferral counters and explicit
limits; no capability or private save is emitted. This is transport correctness
evidence, not packet-loss emulation or production concurrency certification.

### Shared heightmap source

The reference host accepts `--heightmap PATH` with explicit `--level` and
`--resolution` if different from its defaults. Native clients use the same local
bundle/settings and verified source identity; the wire transfers mutable views,
not map files. Semantic bundles bind manifest/raster/GeoJSON content into
`HeightmapProvider::source_revision()`, while legacy raster-only identity remains
compatible. The spawn tile must be wholly covered, and only covered neighbours
enter the bounded streamer. Combining this source with `--world-pack`/`--epoch`
rejects before hosting. See [the shared heightmap commands](../../docs/MULTIPLAYER.md).
