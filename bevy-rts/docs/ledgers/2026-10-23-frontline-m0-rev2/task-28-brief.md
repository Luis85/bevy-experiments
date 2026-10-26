### Task 28: Contract snapshots — public API, serde formats, `SimTick` system order

**Track:** contract + sim-core · **Refs:** ADR-0011, ADR-0005, ADR-0006 · **Lane 1**

**Files:**
- Create:
  - `xtask/src/api.rs`, `xtask/api-toolchain.txt` (the installed dated nightly)
  - blessed `xtask/snapshots/api/{fl_math,fl_protocol,fl_view,fl_sim}.txt`
  - `crates/{fl_protocol,fl_view,fl_sim}/tests/formats.rs` with blessed snapshots
  - `crates/fl_test_utils/tests/system_order.rs` with a blessed snapshot
- Modify: `xtask/src/{main,gate}.rs` (subcommand `api`, gate row `api`), `crates/fl_sim/src/classify.rs` (each `Codec` gains a serde-reflection `trace` fn), `crates/fl_sim/src/lib.rs`

**Interfaces:**
- Produces:
```rust
// xtask/src/api.rs
pub const API_CRATES: &[&str] = &["fl_math", "fl_protocol", "fl_view", "fl_sim"];
pub fn public_api(root: &Path, krate: &str, toolchain: &str) -> anyhow::Result<String>;   // cargo-public-api, simplified output, with the pinned toolchain
pub fn compare(current: &str, committed: Option<&str>) -> ApiStatus;
pub enum ApiStatus { Same, Changed, New }
// CLI: cargo xtask api [--check] — writes xtask/snapshots/api/<crate>.txt.new for every Changed/New crate; --check fails if any exist
// gate row "api": task (only if a touched package is in API_CRATES, else Skip), pr, milestone
// fl_sim
pub fn codec_formats(app: &App) -> String;    // one block per Hashed codec, in name order: "<name>@<version>:\n<serde-reflection format>"
```
- **Format guard.** Each `formats.rs` snapshots the traced formats as text whose first line names its version: `FORMAT_VERSION = n` for protocol, `VIEW_FORMAT_VERSION = n` for view, `SAVE_FORMAT_VERSION = n` plus per-codec `@version` for sim. A small guard in the same test file reads the committed `.snap`. If a type's format text differs while its version is unchanged, it panics with `wire format of <Type> changed without a version bump` (or `<name>: format changed without a version bump` per codec). The plain insta diff then needs a bless.
- **Traced types:**
  - `fl_protocol`: `PlayerCommand`, `CommandKind`, `MatchHeader`, `StringTables` and the eight payloads.
  - `fl_view`: `SimSnapshot`, `EntityRecord`, `SimEvent`, `TickInput`.
  - `fl_sim`: `SaveFile` plus every core codec.
- **System order:** `system_order__sim_tick.snap` holds `system_order` for `fl_catalog::features()`.

- [ ] **Step 1: Write the failing tests:**
```rust
// xtask/src/api.rs
#[test] fn compare_detects_change()             // compare("a\nb", Some("a")) == Changed; compare("a", Some("a")) == Same; compare("a", None) == New
// crates/fl_protocol/tests/formats.rs
#[test] fn protocol_formats()                   // insta snapshot "protocol_formats"
#[test] fn format_guard_flags_unbumped_change() // guard(committed "FORMAT_VERSION = 1\nMoveCmd: A", current "FORMAT_VERSION = 1\nMoveCmd: B") → Err naming MoveCmd;
                                                //   with "= 2" in current → Ok
// crates/fl_view/tests/formats.rs
#[test] fn view_formats()
// crates/fl_sim/tests/formats.rs
#[test] fn save_codec_formats()                 // contains "core.pose@1:" and "core.match_seed@1:"
// crates/fl_test_utils/tests/system_order.rs
#[test] fn sim_tick_system_order()
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p xtask -p fl_protocol -p fl_view -p fl_sim -p fl_test_utils -E 'test(/formats|system_order|compare_detects/)'`. Expected: FAIL. insta writes `*.snap.new`, and the guard tests fail on missing items.
- [ ] **Step 3: Implement.** Run `cargo xtask api` to write the `.txt.new` files.
- [ ] **Step 4: Bless** (lead): `cargo xtask bless xtask/snapshots crates/fl_protocol/tests/snapshots crates/fl_view/tests/snapshots crates/fl_sim/tests/snapshots crates/fl_test_utils/tests/snapshots --reason "M0 contract baseline"`.
- [ ] **Step 5: Run** the step 2 command and `cargo xtask api --check`. Expected: PASS and `api: 4 crates unchanged`.
- [ ] **Step 6: Gate** `cargo xtask gate pr`. Expected: `PASS api` and `gate pr: PASS`.
- [ ] **Step 7: Commit** `test(contract): public API, serde format and SimTick system-order snapshots` with trailers `Refs: ADR-0011` and `Blessed: M0 contract baseline`.

