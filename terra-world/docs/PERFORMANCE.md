# Measuring a regional world

Terra makes presentation cost, geographic work, replication traffic and regional
checkpoint latency inspectable. Measurements in this repository are finite fixture evidence. They
describe their actual actors, view limits, build profile and environment; a
storage ceiling or index entry count is not concurrent-player capacity.

## Native quality and diagnostics

```sh
cargo run -p terra-demo --locked -- \
  --quality balanced --telemetry reports/local-session.json

cargo run -p terra-demo --locked -- \
  --config configs/large-region.json --quality performance \
  --telemetry reports/large-region.json

cargo run -p terra-demo --locked -- \
  --config configs/heightmap.json --quality high --time-scale 10 \
  --telemetry reports/heightmap.json
```

**F10 → World** changes Performance/Balanced/High, atmospheric sky and the
possession focus policy. **F12** opens the performance pane. The world and
performance panes share the left side; the gameplay inspector remains
independent. All long panels scroll with the pointer over them, including at
1100 × 700. Reset timings after the scene finishes loading or after switching
quality so the rolling window represents the state you intend to measure.

Performance disables multisampling, atmospheric scattering and bloom, limits
shadow resolution/distance, and caps nearby ground detail. Balanced and High use
4× multisampling; High increases shadow and sky budgets and keeps more terrain
detail. These are cost controls rather than frame-rate promises. The exact table,
adapter fallback, terrain topology and model override API are in
[GRAPHICS_AND_MAPS.md](GRAPHICS_AND_MAPS.md).

The default region is **49 chunks**, with **65 × 65 source height samples** per
chunk. The optional larger-region preset requests **121 chunks** and provides
144 resident/queued slots, while retaining two concurrent load tasks and two
integrations per frame. It is for offline or prepared data. Raising the view
radius is not permission to bulk-download public demonstration services.
Heightmap requests shrink to complete covered tiles; the included raster does
not cover the 121-chunk preset at its original bounds.

### What each measurement means

| Measurement | Meaning | Practical limit |
|---|---|---|
| Frame interval | Wall-clock interval between final main-app updates; last, mean, p50, p95 and maximum of at most 240 samples | Includes waiting, rendering synchronization and other CPU work; not a GPU timestamp or isolated render cost |
| Stream integration | CPU time accepting streamed results, updating residency and associated base state | Provider/background worker duration is separate |
| World rules | Local authoritative base-world fixed update | Absent on a network client |
| Wildlife decisions | The ecology update, including indexed threats, scheduled behavior and ecological maintenance | Far-decision savings do not mean all ecology work was skipped |
| Bodies / production | Living advancement including needs, routing, movement and production | Uses the configured physical resolver and current workload |
| Economy / contracts | Settlement/economic maintenance after production | A bounded pass can be cheap between maintenance deadlines |
| Controlled movement | Local controlled-body advancement outside ordinary autonomous movement | Reports only when that path runs |
| Regional checkpoint | Main-thread automatic/residency checkpoint submission, including snapshot copying and request construction | Worker validation/commit and completion installation are separate costs; F5, startup restore and final exit are separate lifecycle calls |
| Terrain vertices | Current integrated terrain vertices versus equivalent full grids | Full resident heights remain authoritative; separate actors and overlays are excluded |
| Source vertices | Integrated source terrain and source feature geometry | Excludes actor, construction, ground-cover and other independent meshes |
| Imported actor scenes | Selected and reserved instances, geometry, animation, textures and estimated payload bytes, with loading/retiring/deferred counts | Counts validated packs; arbitrary overrides are unmetered. Estimates are not GPU allocator measurements, and lowered limits may be draining |
| Wildlife work | Admitted, near/far, deferred and threat-candidate counts from the latest behavior pass | Deferred animals still move and maintain needs |
| Replication bytes | Actual encoded actor/sandbox updates compared with equivalent full views | Excludes handshake/control traffic; transport counters provide actual socket totals |

`WorldDiagnostics` is a reusable Bevy resource. Each `TimingWindow` has a fixed
240-value capacity and ignores invalid/negative samples. `WorldStage` separates
streaming, base world, ecology, living, economy, controlled movement and regional checkpoints.
`report()` returns serializable summaries; `reset()` clears timing windows.
This is an opt-in diagnostic surface, not a change to simulation authority.

`--telemetry FILE` records a final native diagnostic capture after the exit
checkpoint barrier. Add `--telemetry-interval-seconds N` for opt-in periodic
capture during play, including while the game clock is paused. A bounded
background writer retains the latest 16 successful captures across runs; each
names its capture time and running/graceful/error exit status. Failed writes
preserve the previous complete file, and a requested final export failure
produces a failing process exit.

The report contains quality/effective atmosphere, source revision/epoch, rolling
timings, render counts, population scope, ecology work, replication/transport
counters and reported simulation/storage errors. It excludes session capabilities
and complete private world state. It is neither a savegame nor a crash dump.
A last `running` capture after termination gives sampled context, not proof of
health. Use a new diagnostic path or a valid schema-one diagnostic file; active
save/database aliases, symlink paths and unrelated documents are rejected.
See [Retained diagnostics](DIAGNOSTICS.md) for byte/queue bounds, retention,
interruption semantics and incident triage.

Network clients label their counts as received views. They cannot report host
CPU stage timings. Connection state is shown before baseline state, so retained
traffic after a disconnection is not presented as an active synchronized stream.
Counters retain initial keyframes and any resync overhead; a very small workload
can have negative savings, displayed explicitly as overhead.

## Spatial and simulation probe

```sh
cargo run -p terra-sandbox --example scaling_probe --locked
```

See [SCALING.md](SCALING.md) for API and exact fixture construction. The archived
[v0.4 probe](scaling-probe-v0.4.json) contains three deliberately separate tests:

| Fixture | Recorded result |
|---|---|
| 100,000 position representations, 64 queries | 5,720 broad-phase candidates versus 6,400,000 direct distance checks; identical IDs |
| 64 complete sandbox views over 512 bodies and 10,000 resources | 37.145 ms scanning versus 5.906 ms index construction + 5.542 ms views; identical 779,280 serialized bytes |
| 1,000 domain fixed steps over 50 physical seconds | 550 admitted animal behavior evaluations versus 1,100 possible full-pass evaluations; 223 production cycles and one physical delivery |

The snapshot batch was approximately **3.24× faster including index construction**
in that single development-profile run. It registered principals but opened no
network connections. The 100,000 positions are not complete living actors.
The domain fixture uses a permissive finite-coordinate resolver and does not
measure imported-world collision. Its behavior-count reduction does not measure
a 50% reduction in total AI CPU or total simulation work.

## Actual TCP replication probe

```sh
cargo run -p terra-net --example replication_load --locked -- \
  --clients 16 --actors 4096 --ticks 200 --output reports/replication.json
```

The harness accepts 1–32 clients, 32–20,000 generic actor records and 40–2,000
ticks. These are harness input ranges. Its result includes workload and platform
metadata, real socket bytes, exact full-view comparison bytes, host-update
percentiles, phase results and client receive counts.

The archived [v0.4 run](network-load-v0.4.json) used 16 actual loopback clients,
4,096 generic kinematic NPC records plus 16 principal records, and a separate
140-body living domain. Client principal views remained capped at 128 records.
The first phase moved 25% of generic records; the second kept those records
stationary while the living settlement continued.

| Whole-run observation | Value |
|---|---:|
| Logical time | 11 seconds at 20 Hz, including 20 warmup ticks |
| Encoded replication / equivalent full views | 28,919,752 / 77,864,588 bytes |
| Replication reduction | 62.86% |
| Actual aggregate client socket receive | 28,936,090 bytes |
| Keyframes / deltas | 64 / 2,592 |
| Host-update mean / p95 | 14.82 / 37.32 ms |
| Disconnects / domain errors | 0 / 0 |

This was an accelerated finite local test in an optimized development build with
debug assertions, Linux x86-64 and eight visible logical CPUs. It did not render
a scene, add Internet latency, use TLS, persist regional data or partition the
host. It predates protocol-4 driving and transport records. The two actor
representations must not be added together as a claim of fully simulated NPCs.
See [NETWORK_REPLICATION.md](NETWORK_REPLICATION.md) for phase-level details,
keyframe fallback, reconstruction validation and resync behavior.

## Archived v0.4 native coverage

The recorded v0.4 larger-region run integrated **all 121 chunks with zero pending
builds**. It contained 2,050,554 source vertices, including 138,681 terrain vertices
against 511,225 original height samples. The High heightmap run integrated 49
chunks and retained the same 107-body/64-animal local scenario. These are separate
workloads and quality settings. The complete exported reports, including their
software-rendered frame intervals, are in
[native-v4-verification.json](native-v4-verification.json); current captures and
interaction findings are in [VALIDATION.md](VALIDATION.md).

## What to measure before raising a budget

Choose a representative prepared geographic region and your actual game assets.
Measure release builds on intended hardware with both calm and active scenes.
Track resident memory, main/render thread frame intervals, actual GPU timestamps,
terrain/texture uploads, physics/navigation, save latency, per-region simulation
cost, authoritative command latency and fanout under representative connections.
Repeat with interest transitions, disconnect/recovery and bounded missing data.

The reference still caps living bodies, wildlife, mutable collections, habitats,
claims and per-client views. Transactional paging reserves dormant capacity.
Current ordinary checkpoints certify unchanged environmental dependency groups,
validate current global/active and dependent cold records, and reuse equal page
bytes on one storage worker. Complete hydration materializes all needed records;
untrusted decode and explicit restore still validate the full bounded world. Authority pauses while that worker owns the
transition; snapshot cloning and result installation still cost main-thread
time. Measure command-to-durable-success latency as well as frame/tick latency.
Verified TLS adds handshake and encrypted-wire costs; the archived plaintext
loopback benchmark does not measure them. Increasing constants does not remove
these costs or add distributed simulation. The next infrastructure boundaries are
detailed in
[MMORPG_FOUNDATION.md](MMORPG_FOUNDATION.md).

## Current regional persistence measurements

`WorldStage::Persistence` records the automatic/residency checkpoint submission
system separately from simulation and source-streaming timings. In the current
implementation this is bounded main-thread work, including the submitted world
copy. It no longer measures SQLite validation and commit on the calling thread.

F12 and `--telemetry` report `last_submission_ms` separately from
`last_checkpoint_ms`, which is the worker duration of the last successful
checkpoint. They also report pending/paused state, committed generation,
resident/dormant counts and complete serialized payload bytes. The worker metric
includes scoped validation, certified-group merge, selection, encoding and commit;
it is not disk I/O alone. Nonblocking polls
and completion installation run in a separate system and are not included in the
automatic submission stage. Profile them and total frame intervals before making
a frame-budget claim. Network hosts expose `ServerReport.persistence_pending`
and `storage_work_ms` for the same distinction.

The byte counter excludes SQLite file, index and journal overhead. Region counts
include only nonempty mutable pages; they do not equal terrain-chunk or population
counts. One outstanding job bounds queued snapshots. Authority stays frozen while
that job or a complete hydrated mutation batch is pending, even though the view
and application loop remain responsive. Initial open/restore, explicit recovery
and export, and ordered final flush remain synchronous maintenance operations.

Run the finite storage probe:

```sh
cargo run -p terra-persistence --example regional_probe --locked -- --output regional-report.json
```

Current output separates the direct maintenance baseline in `checkpoint` from
`background_checkpoint`: submission including snapshot cloning, nonblocking poll
cost and worker latency. `verified_partition_cache` records cache reuse and cold
page decoding; complete restore is a separate measurement. Every checkpoint,
restart and online backup must preserve the complete logical state. Dormant
resource values must remain unchanged. No collection cap is raised by this probe.

Its full and active simulation passes deliberately do different work: dormant
environmental records freeze while the global economy and active owners' bodies
continue. A separate owner-dormancy fixture now measures retirement of disconnected
owners' bodies and homes, along with exact preservation and reactivation checks.
The report uses temporary private databases and emits no capabilities. It has no
renderer, live data, imported collision, socket load or distributed handoff. The
fixtures exercise living/resource residency and owner lifecycle. They do not
measure a driving-job workload, company dispatch throughput or encrypted session
load. A small submission duration does not imply the complete checkpoint finishes
that quickly; a worker latency does not imply a main-thread stall of that duration.

### Recorded 0.7 worker fixture

The [final versioned 0.7 result](regional-probe-v0.7.json) was recorded during
validation on 2026-10-27 against the final 0.7 sources. Its complete world contains
**239 living bodies and 6,400 resource nodes**; the active working set contains
**59 bodies and 400 nodes**. It runs 500 simulation steps, covering 25 physical
seconds and 250 calendar seconds, then 16 samples of each checkpoint mode.
All 33 committed generations, complete exports, restart, online backup and
dormant-resource preservation checks pass exactly.

| Measured stage | p50 | p95 or single measurement |
|---|---:|---:|
| Background submission, including snapshot copy | 0.483 ms | 5.369 ms p95 |
| Background worker validation/commit | 32.104 ms | 46.387 ms p95 |
| Direct synchronous maintenance checkpoint | 30.905 ms | 38.713 ms p95 |
| Nonblocking poll | — | 0.004898 ms maximum across 473 calls |
| Complete explicit restore | — | 32.695 ms |

The initial checkpoint took 31.574 ms. The final complete checkpoint payload is
1,700,829 bytes, excluding SQLite file/index/journal overhead. The guarded
partition cache recorded **98 hits and zero repeated checkpoint decodes**. Its
immutable, catalog-pinned certificate avoids redundant validation of unchanged
cold partitions. Changed active state and every complete merged result still
receive full validation, and checkpoints still serialize the bounded payloads.
This does not implement transactions limited to affected dependency groups.

The full and active simulation passes took **99.654 ms and 40.162 ms in total**
for their respective 500 steps. They intentionally advance different resident
populations, so these totals cannot be interpreted as an optimization of identical
simulation work. They also exclude rendering, live providers and socket traffic.

The run used an optimized development build (`opt-level=1`, debug assertions
enabled), SQLite 3.53.2 and eight visible logical CPUs in a shared Linux x86-64
container. Main-thread tail costs remain visible: the measured submission p95 is
higher than the archived 0.6 sample even though the worker and direct-checkpoint
samples are lower. Shared machine load, the current simulation workload and
serialization changes prevent isolating a single code change from these runs.
Repeat representative release-build measurements before assigning a frame,
command-latency or hosting budget. Authority still waits for worker completion;
result installation is outside the submission measurement.

### Recorded 0.7 owner residency

The same [0.7 report](regional-probe-v0.7.json) includes a separate fixture with
16 synthetic owners, five bodies and one referenced home per owner. It keeps one
owner active, marks 15 dormant and commits a regional transition.

| Resident authoritative records | All 16 owners active | One active, 15 dormant |
|---|---:|---:|
| Owned living bodies | 80 | 5 |
| Referenced homes | 16 | 1 |

The retirement checkpoint took **4.008 ms** in this single sample. After 100
steps, covering 5 physical seconds and 50 calendar seconds, the offline bodies,
needs, orders, account values, inventory and cold renewable resources remain
exact. Closing and reopening the database preserves the complete state, and
reactivating the owners passes complete reference validation.

All identities and account ledgers remain authoritative, and offline records
still count toward the existing aggregate limits. This fixture has no network
clients or renderer and does not measure concurrent-player capacity or resident
memory savings. The longer cargo/payroll dormancy and actual TCP restart/resume
regressions are correctness checks documented in [VALIDATION.md](VALIDATION.md),
separate from this timing probe.

### Recorded 0.6 worker fixture

**Archived 0.6 evidence.** The [versioned 0.6 result](regional-probe-v0.6.json) was
recorded during final validation on 2026-10-27. Its complete world contains 239
living bodies and 6,400
resource nodes; the active working set contains 59 bodies and 400 nodes. It runs
500 simulation steps and 16 samples of each checkpoint mode, with every durable
generation, complete restart, online backup and dormant resource check exact.

| Measured stage | p50 | p95 or single measurement |
|---|---:|---:|
| Background submission, including snapshot copy | 0.687 ms | 0.991 ms p95 |
| Background worker validation/commit | 47.622 ms | 73.985 ms p95 |
| Direct synchronous maintenance checkpoint | 44.527 ms | 63.280 ms p95 |
| Nonblocking poll | — | 0.063 ms maximum across 773 calls |
| Complete explicit restore | — | 58.560 ms |

The verified partition cache recorded 98 hits and no cold checkpoint decodes
after the initial verified commit in this fixture. The authority still waits for
the worker's completion; its longer latency is not hidden from command completion
or presented as concurrent simulation. Result installation is not included in the
submission number. This was an optimized development build with debug assertions,
SQLite 3.53.2, and eight visible logical CPUs in a shared Linux x86-64 container.
It is finite regression evidence, not an isolated hardware benchmark or a claim
about gaming-GPU performance, active players or production MMORPG capacity.

### Archived 0.5 storage evidence

The [recorded 0.5 report](regional-probe-v0.5.json) describes the original
synchronous implementation: actual full/active body and resource counts, 500
simulation steps, checkpoint samples, complete restore latency and exact
preservation checks. Those timing samples are historical evidence. They do not
measure the current worker's submission, polling or cache path. Record a new
report with the actual package version and environment to evaluate current costs.

The next work is extending certified dependency classes beyond static environment
records and measuring longer representative multi-client workloads. Background
storage, TLS, owner dormancy, static-group certificates and finite real-session
churn evidence are implemented; distributed ownership/handoff and production
capacity remain separate infrastructure work.

## 0.8 scoped validation and checkpoint stages

Run `regional_probe` to compare two domain merge paths against the same revision:
`ValidatedRegionPartition::merge_validating_all` repeats complete merged-state
validation, while `merge` certifies immutable cold resource/source/tombstone
groups. Sixteen samples alternate execution order and require identical canonical
partitions outside the timed scopes. This comparison excludes storage, encoding,
working-set selection and socket work.

The checkpoint report separates composition, selection, encoding, transaction and
worker installation, with exact validated/certified record counts and encoded or
reused page bytes. Sharing the immutable certificate makes cache-hit clones cheap;
unchanged page payloads avoid repeat serialization and SQL binding. Geographic
pin discovery, ID admission and final partition assembly still perform bounded
regional work. Host-visible state replacement is outside worker-install timing.

### Recorded 0.8 storage result

The [0.8 regional report](regional-probe-v0.8.json) retains the 239-body,
6,400-resource fixture and its exact checkpoint/export/restart/backup checks.
Its working set has 59 bodies and 400 resources. The alternating same-revision
comparison gives these measurements on the shared Linux development environment:

| Operation | p50 | p95 |
|---|---:|---:|
| Full merged-scope domain validation and partition | 10.777 ms | 17.261 ms |
| Certified environmental-group domain merge | 3.462 ms | 8.902 ms |
| Background submission including snapshot copy | 0.408 ms | 1.265 ms |
| Complete background checkpoint worker | 10.424 ms | 17.855 ms |
| Direct maintenance checkpoint | 8.433 ms | 13.644 ms |

The certificate covers **6,000 unchanged environmental records in 15 dependency
groups**. The supplied active state validates 476 records; the merged validation
scope includes 656 after adding 180 actual cold dependents. Working-set selection
validates its resulting active records separately. In the last checkpoint all
19 equal pages, totaling 1,683,609 payload bytes, are reused; no regional page is
encoded again. Global state and metadata still serialize and commit normally.
The explicit complete restore takes 36.372 ms and still performs full
untrusted-data/canonical validation. This is a scoped reduction, not removal of
whole-world boundaries or proof of fixed hardware latency.

| Background worker stage | p50 | p95 |
|---|---:|---:|
| Compose certified revision | 4.723 ms | 7.846 ms |
| Select and validate the next working set | 2.713 ms | 6.081 ms |
| Encode global state and changed pages; account for retained bytes | 0.748 ms | 1.394 ms |
| SQLite transaction | 0.994 ms | 1.306 ms |
| Install the store's completed working set and cache | 1.148 ms | 2.591 ms |

These are independent percentiles over sixteen checkpoints; adding their medians
does not reproduce the median complete checkpoint. The host's later visible-state
replacement remains outside the worker-install stage. The guarded cache records
99 hits and zero repeated checkpoint decodes after initial certification.

## 0.8 actual regional session churn

```sh
cargo run -p terra-net --example regional_session_load --locked -- \
  --clients 8 --cycles 3 --ticks 120 --transport tls \
  --output reports/regional-session-load.json
```

The harness connects real clients to a local authority with full living ownership,
private held cargo, crafting commands and a regional database. It alternates
overlapping/spread spectator interests, disconnects half the clients while the
others keep simulating, resumes with the same capabilities and sequences, and
restarts the host between cycles. Private saved fixture bytes remain temporary.
Only counts, timings, memory samples and verification results enter the report.

It also holds a real second SQLite write transaction until the worker reports a
failed checkpoint, verifies that authority state/tick/generation did not advance,
then releases the lock and requires exact successful retry. Full private owned
state is compared before/after offline simulation, authenticated resume without
a tick, and restart. Vehicle cargo and repeated craft inventory remain exact.

`--transport tls` uses the public test CA with localhost verification and encrypted
sockets; `--transport tcp` measures plaintext loopback separately. Host-update
tails distinguish advancing and storage-wait calls; socket bytes include TLS and
control traffic when present. Process RSS includes the authority and all local
client fixtures. A finite RSS trace does not prove a long-duration memory plateau.
An advancing sample reports at least one fixed step; a storage-wait sample reports
`persistence_pending` at return. These classifications can overlap, and the
pending-update count is not elapsed pause duration or pure poll cost. Bootstrap
fixture construction is excluded from measured socket/timing totals; admission,
resume and restarts inside the churn cycles are included.
The surface is authored flat geometry; there is no renderer, imported collision,
live data provider, Internet delay/loss or distributed handoff. These are finite
correctness and workload measurements, not an MMORPG concurrency target.

### Recorded encrypted churn and recovery

The [eight-client TLS](regional-session-load-tls-v0.8.json) and
[eight-client TCP](regional-session-load-tcp-v0.8.json) reports retain three
cycles, 100 full living bodies, 518 resources and 540 fixed steps. The separate
[extended TLS report](regional-session-stress-tls-v0.8.json) uses sixteen actual
clients, 140 full living bodies (80 owned), 1,030 resources and eight cycles.
Its 4,800 fixed steps represent **240 physical simulation seconds**, executed in
48.620 measured wall seconds. It reaches committed generation 162, compares
64 offline owners, 64 resumed owners and 128 restarted owners exactly, and
preserves all cargo, inventories and command sequences through one forced writer
failure and successful retry.

```sh
cargo run -p terra-net --example regional_session_load --locked -- \
  --clients 16 --cycles 8 --ticks 400 --transport tls \
  --output reports/regional-session-stress.json
```

| Recorded observation | Eight TCP clients | Eight TLS clients | Sixteen TLS clients |
|---|---:|---:|---:|
| Fixed steps / restart cycles | 540 / 3 | 540 / 3 | 4,800 / 8 |
| All host updates, p95 / p99 | 9.130 / 10.915 ms | 9.916 / 12.846 ms | 19.664 / 24.758 ms |
| Advancing updates, p95 / p99 | 9.564 / 12.174 ms | 10.472 / 14.319 ms | 20.150 / 25.421 ms |
| Largest observed host update | 13.835 ms | 37.909 ms | 112.079 ms |
| Updates reporting pending storage | 369 | 322 | 1,007 |
| Actual aggregate client receive | 19,890,922 bytes | 19,619,750 bytes | 291,225,409 bytes |
| Actual aggregate client send | 55,780 bytes | 85,564 bytes | 1,098,464 bytes |
| Process RSS: initial / peak / final | 13,916 / 23,284 / 23,012 KiB | 13,844 / 23,468 / 22,844 KiB | 16,440 / 31,668 / 31,560 KiB |

The extended run encodes 282,139,960 replication bytes versus 804,739,441 bytes
for equivalent full snapshots, with 832 keyframes, 47,488 deltas and no resync
requests. Socket totals include other traffic and transport overhead. These runs
have different asynchronous batching and update counts, so their differences do
not isolate TLS overhead. The 112 ms maximum also prevents turning the lower
percentiles into a guaranteed 20 Hz host budget.

The native software renderer and Rust compilation were stopped during these
sequential measurements; the shared container was otherwise uncontrolled. The
[provenance report](regional-measurement-provenance-v0.8.json) retains exact binary
and compiled input hashes independently of the subsequent construction-placement
review fix. The final correctness gate is separate. Four simulated minutes and
the finite process RSS trace are useful churn/recovery evidence, not a long
production soak, an isolated server-memory measurement or an MMORPG capacity
result.


## Current combined session and recovery measurement

The [0.12 combined workload](SCALING.md#012-combined-prepared-terrain-and-cold-world-acceptance)
uses actual TLS clients on prepared terrain alongside dormant regional overlays.
Its report separates advancing host updates, storage-wait calls, ping round-trips
and durable command acknowledgements. Pending storage pauses authority; a small
poll duration does not imply a small command latency. Process RSS includes the
host and in-process clients. Current values and complete provenance are in
[Validation](VALIDATION.md); archived measurements above are historical.
