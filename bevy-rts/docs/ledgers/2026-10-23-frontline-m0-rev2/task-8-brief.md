### Task 8: `fl_content` — canonical binary rules hash, frozen registry, string tables

**Track:** contract + content · **Refs:** ADR-0007, ADR-0004 · **Lane 2** (`C:\fl\t08`, cut after Task 6)

**Files:**
- Modify: `crates/fl_content/src/{hash,registry,db,mods,lib}.rs`, `crates/fl_content/tests/load.rs`
- Create fixtures `tests/fixtures/{hash_faction_b,hash_rules_b}/`: copies of the hash fixture with one faction colour or one rules value changed.

**Interfaces:**
- Consumes: `fl_protocol::{StringTables, tables}` (Task 6).
- Produces:
```rust
pub const RULES_HASH_FORMAT: u32 = 1;
//   rules_hash = xxh64(seed 0) over RULES_HASH_FORMAT.to_le_bytes() ++ postcard(canonical tree):
//   (mods: [(id, version)] in load order,
//    units: [(id, name, tags, BTreeMap<&str, &dyn DefComponent>)] sorted by id,
//    kinds: [(ext, [postcard bytes of each item, sorted by id])] for IN_RULES_HASH kinds, sorted by ext,
//    rules: [(section name, postcard bytes)] sorted by name)
impl DefRegistry {
    pub fn freeze(&mut self);
    pub fn is_frozen(&self) -> bool;
    pub fn string_tables(&self) -> StringTables;   // DEF_COMPONENTS, OBJECT_KINDS, CONTENT_KINDS, RULES_SECTIONS, each sorted
}   // every register_* after freeze panics: "DefRegistry is frozen: register '<name>' in SimFeature::build"
impl ContentDb { pub fn string_tables(&self) -> StringTables; }   // UNITS in UnitTypeId order; "content.<ext>" per kind, in KindTable order
// load_dirs freezes the registry it receives before parsing.
```
- `ContentDb::same_id_sets` is deferred to M1 (hot reload, ADR-0012).

- [ ] **Step 1: Write the failing tests:**
```rust
// src/hash.rs (unit test; test DefComponent `Big(Fx)`)
#[test] fn rules_hash_sees_low_bits_at_large_magnitude()   // trees differing only in Fx::from_raw((1 << 53) | 1) vs ((1 << 53) | 2) → hashes differ
// tests/load.rs
#[test] fn rules_hash_changes_with_faction_color()   // hash fixture vs hash_faction_b → hashes differ
#[test] fn rules_hash_changes_with_rules_section()   // vs hash_rules_b → hashes differ
#[test] #[should_panic(expected = "DefRegistry is frozen")] fn register_after_freeze_panics()
#[test] fn registry_string_tables_sorted()           // the DEF_COMPONENTS table == the sorted registered names
#[test] fn content_string_tables_in_id_order()       // UNITS table[i] == db.units()[i].id for every i
// kept: rules_hash_stable_across_loads, rules_hash_ignores_formatting, rules_hash_ignores_visuals, rules_hash_changes_with_stat
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_content`. Expected: FAIL.
- [ ] **Step 3: Implement.** Add `#![deny(clippy::float_arithmetic)]` to `lib.rs`. Confirm with a grep that no rules-hashed type uses `#[serde(flatten)]`.
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract --track content`. Expected: PASS.
- [ ] **Step 6: Commit** `refactor(fl_content)!: canonical postcard rules hash, frozen registry, string tables` with trailer `Refs: ADR-0007, ADR-0004`.

