# Terra World 0.4 validation record

Validation session: **2026-10-27**. Source release **0.4.0**, wire protocol **3**,
Rust **1.99.1**, Bevy **0.20.0**, Linux x86-64. The final automated gates contain
**258 distinct passing Rust tests**, plus six successful process-test groups,
two successful finite headless examples and a successful final native build.
Repeated runs and shared feature variants are counted once.

The probes below use the workspace development profile (`opt-level = 1`, debug
assertions enabled). They are finite, reproducible workloads with explicit
representation and timing boundaries. They do not certify production server
capacity, gaming-GPU performance or MMORPG concurrency. Native confirmation
belongs to the separate evidence section below.

## Final automated gates

| Suite | Distinct passing tests | Coverage |
|---|---:|---|
| Core | 12 | Geographic round trips, poles/date line, clocks, solar/seasons, actor and environment contracts |
| History | 13 | Dated packs, missing coverage, provenance, path/file bounds, political geometry and bundled scenarios |
| CLI | 4 | Argument validation, deterministic simulation, complete sandbox workflow and exact saved coordinates |
| Stream | 20 | 13 unit tests, six semantic/provider fixtures and one actual local HTTP/cache integration |
| Sandbox | 90 | Seven faction, 13 gameplay, 20 living, 14 ecology, 18 settlement, five logistics and 13 scaling regressions |
| Network library / host / TCP | 54 | 25 library, one server-binary, five delta TCP, six frontier and 17 established loopback tests |
| Bevy library with rendering | 40 | Includes the 21 shared headless library tests, plus terrain/material/quality/scene and other rendering regressions |
| Bevy integration | 11 | Six headless engine tests, full living/frontier/procedural flows and two real framed native-replica fixtures |
| Native demo | 14 | Control/possession, panel behavior, road grounding, gathering/contribution UI and telemetry export lifecycle |
| **Total** | **258** | **Final gates passed; shared tests and reruns counted once** |

The compatibility subtotal is 49 (core, history, CLI and stream). The no-render
Bevy invocation passed 32 tests: 21 library and 11 integration. Its 21 library
tests are already included in the 40-test rendering-feature row; they are not an
additional 21 release tests. Zero-test library/doc-test harnesses, executable
examples, process groups and benchmark iterations are not added to the Rust-test
total.

The final `terra-demo` native build succeeded after the final library and demo
test gates. Socket fixtures ran in an environment permitting actual local TCP;
blocked or unexecuted socket attempts are not counted as passes.

### Geographic work and simulation correctness

The thirteen scaling regressions exercise exact scan/index parity at the date
line and poles, altitude-independent interest, bad-input rejection, bounded wide
queries, candidate reduction and complete snapshot parity. They also check
private recipient state, distant owned bodies, index/world mismatch and rebuild
after removal.

Scheduling tests cover far-budget fairness, approaching observer/vehicle
preemption, explicit observer authority, full-rate needs/resources/production and
nonanimal movement, unknown-ground rejection, atomic invalid inputs, rewind/reset,
traffic yielding at 512 vehicles and backward defaults for added metrics. These
checks constrain the optimization to decision/query work. Movement is not
accumulated into a deferred jump, and a deferred animal decision does not suspend
the economy or replenish depleted stock.

The existing ecology, settlement and logistics suites remain part of the gate.
They cover grounded wildlife and recovery, persistent depletion, custom equipment
catalogs, complete neighborhood-to-village-to-city progression, faction access,
funded work, private journals, deadlines, treasury/item conservation, cargo
reservation, blocked pickup cancellation and physical cargo return.

### Replication, privacy and authority

Protocol 3 tests cover ordered reconstruction, keyframe/delta equivalence,
identity/reference validation, entity removal, view order, private data,
malformed updates, revision gaps and bounded resync. The cumulative byte-budget
regression rejects a chain of individually small updates that would grow the
reconstructed sandbox view beyond 240 KiB. Collection limits and the 256 KiB wire
frame ceiling remain enforced.

Five real TCP delta fixtures exercise the transport/decoder path, including an
actual wire revision gap and recovery. The 17 established loopback and six
frontier fixtures retain command replay rejection, save/session behavior,
source-manifest compatibility, catalog migration/admission and loaded-surface
requirements. The server-binary fixture retains spawn residency requirements.
Old wire protocols reject before allocating a new player/session identity.

`NetClient` reconstructs complete views before the Bevy adapter receives them.
Two actual framed replica fixtures check valid owned wildlife and atomic
rejection of malformed animal/resource references or another player's private
progress. Forty client frames leave host ecology, living state and settlements
unchanged, with zero ecology scheduler passes and zero authoritative ecology
CPU-stage samples.

### Complete engine scenarios

`frontier_world.rs` runs actual schedules from streaming and seeding through
wildlife behavior, gathering, territory founding, supplies, funded contract
fulfillment and exact save/restore. It checks actual inventory, depleted stock,
receiving-site goods, money and progression. The v0.4 extension checks populated
timing windows, explicit scheduler/copied-diagnostics reset on a same-clock load,
and preservation of scheduler progress and player identity across an actual
floating-origin rebase.

`living_world.rs` retains buy, possession of another owned body, direct movement,
release, RTS orders, production/delivery and exact restore in one app. It checks
the current authoritative quote and exact debit rather than assuming a fixed
market price.

`procedural_settlement.rs` uses the actual procedural provider, source
roads/buildings/land use, resident terrain and the physical motion resolver over
3,600 fixed updates. Its v0.4 production/delivery assertions passed. This record
does not reuse the earlier release's final population, production-cycle or
transaction counts as fresh v0.4 observations.

### Graphics and product regressions

The rendering gate includes crack-free terrain interior reduction while keeping
source heights and every shared border sample, finite geometry, date-line
material continuity, bounded mip chains/normal encoding, render-quality costs,
capability fallback and resource/ground-cover/territory geometry constraints.
The game-supplied scene fixture uses Bevy's actual scene-ready lifecycle to
replace a procedural body mesh and restore that same body's fallback on removal.
This checks the asset integration contract; it is not evidence of a bundled
production character or animation library.

The deterministic default 49-chunk source-geometry fixture records **1,620,763
vertices**, with six near, twenty middle and twenty-three far chunks. Its terrain
meshes use **87,121 vertices** against **207,025 original height samples**, a
57.9% render-only reduction. Physical terrain queries retain the original grids.
The under-two-million regression excludes separate actors, player overlays and
nearby detail; it is a CPU mesh-count check, not a GPU frame-rate measurement.
See [GRAPHICS_AND_MAPS.md](GRAPHICS_AND_MAPS.md) for the geometry/profile scope.

Demo tests preserve bounded hit testing, viewport/UI exclusion, possession
identity, movement yaw/speed, chase distance, road grounding, reachable gathering
and exact remaining public-works quantities. The new panel tests cover automatic
receding on possession while respecting deliberate context opening, text entry
and the user's keep-panels preference. Two final telemetry regressions check a
normal window exit exporting in that same final update and a failed requested
export overriding an otherwise successful exit. Export ordering now follows
Bevy's exit systems in `Last`.

## Finite simulation and index probe

The complete result is [scaling-probe-v0.4.json](scaling-probe-v0.4.json), with API
contracts and reproduction detail in [SCALING.md](SCALING.md). Three workloads
are intentionally separate:

| Workload | Recorded result | Scope |
|---|---|---|
| 100,000 indexed point records; 64 queries of radius 250 m | 5,720 candidates versus 6,400,000 linear distance checks; identical results | Index representation/query work only; no full living population |
| Supported snapshot batch: 512 living bodies, 10,000 resources, 64 registered recipient views | Byte-identical output; 37,145 µs scanning versus 5,906 µs index build + 5,542 µs indexed snapshots | Privacy and view-byte trimming included; zero network connections |
| 1,000 fixed domain steps; 50 physical seconds and 3,000 calendar seconds | 65 observed bodies, 22 managed animals, 223 production cycles and one physical delivery | Permissive finite-coordinate motion resolver; imported collision world excluded |

For the point-index workload, building took 34,860 µs, indexed queries 3,804 µs
and linear queries 270,878 µs. The snapshot batch's indexed total including
construction was **11,448 µs**, approximately 3.24 times faster than its scan path
in this one execution. Building an index for a single small view can still cost
more than scanning.

The domain workload admitted **550 behavior evaluations** against **1,100** for
full-pass evaluation over the same 50 behavior passes. It recorded 148 indexed
threat candidates, 13,723 traffic queries and 23,224 traffic candidates. Threat
sensing, movement, needs and economic time continue; half as many admitted
behavior decisions does not mean half the total simulation CPU. The complete
finite domain run took 27,789 µs under the probe's stated permissive environment.
These timings are shared-container observations, not hardware-independent limits.

## Finite TCP replication probe

The complete result is [network-load-v0.4.json](network-load-v0.4.json). The
[replication guide](NETWORK_REPLICATION.md) explains revision chains, resync,
private baselines, byte accounting and the workload.

The run connected **sixteen actual local TCP clients** to one library host with
**4,096 generic kinematic actor records**, sixteen principal records and a
separate **140-body living domain**. These are different representations and
must not be summed as independent full NPCs. Each client received 110 actor
views and stayed connected. There were **zero disconnects, domain errors or
resync requests**.

The harness advanced twenty warmup ticks and 200 measured ticks at a logical
20 Hz. Four geographic regions partitioned the generic records. During the first
100 measured ticks, 25% of generic records moved; during the second 100, those
records stopped while the living domain continued.

| Phase | Encoded replication bytes | Equivalent complete-view bytes | Payload reduction |
|---|---:|---:|---:|
| Moving generic-record phase | 14,128,273 | 35,109,141 | 59.76% |
| Stationary generic-record phase; living world active | 11,173,320 | 35,430,499 | 68.46% |
| Whole run, including warmup | 28,919,752 | 77,864,588 | 62.86% |

The run sent 64 keyframes and 2,592 deltas. Actual client socket bytes received
were **28,936,090**, including handshake/control traffic excluded from the
replication-payload comparison. Periodic full refreshes are included in the
stationary phase.

Measured host updates averaged **14.82 ms**, with **37.32 ms p95**, **39.42 ms
p99** and **40.34 ms maximum**. Wall time was **4.78 seconds** for eleven logical
seconds including warmup. Host-update timing excludes the harness's explicit
one-millisecond socket scheduling pause and client decoding; wall time includes
both. The environment reported eight visible logical CPUs on Linux x86-64.
Other network/simulation Cargo builds were paused for this run, but the container
was not a controlled dedicated performance machine.

This exercises local TCP, bounded interest views and simple actor records. It
does not exercise public-network latency/loss, TLS, GPU rendering, durable
services, regional handoff or distributed simulation. The payload reduction is
against equivalent complete views from the same run, not a CPU comparison with a
separately executed older server. Per-view caps and the 512-body living limit
remain in place.

## Independent executable checks

Fresh v0.4 server/client binaries passed all six real-process groups:

| Group | Result |
|---|---|
| Two simultaneous procedural-world clients | Passed |
| Save/restart preserving both identities and command sequences | Passed |
| Historical pack with matching political data | Passed |
| Mismatched epoch, pack and politics revisions | Rejected as required |
| Invalid historical startup and resume configuration | Rejected as required |
| FIFO/special-file startup inputs | Rejected without blocking |

The script launches actual server/client processes with local sockets and
bounded completion time. It uses local data and does not download public world
data. Both finite `terra-bevy --no-default-features` examples also passed:
`headless` loaded one synthetic chunk and reported its terrain height;
`custom_provider` loaded one chunk from the installed provider.

## Native interaction and visual review

Native review used Linux/X11, a 1440 × 900 reference window and the 1100 × 700
minimum, with Mesa llvmpipe software Vulkan. Actual input exercised the quality
cycle, first/third-person possession, automatic panel hiding, contextual E
reopening, RTS restoration, F5 save, F12 diagnostics and minimum-window scrolling.
The saved local envelope was inspected separately from screenshots. The full
sanitized measurements are in [native-v4-verification.json](native-v4-verification.json).

The first Balanced run exposed unsupported 2× multisampling in the software
adapter's HDR/depth formats. Balanced/High now use 4× and Performance uses none;
the subsequent native runs reported no rendering validation errors. A normal
window close then exposed the unordered telemetry exporter. It now runs after
Bevy's `ExitSystems` in `OnAppExitSystems`; actual normal closes produced JSON
reports and successful process exits, and the two automated lifecycle
regressions cover successful export and failure propagation.

| Native workload | Observed integrated state at graceful exit |
|---|---|
| Bundled heightmap, High profile | 49 chunks, 161,393 terrain/source vertices against 207,025 full samples, 107 bodies and 64 animals; atmosphere active; zero pending builds |
| Large procedural region, Performance profile | 121 chunks, 2,050,554 total source vertices; 138,681 terrain vertices against 511,225 full samples; near/middle/far 6/19/96; 107 bodies and 64 animals; atmosphere disabled; zero pending builds |
| Actual protocol-3 graphical client, Balanced profile | Nine source chunks, 64 received bodies and 27 received animals; 26 keyframes and 1,787 deltas; zero requested resyncs; zero pending builds |

The graphical client received 30,910,022 socket bytes. Its actor/sandbox payload
was 30,908,323 bytes against 52,146,856 full-view-equivalent bytes, a 40.73%
reduction for this separate interactive run. This is not the 16-client harness's
62.86% result. Its CPU report contains streaming only, consistent with the host
owning ecology, movement, needs, production and contracts. All three workload
reports have no last-world or last-simulation error. Live counters in a screenshot
can precede the final exported totals and can change when the camera changes tiers.

The [third-person heightmap view](screenshots/v4-third-person.png) shows terrain
relief, the inhabited settlement, wildlife and the compact needs/wallet footer.
The [heightmap overview](screenshots/v4-heightmap-landscape.png) is an elevated
inspection view of finite coverage, not an infinite scenic backdrop. The
[121-chunk view](screenshots/v4-large-region.png) confirms fully integrated
regional coverage; [diagnostics](screenshots/v4-diagnostics.png) expose the
actual local work. The [minimum window](screenshots/v4-minimum-window.png)
confirms scroll access to the final diagnostic line. The
[native multiplayer view](screenshots/v4-multiplayer-diagnostics.png) shows
connected host authority, protocol 3 deltas and baseline synchronization.

These are visual/interaction confirmations with software rendering. Their frame
intervals do not establish production GPU performance, and the different quality
profiles and workloads are not a controlled frame-rate comparison. The models,
road layout and terrain fixtures remain authored reference content. Screenshot
review still identifies crowd overlap, reference art and limited accessibility
as production work; see [PRODUCT_REVIEW.md](PRODUCT_REVIEW.md).

## Source-data scope

The automated provider gates use local semantic fixtures and a real local HTTP/
cache pipeline. This record makes no new claim of a successful public Overpass
download. Earlier strict public elevation/weather evidence and its explicit
Overpass timeout remain in the provider verification files and
[VALIDATION_v0.2.md](VALIDATION_v0.2.md); they are not relabeled as new v0.4 fetches.

Default terrain, habitats, wildlife, resources, production and economy are
authored simulation content. The bundled raster is synthetic, historical example
packs are fictional dated scenarios, and the Natural Earth snapshot is current
generalized political geography. Imported heights and vector semantics do not
establish observed populations, yields, workers, inventory, prices or historical
social conditions. Provider provenance and synthetic/fallback labels remain
part of the world data.

The default source neighborhood is **49 requested chunks**. The optional
[large-region preset](../configs/large-region.json) requests **121**, with 144
resident and queued ceilings. Neither is a full-planet resident world, an
increase to the 512 living-body limit, or a production concurrency result.

## Reproduce the automated and finite gates

Run from the workspace root with the pinned toolchain and required native build
dependencies. Recorded component runs used `--locked --offline` with dependencies
already cached. Omit `--offline` for an initial dependency fetch when necessary.
Local TCP fixtures require permission to bind and connect loopback sockets.

```sh
cargo test -p terra-core -p terra-history -p terra-cli -p terra-stream --locked --offline
cargo test -p terra-sandbox --locked --offline
cargo test -p terra-net --locked --offline
cargo test -p terra-bevy --no-default-features --locked --offline
cargo test -p terra-bevy --lib --locked --offline
cargo test -p terra-demo --locked --offline
cargo build -p terra-demo --locked --offline
cargo run -p terra-bevy --no-default-features --example headless --locked --offline
cargo run -p terra-bevy --no-default-features --example custom_provider --locked --offline
```

The convenience recipe `./scripts/verify.sh` groups the workspace, headless,
render-library, demo and example checks and also checks formatting. The release
count above is assembled from the final component gates, with feature overlap
removed.

Build and run the independent process groups:

```sh
cargo build -p terra-net --bins --example replication_load --locked --offline
python3 crates/terra-net/tests/process_smoke.py
```

The process script defaults to `target/debug/terra-server` and
`target/debug/terra-client`. If using a custom `CARGO_TARGET_DIR`, pass its
corresponding binaries through `--server` and `--client`.

Reproduce the probes without overwriting the recorded release evidence:

```sh
mkdir -p snapshots
cargo run -p terra-sandbox --example scaling_probe --locked --offline > snapshots/scaling-probe.json
cargo run -p terra-net --example replication_load --locked --offline -- \
  --clients 16 --actors 4096 --ticks 200 --output snapshots/network-load.json
```

Keep the workload and build profile with every comparison; timings from a
release-optimized build or different host are a new measurement. Native timing
capture and interpretation are documented in [PERFORMANCE.md](PERFORMANCE.md).
Source archives exclude private resume capabilities, full personal host saves,
build caches and temporary runtime files.

Earlier release-specific evidence remains in
[VALIDATION_v0.3.md](VALIDATION_v0.3.md),
[VALIDATION_v0.2.md](VALIDATION_v0.2.md) and
[VALIDATION_v0.1.md](VALIDATION_v0.1.md).
