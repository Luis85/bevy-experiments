### Task 4: `fl_content` — registry, object parsing, inheritance

**Files:** Create `crates/fl_content/src/{lib,error,registry,def,object,inherit,link}.rs`, `crates/fl_content/tests/objects.rs`, and fixtures under `crates/fl_content/tests/fixtures/objects/`.

**Interfaces — Produces:**
```rust
// error.rs
pub struct ContentError { pub file: PathBuf, pub line_col: Option<(usize, usize)>, pub path: String, pub message: String }
//   Display: "{file}:{line}:{col}: {path}: {message}" (omit missing parts)
pub struct ContentErrors(pub Vec<ContentError>);   // Display: one error per line; impl std::error::Error

// def.rs
pub trait DefComponent: erased_serde::Serialize + std::fmt::Debug + Send + Sync + 'static {
    fn validate(&self, _ctx: &mut LinkCtx<'_>) {}
    fn instantiate(&self, ctx: &SpawnCtx<'_>, entity: &mut bevy_ecs::world::EntityWorldMut<'_>);
    fn as_any(&self) -> &dyn std::any::Any;
}
erased_serde::serialize_trait_object!(DefComponent);
pub struct SpawnCtx<'a> { pub content: &'a ContentDb, pub unit: UnitTypeId, pub player: PlayerId, pub pos: FxVec2, pub heading: Angle }
//   (ContentDb arrives in Task 5; in this task declare a minimal `pub struct ContentDb` in db.rs that Task 5 fills in.)

// link.rs (this task: the minimal surface; Task 5 extends it)
pub struct LinkCtx<'a> { … }
impl LinkCtx<'_> { pub fn error(&mut self, path: impl Into<String>, msg: impl Into<String>); }

// registry.rs
#[derive(bevy_ecs::resource::Resource, Default)]
pub struct DefRegistry { /* components: BTreeMap<&'static str, ComponentEntry>, type_names: BTreeMap<TypeId, &'static str>, object kinds, content kinds, rules sections */ }
impl DefRegistry {
    pub fn register_component<T: DefComponent + serde::de::DeserializeOwned>(&mut self, name: &'static str); // panics on duplicate name or type
    pub fn register_object_kind(&mut self, ext: &'static str);                                                  // "unit" ⇒ *.unit.ron
    pub fn component_name<T: DefComponent>(&self) -> Option<&'static str>;
    pub fn fingerprint(&self) -> u64;   // xxh64 over sorted component, object-kind, content-kind and rules-section names
}

// object.rs
pub struct ObjectDef { pub kind: &'static str, pub id: String, pub inherits: Option<String>, pub is_abstract: bool,
    pub name: Option<String>, pub tags: Vec<String>, pub is_override: bool,
    pub components: BTreeMap<String, Arc<dyn DefComponent>>, pub remove: Vec<String>, pub file: PathBuf }
pub fn parse_object(reg: &DefRegistry, kind: &'static str, file: &Path, text: &str) -> Result<ObjectDef, ContentError>;

// inherit.rs
pub struct ResolvedObject { pub kind: &'static str, pub id: String, pub name: String /* falls back to id */,
    pub tags: Vec<String> /* sorted, deduped union along the chain */,
    pub components: BTreeMap<String, Arc<dyn DefComponent>>, pub file: PathBuf }
impl ResolvedObject {
    pub fn get<T: DefComponent>(&self, reg: &DefRegistry) -> Option<&T>;   // via component_name + as_any downcast
    pub fn has(&self, name: &str) -> bool;
}
pub struct Resolved { pub concrete: Vec<ResolvedObject> /* sorted by id */, pub abstract_ids: BTreeSet<String> }
pub fn resolve_objects(raw: Vec<ObjectDef>) -> Result<Resolved, ContentErrors>;
```
The RON object schema is below. `ron` ignores the struct name, and every field except `id` is optional:
```ron
Unit(id: "base.de.pz3", inherits: Some("base.abstract.medium_tank"), abstract: false, name: "Pz.Kpfw. III",
     tags: ["armored"], override: false, components: { "Hull": (hp: 160, footprint: (2.9, 5.4), hitbox_shrink: 0.5) }, remove: ["Crew"])
```

- [ ] **Step 1: Write the failing tests** in `tests/objects.rs`. They use two test-only def components, both `#[serde(deny_unknown_fields)]`: `TestHull { hp: u32 }` (registered as `"Hull"`) and `TestSpeed { kmh: Fx }` (`"Speed"`).
  - `parses_registered_components`: `obj.components["Hull"]` downcasts to `TestHull { hp: 160 }`.
  - `unknown_component_is_error_with_line`: `"Armour": ()` gives `Err(e)` where `e.message` contains `unknown component 'Armour'`, `e.line_col.is_some()`, and `e.file` ends with the fixture name.
  - `unknown_field_is_error`: `"Hull": (hp: 1, colour: 2)` gives an error mentioning `colour`.
  - `child_replaces_parent_component_wholesale`: a parent with `Hull(hp:100)` and `Speed(kmh:10)` and a child with `Hull(hp:200)` resolve to Hull 200, Speed 10.
  - `remove_drops_inherited_component`: a child with `remove: ["Speed"]` has no Speed.
  - `three_level_chain_resolves`, `abstract_objects_are_not_output`, `tags_are_unioned_sorted`.
  - `inheritance_cycle_is_error`: a→b→a gives a message containing `cycle` and both ids.
  - `missing_parent_is_error`, `removing_absent_component_is_error`.
- [ ] **Step 2: Run** `cargo test -p fl_content --test objects`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - Object parsing uses a `serde::de::DeserializeSeed` over the whole object struct.
  - Its `components` field uses a map visitor that looks each key up in the registry, then calls that entry's `fn(&mut dyn erased_serde::Deserializer) -> Result<Arc<dyn DefComponent>, erased_serde::Error>` through `<dyn erased_serde::Deserializer>::erase(d)`.
  - Map the position in `ron::error::SpannedError` into `line_col`. Set `path` to `"{id}/components.{name}"` when the id is known.
- [ ] **Step 4: Run** `cargo test -p fl_content --test objects`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_content): def-component registry, object parsing, inheritance`.

