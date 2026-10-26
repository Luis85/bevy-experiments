### Task 6: `fl_sim` skeleton — schedule, ids, clock, RNG, state hash, features

**Files:**
- Create `crates/fl_sim/src/{lib,schedule,clock,ids,rng,hash,events,feature,app_ext,build,players,map,spatial}.rs` and `crates/fl_sim/tests/{schedule,hash}.rs`.
- Create `crates/fl_sim/tests/fixtures/core_mod/{mod.ron,rules.ron}`. Task 7 adds units and maps there.
- `lib.rs` declares every module in the file map and sets `#![deny(clippy::float_arithmetic)]`. `commands`, `spawn`, `units`, `movement`, `scenario` and `scale` start as empty files for Task 7.

**Interfaces — Produces:**
```rust
// schedule.rs
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)] pub struct SimTick;
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimSet { ApplyCommands, Orders, Behaviour, Movement, Combat, Damage, Vision, Lifecycle, Hash }
pub fn configure_sim_schedule(app: &mut App);  // chain(), SingleThreaded, ambiguity_detection: LogLevel::Error
// clock.rs
#[derive(Resource, Default)] pub struct SimClock { pub tick: Tick }   // tick being simulated; `advance_clock` (last system of SimSet::Hash) increments it
// ids.rs
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)] #[component(immutable, on_add = …, on_remove = …)]
pub struct SimEntity(pub SimId);          // hooks keep SimIndex in sync
#[derive(Resource, Default)] pub struct SimIdAllocator { next: u32 }   // first id = 1; fn alloc(&mut self) -> SimId
#[derive(Resource, Default)] pub struct SimIndex { /* Vec<(SimId, Entity)> sorted */ }
impl SimIndex { pub fn get(&self, id: SimId) -> Option<Entity>; pub fn iter(&self) -> impl Iterator<Item = (SimId, Entity)> + '_; pub fn len(&self) -> usize; pub fn is_empty(&self) -> bool; pub fn ids(&self) -> impl Iterator<Item = SimId> + '_ }
// rng.rs
#[derive(Resource)] pub struct SimRng(pub Pcg32);   // stream 0xF1; fn reseed(&mut self, seed: u64)
// hash.rs
pub struct StateHasher { … }   // write_u8/u16/u32/u64/i64/bool, write_fx(Fx), write_bytes; finish() -> u64
pub trait SimHash { fn sim_hash(&self, h: &mut StateHasher); }   // impls for u8..u64, i32, i64, bool, Fx, FxVec2, Angle, SimId, PlayerId, UnitTypeId, Tick, Option<T>, Vec<T>
#[derive(Resource)] pub struct StateHashLog { pub interval: u32, pub entries: Vec<(Tick, u64)> }   // interval 0 = off
pub fn compute_state_hash(world: &World) -> u64;  // tick, rng state, entity count, then per SimIndex entry: id + each registered component (name order): 0 or 1+data
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)] #[serde(deny_unknown_fields)]
pub struct StateHashRules { pub interval_ticks: u32 }   // RulesSection "StateHash"
// events.rs
#[derive(Resource)] pub struct SimEvents<E> { /* Vec<E> */ }   // fn push(&mut self, e: E); fn drain(&mut self) -> std::vec::Drain<'_, E>; fn iter(&self)
// feature.rs
pub trait GameFeature: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn sim(&self, sim: &mut App);
    fn client(&self, _app: &mut App) {}
}
pub struct CoreFeature;   // id "core"; adds schedule, clock, ids, rng, hash, players, map, SpatialPlugin, and (Task 7) commands/units/movement
// app_ext.rs
pub trait SimAppExt {
    fn register_def_component<T: DefComponent + DeserializeOwned>(&mut self, name: &'static str) -> &mut Self;
    fn register_content_kind<K: ContentKind>(&mut self) -> &mut Self;
    fn register_rules_section<S: RulesSection>(&mut self, name: &'static str) -> &mut Self;
    fn register_hashed<C: Component + SimHash + Serialize>(&mut self, name: &'static str) -> &mut Self;  // also stores a JSON dump fn for inspection
//  inspection (hash.rs or a small inspect.rs): pub fn inspect_entity(world: &World, id: SimId) -> Option<serde_json::Value>  // {"sim_id":n,"components":{name: value}}
//                                              pub fn inspect_world(world: &World) -> serde_json::Value                      // {"tick","rng","state_hash","entities","players","map"}
//  Ruling: add `serde_json` (workspace dep) to fl_sim, and enable bevy_ecs's "trace" feature in the workspace dependency (per-system spans for the profiler, spec §15 D).
    fn add_sim_event<E: Send + Sync + 'static>(&mut self) -> &mut Self;   // inserts SimEvents<E>, clears it first thing in ApplyCommands
}
impl SimAppExt for App { … }   // the registry lives in the DefRegistry resource (inserted as DefRegistry::with_builtins())
// build.rs
#[derive(Resource, Clone, Debug)] pub struct SimConfig { pub seed: u64, pub hash_interval: Option<u32>, pub allow_debug_commands: bool }
pub fn sim_registry(features: &[Arc<dyn GameFeature>]) -> DefRegistry;  // scratch App: CoreFeature + features, take DefRegistry
pub fn load_content(features: &[Arc<dyn GameFeature>], mods: &[PathBuf]) -> Result<Arc<ContentDb>, ContentErrors>;
pub fn build_sim_app(features: &[Arc<dyn GameFeature>], content: Arc<ContentDb>, config: SimConfig) -> App;
//   panics if content.registry().fingerprint() != sim_registry(features).fingerprint(); inserts Content, SimConfig, SimRng(seed),
//   StateHashLog(interval = config.hash_interval.unwrap_or(rules StateHash.interval_ticks)); calls app.finish(); app.cleanup().
#[derive(Resource, Clone)] pub struct Content(pub Arc<ContentDb>);
// players.rs
pub struct PlayerInfo { pub id: PlayerId, pub faction: FactionId, pub team: TeamId, pub controller: Controller }
#[derive(Resource, Default)] pub struct Players { pub list: Vec<PlayerInfo> }   // fn get(&self, id) -> Option<&PlayerInfo>
// map.rs
#[derive(Resource, Clone, Debug)] pub struct MapInfo { pub size: FxVec2, pub cell: Fx, pub cells_x: u32, pub cells_y: u32 }
impl MapInfo { pub fn flat(size: FxVec2, cell: Fx) -> Self; pub fn contains(&self, p: FxVec2) -> bool /* 0 ≤ p < size */; pub fn clamp(&self, p: FxVec2) -> FxVec2 /* into [0, size − EPSILON] */ }
// spatial.rs (stub; Task 9 implements)
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)] #[serde(deny_unknown_fields)] pub struct SpatialRules { pub cell_m: Fx }  // RulesSection "Spatial"
pub struct SpatialPlugin;  // build(): registers the "Spatial" rules section only
```
Headless stepping is `app.world_mut().run_schedule(SimTick)`. The sim never calls `app.update()`.

- [ ] **Step 1: Write the failing tests.**
  - `tests/schedule.rs` uses a test feature whose systems push their set name into a `Trace(Vec<&'static str>)` resource.
    - `sets_run_in_declared_order`: after one tick, the trace equals the 9 set names in order.
    - `clock_advances_per_tick`: after 3 ticks, `SimClock.tick == Tick(3)`.
    - `sim_index_tracks_spawn_and_despawn`: spawn 3 `SimEntity`, despawn the middle one, and the ids are `[1, 3]`.
    - `ambiguous_systems_fail_schedule_build`: `#[should_panic]`, with two unordered systems writing the same resource in `SimSet::Orders`.
    - `feature_registers_def_component`: `sim_registry(&[test]).component_name::<TestDef>() == Some("Test")`.
    - `sim_events_cleared_each_tick`.
  - `tests/hash.rs` registers a test component `Hp(u32)` as hashed.
    - `equal_worlds_hash_equal`, `component_change_changes_hash`, `rng_draw_changes_hash`.
    - `hash_log_records_every_interval`: interval 5 over 20 ticks records ticks `[0,5,10,15]`.
    - `content_fingerprint_mismatch_panics`: `#[should_panic]`.
  - Fixture `core_mod`:
    - `mod.ron`: `Mod(id: "core_test", version: "0.1.0", depends: [], load_after: [])`.
    - `rules.ron`: `Rules({ "StateHash": (interval_ticks: 20), "Spatial": (cell_m: 32.0) })`.
- [ ] **Step 2: Run** `cargo test -p fl_sim`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - `advance_clock` and the hash system are exclusive systems, chained inside `SimSet::Hash` with the hash first.
  - Hash registration stores `(name, fn(&World, Entity, &mut StateHasher) -> bool)`, sorted by name.
- [ ] **Step 4: Run** `cargo test -p fl_sim`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_sim): SimTick schedule, SimId index, clock, RNG, state hash, GameFeature`.

