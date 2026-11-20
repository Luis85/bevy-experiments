# Network protocol 11, replication and transport

The current host uses revisioned entity deltas over bounded framed TCP, with
optional verified TLS. Commands and physical movement remain server-owned;
recipient views separate visible world appearance from private player records.
Protocol **4** introduced pedal and steering intent plus transport replication. Network
protocol versions are independent of domain save and regional database versions.
An older client must upgrade its wire protocol; this does not require rewriting
a compatible complete version-one world save.

This is one bounded regional authority. The implementation and correctness tests
below describe the current code. The performance numbers at the end are an
**archived v0.4 workload**, not a measurement of protocol 11, TLS, regional storage,
or the current transport game. No section certifies production MMORPG capacity.

Protocol 5 introduced recipient-only transport-company state in full views and
delta headers. Protocol 6 introduced private `MotionState` snapshots for the recipient's
active body, including the movement sequence applied by a completed host step.
These snapshots support bounded display prediction and reconciliation without
changing gameplay authority; see [CLIENT_PREDICTION.md](CLIENT_PREDICTION.md). Company cash, driver wages, routes and loaded trip escrow never
appear in another owner’s view. Header installation remains atomic and passes
complete replica validation before becoming a baseline. Earlier protocol peers fail
application admission, and an obsolete TLS ALPN cannot negotiate a session.
Protocol 7 introduced the finite depot market and funded freight command/view semantics. Protocol 8 added `LivingCommand::TradeQuoted`, carrying the personal trade total reviewed by the client. The host rejects a changed total before any stock or account mutation; command replay protection still applies. Protocol 9 introduced owner-private categorized cash records and the `AccountingFull` hold status. Protocol 10 introduced explicit community invitation, membership and leadership commands. Protocol 11 adds `DissolveFaction` for an asset-free sole-leader community, with current-authority checks and normal replay rejection. Hosts and clients must upgrade together; see [Compatibility](COMPATIBILITY.md). Offline-owner residency descriptors remain authority-only storage metadata and
are absent from this protocol.

## Connection and authoritative control

`Hello` declares the protocol version, initial spectator interest, optional
private resume capability and optional expected base-world manifest. The host
rejects a protocol or manifest mismatch before assigning a principal. A resume
capability restores the same saved player; it does not create another wallet or
reset accepted command sequences. Two active connections cannot control the same
saved principal.

| Client message | Authority contract |
|---|---|
| `Move` | Bounded finite velocity intent for the active body, with current needs and terrain admission enforced by the host. A registered road vehicle rejects nonzero `Move`; zero velocity is an explicit stop. |
| `Drive` | Normalized throttle in `[-1, 1]`, brake in `[0, 1]`, steering in `[-1, 1]`, and handbrake. Requires possession of the recipient's registered active vehicle. The host computes pose, speed, heading, fuel, damage and mileage. |
| `Focus` | Changes spectator interest. It cannot move the player or an owned body. |
| `Sandbox` | Validated gameplay command, including possession, ownership, transport work and commerce. Uses its own persisted replay sequence and returns an authoritative outcome or rejection. |
| `Resync` | Requests fresh replication baselines at a bounded rate. It changes no gameplay state. |
| `Ping` | Returns a nonce and current host tick. It changes no gameplay state. |

`Move` and `Drive` share one increasing connection-local sequence. A stale
movement message cannot overtake a later pedal or stop message. Pedal intent is
bound to the active vehicle ID, and successful control transitions clear it.
After one second without fresh input, the authority applies braking rather than
continuing the last throttle indefinitely. Vehicle licensing, condition, fuel,
parking brake and the present driver's needs constrain admitted movement.

The reference runs physical simulation at 20 Hz and actor snapshots at 10 Hz.
Vehicle dynamics use physical seconds, independent of accelerated calendar time.
World pause stops movement and transport clocks. The shared reference handling
model and sampled vehicle envelope do not provide rigid-body multiplayer physics,
whole-world rollback or lag-compensated racing. The native owner display may
replay at most 250 ms of locally queued input, with terrain, source-clearance and
body-contact admission; stale or missing coverage stops that preview. See
[Transport](TRANSPORT.md) for gameplay and model limits.

## Streams and lifecycle

`ActorUpdate` and `SandboxUpdate` are independent streams. Each contains a
positive connection-local `revision`, `base_revision`, and simulation `tick`.
Delta revisions immediately follow their named base. A keyframe has no base and
may replace an older baseline at a higher revision. Repeated/stale revisions and
backwards ticks reject. Multiple command results may legitimately share a tick.

| Event | Host behavior | Client behavior |
|---|---|---|
| New or resumed connection | Empty caches; full recipient views | Bind welcomed player identity; install validated keyframes |
| Normal replication | Construct current bounded interest view; choose smaller delta/full frame | Atomically reconstruct; return normalized complete view to the game |
| No actor changes | Empty actor delta with current tick/time/counts | Retain entities, advance authoritative time |
| Entity leaves interest or is removed | Remove its replica ID; send complete order when membership changes | Remove it locally without inventing a world deletion command |
| Recipient switches active body | Build from that body's authoritative position and protected owned IDs | Apply the new complete interest state after reconstruction |
| Base revision missing | Await bounded client resync request or periodic refresh | Keep prior state; request fresh baselines once |
| Resync | At most once per real second; reset both caches | Wait for both keyframes; a rate rejection schedules one retry after a one-second client cooldown |
| Optional view backpressure | Defer construction before advancing its revision; retain one pending sandbox refresh flag | Drain the existing ordered revision chain, then receive the latest current view |
| Reliable response backpressure | Retain the bounded decoded request batch until a maximum-size response fits | Await the original command result; do not repeat the mutation with a new sequence |
| Ten fixed-step seconds since keyframe | Full refresh even when a delta would be smaller | Replace baseline using the new revision |
| Malformed delta or oversized result | Trusted host should never create it | Reject before baseline mutation; host application disconnects on protocol failure |
| Reconnect/server restart | New transient revision chains; saved principal/command sequence retained | New decoder baselines, existing private session identity |

TCP ordering means a delta can refer to the last successfully queued view without
an additional acknowledgement round trip. Each peer retains at most 512 KiB of
application output. Before constructing an optional view, the host reserves room
for one maximum-size frame plus 64 KiB of reliable control traffic. If that room
is absent, it defers construction and leaves the peer's baseline unchanged. A
single sandbox-refresh flag coalesces skipped views; once room returns, the host
constructs a current recipient-private view. Already queued frames retain their
original order. No delta can name an unsent baseline.

Before consuming each retained request, the host requires room for one maximum
256 KiB response plus its four-byte prefix. Commands stay in the existing bounded
32-message inbox until this condition holds. Accepted mutations therefore do not
lose their reliable result merely because periodic views filled the queue. Each
peer keeps its own read, write and message budgets, so a stalled receiver does
not consume another peer's allowance. This is bounded service fairness, not a
CPU or real-time deadline guarantee. The existing message rates, idle timeout,
malformed-frame rejection and ultimate queue cap still apply; a permanently
stalled or abusive peer is not kept alive indefinitely.

Resync state is installed only after the request is successfully queued. A
host `resync_rate` response retains the recovery need and schedules one retry
after a one-second client cooldown; both streams must receive valid keyframes
to complete recovery. Periodic valid keyframes can satisfy that need during the
cooldown. Recovery does not bypass view validation or disclose another principal's
private records. These changes do not remove TCP head-of-line blocking under
Internet packet loss.

## Entity patches and bounds

A `CollectionDelta<K,T>` carries typed `upserts`, `removed` IDs, and an optional
`order`. Membership changes require a complete order naming each result exactly
once. Duplicate IDs, conflicting upsert/removal IDs, absent removal IDs, invalid
orders and over-budget result counts reject. Updates to existing entities can
omit the order. The decoder preserves the exact order of the host's view.

Sandbox patches cover factions, plots, structures, resource nodes, crops, living
units, shops, production sites, deliveries, roads, wildlife metadata, habitats,
natural-resource metadata, territories and supply contracts. Transport adds
private owned-vehicle records, public vehicle appearances, facilities, jobs,
trailers and courses. Player state is sent when it changes; small headers carry
clocks, active-account fields, metrics, progression, the recipient's current race,
personal bests, bounded visible character profiles and truncation flags. Nullable
fields can be explicitly cleared. Stock, money and cargo are authoritative
values, never client-applied increments that could be replayed twice.

Reconstruction checks collection limits, the recipient player, domain living/
ecological/settlement/transport validation, matching wildlife bodies and resource
nodes, and the 240 KiB complete-view budget. Wire frames remain limited to 256 KiB.
The larger of old and new interest sets cannot accumulate forever through small
patches. A delta that encodes larger than the full keyframe falls back to full.

Transport replica limits are twelve private owned vehicles, 64 public vehicle
appearances, 24 facilities, 32 jobs, 32 trailers, eight courses, eight personal
bests and 64 visible character profiles. These are upper bounds, further
constrained by visible living bodies, references and the complete-view byte
budget. They do not increase the authoritative world's population caps.

## Public appearance and private records

Each connection's baseline contains only the bounded view eligible for that
recipient. The visible representation of another player's vehicle is separate
from its full owned record:

| Data | Recipient visibility |
|---|---|
| Vehicle model, paint, lights, indicators, steering and attached-trailer reference | Public appearance when the corresponding owned living body is visible. |
| Full vehicle record, including fuel, condition, cargo and handling state | Only the owning player, while the vehicle body is visible. |
| Driver progress, licenses and current race | Only the recipient. The current race also requires its owned vehicle in the view. |
| Personal bests | Only the recipient's results. |
| Character appearance profile | Public for owners of visible bodies; also the recipient's own profile. |
| Facility and course descriptions | Bounded public scenario records. |
| Job offers | Unassigned offers are public. An assignment is sent only to its owner with its vehicle visible. |
| Trailer | The recipient's own detached trailers, and trailers attached to visible vehicles. Other detached trailers are omitted. |
| Issuer treasury, other players' wallets, private vehicle cargo and job assignments | Omitted from the recipient's view. |

The decoder verifies each appearance against a visible body and its owner. An
owned full record must agree with its public appearance. Trailer attachments
must agree with visible vehicle references. Another player's progress or assigned
job cannot enter a baseline through a delta. Removing a body from interest also
removes dependent appearances and assignments; it does not destroy their saved
records.

Faction rosters follow the same bounded visibility policy in full snapshots and
deltas: the recipient's memberships and invitations come first, followed by
factions that own plots in the visible view, up to eight rosters. A departed
member or nearby outsider can therefore still identify the community owning
visible land. This public roster does not grant membership or expose private
inventories, wallets or resume capabilities. When land leaves interest, an
otherwise unrelated roster must leave with it. The two-principal
[TLS lifecycle regression](../crates/terra-net/tests/faction_lifecycle_tls.rs)
checks departure, shared-property permissions, private snapshots and exact
identity/command continuation after a host restart.

Client reconstruction keeps public vehicle appearance in a serialization-skipped
cache. Authoritative domain validation rejects that cache in a world save, so a
network presentation state cannot silently pass the authority save APIs as a
complete world. Resume capabilities are separate private credentials; they never
appear in replication deltas or telemetry.

## Interest and scheduling

The server builds one immutable `GeoSpatialIndex` for principal actor records per
actor fanout, and one `SandboxInterestIndex` per sandbox fanout. It reuses those
indices across clients. Exact great-circle filtering follows conservative spatial
candidates and retains date-line/pole behavior. The owned principal is always
included. Selection first partitions the bounded nearest prefix, then sorts that
prefix by owner priority, distance and ID. Dense-interest candidate work still
exists; this is not a distributed spatial database.

Sandbox interest follows the active authoritative body, not spectator camera
focus. The indexed API borrows its source world, preventing mutation while the
index is used. Direct command responses still construct a current view on demand.
Transient ecology scheduling reduces distant decision frequency without skipping
physical movement, needs, production, contract deadlines or currency updates.

## Regional persistence and durable outcomes

When a regional store is attached, a new principal or gameplay command may need
the complete saved world for ownership, overlap and resource checks. The host
submits hydration to a bounded storage worker, retains the received batch, and
keeps the current visible state until that job completes. It applies the batch
only after hydration, then submits one checkpoint and withholds success responses
until the complete batch commits. An authority does not advance dormant records
while this complete mutation view is installed.

Normal updates poll instead of waiting on SQLite. One outstanding job provides
backpressure; each peer's retained inbox is bounded to 32 messages. Fixed
simulation pauses during storage work and does not catch up that elapsed wall
time afterward. This preserves a serial authority with responsive application
loops; it does not promise uninterrupted simulation during a checkpoint.

`Move`, `Drive`, `Focus` and `Ping` alone do not hydrate the complete world or
request a checkpoint for each message. Continuous driving uses the retained
active bodies and ordinary timed/source checkpoints. A snapshot of passive
simulation can therefore be newer than the last durable generation. Explicit
gameplay success and new-identity admission retain the durable batch barrier.

A failed checkpoint leaves the last committed generation and visible working
state available; it does not flush queued success. The reference CLI stops on a
storage error. An embedding host may recover a transient condition and retry the
retained batch. Checksum, source identity and stale-writer failures require
validated recovery, not blind retries. Explicit shutdown drains existing work
and checkpoints the current state in order. See
[Regional persistence](REGIONAL_PERSISTENCE.md) and [Operations](OPERATIONS.md)
for the actual recovery, backup and migration contract.

## Verified TLS and deployment policy

The optional `terra-net/tls` feature uses rustls with normal CA and hostname
verification. It is enabled by the reference server/client defaults and by the
native demo through `terra-bevy/tls`. The embedding APIs are `Server::bind_tls`
and `NetClient::connect_tls`; Bevy exposes `NetworkSecurity` and the
`TerraNetworkPlugin::with_tls` builder.

| Endpoint | Required configuration |
|---|---|
| Reference TLS server | `--tls-cert CHAIN.pem --tls-key PRIVATE.pem` |
| Reference CLI or native TLS client | `--tls-ca CA.pem --tls-server-name HOSTNAME_OR_IP_SAN` |
| Non-loopback plaintext reference connection or listener | Explicit `--allow-insecure-remote`; the native launcher exposes the same choice. |
| Loopback development | Plaintext remains the default when TLS is not configured. |

The client holds its application Hello and resume capability until certificate
validation and exact `terra-world/11` ALPN negotiation finish. TLS early data is
disabled. A wrong CA, hostname, protocol or malformed handshake fails without a
plaintext fallback. `NetClient::is_encrypted()` and Bevy's
`NetworkStatus.encrypted` report a completed verified handshake, not merely a
checked settings box.

Configuration bounds each PEM input to 1 MiB and each certificate list to 32
entries. Socket and TLS processing are nonblocking, with 64 KiB I/O work budgets
per call and the existing frame/output caps. Queued application bytes remain
charged to the output budget while their TLS records drain. Native certificate
loading and TCP setup run on the connection worker; existing handshake and idle
deadlines still apply.

TLS protects the server connection and bearer capability in transit. Session
files and complete server backups still contain private credentials. Account
enrollment, password recovery, moderation, distributed authorization and
production Internet deployment remain game/operator responsibilities. Public
test certificates and keys under `tests/fixtures/tls` must never serve a real
world. See [Operations](OPERATIONS.md#tls-transport-and-player-capabilities) for
complete commands and private-file handling.

## Metrics and integrations

`NetClient::poll()` consumes the new wire variants and returns the familiar
`ServerMessage::Snapshot` and `SandboxSnapshot` forms. Games can keep their
existing snapshot validation/rendering path. They should reject raw wire update
variants if an application accidentally bypasses the decoder.

- `replication_stats()`: keyframes, deltas, upserts, removals, resync requests,
  encoded replication bytes, and equivalent complete-snapshot bytes.
- `transport_stats()`: actual socket bytes sent/received, queued frame count, and
  current/peak application pending bytes. TLS retains plaintext against the
  budget until its encrypted records drain; OS socket buffers are outside it.
- `resync_pending()`: whether recovery still awaits its two keyframes, including
  a rate-rejected request awaiting its bounded retry.
- `ServerReport.replication`: per-update payload counters, including immediate
  command/handshake view construction and `backpressure_deferrals` for optional
  views skipped before baseline construction.
- `ServerReport.deferred_command_batches`: peers whose retained reliable input
  cannot yet fit a maximum-size response.
- `ServerReport.max_peer_pending_bytes` / `peak_peer_pending_bytes`: current
  maximum and observed connection high-water marks for retained application bytes.
- `ServerReport.interest_candidates_tested`: principal index candidates tested
  during AOI queries; it excludes distance recomputation for nearest-first order.
- `Server::ecology_schedule_stats()`: evaluated/deferred ecology decision work.
- `ServerReport.persistence_pending`: a worker request is outstanding; the update
  returned without waiting for it.
- `ServerReport.storage_work_ms`: duration of the most recently completed storage
  job, reported separately from the update's main-thread work.

Payload reduction compares the generated replication frames with the complete
`Snapshot`/`SandboxSnapshot` messages they replace, including the four-byte frame
prefix. It excludes Welcome, command results, movement intents and pings. Socket
counters include all actual traffic and TLS overhead when encryption is enabled.
Initial/keyframe overhead can produce a negative reduction in a very short or
tiny workload. JSON serialization and
baseline comparison add CPU work; reduced bytes are not automatically faster
simulation. Measure both.

## Current correctness evidence

The current socket regression suites exercise the real host and decoder:

```sh
cargo test -p terra-net --locked \
  --test driving --test regional_persistence --test tls_transport
```

- [Driving tests](../crates/terra-net/tests/driving.rs) cover pedal acceleration,
  steering, coasting, fuel, stale-input braking, shared movement replay guards,
  possession ownership, public vehicle/trailer deltas, private financial data,
  and a regional restart with a funded job.
- [Regional tests](../crates/terra-net/tests/regional_persistence.rs) cover
  command success withheld behind a real SQLite write lock, retained recovery,
  source/dormant invariants, stale writers, exact identities, safe conversion,
  and continuous driving/focus/ping without a per-input storage barrier.
- [TLS tests](../crates/terra-net/tests/tls_transport.rs) cover trusted command
  and resume sessions, untrusted CA and hostname rejection before admission,
  a plaintext peer rejected by a TLS listener while other peers continue, and
  invalid or oversized configuration.

These establish bounded correctness cases on loopback. They do not measure
Internet packet loss, production certificate operations, GPU frame time,
regional handoff, or large concurrent populations. The TLS test suite requires
the `tls` feature, enabled by the default reference configuration.

## Reproduce delayed, fragmented and stalled connections

```sh
cargo test -p terra-net --locked --test network_conditions --test operator_controls --test delta_replication
cargo run -p terra-net --example network_conditions --locked -- tcp
cargo run -p terra-net --example network_conditions --locked -- tls
```

The shared fixture opens two actual localhost clients. One uses an opaque bounded
byte relay: 25 ms one-way scheduled delay, deterministic 0–15 ms ordered jitter,
write fragments of at most 997 bytes, and 128 KiB of buffering per direction. The
other connects directly. The relay forwards TCP/TLS bytes unchanged; its write
fragments are application writes, not simulated Internet packets. Pausing its
downstream eventually fills actual socket buffers and backpressures host output.

The fixture requires optional-view deferral to occur, retains the stalled owner,
serves the healthy peer's pings, issues a reliable craft while stalled, then drains
the queue. It compares the reconstructed private view with current authority and
both actor replicas by ID, checks exact finite inventory and private company
filtering, disconnects, saves and restarts the host, resumes the same capability,
and rejects the already-accepted command sequence. The relay never decodes or
prints capabilities. Temporary private worlds are deleted on completion.

Output includes package/protocol, condition bounds, actual byte/fragment counters,
queue high-water marks, snapshot/request deferrals, healthy pongs and explicit
correctness checks. The 512 principal actor records include 510 simple kinematic
stress records; they are not 512 independently simulated living inhabitants.
The separate reliable-admission regression preloads valid bounded control frames,
proves that a craft waits without mutation when its response cannot fit, and then
executes exactly once after drain. Operator regressions exercise closed-guest
provisioning, capacity and endpoint rejection, restart continuity, and an actual
SQLite write lock without changing IDs, capabilities or cold residency.

This finite acceptance fixture does not model packet reordering/loss, UDP,
congestion-control choices, public identity services, long-lived sessions or
MMORPG capacity. Wall-clock duration is diagnostic and depends on scheduling;
compare the invariants and limits before interpreting timing.

## Reproduce a finite loopback workload

```sh
cargo run -p terra-net --example replication_load --locked -- \
  --clients 16 --actors 4096 --ticks 200 \
  --output snapshots/replication-load.json
```

The example opens actual loopback TCP connections to the real library host. It
places generic kinematic actor records into four geographic regions and also
runs the bounded living settlement. The first measured phase moves 25% of generic
records. The second stops those records while the living settlement continues.
Separate phase counters expose dynamic versus stable replication behavior. Clients choose
regional interest and continue receiving their owned principal. The finite test
requires all clients to handshake, receive snapshots, and remain connected with
zero domain errors. It reports actual socket traffic, full-view byte equivalents,
actor view counts, AOI candidates, ecology jobs and measured host-update times.

Arguments are bounded to 1–32 clients, 32–20,000 generic actor records and
40–2,000 measured ticks, plus twenty warmup ticks. Simulation advances at a fixed
20 Hz logical cadence; the harness is not required to wait for wall-clock time.
Host-update timing excludes an explicit one-millisecond socket scheduling pause
and excludes client decoding. The reported wall time includes both. No GPU,
public network, encryption, persistence service, regional handoff or distributed
simulation is exercised. Generic actor records are substantially simpler than
full living actors and must not be reported as thousands of fully simulated NPCs.

## Archived v0.4 performance run

The [complete JSON result](network-load-v0.4.json) records the finite run on
2026-10-27 using the v0.4 implementation. Running the command above against the
current source produces a new result; it does not reproduce the older build's
measurements by definition. The archived run did not include protocol 6 driving,
transport records, verified TLS or the current storage worker.

All sixteen clients completed their handshakes, received 110 actor
views each, and stayed connected. There were zero disconnects, domain errors or
resync requests. The host had 4,096 generic kinematic records plus sixteen
principal records, and a separate living domain containing 140 bodies. These
counts describe different representations and must not be added together as
independent full NPCs. Each principal view was capped at 128 actor records.

| Phase | Encoded replication bytes | Equivalent full-view bytes | Payload reduction |
|---|---:|---:|---:|
| 100 measured ticks; 25% of generic records moving | 14,128,273 | 35,109,141 | 59.76% |
| 100 measured ticks; generic records stationary, living domain active | 11,173,320 | 35,430,499 | 68.46% |
| Whole run including 20 warmup ticks | 28,919,752 | 77,864,588 | 62.86% |

The second phase includes periodic full refreshes. The moving records occupy
one of four regions; four of sixteen clients view that moving region, while all
clients retain their own principal and run the living settlement. The run sent
64 keyframes and 2,592 deltas. Total actual client socket traffic received was
28,936,090 bytes, including handshakes and other control messages. This is a
measured byte comparison against equivalent complete views of the same run,
not a CPU comparison with a separately executed older host.

The 200 measured host updates averaged **14.82 ms**, with **37.32 ms p95**,
**39.42 ms p99**, and **40.34 ms maximum**. Wall time was 4.78 seconds for eleven
logical seconds, including warmup, client decoding and scheduling pauses. The
build used Rust 1.99.1 and the workspace's optimized development profile
(`opt-level = 1`, debug assertions enabled), on Linux x86-64 with eight visible
logical CPUs. The reported processor model was AMD EPYC 9V74. Other workspace
network/simulation Cargo builds were paused for this measurement; the container
is not a controlled dedicated performance machine. These results establish a
bounded regression workload and observed payload reduction, not production
capacity or an MMORPG concurrency guarantee.

## SDK handshake and wire/event admission

`NetClient::poll` validates application Welcome fields through the shared
`ServerMessage::validate_welcome` before replica binding. It retains any requested
source manifest and requires `matches_data`; a host save name is not a source
identity. This checks protocol/identities, physical spawn bounds, finite clock
and time scale, tick-rate limits, resume capability shape and sequence headroom.
Native direct message admission uses the same validator before saving identity.
Game-specific catalog and trust policy remain with the embedding game.

`ActorUpdate` and `SandboxUpdate` are remote revisioned messages. `Snapshot` and
`SandboxSnapshot` remain the public normalized output API, but direct receipt of
those variants from a socket is now rejected; they cannot bypass reconstruction
and recipient validation. Wire protocol remains 10: the reference server already
emits revisioned updates. Error text returned for invalid handshakes does not
include bearer tokens. Protocol admission does not authenticate an untrusted
server or establish public-service account/moderation/WAN readiness.
