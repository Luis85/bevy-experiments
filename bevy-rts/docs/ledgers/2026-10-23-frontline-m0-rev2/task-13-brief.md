### Task 13: Save/load skeleton with typed migrations, round-trip and branch tests

**Track:** contract + sim-core · **Refs:** ADR-0006, ADR-0004

**Files:**
- Create: `crates/fl_sim/src/save/{mod,format,load}.rs`, `crates/fl_sim/tests/save.rs`
- Modify: `crates/fl_sim/src/{classify,units,runner/mod,ids,lib}.rs`

**Interfaces:**
- Consumes: the state registry and codecs (Task 9), the spawn path (Task 11) and `MatchHeader::check_save` (Task 6).
- Produces:
```rust
pub const SAVE_FORMAT_VERSION: u32 = 1;
#[derive(Serialize, Deserialize)] pub struct SaveFile { pub format_version: u32, pub header: MatchHeader, pub sim: SimSave }
#[derive(Serialize, Deserialize)] pub struct SimSave { pub tick: Tick, pub sim_id_next: u32, pub pending_commands: Vec<PlayerCommand>,
    pub resources: Vec<(String, u32, Vec<u8>)>,                 // Hashed resources, sorted by name
    pub entities: Vec<(SimId, Vec<(String, u32, Vec<u8>)>)> }   // sorted by SimId, then component name
pub fn save_world(world: &World, pending: &[PlayerCommand]) -> Vec<u8>;   // postcard
pub fn load_world(features: &[Arc<dyn SimFeature>], content: Arc<ContentDb>, bytes: &[u8], run: RunConfig) -> Result<Loaded, LoadError>;
pub struct Loaded { pub app: App, pub pending: Vec<PlayerCommand>, pub last_seq: BTreeMap<PlayerId, u32> }
pub enum LoadError { Decode(String), FormatVersion { found: u32, supported: u32 }, Header(HeaderMismatch), UnknownComponent(String),
    UnknownResource(String), NoMigration { name: String, from: u32, to: u32 }, Codec { name: String, message: String }, Build(BuildError) }
impl<T> Codec<T> {
    pub fn migrate_from<Old: DeserializeOwned + Into<T>>(self, version: u32) -> Self;   // decodes bytes saved at `version`, then .into()
    pub fn custom(version: u32, encode: fn(&T, &SaveCtx) -> Vec<u8>, decode: fn(u32, &[u8], &LoadCtx) -> Result<T, String>) -> Self;
}
pub struct SaveCtx<'a> { .. }
pub struct LoadCtx<'a> { .. }   impl LoadCtx<'_> { pub fn remap(&self, table: &str, old: u32) -> Result<u32, String>; }   // saved tables → current tables
```
- **Load procedure:**
  1. Decode, and check `format_version`.
  2. Build a fresh app with `MatchSetup { seed: header.seed, cheats_allowed: header.cheats_allowed }`.
  3. Run `current_header.check_save(&saved.header)`. The rules hash is not compared, so a hot-reload rebuild can load against new content.
  4. Spawn the entities in `SimId` order with their saved ids, through the spawn path (`pub(crate) fn spawn_with_id`).
  5. Decode components and resources by name, migrating by version.
  6. Set `SimIdAllocator` and `SimClock`.
  7. Run every `Derived` rebuild in registry-name order.
- `UnitType` uses `Codec::custom`: it saves the dense id and remaps it through `tables::UNITS` on load.
- The runner answers `SimQuery::Save { pending }` with `SimReply::Save(save_world(..))`.

- [ ] **Step 1: Write the failing tests** (`tests/save.rs`; hash interval 1):
```rust
#[test] fn round_trip_is_byte_identical()        // duel + wander, 50 ticks: s1 = save; load; s2 = save → s1 == s2
#[test] fn save_at_tick_zero_round_trips()
#[test] fn branch_test_hashes_match_every_tick() // A runs 0..120 with moves at 10 and 60; B runs to 40, saves, loads fresh and feeds the same inputs to 120 → hashes equal for 40..120
#[test] fn branch_after_despawns()               // two units despawned before tick 40 (SimId gaps) → branch hashes equal
#[test] fn pending_commands_survive_save()       // TestDriver delay 2 holds a move; save at that point; load + driver.restore → applied at the same exec_tick, same hashes
#[test] fn load_rejects_unknown_component_name() // rename "core.pose" to "core.posee" in the decoded SaveFile and re-encode → Err(UnknownComponent("core.posee"))
#[test] fn load_rejects_feature_mismatch()       // saved with an extra feature → Err(Header(HeaderMismatch::Features))
#[test] fn migration_from_v1()                   // a test component at Codec v2 .migrate_from::<v1::TestHp>(1); a save holding v1 bytes → the migrated value
#[test] fn load_remaps_unit_type_ids()           // saved with core_mod, loaded with core_mod_extra → UnitType names preserved; equal state hash
#[test] fn derived_state_rebuilt_on_load()       // SimIndex after load == SimIndex before save
#[test] fn runner_save_query()                   // SimQuery::Save { pending: vec![] } → SimReply::Save(bytes), and load_world(bytes) is Ok
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test save`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract --track sim-core`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_sim): versioned save/load with typed migrations, round-trip and branch tests` with trailer `Refs: ADR-0006`. The controller then runs `cargo xtask gate pr` and fast-forwards `main`; the contracts are in place.

---

