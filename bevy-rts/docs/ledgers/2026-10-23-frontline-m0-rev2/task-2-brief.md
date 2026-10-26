### Task 2: `xtask` gate levels, arch-check, generated clippy bans, cargo-deny, nextest profile

**Track:** contract · **Refs:** ADR-0009, ADR-0001 · **Gate:** interim, then `cargo xtask gate task` from Step 6

**Files:**
- Create:
  - `xtask/src/{gate,changed,arch,clippy_gen,cmd}.rs`, `xtask/clippy-template.toml`
  - `xtask/tests/fixtures/{metadata_ok,metadata_client_sim,metadata_features}.json`, made by running `cargo metadata --format-version 1` on the workspace and then hand-editing the copies
  - `deny.toml`, `.config/nextest.toml`
  - generated `crates/{fl_math,fl_protocol,fl_view,fl_content,fl_sim}/clippy.toml`
- Modify: `xtask/src/main.rs`, root `clippy.toml` (now generated, client profile), `README.md` (gate and run sections)
- Delete: `scripts/check_deps.sh`

**Interfaces:**
- Produces:
```rust
// gate.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)] pub enum Level { Task, Pr, Nightly, Milestone }
#[derive(Debug, Clone, PartialEq)] pub enum Outcome { Pass, Warn(String), Skip(String), Fail(String) }
pub struct Ctx { pub root: PathBuf, pub level: Level, pub base: String, pub touched: Vec<String>, pub tracks: Vec<String>, pub milestone: Option<String> }
pub struct Step { pub name: &'static str, pub levels: &'static [Level], pub run: fn(&Ctx) -> Outcome }
pub fn steps() -> Vec<Step>;                     // ordered table; Tasks 3, 4 and 28 append rows
pub struct GateReport { pub level: Level, pub rows: Vec<(&'static str, Outcome, Duration)>, pub total: Duration }
impl GateReport { pub fn passed(&self) -> bool; pub fn render(&self) -> String; }
// changed.rs
pub fn merge_base(root: &Path, base: &str) -> anyhow::Result<String>;
pub fn changed_files(root: &Path, base_rev: &str) -> anyhow::Result<Vec<PathBuf>>;   // committed since base ∪ staged ∪ unstaged ∪ untracked; repo-relative, sorted
pub fn touched_packages(meta: &cargo_metadata::Metadata, files: &[PathBuf]) -> Vec<String>;   // root Cargo.toml/Cargo.lock/.cargo/** → every package
// arch.rs
pub struct Violation { pub krate: String, pub detail: String }
pub fn check_edges(meta: &cargo_metadata::Metadata) -> Vec<Violation>;
pub fn check_tree(krate: &str, tree: &str) -> Vec<Violation>;   // `cargo tree -p <krate> -e normal,build --prefix none` output
// clippy_gen.rs
pub struct Template { /* parsed clippy-template.toml */ }
pub fn render(t: &Template, krate: &str, resolvable: &dyn Fn(&str) -> bool) -> Option<String>;
pub fn drift(root: &Path) -> anyhow::Result<Vec<PathBuf>>;
```
- CLI: `cargo xtask gate <task|pr|nightly|milestone> [--base main] [--track T]... [--milestone M0]`, `cargo xtask arch-check`, `cargo xtask gen-clippy [--check]`.

**Allowed workspace edges** (`check_edges`; normal and build edges, plus dev edges for client crates):

| crate | may depend on |
|---|---|
| fl_math | — |
| fl_protocol | fl_math |
| fl_view | fl_math, fl_protocol |
| fl_content | fl_math, fl_protocol |
| fl_sim | fl_math, fl_protocol, fl_view, fl_content |
| fl_feat_x | fl_math, fl_protocol, fl_view, fl_content, fl_sim, plus `fl_feat_y` for each y in `[package.metadata.fl] sim_deps` |
| fl_feat_x_client | fl_math, fl_protocol, fl_view, fl_content, fl_client, fl_feat_x |
| fl_catalog | fl_sim, fl_feat_* (not `_client`) |
| fl_observe | fl_math, fl_protocol, fl_view |
| fl_test_utils | fl_math, fl_protocol, fl_view, fl_content, fl_sim, fl_catalog |
| fl_cli | the above headless crates plus fl_test_utils, fl_observe |
| fl_client | fl_math, fl_protocol, fl_view, fl_content (dev edges checked too: never fl_sim) |
| fl_game | anything except xtask and probes |
| xtask | — |
| probes/* | unchecked |

**Banned external crates** (`check_tree`):
- `fl_math`, `fl_protocol`, `fl_view`: any crate whose name starts with `bevy`.
- `fl_content`: any `bevy*` except the `bevy_ecs` family: `bevy_ecs`, `bevy_ecs_macros`, `bevy_ecs_macro_logic`, `bevy_ptr`, `bevy_utils`, `bevy_platform`, `bevy_tasks`, `bevy_reflect`, `bevy_reflect_derive`, `bevy_macro_utils`.
- Headless crates (`fl_sim`, `fl_feat_*` not `_client`, `fl_catalog`, `fl_observe`, `fl_test_utils`, `fl_cli`): `bevy`, `bevy_internal`, `bevy_render`, `bevy_window`, `bevy_input`, `bevy_time`, `bevy_winit` and `wgpu`.

**Clippy template profiles** (`xtask/clippy-template.toml`):
- **`sim`** (`fl_protocol`, `fl_view`, `fl_sim`, `fl_feat_*` not `_client`):
  - `disallowed-types`: `f32`, `f64`, `std::collections::{HashMap,HashSet}`, `bevy_platform::collections::{HashMap,HashSet}`, `std::time::{Instant,SystemTime}`, `bevy_ecs::system::ParallelCommands`, `bevy_time::Time`, `fl_math::Pcg32`
  - `disallowed-methods`: `std::time::SystemTime::now`, `std::time::Instant::now`, `std::thread::spawn`, `rand::rng`, `rand::random`, `std::env::var`
- **`edge`** (`fl_math`, `fl_content`): the collection, time, thread and env bans only.
- **`client`** (the root `clippy.toml`, which every other crate reads): `std::collections::{HashMap,HashSet}`, `std::time::{Instant,SystemTime}`.
- **`[[pending]]`** `{ crate, path, until }` entries are omitted from the generated file, with a comment `# pending: <path> until <until>`. Seed it with `{ crate = "fl_sim", path = "fl_math::Pcg32", until = "Task 10" }`, plus any other ban that existing code violates, each naming the task that removes it. Never drop a ban silently.
- Only paths that resolve in the crate's dependency graph are emitted.

**Gate rows added by this task:**

| step | levels | command and rule |
|---|---|---|
| `fmt` | all | `cargo fmt --all --check` |
| `clippy-drift` | all | `gen-clippy --check` |
| `clippy` | task (touched), pr/milestone (workspace minus probes) | `cargo clippy -p … --all-targets -- -D warnings` |
| `arch-check` | task, pr, milestone | `check_edges` plus `check_tree` for every headless or pure crate |
| `test-headless` | all | headless packages only, so the sim proves it builds without client code: `cargo nextest run --profile agent -p <pkgs> -E '<filter>'`. **Task:** touched headless packages, then `-p fl_sim -p fl_test_utils -E 'test(/smoke_/)'`. **Pr:** all headless packages. The filter excludes `gpu_` and `brp_` always, and `milestone_` below milestone. Env: `FL_SHUFFLE_SEEDS=1` (task) or `3` (pr, milestone); `PROPTEST_CASES=4096` (nightly). |
| `test-client` | task (if `fl_client`/`fl_game` touched), pr, milestone | `cargo nextest run --profile agent -p fl_client -p fl_game` with the same filter |
| `deny` | pr, milestone | `cargo deny check` |
| `data-check` | task, pr, milestone | `cargo run -q -p fl_cli -- data-check assets/base`. Skip with a reason until both `crates/fl_cli/src/data_check.rs` and `assets/base/mod.ron` exist. |
| `gpu` | nightly, milestone | `cargo nextest run --profile agent --workspace -E 'test(/gpu_/)'` |
| `brp` | nightly, milestone | `cargo nextest run --profile agent -p fl_game --features debug_remote -E 'test(/brp_/)'` |
| `perf-file` | milestone | `docs/perf/<milestone>.json` exists |

**Output format.**
- One line per step: `PASS fmt (2.1s)`, `WARN …`, `SKIP data-check: <reason>` or `FAIL clippy (34.0s)`.
- Each FAIL line is followed by at most 40 lines of the failing command's tail.
- Last line: `gate <level>: PASS in <n>s` or `gate <level>: FAIL (<step>, <step>) in <n>s`.
- Exit code 0 or 1. Above 300 s at task level, print `WARN gate task took <n>s (budget 300s)`.

**`deny.toml`:**
- Licences: allow the permissive set the tree needs (MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2/3-Clause, ISC, Zlib, Unicode-3.0, CC0-1.0, MPL-2.0); never anything NC, ND, SA or GPL-family.
- `[bans] deny = [{ name = "bincode" }, { name = "bevy_dynamic_plugin" }]`.
- Multiple versions of any `bevy_*` crate fail. Use the per-crate key that the installed cargo-deny supports.
- Advisories from the default database; sources from crates.io only.

**`.config/nextest.toml`:**
- `[profile.agent]`: `failure-output = "final"`, `success-output = "never"`, `status-level = "fail"`, `final-status-level = "fail"`, `fail-fast = { max-fail = 5 }`, `slow-timeout = { period = "60s", terminate-after = 3 }`, and `junit.path = "junit.xml"`.
- `[test-groups] gpu = { max-threads = 1 }`, with an override that puts `test(/gpu_/)` in it.

- [ ] **Step 1: Write the failing tests** (`xtask/src/*` unit tests; no cargo invocations):
```rust
#[test] fn touched_maps_files_to_packages()        // ["crates/fl_sim/src/x.rs", "docs/a.md"] → ["fl_sim"]
#[test] fn root_manifest_touches_everything()      // ["Cargo.toml"] → every package
#[test] fn edges_flag_client_to_sim()              // metadata_client_sim.json → [Violation { krate: "fl_client", detail: "fl_client -> fl_sim not allowed" }]
#[test] fn edges_accept_fixture_workspace()        // metadata_ok.json → []
#[test] fn feature_edge_needs_declared_dep()       // metadata_features.json: fl_feat_b -> fl_feat_a without sim_deps = ["a"] → 1 violation
#[test] fn tree_flags_render_in_headless_crate()   // check_tree("fl_sim", "bevy_render v0.20.0") → 1; check_tree("fl_client", same) → 0
#[test] fn tree_flags_any_bevy_in_pure_crate()     // check_tree("fl_view", "bevy_ecs v0.20.0") → 1
#[test] fn tree_allows_ecs_family_in_content()     // fl_content: "bevy_ecs v0.20.0\nbevy_ptr v0.20.0" → 0; "bevy_app v0.20.0" → 1
#[test] fn render_omits_unresolvable_paths()       // fl_view (no bevy_time dep) → no "bevy_time::Time"; fl_protocol → contains "\"f32\""
#[test] fn pending_entry_is_omitted_with_comment() // fl_sim → no "fl_math::Pcg32" entry; contains "# pending: fl_math::Pcg32 until Task 10"
#[test] fn report_renders_summary_line()           // rows [fmt Pass, clippy Fail] → last line == "gate task: FAIL (clippy) in 3s"
```
- [ ] **Step 2: Run** `cargo test -p xtask`. Expected: FAIL (items not defined).
- [ ] **Step 3: Implement** the modules, the templates, `deny.toml` and `nextest.toml`. Run `cargo xtask gen-clippy` to write the per-crate files. Delete `scripts/check_deps.sh`. Rewrite README's gate section to show `cargo xtask gate task`.
- [ ] **Step 4: Run** `cargo test -p xtask`. Expected: PASS.
- [ ] **Step 5: Verify the tools.**
  - `cargo xtask arch-check` → `arch-check: OK (<n> packages)`.
  - `cargo xtask gen-clippy --check` → `gen-clippy: up to date (6 files)`.
  - `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`.
  - **Primitive ban check:** temporarily add `pub fn _probe(x: f32) -> f32 { x }` to `crates/fl_view/src/lib.rs`, then run `cargo clippy -p fl_view -- -D warnings`. Expected: a `disallowed_types` error naming `f32`. Remove the line. If clippy cannot ban a primitive this way, report it (a spec §12.2 gap) and keep `float_arithmetic` as the float guard.
- [ ] **Step 6: Run** `cargo xtask gate task --track contract`. Expected: last line `gate task: PASS in <n>s`, with n ≤ 300.
- [ ] **Step 7: Commit** `build(xtask): gate levels, arch-check, generated clippy bans, cargo-deny, nextest profile` with trailer `Refs: ADR-0009`.

