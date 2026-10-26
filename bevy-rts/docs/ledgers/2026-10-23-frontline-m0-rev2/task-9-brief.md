### Task 9: `fl_sim` — `SimFeature` + `SimBuilder`, stable registries, classification, command handlers, `MatchHeader`

**Track:** contract + sim-core · **Refs:** ADR-0001, ADR-0004, ADR-0006, ADR-0007 · Starts after Task 8 is merged into `m0/phase-a`.

**Files:**
- Create: `crates/fl_sim/src/{registry,builder,classify}.rs`, `crates/fl_sim/tests/{feature,classify}.rs`
- Modify: `crates/fl_sim/src/{feature,build,lib,hash,inspect,players,map,units,commands,events,rng,spatial,movement,scale,scenario,spawn}.rs`. Registration moves to `SimBuilder`, and the rev-1 payload structs are replaced by the `fl_protocol` payloads plus sim-side handlers.
- Port: `crates/fl_sim/tests/{schedule,hash,commands,movement}.rs` to the new API
- Delete: `crates/fl_sim/src/app_ext.rs`

**Interfaces:**
- Consumes: Tasks 5–8.
- Produces:
```rust
// registry.rs
pub struct StableRegistry<V> { .. }
impl<V> StableRegistry<V> {
    pub fn new(table: &'static str) -> Self;
    pub fn register(&mut self, name: &'static str, registrant: &'static str, value: V);
    //   panics "'<name>' registered by '<a>' and '<b>'" on a duplicate, and "registry '<table>' is frozen" after freeze
    pub fn freeze(&mut self);                           // sort names; assign dense ids; precompute name keys = fl_math::stream_key(name)
    pub fn is_frozen(&self) -> bool;
    pub fn id(&self, name: &str) -> Option<u32>;
    pub fn name(&self, id: u32) -> Option<&'static str>;
    pub fn name_key(&self, id: u32) -> Option<u64>;
    pub fn get(&self, name: &str) -> Option<&V>;
    pub fn get_by_id(&self, id: u32) -> Option<&V>;
    pub fn iter(&self) -> impl Iterator<Item = (u32, &'static str, &V)>;   // dense order; frozen only
    pub fn export(&self, tables: &mut StringTables);
}
// feature.rs
pub trait SimFeature: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn version(&self) -> u32;
    fn deps(&self) -> &'static [&'static str] { &[] }
    fn build(&self, sim: &mut SimBuilder);
}
pub struct CoreFeature;   impl CoreFeature { pub const ID: &'static str = "core"; }   // version 1; always first; callers never list it
pub enum FeatureError { Duplicate(String), Reserved(String), MissingDep { feature: String, dep: String }, Cycle(Vec<String>) }
pub fn sort_features(features: &[Arc<dyn SimFeature>]) -> Result<Vec<Arc<dyn SimFeature>>, FeatureError>;
//   Kahn's algorithm with the ready set ordered by id; core first; Cycle lists the remaining ids, sorted
// classify.rs
pub enum Class<T> { Hashed(Codec<T>), Derived { rebuild: fn(&mut World) }, Transient, External }
impl<T: SimHash + Serialize + DeserializeOwned + Send + Sync + 'static> Class<T> { pub fn hashed(version: u32) -> Class<T>; }
pub struct Codec<T> { pub version: u32, /* hash, encode (postcard), decode, dump (serde_json) fns */ }
pub fn check_classification(world: &World) -> Result<(), Vec<String>>;
//   every registered component/resource type whose outermost path starts with "fl_sim::" or "fl_feat_" (not a "_client" crate)
//   must be in the state registry; returns the sorted unclassified type names
// builder.rs
pub struct SimBuilder { .. }
impl SimBuilder {
    pub fn feature_id(&self) -> &'static str;
    pub fn register_def_component<T: DefComponent + DeserializeOwned>(&mut self, name: &'static str) -> &mut Self;
    pub fn register_content_kind<K: ContentKind>(&mut self) -> &mut Self;
    pub fn register_rules_section<S: RulesSection>(&mut self, name: &'static str) -> &mut Self;
    pub fn register_component<C: Component>(&mut self, name: &'static str, class: Class<C>) -> &mut Self;   // External panics: "External is for resources"
    pub fn register_resource<R: Resource>(&mut self, name: &'static str, class: Class<R>) -> &mut Self;
    pub fn register_command<P: CommandPayload>(&mut self, handler: impl CommandHandler<P>) -> &mut Self;
    pub fn register_stream(&mut self, name: &'static str) -> &mut Self;
    pub fn add_systems<M>(&mut self, set: SimSet, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) -> &mut Self;   // Task 10 swaps SimSet for SimHook
    pub fn app_mut(&mut self) -> &mut App;   // escape hatch; review flags every use
}
// commands.rs (the handler half of the split)
pub trait CommandHandler<P: CommandPayload>: Send + Sync + 'static {
    fn unit_can(&self, _payload: &P, _unit: EntityRef<'_>) -> bool { true }
    fn apply(&self, payload: &P, world: &mut World, ctx: &CommandCtx<'_>) -> Result<(), RejectReason>;
}
pub struct CommandCtx<'a> { pub tick: Tick, pub player: PlayerId, pub seq: u32, pub units: &'a [(SimId, Entity)], pub queue: bool }
// build.rs
#[derive(Clone, Debug, Default)] pub struct MatchSetup { pub seed: u64, pub cheats_allowed: bool }
#[derive(Clone, Debug, Default)] pub struct RunConfig { pub hash_interval: Option<u32> }   // later tasks add fields; construct it with ..Default::default()
pub enum BuildError { Feature(FeatureError), Content(ContentErrors), Unclassified(Vec<String>), RegistryMismatch }
pub const SIM_VERSION: u32 = 1;
pub fn build_id() -> String;   // "<CARGO_PKG_VERSION>+<FL_GIT_REV>" when option_env!("FL_GIT_REV") was set at compile time, else "dev"
pub fn sim_registry(features: &[Arc<dyn SimFeature>]) -> Result<DefRegistry, BuildError>;
pub fn load_content(features: &[Arc<dyn SimFeature>], mods: &[PathBuf]) -> Result<Arc<ContentDb>, BuildError>;
pub fn build_sim_app(features: &[Arc<dyn SimFeature>], content: Arc<ContentDb>, setup: MatchSetup, run: RunConfig) -> Result<App, BuildError>;
//   sort → build each feature through SimBuilder → freeze every registry in finish() → build the MatchHeader
//   (features, mods, rules hash, seed, cheats flag, string tables from fl_content and every StableRegistry, build_id, SIM_VERSION)
//   → classification check (cfg!(debug_assertions)) → initialize SimTick
#[derive(Resource)] pub struct MatchHeaderRes(pub MatchHeader);   // External "core.match_header"
```
- **Core registrations.** The names are a contract; Tasks 10–13 add theirs.
  - Components:
    - Hashed v1: `core.owner`, `core.unit_type`, `core.pose`, `core.health`, `core.footprint`, `core.mobility`, `core.move_order`, `core.wander`, `core.selectable`.
    - `core.sim_entity` is Derived with a no-op rebuild. It is structural: load recreates it, and the hash and the save use its id as the entity key.
  - Resources:
    - Hashed: `core.clock`, `core.sim_id_allocator`, `core.players`, `core.map`, plus `core.rng` (`SimRng`, until Task 10) and `core.command_queue` (until Task 11).
    - Derived: `core.sim_index`.
    - External: `core.content`, `core.match_header`, `core.def_registry`, `core.command_registry`.
    - Transient: `core.command_log`, `core.sim_events`.
  - Commands: `core.move`, `core.stop`, `debug.spawn` and `debug.wander`, with handlers that keep the rev-1 behaviour. `debug.spawn` now resolves the unit name. Task 11 adds the rest.
  - Streams: `debug.wander`, `setup.scale`.
- **Inspection:** `inspect_entity` keys components by registry name, as `{"sim_id": n, "components": {"core.pose": …}}`. `inspect_world` adds `"seed"` and `"header": {features, mods, rules_hash}`.

- [ ] **Step 1: Write the failing tests:**
```rust
// tests/feature.rs
#[test] fn features_sort_topologically_ties_by_id()   // [c, b(deps a), a] → ids ["core", "a", "b", "c"]
#[test] fn feature_cycle_is_error()                   // a(deps b), b(deps a) → Err(Cycle(vec!["a", "b"]))
#[test] fn missing_dependency_is_error()              // b(deps z) → Err(MissingDep { feature: "b", dep: "z" })
#[test] fn duplicate_feature_id_is_error()
#[test] fn core_id_is_reserved()                      // a test feature with id "core" → Err(Reserved("core"))
#[test] fn header_lists_features_mods_and_tables()
//   header.features == [("core", 1), ("test", 3)]; header.sim_version == SIM_VERSION; header.rules_hash == content.rules_hash();
//   header.cheats_allowed == setup.cheats_allowed; header.seed == setup.seed;
//   string_tables.get("commands") == ["core.move", "core.stop", "debug.spawn", "debug.wander"]
#[test] fn registry_ids_sorted_by_name()              // register "b", "a", "c" → id("a") == 0, id("c") == 2, name_key(0) == stream_key("a")
#[test] #[should_panic(expected = "is frozen")] fn register_after_freeze_panics()
#[test] #[should_panic(expected = "registered by 'test' and 'other'")] fn duplicate_name_panics_naming_both()
// tests/classify.rs
#[test] fn core_app_is_fully_classified()             // build_sim_app(core_mod) → Ok
#[test] #[should_panic(expected = "External is for resources")] fn external_component_panics()
#[test] fn classified_names_in_tables()               // "sim_components" contains "core.pose"; "sim_resources" contains "core.players"
// src/classify.rs (a unit test, so the stray type's path starts with fl_sim::)
#[test] fn unclassified_sim_type_fails_build()        // a feature inserts an unregistered `Stray` → Err(Unclassified(vec!["fl_sim::classify::tests::Stray"]))
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: FAIL.
- [ ] **Step 3: Implement**, and port the existing tests. Behaviour doesn't change; only names and APIs do.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract --track sim-core`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_sim)!: SimFeature + SimBuilder, stable registries, state classification, command handlers, MatchHeader` with trailer `Refs: ADR-0001, ADR-0004, ADR-0006, ADR-0007`.

