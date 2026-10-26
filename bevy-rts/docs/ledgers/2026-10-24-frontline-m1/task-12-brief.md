### Task 12: Hot reload, sim side — ID-set comparison, in-place values, runner control, lenient load

**Track:** contract + sim-core + content + client · **Refs:** ADR-0012 (refinement), ADR-0006 · **Lane 1** (after Task 11)

**Files:**
- Modify:
  - `crates/fl_content/src/{db,def}.rs`, `crates/fl_content/tests/load.rs`
  - `crates/fl_view/src/{runner,lib}.rs`, `crates/fl_view/tests/view.rs`
  - `crates/fl_sim/src/{units,lib}.rs`, `crates/fl_sim/src/runner/{mod,threaded}.rs`, `crates/fl_sim/src/save/{mod,load}.rs` (`rebuild_derived` becomes `pub(crate)` so `reload.rs` runs the same rebuilds)
  - `crates/fl_client/tests/{debug,view}.rs`: their fake runners match `ToRunner` exhaustively and fail to compile once it gains a variant, so each gets a `_ => {}` arm. This is the only client change.
- Create: `crates/fl_sim/src/reload.rs`, `crates/fl_sim/tests/reload.rs`
- Blessed: API (`fl_content`, `fl_view`, `fl_sim`), view formats (if `SimReply` is traced).

**Interfaces:**
- Consumes: `ContentDb::string_tables`, `DefRegistry` tables, `rebuild_derived` (save/load: every feature's one `Derived` rebuild in feature load order, R74), `MatchHeaderRes`, `Content`, `save_world` (returns `Result<_, SaveError>`).
- Produces:
```rust
// fl_content
impl ContentDb {
    pub fn same_id_sets(&self, other: &ContentDb) -> bool;            // every content and registry string table holds the same names
    pub fn id_set_diff(&self, other: &ContentDb) -> Vec<String>;      // sorted lines "content.unit: +base.de.pz4", "content.terrain: -base.sand"
}
trait DefComponent { fn reload(&self, ctx: &SpawnCtx<'_>, entity: &mut EntityWorldMut<'_>) { self.instantiate(ctx, entity) } }
// fl_sim units.rs: HullDef::reload keeps the HP ratio — hp = if old hp == 0 { 0 } else { max(1, old_hp × new_max / old_max) }; Footprint replaced.
//                  MobilityDef::reload replaces Mobility only (MoveOrder and PathFollow kept). SelectableDef keeps the default.
// fl_sim/src/reload.rs
pub fn apply_content_values(world: &mut World, content: Arc<ContentDb>) -> Result<ReloadReport, ReloadError>;
    // needs cheats_allowed and same_id_sets; reloads every unit's def components (SimIndex order, component-name order); replaces Content and
    // MatchHeaderRes.rules_hash; runs rebuild_derived (every feature's one rebuild, feature load order); journals "core.content_reload"
    // { old_rules_hash, rules_hash, units }
#[derive(Debug, Clone, PartialEq, Eq)] pub struct ReloadReport { pub tick: Tick, pub old_rules_hash: u64, pub rules_hash: u64, pub units_reloaded: u32 }
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)] pub enum ReloadError { #[error("hot reload needs cheats_allowed")] CheatsOff,
    #[error("the content's ids changed: {0:?}")] IdSetsDiffer(Vec<String>), #[error("the content was loaded with another feature set")] RegistryMismatch }
// fl_view runner.rs
#[non_exhaustive] pub enum ToRunner { Tick(TickInput), Query { id: u64, query: SimQuery }, Control { id: u64, control: RunnerControl }, Shutdown }
    // non-exhaustive from now on (ADR-0012 refinement), like SimQuery and SimReply; matches outside fl_view need a wildcard arm
#[derive(Debug, Clone, PartialEq)] #[non_exhaustive] pub enum RunnerControl { ReloadContent(OpaqueContent) }
#[derive(Clone)] pub struct OpaqueContent(pub Arc<dyn Any + Send + Sync>);   // Debug prints "OpaqueContent"; PartialEq is Arc::ptr_eq
SimReply::Reloaded { tick: Tick, rules_hash: u64 }
// fl_sim runner
impl SimRunner { pub fn control(&mut self, control: RunnerControl) -> SimReply; }   // ReloadContent: downcast to ContentDb → apply_content_values →
                                                                                     // Reloaded, or Error(text); the threaded loop answers FromRunner::Reply
// fl_sim save
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)] pub struct LoadOptions { pub drop_unknown_units: bool }
pub fn load_world_with(features: &[Arc<dyn SimFeature>], content: Arc<ContentDb>, bytes: &[u8], run: RunConfig, opts: LoadOptions) -> Result<Loaded, LoadError>;
    // load_world(..) == load_world_with(.., LoadOptions::default()); with drop_unknown_units an entity whose UnitType name is unknown is skipped
    // (its SimId stays unused) and listed in Loaded.dropped, with a warn log. check_save ignores the rules hash (R70), so a save of the
    // running match loads under the edited content; no SimEntity is created for a skipped entity (the load path is the only other creator, R73)
pub struct Loaded { .., pub dropped: Vec<(SimId, String)> }
// any load: a TerrainGrid palette name with no TerrainDef in the content → LoadError::Codec { name: "core.terrain", message: "unknown terrain '<id>'" }
// apply_content_values, after the rebuilds: in SimIndex order, every unit whose movement profile changed (class or size) and whose PathFollow is
//   Waiting, Following or Flow → Idle, journaled order_changed "replanned"; advance_orders re-requests it (the old request turns stale)
// neither reload path rewrites TerrainGrid: edited map layers change the content and rules hash only (ADR-0012 refinement)
```

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_content/tests/load.rs
#[test] fn same_id_sets_true_for_value_change()
#[test] fn same_id_sets_false_when_unit_added()   // diff == ["content.unit: +a.gamma"]
#[test] fn terrain_kind_added_changes_id_set()
// crates/fl_view/tests/view.rs
#[test] fn opaque_content_eq_is_identity()
// crates/fl_sim/tests/reload.rs
#[test] fn value_reload_keeps_ids_and_hp_ratio()   // tank 50/100 → content with hp 200 → 100/200; SimIds, poses, MoveOrder unchanged
#[test] fn value_reload_changes_speed()            // road_speed 36 → 72 km/h: the next tick's step doubles
#[test] fn value_reload_updates_passability()      // forest gains blocks [Tracked]: passable() flips for t.tank; HpaGraphs == a fresh build
#[test] fn reload_refused_without_cheats()         // Err(CheatsOff); state hash unchanged
#[test] fn reload_refused_when_ids_differ()        // Err(IdSetsDiffer(["content.unit: +t.extra"]))
#[test] fn reload_journals_content_reload()
#[test] fn reload_is_deterministic()               // two sims reloaded at tick 50 → equal hash streams to tick 200
#[test] fn rebuild_drops_vanished_unit_types()     // save_world(..).expect("no pending spawns"); content without t.truck; load_world_with(drop) → trucks in
                                                   //   `dropped`; survivors keep SimIds; runs on (twin-equal)
#[test] fn load_without_drop_rejects_unknown_unit() // load_world → Err(LoadError::Codec { name: "core.unit_type", .. })
#[test] fn threaded_runner_applies_reload()        // spawn_threaded; Control ReloadContent → Reply Reloaded { rules_hash: new }; the next tick uses the new speed
#[test] fn rebuild_with_vanished_terrain_is_load_error()   // a save whose TerrainGrid names "t.swamp"; content without it → Err(LoadError::Codec { name: "core.terrain", .. })
#[test] fn reload_changing_profile_replans()       // t.tank width 3.0 → 7.0 while following a path: "replanned"; its next plan uses size 2; HpaGraphs == a fresh build
#[test] fn value_reload_keeps_terrain_grid()       // new content with one map pixel changed: TerrainGrid unchanged, rules hash changed
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_content -p fl_view -p fl_sim -E 'test(/reload|same_id|id_set|opaque|drop|terrain_kind_added/)'`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_content -p fl_view -p fl_sim -p fl_client`. Expected: PASS. `fl_client`'s sources only send `ToRunner` values and never match one, so they compile unchanged. Its two test fake runners gain the wildcard arm (Files) and never receive a `Control`.
- [ ] **Step 5: Bless (lead)** API (and formats if they changed): `M1 Task 12: hot reload runner control, lenient load`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract --track sim-core --track content --track client`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(fl_sim): hot reload — ID-set check, in-place value reload keeping ratios, runner control message, lenient load` with trailers `Refs: ADR-0012` and `Blessed: …`.

