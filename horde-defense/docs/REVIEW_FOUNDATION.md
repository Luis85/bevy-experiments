# Foundation review: Bevy practice, seams, logging and debugging

Reviewed on 2026-11-04 at monorepo commit `a1b65ba`. Four parallel reviewers covered the Bevy app architecture, the `horde_core` seams, the UI and test architecture, and logging, debugging and tooling. Their headline claims were then spot-checked by hand. No code was changed. Paths are relative to `horde-defense/crates/`.

## Verdict

The **simulation core** is a strong base:
- engine-free, with fixed 60 Hz whole ticks
- serialized RNG and stable IDs
- atomic, validated saves
- a typed `GameCommand` → `RuntimeNotice` protocol that egui re-layout cannot double-apply

The **extension seams** are weaker:
1. **Rules live in the frontend.** Core has no command API, so mutation and rules have leaked into `horde_game`.
2. **Saves depend on ordering.** The save format follows enum and catalog order.
3. **Bevy is only a host.** The whole game runs in one god-resource (`ui::Frontend`), drawn by egui, with no plugins, states or ECS presentation.
4. **Logging, error handling and debug tooling are missing.** There is no logging and no typed error handling. Diagnostics are spread across about 33 env vars.

None of this is urgent for playing the game. All of it gets more expensive with every new enemy kind, screen or VFX added before the seams are fixed.

## Gate status

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | 0 warnings |
| `cargo test --locked --workspace` | 432 passed, 0 failed, 1 ignored (offline audio export) |

## P0: do these before the next iteration

**1. Put the workspace under version control.**
- `git ls-files horde-defense` returns nothing; the directory is untracked (`?? horde-defense/`).
- The `ci.yml` matrix entry and the `scripts/dev.ps1` entry for it are uncommitted working-copy edits.
- Result: there is no history, and CI has never run on this workspace.
- Fix: commit the directory together with the CI and script changes.

**2. Move the command API into core** (`horde_game/src/runner.rs:34`, `1170-1300`).
- **The problem:** `GameCommand` and its dispatch live in the frontend and reach into sub-aggregates directly, e.g. `game.strategy.recruit(&mut game.campaign, …)` and `game.battles.iter_mut().find(..).game.place(..)`. Real rules live there too: the recall limit `> 75` (`runner.rs:1225`) and the recruit-must-be-Ready check.
- **Why it happens:** `Game` exposes `pub campaign/attempt/strategy/battles` (`horde_core/src/sim.rs:617-634`). Core has about 840 `pub` fields, and tests mutate the RNG, coins and ticks directly.
- **Fix:**
  - `Game::apply(Command) -> Result<Outcome, CommandError>` becomes the only mutation path.
  - Fields become `pub(crate)`, with read-only accessors.
  - Test mutators go behind a `test-support` feature.
- This unblocks almost everything below.

**3. Add a typed sim event stream** (`horde_core/src/sim.rs:233-245`, `523`, `1096`).
- **What exists:**
  - Events are an untyped rolling window (`from/to/value`, capped at 512, O(n) `remove(0)`). They are saved in the game file.
  - Consumers poll with `event.tick == a.tick` (`runner.rs:464`).
  - They diff `melee_cooldown` to detect "ready".
  - They grep log text for `"3d6"` (`horde_game/src/inspector.rs:207`).
- **Confirmed bug:** events from the final tick are never played. `finish` consumes the `Attempt`, and the runner then takes the `game.attempt.is_none()` branch (`runner.rs:470-503`).
- **Fix:**
  - `enum SimEvent { Hit{..}, Killed{..}, CooldownReady, AttemptFinished(Report), … }` with a monotonic `seq`.
  - `tick` drains events into `&mut Vec<SimEvent>`.
  - Events are not saved.

**4. Use stable IDs in saves, not positions** (`horde_core/src/campaign.rs:327`).
- **Positional references:** `Item.definition: usize` indexes `content().items[..]` by position, in about 55 places.
- **Arrays sized by enums:**
  - `kills_by_kind: [u32; 5]` (`sim.rs:279`)
  - `npc_decisions: [u32; 8]`
  - `blueprints: [bool; 3]`
  - `structure_upgrades: [u32; 3]`
  - `investments: [u32; 8]`
  - `EnemyCatalog.defaults: [String; 5]`
- **Effects:**
  - Inserting a catalog item in the middle silently turns saved gear into different gear.
  - Adding a `StructureKind` makes every old save fail to load.
  - `RULESET_VERSION` is never written to the save, and there is no content fingerprint.
- **Fix:**
  - Stable string IDs, which enemy archetypes already have.
  - `BTreeMap<KindId, u32>` instead of the arrays.
  - A content manifest or hash in the save envelope.
- Do this **before** adding any new kinds.

**5. Add logging** (none exists today).
- Today: `info!`/`warn!`/`tracing` are used 0 times. There are 27 `println!`/`eprintln!`. `LogPlugin` runs on defaults, with no file output and no panic hook.
- **Fix:**
  - Make `tracing` (engine-free) a core dependency.
  - Spans per battle and tick, with fields `attempt`, `tick`, `seed`, `enemy`.
  - Configure `LogPlugin { filter, custom_layer }` once, writing a rolling file under the save dir.
  - A panic hook that writes `logs/crash-<ts>.txt` with build info and the last log lines.
  - Change `profile.dev` to `debug = "line-tables-only"`. Playtest builds use the debug profile, so crashes currently have no symbols.

## P1: Bevy structure

**6. Split `ui::Frontend`** (`horde_game/src/ui.rs:46-114`).
- **The problem:**
  - It has about 60 fields: the authoritative game, the runtime, navigation, armory filters, aim state, egui rects, paths and modals.
  - It is used by 90 function signatures in 19 files and by 10 systems, so no two of those systems can run in parallel.
  - `draw` writes `f.textures` every frame, which defeats change detection.
  - Every `ui/*` file does `use super::*`, which gives every screen access to every field.
- **Fix:** split it into `GameSession`, `Runtime`, `ProfilePaths`, `UserSettings` and per-screen `ArmoryUi`/`BattleUi` resources, plus a `#[derive(SystemParam)] UiCtx`. Once that is done, `RuntimeHost<H>` is no longer needed. `docs/ARCHITECTURE.md` already plans this split.

**7. Organise the app into plugins and system sets** (`horde_game/src/main.rs:80-141`).
- **The problem:** everything is wired in `main()`. One `.chain()` serialises production systems (`persist`, `play_sounds`) together with review tooling (`capture`, `capture_performance`, `capture_frames`), even though nothing requires that order.
- **Fix:** follow `AtmosphereAudioPlugin`, which already shows the pattern:
  - `SimRuntimePlugin` with `RuntimeSet::{Apply, Advance, Persist}`
  - `SfxPlugin`, which drains a `Message` of sound cues instead of `Runtime.sounds: Vec`
  - `UiPlugin`
  - `ReviewToolsPlugin`, feature-gated

**8. Make screens Bevy `States`** (`horde_game/src/ui.rs:23-50`, `503-781`).
- **The problem:** `Screen` and `Tab` are plain enums, assigned in 62 places. There is a hand-built return stack, and a 280-line `runtime_notice` match mixes command results with navigation resets.
- Adding a screen means editing about 8 places, including a hard-coded list in `ui/feedback.rs:160-168`.
- **Fix:**
  - `States` for `Screen`, `SubStates` for `CampTab`, `ComputedStates` for `InCombat`.
  - `RuntimeNotice` becomes a `Message`, and a navigation system writes `NextState`.
  - `OnEnter(Screen::Battle)` takes over the reset logic now duplicated across the Deploy, ChooseStart and EnterCountry arms.
  - Use `run_if(in_state(..))` for systems that only apply to one screen.
- `bevy_state` is already compiled in (via `default_app`).

**9. Add a `LaunchConfig` and a `dev` cargo feature.**
- **The problem:**
  - About 33 `LASTLIGHT_*` env vars, some read every frame (`main.rs:209-261`, `ui.rs:813`, `ui/battle.rs:228`, `ui/damage.rs:207`, `ui/loot.rs:162`), some through raw `std::env::var`.
  - `ui/review.rs` (2,163 lines), `capture_sequence.rs` and the perf report ship in release builds and are always scheduled.
- **Fix:**
  - Parse the variables once into a typed resource. `clap` with `env = "LASTLIGHT_…"` keeps the existing scripts working, and `--help` documents them.
  - Gate the review and capture tooling behind `review` / `dev` features.
  - Replace the hand-written incompatibility checks (`main.rs:63-73`) with `conflicts_with`.

**10. Load catalogs and assets fallibly and inject them.**
- **The problem:**
  - Every catalog is a `OnceLock` plus `include_str!` that **panics** on a bad override (24 `panic!`s), so the `Err` path in `validate_startup_catalogs` never runs.
  - One test process gets exactly one content set.
  - Art uses three parallel loading mechanisms: AssetServer, `include_bytes!` (one PNG is embedded twice) and `std::fs`. Derived textures are cached in `OnceLock`, so hot reload is impossible.
- **Fix:**
  - A `ContentDb::from_sources() -> Result<Arc<ContentDb>, ContentError>` passed to `Game::new`.
  - Typed `Asset` loaders for the JSON definitions.
  - Rebuild derived data from `AssetEvent::Modified`.
  - Add `dev = ["bevy/file_watcher", "bevy/bevy_dev_tools"]`.

## P1: extensibility

**11. Adding new kinds means shotgun surgery.**
- A new enemy *behaviour* touches about 9 places: `EnemyKind`/`ALL`, `VisualSprite`, `defaults[5]`, `kills_by_kind`, a length-checked JSON table, 7 match sites in `sim.rs`, 15 in `art.rs`, 6 in `animation.rs`, and a save migration.
- A new weapon *behaviour* means a `WeaponClass` variant, with 61 match references.
- Skills use a closed `SkillEffect` enum that mixes generic effects with one-offs like `GrenadeUnlock`.
- Status effects are ad-hoc fields (`hit_stop_left`, `brace_left`, fire zones).
- **Fix:**
  - Data-keyed IDs plus a small set of behaviour traits registered by ID.
  - A generic `Vec<StatusEffect { id, remaining, stacks, params }>` on each actor.
- Stat-only variants are already JSON-only, which is good.

**12. Game rules are duplicated in views.**
- `ui/progression.rs:32-34` re-implements `can_purchase_node`, and uses strings for node state.
- The fallback recommendation logic is written three different ways: `ui.rs:380`, `ui/results.rs:149`, `ui/armory.rs:832`.
- The armory computes rank thresholds and `scale_item` offers itself (`ui/armory.rs:325`, `570`).
- **Fix:** move these into core (`node_status() -> NodeStatus`, `recommendation`, `shop_offer`), and add per-screen `view_model(&Game)` functions.

**13. Make the presentation seam ready for ECS.**
- **What exists:**
  - The battle is one ~650-line egui painter (`art::battlefield`, `horde_game/src/art.rs:545-1191`), with hard-coded atlas rects.
  - There are no entities, no camera, and no tick interpolation (`overstep` is unused). Motion therefore steps at 60 Hz on high-refresh displays.
  - `bevy_picking` is compiled in but unused.
- **Fix:**
  - First, extract a `BattleViewModel` snapshot that the painter consumes.
  - Later, a `BattleViewPlugin` can sync entities by stable ID, without rewriting the sim.

## P2: cleanup and quality

- **Testability:**
  - `Frontend::load()` reads `LOCALAPPDATA`, the real `campaign.json` and env vars, including in tests.
  - Several test modules keep `save_path` pointing at the developer's real save: `ui/progression.rs:314`, `ui/review.rs:1997`, `ui/menu.rs:480`.
  - Fix: add a pure `Frontend::new(profile, config)` and a shared `test_frontend()`.
- **Errors:**
  - About 220 `Result<_, String>` signatures.
  - Settlement discards errors with `let _ = add_item_with_quality(..)` and `let _ = record_country_stage_clear(..)` (`horde_core/src/sim/parallel.rs:158`, `172`), so loot can be lost silently.
  - Fix: typed `CommandError`/`ContentError`/`SaveError`, and surface settlement failures.
- **Persistence versioning:**
  - The envelope version is permanently `1`.
  - Migrations are scattered per-field markers.
  - The save is a raw dump of internal structs.
  - Fix: an ordered `migrations` chain over `serde_json::Value`, checked-in legacy save fixtures, and eventually a `SaveDto`.
- **Determinism hardening:**
  - One RNG stream is shared by spawning, NPC and combat rolls, so any new roll reshuffles everything that follows. Derive a sub-stream per purpose.
  - Authoritative positions are `f32` with libm trig. That matters only if cross-machine replay becomes a goal.
- **Debug tooling:** the sim is deterministic and command-driven, so recording `(tick, GameCommand, Input)` as JSONL gives exact bug repros. A `dev`-gated `DebugPlugin` could add:
  - FPS overlay
  - F3 sim panel
  - single-step while paused
  - state dump
  - cheats
- **Dead weight:**
  - About 2,900 lines of retired `character_rig` compiled only for tests.
  - `player_model_preview.rs` mixes review code with production code.
  - 11 `#[path]` attributes.
  - Sim test fixtures in the public core API (`horde_core/src/sim.rs:3192-3551`).
- **God files:** `sim.rs` 3.5k, `campaign.rs` 2.8k, `runner.rs` 2.2k, `ui/review.rs` 2.2k, `art.rs` 2.1k, `visual_assets.rs` 2.2k, `ui_tests.rs` 2.5k. Split them along the seams above, not mechanically.
- **Lints:** no `[workspace.lints]`. Add `unsafe_code = "forbid"`, `dbg_macro`, `print_stdout`/`print_stderr`, and `unwrap_used` as a warning. Some lines (e.g. `ui/armory.rs:573`, 456 characters) suggest rustfmt isn't applied consistently.
- **Missing tests:**
  - Golden state digests after N ticks for fixed seeds.
  - Property tests.
  - Headless `App` tests for scheduling.

## Preserve

- An engine-free `horde_core` that depends only on serde. A fixed-step `tick(Input)`. Speed-up runs whole ticks.
- SplitMix RNG with saved state, a `BTreeSet` reward ledger, no `HashMap`/`Instant`/threads in core, and per-actor snapshots of resolved enemy profiles.
- Same-frame command deduplication across egui passes, and `RuntimeNotice` acknowledgements. Views never tick or pay out.
- Atomic save generations: temp file, fsync, `.bak`, rename, checksum, and an off-thread writer.
- The `RuntimeHost` headless tests. The egui harness tests that drive real widgets without a GPU.
- `AtmosphereAudioPlugin`, a model plugin with pure, tested mixing. The player-model resolve pipeline.
- Thorough validation of content and saves, and `deny_unknown_fields`.

## Suggested order

1. Commit the workspace and CI (P0.1).
2. Core `Game::apply`, typed events, then stable IDs (P0.2–P0.4).
3. Logging, crash hook and line tables (P0.5) in parallel.
4. Split `Frontend`, plugins and system sets, then `States` (P1.6–P1.8).
5. `LaunchConfig`, the `dev` feature and `ContentDb` (P1.9–P1.10).
6. Behaviour registry, status effects and `BattleViewModel` (P1.11–P1.13), driven by the next content you actually want to add.

## Status after hardening (2026-11-04)

Evidence is the foundation-hardening branch (merge `51b31ac` plus the final-review fixes). Statuses are honest: "Partially" and "Deferred" items are listed again under follow-ups.

| Item | Status | Evidence |
|---|---|---|
| P0.1 Version control and CI | Done | `horde-defense` is tracked; `ci.yml` matrix and `scripts/dev.ps1 verify` cover it, now including the `--no-default-features` and `--features dev` clippy/test runs. |
| P0.2 Core command API | Done | `Game::apply` and typed `CommandError` live in `horde_core/src/command.rs`; widgets only see `&Game`. `Game` fields are crate-private behind accessors; frontends mutate through `apply`, runtime plumbing, `Game::staging()` for synthetic scenes, and tests through the dev-only `test-support` feature. Other core aggregates (`Campaign`, `Attempt`, …) still have `pub` fields. |
| P0.3 Typed sim event stream | Done | `sim/events.rs` `SimEvent` stream; final-tick events delivered; settlement failures surface as `SettlementIssue`. |
| P0.4 Stable IDs in saves | Partially | Items, unlocks, statuses and enum-keyed `KindMap`s use IDs (saves v2-v5 with migrations and fixtures). Level-positional progress remains. |
| P0.5 Logging and crash reports | Partially | `LogPlugin` file logs, crash file, `playtest` profile. The crash report does not include the last log lines. |
| P1.6 Split `Frontend` | Done | `GameSession`, `Runtime`, `ProfilePaths`, settings and per-screen resources; `UiCtx` borrows them. |
| P1.7 Plugins and system sets | Done | `SimRuntimePlugin`, `UiPlugin`, `SfxPlugin`, `ReviewToolsPlugin` with named sets. |
| P1.8 Screens as `States` | Done | `Screen`/`Tab` Bevy states, `Navigation`, one notice system. |
| P1.9 `LaunchConfig` and `dev` feature | Done | `launch.rs` parsed once in `main`; `dev` feature with debug tools, recorder and headless replay. |
| P1.10 Fallible, injectable catalogues | Partially | `ContentError` and `init_catalogs` are fallible. Process-wide accessors remain; `ContentDb` injection deferred. |
| P1.11 New kinds without shotgun surgery | Partially | Data-driven enemy composition and visuals, skills with `Unlock(UnlockId)`, generic `StatusEffect`s. The procedural fallback art still matches on `EnemyKind`. |
| P1.12 Rules out of views | Partially | UI rules moved to core view helpers (`node_status`, recommendations, shop offers); `BattleViewModel` seam. The battle screen still reads the attempt directly in places. |
| P1.13 ECS-ready presentation seam | Partially | `BattleViewModel` with tick interpolation and an event-driven art pipeline; no ECS presentation yet (not required). |
| P2 Testability | Done | Pure `FrontendParts::new`, hermetic `test_frontend()`; no test touches the real save. |
| P2 Errors | Done | `CommandError`, `ContentError`, `SaveError`, `SettlementIssue`; settlement no longer discards results. |
| P2 Persistence versioning | Done | Envelope v5, ordered migration chain, checked-in `save_v1..v5` fixtures with continuation traces. |
| P2 Determinism hardening | Deferred | Golden digests and properties exist; RNG sub-streams and fixed-point positions deferred. |
| P2 Debug tooling | Done | `dev`-gated debug plugin, command recorder, headless replay, `Game::state_digest`. |
| P2 Dead weight | Done | Retired `character_rig` and the `#[path]` modules removed; render fixtures kept un-gated deliberately (shipped practice scenes use them). Orphaned rig assets remain. |
| P2 God files | Done | `sim`, `campaign`, `runner`, `visual_assets`, ui tests and review fixtures split along phase seams. |
| P2 Lints | Done | `[workspace.lints]` (`unsafe_code = "forbid"`, `dbg_macro`, print and `todo` lints); fmt enforced in CI. |
| P2 Missing tests | Done | Determinism digests, seeded property tests, headless runner/App tests, egui harness tests. |

### Tracked follow-ups

- Core field visibility below `Game`: `Campaign`, `Attempt`, `StrategyState` and `ParallelBattle`'s bookkeeping keep `pub` fields (reachable only through `&` accessors, staging or `test-support`).
- Level-positional progress (`first_clears`, `unlocked`, `Command::BuyItem(usize)`) must move to stable IDs before levels are reordered.
- The crash report lacks the last log lines.
- The battle screen still reads the attempt directly in places (the view model is partial).
- Procedural fallback art matches on `EnemyKind`; a new sprite needs a `VisualSprite` variant.
- RNG sub-streams and fixed-point positions are deferred.
- `ContentDb` injection is deferred (catalogues are still process-wide).
- Content drift warns on every save after a content change, not once.
- Orphaned rig assets remain in `assets/`.
