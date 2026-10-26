# Frontline M0 (Foundation) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Frontline workspace foundation (design §9 M0): a deterministic fixed-point sim with an open def-component content system, a headless toolchain (data-check, hash-dump, bench), and an isometric Bevy 0.20 testbed that shows placeholder units spawned from RON.

**Architecture:** A virtual Cargo workspace. `fl_math` and `fl_protocol` are Bevy-free contracts. `fl_content` loads mods into an immutable `ContentDb` through a registry that features fill. `fl_sim` is a separate Bevy `App`/`World` that uses only `bevy_app` + `bevy_ecs` and is stepped one `SimTick` at a time. `fl_client` owns that sim `World` inside a resource, feeds it `PlayerCommand`s from its 20 Hz `FixedUpdate`, and mirrors sim state into interpolated visual entities.

**Tech Stack:** Rust 1.99.1 (edition 2024), Bevy `=0.20.0`, bevy_egui 0.43, `fixed` (I32F32), serde + ron 0.12, erased-serde 0.4, postcard 1, xxhash-rust (xxh64), clap 4, criterion, proptest, tracing.

**Spec:** `docs/superpowers/specs/2026-10-23-frontline-rts-design.md` is the source of truth. API facts are in `docs/research/bevy020-api-reference.md`; trust it over memory (Message vs Event, `On<Add<T>>`, `WorldAssetRoot`, `ui_widgets::Button`, WESL). When unsure of a 0.20 signature, grep the crate source in `~/.cargo/registry/src/*/bevy_*-0.20.0/`.

## Global Constraints

- Bevy pinned exactly: `bevy = "=0.20.0"`, `bevy_app = "=0.20.0"`, `bevy_ecs = "=0.20.0"`. Rust ≥ 1.97.1, pinned to `1.99.1` in `rust-toolchain.toml`. Edition 2024.
- Build output on a short path: `.cargo/config.toml` has `[build] target-dir = "C:/t/fl"` (Windows MAX_PATH / LNK1104).
- **Where to edit.** `main` lives in `C:\Projects\bevy-experiments`. A guard blocks the Write and Edit tools under `C:\Projects\`, so all editing happens in git worktrees under `C:\fl\` (`C:\fl\a` for Phase A, `C:\fl\tNN` for parallel tasks). The controller fast-forwards or merges `main` with `git -C C:/Projects/bevy-experiments …`.
- Dependency rules (§5.1):
  - `fl_math` and `fl_protocol` have no Bevy dependency.
  - `fl_content` may use `bevy_ecs` only.
  - `fl_sim`, `fl_catalog`, `fl_test_utils` and `fl_cli` must never depend, even transitively, on `bevy_render`, `bevy_window`, `bevy_input`, `bevy_time`, `bevy_winit` or the `bevy` umbrella crate.
  - `scripts/check_deps.sh` enforces these.
- Determinism (§5.4):
  - Sim state uses `Fx` and integers only. `fl_sim` and `fl_protocol` have no `f32`/`f64` arithmetic, enforced by `#![deny(clippy::float_arithmetic)]`.
  - `std::collections::HashMap`/`HashSet` and `std::time::Instant`/`SystemTime` are banned workspace-wide through `clippy.toml` `disallowed-types`. Client code uses `BTreeMap` or `bevy::platform::collections::HashMap`; bench timing code carries a local `#[allow]`.
  - All randomness goes through `SimRng`.
  - Any iteration over entities that changes state follows `SimIndex` (SimId) order.
- `SimTick` runs these sets in order: `ApplyCommands → Orders → Behaviour → Movement → Combat → Damage → Vision → Lifecycle → Hash`. Single-threaded executor; ambiguity detection at `LogLevel::Error`.
- Sim tick rate is `SIM_TICK_HZ = 20` (in `fl_protocol`). The state hash (xxh64) runs every 20 ticks by default, set by `StateHash.interval_ticks` in `rules.ron`.
- Unit types are never a closed Rust enum. Content IDs are strings namespaced by mod (`"base.de.pz3"`) and interned to dense IDs in sorted order.
- Content is strict. Every def and kind struct uses `#[serde(deny_unknown_fields)]`. Unknown component names, rules sections and `*.X.ron` kinds are errors that carry the file and line.
- Never copy code or data from nival/Blitzkrieg; every stat in `assets/base` is our own number. M0 uses primitive placeholders only, and nothing is downloaded.
- Sim coordinates are metres on the ground plane, `x` east and `y` north. Heading is an `Angle`: 0 is +x, increasing counter-clockwise toward +y.
  - The client maps sim `(x, y)` to Bevy `Vec3::new(x, height, y)`, and Bevy yaw = `-heading_radians`.
  - Placeholder meshes are built with their length along local +X.
- **Observability (spec §15).**
  - Journal, profiler timings, `SimCounters` and `SetTimings` are observe-only. Sim logic never reads them, and they stay out of the state hash.
  - Sim systems live in module paths that name their owner (`fl_sim::<module>::…`, `fl_feat_<name>::…`); the profiler attributes time to features from those paths.
  - Log with `tracing` using the default module-path targets.
  - `std::time::Instant` is allowed only inside `fl_sim::observe`, `fl_observe` and bench code, each with a local `#[allow(clippy::disallowed_types)]`.
- **Task gate.** Run these from the worktree root before every commit; all must pass:
  ```bash
  cargo fmt --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  bash scripts/check_deps.sh
  ```
- Commit only the files your task owns. Use messages like `feat(fl_x): …`, ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Hostile content** (an inheritance cycle, a missing parent, NaN or `1e40` numbers, a speed of `1e12`, an abstract unit placed in a scenario) must produce a `ContentError` naming the file and location, never a panic or an Fx overflow. Tests: T2 `fx_rejects_out_of_range`; T4 `inheritance_cycle_is_error`; T5 `scenario_rejects_abstract_unit`; T7 `mobility_rejects_absurd_speed`.
2. **Map edges** (a spawn at exactly `x == width`, a move target outside the map, a wander home in a corner): a spawn outside the map is rejected, targets are clamped, and the spatial cell index is always in range. Tests: T7 `move_target_outside_map_is_clamped`, `spawn_outside_map_is_error`; T9 `position_on_max_edge_maps_to_last_cell`.
3. **Bad commands** (a late tick, an unknown `CommandTypeId`, a corrupt payload, foreign, despawned or duplicate `SimId`s) are rejected into `CommandLog`. The sim continues, and its state hash matches a run without that command. Test: T7 `bad_commands_do_not_change_state`.
4. **Broken mod directories** (a missing `mod.ron`, a missing dependency, a duplicate mod id, an unknown `*.xyz.ron`, invalid UTF-8): all errors are collected, and `data-check` prints every one and exits 1. Tests: T5 `mod_errors_are_aggregated`; T10 `data_check_bad_mod_exit_code`.
5. **Client edge cases** (a zero-size or minimised window, the cursor outside the window, zero units, a unit despawned between frames, extreme zoom): no panic, and the camera stays clamped. Tests: T12 `edge_pan_zero_size_window`, `zoom_is_clamped`; T13 `despawn_removes_visual`.

## Execution topology

```
Phase A (sequential, contracts) : T1 → T2 ⇄ T3 → T4 → T5 → T6 → T7 → (T8 ∥ T18)  (T2 and T3 may run in parallel)
Phase B (parallel, worktrees)   : T9 spatial | T10 fl_cli data-check | T11 test utils + determinism | T12 client camera/ground | T13 client sim view | T19 fl_observe
Phase C (parallel, worktrees)   : T14 debug tools (after T13) | T15 GameBuilder + fl_game (after T12, T13, T19) | T16 bench harness (after T9, T10, T11, T19)
Phase C2 (parallel, worktrees)  : T20 fl_cli agent commands (after T16) | T21 live debug: BRP + perf panel (after T14, T15)
Phase D (sequential)            : T17 integration, docs/debugging.md, render stress test, perf baseline, demo checklist
```

- **Worktrees.** Each Phase B/C agent works in its own git worktree, `C:\fl\tNN` on branch `m0/tNN-…`. All agents share the `C:/t/fl` target dir, and each touches only the files listed under its task.
- **Why later tasks never touch shared files.** T1 creates every crate, every `Cargo.toml` with its full M0 dependency list, and every module stub. Later tasks never edit `Cargo.toml`, `Cargo.lock`, or a `lib.rs` they don't own.
- **New dependencies.** If a task truly needs one, it stops and reports instead of adding it.
- **Merging.** The controller merges branches into `main` in task order and re-runs the gate after each merge.

## File map

```
Cargo.toml  rust-toolchain.toml  clippy.toml  .gitignore  .cargo/config.toml  README.md
scripts/check_deps.sh
crates/fl_math/src/{lib,fx,vec,angle,trig,rng}.rs        tests/fx_props.rs
crates/fl_protocol/src/lib.rs
crates/fl_content/src/{lib,error,registry,def,object,inherit,kinds,builtin,mods,rules,link,hash,db}.rs
                  tests/{objects,load}.rs  tests/fixtures/**
crates/fl_sim/src/{lib,schedule,clock,ids,rng,hash,events,feature,app_ext,build,players,map,
                   commands,spawn,units,movement,scenario,scale,spatial}.rs
             tests/{schedule,hash,commands,movement,spatial}.rs  tests/fixtures/core_mod/**  benches/sim_tick.rs
crates/fl_catalog/src/lib.rs
crates/fl_test_utils/src/{lib,builder,determinism}.rs    tests/determinism.rs
crates/fl_cli/src/{lib,main,data_check,hash_dump,bench}.rs  tests/{data_check,bench}.rs  tests/fixtures/bad_mod/**
crates/fl_client/src/{lib,camera,map_view,view,placeholder,game}.rs  src/debug/{mod,console,spawn_panel,stats}.rs
crates/fl_game/src/main.rs
assets/base/{mod,rules,credits}.ron  factions/  units/templates/ units/de/ units/su/  visuals/  maps/  scenarios/
docs/perf/{m0-baseline.md,baseline.ron}   docs/m0-demo-checklist.md
```

---

## Phase A — contracts (sequential, worktree `C:\fl\a`)

### Task 1: Workspace scaffold

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `clippy.toml`, `.gitignore`, `README.md`, `scripts/check_deps.sh`
- Modify: `.cargo/config.toml` (keep the existing `[build] target-dir`)
- Create: `crates/<crate>/Cargo.toml` and `src/lib.rs` for each of `fl_math fl_protocol fl_content fl_sim fl_catalog fl_test_utils fl_cli fl_client fl_game`, plus `fl_cli/src/main.rs` and `fl_game/src/main.rs`
- Create stubs: `crates/fl_client/src/{camera,map_view,view,placeholder,game}.rs`, `crates/fl_client/src/debug/mod.rs`

**Interfaces — Produces:**
- Compiling empty crates.
- `fl_client` plugin stubs `CameraPlugin`, `MapViewPlugin`, `SimViewPlugin`, `PlaceholderPlugin` and `DebugToolsPlugin`, each a unit struct with an empty `impl Plugin` `build`.
- `pub struct ClientPlugins;` in `lib.rs`, a `PluginGroup` that adds all five.
- An empty `game.rs` module.

- [ ] **Step 1: Root `Cargo.toml`.**
  - Virtual workspace with `resolver = "3"` and `members = ["crates/*"]`.
  - `[workspace.package]`: `edition = "2024"`, `rust-version = "1.97.1"`, `license = "MIT OR Apache-2.0"`.
  - `[workspace.dependencies]`, resolved to the latest compatible versions at implementation time (Bevy stays exact): `bevy = "=0.20.0"`, `bevy_app = "=0.20.0"`, `bevy_ecs = "=0.20.0"`, `bevy_egui = "0.43"`, `fixed = "1"`, `serde = { version = "1", features = ["derive"] }`, `ron = "0.12"`, `erased-serde = "0.4"`, `postcard = { version = "1", features = ["alloc"] }`, `xxhash-rust = { version = "0.8", features = ["xxh64"] }`, `thiserror = "2"`, `clap = { version = "4", features = ["derive"] }`, `criterion` (latest), `proptest = "1"`, `tracing = "0.1"`, and path deps for every `fl_*` crate.
  - `[workspace.lints.rust] unsafe_code = "forbid"`.
  - Profiles: `[profile.dev] opt-level = 1`, `[profile.dev.package."*"] opt-level = 3`, `[profile.release] debug = "line-tables-only"`.

- [ ] **Step 2: Per-crate dependencies.** Each crate also sets `[lints] workspace = true`.

| crate | deps | dev-deps |
|---|---|---|
| fl_math | fixed, serde | proptest, ron |
| fl_protocol | serde | postcard, ron |
| fl_content | fl_math, fl_protocol, bevy_ecs, serde, ron, erased-serde, xxhash-rust, thiserror, tracing | — |
| fl_sim | fl_math, fl_protocol, fl_content, bevy_app, bevy_ecs, serde, postcard, xxhash-rust, thiserror, tracing | criterion, proptest (`[[bench]] name = "sim_tick", harness = false`) |
| fl_catalog | fl_sim | — |
| fl_test_utils | fl_math, fl_protocol, fl_content, fl_sim, fl_catalog, bevy_app, bevy_ecs | — |
| fl_cli (lib + bin `fl_cli`) | fl_math, fl_protocol, fl_content, fl_sim, fl_catalog, fl_test_utils, bevy_app, bevy_ecs, clap, thiserror, tracing, ron, serde | — |
| fl_client | bevy, bevy_egui, fl_math, fl_protocol, fl_content, fl_sim, tracing | — |
| fl_game (bin) | fl_client, fl_catalog, clap | — |

- [ ] **Step 3: Tooling files.**
  - `rust-toolchain.toml`: `channel = "1.99.1"`, components `rustfmt` and `clippy`.
  - `clippy.toml`: `disallowed-types = ["std::collections::HashMap", "std::collections::HashSet", "std::time::Instant", "std::time::SystemTime"]`.
  - `.gitignore`: `/target`, `/.claude/`, `*.log`.
  - `README.md`: one paragraph, the gate commands, and `cargo run -p fl_game`.
- [ ] **Step 4: `scripts/check_deps.sh`.** It checks each crate's normal dependencies with `cargo tree -p <crate> -e normal --prefix none`:
  - `fl_math`, `fl_protocol`: fail if any line starts with `bevy`.
  - `fl_content`: fail on any `bevy*` crate except `bevy_ecs`, `bevy_ecs_macros`, `bevy_ptr`, `bevy_utils`, `bevy_platform`, `bevy_tasks`, `bevy_reflect*` and `bevy_macro_utils`.
  - `fl_sim`, `fl_catalog`, `fl_test_utils`, `fl_cli`: fail on `^(bevy |bevy_render|bevy_window|bevy_input|bevy_time|bevy_winit|wgpu)`.

  On success it prints `deps OK`. On failure it prints the offending crate and line, then exits 1.
- [ ] **Step 5: Build** with `cargo build --workspace`. A cold Bevy build takes about 5–10 min, so run it in the background. Expected: success.
- [ ] **Step 6: Run the task gate.** Expected: everything passes, and the script prints `deps OK`.
- [ ] **Step 7: Commit** everything above together with this plan: `chore: workspace scaffold for Frontline M0`.

### Task 2: `fl_math` — Fx, vectors, Angle, trig, PCG

**Files:** Create `crates/fl_math/src/{lib,fx,vec,angle,trig,rng}.rs` and `crates/fl_math/tests/fx_props.rs`.

**Interfaces — Produces:**
- **`pub struct Fx(I32F32)`**, `#[repr(transparent)]`, deriving `Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash`.
  - Consts: `ZERO`, `ONE`, `HALF`, `MAX`, `MIN`, `EPSILON` (raw 1).
  - Constructors and accessors: `from_int(i32)`, `from_ratio(num: i64, den: i64)`, `from_raw(i64)`, `raw() -> i64`, and `from_f64(f64) -> Result<Fx, FxRangeError>`. `from_f64` is for load time only; it rejects NaN, ±inf and |v| ≥ 2^31.
  - Presentation only: `to_f32()`, `to_f64()`.
  - Maths: `floor_to_i64()`, `abs`, `min`, `max`, `clamp`, `sqrt` (≤ 0 gives ZERO), `mul_int(i32)`.
  - Ops: `Add Sub Mul Div Neg` and the `*Assign` forms. `Display` prints a decimal.
  - serde: `Deserialize` accepts RON integers and floats through `from_f64`, and the error names the value. `Serialize` writes an `f64`.
- **`pub struct FxVec2 { pub x: Fx, pub y: Fx }`**, serialized as a 2-tuple `(x, y)`.
  - `new`, `ZERO`, `Add Sub Neg`, `Mul<Fx>`, `Div<Fx>`.
  - `dot`, `perp_dot`, `length_sq`, `length`, `distance_sq`, `normalize_or_zero`.
  - `from_angle(Angle)` gives a unit vector; `rotate(Angle)`; `angle() -> Angle` uses atan2; `clamp_to_rect(min: FxVec2, max: FxVec2)`.
- **`pub struct FxVec3 { pub x: Fx, pub y: Fx, pub z: Fx }`**: `new`, `ZERO`, `Add Sub`, `Mul<Fx>`, `xy() -> FxVec2`.
- **`pub struct Angle(u16)`**, a binary angle where 65536 is a full turn. Derives `Copy, Eq, Ord, Hash, Default, Debug`.
  - Consts: `ZERO`, `QUARTER` (16384), `HALF` (32768).
  - `from_raw(u16)`, `raw()`, `from_degrees(Fx)`, `to_degrees() -> Fx`, and `to_radians_f32()` for presentation.
  - Wrapping `Add Sub Neg`.
  - `signed_diff(self, to: Angle) -> i32`, in `[-32768, 32767]`.
  - `rotate_towards(self, target: Angle, max_step: Angle) -> Angle`.
  - `sin() -> Fx`, `cos() -> Fx`, `atan2(y: Fx, x: Fx) -> Angle` (`atan2(0,0)` = ZERO).
  - serde: serialized as degrees (`f64`), going through `Fx`.
- **`pub struct Pcg32 { state: u64, inc: u64 }`**: PCG-XSH-RR 64/32, seeded exactly like `pcg32_srandom_r`. Derives `Clone, Debug, PartialEq, Eq, Serialize, Deserialize`.
  - `new(seed: u64, stream: u64)`, `next_u32`, `next_u64`.
  - `range_u32(lo, hi_excl)`, unbiased (Lemire or rejection).
  - `chance(num: u32, den: u32) -> bool`.
  - `unit_fx() -> Fx` returns `Fx::from_raw(next_u32() as i64)`, so the result is in `[0,1)`.
  - `range_fx(lo: Fx, hi: Fx) -> Fx`, `state() -> (u64, u64)`.
- **`pub fn derive_seed(seed: u64, tick: u32, sim_id: u32, stream: u32) -> u64`**: splitmix64 over the packed inputs, for parallel phases (§5.4).

- [ ] **Step 1: Write the failing tests.** These are unit tests in each module; f64 is allowed in tests only.
```rust
#[test] fn pcg32_reference_vector() {               // pcg32-demo, seed 42 stream 54
    let mut r = Pcg32::new(42, 54);
    let got: Vec<u32> = (0..6).map(|_| r.next_u32()).collect();
    assert_eq!(got, [0xa15c02b7, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e]);
}
#[test] fn sin_table_exact_points() { assert_eq!(Angle::QUARTER.sin(), Fx::ONE); assert_eq!(Angle::HALF.sin(), Fx::ZERO); assert_eq!(Angle::ZERO.cos(), Fx::ONE); }
#[test] fn sin_matches_f64_everywhere() { for r in 0..=u16::MAX { let a = Angle::from_raw(r); let e = (r as f64 / 65536.0 * std::f64::consts::TAU).sin(); assert!((a.sin().to_f64() - e).abs() < 1e-6, "{r}"); } }
#[test] fn atan2_error_within_4_units() { /* grid x,y in -50..=50 step 1, skip (0,0): |signed_diff(atan2, round(f64 atan2 * 65536/TAU))| <= 4 */ }
#[test] fn rotate_towards_takes_short_way() { let a = Angle::from_degrees(Fx::from_int(350)); let t = Angle::from_degrees(Fx::from_int(10)); assert_eq!(a.rotate_towards(t, Angle::from_degrees(Fx::from_int(5))), Angle::from_degrees(Fx::from_int(355))); }
#[test] fn fx_rejects_out_of_range() { assert!(Fx::from_f64(f64::NAN).is_err()); assert!(Fx::from_f64(1e40).is_err()); assert!(ron::from_str::<Fx>("1e40").is_err()); assert_eq!(ron::from_str::<Fx>("40").unwrap(), Fx::from_int(40)); }
#[test] fn sqrt_exact_squares() { assert_eq!(Fx::from_int(144).sqrt(), Fx::from_int(12)); assert_eq!(Fx::from_int(-4).sqrt(), Fx::ZERO); }
#[test] fn unit_fx_in_range() { let mut r = Pcg32::new(1, 1); for _ in 0..10_000 { let u = r.unit_fx(); assert!(u >= Fx::ZERO && u < Fx::ONE); } }
#[test] fn derive_seed_distinct_neighbours() { assert_ne!(derive_seed(1, 0, 1, 0), derive_seed(1, 0, 2, 0)); assert_eq!(derive_seed(9, 3, 4, 5), derive_seed(9, 3, 4, 5)); }
```
  `tests/fx_props.rs` holds the proptests:
  - `add_sub_roundtrip`
  - `mul_matches_f64_within_2eps`, with operands in ±40_000
  - `vec_length_matches_f64`, with components in ±2_000 and relative error < 1e-6
  - `angle_from_vec_roundtrip`: `FxVec2::from_angle(a).angle()` is within 4 units of `a`
- [ ] **Step 2: Run** `cargo test -p fl_math`. Expected: FAIL (items not defined).
- [ ] **Step 3: Implement.**
  - **Sine:** a quarter-wave table of 16 385 `Fx` values, built once in a `std::sync::LazyLock` by a **pure integer** routine: a Taylor series in i128 fixed point with π as an integer constant, and no f64. The other quadrants mirror it.
  - **atan2:** CORDIC in vectoring mode on i64, after reducing to the right half-plane. Use 16 iterations and a constant table of `atan(2^-i)` in binary-angle units: `8192, 4836, 2555, 1297, 651, 326, 163, 81, 41, 20, 10, 5, 3, 1, 1, 0`.
  - **sqrt:** `u128::isqrt((raw as u128) << 32)`.
- [ ] **Step 4: Run** `cargo test -p fl_math`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_math): Fx fixed-point, vectors, binary angles, trig tables, PCG32`.

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

### Task 7: `fl_sim` core — commands, spawning, unit components, movement, scenarios, scale scenario

**Files:**
- Fill `crates/fl_sim/src/{commands,spawn,units,movement,scenario,scale}.rs`.
- Modify `feature.rs` (the CoreFeature registrations) and `app_ext.rs` (add `register_command`).
- Create `crates/fl_sim/tests/{commands,movement}.rs`.
- Add these fixtures to `tests/fixtures/core_mod/`:
  - `units/tank.unit.ron`: `core_test.tank` with Selectable, Hull (hp 100, footprint (3.0, 6.0)) and Mobility (Tracked, 36 km/h, terrain 0.5, turn 90°/s).
  - `units/abstract_vehicle.unit.ron`: abstract.
  - `factions/{red,blue}.faction.ron`.
  - `maps/flat.map.ron`: 1000×1000, cell 4.
  - `scenarios/duel.scenario.ron`: 2 players, one tank each, at (100,100) and (900,900).
  - A sibling fixture `tests/fixtures/absurd_speed/`, a copy of core_mod with `road_speed_kmh: 1e12`.

**Interfaces — Produces:**
```rust
// commands.rs
pub trait CommandPayload: Serialize + DeserializeOwned + std::fmt::Debug + Send + Sync + 'static {
    const NAME: &'static str;
    const NEEDS_UNITS: bool = true;
    const DEBUG_ONLY: bool = false;
    fn unit_can(&self, _unit: EntityRef<'_>) -> bool { true }
    fn apply(&self, world: &mut World, ctx: &CommandCtx<'_>);
}
pub struct CommandCtx<'a> { pub tick: Tick, pub player: PlayerId, pub units: &'a [(SimId, Entity)], pub queue: bool }
#[derive(Resource, Default)] pub struct CommandRegistry { … }
impl CommandRegistry { pub fn type_id<P: CommandPayload>(&self) -> CommandTypeId; pub fn encode<P: CommandPayload>(&self, p: &P) -> CommandKind; pub fn name(&self, id: CommandTypeId) -> Option<&'static str>; }
// SimAppExt gains: fn register_command<P: CommandPayload>(&mut self) -> &mut Self;   ids assigned by sorted NAME in build_sim_app
#[derive(Resource, Default)] pub struct CommandQueue { … }
impl CommandQueue { pub fn push(&mut self, cmd: PlayerCommand, now: Tick) -> Result<(), RejectReason>; }  // cmd.tick < now ⇒ Err(Late)
pub enum RejectReason { Late, UnknownType, BadPayload, UnknownPlayer, NoValidUnits, DebugDisabled }
#[derive(Resource, Default)] pub struct CommandLog { pub applied: Vec<PlayerCommand>, pub rejected: Vec<(PlayerCommand, RejectReason)> }
pub fn submit(world: &mut World, cmd: PlayerCommand) -> Result<(), RejectReason>;   // push at current clock; Late rejections also logged
// apply_commands: exclusive, SimSet::ApplyCommands, after the SimEvents clear. Commands for clock.tick in push order;
// units deduped+sorted, filtered to existing, owned by cmd.player, unit_can; NEEDS_UNITS && empty ⇒ NoValidUnits.
// Core payloads (all #[serde(deny_unknown_fields)]):
pub struct MoveCmd { pub target: FxVec2 }                 // "core.move"; unit_can: has Mobility; queue ⇒ append waypoint, else replace
pub struct StopCmd;                                       // "core.stop"; clears MoveOrder
pub struct DebugSpawnCmd { pub unit: UnitTypeId, pub pos: FxVec2, pub heading: Angle }   // "debug.spawn"; NEEDS_UNITS false; DEBUG_ONLY
pub struct DebugWanderCmd { pub radius: Fx }              // "debug.wander"; DEBUG_ONLY; radius 0 removes Wander, else inserts Wander{home: pos, radius}
// units.rs — def components (registered by CoreFeature) and runtime components
pub struct SelectableDef;                                                  // "Selectable" → Selectable marker
pub struct HullDef { pub hp: u32, pub footprint: FxVec2, pub hitbox_shrink: Fx }     // "Hull" → Health{hp,max}, Footprint; validate hp ≥ 1, 0 < footprint ≤ 50, 0 ≤ shrink ≤ 1
pub enum MoveClass { Foot, Wheeled, HalfTrack, Tracked }
pub struct MobilityDef { pub class: MoveClass, pub road_speed_kmh: Fx, pub terrain_factor: Fx, pub turn_rate_deg_s: Fx }  // "Mobility"; validate 0 < kmh ≤ 200, 0 ≤ factor ≤ 1, 0 < turn ≤ 720
#[derive(Component)] pub struct Owner(pub PlayerId);
#[derive(Component)] pub struct UnitType(pub UnitTypeId);
#[derive(Component)] pub struct Pose { pub pos: FxVec2, pub heading: Angle }
#[derive(Component)] pub struct Health { pub hp: u32, pub max: u32 }
#[derive(Component)] pub struct Footprint { pub width: Fx, pub length: Fx, pub hitbox_shrink: Fx }
#[derive(Component)] pub struct Mobility { pub class: MoveClass, pub speed: Fx /* m per tick = kmh / 72 */, pub terrain_factor: Fx, pub turn_rate: Angle /* per tick = deg_s / 20 */ }
#[derive(Component, Default)] pub struct MoveOrder { pub waypoints: Vec<FxVec2> }
#[derive(Component)] pub struct Wander { pub home: FxVec2, pub radius: Fx }
#[derive(Component)] pub struct Selectable;
// hashed: "Owner", "UnitType", "Pose", "Health", "MoveOrder", "Wander"
// spawn.rs
pub fn spawn_unit(world: &mut World, unit: UnitTypeId, player: PlayerId, pos: FxVec2, heading: Angle) -> Result<SimId, SpawnError>;
pub fn despawn_unit(world: &mut World, id: SimId) -> bool;
pub enum SpawnError { UnknownUnit, UnknownPlayer, OutsideMap, NoMap }
// movement.rs
#[derive(SystemSet, …)] pub struct MovementSystems;   // label Task 9 orders after
// wander_system (SimSet::Behaviour, SimIndex order): Wander + empty MoveOrder ⇒ push map.clamp(home + (range_fx(-r,r), range_fx(-r,r)))
// move_system (SimSet::Movement, in MovementSystems, SimIndex order): toward waypoint[0]: heading.rotate_towards(desired, turn_rate);
//   advance only when |signed_diff| ≤ 8192 (45°): step = min(speed, dist) along the straight line to the target; arrive (dist ≤ speed) ⇒ snap + pop; pos = map.clamp(pos)
// scenario.rs
pub enum SetupError { UnknownMap(String), UnknownScenario(String), UnknownFaction(String), Spawn(SpawnError) }
pub fn setup_map(world: &mut World, map_id: &str) -> Result<(), SetupError>;              // inserts MapInfo
pub fn setup_scenario(world: &mut World, scenario_id: &str) -> Result<Vec<SimId>, SetupError>;  // map, Players, SimRng reseed(scenario.seed), units in file order
pub fn setup_scenario_def(world: &mut World, def: &ScenarioDef) -> Result<Vec<SimId>, SetupError>; // same, from a def (setup_scenario delegates to it)
// all hashed runtime components also derive serde::Serialize (inspection, spec §15 C)
// scale.rs
pub struct ScaleScenario { pub units: u32, pub players: u8, pub map_m: i32, pub wander_m: i32, pub seed: u64 }
impl Default for ScaleScenario { /* 3000, 4, 2000, 150, 1 */ }
impl ScaleScenario { pub fn populate(&self, world: &mut World) -> Result<Vec<SimId>, SetupError>; }
//   flat map map_m×map_m cell 4; players i → faction i % factions (sorted), team i; positions uniform in player i's quadrant via Pcg32::new(seed, 7);
//   unit types cycle over concrete units with Mobility (sorted id); every unit gets Wander{home: pos, radius: wander_m}; SimRng reseeded with seed
```

- [ ] **Step 1: Write the failing tests.**
  - `tests/commands.rs`:
    - `move_command_reaches_target`: a tank at (100,100) with heading 0 gets `MoveCmd{target:(200,100)}` at tick 0. Within 200 ticks its pose is (200,100) and its MoveOrder is empty.
    - `queued_moves_append`: `queue=true` leaves 2 waypoints.
    - `foreign_units_rejected`: `NoValidUnits` appears in `CommandLog.rejected`.
    - `late_command_rejected`.
    - `debug_spawn_requires_flag`: `DebugDisabled` when `allow_debug_commands` is false; a 3rd unit spawns when it is true.
    - `command_type_ids_sorted_by_name`: `core.move` < `core.stop` < `debug.spawn` < `debug.wander`.
    - `bad_commands_do_not_change_state`: run A is 50 ticks. Run B is the same plus four bad commands: a late command, an unknown `CommandTypeId(999)`, payload `[0xFF; 3]` for core.move, and a move for a foreign SimId. The final `compute_state_hash` values are equal, and `rejected.len() == 4`.
  - `tests/movement.rs`:
    - `kmh_converts_to_m_per_tick`: 36 km/h gives exactly `Fx::from_int(36) / Fx::from_int(72)` = 0.5.
    - `tank_turns_before_moving`: with the target behind, the first ticks change the heading while the position stays the same.
    - `move_target_outside_map_is_clamped`: a target of (5000,5000) leaves the final position inside the map.
    - `spawn_outside_map_is_error`.
    - `spawn_unknown_unit_is_error`: `spawn_unit` with an out-of-range `UnitTypeId` gives `UnknownUnit`, and an abstract id is not interned (`unit_id` is None).
    - `mobility_rejects_absurd_speed`: loading `absurd_speed` gives `ContentErrors`.
    - `wander_stays_within_radius_and_map`: with a scale scenario of 200 units over 400 ticks, every position stays inside the map and within radius·√2 of home.
    - `scale_populate_is_deterministic`: two worlds with the same seed have identical pose lists, and 4 players are present.
    - `setup_scenario_spawns_units`: duel gives 2 SimIds, `[1,2]`.
- [ ] **Step 2: Run** `cargo test -p fl_sim`. Expected: FAIL.
- [ ] **Step 3: Implement** with the signatures above. `DebugSpawnCmd.apply` calls `spawn_unit` for `ctx.player`.
- [ ] **Step 4: Run** `cargo test -p fl_sim`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_sim): commands, spawning, core unit components, movement, scenarios, scale scenario`.

### Task 8: Base content and `fl_catalog`

**Files:**
- `crates/fl_catalog/src/lib.rs` and `crates/fl_catalog/tests/base_content.rs`.
- `assets/base/{mod,rules,credits}.ron`.
- `assets/base/factions/{de,su}.faction.ron`.
- `assets/base/units/templates/{light_tank,medium_tank,heavy_tank,truck,infantry}.unit.ron`: abstract, with ids `base.abstract.*`.
- `assets/base/units/de/{pz2,pz3,tiger,opel_blitz,rifleman}.unit.ron` and `assets/base/units/su/{t70,t34,kv1,zis5,rifleman}.unit.ron`.
- `assets/base/visuals/<unit>.visual.ron`, one per concrete unit.
- `assets/base/maps/{testbed_1km,scale_2km}.map.ron` and `assets/base/scenarios/testbed.scenario.ron`.

**Interfaces — Produces:**
- `pub fn features() -> Vec<Arc<dyn GameFeature>>`. Empty in M0 because CoreFeature is implicit; each feature crate adds one line here later.
- `pub fn base_mod_dir() -> PathBuf`: `env!("CARGO_MANIFEST_DIR")/../../assets/base`, canonicalized.

**Content:**
- **Factions:** `base.de` (Germany, colour (0.42, 0.46, 0.50)) and `base.su` (USSR, colour (0.38, 0.45, 0.22)).
- **Maps:** `base.testbed_1km` (1000×1000, cell 4) and `base.scale_2km` (2000×2000, cell 4).
- **Scenario `base.testbed`:** map testbed_1km, seed 1. Players: DE (team 0, Human) and SU (team 1, Human). It places all 10 unit types: the DE row along y = 350 with heading 90°, and the SU row along y = 650 with heading 270°, each spaced 30 m apart starting at x = 350.
- **`rules.ron`:** `Rules({ "StateHash": (interval_ticks: 20), "Spatial": (cell_m: 32.0) })`.
- **`credits.ron`:** `Credits([])`.
- **Unit keys:** only `Selectable`, `Hull` and `Mobility`, since no others are registered in M0.
- **Stats** are our own plausible numbers:
  - Road speeds (km/h): pz2 40, pz3 40, tiger 38, t70 45, t34 54, kv1 35, trucks 60, riflemen 5.
  - Movement classes: infantry `Foot`, trucks `Wheeled`, tanks `Tracked`.

- [ ] **Step 1: Write the failing tests.**
  - `base_content_loads`: `fl_sim::load_content(&fl_catalog::features(), &[base_mod_dir()])` is `Ok` and has 10 concrete units, 2 factions, 1 scenario and no warnings.
  - `base_testbed_runs`: build the sim; `setup_scenario("base.testbed")` returns 10 ids, and 100 ticks run without a panic.
- [ ] **Step 2: Run** `cargo test -p fl_catalog`. Expected: FAIL.
- [ ] **Step 3: Write the RON files and `lib.rs`.**
- [ ] **Step 4: Run** `cargo test -p fl_catalog`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(content): base mod with DE/SU placeholder roster; fl_catalog`.

---

## Phase B — parallel tracks (one worktree each, after Task 8 is on `main`)

### Task 9: Incremental spatial grid (`fl_sim/src/spatial.rs`)

**Files:** Modify `crates/fl_sim/src/spatial.rs` only; create `crates/fl_sim/tests/spatial.rs`.

**Interfaces — Consumes:** `MapInfo`, `Pose`, `SimEntity`, `SimSet::Movement`, `MovementSystems`, `SpatialRules`, `ScaleScenario`. **Produces:**
```rust
#[derive(Resource, Debug, Clone)] pub struct SpatialGrid { /* cell: Fx, cols: u32, rows: u32, cells: Vec<Vec<SimId>> each sorted */ }
impl SpatialGrid {
    pub fn new(map: &MapInfo, cell: Fx) -> Self;
    pub fn cell_of(&self, pos: FxVec2) -> u32;              // clamps into the grid; pos == size maps to last cell
    pub fn insert(&mut self, id: SimId, cell: u32);
    pub fn remove(&mut self, id: SimId, cell: u32) -> bool;
    pub fn relocate(&mut self, id: SimId, from: u32, to: u32);
    pub fn members(&self, cell: u32) -> &[SimId];
    pub fn query_radius(&self, center: FxVec2, radius: Fx, out: &mut Vec<SimId>);  // broad phase: cells overlapping the circle's AABB; out cleared, sorted ascending, no dups
    pub fn len(&self) -> usize;
}
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)] pub struct SpatialCell(pub u32);
```
**Systems.** All run in `SimSet::Movement`, `.after(MovementSystems)`:
- `rebuild_on_map_change`: when `MapInfo` changes, build a new grid from every Pose and insert `SpatialCell`.
- `track_new`: `Added<Pose>` without `SpatialCell`.
- `track_moved`: `Changed<Pose>` with `SpatialCell`; relocate when the cell differs.
- Removal: an observer on `On<Remove<SpatialCell>>` removes the entity from the grid.

Iterating `Changed<Pose>` in archetype order is allowed here: each cell is kept sorted, so the final grid doesn't depend on visit order. Say so in a comment.

- [ ] **Step 1: Write the failing tests.**
  - `insert_keeps_cells_sorted`.
  - `position_on_max_edge_maps_to_last_cell`: on a 1000 m map with cell 32, `cell_of((1000,1000)) == cols*rows - 1`.
  - `relocate_moves_between_cells`.
  - `query_radius_matches_brute_force`: a proptest with random ids and positions on a 2 km map and radius 1..300. The broad-phase result contains the exact brute-force set and is sorted and unique.
  - `grid_tracks_wandering_units`: ScaleScenario with 500 units for 300 ticks. Each entity's `SpatialCell == cell_of(pose)`, it is a member of exactly that cell, and `len() == 500`.
  - `despawn_removes_from_grid`.
- [ ] **Step 2: Run** `cargo test -p fl_sim --test spatial`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_sim --test spatial`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_sim): incremental uniform spatial grid`.

### Task 10: `fl_cli` data-check and hash-dump

**Files:** `crates/fl_cli/src/{lib,main,data_check,hash_dump}.rs`, `crates/fl_cli/tests/data_check.rs`, and `crates/fl_cli/tests/fixtures/bad_mod/**` (a missing dependency, an unknown component and an unknown file kind).

**Interfaces — Produces:**
```rust
pub struct DataCheckReport { pub mods: Vec<String>, pub units: usize, pub kinds: Vec<(&'static str, usize)>, pub warnings: Vec<String>, pub rules_hash: u64 }
pub fn data_check(dirs: &[PathBuf]) -> Result<DataCheckReport, ContentErrors>;    // uses fl_catalog::features()
pub fn hash_dump(mods: &[PathBuf], scenario: &str, ticks: u32, interval: u32) -> Result<Vec<(Tick, u64)>, CliError>;
```
The CLI uses `clap` derive:
- `fl_cli data-check <DIRS>...` prints the report (including `rules hash: 0x{:016x}`) and warnings. On failure it prints every error line, then exits 1.
- `fl_cli hash-dump --mods <DIR>... --scenario <ID> --ticks <N> [--interval <N>]` prints `tick hash` lines.

- [ ] **Step 1: Write the failing tests.**
  - `base_mod_passes`: `data_check(&[base_mod_dir()])` is Ok and `units == 10`.
  - `bad_mod_lists_all_errors`: at least 3 errors, and each one's Display contains `bad_mod`.
  - `data_check_bad_mod_exit_code`: run the built binary via `env!("CARGO_BIN_EXE_fl_cli")` on the bad fixture. It exits with code 1, and its output contains all the errors.
  - `hash_dump_is_repeatable`: `base.testbed`, 100 ticks, interval 10. Two calls are equal and have 10 entries.
- [ ] **Step 2: Run** `cargo test -p fl_cli`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_cli`. Expected: PASS, and `cargo run -p fl_cli -- data-check assets/base` prints the report.
- [ ] **Step 5: Gate + commit** `feat(fl_cli): data-check and hash-dump`.

### Task 11: `fl_test_utils` — ScenarioBuilder and determinism twin-run

**Files:** `crates/fl_test_utils/src/{lib,builder,determinism}.rs` and `crates/fl_test_utils/tests/determinism.rs`.

**Interfaces — Produces:**
```rust
pub fn base_content() -> Arc<ContentDb>;            // OnceLock; fl_catalog features + base mod
pub struct ScenarioBuilder { … }
impl ScenarioBuilder {
    pub fn new() -> Self;                            // base_content(), seed 0, hash_interval 1, allow_debug_commands true
    pub fn with_content(content: Arc<ContentDb>, features: Vec<Arc<dyn GameFeature>>) -> Self;
    pub fn seed(self, seed: u64) -> Self;
    pub fn hash_interval(self, n: u32) -> Self;
    pub fn scenario(self, id: &str) -> Self;         // setup_scenario; its seed is then overridden by .seed() if given
    pub fn map_flat(self, width_m: i32, height_m: i32) -> Self;
    pub fn player(self, faction: &str, team: u8) -> Self;          // PlayerId in call order
    pub fn unit(self, unit: &str, player: u8, x: i32, y: i32) -> Self;   // builder units get SimIds after scenario units, in call order
    pub fn scale(self, s: ScaleScenario) -> Self;
    pub fn wander_all(self, radius_m: i32) -> Self;
    pub fn command<P: CommandPayload>(self, tick: u32, player: u8, units: &[u32], payload: P) -> Self;
    pub fn build(self) -> Scenario;                  // panics with a readable message on setup errors
}
pub struct Scenario { pub app: App }
impl Scenario {
    pub fn run_ticks(&mut self, n: u32);             // submits builder commands as their tick comes up
    pub fn tick(&self) -> Tick;
    pub fn hashes(&self) -> &[(Tick, u64)];
    pub fn world(&self) -> &World; pub fn world_mut(&mut self) -> &mut World;
    pub fn pose(&self, id: u32) -> Option<Pose>;
    pub fn issue<P: CommandPayload>(&mut self, player: u8, units: &[u32], payload: P, queue: bool);
}
pub fn assert_deterministic(make: impl Fn() -> ScenarioBuilder, ticks: u32);   // two runs; panics naming the first divergent tick and both hashes
```
- [ ] **Step 1: Write the failing tests** in `tests/determinism.rs`.
  - `testbed_twin_run_600_ticks`: base.testbed with `wander_all(100)`.
  - `scale_twin_run_500_units`: `ScaleScenario{units:500,..}` for 200 ticks.
  - `commands_mid_run_twin`: move commands at ticks 10, 50 and 90.
  - `different_seeds_diverge`: seeds 1 and 2 on a 500-unit scale scenario give different final hashes.
  - `builder_units_get_sequential_ids`.
  - `divergence_is_reported`: `#[should_panic(expected = "diverged at tick")]`. The builder reads a process-global counter so that one unit's position changes on the second run.
- [ ] **Step 2: Run** `cargo test -p fl_test_utils`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_test_utils`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_test_utils): ScenarioBuilder and determinism twin-run`.

### Task 12: `fl_client` camera, ground and lighting

**Files:** `crates/fl_client/src/camera.rs` and `crates/fl_client/src/map_view.rs`, and no others.

**Interfaces — Produces:**
```rust
// map_view.rs
#[derive(Resource, Clone, Debug)] pub struct MapView { pub size_m: Vec2, pub ground_color: Color }
// MapViewPlugin: on Startup (if MapView exists) spawn ground plane centred at (w/2, 0, h/2), a DirectionalLight
// (shadow_maps_enabled, cascades tuned for ≤ 2 km), GlobalAmbientLight; F3 toggles a 100 m gizmo grid.
// camera.rs
#[derive(Component, Clone, Debug)] pub struct RtsCamera { pub focus: Vec3, pub yaw: f32, pub pitch: f32, pub distance: f32, pub view_height: f32 }
//   defaults: yaw 45°, pitch 35.264°, distance 400, view_height 120
#[derive(Resource, Clone, Debug)] pub struct CameraSettings { pub pan_speed: f32 /* view-heights per s, 1.0 */, pub edge_margin_px: f32 /* 8 */,
    pub zoom_min: f32 /* 15 */, pub zoom_max: f32 /* 600 */, pub zoom_step: f32 /* 0.1 */, pub rotate_speed: f32 /* 90°/s in rad */ }
pub fn rig_transform(r: &RtsCamera) -> Transform;                       // as in api-reference §2.2
pub fn pan_input(keys: &ButtonInput<KeyCode>) -> Vec2;                   // WASD + arrows, (x right, y forward)
pub fn edge_pan_input(cursor: Option<Vec2>, window: Vec2, margin: f32) -> Vec2;   // zero if window has zero size or cursor None
pub fn pan_delta(input: Vec2, yaw: f32, view_height: f32, speed: f32, dt: f32) -> Vec3;
pub fn clamp_focus(focus: Vec3, size_m: Vec2) -> Vec3;
pub fn zoom(view_height: f32, scroll: f32, s: &CameraSettings) -> f32;
pub fn snap_yaw(yaw: f32) -> f32;                                        // nearest 45°
// CameraPlugin: spawns Camera3d + orthographic projection (ScalingMode::FixedVertical) + RtsCamera focused on map centre;
// Update systems: pan (keys + edge), zoom (AccumulatedMouseScroll), rotate (Q/E free, R snap), clamp, apply_rig.
```
- [ ] **Step 1: Write the failing tests.** Unit tests in `camera.rs`:
  - `pan_forward_moves_along_yaw`: yaw 0 and input (0,1) give delta.z < 0 and |delta.x| < 1e-5.
  - `edge_pan_left_edge`: cursor (2, 300) in an 800×600 window gives x = −1.
  - `edge_pan_zero_size_window`: gives `Vec2::ZERO`.
  - `zoom_is_clamped`: 10 000 wheel steps each way stay within [15, 600].
  - `clamp_focus_inside_map`, `snap_yaw_nearest_45`.
  - `rig_looks_at_focus`: `rig_transform(..).forward()` points from the translation to the focus, within 1e-4.

  One app test:
  - `wasd_moves_focus`: `MinimalPlugins` plus a manually inserted `ButtonInput<KeyCode>` resource. Press `KeyW`, set `TimeUpdateStrategy::ManualDuration(100 ms)`, and run 3 updates; the focus changes.
  - The systems must not require a window, so use `Option<Single<&Window, With<PrimaryWindow>>>`.
- [ ] **Step 2: Run** `cargo test -p fl_client camera`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_client camera`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_client): isometric RTS camera, ground and lighting`.

### Task 13: `fl_client` sim host, sync, interpolation, placeholders

**Files:** `crates/fl_client/src/view.rs` and `crates/fl_client/src/placeholder.rs`, and no others.

**Interfaces — Consumes:**
- fl_sim: `SimIndex`, `Pose`, `Owner`, `UnitType`, `SimClock`, `Players`, `CommandRegistry`, `submit`, `SimTick`.
- fl_content: `VisualDef`, `Placeholder`, `Faction`.

**Produces:**
```rust
// view.rs
#[derive(Resource)] pub struct SimHost { world: World, outbox: Vec<PlayerCommand>, pub paused: bool, pub step_requests: u32, last_tick: std::time::Duration }
impl SimHost {
    pub fn new(world: World) -> Self;                   // from build_sim_app(..) after setup: std::mem::take(app.world_mut())
    pub fn world(&self) -> &World;
    pub fn world_mut(&mut self) -> &mut World;          // debug/test use only; gameplay goes through issue()
    pub fn tick(&self) -> Tick;
    pub fn issue<P: CommandPayload>(&mut self, player: PlayerId, units: Vec<SimId>, payload: &P, queue: bool);  // stamped at the next tick to run
    pub fn last_tick_duration(&self) -> std::time::Duration;   // measured with bevy::platform::time::Instant (allowed in client)
    pub fn step(&mut self);                             // submit outbox, run SimTick once, measure, then drain fl_sim's Journal into `journal`
    pub fn journal(&self) -> &std::collections::VecDeque<JournalEntry>;  // last 10 000 entries (ring), feeds the combat log / BRP / file sink
}
#[derive(Resource, Clone)] pub struct ClientContent(pub Arc<ContentDb>);
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)] pub struct LocalPlayer(pub PlayerId);
#[derive(Component, Clone, Copy, Debug)] pub struct SimLink(pub SimId);
#[derive(Component, Clone, Copy, Debug)] pub struct UnitVisual { pub unit: UnitTypeId, pub player: PlayerId }
#[derive(Component, Clone, Copy, Debug)] pub struct ViewPose { pub prev: Vec3, pub curr: Vec3, pub prev_yaw: f32, pub curr_yaw: f32 }
#[derive(Resource, Default)] pub struct SimLinks(pub BTreeMap<SimId, Entity>);
pub fn sim_to_world(pos: FxVec2, height: f32) -> Vec3;      // Vec3::new(x, height, y)
pub fn heading_to_yaw(h: Angle) -> f32;                      // -h.to_radians_f32()
pub fn lerp_yaw(a: f32, b: f32, t: f32) -> f32;              // shortest arc
// SimViewPlugin: Time::<Fixed>::from_hz(SIM_TICK_HZ as f64); FixedUpdate: (step_sim, sync_views).chain()
//   step_sim: unless paused, host.step(); when paused, run step_requests ticks then zero it.
//   sync_views: merge SimIndex with SimLinks — spawn (SimLink, UnitVisual, ViewPose, Transform, Visibility) for new ids,
//   despawn (recursive) for vanished ids, shift curr→prev and copy new curr for all others.
// Update: interpolate_views sets Transform from ViewPose with Time<Fixed>::overstep_fraction().
// placeholder.rs
// PlaceholderPlugin: Startup builds PlaceholderAssets { meshes per UnitTypeId (from VisualDef, or a Box from Hull footprint), materials per PlayerId (faction colour) };
// on Added<UnitVisual> attach child meshes: Tank = hull cuboid + turret cuboid + barrel cylinder (along local +X), Truck = cab + bed,
// Infantry = capsule, Box = cuboid. Meshes/materials are shared handles so Bevy batches them.
```
- [ ] **Step 1: Write the failing tests.** They use `MinimalPlugins` and `SimViewPlugin` only, with no render.
  - Pure functions:
    - `sim_to_world_axes`.
    - `heading_90_faces_plus_z`: rotating +X by `heading_to_yaw(90°)` gives +Z within 1e-5.
    - `lerp_yaw_wraps`: from 170° to −170° at t = 0.5 gives ±180°.
  - App tests, with a SimHost built from `base.testbed` and `TimeUpdateStrategy::ManualDuration(50 ms)`:
    - `visuals_match_sim_units`: 3 updates give 10 `SimLink` entities.
    - `debug_spawn_adds_visual`: issuing `DebugSpawnCmd` gives 11.
    - `despawn_removes_visual`: despawning a SimId directly in the host world leaves 9 after the next fixed step, with no panic in interpolation.
    - `paused_host_does_not_tick`: when paused, the tick stays the same; `step_requests = 2` advances it by 2.
  - `interpolation_midpoint`: ViewPose with prev (0,0,0) and curr (10,0,0) at overstep 0.5 gives x = 5.
- [ ] **Step 2: Run** `cargo test -p fl_client view`. Expected: FAIL.
- [ ] **Step 3: Implement.** If `World` cannot live in a `Resource` on 0.20, fall back to `insert_non_send` and keep the same `SimHost` API.
- [ ] **Step 4: Run** `cargo test -p fl_client view`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_client): sim host, sim→client sync, interpolation, placeholder visuals`.

---

## Phase C — parallel tracks (worktrees, after their Phase B inputs are merged)

### Task 14: Debug tools (egui console, spawn panel, stats, time controls)

**Files:** `crates/fl_client/src/debug/{mod,console,spawn_panel,stats}.rs`, and no others.

**Interfaces — Consumes:** `SimHost`, `LocalPlayer`, `ClientContent`, `RtsCamera`. **Produces:**
```rust
pub type ConsoleFn = fn(&mut World, &[&str]) -> Result<String, String>;
#[derive(Resource, Default)] pub struct ConsoleCommands { /* BTreeMap<&'static str, (help, ConsoleFn)> */ }
pub trait ConsoleAppExt { fn register_console_command(&mut self, name: &'static str, help: &'static str, f: ConsoleFn) -> &mut Self; }
#[derive(Resource, Default)] pub struct ConsoleState { pub input: String, pub history: Vec<String>, pub open: bool }
pub fn run_console_line(world: &mut World, line: &str) -> String;   // tokenises, dispatches, "unknown command" otherwise
#[derive(Resource, Default)] pub struct CursorGround(pub Option<Vec3>);
#[derive(Resource)] pub struct SpawnPanel { pub unit: Option<UnitTypeId>, pub player: PlayerId, pub placing: bool, pub heading_deg: i32 }
```
**Built-in console commands:**
- `help`
- `spawn <unit_id> <player> <x> <y>`
- `wander <player> <radius>`
- `player <id>`: sets `LocalPlayer`.
- `pause`, `resume`, `step [n]`
- `speed <0.25..8>`: sets `Time<Virtual>::set_relative_speed`.
- `hash`: prints `compute_state_hash` of the host world.
- `tick`

**`DebugToolsPlugin`:**
- Adds `EguiPlugin::default()` only if it isn't already added, plus `FrameTimeDiagnosticsPlugin`.
- Draws the egui windows in `EguiPrimaryContextPass`:
  - the console, toggled with backquote or F1
  - the spawn panel
  - stats: FPS, sim tick, last tick ms, unit count, last logged state hash, rules hash, local player
  - time controls: pause, step, speed
- **Click-to-place.** While `placing`, a primary click that isn't over egui (`ctx.wants_pointer_input()` is false) issues `DebugSpawnCmd` at `CursorGround` for the panel's player.
- `CursorGround` is updated every frame by the ray-to-plane code in api-reference §2.5.

- [ ] **Step 1: Write the failing tests.** They work at World level, with no egui.
  - `console_spawn_queues_command`: with a SimHost on base.testbed, `run_console_line(w, "spawn base.de.pz3 0 500 500")` followed by one `host.step()` gives 11 units.
  - `console_unknown_command`.
  - `console_spawn_bad_args`: `spawn nope 0 1 1` returns error text and does not panic.
  - `console_player_switch`, `console_step_while_paused`.
- [ ] **Step 2: Run** `cargo test -p fl_client debug`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_client debug`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_client): egui debug console, spawn panel, stats and time controls`.

### Task 15: `GameBuilder` and the `fl_game` testbed binary

**Files:** `crates/fl_client/src/game.rs` and `crates/fl_game/src/main.rs`, and no others.

**Interfaces — Produces:**
```rust
pub enum StartMode { Scenario(String), Stress { units: u32, seed: u64 } }
pub struct GameBuilder { … }
impl GameBuilder {
    pub fn new() -> Self;                                     // StartMode::Scenario("base.testbed")
    pub fn feature(self, f: impl GameFeature) -> Self;
    pub fn features(self, fs: Vec<Arc<dyn GameFeature>>) -> Self;
    pub fn content_mod(self, dir: impl Into<PathBuf>) -> Self;
    pub fn scenario(self, id: &str) -> Self;
    pub fn stress(self, units: u32) -> Self;                  // ScaleScenario { units, ..default } with wander
    pub fn exit_after(self, secs: Option<f32>) -> Self;
    pub fn prepare(self) -> Result<App, String>;              // everything except run(); content/setup errors as text
    pub fn run(self) -> AppExit;                              // prepare() then app.run(); on Err print every line, return AppExit::error()
}
```
**`prepare`:**
1. `load_content`.
2. `build_sim_app(features, content, SimConfig { seed, hash_interval: None, allow_debug_commands: true })`.
3. `setup_scenario` or `ScaleScenario::populate`.
4. `SimHost::new(take world)`.
5. Build the client `App`:
   - `DefaultPlugins`, with window title `"Frontline — testbed"` and `present_mode: AutoNoVsync` in stress mode so FPS is measurable.
   - `MeshPickingPlugin`, `ClientPlugins`, and every `feature.client(&mut app)`.
   - Insert `SimHost`, `ClientContent`, `LocalPlayer(PlayerId(0))`, and `MapView` built from the sim's `MapInfo` and the map's ground colour.
6. Observability wiring (spec §15, uses T19's `fl_observe`):
   - Load `DebugConfig` (`debug.ron` / `RUST_LOG`).
   - In debug builds, create the run dir `logs/<run-id>/` and `fl_observe::set_global_run_dir` it before building the App.
   - Set `LogPlugin { filter, custom_layer: client_log_layers, .. }`. `client_log_layers(&mut App) -> Option<BoxedLayer>` is defined in `game.rs`. It boxes `global_profiler().layer()`, plus `json_file_layer(global_run_dir())` when present.
   - Add a `PostUpdate` system that appends new `SimHost::journal()` entries to `journal.jsonl` through `fl_observe::JournalSink`, and calls `global_profiler().end_tick(..)` once per sim tick.
   - `GameBuilder::debug_remote(bool)` and the `--remote` flag are stored for T21.

**`exit_after`:** after the duration, write `AppExit::Success` and log this line to stdout, ignoring the first 3 s:

`stress: units={n} avg_fps={:.1} p1_low_fps={:.1} frames={}`

**`fl_game` CLI (clap):**
- `--scenario <ID>`, default `base.testbed`.
- `--stress <N>`.
- `--exit-after <SECS>`.
- `--mods <DIR>...`, default `fl_catalog::base_mod_dir()`.

- [ ] **Step 1: Write the failing test** `builder_rejects_broken_content`: `GameBuilder::new().content_mod(<path to crates/fl_cli/tests/fixtures/bad_mod>).prepare()` is `Err` and its text names the bad file. This works without a window because content loads before the client App is built. Everything else is checked by running the binary in T17.
- [ ] **Step 2: Run** the test. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the test. Expected: PASS. Then `cargo run -p fl_game` opens the window. Manual check: ground, 10 placeholders, pan/zoom/rotate.
- [ ] **Step 5: Gate + commit** `feat(fl_game): GameBuilder and testbed binary with stress mode`.

### Task 16: Bench harness (`fl_cli bench` + criterion)

**Files:** `crates/fl_cli/src/bench.rs`; modify `crates/fl_cli/src/{lib,main}.rs` to add the subcommand; `crates/fl_cli/tests/bench.rs`; `crates/fl_sim/benches/sim_tick.rs`.

**Interfaces — Produces:**
```rust
pub struct BenchConfig { pub units: Vec<u32>, pub ticks: u32, pub warmup: u32, pub seed: u64 }
pub struct BenchRow { pub units: u32, pub mean_ms: f64, pub p95_ms: f64, pub max_ms: f64, pub per_set_mean_ms: Vec<(String, f64)> }
// per-SimSet times come from T18's fl_sim::observe::SetTimings (enable via SimConfig.profile = true);
// per-system / per-feature times come from T19's fl_observe::Profiler layer (installed by fl_cli for --profile)
pub fn run_bench(cfg: &BenchConfig) -> Vec<BenchRow>;        // ScaleScenario per size, base content, warmup excluded
pub fn profile_json(rows: &[BenchRow], snapshots: &[PerfSnapshot]) -> serde_json::Value;  // per size: per-set, per-feature, per-system p50/p95/max
pub fn render_markdown(rows: &[BenchRow], meta: &str) -> String;
pub fn compare(rows: &[BenchRow], baseline: &[BenchRow], max_regression: f64) -> Result<(), String>;  // mean_ms per size
```
**CLI:** `fl_cli bench [--units 500,3000,5000] [--ticks 400] [--warmup 40] [--seed 1] [--out <md>] [--save-baseline <ron>] [--baseline <ron> --max-regression 0.20] [--profile --json <path>]` prints the markdown table and exits 1 on a regression. `--profile --json docs/perf/m0.json` writes the per-system breakdown.

**Criterion bench `sim_tick`:** groups `tick/500`, `tick/3000` and `tick/5000`, each iteration one `SimTick` after a 40-tick warmup.

- [ ] **Step 1: Write the failing tests.**
  - `bench_reports_each_size`: `run_bench` with units `[50, 100]` and 20 ticks gives 2 rows. Every `per_set_mean_ms` has 9 sets, and the means are > 0.
  - `compare_flags_regression`: against a 1.0 ms baseline at 0.20, 1.3 ms is Err naming the size and 1.1 ms is Ok.
  - `instrumented_hash_unchanged`: the same scenario with profiling on and off gives an equal final state hash.
  - `profile_json_has_features`: `--profile` output for 100 units lists `core.movement` with p95 > 0 and at least one system name per non-empty SimSet.
- [ ] **Step 2: Run** `cargo test -p fl_cli --test bench`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_cli --test bench`. Expected: PASS, and `cargo bench -p fl_sim --bench sim_tick -- --quick` runs.
- [ ] **Step 5: Gate + commit** `feat(fl_cli): scale benchmark harness and criterion tick bench`.

---

## Phase D — integration

### Task 17: Integration, stress test, perf baseline, demo checklist

**Files:** Create `docs/perf/m0-baseline.md`, `docs/perf/baseline.ron`, `docs/perf/m0.json`, `docs/m0-demo-checklist.md` and `docs/debugging.md`; modify `README.md`.

- [ ] **Step 0:** Write `docs/debugging.md` for humans and agents (spec §15 C). It covers:
  - launching in debug mode (`--remote`, `debug.ron`, `RUST_LOG`, `trace_chrome` / `trace_tracy` features)
  - every `fl_cli` command with `--json` examples
  - every BRP method with example request and response JSON
  - log, journal and replay locations
  - the reproduce-a-bug recipe: seed + scenario + replay → `bisect-desync` → `inspect`

  Verify each documented command by running it.

- [ ] **Step 1:** With all branches merged into `main`, run the task gate. Expected: all green and `deps OK`.
- [ ] **Step 2:** `cargo run -p fl_cli --release -- data-check assets/base`. Expected: exit 0, 10 units, and the rules hash printed.
- [ ] **Step 3:** `cargo test -p fl_test_utils --release --test determinism`. Expected: PASS.
- [ ] **Step 4:** `cargo run -p fl_cli --release -- bench --units 500,3000,5000 --ticks 400 --warmup 40 --out docs/perf/m0-baseline.md --save-baseline docs/perf/baseline.ron --profile --json docs/perf/m0.json`. Expected: a table with three rows, and a 3000-unit mean of at most 15 ms (G7). Add machine, rustc and commit metadata.
- [ ] **Step 5:** `cargo run -p fl_game --release -- --stress 3000 --exit-after 25`. Expected: `stress: units=3000 avg_fps=…` with an average of at least 60; append that line to `docs/perf/m0-baseline.md`. If it is below 60, profile and fix (shared mesh handles, change-only sync) before closing M0.
- [ ] **Step 6:** `cargo run -p fl_game --release`, then walk through `docs/m0-demo-checklist.md`:
  - An iso view of the 1 km ground, with 10 placeholders in faction colours.
  - Camera: WASD, arrow and edge pan, wheel zoom, Q/E rotate, R snap, all clamped at the map edge.
  - Debug tools: console `spawn`, spawn-panel click-to-place, and `wander 0 100` showing smooth interpolated movement.
  - The stats panel shows the tick, FPS and hash.
- [ ] **Step 7:** Run a final whole-branch review (requesting-code-review), fix the findings, run the gate, and commit `docs: M0 perf baseline and demo checklist`.

---

## Observability tasks (spec §15; added after the user's observability requirement)

### Task 18: Sim observability core — journal, counters, set timings, replay (`fl_sim::observe`, `fl_sim::replay`)

Runs in Phase A after T7, in parallel with T8; files are disjoint.

**Files:**
- Create `crates/fl_sim/src/{observe,replay}.rs` and `crates/fl_sim/tests/{observe,replay}.rs`.
- Modify `crates/fl_sim/src/{lib,app_ext,build,commands,spawn}.rs` (module declarations, registration, recording calls, config) and `crates/fl_sim/Cargo.toml` (add `serde_json` if T6 didn't).

**Interfaces — Produces:**
```rust
// observe.rs
pub trait JournalRecord: Serialize + std::fmt::Debug + Send + Sync + 'static { const KIND: &'static str; }   // e.g. "core.command_applied"
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct JournalEntry { pub tick: Tick, pub kind: String, pub sim_id: Option<SimId>, pub player: Option<PlayerId>, pub data: serde_json::Value }
#[derive(Resource)] pub struct Journal { /* enabled: bool, entries: Vec<JournalEntry>, cap: usize = 1_000_000 */ }
impl Journal {
    pub fn enabled(&self) -> bool;
    pub fn record<R: JournalRecord>(&mut self, tick: Tick, sim_id: Option<SimId>, player: Option<PlayerId>, rec: &R);  // no-op when disabled; debug_assert kind registered
    pub fn drain(&mut self) -> Vec<JournalEntry>;      // host takes everything recorded so far
    pub fn len(&self) -> usize;
}
#[derive(Resource, Default)] pub struct SimCounters { /* BTreeMap<&'static str, u64>, reset at tick start */ }   // fn add(&mut self, name: &'static str, n: u64); fn snapshot(&self) -> Vec<(&'static str, u64)>
#[derive(Resource, Default)] pub struct SetTimings { /* last tick: [(SimSet name, Duration); 9] */ }            // filled by marker systems when SimConfig.profile
pub fn current_tick() -> u32;   // static AtomicU32 set by the first ApplyCommands system; read by log layers only
// SimAppExt gains: fn register_journal_record<R: JournalRecord>(&mut self) -> &mut Self;
// build: Journal, SimCounters and SetTimings are registered with app.allow_ambiguous_resource::<_>() so unordered systems may write them
// SimConfig gains: pub journal: Option<bool> (None = cfg!(debug_assertions)), pub profile: bool
// Core records: CommandApplied { command: String, units: Vec<SimId>, queue: bool }, CommandRejected { command: String, reason: String },
//               UnitSpawned { unit: String, pos: FxVec2 }, UnitDespawned {}, MoveOrderChanged { waypoints: usize }
// replay.rs
#[derive(Clone, Debug, Serialize, Deserialize)] pub enum ReplayStart { Scenario(String), Scale(ScaleScenario) }   // ScaleScenario derives Serialize/Deserialize
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Replay { pub version: u32, pub rules_hash: u64, pub seed: u64, pub start: ReplayStart, pub ticks: u32, pub commands: Vec<PlayerCommand>, pub hashes: Vec<(Tick, u64)> }
impl Replay {
    pub fn capture(world: &World, start: ReplayStart, seed: u64) -> Replay;                 // from CommandLog.applied + StateHashLog
    pub fn save_json(&self, path: &Path) -> std::io::Result<()>; pub fn load_json(path: &Path) -> std::io::Result<Replay>;
    pub fn build_world(&self, features: &[Arc<dyn GameFeature>], content: Arc<ContentDb>, hash_interval: u32) -> Result<App, ReplayError>;
}
pub fn verify_replay(r: &Replay, features: &[Arc<dyn GameFeature>], content: Arc<ContentDb>) -> Result<(), Divergence>;  // re-run; compare every recorded hash
#[derive(Debug, Clone, PartialEq)] pub struct Divergence { pub tick: Tick, pub expected: u64, pub actual: u64 }
pub fn first_divergence(a: &[(Tick, u64)], b: &[(Tick, u64)]) -> Option<Divergence>;
```
- [ ] **Step 1: Write the failing tests.**
  - `tests/observe.rs`:
    - `applied_and_rejected_commands_are_journaled`: one move plus one foreign-unit move gives exactly one `core.command_applied` and one `core.command_rejected` with reason `NoValidUnits`.
    - `journal_disabled_records_nothing`.
    - `journal_does_not_change_state_hash`: journal on and off give the same final hash.
    - `counters_reset_each_tick`.
    - `set_timings_cover_all_nine_sets_when_profiling`.
    - `unordered_systems_may_write_journal`: two unordered test systems record without an ambiguity panic.
    - `current_tick_tracks_clock`.
  - `tests/replay.rs`:
    - `replay_roundtrip_verifies`: run the duel with moves, capture, save to JSON, load, and `verify_replay` is Ok.
    - `tampered_replay_reports_divergence`: change one command's target, and the result is `Err(Divergence)` at the right tick.
    - `first_divergence_finds_earliest`.
- [ ] **Step 2: Run** `cargo test -p fl_sim --test observe --test replay`. Expected: FAIL.
- [ ] **Step 3: Implement.** The set-timing marker systems are added only when `SimConfig.profile` is set. They use `std::time::Instant` under a local `#[allow(clippy::disallowed_types)]` and write `SetTimings` only.
- [ ] **Step 4: Run** the same tests. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_sim): event journal, counters, set timings, replays`.

### Task 19: `fl_observe` — logging setup, profiler layer, journal sink, debug.ron

Phase B, its own worktree.

**Files:**
- Create the crate `crates/fl_observe/` (`Cargo.toml`, `src/{lib,config,logging,profiler,sink}.rs`, `tests/profiler.rs`) and `debug.ron` at the repo root.
- Modify the root `Cargo.toml` to add the workspace deps `tracing-subscriber` (features `env-filter`, `json`, `fmt`, `registry`), `serde_json` if absent, and the `fl_observe` path dep.
- Modify `scripts/check_deps.sh` to add `fl_observe` to the headless list.
- `fl_observe` deps: fl_sim, fl_protocol, tracing, tracing-subscriber, serde, serde_json, ron, thiserror. No Bevy beyond what fl_sim brings.

**Interfaces — Produces:**
```rust
#[derive(Clone, Debug, Deserialize, Serialize)] #[serde(deny_unknown_fields, default)]
pub struct DebugConfig { pub log_filter: String /* "info,fl_sim=debug" */, pub json_log: bool, pub journal_file: bool, pub profiler: bool, pub budgets: Budgets }
#[derive(Clone, Debug, Default, Deserialize, Serialize)] #[serde(deny_unknown_fields, default)]
pub struct Budgets { pub tick_ms: f64 /* 15.0 */, pub per_set_ms: BTreeMap<String, f64>, pub per_feature_ms: BTreeMap<String, f64> }
pub fn load_debug_config(path: Option<&Path>) -> DebugConfig;     // explicit path, else ./debug.ron if present, else defaults; RUST_LOG overrides log_filter
pub struct RunDir { pub id: String, pub path: PathBuf }
pub fn new_run_dir(root: &Path) -> std::io::Result<RunDir>;       // root/<YYYYMMDD-HHMMSS>-<pid>/ ; also writes root/latest.txt with the id
pub fn json_file_layer<S>(run: &RunDir) -> impl Layer<S>;           // log.jsonl; each line: timestamp, level, target, fields, span names, "tick": current_tick()
pub fn init_headless(cfg: &DebugConfig, run: Option<&RunDir>) -> &'static Profiler;   // EnvFilter + stderr fmt + json file (if run) + profiler layer; idempotent
pub fn global_run_dir() -> Option<&'static RunDir>;  pub fn set_global_run_dir(run: RunDir);   // lets fn-pointer layer factories (Bevy LogPlugin) find the run dir
pub fn global_profiler() -> &'static Profiler;
pub struct Profiler { … }
impl Profiler {
    pub fn layer<S: Subscriber + for<'a> LookupSpan<'a>>(&'static self) -> impl Layer<S>;  // times `system` spans (bevy_ecs trace) by `name` field
    pub fn end_tick(&self, tick: u32, sets: &[(&'static str, Duration)], counters: &[(&'static str, u64)]);  // folds the tick, checks budgets → warn!(target: "fl_observe::budget", …)
    pub fn set_budgets(&self, b: Budgets);
    pub fn snapshot(&self) -> PerfSnapshot;
    pub fn reset(&self);
}
#[derive(Clone, Debug, Serialize)] pub struct Stat { pub mean_ms: f64, pub p50_ms: f64, pub p95_ms: f64, pub max_ms: f64, pub samples: u64 }
#[derive(Clone, Debug, Serialize)] pub struct PerfSnapshot { pub ticks: u64, pub tick: Stat, pub sets: Vec<(String, Stat)>, pub features: Vec<(String, Stat)>,
    pub systems: Vec<SystemStat>, pub counters: Vec<(String, u64)>, pub history_ms: Vec<f32> /* last 240 ticks, for sparklines */ }
#[derive(Clone, Debug, Serialize)] pub struct SystemStat { pub name: String, pub feature: String, pub stat: Stat, pub history_ms: Vec<f32> }
pub fn feature_of(system_name: &str) -> String;   // "fl_feat_artillery::fire::x" → "artillery"; "fl_sim::movement::move_system" → "core.movement"; other → crate name
pub struct JournalSink { … }   // fn create(run: &RunDir) -> io::Result<Self>; fn write(&mut self, entries: &[JournalEntry]) -> io::Result<()>  (journal.jsonl, one JSON object per line)
```
- Rolling statistics use a fixed window of the last 240 ticks per series, with nearest-rank percentiles.
- System spans that fire outside a sim tick (client systems) are attributed by the same `feature_of`. Client systems therefore show up under `fl_client` and `bevy_*`, which is useful for the client panel.

- [ ] **Step 1: Write the failing tests** (`tests/profiler.rs` plus unit tests).
  - `feature_of_maps_paths`: covers the cases in the interface comment.
  - `profiler_times_system_spans`: build a sim app with `SimConfig.profile = true` and a scoped subscriber with `Profiler::layer` (use a fresh non-global `Profiler` via `Profiler::new()`). Run 30 ticks, calling `end_tick` each tick. The snapshot has `fl_sim::movement::move_system` with samples ≥ 30 and feature `core.movement`.
  - `budget_overrun_warns`: `tick_ms: 0.0` gives a captured `fl_observe::budget` warning.
  - `percentiles_nearest_rank`: [1..=100] gives p50 = 50 and p95 = 95.
  - `debug_config_defaults_and_rust_log_override`.
  - `json_layer_writes_tick_field`: a temp run dir under `target/`; log one event and read back a line with `"tick"`.
  - `journal_sink_writes_jsonl`.
- [ ] **Step 2: Run** `cargo test -p fl_observe`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_observe`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_observe): logging setup, sim profiler layer, journal sink, debug.ron`.

### Task 20: `fl_cli` agent commands (`--json` everywhere)

Phase C2, after T16. It owns `crates/fl_cli/src/{agent,inspect,journal_cmd}.rs` and modifies `lib.rs` and `main.rs`.

**Interfaces — Produces.** A global `--json` flag. With it, every subcommand prints exactly one JSON document to stdout; logs go to stderr. Without it, output is human-readable.

| command | behaviour |
|---|---|
| `run-scenario <ID|path.scenario.ron> --ticks N [--seed S] [--trace] [--dump-state-every K] [--out <dir>]` | Runs headless. Writes `<run dir>/{log.jsonl, journal.jsonl, replay.json, states.jsonl}`; states holds `inspect_world` plus every entity every K ticks. `--trace` sets the filter to `trace`. JSON output: `{run_dir, ticks, final_hash, rules_hash, journal_entries, rejected_commands}`. |
| `inspect <replay.json|scenario> --tick T [--entity N]` | Re-simulates to tick T and prints `inspect_world`, or `inspect_entity` when `--entity` is given. |
| `journal <run dir|journal.jsonl> [--kind K] [--sim-id N] [--player P] [--from T] [--to T] [--limit L]` | Filters and prints entries in tick order. |
| `replay-verify <replay.json>` | Exit 0 if every hash matches. Otherwise exit 1, printing `{tick, expected, actual}`. |
| `bisect-desync <a> <b>` | `a`/`b` are replays or run dirs. It re-simulates both with hash interval 1 and reports the first divergent tick, plus `inspect_world` diffs of both at that tick (entity ids whose dumps differ). |
| `data-check`, `hash-dump`, `bench` | Gain `--json`. |

- [ ] **Step 1: Write the failing tests** (`tests/agent.rs`; run the binary with `--json` and parse stdout with serde_json).
  - `run_scenario_json_outputs_paths`: the files exist and the journal has ≥ 1 entry.
  - `inspect_entity_at_tick`: base.testbed at tick 10, entity 1, gives a JSON object with `components.Pose`.
  - `journal_filter_by_kind`.
  - `replay_verify_ok_and_tampered`: an edited replay exits 1 with the tick.
  - `bisect_desync_finds_tick`: two replays that differ by one command at tick 37 give a first divergence at tick 37, or 38 if the hash is recorded after the tick; document which.
  - `scenario_file_path_accepted`.
- [ ] **Step 2: Run** `cargo test -p fl_cli --test agent`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_cli --test agent`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_cli): agent debugging commands with --json`.

### Task 21: Live debug mode — BRP methods, perf panel, render stats, trace features

Phase C2, after T14 and T15.

**Files:**
- Create `crates/fl_client/src/debug/{remote,perf}.rs` and `crates/fl_client/tests/remote.rs`.
- Modify `crates/fl_client/src/debug/mod.rs` (register submodules), `crates/fl_client/Cargo.toml` and `crates/fl_game/Cargo.toml` (features), and `crates/fl_client/src/game.rs` (`.debug_remote(true)` adds `RemoteDebugPlugin`).

**Features:**
- `fl_client` gets `debug_remote = ["bevy/bevy_remote"]`.
- `fl_game` gets `debug_remote = ["fl_client/debug_remote"]`, `trace = ["bevy/trace"]`, `trace_tracy = ["bevy/trace_tracy"]` and `trace_chrome = ["bevy/trace_chrome"]`. `fl_game` gains a direct optional `bevy` dep only if needed for the pass-through.

**Interfaces — Produces:**
- `RemoteDebugPlugin` (under `cfg(feature = "debug_remote")`) adds Bevy's remote plugin and HTTP plugin on `127.0.0.1:15702`, or `--remote-port`.
  - Before writing any code, check the bevy_remote 0.20 API: `cargo doc`, or read `~/.cargo/registry/src/*/bevy_remote-0.20.0`. Use its plugin names and custom method registration.
- Custom methods. Params and results are JSON. Commands go through `SimHost::issue`, never direct mutation, except the debug pause/step/speed controls:
  - `fl/sim_state` → `inspect_world`
  - `fl/entity {sim_id}` → `inspect_entity`
  - `fl/issue_command {player, units, command, payload}`: `command` is the command NAME (`core.move`) and `payload` is the payload as JSON, deserialized through a registry of JSON decoders built from `CommandRegistry` names. Each command kind registers a JSON decoder via `register_command`; if T7's registry lacks it, add `CommandRegistry::encode_json(name, serde_json::Value) -> Result<CommandKind, String>` in `fl_sim::commands` with a test.
  - `fl/pause`, `fl/resume`, `fl/step {n}` (returns the new tick), `fl/set_speed {x}`
  - `fl/spawn {unit, player, x, y}`
  - `fl/journal_since {tick}` → entries from `SimHost::journal()`
  - `fl/perf` → `global_profiler().snapshot()` plus client frame stats
  - `fl/screenshot {path}`: uses Bevy's `Screenshot` + `save_to_disk`; returns an error without a window.
  - `fl/toggle_overlay {name}`: `grid` (F3) and the others registered in an `Overlays` resource.
  - `fl/load_scenario {id}`: rebuilds the `SimHost`.
  - `fl/save_scenario {path}`: writes the current units as a `*.scenario.ron` that `fl_content` can load.
- Perf panel (`debug/perf.rs`, toggled with F2): an egui table of features, sets and systems with p50/p95/max, a sparkline per row from `history_ms`, frame-time and FPS graphs from `DiagnosticsStore`, entity count (`EntityCountDiagnosticsPlugin`), and render stats (visible unit visuals, draw-relevant entity count).

- [ ] **Step 1: Write the failing test** `remote_step_state_command` (`tests/remote.rs`, `#![cfg(feature = "debug_remote")]`).
  - Build a client `App` with `MinimalPlugins`, `SimViewPlugin` and `RemoteDebugPlugin` on a free port, with a SimHost from `base.testbed`.
  - Pump `app.update()` on the test thread while a spawned thread sends raw HTTP POSTs over `std::net::TcpStream`: JSON-RPC `fl/step {n:5}`, then `fl/sim_state` (tick ≥ 5), then `fl/issue_command` (core.move for unit 1), then `fl/step {n:40}` and `fl/entity {sim_id:1}` (position changed).
  - Run it with `cargo test -p fl_client --features debug_remote --test remote`.
- [ ] **Step 2: Run** the test. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the test. Expected: PASS. `cargo run -p fl_game --features debug_remote -- --remote` serves `fl/sim_state`; check it with `curl`.
- [ ] **Step 5: Gate + commit** `feat(fl_client): BRP live debug methods, perf panel, trace features`. The gate also runs `cargo clippy -p fl_client --features debug_remote --all-targets -- -D warnings`.

---

## Roadmap M1–M9 (milestone level; each gets its own detailed plan before work starts)

Every milestone ends with all of these:
- journal records, log targets and profiler counters/spans for every new mechanic (spec §15), plus a `docs/debugging.md` update for any new CLI command or BRP method
- the task gate green
- `data-check` green
- determinism twin-runs for every new scenario test
- the bench re-run against `docs/perf/baseline.ron`, with a regression of at most 20% or a written justification
- a demo checklist
- baselines updated in `docs/perf/`

| M | Scope (design §9) | Where | Parallel tracks after its own contracts |
|---|---|---|---|
| **M1 Control & movement** | **Terrain:** height and terrain-type PNG layers (roads, forest, river, bridge), the speed formula, movement-class blocking and clearance. **Pathfinding:** HPA* with 16×16 clusters and a budgeted request queue, flow fields for group moves, steering and separation, hull rotation. **Selection:** click, box, Shift-add, double-click for the same type, control groups. **Commands:** Move, Stop, Rotate, Follow, Shift-queue. **Testbed:** pause while still giving orders, game speed, player switch; path, passability and cell overlays. **Also:** content hot reload through Bevy `file_watcher` (deferred from M0), and a do-nothing `Controller::Ai` with a `PlayerView` stub. | `fl_sim::{map, movement}`, `fl_client::{selection, input}`, `fl_content` (terrain kind) | pathfinding ∥ terrain mesh/overlays ∥ selection/input ∥ hot reload |
| **M2 Direct-fire combat** | **Data:** weapon and shell kinds, the Turrets def. **Mechanics:** firing cycle, dispersion-ellipse hit test, armour facing, penetration, damage, HE splash, ammo, track breaks, wrecks. **Targeting:** acquisition staggered by `SimId % N`, computed in a pure parallel phase with an ordered apply. **Commands:** Attack, AttackGround, Swarm, StandGround. **Tools and presentation:** range and armour overlays, a combat log, tracer and impact VFX, and CC0 sounds (ask before downloading). | `fl_sim::combat`, `fl_client::{vfx, audio, debug}` | ballistics/penetration ∥ targeting ∥ VFX/audio ∥ overlays |
| **M3 Vision** | Per-team ref-counted visibility grid with shadowcasting templates, height bonus and time-sliced updates. Camouflage and reveal, ghosts. A fog material (WESL `ExtendedMaterial` sampling an R8Unorm texture); enemies hidden in fog. `PlayerView` for the AI. | `fl_sim::vision`, `fl_client::fog` | LOS/visibility ∥ fog shader ∥ ghosts |
| **M4 Infantry & buildings** | `fl_feat_infantry`: squads, formations, prone and suppression, cover, grenades, garrisoning buildings and trenches, building damage, enter/leave, transports. Adds the building object kind. | `features/fl_feat_infantry` (+ one line in `fl_catalog`) | squads ∥ buildings/garrison ∥ transports |
| **M5 Artillery, supply, morale** | `fl_feat_artillery`: indirect fire, spotting, minimum range, ranging, counter-battery circles, crews, capturable guns. `fl_feat_supply`: generic resources, depots, trucks, resupply, re-manning, morale. | two feature crates | artillery ∥ supply |
| **M6 HUD & art** | **HUD:** command panel on a QWER/ASDF/ZXCV grid built from feature-contributed buttons (`ui_widgets::Button` + `On<Activate>`), info panel, status bars, minimap with fog, icons, OFL fonts. **Art:** the first CC0/CC-BY models through `VisualDef.model` + `WorldAssetRoot`, plus `credits.ron` and a credits screen. Asset downloads need the user's OK. | `fl_client::ui`, assets | HUD ∥ minimap ∥ model pipeline |
| **M7 Engineering, towing, entrenchment** | `fl_feat_engineering`: AP/AT mines, wire, trenches, AT obstacles, repairs, bridges. `fl_feat_towing`: towing force vs weight, hook/unhook, deploy/pack. Entrenchment. | two feature crates | engineering ∥ towing |
| **M8 Air support & AA** | `fl_feat_air_support`: recon, bombers, attack aircraft, fighters and paratroopers, called in from the map edge with cooldowns; AA ceiling. | feature crate | aircraft ∥ AA |
| **M9 Experience & specials** | `fl_feat_experience` (4 ranks as a stat-modifier source), snipers, officers with binoculars, smoke, `fl_feat_weather`, medics, storming buildings. | feature crates | one track per feature |

Roster growth: each milestone adds the unit types its mechanic exercises. The design §9 roster gives each faction 3 tanks, an AT gun, a howitzer, a mortar, an AA gun, a rifle squad with officer, an HMG team, a sniper, a supply truck and an engineer truck; aircraft arrive in M8.

## Deviations and builder's calls recorded here

- `Fx` = `fixed::I32F32`: range ±2.1·10⁹, resolution 2.3·10⁻¹⁰, so 2 km maps and squared distances fit. `Angle` is a 16-bit binary angle, with an integer-generated quarter-wave sine table and CORDIC atan2.
- The client holds the sim as a `World` inside the `SimHost` resource rather than a `SubApp`, and steps it once per client `FixedUpdate` at 20 Hz.
- New crate `fl_catalog`: the single list of linked feature crates. `fl_game` and `fl_cli` share it, so data-check validates exactly what the game registers.
- `Faction`, `MapDef`, `ScenarioDef` and `VisualDef` are built-in content kinds in `fl_content` (pure data), so a headless data-check validates them. The sim never reads `VisualDef`.
- M0 loads content with `std::fs`, not the `AssetServer`; hot reload moves to M1.
- `Wander` (random movement) is a debug and bench behaviour, driven by the `debug.wander` command or the scale scenario.
- Editing happens in worktrees under `C:\fl\`, because the session's guard blocks the Write and Edit tools under `C:\Projects\`.
