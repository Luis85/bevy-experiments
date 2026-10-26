### Task 3: `fl_protocol` — ids and commands

**Files:** Create `crates/fl_protocol/src/lib.rs`.

**Interfaces — Produces.** Every type derives `Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize`; the newtypes also derive `Copy, PartialOrd, Ord, Default`.
- `pub const SIM_TICK_HZ: u32 = 20;`
- Newtypes:
  - `Tick(pub u32)`, with `next()` and `Display`.
  - `PlayerId(pub u8)`, `TeamId(pub u8)`.
  - `SimId(pub u32)`, whose `Display` gives `#12`.
  - `CommandTypeId(pub u16)`, `UnitTypeId(pub u16)`, `FactionId(pub u16)`.
- `pub struct CommandKind { pub type_id: CommandTypeId, pub payload: Vec<u8> }`
- `pub struct PlayerCommand { pub tick: Tick, pub player: PlayerId, pub units: Vec<SimId>, pub kind: CommandKind, pub queue: bool }`
- `pub enum Controller { Human, Ai(String), None }`

- [ ] **Step 1: Write the failing tests.**
  - `player_command_postcard_roundtrip`: encode with `postcard::to_allocvec`, decode, then `assert_eq!`.
  - `tick_ordering`: `Tick(3) < Tick(4)` and `Tick(3).next() == Tick(4)`.
  - `sim_id_display`: `SimId(12).to_string() == "#12"`.
  - `controller_ron_names`: `ron::to_string(&Controller::Human).unwrap() == "Human"`.
- [ ] **Step 2: Run** `cargo test -p fl_protocol`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_protocol`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_protocol): tick, ids, PlayerCommand, Controller`.

