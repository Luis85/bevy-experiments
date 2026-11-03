# Foundation hardening plan (2026-11-04)

Spec: `horde-defense/docs/REVIEW_FOUNDATION.md` (the review). Every finding there is in scope unless a ruling below defers it.

## Global constraints

- Workspace: `horde-defense/` (independent Cargo workspace, Rust 1.99.1, Bevy =0.20.0, bevy_egui =0.43.1). Read `horde-defense/AGENTS.md` and `../AGENTS.md`.
- Run cargo **inside `horde-defense/`** with `--locked`. Set `CARGO_TARGET_DIR=C:\Projects\bevy-experiments\.claude\worktrees\horde-defense-review-c582e7\horde-defense\target` (shared warm cache; cargo's lock serializes builders). Never `cargo update`; adding a new dependency is allowed only where a task says so, and then `Cargo.lock` must be updated with `cargo add`/a normal build without `--locked` once, then verified with `--locked`.
- Resolve Bevy/bevy_egui APIs from installed sources in `C:\Users\LuisMendez\.cargo\registry\src\*\bevy_*-0.20.0` / `bevy_egui-0.43.1`, never from memory of older versions.
- Simulation invariants: `horde_core` stays engine-free (no Bevy/egui). Fixed 60 Hz whole ticks, stable IDs, serializable RNG, no renderer-driven outcomes. **Gameplay outcomes for a given seed must not change** unless a task explicitly says so. Never weaken tests, delete assertions or silently rebaseline numbers; a test may move or change its call path but must keep asserting the same behavior.
- Existing saves must keep loading (add migrations where formats change; add a checked-in legacy fixture test when you touch the save format).
- Gate before reporting DONE: `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace` — all green. Baseline: 432 passed, 1 ignored.
- Keep docs truthful: update `docs/ARCHITECTURE.md` (and other touched docs) when structure changes.
- Commit on your branch with a conventional message ending in `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not push.
- Match surrounding code style and comment density. No new god files: new modules should have one responsibility.

## Waves

Tasks in the same wave touch disjoint files and run in parallel in isolated worktrees; each wave merges before the next starts.

- Wave 1: Tasks 1, 2, 3
- Wave 2: Tasks 4, 5, 6
- Wave 3: Task 7, then Task 8
- Wave 4: Tasks 9, 10, 13
- Wave 5: Tasks 11, 12
- Wave 6: Task 14 (file splits), then final review

---

### Task 1: Core command API (`Game::apply`) and settlement errors

Files: new `crates/horde_core/src/command.rs`; `horde_core/src/lib.rs`; `horde_core/src/sim.rs` (Game impl only, not tick internals); `horde_core/src/sim/parallel.rs`; `horde_core/src/campaign.rs`/`strategy.rs` only where needed for rule helpers; `crates/horde_game/src/runner.rs`.

1. Move the `GameCommand` enum (and payload types it needs that are pure data) from `horde_game/src/runner.rs:34` into `horde_core::command::Command`. Commands that are purely frontend concerns (save intents, UI-only toggles, if any) stay in the runner as a separate `AppCommand`/wrapper enum that embeds `Command` — the runner's dedup/queue logic keeps working on the wrapper.
2. Implement `impl Game { pub fn apply(&mut self, cmd: &Command) -> Result<CommandOutcome, CommandError> }` containing the dispatch currently at `runner.rs:~1170-1300`. All domain rules move with it: recall threshold limit (0..=75), recruit-must-be-Ready, worker lookup/settled filtering (deduplicate the ~4 copies into one helper), layout building from worker structures.
3. `CommandError`: a typed enum (`#[derive(Debug, Clone, PartialEq, Eq)]`) with `Display` producing the exact user-facing strings used today (UI shows them; tests may match them). Wrap existing `Result<_, String>` from campaign/strategy calls in a `CommandError::Rejected(String)` variant where a finer variant isn't natural. `CommandOutcome` replaces whatever the runner returns today (e.g. `Applied`).
4. Runner calls `game.apply(..)` and maps outcomes to `RuntimeNotice` exactly as before. The runner must contain **no** domain rule checks after this task.
5. Settlement (`sim/parallel.rs:~150-175`): stop discarding `add_item_with_quality` and `record_country_stage_clear` results. Remove the temporary `world.active` swap by adding a campaign method that records a stage clear for an explicit country id. Surface failures: return them from settlement (e.g. collect into a `Vec<SettlementIssue>` stored on `Game` or returned from tick) and add a test proving a failed loot insert is reported rather than silently lost (construct the failing condition via the public/test API).
6. Add core unit tests for `Game::apply` covering at least: recall threshold rejection, recruit-not-ready rejection, one successful purchase, one worker command.
7. Do not change field visibility yet beyond what's needed (that's Task 7/8 territory) but add doc comments stating `apply` is the mutation path for frontends.

### Task 2: Logging, crash reports, lints and build profile

Files: `horde-defense/Cargo.toml`, both crate `Cargo.toml`s, new `crates/horde_game/src/logging.rs`, `crates/horde_game/src/main.rs` (only the plugin setup and println/eprintln lines), `diagnostics.rs`, `capture_sequence.rs`, `art.rs:~1263`, `visual_assets.rs:~430`, `player_model/startup.rs:~30`, `scripts/package_playtest.ps1`, docs.

1. Add `tracing = { version = "0.1", default-features = false, features = ["std", "attributes"] }` to `horde_core` (engine-free). Do not instrument `sim.rs` in this task (Task 4 does); you may instrument `sim/persistence.rs` (save/load start, success, failure with reason at `info`/`warn`).
2. In `horde_game`, configure `bevy::log::LogPlugin` once (via `DefaultPlugins.set(..)`): default filter `warn,horde_core=info,horde_defense=info,wgpu=error,naga=warn`, documented that `RUST_LOG` overrides it. Add a `custom_layer` that writes a plain-text log file to `<save dir>/logs/lastlight-<unix-ts>.log` (keep the newest 10 files, delete older). Use `tracing-appender` only if needed; a hand-written `MakeWriter` over a `File` is fine. If you add `tracing-subscriber`/`tracing-appender`, use versions already in `Cargo.lock` (Bevy pulls tracing-subscriber) to avoid churn.
3. Panic hook (`std::panic::set_hook`, chaining the default hook) writing `<save dir>/logs/crash-<unix-ts>.txt` with package version, OS, panic message/location and a backtrace (`std::backtrace::Backtrace::force_capture()`), and logging it at `error`.
4. Replace every non-test `println!`/`eprintln!` in `horde_game` with `info!`/`warn!`/`error!`, **except** stdout output that scripts parse (`--smoke` report, any machine-read evidence lines — check `scripts/*.ps1`/`*.py` for what they read); keep those with a `#[allow(clippy::print_stdout)]` and a one-line reason.
5. `[profile.dev] debug = "line-tables-only"` (keep opt-levels). Add `[profile.playtest] inherits = "release"`, `debug = "line-tables-only"`, and make `package_playtest.ps1` default to it if it currently defaults to debug (keep a parameter to choose).
6. `[workspace.lints]`: rust `unsafe_code = "forbid"`; clippy `dbg_macro = "deny"`, `print_stdout = "warn"`, `print_stderr = "warn"`, `todo = "warn"`. Both crates `[lints] workspace = true`. Gate must stay `-D warnings` clean (fix or justify-allow each hit; tests may `allow` print lints at module level).
7. Ruling: `unwrap_used` is NOT enabled workspace-wide (≈130 invariant `expect`s); instead convert the specific risky ones: `capture_sequence.rs:~74/122` (`let Some`), `diagnostics.rs:~270/288` (`NonZeroU8::new(1).expect` → `AppExit::error()`).
8. Make `bounded_count` in `diagnostics.rs` `warn!` when it replaces an out-of-range value.
9. Document logging (filter, RUST_LOG, log/crash file locations) in `README.md` and `docs/PLAYTEST_GUIDE.md`.

### Task 3: Remove dead weight in the frontend

Files: `crates/horde_game/src/character_rig.rs`, `character_rig/**`, `visual_assets.rs` (cfg(test) items and the `mod character_rig` decl), `player_model_preview.rs`, `ui/character.rs` import only, `ui/review.rs` imports only, docs mentioning the retired rig.

1. Delete the retired `character_rig` module (≈2,900 lines) and its tests (`canonical_tests`, `coverage_tests`). Before deleting, identify any assertion in those tests that guards *current production* behavior (e.g. parity of the player_model with shipped art); port such cases into `player_model/tests.rs`. List ported and dropped tests in your report with one-line reasons.
2. Remove `#[cfg(test)]`-only helpers in `visual_assets.rs` that existed for the rig (`defender_muzzle`, `defender_throw_origin`, `RigBody`, `RigGun`, …) unless still used by surviving tests.
3. The legacy `defender-rig-v1.png` atlas: if it is only used for weapon icons and a readiness check, keep loading it but rename/describe it as the weapon-icon source in code (do not re-cut art). Leave a short comment.
4. Split `player_model_preview.rs`: production `defender_preview` stays (rename file/module to `player_preview.rs` if clearer); review-only `rig_review`/`rig_comparison` move into `ui/review/` (create `ui/review/` as a module directory with `mod.rs` = current `ui/review.rs` content, plus `player_model.rs` for the moved code). Do not otherwise restructure review.rs in this task.
5. Replace the 11 `#[path = ...]` attributes with standard module layout (`foo.rs` + `foo/` or `foo/mod.rs`), no behavior change.

### Task 4: Typed simulation event stream, final-tick fix, sim tracing

Files: `horde_core/src/sim.rs` (event types, tick, finish), new `horde_core/src/sim/events.rs`, `horde_game/src/runner.rs` (event consumption ~456-503), `ui/damage.rs`, `inspector.rs`, `animation.rs`/`art.rs` only where they read `attempt.events`.

1. Introduce `pub enum SimEvent` with typed variants covering every current `EventKind` plus `CooldownReady` (melee ready), `AttemptFinished { outcome, .. }`, and the data consumers need (attacker/target ids, injury/value, roll text if the inspector needs it — inspector must not grep log prose for "3d6" anymore). Each emitted event gets a monotonic `seq: u64` (per Game, saved as a counter so it never repeats after load).
2. `Game::tick` additionally pushes newly emitted events into a drainable outbox: `pub fn drain_events(&mut self) -> std::vec::Drain<'_, SimEventRecord>` (record = seq + tick + battle id (root or worker id) + event). Events emitted on the tick an attempt finishes MUST be delivered (fixes the final-tick bug). Worker battle events are delivered with their battle id.
3. The outbox is not serialized. The existing `Attempt.events` rolling window stays for now **only** if animation/damage presentation still needs history for scene replay; if it remains, replace the O(n) `remove(0)` with a `VecDeque`. If it is saved today, keep loading old saves (serde default) — removing it from the save format is allowed only with a migration and legacy test.
4. Runner: consume `drain_events()` for sounds (replacing the `event.tick == a.tick` polling and the `melee_cooldown` diffing). Damage numbers dedupe by `seq` instead of tuple keys. Add a runner test proving a sound for an event on the finishing tick is played.
5. Tracing: `info_span!("battle", attempt, mode, seed)` around attempt lifetime events (start/finish logged at info with outcome), `debug!` for NPC decisions and rule resolutions with structured fields, `trace!` per tick. No measurable cost when no subscriber (macros only).
6. Gameplay outcomes must be byte-identical: the existing save/resume replay test and all balance tests must pass unchanged.

### Task 5: Fallible catalogs, ContentError, single env-var source

Files: `horde_core/src/{content,world,enemy_catalog,skill_catalog,ordnance,progression,quality,balance}.rs` (loaders only), new `horde_core/src/catalog.rs` (or `content_db.rs`), `horde_game/src/main.rs` (`validate_startup_catalogs` only), `horde_game/src/environment.rs` (`catalog_sources`).

1. Each catalog gets `pub fn try_load() -> Result<Self, ContentError>` (bundled default or env override). Add `pub fn init_catalogs() -> Result<CatalogReport, ContentError>` that loads + validates all catalogs and installs them into the existing `OnceLock`s; the existing accessors (`content()`, `world()`, …) keep their signatures and lazily fall back to the bundled default (which must never panic except on a corrupted build: `expect("bundled … is valid")`).
2. `ContentError` typed enum (`Io { path, source }`, `Parse { catalog, message }`, `Invalid { catalog, message }`, `TooLarge {..}`), `Display` + `std::error::Error`. No `panic!` remains on the override path (24 today).
3. One constant table `pub const CATALOG_OVERRIDES: &[(&str /*name*/, &str /*env var*/)]` in core used by both the loaders and `environment::catalog_sources()` (remove the duplicate list).
4. `main.rs` calls `init_catalogs()` first; a bad override now prints a clean error and exits with `AppExit::error()`. Add tests: invalid override JSON → `Err(ContentError::Parse ..)` (test the pure parse function with a string, not the env var, to stay process-safe).
5. Ruling (recorded): full dependency injection of a `ContentDb` into `Game::new` is out of scope (content() is called from hundreds of sites); but add `ContentDb`-style pure constructors (`Content::from_json_str`, etc.) so tests can build and validate alternative catalogs in-process.

### Task 6: Pure `Frontend` construction and test isolation

Files: `horde_game/src/ui.rs` (`Frontend::load` and a new `Frontend::new`), `settings.rs`, `ui_tests.rs` (`IsolatedFrontend`), test modules in `ui/menu.rs`, `ui/progression.rs`, `ui/review.rs` (or `ui/review/mod.rs` after Task 3), new `ui/test_support.rs`.

1. Split `Frontend::load()` into: `StartupConfig::from_env()` (all env reads used by load, incl. review/reduced overrides), `LoadedProfile::load(paths) -> Result<..>` (all filesystem reads), and `Frontend::new(profile, config, paths) -> Frontend` which touches no files and no env vars. `load()` composes them; behavior identical.
2. `ui/test_support.rs` (cfg(test)) with `test_frontend()` that builds via `Frontend::new` with a temp-dir `ProfilePaths` and default config. Replace `IsolatedFrontend::new` and every test that calls `Frontend::load()` so **no test reads `LOCALAPPDATA`, the real save, or `LASTLIGHT_*` env vars**. Add a test that asserts `test_frontend().save_path` is under the temp dir.
3. Do not split the Frontend fields yet (Task 8).

### Task 7: Stable content IDs in saves, versioned migrations, content fingerprint

Files: `horde_core/src/{campaign,content,sim,skill_catalog,enemy_catalog}.rs` (ID/array changes), `horde_core/src/sim/persistence.rs`, new `horde_core/src/sim/migrations.rs`, `horde_core/tests/` (new legacy fixture test + fixture JSON), plus every `horde_game` call site that indexes by `definition: usize` or the enum arrays.

1. Items, levels and structures get stable string IDs in their catalogs (add `id` fields to content.json entries; IDs = current names slugified; validate uniqueness). `Item.definition` becomes a `ContentId`/`ItemDefId(String)` (or interned newtype) resolved through a lookup; keep a fast index map built at catalog load.
2. Replace enum-indexed fixed arrays (`kills_by_kind [u32;5]`, `npc_decisions [u32;8]`, `blueprints [bool;3]`, `structure_upgrades [u32;3]`, `body [u32;5]`, `investments [u32;8]`, `SkillModifiers` arrays, `EnemyCatalog.defaults [String;5]`) with `BTreeMap<Key, V>` keyed by a stable kind id (serialize as maps). Relax `Content::validate`'s exact-length requirements to "every kind present".
3. Persistence: bump envelope `version` to 2. Add an ordered migration chain `fn migrate(value: serde_json::Value, from: u32) -> Result<Value, SaveError>` operating on raw JSON before typed deserialization: v1→v2 converts positional item definitions and arrays to IDs/maps using the v1 ordering frozen in the migration (copy the v1 order as a constant — do not read current catalogs for v1 meaning). Store `ruleset_version` and a content fingerprint (stable hash of catalog ids) in the envelope; on load, a mismatch logs a `warn!` (do not reject).
4. `SaveError` typed enum replaces `String` errors in persistence.
5. Check in a real v1 save fixture (generate one from the current code BEFORE changing formats — e.g. a test-only helper writing a mid-campaign save with items, kills, blueprints) under `horde_core/tests/fixtures/save_v1.json`, and a test that it loads, migrates and preserves item identities/counters.
6. Existing tests keep passing; the byte-equal save/resume replay test must still pass for v2.

### Task 8: Split `Frontend`, plugins, system sets, Bevy States, LaunchConfig, dev/review features

Files: `horde_game/src/main.rs`, `ui.rs`, `ui/*.rs`, `runner.rs` (system fns + `RuntimeHost`), `diagnostics.rs`, `capture_sequence.rs`, `cursor.rs`, `atmosphere_audio.rs` (registration only), `audio.rs`, `horde_game/Cargo.toml`, new plugin modules, docs.

1. **LaunchConfig**: one typed resource parsed once at startup from args + `LASTLIGHT_*` env vars (hand-written parser or `clap` with `env` — if clap, add it with derive feature). Contains review/perf/capture/smoke/catalog/size options, validates incompatible combinations (replacing `main.rs:63-73`), and prints `--help`. No `std::env::var` reads remain outside the config parser (grep to prove; `environment.rs` helpers become config-internal).
2. **Cargo features**: `review` (model workshop, review fixtures/screens, capture sequence, static capture, perf report) default **on** so existing scripts keep working; release/playtest packaging builds with `--no-default-features --features …` excluding it — update `package_playtest.ps1` accordingly. `dev` feature = `bevy/bevy_dev_tools`, `bevy/file_watcher`, `bevy/track_location`, `bevy/sysinfo_plugin`. Both features must compile and pass clippy (`cargo clippy --no-default-features` also clean).
3. **Plugins + sets**: `SimRuntimePlugin` (`RuntimeSet::{Apply, Advance, Persist}`), `SfxPlugin` (sound cues become a Bevy `Message` written by the runner, read by an SFX system — removes `Runtime.sounds` Vec and the generic `play_sounds::<H>`), `UiPlugin` (egui, cursor, draw, camera spawn as a named system), `ReviewToolsPlugin` (cfg(feature="review"), added only when `LaunchConfig.review` is set, otherwise not scheduled), `AtmosphereAudioPlugin` unchanged. Remove the blanket `.chain()`; only real dependencies are ordered. `show_window` runs via a run condition and stops after showing.
4. **Resource split**: replace the ~60-field `Frontend` with `GameSession { game }`, `Runtime`, `ProfilePaths`, `UserSettings`, `Navigation` (screen/tab/return stack), and per-screen UI state resources (`ArmoryUi`, `BattleUi`, `HistoryUi`, `SettingsUi`, …). Screen fns take a `#[derive(SystemParam)] UiCtx` or explicit params. `draw` must not mutate `GameSession` (only `Runtime` queues and UI state); textures cache lives in its own resource so `GameSession` change detection works; `publish_scene_audio` runs only when inputs changed. Replace `use super::*` in `ui/*` with explicit imports. Remove `RuntimeHost<H>` generics if no longer needed, keeping headless runner tests (port them to the new resources).
5. **States**: `Screen` becomes a Bevy `States`; camp tabs a `SubStates`; add `ComputedStates` `InCombat` if useful. `RuntimeNotice` becomes a `Message`; one navigation system turns notices into `NextState` and `OnEnter` handlers own the reset logic duplicated today across Deploy/ChooseStart/EnterCountry. Add `Screen::ALL` and derive `feedback.rs`'s screen list from it; per-screen chrome/audio/return-label via exhaustive `match` methods on `Screen`.
6. All 65 egui harness tests and runner headless tests keep passing (port, don't delete). Update `docs/ARCHITECTURE.md` (scheduling diagram, ownership table) and the README's dev-mode docs (`--help`).

### Task 9: Move UI-side game rules into core view helpers

Files: `horde_core/src/{campaign,progression,sim}.rs` (new query helpers), `horde_game/src/ui/{progression,results,armory}.rs`, `ui.rs` (`focus_next_upgrade`), `defender_presentation.rs`.

1. `Campaign::node_status(id) -> NodeStatus` enum (Owned/Available/Locked{reason}/Unaffordable…) replacing the UI re-implementation at `ui/progression.rs:~32-34` and string states `"OWNED"/"LOCKED"`; UI maps enum→label. The drift-guard test becomes a core test of `node_status` vs `can_purchase_node`.
2. One core `recommendation_fallback(&Game, context) -> Option<Recommendation>` replacing the three variants (`ui.rs:~380`, `ui/results.rs:~149`, `ui/armory.rs:~832`) — preserve each call site's current output (add tests pinning current behavior first).
3. `next_gear_level_rank` and `shop_offer(index) -> OfferPreview` in core replacing `ui/armory.rs:~325-330` and `~570` computations.
4. `Attempt::brace_active_at(tick)` replacing `defender_presentation.rs:~16-27` re-derivation.
5. Add per-screen `view_model(&Game) -> XxxView` pure functions where a screen still reads >10 deep fields (armory, battle HUD, briefing, operations), with unit tests. Views render from the view model.

### Task 10: Presentation seam — BattleViewModel, data-driven enemy visuals, interpolation

Files: `horde_game/src/art.rs` (battlefield), new `horde_game/src/battle_view.rs`, `visual_assets.rs` (atlas rects/WALK_SPECS), new `assets/art/enemy_visuals.json` (or RON), `animation.rs` as needed.

1. Extract `BattleViewModel` (pure snapshot built from `&Game` + presentation caches: actors with id/kind/position/pose/health, structures, ordnance, fx) consumed by the egui painter. `art::battlefield` becomes a renderer of the view model; split it into submodules (terrain, enemies, structures, ordnance, fx). Unit-test view-model construction headlessly.
2. Move enemy kind → atlas rect/anchor/walk spec mapping from hard-coded Rust (`visual_assets.rs:~507-560`, `art.rs:~1392-1450`, WALK_SPECS) into a validated data file loaded at startup, keyed by the enemy visual id from the enemy catalog. Adding an enemy visual variant must need only data.
3. Interpolate actor positions between fixed ticks using `Time<Fixed>::overstep_fraction()` (verify API in bevy_time 0.20 source) in the view model (presentation only; store previous-tick positions in a presentation cache keyed by actor id). Respect reduced-motion/capture determinism: capture sequence mode (fixed timesteps) must produce identical frames to today (interpolation fraction 0 or 1 there).

### Task 11: Asset pipeline and effects audio cleanup

Files: `visual_assets.rs` (loading/egui upload), `player_model/{mod,startup,backend_egui}.rs` (embedding), `audio.rs`, `horde_game/Cargo.toml`.

1. Remove double embedding: `menu-survivors-v1.png` (`backend_egui.rs:~123`) and other `include_bytes!` PNGs that duplicate `assets/` files — load through the AssetServer path; keep `include_*` only for catalogs that must exist before the app (document which and why).
2. Replace `OnceLock` caches for derived textures/atlases (`FLASH_MASKS`, `RIG_ATLAS`, `egui_textures: [OnceLock;7]`) with a resource rebuilt on `AssetEvent<Image>::{LoadedWithDependencies, Modified}` (`MessageReader<AssetEvent<Image>>`), so with the `dev` feature's `file_watcher` art hot-reloads. Readiness becomes a resource flag set by that system (replacing ad-hoc `all_loaded` polling).
3. Unify the two embedded player-model catalogues (`bundled_catalogue` vs `bundled_equipment_catalogue`) into one source.
4. Remove unused Bevy features (`bevy_picking`) from `Cargo.toml` if nothing uses them after Task 10.

### Task 12: Extensibility — status effects and behavior data

Files: `horde_core/src/sim.rs` (enemy/attempt status fields), new `horde_core/src/status.rs`, `skill_catalog.rs`, `enemy_catalog.rs`, `content.rs`, `art.rs`/`animation.rs` kind matches.

1. Generic `StatusEffect { id: StatusId, remaining_ticks: u32, stacks: u8, magnitude: i32 }` stored as `Vec<StatusEffect>` (sorted by id) on actors with apply/tick/expire helpers. Migrate `hit_stop_left` and `brace_left` (and fire-zone burn if it's per-actor) onto it with **identical** timing and outcomes (prove with existing tests + a new equivalence test). Save migration via serde default from the old fields.
2. Split `SkillEffect` into generic stat modifiers vs `Unlock(UnlockId)` (data-keyed) so grenade/molotov-style unlocks are data, not enum variants.
3. Enemy behavior: collect per-kind numeric differences currently expressed as `match EnemyKind` arms in `sim.rs` into fields of the resolved enemy profile (from `enemy_catalog.json`), leaving `match` only where behavior is genuinely different code. Document in `docs/ENEMY_COMPOSITION.md` the exact steps to add a new enemy (data-only variant vs new behavior).
4. Mark core public enums that frontends match on but may grow as appropriate; ruling: do not add `#[non_exhaustive]` inside the same workspace (exhaustive matches are a deliberate checklist) — document that in ARCHITECTURE.md.

### Task 13: Debug tooling, replay and determinism tests

Files: new `horde_game/src/debug_tools.rs` (cfg(feature="dev")), `runner.rs` (command recorder hook), `horde_core/tests/` (golden digest, property-style tests), `horde_core/tests/support/mod.rs` (dedupe helpers).

1. `DebugPlugin` (feature `dev`): Bevy `FpsOverlayPlugin` (verify in bevy_dev_tools 0.20), F3 egui panel (tick, enemies/cap, corpses, attempt id, seed, persistence queue state, frame phase timers), F5 single fixed tick while paused (through the runner, not by mutating Game), F6 dump `Game` JSON to `<save dir>/dumps/`, F7 toggle command recording.
2. Command recorder: when enabled (F7 or `--record <path>` in LaunchConfig), write JSONL `{tick, command|input}` of everything the runner applies; `--replay <path>` (dev feature) drives a headless run from a seed + recording and prints a final state digest. Core needs a stable `Game::state_digest() -> u64` (hash of the serialized save payload via a stable hasher, e.g. FNV-1a implemented locally).
3. Golden determinism test: for 3 fixed seeds, run N ticks through the headless smoke path and assert `state_digest()` equals pinned values (generate pins from current code and commit them; failure message explains how to re-bless intentionally).
4. Property-style tests without new deps: seeded loops (100+ seeds) asserting invariants (rewards exactly once, no negative coins, save→load→save byte-equal, settlement idempotent).
5. Deduplicate `fn game(`/`fn enemy(`/`fn advance(` helpers in `horde_core/tests` into `tests/support/mod.rs`; make `acquisition.rs`/`progression_loop.rs` use it.
6. Ruling (recorded): RNG sub-streams per purpose and fixed-point coordinates are deferred — both change every seeded outcome (forbidden silent rebaseline) and cross-machine lockstep is not a goal; document the rationale in `docs/SIMULATION.md`.

### Task 14: Split god files along seams

Files: `horde_core/src/sim.rs` → `sim/{director,movement,structures,npc,combat,manual,telemetry}.rs`; `horde_core/src/campaign.rs` → `campaign/{armory,progression,ledger,world}.rs`; `horde_game/src/runner.rs` → `runner/{command,runtime,apply,profile}.rs` + `runner/tests.rs`; `horde_game/src/ui_tests.rs` → `ui_tests/<screen>.rs`; `ui/review/mod.rs` → `review/{workshop,matrix,fixtures}.rs`; move core test fixtures (`smoke*`, `peak_render_fixture`, `maximum_progression_render_fixture`) behind a `test-support` cargo feature used by `horde_game` dev/review builds and core tests.

1. Pure moves + visibility adjustments; no behavior change; no file > ~1,200 lines afterwards except data tables.
2. Keep public API paths stable via re-exports where the frontend uses them.
3. Update ARCHITECTURE.md module table.
