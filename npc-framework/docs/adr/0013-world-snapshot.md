# ADR 0013: World snapshots: a registry of tick-deciding state, saved between ticks

- Status: Accepted (game menu and AI explorer plan, Task 1)
- Date: 2026-10-24
- Source: `docs/superpowers/specs/2026-10-24-game-menu-and-ai-explorer-design.md` §5.3, ADR 0006, ADR 0009

## Context

The AI explorer keeps a world alive for many hours and many sessions. The only way to resume a world
today is a replay: rebuild the app from the scenario and seed, then re-apply every recorded command from
tick 0. That costs time in proportion to the world's age, and the recording grows without bound. A resume
must take the same time on day 1 and on day 100.

The state hash already has a registry (`SimHashRegistry`, ADR 0009). It knows which components and
resources are sim state, but it stores hash and JSON functions only: nothing that can rebuild a value. It
also does not cover all the state the next tick reads. `SimIdAllocator`, the pending command queue and
derived caches such as the spatial grid are not hashed, and `BrainState`, `ActiveLeaf` and `Blackboard`
are hashed without `Reflect`.

## Decision

- **A snapshot registry, separate from the hash registry.** `npc_kernel::snapshot::SnapshotRegistry`
  holds, sorted by stable name, a save and a load function per component (`fn(&World, Entity) ->
  Option<Value>`, `fn(&mut EntityWorldMut, Value) -> Result<(), String>`) and per resource
  (`fn(&World) -> Option<Value>`, `fn(&mut World, Value) -> Result<(), String>`), plus rebuild functions
  for derived state. Crates register through `SnapshotAppExt` (`register_snapshot_component_named`,
  `register_snapshot_component_with`, `register_snapshot_resource_named`, `register_snapshot_resource_with`,
  `add_snapshot_rebuild`) or the registry's own `add_*` methods, always under an explicit name (Task 2).
- **Hashed state registers itself.** `register_sim_component::<T>()` (and `_with`) also registers `T` under
  `SimHash::NAME`, so its bound grows by `DeserializeOwned` (and `Serialize` for `_with`). A
  `SimHashResource` opts in by overriding `SimHashResource::snapshot_fns` (usually
  `Some(snapshot::serde_resource_fns::<Self>())`); `register_sim_resource` then registers it under
  `SimHashResource::NAME`. Rust has no specialization, so this cannot be detected from a `Serialize` bound.
  Standalone registrations take an explicit snake_case name in the hash registry's style (Task 2: the
  first version keyed them by `std::any::type_name`, which is not stable across toolchains or refactors).
- **Snapshot entries are not part of `ContentHash`.** Adding a snapshot entry never changes the content
  hash, so it never invalidates a recorded replay. The content hash is stored in the snapshot to detect a
  save from another build.
- **The format** is `WorldSnapshot { version, content_hash, tick, header, entities, resources }` with
  `SNAPSHOT_VERSION = 1`. Entities are listed by ascending `SimId`, each with its components keyed by name
  (a `BTreeMap`), resources keyed by name. The `header` is host data (agent name, world name, dates); the
  sim never reads it. JSON (`serde_json::Value`) is the encoding; a binary encoding can replace it later
  behind a version bump.
- **Save between ticks only.** `snapshot::save` reads the world; it never runs inside `NpcSimTick`. It does
  not change sim results (ADR 0006).
- **Load into an app built from the same scenario and seed.** Content, registries and statics come from
  the build. `snapshot::load` then replaces the sim state, in this order: check the version and the
  content hash (`LoadMode::Strict` fails, `LoadMode::Lenient` continues and lists the mismatch in
  `LoadReport::skipped`); insert the `SnapshotLoading` marker; despawn every `SimId` entity in id order;
  restore the registered resources (unknown names are skipped, registered names missing from the snapshot
  keep their built value and are listed in `defaulted`); set `SimClock.tick`; spawn each entity as
  `(SimId(id),)` and load its components (unknown names skipped); set `PrevSimPos = SimPos`; run the
  rebuild functions in name order; clear the journal; remove the marker. While the marker exists, the
  `SimId` leave hooks and the `Spawn`/`Despawn` journal entries are skipped: those entities are not
  leaving the sim, the build is being replaced. A failed load (`Err`) leaves the world half replaced:
  build a fresh app before trying again.
- **Derived state is rebuilt, not stored.** Caches such as the spatial grid's entity index and door cells
  register a rebuild function.

## Consequences

- Every crate registers the state its systems read on the next tick. Hashed components get it for free;
  everything else is registered explicitly (game menu plan, Task 2).
- **The invariant test is the completeness check:** for any tick T, save at T, load into a fresh app and
  run N ticks gives the same state hash at every checkpoint as running to T + N without saving. A sim that
  loads equal but runs differently has a piece of state the snapshot missed; the test finds it.
- Tooling resources (journal, `AiTrace`, profiler, BRP state) are not saved. The journal of a resumed world
  starts empty.
- A snapshot from another build is never loaded silently: strict mode refuses it, lenient mode reports
  every skipped and defaulted name.

## Alternatives rejected

- **Replay-only resume.** Resume time and file size grow with the world's age.
- **Bevy `DynamicScene` via reflection.** `BrainState`, `ActiveLeaf`, `Blackboard`, `ExternalQueue` and
  others have no `Reflect`, scenes key components by type path (which moves with refactors) rather than by
  stable name, and scenes save every reflected component, including presentation and tooling state.
- **Extending `SimHashRegistry` with load functions.** The hash registry is part of `ContentHash`; state
  that must be saved but not hashed (the id allocator, the command queue) would have to enter the content
  hash and so change every recorded replay.

## Task 2: what is saved, what is rebuilt, and change detection after a load

Added by the game menu plan, Task 2. The invariant test is
`npc_tools::snapshot_check::assert_resume_equivalent(make, at, run, every)`: three apps from `make`, one
never saved, one saved at `at` (proves saving does not change the sim), one loaded from that snapshot (via a
JSON string); their state hashes must agree right after the load and every `every` ticks. A mismatch is
narrowed to the first differing tick and printed with `replay::entity_diffs` (loaded vs never saved). The
testbed (`content/testbed_content/tests/snapshot.rs`) and the village (`content/village_content/tests/snapshot.rs`)
run it for every embedded scenario, saves mid-walk, mid-craft, waiting on a key, mounted, and with pending
commands.

- **Explicit names everywhere.** Every entry is registered under a stable snake_case name. Registering the
  same type under the same name again is a no-op (a non-unique plugin such as `NpcLocomotionPlugin`, or a
  component that is saved before a capability starts hashing it under the same name); the same name for
  another type, or twice with custom functions, still panics.
- **Saved as serde values:** the hashed components (automatic), `brain_state`, `active_leaf`, `blackboard`,
  `brain_stats`, `think_request`, `host_position`, `move_intent`, `move_status`, `external_mover` (saved
  whether or not `register_locomotion_hash` hashes them), `pause_decay`, `provides`, `approach`,
  `external_controller`, `external_queue`, content's unhashed identity components (the village's `villager`,
  `place`, `home`, `bed`, `work_site`, `well`) and spawn mirrors (`village_ids`, `testbed_ids`).
- **Saved with custom functions:** `brain` (the tree's name, resolved to the build's `TreeId` on load),
  `command_queue` (pending `(tick, seq, command)` and `applied_through`; replaces the build's queue),
  `reservations` (a list: JSON keys must be strings), `spatial_grid` (geometry, static layer, the dynamic
  layer as of the last update, unreported static changes), `nav_queue` (requests, the paused head search with
  its open list and the scratch stamps it owns, the scratch's length and generation; the `GridChanged` cursor
  starts at the current end of the buffer, which a load empties), `pending_noises` (the buffered
  `Messages<Noise>`), `agent_io_state` (handle and controller id counters; the epoch is not saved: a loaded
  world is a new world instance). `door_cells` and `key_replan_pending` carry
  change-detection state (see "Change detection after a load").
- **Rebuilt:** the spatial grid's index (`spatial_grid` rebuild) and `DoorCells` (`door_cells`, from the
  saved synced cells; see below). The grid is
  also marked for one re-sync: the first `spatial_update_system` after a load re-indexes every entity and
  recomputes the dynamic layer against the saved one, so the `GridChanged` cells it writes are exactly those
  an uninterrupted run writes (blocker moves between the last update and the save included).
- **Not saved (build or tooling state):** registries and statics, configuration resources
  (`*Config`, `NavBudget`, `SimConfig`, `WorldSeed`), `VillagerTree`, `ExternalActionId`, `RollCounters`
  (per-tick, empty in effect at a boundary), the journal, `AiTrace`, `CommandLog`, `SkippedCommands`,
  `SkippedSeen`, agent events and stats, `NavStats`, `PrevSimPos` (set to `SimPos`; it is overwritten before
  any sim system reads it). A recorder started after a resume therefore only knows the commands applied since.
- **`SimId`s are never handed out twice:** after the entities are restored, `next = max(next, max id + 1)`, so
  a lenient load without `sim_id_allocator` is safe. Ids are range-checked (`1..=u32::MAX`) before the world
  is touched.

### Change detection after a load

A loaded world inserts every component anew, so to systems that track changes (`spatial_update_system`,
`door_sync_system`, `key_replan_system` and any content `Changed<..>`/`Added<..>` query) everything would look
added, which an uninterrupted run never sees. The brief asked to set every `NpcSimTick` system's last-run
tick to the world's change tick. **Bevy 0.20 has no public mutable access to an initialized schedule's
systems** (`Schedule::systems` returns shared references; the executable is crate-private), so the last-run
ticks cannot be moved. The equivalent, done in `load` before the rebuild functions: every component of every sim entity gets
its added and changed ticks set to `change_tick - MAX_CHANGE_AGE` (`MutUntyped::set_last_added`). Change
ages are clamped to `MAX_CHANGE_AGE`, so such a tick is never newer than any system's last run (a system
that never ran counts as exactly that old, and equal is not newer). `World::clear_trackers` twice drops the
removal buffers of the replaced build. The first tick after a load sees only its own changes, and
`World::query` change filters in exclusive systems (which use the system's last run too) behave the same.
Immutable components cannot be borrowed mutably and are skipped; no sim system change-detects one.

The settle step alone would lose a change made *after* a change-detecting system ran in the saved tick,
which an uninterrupted run sees on the next tick. Every such system (a sweep of `Changed<`, `Added<` and
`RemovedComponents` in the sim crates and content finds exactly these three) carries that state over
explicitly, so the invariant holds for any save tick:

- `spatial_update_system`: the saved dynamic layer plus one re-sync (above).
- `door_sync_system`: the `door_cells` entry saves each synced cell with its door's `SimId`, or none when
  that door was removed (or lost `Door`) after the saved tick's sync. The rebuild maps every door to the cell
  it was synced to, keeps removed doors' cells as orphans and asks for one full re-sync; the first
  `door_sync_system` after the load clears the orphans (exactly what the removal does in an uninterrupted
  run, in the same tick, so the static change is reported by the same later `GridChanged`) and syncs every
  door (a no-op for unchanged doors, `set_static` is idempotent; a door moved or changed since the sync is
  synced from its old cell as it would have been).
- `key_replan_system` (`Changed<Inventory>` / `Changed<Equipment>`): `save` reads its last-run tick through
  `Schedules` → `Schedule::systems()` (shared references) → `System::get_last_run`, finding it by
  `System::system_type` (no debug names needed), and the `key_replan_pending` entry lists the sim entities
  whose `Inventory` / `Equipment` changed after that (all of them when it never ran). The rebuild marks
  those components changed again. For that, **rebuild functions run after the settle step** (load steps 7
  and 8). Nothing else reads these changes.

Tests that save at exactly those moments (a door despawned in `NpcSet::Effects`, a key put straight into a
failed-path agent's inventory in `NpcSet::Effects`) pass `assert_resume_equivalent` for saves just before,
at and after them, and fail without the fix. Resource change detection is not settled: no sim system reads
it. A new change-detecting sim system must carry its state over the same way (the invariant tests are the
check).

## Addendum: grid footprints are saved without hashing

Added after a docs review (2026-10-25). `grid_blocker` and `grid_occluder` were saved only through
`register_spatial_hash` (called by `npc_nav`, `npc_perception`, `npc_interaction` and `npc_command`), so a host
with `SpatialPlugin` and none of those lost every `GridBlocker` / `GridOccluder` on a load, and the first
`spatial_update_system` re-sync then cleared their cells. `SpatialPlugin` now registers both for snapshots
under the same names, like `external_mover` in the Task 2 list (saved whether or not they are hashed); the
later `register_sim_component` of the same type and name is a no-op, in either order. The content hash and
the replays do not change. Test: `crates/npc_kernel/tests/snapshot.rs`
(`spatial_plugin_saves_blockers_and_occluders_with_or_without_hashing`).
