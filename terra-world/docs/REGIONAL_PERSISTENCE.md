# Durable regional state

Terra includes an opt-in regional database in the local game and reference server.
This is a concrete step toward a persistent online world: geographic gameplay
records can leave the active simulation, remain on disk, and return with their
identities and mutations intact. Immutable source terrain still uses its own
streaming cache.

The implementation is a **single-authority, bounded regional store**. The
existing aggregate population and collection caps still apply across active and
dormant records. Untrusted decode and explicit restore reconstruct and validate
the complete bounded world. Ordinary checkpoint merges certify unchanged
environmental dependency groups while validating every current global ledger and
dependent cold record. This release does not claim
unlimited dormant population, multiple authoritative shards, or production MMORPG
concurrency. Ordinary checkpoint and hydration work now runs on one bounded
background worker. Authority waits for its result while rendering and menus can
continue; startup, explicit restore/export and final shutdown remain maintenance
barriers.

## Run it

Create or resume a local world:

```sh
cargo run -p terra-demo -- --region-store saves/frontier.db --quality balanced
```

Open the same path with the same source configuration to resume. F5 checkpoints
now; F9 restores the latest checkpoint and discards subsequent changes. Automatic
checkpoints run at the configured interval (30 seconds by default), at residency
and gameplay-command boundaries, and on graceful exit. `--checkpoint-seconds N`
accepts 1–3,600 seconds. F12 shows the committed generation, active/dormant page
and record counts, retained global records, pins, payload size and checkpoint
worker latency. The frame persistence stage measures snapshot submission, not
the worker's complete disk and validation time. The World drawer explains the
selected persistence mode. While storage is working, new commands remain queued
and the last working state stays visible. A checkpoint is saved only when its
completion advances the committed generation.

Region counts describe **nonempty persisted pages** at the latest successful
barrier, so they need not equal the source terrain's resident chunk count.
Requested empty keys do not add a page. Active/dormant record counts describe
paged records; retained global records have a separate counter. Records include
metadata and tombstones, so one animal body and its ecology metadata count as
separate records. These counters are not player or NPC capacity measurements.
Pins may overlap requested regions; the displayed counts are not additive.

Without `--region-store`, existing explicit JSON F5/F9 behavior remains. The
demo rejects combining `--save` with `--region-store`, and rejects local storage
on a `--connect` client. Multiplayer persistence belongs to the host.

Start a regional reference server:

```sh
cargo run -p terra-net --bin terra-server -- --region-store saves/server.db
```

The host restores its private resume identities, accepted command sequence
history, clock, catalog, generic actors and sandbox together. It checkpoints
input and source batches, every 30 seconds and on graceful shutdown. Its
`--save PATH` option can additionally export a complete legacy server JSON file;
the export includes dormant records. `--load` and `--region-store` are mutually
exclusive on the CLI to prevent ambiguous restore precedence. A custom host can
explicitly call `load_world` before opening a new empty store to import a legacy
save; it must preserve the source manifest and catalog. Explicit CLI migration
and no-overwrite exports are described in [Operations](OPERATIONS.md).

## What sleeps, and what remains resident

| State | Residency policy | Why |
|---|---|---|
| Principals, faction roles, player inventory, account money, progression and identity counters | Global | Rights, balances and global identity allocation must remain authoritative |
| Land plots and community territory claims | Global | An unloaded region must not become unowned or available for an overlapping claim |
| Shops, producers, their stock/tills, roads, unowned couriers and delivery reservations | Global | Production, dispatch, physical delivery and refund semantics remain connected |
| Funded contracts, escrow, price anchors and settlement clocks | Global | Deadlines and refunds continue against the same issuer and target |
| Transport licenses, facilities, owned vehicle records and accepted transport jobs | Global | Deposits, cargo, vehicle references and delivery progress remain connected |
| Connected/local owners’ bodies and homes | Global | Control, possession and housing survive camera movement |
| Disconnected owners’ bodies and homes | Geographic pages with private reference indexes | Remove offline-owner terrain pins while keeping ownership, needs, inventory and housing intact |
| Other structures, farms, resource nodes, source-feature references and tree tombstones | Geographic pages | Their mutations persist while their region is outside the active working set |
| Ambient unowned bodies, wildlife habitats, animals and natural-resource metadata | Geographic pages with dependency closure | Habitat/animal/resource references must load and retire together |

Pages use the existing WGS84 equirectangular `TileKey` at the configured source
level. They are not Web Mercator map tiles. Source residency supplies requested
keys, including the regions of first-point feature anchors that lie across a
source chunk boundary. Existing pages around active players, their owned bodies, couriers and active homes
are pinned in an immediate neighbor ring, supplemented by 160 metres of exact
surface-distance coverage around existing record anchors. This protects poles,
the date line and fine tile levels where nearby objects can occupy non-neighboring
keys. Cross-page references then expand the working set until it is complete. This may keep more pages active than the
camera requests. Source geometry residency remains a separate physical
admission requirement; loading gameplay records does not invent collision data.

Dormant environmental records **freeze**. Crops do not receive hidden harvest
cycles, deposits do not replenish, and sleeping bodies do not accumulate needs
or movement. Reopening a world does not simulate elapsed wall time. The resident
economy continues while the running world advances; a stopped process advances
nothing. These are explicit reference-game policies that a larger game can
replace with a validated coarse offscreen simulation.

## Owner disconnect, cold records and resume

The server calls `SandboxState::set_player_dormant(owner, true)` when the last
accepted connection closes and for every principal installed during host startup.
No peer survives a process restart. The operation clears possession, body velocity
and vehicle momentum while preserving selected-body identity, orders, fuel,
condition, needs, inventories, ownership and finances. It is a trusted host API,
not a player command. Unconnected owners cannot issue gameplay commands.

Offline owned bodies and occupied homes become ordinary geographical records at
the next successful checkpoint. Bounded private descriptors contain the
identity/owner/type/position or housing ownership needed to check references in
an incomplete working set. A player-owned legacy logistics courier also retains
a small cargo projection so its global delivery receipt remains verifiable.
These projections are derived from actual cold records, checked
against dormant reservations and never copied to network views. A complete save
must still assemble and validate the real bodies and structures. Fabricated
references cannot turn a partial state into a valid standalone save.

If another observer keeps such a region resident, its offline owned bodies remain
frozen in place, including their needs and orders. They can remain physical
obstacles; they receive no simulated movement or payroll. Personal transport
assignments preserve cargo, passengers, funded rewards and remaining physical
time. Active race and company-trip clocks are shifted by the actual bounded
transport-clock increment. Company running/paused choices stay unchanged; payroll
and dispatcher work resume only when the owner returns. Public production and
settlement offer expiry still follow the running world’s normal policy.

An owned legacy courier pauses its existing delivery, including stall/retry
progress, whether its body is hot or cold. Its stock/service-bay reservations
remain attached to that obligation. Dormant resident couriers are not dispatched
on new work. Complete merge still verifies the actual body cargo: altering both
a global receipt and its private projection cannot create goods during checkpoint.
Unowned public couriers continue their normal physical delivery work.

A verified resume capability triggers hydration before reactivation. Waking an
owner while any body or referenced home remains cold fails without mutation.
After hydration the existing identity, ledger and selected body are reused; no
wall-clock needs or production catch-up is performed and stale manual input is
not reinstated. Pure local play has no network dormancy transition and continues
its existing offline-world policy.

Cheap handshake/sequence checks run before network-driven hydration. An obsolete
protocol, unknown resume capability, unauthenticated sandbox command or replayed
sequence cannot cause a complete-world storage job merely to receive rejection.
A successfully admitted gameplay command still hydrates its full dependency
context and waits for durable acknowledgement.

## Transaction and failure model

The `terra-persistence` crate uses pinned rusqlite 0.40.2 and bundled SQLite
3.53.2. It enables WAL journaling and `synchronous=FULL`. A checkpoint commits
the global ledger, changed regional payloads, deletions, host metadata and a new
generation in one SQLite transaction. An actor moving between regions has one
identity before and after that transaction; both sides change together.

The database contains one world header and one row per nonempty region. Each
payload has a BLAKE3 digest. Loading verifies payload sizes/digests, the database
schema, geographic placement, canonical dependency links, complete domain
references and catalog consistency. Source/epoch/host identity, region level and
catalog are bound at open. Unknown schemas and foreign worlds are rejected;
there is no fallback that silently creates an empty replacement world.

Verified canonical partitions are cached between operations. A cache hit requires
matching generation, schema, descriptor, connection data-version and local change
counter. Out-of-band writes invalidate the cache even without a generation bump;
`BEGIN IMMEDIATE` rechecks the external-write epoch before any mutation. Decoded
bytes always pass checksum, payload inventory and complete canonical validation.
The immutable `ValidatedRegionPartition` and catalog are shared across clones;
there is no mutable accessor, deserializer or public unchecked constructor.
`inspect` deliberately bypasses the cache for a scrub.

### Certified environmental dependency groups

A 0.8 checkpoint still validates the supplied active state first. It then builds
an exact merged validation scope containing current global ledgers, active records,
and **actual** cold bodies, cargo, housing, farms and ecology. This prevents a
forged matching private reference projection from standing in for saved records.
Only unchanged cold resource nodes, source features and tombstones can use their
previous immutable proof instead of repeated record/catalog validation.

A reusable group must be wholly cold and closed under the previously validated
symmetric dependency graph. Any component containing natural-resource ecology
metadata is excluded, because those records depend on the current global ecology
configuration. Mixed pages work: their environmental records can be certified
while their people, homes or other dependent records enter ordinary validation.
Cross-boundary duplicate IDs, aggregate capacity reservations and canonical
placement remain checked. Exact resource-to-source-feature links are rebuilt;
unrelated body/habitat links are not copied across lifecycle changes.

This is a deliberately conservative first dependency class. It does not certify
arbitrary economy, ownership, transport or ecology changes independently. New
validators that add dependencies on resource/source records must extend this
classification or keep the relevant groups in the merged validation scope.
`merge_validating_all` remains an explicit full-scope reference path; differential
regressions compare its canonical result with the certified path across residency,
movement, edits, deletion and global-ledger mutations.

Unchanged page bytes are also reused after exact page equality against the guarded
immutable proof. Their checksummed payloads and sizes remain in the complete
checkpoint inventory, but they are neither serialized again nor bound into SQL
updates. Changed and deleted pages, global state and host metadata still commit in
one transaction. Failed validation or writes do not install a new cache, residency
cursor, generation, active state or successful-work report.

`RegionStore::checkpoint_metrics()` and `RegionCompletion.checkpoint_metrics`
separate compose, select, encode, transaction and worker-install durations. They
report active/merged/certified record counts, selected records validated, encoded
and reused pages/bytes, and deleted pages. The worker's installation duration is
not the host's later replacement of its visible state. Counters contain no private
inventories, balances, capabilities or filesystem paths.

`BEGIN IMMEDIATE` serializes commits. The stored generation must still match
the handle's expected generation. A stale writer receives `StoreError::Conflict`
and cannot overwrite a newer checkpoint. The same connection's SQLite
`data_version` must also remain unchanged from snapshot preparation through
acquisition of that write lock. An out-of-band edit in that interval is rejected
even if it leaves the authority's generation unchanged; the prepared snapshot
cannot silently overwrite the edit. This is optimistic conflict detection,
not a lease or distributed writer election. Applications should run one authority
for a database. The second-writer check is a safeguard against accidental
concurrent hosts.

The 0.22 native managed-world path adds a separate cooperative session lease.
It holds an OS file lock on `world-session.lock` beside the selected `world.json`
from startup admission through the final gameplay save and presentation metadata
update. A second managed session for that profile stops before initialization.
The file remains in place when unlocked; clean exit and process termination
release the OS handle. Do not delete it to force concurrent access. This is
local managed-session coordination, not a database format change or distributed
authority election. Direct SDK/server writers and older applications must still
follow the single-authority contract. The 0.21 `world-started.json` witness
retains its separate role in identifying a previously admitted first launch.

The live working set changes only after the transaction succeeds. Failed writes
preserve its pre-checkpoint contents and the previous durable generation.
Local authoritative mutation pauses with a visible recovery message. A reference
server reports the persistence error and the CLI stops. With a store attached,
player admission and successful sandbox responses are held until their complete
input-batch transaction commits. A failed server barrier also prevents new input
from being consumed and pauses simulation until recovery. Movement and passive
simulation snapshots can be newer than the last commit; abrupt termination can
lose those later changes. Local changes made since a checkpoint can also be lost
or explicitly discarded with F9.

The worker accepts exactly one owned snapshot at a time. A second submission
returns `Busy`; neither channel grows with input rate. `try_complete` never waits
for disk. The local adapter checks the submitted revision before installation,
and rejects stale results without replacing visible state. The server serializes
authority access and rejects host mutation APIs while a job is pending. Received
messages are retained in bounded per-peer batches, with successful responses held
until the durable batch completes. Completed responses flush before admission of
the next batch, avoiding acknowledgement starvation under continuous input.
Storage wait time is dropped from fixed-step catch-up, so delayed storage cannot
produce a burst of simulation against newly hydrated dormant records.

New database and backup files use owner-only permissions on Unix. Database files
contain private state and, on the server, resume capabilities. Diagnostic reports
contain counters and timing, not those capabilities or complete inventories.
Symlinks and nonregular database paths are rejected. Alternate exports,
telemetry, screenshots and backups cannot target the active database or its
WAL/shared-memory/journal companions; guards run before startup and at writes.
Canonical, relative, symbolic-link and Unix hard-link aliases are checked.
The release archive excludes
databases, journals, session files and private saves.

File exports and snapshots now use the shared staged publisher. Errors before
publication preserve an existing destination. A failure after replacement or
no-clobber publication returns **published, durability unconfirmed** and keeps
the new visible bytes. Callers must inspect that outcome instead of assuming
rollback or deleting the destination. New parent directories and Unix directory
entries receive checked synchronization; file bytes are flushed on every
supported platform. The non-Unix implementation does not claim an equivalent
directory flush or Windows power-loss qualification. These file-publication
outcomes are separate from SQLite's transactional checkpoint/generation contract.
See [operations](OPERATIONS.md#file-publication-outcomes) for recovery steps and
the typed SDK error boundary.

SQLite durability still depends on the operating system, filesystem and storage
hardware honoring synchronization. Recovery tests exercise abrupt process exit
and unfinished transactions, not a physical power cut. Use local storage; WAL
databases are not intended for a shared network filesystem.

## Library lifecycle

The core APIs are independent of Bevy:

| API | Contract |
|---|---|
| `RegionStore::open(path, identity, catalog, level)` | Open/create without replacing incompatible data; an existing checkpoint must be restored before modification |
| `restore_validated(callback)` | Read the full committed state; validate game-owned metadata before changing the handle's generation or residency cursor |
| `hydrate(&mut state)` | Materialize dormant records before a whole-world lookup, source registration, gameplay command or admission |
| `checkpoint(&mut state, metadata, &requested)` | Validate, atomically commit and then install the dependency-closed working set |
| `export(&state)` | Recompose a validated complete world including active changes and dormant data without altering live residency |
| `status()` | Safe operational counts and the latest generation observed by this handle |
| `backup_to(new_path)` | Consistent online backup of committed data, including WAL contents; refuse an existing destination |
| `RegionWorker::submit_hydrate(state)` / `submit_checkpoint(state, metadata, requested)` | Submit one owned snapshot; reject a second outstanding job with `Busy` |
| `RegionWorker::try_complete()` | Nonblocking completion poll; install only if the submitted authority revision still matches |
| `RegionWorker::finish()` | Explicit ordered drain for startup, restore, maintenance or shutdown; do not call at ordinary frame barriers |
| `RegionWorker::with_store(callback)` | Synchronous maintenance access while idle; returns `Busy` before acquiring a pending worker lock |
| `RegionStore::open_existing`, `inspect`, `archive` | Bounded existing-world operations that preserve identity and do not advance the live restore cursor |
| `RegionArchive::import_new` | Validate and publish a new checkpoint database without overwriting an existing path |

The domain exposes `RegionPartition`, `RegionPage`, `RegionWorkingSet` and
`RegionKey`. The partition is not a normal `SandboxState` save. Its global
ledger alone is intentionally incomplete. `assemble_all` verifies canonical
placement; `working_set` expands dependencies; `merge_active` preserves cold
records while replacing exactly the previous active set, including deletions.

Active states carry exact dormant capacity reservations. This prevents passive
wildlife recovery or new allocations from exceeding global limits simply
because old records are sleeping. `is_partial`, `dormant_reservations` and
`require_complete` make the distinction explicit. Ordinary sandbox and host JSON
save validation rejects partial states; use `export` for a full save. Custom
serializers must also call `require_complete` when promising a whole world.

Always hydrate **before** registering source features. Otherwise an unloaded
depleted node could appear absent and be seeded again. The provided adapters
perform this barrier and commit/retire after the batch. They also hydrate before
gameplay commands so workbench lookup, building overlap, territorial upgrade
requirements and other global queries see all authoritative records. Do not run
hydration or full export inside a per-frame inspector just to render a number.

The Bevy adapter uses `RegionalPersistence`, a dedicated persistence schedule
stage, a nonblocking `PreUpdate` completion poll, pre-startup restore and an
ordered final checkpoint before telemetry.
Restore precedes camera setup and preserves the saved clock. World replacement
resets transient ecology scheduling; moving the floating origin does not. In
network mode the server owns storage and clients retain only bounded views.

Game-owned systems must obey `RegionalPersistence::authority_paused()` before
mutating the sandbox, bodies or clock. `blocked()` reports an error, while
`pending()` reports work in flight; these states are distinct. Increment
`SandboxRuntime::revision` for game-owned authoritative state mutations. Do not
retain a mutable world reference across a background job. Rendering, camera
movement, inspection and menu navigation can continue while authority waits.

Custom server source streamers should retain a bounded batch of completed events,
call `prepare_source_batch()` until it returns true, register the batch, then
call `request_region_checkpoint()`. The reference CLI does this. A source owner
must finish an admitted batch so its hydrated state can retire. Explicit
`checkpoint_regions`, `try_register_chunk` without prior preparation, and complete
JSON exports are synchronous maintenance APIs; they are not the normal tick path.

`RegionalPersistence::export_world` returns a complete validated `LocalWorldSave`
without changing residency. Call `RegionalPersistence::check_output_path` before
writing that envelope to an explicit destination; this rejects aliases of the
attached database and its SQLite sidecars. `RegionalPersistence::backup_to`
backs up the last committed database, whereas an export includes current active
changes. The native CLI validates telemetry, screenshot and political-export
destinations before opening a window, and writers recheck their destinations.
These adapter operations do not require the renderer feature.

## Backup, recovery and migration

Use `backup_to` while a store is open. A raw copy of only `world.db` can omit
committed WAL data. For an offline copy, stop the authority cleanly before
copying the database; preserve any surviving journal files during incident
recovery. Keep the original incident files unchanged while investigating.

After a conflict, stop the extra authority and explicitly restore the newest
checkpoint before writing again. The reference `Server` must restart for that
generation change; it does not replace an attached connected world in place.
After a storage failure, fix the underlying
filesystem condition, then retry F5 or restore with F9. F9 can discard accepted
but uncommitted progress; its UI says so. A checksum/schema/source mismatch
requires a known-good backup or an explicit migration, not repeated retries.

Existing v0.4/v0.5 complete JSON saves remain readable under matching source and
catalog rules. The added residency marker has a serialization default. The new
database has schema version 1, independent of domain save version 1 and network
protocol versions. Local-game and server envelopes deliberately have different host
identities and cannot be swapped. No automatic source, catalog, level or database
schema migration is implied.

Use `terra-worldctl import-local` with a matching world configuration for local
JSON saves, or `terra-server --import-json` with matching world flags for server
saves. Portable regional archives preserve the host metadata and pinned catalog.
All import destinations must be new. Do not rename a JSON save to a database or
expect `--region-store` to interpret it. See [Operations](OPERATIONS.md) for commands.

The Rust domain struct now has a private residency-reservation field. Downstream
games that previously constructed `SandboxState` with a struct literal should
use `SandboxState::new()` or `Default`, then the public mutation APIs/fields.
This source-level change prevents normal callers from accidentally discarding
cold capacity. It does not change complete legacy JSON data compatibility.

## Measured limits and next structural work

| Store limit | Value |
|---|---:|
| Regional pages | 4,096 |
| Explicit requested keys per store checkpoint | 1,024 |
| Serialized document | 32 MiB |
| Complete checkpoint payload | 64 MiB |
| Main database file at open | 256 MiB |

The domain's lower collection limits still constrain real gameplay population.
These are admission bounds, not tested capacity promises. Current global/dependent record validation and full untrusted-data validation
still cost worker time. The authority pauses during a job; this is
responsive presentation with serialized mutation, not concurrent simulation and
storage. Snapshot copying and result installation still have bounded main-thread
costs. The finite regional probe reports submission including snapshot cloning,
nonblocking polls, worker latency, cache hits and explicit restore time separately.
The [v0.8 report](regional-probe-v0.8.json) compares complete and certified-domain
merges over the same revision, requires exact canonical equality, and exposes
individual checkpoint stages. It also verifies 16 synthetic owners:
disconnecting 15 reduces resident owned bodies from 80 to 5 and referenced homes
from 16 to 1, while preserving private state, cold resources and exact restart.
Its single-owner environment fixture still checks every checkpoint, worker
completion, full export and online backup for exact logical equivalence.

Next steps are indexed durable record queries, extending certified dependency
classes beyond static environment records, formal database migrations and measured
authority handoff between regional hosts. Geographic pin discovery, duplicate
checks and the final bounded partition still inspect or copy regional records;
this is not a constant-time checkpoint or concurrent authority writer.
Those changes must preserve ownership, conservation, identity and replay
invariants before aggregate limits are increased.

Owner dormancy removes logout-driven body/home pins; it does not evict principals,
accounts, vehicle ledgers or other global economy records. The 128-player and
512-body aggregate caps continue to count offline records. This remains a bounded
single-authority service rather than elastic account storage.

Primary implementation references: [SQLite WAL](https://www.sqlite.org/wal.html),
[synchronization policy](https://www.sqlite.org/pragma.html#pragma_synchronous),
[rusqlite transactions](https://docs.rs/rusqlite/0.40.2/rusqlite/struct.Connection.html),
and the [store implementation](../crates/terra-persistence/src/lib.rs).
