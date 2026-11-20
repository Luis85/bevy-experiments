# Regional simulation and replication scaling

Terra combines a reusable geographic broad phase, shared broadcast indexes,
bounded distant wildlife decisions and background regional storage. These reduce
repeated work while preserving authoritative population and save limits. They do
not turn the reference host into a distributed MMORPG or establish
concurrent-player capacity. Recorded v0.4–v0.8 results remain archived evidence
for those releases. The v0.8 measurements cover certified cold environmental
groups, exact unchanged-page reuse and actual encrypted session churn, including
disconnected-owner residency and preservation checks. They are not current-release
performance measurements. Current release correctness and executed gates are
recorded in [VALIDATION.md](VALIDATION.md).

## Geographic indexing

`GeoSpatialIndex<K = u64>` indexes numeric actor IDs or other ordered keys such as
resource IDs. It projects latitude/longitude onto the same mean-radius surface
sphere used by `GeoPosition::surface_distance_m`, then stores occupied Cartesian
cells. Altitude is intentionally ignored for interest selection; reach and
physical admission retain their own three-dimensional rules. Cartesian cells
avoid a longitude discontinuity and singular polar grid.

```rust
use terra_sandbox::GeoSpatialIndex;

let index = GeoSpatialIndex::from_positions(
    256.0,
    actors.values().map(|actor| (actor.id, actor.position)),
)?;
let visible = index.query(center, radius_m)?;
// visible.ids is deterministic and sorted.
// visible.candidates_tested reports exact distance checks after broad phase.
```

The cube broad phase includes every point whose surface distance can meet the
request. Exact great-circle distance performs final admission. Wide queries scan
occupied cells rather than iterating an unbounded volume of empty cells. Invalid
positions/radii, invalid cell sizes and duplicate identities reject. Queries do
not grant ownership or replace collision/navigation validation.

Traffic builds one 16 m-cell vehicle index per physical step. Same-direction
following checks only nearby candidates; its original ahead/lateral/heading
rules still decide yielding. Wildlife builds one 64 m-cell threat index per
behavior pass, including ground actors and foxes for hare avoidance. Actual
candidate work is observable rather than inferred from population.

## Shared interest snapshots

Build `SandboxInterestIndex` **once after world updates**, then reuse it for every
peer in that immutable broadcast. It covers living bodies, plots, structures,
farms, resource nodes and source-feature tombstones. Small bounded facility,
territory and faction collections retain their existing policies.

```rust
use terra_sandbox::SandboxInterestIndex;

let index = SandboxInterestIndex::new(&state)?;
for player_id in connected_players {
    let view = state.snapshot_for_indexed(player_id, interest_radius_m, &index)?;
    // Serialize/send through the authoritative network boundary.
}
```

The index borrows the authoritative state. Rust prevents mutation of that state
while a broadcast still uses its positions. Passing an index from another world
rejects. Rebuild after commands, movement, removal, load or any other state change;
never persist the index or accept it from a client.

`LivingInterestIndex` provides the standalone body surface, and
`LivingWorld::snapshot_indexed` reuses it. The original `snapshot_for` and
`snapshot` remain available. Indexed and original snapshots preserve the same
ordering, protected active/avatar units, distant owned units, private wallet and
pocket filtering, limits and byte-budget trimming. Tests compare their complete
serialized output, including depletion and other overlay layers.

This is an immutable batch index, not a durable world database or a concurrent
mutable spatial tree. Building it once for one small client can cost more than a
simple scan. Its intended benefit is amortizing repeated regional selection
across a populated world and multiple views; the finite probe reports index build
time separately from query time.

## Near and distant wildlife work

`EcologyScheduler` is a transient host resource. It budgets **behavior decisions**,
not physical movement. Existing fixed-step movement, collision callbacks, needs,
crop/resource advancement, production and cargo arrival must continue every step.
There is no accumulated offscreen movement jump and no skipped economic clock.

Default policy:

| Setting | Default | Behavior |
|---|---:|---|
| Near observer distance | 160 m | Evaluate near wildlife at every normal ecology behavior pass |
| Distant reconsideration | 2 physical seconds | Reconsider due far animals on a coarser cadence |
| Distant budget | 16 per behavior pass | Rotate eligible IDs so high IDs do not starve |
| Immediate threat override | Existing species/vehicle radius | A nearby threatening actor bypasses the far budget |
| Behavior pass cadence | Existing ecology policy; default 1 second | Near reactions preserve the existing cadence, not a new 20 Hz AI promise |

```rust
use terra_sandbox::EcologyScheduler;

// Persistent application resource, recreated/reset when replacing a world.
let mut scheduler = EcologyScheduler::default();

// In each authoritative fixed update:
let observers: Vec<_> = state.living.units.values()
    .filter(|unit| unit.owner.is_some())
    .map(|unit| unit.position)
    .collect();
state.advance_ecology_scheduled_with_environment(
    real_dt, sim_dt, hour_of_day,
    &mut scheduler, &observers,
    |candidate| resolve_loaded_ground(candidate),
)?;
state.advance_living_with_environment(real_dt, sim_dt, &catalog, resolve_motion)?;
```

Observers must be trusted owned-body positions, not a remote camera that can
manufacture activity. The server can restrict observers to connected principals;
local hosts may use all owned bodies. Connected/local player positions remain active
centers. A geographic threat query still runs on a behavior pass for each eligible
animal so an approaching vehicle or person cannot be missed merely because a far
AI decision was deferred. Indexed candidates reduce that detection work.

The behavior budget limits far work, not near work. Near or threatened animals
retain priority under current population bounds. With `F` eligible far animals,
budget `B`, pass interval `P`, and reconsideration interval `T`, a conservative
steady-state reconsideration bound is `T + ceil(F/B) * P`, excluding a paused host
or unavailable terrain. Fresh far populations are first considered within
`ceil(F/B) * P`. Population limits are still enforced separately.

The normal ground resolver gates roaming targets and population recovery. Unknown
terrain does not become walkable because an AI job is due. Owned animals retain
their game/player orders. Recovery clocks, animal age and depletion state remain
saved authoritative data; the scheduling cursor and diagnostics do not.

Call `scheduler.reset()` when loading/replacing a world, even if its calendar
value matches the previous world. Clock rewind also resets automatically. The
legacy `advance_ecology_with_environment` remains a full-pass option for hosts
that choose their own scheduling policy.

## Diagnostics and measurement

`EcologyScheduler::stats()` reports the latest eligible, near, far, evaluated and
deferred counts, actual threat/observer candidates, and cumulative pass/work
counters. These are job counts, not timings. Bevy/host instrumentation reports
elapsed time separately. `LivingMetrics::traffic_queries` and
`traffic_candidate_tests` are actual cumulative broad-phase work; their new save
fields default to zero for earlier saves.

Run the finite probe from the workspace root:

```sh
cargo test -p terra-sandbox --test scaling --locked
cargo run -p terra-sandbox --example scaling_probe --locked
```

It reports three deliberately separate scenarios:

1. **100,000 point representations and 64 geographic queries.** Every indexed
   result is checked against a full scan. This probes the index only; those points
   are not full simulated or networked actors.
2. **64 registered players, 512 living bodies and 10,000 resources.** It builds
   64 actual bounded `SandboxView`s, checks byte-for-byte equivalence to the old
   selector, and reports index build/query time, total snapshot bytes and the
   scan baseline. It does not create 64 network connections.
3. **1,000 actual domain steps.** The smaller ecology/settlement fixture uses
   scheduled wildlife alongside full-rate resources, needs, motion, production
   and delivery. It reports observed work and transfers. Its motion resolver is
   permissive and does not benchmark imported-world collision.

Wall timings depend on build profile and machine load. Candidate counts and
serialized equivalence are deterministic gates; one run's microseconds are not
a server capacity promise. See the release validation record for the actual
recorded result and the network guide for separate real TCP tests.

## Archived v0.4 probe

The [machine-readable result](scaling-probe-v0.4.json) records one Linux run of
v0.4 in the workspace's optimized development profile (`opt-level=1`). These
numbers predate the current transport domain and background storage worker.
Running the probe against current source produces new evidence, not an updated
meaning for the archived result. Other build work may share the machine;
deterministic equivalence/work counts carry stronger meaning than a single
elapsed-time sample.

| Scenario | Full-scan baseline | Indexed/scheduled result |
|---|---:|---:|
| 100,000 point representations, 64 radius queries | 6,400,000 distance checks | 5,720 candidate distance checks; identical selected IDs |
| Same index-only query timing | 270.878 ms | 34.860 ms index build + 3.804 ms queries |
| 64 actual sandbox views, 512 bodies, 10,000 resources | 37.145 ms | 5.906 ms index build + 5.542 ms views |
| Snapshot output | 779,280 bytes total | Byte-identical, with privacy and wire-budget rules retained |
| 1,000 domain steps, 22 animals, 50 behavior passes | 1,100 possible full-pass animal evaluations | 550 evaluations and 550 deferrals |
| Physical/calendar time in that domain fixture | 50 seconds / 3,000 seconds | Full-rate motion, needs and economy; 223 production cycles and one physical delivery |

The observed snapshot batch including index construction took about **3.24× less
time** than its scan baseline in this run. That ratio applies to this fixture;
a smaller population or one client can have a different crossover point. The
100,000-entry row measures the index alone. The 64-view row uses registered
principals without network sockets and remains within the current living-body
limit. No concurrent-player or distributed-world capacity is claimed.

## Recorded v0.7 owner residency

The [v0.7 regional probe](regional-probe-v0.7.json) contains a separate fixture
with 16 synthetic owners, five bodies and one home per owner. It keeps one owner
active and marks the other 15 dormant before committing a regional transition.

| Resident authoritative records | All owners active | Fifteen owners dormant |
|---|---:|---:|
| Owned living bodies | 80 | 5 |
| Referenced houses | 16 | 1 |
| Private account/identity ledgers | 16 owners retained | 16 owners retained |

The cold bodies, needs, orders, inventory and private account values remain exact
after 100 steps (5 physical seconds and 50 calendar seconds). Renewable resource
records at the retired owners' locations also remain exact. The probe commits,
closes and reopens the database, compares the complete state, then reactivates the
owners and validates all references. Separate domain tests keep loaded freight,
passenger escrow and company payroll frozen for 2,000 physical seconds; actual
TCP tests cover disconnect, restart and capability-based resume.

The same report retains the bounded 239-body/6,400-resource fixture and records
snapshot submission, worker completion, polling and exact checkpoint/backup
roundtrips. These are synthetic residency and correctness measurements with no
sockets, renderer or imported collision. They do not establish concurrent-player
capacity, and all aggregate caps still include offline records.

## Recorded v0.8 scope and real sessions

The [v0.8 regional probe](regional-probe-v0.8.json) retains the 239-body,
6,400-resource workload and exact lifecycle checks. It certifies 6,000 unchanged
cold environmental records across 15 dependency groups while validating a merged
scope of 656 records, including actual cold dependent bodies. Sixteen alternating
same-revision full and certified merges produce identical canonical partitions.
The last checkpoint reuses all 19 equal regional page payloads (1,683,609 bytes);
global state and host metadata still commit normally. Detailed compose/select/
encode/transaction/install and full-restore timings are in [Performance](PERFORMANCE.md).

The separate [TLS session fixture](regional-session-load-tls-v0.8.json) and
[plaintext TCP fixture](regional-session-load-tcp-v0.8.json) use **eight actual
socket clients**, 100 full living bodies (40 owned), 518 resource nodes and a
regional database. Each covers three churn/restart cycles and 540 actual fixed
steps, equivalent to 27 physical simulation seconds. They alternate overlapping
and spread interests, issue durable crafting commands, preserve seven held cargo
items per owner, and simulate while half the owners are disconnected.

Both fixtures require 12 exact offline owner comparisons, 12 exact authenticated
resume comparisons and 24 exact owner comparisons after host restart. A real
second SQLite writer induces one failure; authority state, tick and generation
must remain unchanged, and retry must preserve the same logical world. Socket
bytes, update-time tails, actual authority-wait calls and process RSS samples are
reported separately. The RSS process includes client fixtures as well as the
host. These finite accelerated samples have no renderer, imported physical world,
Internet network conditions or distributed authority, and do not establish a
production population target or a long-duration memory plateau.

The separate [extended TLS fixture](regional-session-stress-tls-v0.8.json) raises
the measured workload to **sixteen actual clients, 140 full living bodies
(80 owned) and 1,030 resources**. Eight churn/restart cycles cover 4,800 fixed
steps, or four physical simulation minutes. It performs 64 exact offline-owner,
64 authenticated-resume and 128 restart-owner comparisons, conserves held cargo
and crafted inventory, and requires failure-atomic recovery from a real second
SQLite writer before reaching committed generation 162.

Its advancing host updates measure 20.150 ms p95 and 25.421 ms p99, with a
112.079 ms maximum. The loop reports pending storage on 1,007 updates. Actual
aggregate client receive is 291,225,409 bytes; process RSS samples move from
16,440 KiB initially to a 31,668 KiB peak and 31,560 KiB at completion. These
figures include the local client fixtures. They do not prove a memory plateau or
a fixed 20 Hz deadline. The [measurement guide](PERFORMANCE.md#recorded-encrypted-churn-and-recovery)
separates the baseline TCP/TLS workloads, explains overlapping sample categories
and preserves [binary/source provenance](regional-measurement-provenance-v0.8.json).
No collection cap is raised, and there is still one authority and one writer.

## Remaining scaling boundaries

Full living movement and needs still iterate the resident living collection.
Resource regrowth/farm updates remain bounded linear passes. The storage limits
of 512 living bodies, 64 wildlife, 32 habitats, 192 ecology resource records,
256 seeded-region keys and 10,000 entries per base overlay collection remain in
force. This pass does not inflate those caps to imply additional capacity.

Transactional regional paging retains the global ledger introduced in 0.5,
canonical dependency closure, active-owner body/home neighborhood pins and exact
cold capacity reservations. Disconnect retires offline owners’ bodies/homes and
keeps their small account/identity/reference ledgers authoritative. Depleted nodes, tombstones and ambient bodies leave the
active simulation and return with their mutations intact. Money, ownership,
funded work, production and actual courier cargo remain connected in the global
ledger. Dormant environmental records freeze; no elapsed-wall-time catch-up is
invented. [REGIONAL_PERSISTENCE.md](REGIONAL_PERSISTENCE.md) specifies the policy.

Current ordinary hydration and checkpoint requests run on one bounded storage
worker. Immutable catalog-pinned proofs share their storage across cache clones.
Checkpoints reuse certified resource/source/tombstone groups only when the entire
dependency component is cold and contains no natural-resource ecology metadata.
Current global ledgers and actual cold bodies, housing, farms, cargo and ecology
still receive complete merged-scope validation. Newly decoded checkpoints always
receive full canonical validation. Unchanged pages retain exact checksummed bytes
and avoid repeat serialization and SQL binding. Schema, source/catalog identity,
generation and connection data-version checks remain mandatory, including a
write-epoch recheck inside the commit transaction. The main thread submits a
snapshot and polls for completion without waiting for SQLite.

Authority mutation pauses while the worker owns a transition. The previous
working state stays visible, success responses wait for a durable batch, and
dormant records receive no catch-up simulation. Snapshot cloning and result
installation remain main-thread costs. Startup, explicit restore/export and
ordered final flush are maintenance barriers. Background work makes the host
loop responsive; it does not make simulation and storage concurrent or remove
command latency.

The [archived 0.5 regional probe](regional-probe-v0.5.json) measured the earlier
synchronous implementation and verifies that fixture's exact checkpoint,
restart and backup roundtrips. The current probe separately measures snapshot
submission, nonblocking polling, worker latency, cache reuse and explicit
restore. Its outputs must be recorded for the actual build before making a new
performance claim. The [archived 0.6 result](regional-probe-v0.6.json) and
[measurement guide](PERFORMANCE.md#recorded-06-worker-fixture) report those stages
separately for the bounded living/resource fixture.

Verified TLS protects reference connections with explicit CA/server-name checks
and no plaintext fallback. Exact-principal suspension, capability rotation and
resume-only admission and transactional dormant-principal provisioning expose
bounded trusted-host policies; external real-person
enrollment and recovery remain game-owned. Larger services still need more
selective durable query classes, measured authority handoff, account integration
and longer representative encrypted-load measurements on declared hardware. [NETWORK_REPLICATION.md](NETWORK_REPLICATION.md)
and [OPERATIONS.md](OPERATIONS.md) describe the implemented APIs and flags.
[MMORPG_FOUNDATION.md](MMORPG_FOUNDATION.md) tracks those integration requirements.


## 0.9 bounded network-condition acceptance

The finite `terra-net` `network_conditions` example uses two actual localhost
clients, an opaque delay/jitter/fragmentation relay, and a paused receiver. It
requires optional snapshot deferrals while the healthy observer continues,
then drains the existing revision chain without resync and checks private-view
convergence, exact inventory/company continuity, host restart and stale-command
rejection. TCP and verified TLS run the same workload. The relay has a 128 KiB
per-direction cap; host application output remains capped at 512 KiB per peer.
The workload includes 510 simple kinematic stress records and two principal
records, not 512 full living inhabitants. Its correctness assertions and
queue counters do not establish Internet packet-loss performance, sustained
concurrency, geographic handoff or an MMORPG service level. See
[the complete fixture bounds](NETWORK_REPLICATION.md#reproduce-delayed-fragmented-and-stalled-connections).


## 0.10 prepared coverage and storage recovery scope

Prepared heightmap collections connect at most 32 bounded adjacent bundles with
validated shared seams and exact covered-tile unions. The example covers 81
level-15 tiles. This is source preparation and admission, not a distributed
terrain service or an increase to gameplay population caps.

The [0.10 recovery probe](recovery-verification-v0.10.json) stores 1,027 nonempty
regions, 6,144 resources and 47 living bodies, then restores a working set with
1,023 dormant regions. Verified online backup and recovery preserve exact full
state, metadata and committed generation; corruption fails without publishing
an output. Its recorded timings are a storage-only workload with no connected
clients, renderer or terrain import. They do not establish populated-world
throughput. The existing 4,096-region and 64 MiB checkpoint payload bounds remain.

Guidance permission work examines a bounded nearest-resource shortlist and uses
a spatial index for inaccessible plots. Autonomous vehicles now perform actual
shared support/envelope handling, so previous waypoint-motion CPU measurements
are historical. The current release validates behavior and exposes measured
stage timings; representative hardware/load soaks are still required before
claiming a larger sustained tick budget.

## 0.12 combined prepared-terrain and cold-world acceptance

The [final 0.12 report](verification/v0.12-final/regional-sessions.json) and
[captured command log](verification/v0.12-final/regional-sessions.log) record a
completed run on 2026-10-28. It used Linux x86_64, SQLite 3.53.2, debug assertions
enabled and eight available logical threads in a shared container. Reproduce
the actual local TLS workload with:

```sh
cargo run -p terra-net --example regional_session_load --locked --offline -- \
  --clients 16 --cycles 8 --ticks 400 --cold-regions 1024 --transport tls \
  --heightmap crates/terra-stream/data/heightmap-connected/manifest.json \
  --output reports/regional-sessions.json
```

Omit `--offline` if dependencies are not cached. The release runner retains the
output under `verification/v0.12-final/`; its gate took 132.362 seconds including
process startup/report handling. The workload itself reports **131.962 wall
seconds** for **4,840 physical fixed steps**: 4,800 in active/offline phases and
40 checking unavailable/restored terrain.

| Measured workload | Value |
| --- | ---: |
| Real verified local TLS clients | 16 |
| Full living bodies / owned bodies | 182 / 80 |
| Resource records | 5,248 |
| Resident prepared-source chunks | 25 |
| Cold overlay regions / depleted resource records | 1,024 / 4,096 |
| Churn/restart cycles | 8 |
| Active / half-disconnected fixed steps per cycle | 400 / 200 |
| Recovered nonempty regions | 1,051 |
| Final active / dormant regions | 27 / 1,024 |
| Final checkpoint payload | 1,686,295 bytes |

Crew placement uses the host's real support/occupancy checks. Cycles alternate
overlapping/spread interest, disconnect half the owners, resume and restart the
host. The cold depleted records are storage fixtures outside the local prepared
terrain; they do not make the missing world walkable or establish an inhabited
world in every region. No population or storage limit was increased.

### Current timing and memory observations

| Distribution | Samples | Mean | p50 | p95 | p99 | Maximum |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| All host updates | 7,038 | 13.417 ms | 10.220 ms | 43.424 ms | 55.531 ms | 91.151 ms |
| Advancing host updates | 4,840 | 19.311 ms | 15.951 ms | 46.691 ms | 57.483 ms | 91.151 ms |
| Storage-wait update calls | 2,119 | 0.328 ms | 0.001 ms | 0.617 ms | 14.753 ms | 35.285 ms |
| Durable command acknowledgements | 128 | 91.983 ms | 90.931 ms | 108.879 ms | 127.518 ms | 128.297 ms |
| Local TLS ping roundtrips | 6,464 | 25.992 ms | 21.040 ms | 58.887 ms | 73.111 ms | 137.605 ms |

Storage paused authority on the 2,119 storage-wait calls. These call durations
measure the synchronous update work, not the full time a command waits for its
durable acknowledgement. The acknowledgement and ping distributions include the
local fixture loop and storage barriers. The advancing-update tail exceeds a
50 ms budget; the result does not guarantee every 20 Hz step meets its deadline.

Recorded socket totals were **504,323,135 bytes received** and **1,137,456 bytes
sent**. Process RSS increased from **36.04 MiB before measured admission to
100.27 MiB after backup recovery and authenticated resume**, also the largest
sample. The process includes the host and local client fixtures. This finite
run does not establish a long-duration memory plateau or isolate server-only RSS.

### Exact continuation and recovery

The run passed 64 exact offline-owner checks, 64 authenticated-resume checks,
128 restart-owner checks and 16 recovered-owner comparisons. All 128 durable
crafting acknowledgements retained exact inventory/sequences; every real client
received snapshots. Cargo and all 4,096 cold resource records remained exact.

A real competing SQLite writer failed without changing authority, tick or
committed generation, and retry preserved the logical world. Only the injected
lock failure at an idle barrier with its single counted error was accepted;
additional or unrelated authority errors reject. Removing a source tile blocked
movement; restoring it resumed physical movement.

Verified backup/recovery preserved complete state, private metadata, source
bytes and **generation 165** before the same clients authenticated. The recovery
receipt records **579 copied pages in five steps, 1,051 nonempty regions and
42.378 ms**. Continuation ended at **generation 169**. The host-specific validator
and authenticated resumption add evidence beyond the generic database copy's
domain validation.

The separate [0.12 storage-only probe](verification/v0.12-final/recovery.json)
retains its zero-client, no-renderer, no-terrain-import scope: 1,027 nonempty
regions, 6,144 resources and 47 bodies, with 1,023 dormant regions after restore.
Its backup/recovery timings are not substitutes for the live-session measurements.

The [0.12 validation record](VALIDATION_v0.12.md) links all 20 accepted gates, 747 distinct
tests, 327 unchanged inputs and the exact executable. This is a finite accelerated
local reference-host workload with synthetic TLS principals and authored terrain.
It does not establish Internet latency, GPU performance or production concurrent
player capacity. The archived 0.8 figures above retain their original meaning.
