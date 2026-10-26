### Task 6: `fl_protocol` — lockstep envelope, payload types, `MatchHeader`, string tables

**Track:** contract (+ mechanical `sim-core` call sites) · **Refs:** ADR-0007, ADR-0004, ADR-0008

**Files:**
- Modify: split `crates/fl_protocol/src/lib.rs` into `src/{lib,ids,command,payloads,header,tables}.rs`.
- Modify mechanically: `crates/fl_sim/src/commands.rs` and `crates/fl_sim/tests/*.rs`. Rename `tick` → `exec_tick` and add a `seq` (0, 1, 2, … per player in each test). No behaviour changes.
- Create: `crates/fl_protocol/tests/protocol.rs`

**Interfaces:**
- Produces (every type derives `Debug, Clone, PartialEq, Serialize, Deserialize`; the id newtypes are as in rev 1):
```rust
pub struct PlayerCommand { pub exec_tick: Tick, pub player: PlayerId, pub seq: u32, pub units: Vec<SimId>, pub kind: CommandKind, pub queue: bool }
pub struct CommandKind { pub type_id: CommandTypeId, pub payload: Vec<u8> }   // unchanged
pub trait CommandPayload: Serialize + DeserializeOwned + Debug + Send + Sync + 'static {
    const NAME: &'static str;
    const NEEDS_UNITS: bool = true;
    const DEBUG_ONLY: bool = false;
}
// payloads.rs — all #[serde(deny_unknown_fields)]; content ids are names, never dense ids
pub struct MoveCmd { pub target: FxVec2 }                                                                  // "core.move"
pub struct StopCmd;                                                                                         // "core.stop"
pub struct DebugSpawnCmd { pub unit: String, pub owner: PlayerId, pub pos: FxVec2, pub heading: Angle }   // "debug.spawn", NEEDS_UNITS false
pub struct DebugWanderCmd { pub radius: Fx }                                                                // "debug.wander"
pub struct DebugSetOwnerCmd { pub owner: PlayerId }                                                         // "debug.set_owner"
pub struct DebugInvulnerableCmd { pub on: bool }                                                            // "debug.invulnerable"
pub struct DebugSetHpCmd { pub hp: u32 }                                                                    // "debug.set_hp"
pub struct DebugTeleportCmd { pub pos: FxVec2 }                                                             // "debug.teleport"
//   every debug.* payload has DEBUG_ONLY = true
// tables.rs
#[derive(Default, Eq)] pub struct StringTables { pub tables: BTreeMap<String, Vec<String>> }   // table → names in dense-id order
impl StringTables {
    pub fn insert(&mut self, table: &str, names: Vec<String>);
    pub fn get(&self, table: &str) -> Option<&[String]>;
    pub fn id_of(&self, table: &str, name: &str) -> Option<u32>;
    pub fn remap(&self, table: &str, current: &StringTables) -> Vec<Option<u32>>;   // index = id in self; value = id in current
}
pub mod tables {
    pub const COMMANDS: &str = "commands";             pub const DEF_COMPONENTS: &str = "def_components";
    pub const OBJECT_KINDS: &str = "object_kinds";     pub const CONTENT_KINDS: &str = "content_kinds";
    pub const RULES_SECTIONS: &str = "rules_sections"; pub const SIM_COMPONENTS: &str = "sim_components";
    pub const SIM_RESOURCES: &str = "sim_resources";   pub const RNG_STREAMS: &str = "rng_streams";
    pub const JOURNAL_RECORDS: &str = "journal_records"; pub const VIEW_CHANNELS: &str = "view_channels";
    pub const UNITS: &str = "content.unit";            // other content kinds: "content.<ext>"
}
// header.rs
pub const FORMAT_VERSION: u32 = 1;
pub struct MatchHeader { pub format_version: u32, pub build_id: String, pub sim_version: u32, pub features: Vec<(String, u32)>,
    pub mods: Vec<(String, String)>, pub rules_hash: u64, pub cheats_allowed: bool, pub seed: u64, pub string_tables: StringTables }
pub enum HeaderMismatch { FormatVersion, BuildId, SimVersion, Features, Mods, RulesHash, CheatsAllowed, Seed, StringTables }   // Display by hand
impl MatchHeader {
    pub fn check_replay(&self, recorded: &MatchHeader) -> Result<Vec<String>, HeaderMismatch>;  // format, features, mods, sim_version, rules_hash equal; a build_id difference is a warning
    pub fn check_handshake(&self, peer: &MatchHeader) -> Result<(), HeaderMismatch>;            // every field equal, build_id included
    pub fn check_save(&self, saved: &MatchHeader) -> Result<(), HeaderMismatch>;                // features and mods equal; saved.format_version ≤ ours
}
#[non_exhaustive] pub enum Controller { Human, Ai(String), None }
```
- `fl_sim` keeps its own rev-1 `CommandPayload` trait and payload structs until Task 9 replaces them. The two traits never meet, because they live in different crates.

- [ ] **Step 1: Write the failing tests** (`tests/protocol.rs`, plus the existing unit tests updated):
```rust
#[test] fn player_command_postcard_roundtrip()       // exec_tick 7, seq 3 → equal after encode/decode
#[test] fn match_header_postcard_roundtrip()         // two features, one mod, two tables
#[test] fn payload_names()                           // MoveCmd::NAME == "core.move", StopCmd "core.stop", DebugSpawnCmd "debug.spawn", … all eight
#[test] fn debug_payloads_are_debug_only()           // the six debug.* payloads are DEBUG_ONLY, the core ones are not; DebugSpawnCmd::NEEDS_UNITS == false
#[test] fn fx_payload_is_lossless_in_postcard()      // MoveCmd { target: (Fx::from_raw(0x7FFF_FFFF_0000_0001), Fx::from_raw(-3)) } round-trips bit-exact
#[test] fn payload_rejects_unknown_fields_in_ron()   // ron "(target: (1.0, 2.0), extra: 1)" → Err
#[test] fn string_tables_id_of_and_remap()           // old ["a","c"], current ["a","b","c"] → remap == [Some(0), Some(2)]; id_of(.., "x") == None
#[test] fn replay_check_warns_on_build_id_only()     // build_id differs → Ok(warnings), warnings.len() == 1
#[test] fn replay_check_rejects_rules_hash_mismatch()   // Err(HeaderMismatch::RulesHash)
#[test] fn handshake_requires_build_id()             // Err(HeaderMismatch::BuildId)
#[test] fn save_check_rejects_feature_mismatch()     // Err(HeaderMismatch::Features)
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_protocol`. Expected: FAIL.
- [ ] **Step 3: Implement**, then adapt the `fl_sim` call sites mechanically.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_protocol -p fl_sim`. Expected: PASS, with every existing `fl_sim` test still green.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract --track sim-core`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_protocol)!: lockstep envelope, CommandPayload, core and debug payloads, MatchHeader, StringTables` with trailer `Refs: ADR-0007, ADR-0004, ADR-0008`.

