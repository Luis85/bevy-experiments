### Task 5: `fl_content` — kinds, mods, link pass, rules, rules hash, `ContentDb`

**Files:** Create `crates/fl_content/src/{kinds,builtin,mods,rules,hash,db}.rs`, modify `link.rs` and `registry.rs`, and create `crates/fl_content/tests/load.rs` with fixtures in `crates/fl_content/tests/fixtures/{mod_a,mod_b,bad_*,…}/`.

**Interfaces — Consumes:** Task 4. **Produces:**
```rust
pub trait ContentKind: serde::de::DeserializeOwned + serde::Serialize + std::fmt::Debug + Send + Sync + 'static {
    const EXT: &'static str;            // file suffix: "faction" ⇒ *.faction.ron
    const IN_RULES_HASH: bool;
    const NAMESPACED: bool = true;      // id must start with "<mod_id>."
    fn id(&self) -> &str;
    fn link(&self, _ctx: &mut LinkCtx<'_>) {}
}
pub trait RulesSection: serde::de::DeserializeOwned + serde::Serialize + std::fmt::Debug + Clone + Send + Sync + 'static {}
impl DefRegistry {
    pub fn with_builtins() -> Self;      // object kind "unit" + kinds Faction, MapDef, ScenarioDef, VisualDef
    pub fn register_kind<K: ContentKind>(&mut self);
    pub fn register_rules_section<S: RulesSection>(&mut self, name: &'static str);   // every registered section is required
}
impl LinkCtx<'_> {
    pub fn unit_exists(&self, id: &str) -> bool;        // concrete or abstract
    pub fn unit_is_abstract(&self, id: &str) -> bool;
    pub fn kind_get<K: ContentKind>(&self, id: &str) -> Option<&K>;
}
pub struct KindTable<K> { /* items sorted by id */ }
impl<K: ContentKind> KindTable<K> { pub fn get(&self, id: &str) -> Option<&K>; pub fn index_of(&self, id: &str) -> Option<u32>; pub fn iter(&self) -> impl Iterator<Item = &K>; pub fn len(&self) -> usize; pub fn is_empty(&self) -> bool; }
pub struct ModManifest { pub id: String, pub version: String, pub depends: Vec<String>, pub load_after: Vec<String>, pub dir: PathBuf }
pub struct CreditEntry { pub asset: String, pub title: String, pub author: String, pub license: String, pub url: String }
pub struct ContentDb { /* … */ }
impl ContentDb {
    pub fn units(&self) -> &[ResolvedObject];                 // sorted by id; index = UnitTypeId
    pub fn unit(&self, id: UnitTypeId) -> Option<&ResolvedObject>;
    pub fn unit_id(&self, id: &str) -> Option<UnitTypeId>;
    pub fn kind<K: ContentKind>(&self) -> &KindTable<K>;      // panics if K not registered
    pub fn rules<S: RulesSection>(&self) -> &S;               // panics if S not registered
    pub fn registry(&self) -> &DefRegistry;
    pub fn mods(&self) -> &[ModManifest];
    pub fn credits(&self) -> &[CreditEntry];
    pub fn warnings(&self) -> &[String];
    pub fn rules_hash(&self) -> u64;
}
pub fn load_dirs(registry: DefRegistry, dirs: &[PathBuf]) -> Result<ContentDb, ContentErrors>;
impl ContentDb { pub fn load_scenario_file(&self, path: &Path) -> Result<ScenarioDef, ContentErrors>; }  // parse a standalone *.scenario.ron and run its link checks against this db (agent CLI, T20)
// builtin.rs (pure data, no Bevy)
pub struct Faction { pub id: String, pub name: String, pub color: (f32, f32, f32), pub roster: Vec<String> }                // EXT "faction"; link: roster units exist and are concrete
pub struct MapDef { pub id: String, pub name: String, pub size_m: FxVec2, pub cell_m: Fx, pub ground_color: (f32, f32, f32) } // EXT "map"; link: 0 < size ≤ 4096 m, cell_m > 0, size divisible by cell_m
pub struct ScenarioDef { pub id: String, pub map: String, pub seed: u64, pub players: Vec<ScenarioPlayer>, pub units: Vec<ScenarioUnit> } // EXT "scenario"
pub struct ScenarioPlayer { pub faction: String, pub team: u8, pub controller: Controller }
pub struct ScenarioUnit { pub unit: String, pub player: u8, pub pos: FxVec2, pub heading_deg: Fx }
pub struct VisualDef { pub unit: String, pub placeholder: Placeholder, pub model: Option<String>, pub scale: f32, pub recolor: bool } // EXT "visual"; IN_RULES_HASH false; NAMESPACED false; link: unit exists
pub enum Placeholder { Box { size: (f32, f32, f32) }, Tank { hull: (f32, f32, f32), turret: (f32, f32, f32), barrel_len: f32 },
                       Infantry { radius: f32, height: f32 }, Truck { cab: (f32, f32, f32), bed: (f32, f32, f32) } }   // sizes are (width, height, length)
```
- `Faction`, `MapDef` and `ScenarioDef` are `IN_RULES_HASH = true`.
- Scenario link checks:
  - the map exists
  - every faction exists
  - every unit exists and is concrete
  - `player < players.len()`
  - `pos` is inside the map (`0 ≤ pos < size`)

**Loading algorithm.**
1. Read `mod.ron` (`Mod(id, version, depends, load_after)`) in every dir.
2. Order the mods topologically by `depends ∪ load_after`, breaking ties by id. A missing `depends` target is an error; a missing `load_after` target is ignored.
3. Walk each mod's files recursively, sorted by relative path:
   - `mod.ron`.
   - `rules.ron`: `Rules({ "Section": (…) })`. A later mod replaces a section wholesale.
   - `credits.ron`: `Credits([ (…) ])`. The license must be one of `CC0-1.0`, `CC-BY-3.0`, `CC-BY-4.0`, `OFL-1.1`.
   - `*.<ext>.ron` for registered object kinds and content kinds.
   - Any other `*.ron` is the error `unknown content file type`. Non-RON files are ignored.
4. Check ids:
   - An object with `override: true` must replace an existing id from an earlier mod; otherwise it is an error.
   - Without `override`, a duplicate id is an error that names both files.
   - Namespaced ids must start with `"{mod_id}."` unless the object is an override.
5. Resolve inheritance, then intern units. More than `u16::MAX` units is an error.
6. Run the link pass: every component's `validate` and every kind's `link`.
7. Check that every registered rules section is present.
8. Warn about each concrete unit that has no `VisualDef`.
9. Collect **all** errors before returning.

**Rules hash.** xxh64 (seed 0) over one canonical RON string, built in this order:
- For each mod in load order, `mod:{id}@{version}\n`.
- Each concrete unit, sorted, as `ron::to_string` of `(id, name, tags, components)`. `components` is the `BTreeMap<String, Arc<dyn DefComponent>>`, which serializes through `serialize_trait_object!`.
- Every `IN_RULES_HASH` kind, ordered by `EXT` then id.
- The rules sections, ordered by name.

- [ ] **Step 1: Write the failing tests** in `tests/load.rs`. Fixtures use test def components and a test rules section `TestRules { gravity: Fx }`.
  - Mod order and ids:
    - `loads_two_mods_in_dependency_order`: mod_b depends on mod_a, and `db.mods()` ids are `["a","b"]`.
    - `override_replaces_unit_from_earlier_mod`.
    - `duplicate_id_without_override_is_error`: the message names both files.
    - `override_without_target_is_error`, `id_must_carry_mod_prefix`.
  - File-level errors:
    - `unknown_file_kind_is_error`: `foo.weapon.ron` when no weapon kind is registered.
    - `mod_errors_are_aggregated`: a fixture with a missing dependency, an unknown component and an unknown file kind returns `ContentErrors` with at least 3 entries.
    - `missing_rules_section_is_error`.
    - `credits_reject_nc_license`: `CC-BY-NC-4.0` is rejected.
  - Link pass:
    - `scenario_rejects_abstract_unit`, `scenario_rejects_unit_outside_map`, `faction_roster_must_exist`.
    - `unit_ids_interned_in_sorted_order`: `db.unit_id("a.alpha") == Some(UnitTypeId(0))`.
  - Rules hash, using committed fixture variants:
    - `rules_hash_stable_across_loads`.
    - `rules_hash_changes_with_stat`.
    - `rules_hash_ignores_visuals`.
    - `rules_hash_ignores_formatting`: a variant with different whitespace and field order hashes the same.
  - `missing_visual_is_warning_not_error`.
- [ ] **Step 2: Run** `cargo test -p fl_content`. Expected: FAIL.
- [ ] **Step 3: Implement** the loading algorithm and rules hash above.
- [ ] **Step 4: Run** `cargo test -p fl_content`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_content): mods, kinds, link pass, rules sections, rules hash`.

