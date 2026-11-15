# qa-captures: merged design spec (production pass)

Base: **Angle A** (2 of 3 judges). Grafts from Angle B are applied where most judges preferred B on that decision. Every listed violation is fixed, and the fixes are summarised at the end. Paths are relative to `horde-defense/`. This remit owns no saved field and moves no digest.

## 0. Facts this spec relies on (verified at HEAD)
- `ui.rs:207` declares `mod review;` privately, and the module is **not** feature-gated. Practice uses `matrix::prepare` in every build. Only `review_tools` and `capture_sequence` are `cfg(feature="review")` (main.rs:6-19). The default features are `["review"]` (horde_game/Cargo.toml:26).
- `matrix.rs:358`: `_ => f.nav.tab = Tab::World`. Workshop scenes (`model-parity-`, `model-parts-`, `model-matrix-`, `model-repairs-`) **reach this arm by design**, and workshop.rs then renders them.
- `matrix.rs:26` sets "Practice scenario - progress is isolated..." unconditionally.
- `matrix.rs:132-153` runs the progressed clears **before** `legacy_features = true` at fixtures.rs:124. That causes the panic at fixtures.rs:138, and with it the 8 failed wave-0 captures.
- `SimEvent::Shot { shooter: Actor, target_id, lane, from, to }` (horde_core/src/sim/events.rs:56) has **no weapon class field**.
- `Command::Equip`, `Command::BuyItem` and `Command::Place` exist (horde_core/src/command.rs).
- No shared "primary button" helper exists in the UI. The amber colour is painted per screen, so the one-amber-primary check stays in headless egui tests (`ui/tests/mod.rs:337 text_count`, `:374 text_color`).
- `capture_review.ps1:6-7` hard-codes 41 scenes. Missing from it: max-lanes, journey-victory, journey-liberated, journey-upgrades, skills-search and others.
- `generate_audio.py` writes `ambience` (shipped) plus 8 clips that are now archived. `generate_atmosphere_audio.py` reads 9 ORIGINAL clips from `assets/audio`, writes LOOPS (shipped) and CUES (archived), and globs `*.wav` at :151.
- `clearing_rect` (ui/battle.rs:475) clamps height to at least 86 pt (:481). `battle_arena(canvas, construction)` (:490) insets the arena for construction.

## 1. Chosen design (pillars)

### 1.1 Windows PowerShell 5.1 harness: `scripts/lib/Native.ps1`
The file is ASCII, LF, without BOM, and starts with `#requires -Version 5.1`. The 5 in-scope scripts dot-source it.

| Function | Contract |
|---|---|
| `ConvertTo-NativeArgument([string[]]$Arguments)` | Returns ONE string using MSVC quoting rules. An argument containing whitespace or `"` is wrapped in quotes, inner `"` becomes `\"`, and trailing backslashes are doubled before the closing quote. An empty argument becomes `""`. |
| `Invoke-Native -FilePath -Arguments [string[]] -LogStem -TimeoutSeconds -WorkingDirectory` | Runs `Start-Process -NoNewWindow -PassThru -RedirectStandardOutput <stem>.out.txt -RedirectStandardError <stem>.err.txt`. `-ArgumentList` is passed only when the argument string is non-empty, because an empty value throws in 5.1. Then `$null=$p.Handle`. If `$p.WaitForExit(ms)` returns false it runs **`taskkill.exe /PID <id> /T /F`** (graft B: this kills cargo's rustc children) and throws `Timed out after N s`. Otherwise it calls `$p.WaitForExit()`. Returns `[pscustomobject]@{ExitCode;Id;Seconds;Out;Err}`. Amended by R24 (plan §2.4, user 2026-11-07): it starts the child with `[Diagnostics.Process]::Start` and a `ProcessStartInfo` instead, UseShellExecute off and stdout and stderr redirected to the same two logs, because in 5.1 `Start-Process -PassThru` can report a `$null` ExitCode once the child has exited (381e563). `>` stays forbidden, and the timeout still runs `taskkill.exe /PID <id> /T /F`. |
| `Invoke-Cargo` | Wraps Invoke-Native. On a non-zero exit it throws and prints the last 40 lines of the err log. |
| `Resolve-Tool <name>` | `(Get-Command <name> -CommandType Application)[0].Source` |
| `Resolve-HordeTarget` | A rooted `$env:CARGO_TARGET_DIR` is used as-is. A relative one is joined to `$workspace`. Otherwise `$workspace\target`. |
| `Get-HordeExecutable -Profile dev\|debug\|release\|playtest` | `dev` maps to `debug`. Returns `<target>\<profile>\horde-defense.exe` and throws if it is missing. |
| `Get-ReviewSceneList -Executable` | Runs `--list-scenes` through Invoke-Native and parses the TSV into objects (see 1.3). |
| `Reset-LastlightEnv` / `Restore-LastlightEnv` | Saves and clears every process env var whose name starts with `LASTLIGHT_`, then restores them in `finally`. `WGPU_BACKEND` is not cleared; its value is recorded in captures.json. |
| `Write-Utf8NoBom -Path -Text` | `[IO.File]::WriteAllText($p,$t,(New-Object Text.UTF8Encoding $false))` |

Timeouts: cargo 1800 s, capture 300 s, smoke 900 s, `--version` 30 s, git and rustc 30 s.

**Scope:** capture_review, capture_motion, measure_performance (5.1 plumbing only; perf-architecture later adds -Profile, -VsyncOff, stress and the budget), package_playtest and verify_playtest. The new curate_capture_pairs.ps1 and scripts/tests/run_script_tests.ps1 also use the helper. capture_player_models, trace_npc and verify_player_models stay PowerShell-7-only on an explicit lint allowlist, and their port is logged as a follow-up.

### 1.2 Evidence layout and isolation
- Output layout:
  - `<dest>/<mode>/<scene>[-small].png`
  - `<dest>/<mode>/<scene>[-small].out.txt|.err.txt`
  - `<dest>/<mode>/<scene>[-small].json` (sidecar, see 1.5)
  - per-capture `$env:LASTLIGHT_SAVE_DIR=<dest>/saves/<mode>-<scene>[-small]`
  - `<dest>/scenes.tsv`, the binary's `--list-scenes` output
  - `<dest>/captures.json`
- `<mode>` is `normal` or `reduced`. `reduced` sets `LASTLIGHT_REVIEW_REDUCED=1`.
- capture_review refuses an existing non-empty `<dest>`; there is no -Force. Each wave writes `target/production-after/wave<N>`, and the final run writes `target/production-after/final`.
- After each process, remove `[IO.Path]::GetTempPath()+'lastlight-review-'+$r.Id` if it exists (profile.rs:72-73 never deletes it).
- `captures.json` (version 1, UTF-8 without BOM):
  - top level: `executable`, `executable_sha256`, `stamp`, `wgpu_backend`, `windows_scale`, `started_utc`
  - per entry: `scene`, `mode`, `size` (`large`/`small`), `scale`, `file`, `exit_code`, `seconds`, `bytes`, `sha256`, `out_log`, `err_log`, `sidecar`, `error`, **`identical_to_normal`** (graft B: true when a reduced PNG's sha256 equals its normal pair's)
- Parameters: `-Scenes` (default: every `Still` from `--list-scenes`), `-SmallScenes` (default `$Scenes`), `-Modes` (ValidateSet normal/reduced, default `normal`; orchestrator runs always pass `normal,reduced`), `-Executable`, `-NoBuild`, `-ContinueOnError` (collects failures and exits 1 at the end), `-TimeoutSeconds 300`, `-OutputDirectory`.
- **Rust guard** (A): `LaunchConfig.log_directory_override: Option<PathBuf>`, default None. parse sets it when review is requested, practice is off, a scenario is set, and `LASTLIGHT_SAVE_DIR` is absent. The override is `profile::profile_directory(base, scenario, false).join("logs")`, the per-PID temp review profile. `log_directory()` returns the override, otherwise `save_directory/logs` (launch.rs:441-443). An explicit SAVE_DIR still wins, and practice and ordinary play are unchanged.

### 1.3 Review scene registry: `crates/horde_game/src/review_scenes.rs`
The module is ui-free (A's placement) and compiled in every build. It is declared `mod review_scenes;` in main.rs.

```rust
pub enum SceneKind { Still, MotionOnly, NeedsInputSave, Workshop }
pub struct ReviewScene {
    pub name: &'static str, pub kind: SceneKind,
    pub motion: bool, pub perf: bool, pub practice: bool,
    pub primary: Option<&'static str>,      // exact amber primary label, copied from ux-flow's loop map
    pub before_proxy: Option<&'static str>, // wave-0 scene used as the "before" until this one exists
    pub owner: &'static str, pub since_wave: u8,
}
pub const REVIEW_SCENES: &[ReviewScene];
pub const WORKSHOP_PREFIXES: [&str; 4] = ["model-parity-","model-parts-","model-matrix-","model-repairs-"];
pub fn lookup(name: &str) -> Option<SceneKind>;   // a prefix plus a non-empty id gives Workshop
pub fn is_workshop(name: &str) -> bool;
pub fn stills() -> impl Iterator<Item = &'static ReviewScene>;
pub fn list_lines(review_build: bool) -> Vec<String>; // TSV: name kind motion perf practice primary before_proxy since_wave
```

- **`--list-scenes`** (graft B) is a new `LaunchAction::ListScenes`, added to HELP. main prints `list_lines(cfg!(feature="review"))` to stdout and exits 0. A non-review build lists only `practice=true` rows. The scripts read scene names only from the binary, and no `$Scenes = @(` arrays or scene ValidateSets remain.
- **Unknown names:** `LaunchConfig::validate` returns `Err("Unknown review scene \"x\"; run --list-scenes")` when `scenario` is Some and `lookup` is None. main.rs:50-55 already prints the error and exits non-zero before a window opens.
- **matrix.rs:358 stays reachable for Workshop names only** (A; this fixes B's `unreachable!` violation): `_ => { debug_assert!(review_scenes::is_workshop(&screen), "registry admitted {screen}"); f.nav.tab = Tab::World }`, with a comment saying the workshop renders these.
- **Expected Screen/Tab** for every Still lives in a test table `EXPECTED: &[(&str, Screen, Option<Tab>)]` in `ui/review/tests.rs`. This keeps ui types out of the registry. A completeness test asserts that every `stills()` entry has a row.
- Scene names are ids and never change.

### 1.4 Fixture fixes
- New `pub(super) fn enable_legacy_ledgers(game: &mut Game)` in fixtures.rs (graft B) sets `staging().campaign().legacy_features = true`. matrix.rs calls it at the top of the `progressed-battle | leveled-armory` block (before :132), and prepare_progression calls it in place of fixtures.rs:124. It is idempotent. Root cause: the 9 DEU clears level the player to 10 with 0 points while the flag is false (award_xp_to, progression.rs:683-700), so the third allocate_body fails (progression.rs:646-648). With the flag hoisted, the player has 11 points before allocation and 8 after it (B's python replay).
- `matrix.rs:26`: set the Practice status only `if launch.practice`. peak, progressed-peak and max-lanes keep their own text (:63).

### 1.5 Capture sidecar, scoped (graft B, trimmed in response to judge 3's scope concern)
- New `#[cfg(feature="review")] #[derive(Resource, Default, Serialize)] pub(crate) struct ReviewFrameFacts { scene, screen: String, tab: Option<String>, overlays: Vec<&'static str>, arena_rect: Option<[f32;4]>, effects_rect: Option<[f32;4]>, lanes: Option<u8>, width_yards: Option<f32>, hud_rows: Option<u8>, reduced_effects: bool, ui_scale: f32, windows_scale: f32 }` in `ui/review/mod.rs`, re-exported `pub(crate)` the same way as `schedule_gesture_review`.
- Battle fields are filled in exactly one place, ui/battle.rs where the arena is computed (about :229-238). `hud_rows` comes from the hud layout. Screen, tab, overlays, reduced and scale are filled by review_tools from frontend state.
- `review_tools::automated_capture` (review_tools.rs:33) writes `<capture>.json` next to the PNG, before it spawns the Screenshot. All IO stays in review_tools, so no widget does IO.
- Primary actions are **not** in the sidecar. The single amber primary is proven by headless tests, using the registry `primary` and `text_color`.
- capture_review asserts from the sidecars:
  - `hud_rows <= 2` for every battle scene at small size
  - `arena_rect(construction) == arena_rect(battle)` at the same size

  The second check is expected to fail until ux-flow lands the construction overlay. It is tracked as an open CF row owned by ux-flow and is not suppressed.

### 1.6 Battle camera QA (user requirement: uniform zoom-to-fit; implemented by ux-flow and vfx-lighting)
- A shared `battle::fit_field(maximum: Rect, lanes: u8, width: f32) -> Rect` keeps today's lane:yard proportion. `s = min(30/width, 11/lanes)`, size `(W*width/30*s, H*lanes/11*s)`, centred. The leftover space is forest margin.
- `battle_arena` loses its `construction` parameter (ui/battle.rs:490-497), and construction becomes an overlay.
- **The observer window (ui/operations.rs:277) uses the same fit_field** (graft B; all 3 judges). Its window is 180-360 pt tall (operations.rs:270-273), so under clearing_rect a 3-lane fight sits on the 86 pt floor (battle.rs:481).
- Expected rects at canvases 1392x692 / 729.8x310.5:
  - 3x12: 1392x471.8 / 729.8x211.7
  - 7x20: full width x 0.9545H
  - 11x30: equals maximum
  - 17x30: 900.7x692 / 472.2x310.5
  - lane_height(3) > 2*lane_height(11)
- Tests:
  - `arena_fits_full_lanes_and_width_on_the_limiting_axis` replaces ui/battle.rs:1089-1104.
  - `construction_toggle_keeps_arena_rect` is new.
  - `observer_uses_fit_field` is new.
  - short_arena_top_and_bottom_rows_share_forest_fx_space (:1180-1199) is rewritten: the `!rect.contains(feet-78)` line is removed with a written reason (lane height 157.3 pt > 156 at 3x12 large), and the effects-clip containment stays.
  - The :1106-1146 round-trip test stays green.
- Captures: intro (3x12), battle-mid (7x20), max-lanes (17x30), construction and battle at both sizes, and operations-observer.

### 1.7 Reduced-effects coverage
Every registered Still, at both sizes, in both modes, on every wave's after-run and on the final run. At plan time that is 66 stills x 4 = 264 processes, about 23 min at the measured 5.2 s per process (the registry count is authoritative).

### 1.8 Scene list (new entries; each registered in the commit where its fixture works)
Shared rules:
- Every new fixture returns early before matrix.rs:163.
- Feature state is staged only through `f.act`/`Game::apply(&Command)`. `staging()` is allowed only inside `entitle_*` helpers (coins, first clears, inventory grants).
- The recipes live in a new `ui/review/fixtures/production.rs`.
- Search helper: `fixtures::first_stage_where(pred: impl Fn(&Country, usize, &StageTheme) -> bool) -> (country_id, stage)` walks `world().countries` in file order and stages `0..level_count`.

| Scene | Wave | Owner | Screen/Tab | Primary | Before proxy | Recipe / assertion |
|---|---|---|---|---|---|---|
| results-defeat | 1 | ux-flow | Results | (ux loop map) | results | DEU, `deploy_country_stage(0, Personal)`, unpause, tick `Input::default()` until the attempt is None (cap 36,000 ticks). Asserts `last_result().outcome==Defeat` with a non-empty cause and tip, and run_history.len()==1. |
| save-error | 1 (status), 3 (modal) | ux-flow | Camp (+ overlay `save-error-modal` in W3) | W3: Retry | world | `f.notices.push(RuntimeNotice::Saved{intent:SaveIntent::Manual, result:Err("Access is denied. (os error 5)".into())}); f.apply_notices();` (runner/command.rs:45-48, state.rs:382). Asserts no campaign.json under f.paths. |
| night-checkpoint | 1 | qa (vfx verifies) | Battle | - | arid | `first_stage_where(t.lighting=="night" && t.id=="checkpoint")` (world.json theme 4), then entitle the earlier clears and deploy paused. No core theme-rule change. |
| weapon-classes-{pistol,smg,shotgun,rifle,marksman,machine-gun} | 1 (W5 tightens) | qa / vfx | Battle | - | battle | `entitle_item` gives the first tier-1 Ranged item of that class (content items 21-26). `f.act(Command::Equip{..})`, prepare_battle, fire at mid-field until the first player `SimEvent::Shot` (cap 60 ticks), pause. **W1 test** asserts the equipped item's `weapon.class` and that a player Shot was emitted. **W5** adds an assertion against the decision-16 gear snapshot. |
| weapon-classes-turret | 1 | qa / vfx | Battle | - | construction | Entitle coins, `f.act(Command::Place{turret..})`, tick until the first turret-actor Shot (cap 600). |
| fx-stress | 1, in perf's stress commit | perf-architecture | Battle | - | max-lanes | Calls perf's single stress fixture, paused. `stress` (perf=true, motion=true, MotionOnly) shares the same function. |
| battle-mid | 1 | ux-flow | Battle | - | battle | First stage whose deployed geometry is 7 lanes x 20 yd (search through the core geometry query deploy uses). Asserts lanes==7 and width==20. If none exists after decision 12, use the nearest pair and assert the recorded geometry. |
| operations-observer | 1 | ux-flow | Camp/operations + observer window | - | operations | prepare_operations roster (fixtures.rs:377-409) plus a live expedition, with the observer open. |
| upgrades | 3 | ux-flow | Upgrades | Back to battle | journey-upgrades | journey-upgrades staging, then Screen::Upgrades. |
| camp-home | 3 | ux-flow | Camp (home) | Go to briefing | active-camp | Roster with 2 recruits, a guard, a live expedition, an outbreak and Mara met. |
| briefing-recruit | 3 | campaign-defenders | Tab::Briefing | (loop map) | briefing-npc | A recruit is ready, then `f.act(Command::SetNextDefender(recruit))`, then open_briefing. |
| equipment-autosell | 3 | loot-economy | Tab::Armory + Auto-sell panel | - | quality-armory | One tier-1 Ranged item per `Quality::ALL` (as matrix.rs:198-211), then `Command::SetAutoSell{enabled:true, sell_below:Uncommon}`. The panel reads "Would sell 2 now · +22" (`ui/armory/auto_sell.rs`; d0e4aff6): decision 6 sells strictly below Uncommon, so the Poor and the Common drop (`auto_sell_preview`; W2-46 pins 2, W2 review; this row read "1" before, then quoted the preview as "would have sold 2"). |
| results-liberation-recruit | 3 | campaign-defenders | Results | Choose next country | journey-liberated | SetAutoSell before the final win, the last German stage really deployed with one drop per quality and the boss at 1 HP (the kill, the settlement and its two sales are core's), then purchase `recruitment_network` (skills.json:4019) on Results after the win: the node needs one liberated country (`minimum_liberated` 1), so it cannot be bought before it (as built, W3-27 d0e4aff6; waves.md W3-27 "Closed by W3-03"; this row first bought it before the win). Shows "Recruit from Germany" and "Auto-sold 2 · +C". |
| takeover-battle | 3 | campaign-defenders / vfx | Battle | - | npc | SetNextDefender(recruit) and deploy. Asserts the drawn identity comes from the defender, not the Mode::Npc Female default (art/battlefield/mod.rs:104-108). |
| storm | 5 | vfx-lighting | Battle | - | battle | `first_stage_where(t.storm)`. The presentation tick is chosen so one pulse peaks inside the motion window. |

Storm tag (recommendation to vfx-lighting, A): a serde-default `storm: bool` on `StageThemeDefinition`, set true for the light_rain and drizzle themes. No theme is appended, because that reshuffles 242 countries via `% len` (world.rs:177-179). The pulse schedule is a pure function of (Country.variation, presentation tick) and never touches core RNG.

**Supplemental before set** (graft B). Right after the fixture-fix commit, and before any presentation change, capture progressed-battle, leveled-armory, journey-victory, journey-liberated, journey-upgrades, skills-search and max-lanes at both sizes in both modes (28 PNGs) into `target/production-before/supplement`. compare_captures merges `production-before` with `supplement`.

### 1.9 Motion manifest
- W1: a pure `capture_sequence::sequence_manifest(scene, reduced_effects, (w,h), scale, frames) -> serde_json::Value` with `version: 2` and top-level `reduced_effects`, `width`, `height`, `scale`, `windows_scale` (replacing version 1 at capture_sequence.rs:139). capture_motion gains `-Viewport normal|small` (small is 1050x700@1.35).
- W5 (vfx supplies the values): per-frame `flash_alpha_max: f32`, `sky_flash: f32` and `muzzle_flash_count: u32`. With `-ReducedEffects`, capture_motion asserts every combat frame is 0. In normal mode a storm needs at least 1 frame with sky_flash > 0, and the per-frame sky_flash delta must stay below 0.2.
- **The "at most 1 pulse per 240 ticks" rule is a vfx unit test on the pure pulse-schedule function**, not a motion assertion: a 24-frame export covers only about 96 ticks (this fixes A's violation).

### 1.10 Release (wave 7)
- **Build stamp.** New `crates/horde_game/src/build_info.rs`:
  - `pub fn stamp_from(version:&str, commit:Option<&str>) -> String` returns `"v0.1.0 (dev build)"`, `"v0.1.0 (abc1234)"` or `"v0.1.0 (abc1234+dirty)"`. The commit is accepted only if it is at most 20 characters of ASCII lowercase hex, optionally followed by `+dirty`; anything else falls back to dev build.
  - `pub fn stamp() = stamp_from(env!("CARGO_PKG_VERSION"), option_env!("LASTLIGHT_BUILD_COMMIT"))`.
  - Where it is shown: the menu footer, right-aligned (`all.right_bottom()+vec2(-24,-20)`, Align2::RIGHT_BOTTOM, 10 pt, MUTED, menu.rs:209-215); a `build: <stamp>` line in crash_report (logging.rs:265-275); a startup `info!("Lastlight {stamp}")` (graft B); and `--version`, a new `LaunchAction::Version` that prints `Lastlight: Horde Defense <stamp>`.
  - Only package_playtest sets LASTLIGHT_BUILD_COMMIT, and it restores the variable in `finally`.
- **Dirty tree.** Packaging refuses unless `-AllowDirty` is passed. Dirty means `git status --porcelain=v1 --untracked-files=all -- crates assets docs/PLAYTEST_GUIDE.md Cargo.toml Cargo.lock scripts` is non-empty (run via Invoke-Native).
- **Manifest v2** `[ordered]`: `version=2, game, game_version, stamp, commit (40 hex), commit_short, dirty, branch, profile, features, platform, rustc, created_utc, curated_docs, files`. It is written with Write-Utf8NoBom. The zip is named `Lastlight-playtest-v<ver>-<sha7>-<utc>.zip`. Built with `Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem` and `[IO.Compression.ZipFile]::CreateFromDirectory`, giving forward-slash entries on .NET 4.8. `Get-PackageRelativePath` replaces GetRelativePath (package_playtest.ps1:74).
- **Curated docs** (A): `$CuratedDocs=@('PLAYTEST_GUIDE.md')`, with its 7 relative links turned into plain-text references. READ ME.txt gains `Build: Lastlight <stamp>, <profile> profile, created <utc>`. There is no BUILD.md. Limits: docs/ holds exactly 1 file and at most 65,536 B. `Test-DocLinkClosure` fails packaging when a shipped .md has a relative link that does not resolve inside the package.
- **Source-only art excluded from the zip** (graft B): provenance entries tagged `"shipped":"source-only"` (the 5 walk strips and pixel/keyart-v1, 9,240,086 B) are skipped by package_playtest.
- **verify_playtest** checks, in order:
  1. Add-Type at the top.
  2. Zip safety: reject entries that contain `\`.
  3. ExtractToDirectory, then the integrity loop.
  4. `manifest.version -eq 2`, commit matches `^[0-9a-f]{40}$`, game_version equals Cargo.toml, features is present, docs equal curated_docs, docs total at most 65,536 B, and no `archive/` or source-only path is present.
  5. `Lastlight.exe --version` (30 s) contains `manifest.stamp`.
  6. `--smoke` (900 s) exits 0 and its first stdout line matches `^PASS: `.
  7. `-NativeCapture` with `review` absent from manifest.features throws before launch, in under 5 s. Otherwise the capture runs with a 300 s timeout.

### 1.11 Archive move (wave 1)
- `git mv assets/art/{actors,biome-panoramas,bosses,checkpoint-keyart,ground,props}-v1.png assets/art/PROVENANCE.json` to `archive/art/`. Each entry gains `sha256`, `bytes`, `archived_from`, `archived: "2026-11-xx"` and a `generator_note`.
- `git mv` the 12 WAVs (armor, boss, damage, incoming, melee, npc_shot, ready, reload, shot, structure, trap, turret_shot) to `archive/audio/`, with a new `archive/audio/PROVENANCE.json` holding their 12 `files` records and 8 preserved_originals. `assets/audio/PROVENANCE.json` keeps the 6 bed records (ambience plus ambient_wind, urban, nature, rain, cold) and preserved_originals {ambience}.
- `archive/README.md`: "Not shipped; retained by production-pass decision 10; restore with git mv".
- `assets/art/animation/PROVENANCE.json` (5 walk strips) and `assets/art/pixel/PROVENANCE.json` (keyart-v1) gain `"shipped":"source-only","reason":...`.
- Generators are retargeted in the same commit:
  - `generate_audio.py:8`: ambience goes to `assets/audio`, the other 8 to `archive/audio`, and the docstring is updated.
  - `generate_atmosphere_audio.py`: `ARCHIVE=ROOT/'archive'/'audio'`. ORIGINAL reads ambience from OUTPUT and the 8 from ARCHIVE. CUES write to ARCHIVE. The :151 glob is limited to `ambience.wav` plus `ambient_*.wav`, which also fixes recording 72 survival WAVs under the wrong generator. Both PROVENANCE files are written.
  - New `--check` (graft B) renders into a temp dir, compares sha256 with the committed files and both PROVENANCE files, and writes nothing in the workspace.
- `assets/CREDITS.md:31-34,40`: "archived in archive/, not shipped" and "six WAV beds plus 72 survival cues".
- Tests in `#[cfg(test)] crates/horde_game/src/asset_audit_tests.rs`:
  - a std-only FIPS 180-4 SHA-256 (about 60 lines), pinned by the "" and "abc" vectors, with hashes computed once via `OnceLock`
  - the runtime set: `visual_assets::loading::PATHS` (made pub(crate)), enemy_visuals.json sheets, player_models manifest and attachments PNGs, and new `audio::runtime_wav_paths()` (Cue::ALL x variant_path 0..3, plus Bed::ALL = 78)

### 1.12 Docs, check_views and the findings ledger
- check_views: fix the 6 broken front-matter `source` lines only; no Rust port (A).
  - battle-ammo-reticle: `crates/horde_game/src/ui/reticle.rs::paint; crates/horde_game/src/ui/battle.rs::set_cursor_icon`
  - new-game-confirmation: `crates/horde_game/src/runner/apply.rs::begin_new_journey`
  - player-model-{matrix,parity,parts,repairs}: `crates/horde_game/src/ui/review/workshop.rs::model_X, paint_model_X`
- 5.1 doc commands: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/<x>.ps1` at README.md:28,171 and VALIDATION.md:175,235. Annotate README.md:165 and VALIDATION.md:147 (root dev.ps1, off limits) and PLAYER_MODEL_VALIDATION.md:67,80,83 with "requires PowerShell 7; not ported in the production pass". AGENTS.md:10 becomes "Each workspace owns target/ unless CARGO_TARGET_DIR is set; every script honours it."
- **Findings ledger** in `docs/plans/2026-11-04-production-pass.md`, between `<!-- capture-findings:begin -->` and `<!-- capture-findings:end -->`:
  `| ID | Wave | Scene | Size | Mode | Finding | Severity | Owner | Status | Fix commit | After evidence | Deferral |`
  - ID: `CF-<wave>-<nn>`
  - Size: `1440x900`, `1050x700@1.35` or `both`
  - Mode: `normal`, `reduced` or `both`
  - Severity: blocker, major, minor or polish
  - Status: open, fixed or deferred
  - Fix commit: 7-40 hex characters
  - After evidence: a committed `docs/screenshots/production/after-*.png`, or `target/production-after/<wave>/<mode>/<scene>[-small].png#<sha12>`
  - Deferral: `approved by user YYYY-MM-DD`, only from the user in chat
  - Proxy rows for not-yet-registered scenes, CF-0-01 (the fixtures.rs:138 panic) and the status-line change are logged the same way.
- **Committed pairs** go in `docs/screenshots/production/{before,after}-<scene>[-small][-reduced].png`. They are made by `scripts/curate_capture_pairs.ps1` (System.Drawing through Add-Type), which downsamples 2:1 when captures.json `windows_scale==2`. Total at most 40 MB. The full-run indexes `captures-before.json` and `captures-after.json` are committed alongside.
  - Mandatory set: the 11 brief scene families (weapon-classes: shotgun and machine-gun only), plus intro, battle-mid, max-lanes, construction and operations-observer, at small size in normal mode. Reduced pairs for fx-stress, storm and weapon-classes-machine-gun. One pair per CF row.
- `scripts/compare_captures.py <before> <after> [--html out.html]`, std-lib only. It pairs entries by (mode, scene, size) using `before_proxy` from `<after>/scenes.tsv`, merges `production-before/supplement`, and prints a changed/unchanged/missing/identical_to_normal table. `--html` writes a side-by-side contact sheet (graft B). It exits 1 on any failed after-capture.
- `scripts/check_capture_findings.py <plan> [--final]`, std-lib only. It checks the vocabulary, that fixed rows have a hex commit and existing evidence, and that deferred rows carry `approved by user <ISO date>`. `--final` also fails on any open row or when the pair folder exceeds 40 MB.
- **Loop map** (critic gap shared with ux-flow): ux-flow authors the screen / primary / Escape-target table in the plan. Registry `primary` values are copied from it, and the goal-10 review walks it.

### 1.13 Docs inventory (closes the critic gap owned by qa-captures)
| Doc (stale anchor) | Change | Owner | Wave |
|---|---|---|---|
| JOURNEYS.md:13 "every main round returns to upgrades" | New loop: Results (3 actions), then Upgrades (Back to battle / Back to camp), then Camp home; takeover; auto-sell | ux-flow + campaign-defenders | 3 |
| UI_REDESIGN.md:107-109 (Escape) | Escape stack and decision 18, one-amber rule, HUD at most 2 rows, camera fit | ux-flow | 3 |
| VIEW_HIERARCHY.md:5 | Add upgrades, camp-home, auto-sell panel, results loot block, save-error modal, defender picker, quit/retreat confirm, menu stamp | ux-flow | 3 |
| docs/components | 6 source fixes (qa, W1); new view docs in their owning wave | qa / owners | 1 / owning |
| PROGRESSION.md:23 "There is no automatic sale system" | Auto-sell rule, level-up coins, milestones, recruit level formula | loot-economy + progression-tree | 2/4 |
| ITEM_QUALITY.md | Relative quality, drop weighting, auto-sell threshold, colour-blind markers | loot-economy | 2/3 |
| SKILLS_AND_WEAPONS.md | Repricing, at most 8 new nodes, class perks; weapon_fx; gun audio v2 | progression-tree / vfx / audio | 2/4, 5, 6 |
| ENEMY_COMPOSITION.md | Archetypes, elites, boss phases, capstone | enemy-balance | 2/4 |
| PROGRESSION_SCALING.md | Two-axis threat, checkpoints, difficulty band, elite_rate | enemy-balance | 2/4 |
| BALANCE_AND_RUN_STATS.md | Auto-sold receipt, RunRecord fields, pacing profiles, smoke before/after | loot-economy + progression-tree | 4 |
| ACCEPTANCE.md AC01 "LANES=11" | 3..17 lanes with zoom-to-fit; new ACs naming test files | qa | 7 |
| AUDIO_SURVIVAL.md:55 | Buses, MusicPlugin director with Deferred track slots, -23 +/-2 LUFS at default sliders, true peak <= 0.95, listening pass pending | audio | 6 |
| VALIDATION.md:175,235 (pwsh); :178 "25-view matrix"; :232-240 package record | 5.1 commands, registry count, new Full-verify steps (run_script_tests.ps1, check_capture_findings.py), new package record | qa | 1 / 7 |
| PLAYTEST_GUIDE.md:31,124 "Escape pauses"; :38 construction; links :7,75,142,179 | Decision 18; sections Loop, Escape, Auto-sell, Takeover, Listening pass, Click-to-photon; links become plain text | qa (content from ux/audio) | 7 |
| README.md:28,171; PLAYER_MODEL_VALIDATION.md:67,80,83; AGENTS.md:10 | 5.1 commands, annotations, target rule | qa | 1 |
| assets/CREDITS.md:31-34,40 | Archive wording; new audio | qa / audio | 1 / 6 |
| EXPANSION.md | Record the camera requirement and decisions 8, 9 and 19 as later user requirements | qa | 7 |
| ARCHITECTURE.md, docs/PERF_BUDGET.json | Ownership, budgets | perf-architecture | 1 |

### 1.14 Click-to-photon checklist (PLAYTEST_GUIDE; logged as a pending human check)
1. Use a 60 Hz display. Note the GPU, driver, refresh rate, the HAGS setting and the menu stamp.
2. Battle practice.cmd: unpause, aim at mid-field, fire 20 single shots at least 1 s apart.
3. Film the mouse button and the muzzle area with a phone at 240 fps or more (4.17 ms per frame).
4. For each click, count frames from button bottom-out to the first changed muzzle or tracer pixel: ms = frames x 1000 / fps.
5. Record the median and max for: VSync on with Low latency off (default, desired_maximum_frame_latency=1, main.rs:119); Low latency on (Mailbox); VSync off if available. Also note qualitatively whether the reticle trails the system cursor during fast flicks.

Targets: median at most 50 ms in the default row and at most 34 ms with Low latency on. Any single click over 67 ms becomes a CF row owned by perf-architecture. The checklist includes a table template.

## 2. Decisions table
| Question | Chosen | Rejected (reason) |
|---|---|---|
| Native invocation | One dot-sourced lib/Native.ps1, Start-Process, `$p.Handle`, taskkill /T. Amended by R24 (plan §2.4): `[Diagnostics.Process]::Start` with a `ProcessStartInfo` and both streams redirected to the logs is the accepted form | `Start-Process -PassThru` (R24: in 5.1 it can report a `$null` ExitCode once the child has exited, 381e563); inline copies (they drift); `&` with EAP=Continue (violates the brief, `*>` writes UTF-16); Stop-Process only (leaves rustc children running) |
| Scripts ported | 5 in scope plus the new scripts; 3 allowlisted | port all 8 (no wave needs them; 2-argument GetFullPath risk) |
| Registry home | ui-free crate::review_scenes | ui/review/scenes.rs embedding Screen/Tab (`mod review` is private; couples launch to the UI); enum + FromStr (large refactor); JSON file (two parsers) |
| Scene list sync | `--list-scenes` from the binary | include_str! sync tests with hard-coded ps1 lists (copies still exist) |
| Unknown name | Launch error in validate; :358 kept for Workshop with debug_assert | `unreachable!` (panics every model-* scene); panic in prepare (exit 101 after window creation); keep the World fallback (lying evidence) |
| Panic fix | enable_legacy_ledgers hoisted, idempotent | top up points (fabricates ledgers); reorder prepare_progression (changes unlock order) |
| Log isolation | Script SAVE_DIR plus Rust log_directory_override | scripts only (one forgotten var prunes real logs); always temp (loses crash reports beside the evidence); B's profile_directory change (not needed) |
| Reduced stills | All stills, both sizes, every wave | effect subset (drifts); waves 0 and 10 only (waves 5-6 add effects) |
| Sidecar | Scoped ReviewFrameFacts (geometry, HUD, mode, scale) | no sidecar (capture facts unverifiable); per-screen amber collection (no shared helper; invasive) |
| night-checkpoint | Predicate search for theme 4 | force the checkpoint theme in core stage_theme_with (cross-remit rule change, reshuffles pairs); fixture lighting override (not real play) |
| Storm tag | serde-default `storm: bool` on rain/drizzle | append a theme (reshuffles via `% len`); hashed storm_chance (rate untested) |
| weapon-classes | 7 real-battle stills in W1, class assertion through the equipped item; snapshot assertion in W5 | W1 snapshot assertion (the Shot event has no class; the snapshot does not exist yet); review-only painter (not the real path); campaign.equip direct (bypasses Command) |
| fx-stress vs stress | One perf-owned fixture, two names | two fixtures (drift); stress only (brief names fx-stress) |
| Observer camera | Shared fit_field | keep clearing_rect (unreadable on the 86 pt floor, two camera models); observer clamps (no evidence a clamp is needed) |
| Extra scenes | Brief 11 plus battle-mid, operations-observer, takeover-battle | B's ~13 extras (unsettled ux features; inflate the matrix) |
| Missing before-evidence | Supplemental before set after the fixture fix | none (the 7 pairs have no before) |
| Build stamp | option_env! plus build_info, menu, crash, log, --version | build.rs git (needs git, reruns on every HEAD move); runtime manifest read (startup IO) |
| Save-level stamp | None | SaveEnvelope.written_by (state_digest hashes the whole payload; GOLDEN churn) |
| Dirty tree | Refuse unless -AllowDirty; path-scoped check | always allow (unreproducible builds); always refuse (blocks dry runs) |
| Curated docs | PLAYTEST_GUIDE only plus a READ ME build line | depth-1 closure (dead second-level links); tester pack (stale history); BUILD.md (redundant with the READ ME line and manifest) |
| Source-only art | Keep in assets/, tag it, exclude from the zip | archive now (needs a user yes); ship it (9.24 MB waste) |
| Archive provenance | Per-class archive/*/PROVENANCE.json | archived_to field (shipped files would describe unshipped ones); a single file (loses the per-class schema) |
| Generator check | `--check` renders into a temp dir | scratch-copy run (manual, easy to skip); retire the generators (loses reproducibility) |
| Asset hashing | In-test SHA-256 | sha2 dev-dependency (Cargo.lock change, network); python checker (outside cargo test) |
| check_views | Fix 6 sources; python gate | Rust port (duplicate checker drifts) |
| Motion proof | Manifest v2 per-frame zeros; pulse rate as a vfx unit test | rate from a 24-frame sequence (about 96 ticks, cannot prove it); luminance only (fragile) |
| Findings | Marked ledger plus check script, fix commit, 40 MB of downsampled pairs | FINDINGS.md (splits the plan); captures.json only (no owner or status); raw 2x PNGs (about 300 MB) |
| Practice status text | Only when practice | REVIEW tag (adds pixels to every capture); leave it (false in evidence) |

## 3. Steps by wave (every test fails first against the old behaviour)

### Wave 1
**1. Native helper and script tests.**
- Files: scripts/lib/Native.ps1, scripts/tests/run_script_tests.ps1, .gitignore (`/scripts/__pycache__/`).
- Failing first: run_script_tests.ps1 fails today because lib/Native.ps1 is missing. It covers:
  - `cmd /c "echo out & echo err 1>&2 & exit 3"` returns 3 and both logs, even under a caller's `*>`
  - ConvertTo-NativeArgument cases 'a b', 'x"y', 'c:\p\' and ''
  - a 1 s timeout kills a 5 s sleeper together with its child
  - Resolve-HordeTarget with CARGO_TARGET_DIR unset, relative and rooted
  - Reset/Restore-LastlightEnv round-trips LASTLIGHT_CONTENT
  - Write-Utf8NoBom writes no BOM
- Done when: it prints PASS under 5.1.26100 with its output redirected.

**2. Lint test and port of the 3 capture scripts.**
- Files: crates/horde_game/src/repo_checks.rs (new, `#[cfg(test)] mod` in main.rs); capture_review.ps1, capture_motion.ps1, measure_performance.ps1 (harness only).
- Failing first:
  1. `repo_checks::powershell_scripts_are_51_safe` checks every scripts/**/*.ps1 except the allowlist (capture_player_models, trace_npc, verify_player_models). Each must be ASCII without BOM, have `#requires -Version 5.1`, and dot-source lib/Native.ps1 if it runs a native process. On comment-stripped lines it forbids: `GetRelativePath`, `& cargo`, `& './target`, `*>`, `Compress-Archive`, `pwsh`, `$Scenes = @(`. Any `[IO.Compression.ZipFile]` must be preceded by `Add-Type -AssemblyName System.IO.Compression`. It fails today (capture_review.ps1:17,31; measure_performance.ps1:25; package_playtest.ps1:74,79).
  2. A redirected `capture_review -NoBuild -Scenes menu -Modes normal,reduced` throws NativeCommandError today (main.rs:39).
  3. With a bogus `$env:LASTLIGHT_CONTENT`, startup validation fails today (main.rs:62-64).
- Done when: the lint passes for these 3 scripts (package and verify are ported in step 11, and the lint marks them pending through an explicit `PENDING_W7` list that step 11 empties). Layout, captures.json, -ContinueOnError and temp-profile cleanup work, the logs listing compares equal before and after, and all 3 scripts honour CARGO_TARGET_DIR.

**3. Rust log guard.**
- Files: launch.rs.
- Failing first:
  - `launch::tests::review_without_save_dir_never_logs_into_the_player_profile` (LASTLIGHT_REVIEW_SCREEN=menu, LOCALAPPDATA=C:/x, no SAVE_DIR; log_directory() must not start with C:/x/LastlightHordeDefense). It fails today (launch.rs:441-443).
  - Companions: explicit SAVE_DIR wins; practice and ordinary play use save_directory/logs.
- Done when: green in default and `--no-default-features` builds.

**4. Registry, --list-scenes, validation, panic fix, practice status.**
- Files: review_scenes.rs (new), main.rs, launch.rs (validate, ListScenes, HELP), ui/review/matrix.rs (:26, :132, :358), ui/review/fixtures.rs (enable_legacy_ledgers), ui/review/tests.rs, capture_review.ps1, capture_motion.ps1 and measure_performance.ps1 (read the list from the binary).
- Failing first:
  - `launch::tests::unknown_review_scene_is_rejected` ('camp-hmoe' gives Err; accepted today).
  - `launch::tests::list_scenes_is_a_launch_action`.
  - `ui::review::tests::stills_{menus_and_camp,battles,results_and_history}_prepare_headless`: every Still prepares to its EXPECTED (Screen, Tab), and where the registry has a `primary`, one harness frame has `text_count(primary)==1` painted AMBER. It panics today for progressed-battle and leveled-armory.
  - `ui::review::tests::every_still_has_an_expectation_row`.
  - `ui::review::tests::progressed_fixtures_have_a_genuinely_leveled_survivor` (level >= 10 and points == 8 after 3 allocations; panics today).
  - `ui::review::tests::non_practice_review_has_no_practice_status`.
  - `ui::review::tests::workshop_names_still_reach_the_workshop` (model-parity-x keeps Tab::World with no panic; guards against the unreachable! regression).
- Done when: all green. `--list-scenes` lists the existing 46 stills plus the Workshop prefixes. The ledger has CF-0-01 fixed and the status change logged.

**5. Supplemental before-captures.**
- Run immediately after the step-4 commit, with no presentation change: 7 scenes x 2 sizes x 2 modes (28 PNGs) into `target/production-before/supplement`, with captures.json.

**6. Capture sidecar.**
- Files: ui/review/mod.rs (ReviewFrameFacts), ui/battle.rs (fill), review_tools.rs (write), capture_review.ps1 (asserts).
- Failing first:
  - `ui::tests::review_frame_facts_report_arena_rect_and_hud_rows` (after a battle frame, `facts.arena_rect == f.battle.arena_rect`; the resource does not exist today).
  - `review_tools::tests::sidecar_json_has_required_keys` (serialisation only).
- Done when: every still has a sidecar. The battle vs construction rect assertion is logged as an open CF row owned by ux-flow.

**7. New W1 scenes.**
- Files: ui/review/fixtures/production.rs (first_stage_where, entitle_*, prepare_results_defeat, prepare_save_error, prepare_weapon_class, prepare_battle_mid, prepare_operations_observer), review_scenes.rs, tests.rs.
- Failing first (all fall through to World or are rejected today):
  - `results_defeat_fixture_settles_a_real_defeat`
  - `save_error_fixture_surfaces_the_failure_without_io` (status contains "Save failed"; no campaign.json)
  - `night_checkpoint_fixture_is_a_night_stage`
  - `weapon_classes_fixtures_equip_and_fire_their_class` (7 names; equipped `weapon.class` matches; a player Shot, or a turret-actor Shot, was emitted)
  - `battle_mid_fixture_is_7x20`
  - `operations_observer_fixture_opens_the_observer`
  - `fx_stress_aliases_the_stress_fixture`, which lands in perf's stress commit
- Done when: green, with both sizes and both modes captured, and capture_motion run for weapon-classes-shotgun and weapon-classes-machine-gun.

**8. check_views and doc commands.**
- Files: 6 component docs (line 7), README.md, VALIDATION.md, PLAYER_MODEL_VALIDATION.md, AGENTS.md:10, repo_checks.rs.
- Failing first:
  - `python -I scripts/check_views.py` exits 1 with 11 errors today.
  - `repo_checks::docs_invoke_scripts_with_windows_powershell` (no `pwsh -NoProfile -File scripts/` in *.md outside docs/plans) fails on 7 lines.
- Done when: "PASS: 69 Views", exit 0, and the test is green.

**9. Archive move.**
- Files: as 1.11.
- Failing first:
  - `asset_audit::sha256_matches_fips_vectors`
  - `shipped_art_has_provenance_with_matching_sha256` (the 6 root entries lack sha256)
  - `shipped_art_is_referenced_or_declared_source_only` (fails on the 6 sheets, 5 strips and keyart-v1)
  - `every_shipped_wav_is_loaded_and_matches_provenance` (exactly 78; 12 orphans today)
  - `archived_files_keep_matching_provenance` (18 files, 16,282,021 B; there is no archive/ today)
  - `python -I scripts/generate_atmosphere_audio.py --check` raises FileNotFoundError until it is retargeted
- Done when: all green. assets/ drops from 45,903,392 B to 29,621,371 B. `--check` reports byte-identical output and writes nothing. CREDITS is updated. Smoke is byte-identical.

**10. Motion manifest v2 (top-level fields) and -Viewport.**
- Failing first: `capture_sequence::tests::manifest_records_reduced_effects_and_viewport` (version 1 today).
- Done when: `capture_motion -ReducedEffects -Viewport small` validates `manifest.reduced_effects` and the size.

### Wave 2
- Core-only staging helpers compile against the real Commands as loot-economy and campaign-defenders land them:
  - `equipment_autosell_rule_counts_only_below_uncommon_in_core` (2 of 6 qualities, Poor and Common; this line read "1 of 6" before the W2 review)
  - `briefing_recruit_staging_sets_next_defender_through_command`
- `repo_checks::production_fixtures_stage_features_through_commands` scans fixtures/production.rs for `staging()` outside `fn entitle_*`.
- Wave-2 close stills (W2-55, W2 review): `boss-phase` (W2-37's phase hold: a rank-5 boss wounded into phase 1, paused with 60 of 90 hold ticks left; `boss_phase_fixture_pauses_a_boss_inside_its_phase_hold`; W2-56 (CF-2-03) pauses it on the entry tick instead, and its motion export holds that tick through the warm-up and resumes on native frame `FIRST - 1`, so frames 0-22 draw the hold and frame 23 its end: `boss_phase_motion_export_plays_the_whole_hold`) and `skills-firearms` (W2-41's Firearms column at x = -1780: the skills recipe focused on `pistol_double_tap`; `skills_firearms_fixture_opens_on_pistol_double_tap_at_both_sizes`). W2-57 (CF-2-04) adds `spitter-windup` (W2-28's ranged windup: a rank-6 field played with the smoke input until the director's first spitter stations at its 4-yard reach, paused halfway through its windup; `spitter_windup_fixture_pauses_a_spitter_mid_windup_at_its_reach`).

### Wave 3
- Register upgrades, camp-home, briefing-recruit, equipment-autosell, results-liberation-recruit and takeover-battle; save-error gains the modal.
- Failing first:
  - `upgrades_fixture_has_one_amber_primary_back_to_battle`
  - `camp_home_fixture_lists_roster_and_go_to_briefing`
  - `briefing_recruit_fixture_selects_next_defender`
  - `equipment_autosell_preview_counts_only_below_uncommon`
  - `results_liberation_recruit_offers_recruit_from_germany` (with "Auto-sold")
  - `takeover_battle_draws_the_recruit_not_mara`
  - `save_error_fixture_opens_retry_modal`
  - The camera tests from 1.6 (ux-flow and vfx implement them; QA verifies): `arena_fits_full_lanes_and_width_on_the_limiting_axis`, `construction_toggle_keeps_arena_rect`, `observer_uses_fit_field`, the rewritten :1180-1199 test.
- Done when: green. The W3 matrix runs both sizes and both modes, compare_captures --html is generated, proxy rows close, and the construction-rect CF row is fixed.
- Closing phase (R36): no still showed the W3-14 boss banner. `boss-warning` (since wave 3, owner ux-flow, before proxy `boss`, motion) is appended with its W3 matrix row: a stage-0 defense after one victory, played 20 s with the smoke input, its clock entitled to one tick before the boss warning. The still plays that tick, pauses on the warning tick and holds the banner 1.0 s into its 2.0 s (`BattleUi` presentation state), so both modes draw it at full strength; the motion export pauses on the tick before and resumes on native frame `FIRST - 1` (`review::motion_release_frame`), so frame-0000 draws the warning tick. The sidecar gains `boss_banner_rect` (`BattleUi::boss_banner_rect`, battle screen only). Tests: `boss_warning_still_draws_the_banner_on_the_warning_tick`, `review_frame_facts_report_the_boss_banner`, `boss_warning_motion_export_starts_on_the_warning_tick`.
- Closing phase (R36): no still showed the W3-25 expedition toasts (`ui/expedition_notices.rs`) or the folded-count toast. `camp-reports` (since wave 3, owner ux-flow, before proxy `active-camp`, primary Go to briefing) is appended with its W3 matrix row: Camp home after two recruits were sent to the Czech Republic through `Command::DeploySurvivor`, fought 20 s and were recalled through `Command::RecallSurvivor`, so two settled outcomes wait unacknowledged, beside 3 outcomes folded by pruning (`entitle_folded_outcomes`, the strategy's `folded_outcomes`). The first frame toasts the two outcome lines and "3 more expedition outcomes · Operations", the whole visible stack. Test: `camp_reports_still_toasts_two_outcomes_and_the_folded_count`.
- Closing phase (R36): no still could show a W3-24 onboarding callout, because `matrix::prepare` marks every unlocked callout seen for a staged scene. `callout-camp` (primary Go to briefing) and `callout-upgrades` (primary Back to camp), since wave 3, owner ux-flow, before proxies `active-camp` and `journey-upgrades`, are appended with their W3 matrix rows: the camp-home state on Camp home, and the same state on Upgrades (`focus_next_upgrade`, `open_upgrades`). They are the one exemption (`production::unread_callout`): after the seen-set is filled they take `feature:parallel` back out, the Expeditions callout, whose place is Operations, so arriving on Camp home or Upgrades never marks it and every frame draws the row with Show me and Got it. The row is static, so both modes draw it alike. Test: `callout_stills_leave_one_callout_unread_on_camp_and_upgrades`.
- Closing phase (R36): no still showed the W3-15 low-HP band (`docs/components/battle-low-hp.md`). `low-hp` (before proxy `battle`) and `low-hp-observed` (before proxy `npc`), since wave 3, owner ux-flow, are appended with their W3 matrix rows: a stage-0 Germany defense after one victory, the personal one played 20 s with the smoke input and Mara's main run (Companion learned after an entitled liberation of France) by her own decisions, then the defender entitled to 25% of its maximum HP (`entitle_defender_hp`) and paused. A paused battle holds the band static at 0.40, so both modes draw the band and the HUD's "· LOW" text alike. Test: `low_hp_stills_draw_the_band_for_the_personal_and_the_observed_defender`.

### Wave 5
- Register storm. The weapon-classes test gains the decision-16 gear-snapshot class assertion. Per-frame manifest fields land.
- Failing first:
  - `storm_fixture_is_a_storm_tagged_stage` (0 storm themes today)
  - `manifest_v2_records_flash_counters`
  - `capture_motion -Scene storm -ReducedEffects` must show every flash field at 0
  - vfx's `storm_pulse_schedule_at_most_one_per_240_ticks` (vfx owns it; QA references it)
- Done when: storm captured in both modes at both sizes; storm, fx-stress and weapon-classes-machine-gun reduced sequences show 0 flashes.

### Wave 7
**11. Build stamp.**
- Files: build_info.rs, main.rs, launch.rs, menu.rs:209-215, logging.rs:265-275.
- Failing first:
  - `build_info::tests::stamp_formats_dev_and_commit_builds`
  - `launch::tests::version_flag_selects_version_action`
  - the menu fit test extended with the stamp at 1050x700@1.35, with no overlap
  - `logging::tests::crash_report_names_build_stamp`

**12. package_playtest and verify_playtest under 5.1** (and the lint's PENDING_W7 list is emptied).
- Failing first:
  - `repo_checks::playtest_guide_has_no_links_outside_the_package` (7 links today)
  - package_playtest under 5.1 fails at GetRelativePath (:74)
  - verify_playtest fails at Add-Type (:14) and on the version-2 assertion
  - -NativeCapture on a non-review zip hangs today (:48-52)
  - run_script_tests gains New-PlaytestZip (no `\` entries) and Test-DocLinkClosure (catches a dead link)
- Done when: on a clean tree the zip has forward-slash entries, docs = {PLAYTEST_GUIDE.md} at most 65,536 B, and no archive/ or source-only files. verify prints PASS (`--version` contains the stamp; smoke `^PASS: `). The non-review -NativeCapture run fails in under 5 s, and the -Review package passes -NativeCapture. Sizes are recorded in VALIDATION.md.

**13. Docs inventory, guide sections and click-to-photon.**
- Failing first: `repo_checks::playtest_guide_covers_the_production_loop` (headings Loop, Escape, Auto-sell, Takeover, Listening pass, Click-to-photon). Also `rg "There is no automatic sale system" docs` hits PROGRESSION.md:23 today.
- Done when: green, and check_views counts at least 69 plus the new views.

**14. Final review (goal 10).**
- Files: compare_captures.py, check_capture_findings.py, curate_capture_pairs.ps1, plan ledger, docs/screenshots/production.
- Failing first: `check_capture_findings.py --final` exits 1 today (no table; CF-0-01 open).
- Done when:
  - the final matrix is complete (registry stills x 4, about 264) with captures.json
  - motion runs done: menu, storm, fx-stress, weapon-classes-shotgun and weapon-classes-machine-gun, both modes, both viewports
  - every changed pair reviewed against the loop map, with no `{:?}` labels and no flashes in reduced mode
  - every CF row fixed or deferred with user approval
  - pairs at most 40 MB, `--final` exits 0
  - click-to-photon and listening passes logged, with numbers or as "pending human check"

## 4. Digest moves
None. QA touches only review fixtures, scripts, docs, presentation-only text (menu stamp, review status), log paths and unreferenced assets. The storm tag is vfx-owned. It is a serde-default content field read only by horde_game stage_theme consumers (battle_view.rs:654, animation.rs:485, briefing.rs:75, context.rs:141-147, chrome.rs:133), so vfx must confirm that GOLDEN and GAMEPLAY_TRACE stay unchanged. If a theme ever enters Attempt or the sim, that wave (2 or 4) must state the move and its reason. smoke.txt stays byte-identical apart from other remits' wave-2 and wave-4 balance moves.

## 5. Risks
- Script proof needs a built exe; captures queue behind serialised builds (2-builder budget). Use `-NoBuild` and Resolve-HordeTarget for both ./target and the shared CARGO_TARGET_DIR.
- The headless prepare-every-still tests are expensive: results up to 28,800 ticks, results-defeat cap 36,000, horde_core at dev opt-level 1. They are split into 3 test fns. If one exceeds about 60 s, cap results-defeat at the first Defeat and keep the cap asserted.
- Launch validation rejects unlisted names. The package launchers use only 'construction' and 'operations', both registered. The Workshop prefixes must stay listed, or capture_player_models breaks; the workshop test guards this.
- option_env! LASTLIGHT_BUILD_COMMIT rebuilds horde-defense when the value changes. Build and copy must happen in one package_playtest invocation, and the variable is restored afterwards. verify's `--version` check guards this.
- ZipFile forward-slash entries need .NET 4.6.1+ (this machine has 4.8). verify asserts that no entry contains `\`.
- All script text must stay ASCII (5.1 reads BOM-less files as ANSI), so the stamp uses ' ( )'. JSON is written UTF-8 without BOM, and the python checkers read utf-8-sig.
- Zoom-to-fit changes every battle capture. compare_captures flags all battle pairs as changed, and each must be reviewed. Two tests pin the old geometry; the second is rewritten with a written reason.
- The search-based scenes (night-checkpoint, storm, battle-mid) can pick a different country if world.json is regenerated (decision 12). The fixtures assert the predicate, not a fixed country, but pair images may differ.
- The orphan test's runtime set must grow in the same commit as new audio banks (wave 6) or new art. That is intended, and owners must know. Excluding source-only art from the zip relies on the same set; smoke plus NativeCapture are the backstop.
- Captures are physical 2x (windows_scale 2.0). Pairs are comparable only at equal windows_scale; captures.json records it, and compare_captures refuses to pair mismatched runs.
- The sidecar construction-rect assertion stays red until ux-flow's overlay lands. It must be tracked as an open CF row, not disabled.

## 6. Numbers to measure
- Wave-0 captures 156/164 OK, giving the target 100% of registry stills x 4.
- Per-process capture time 5.2 s, so the final run is about 23 min.
- assets/ 45,903,392 B to 29,621,371 B. Package non-exe payload about 72.5 MB to about 21.6 MB, plus wave-6 audio (recorded at W7).
- docs/ in the package: 26,555,784 B (143 files) to at most 65,536 B (1 file).
- Committed pairs at most 40 MB.
- Click-to-photon median at most 50 ms (default) and at most 34 ms (Low latency); any click over 67 ms is a finding.
- check_views at least 69 views; 78 shipped WAVs until wave 6.

## 7. Violations fixed
- B's matrix.rs:358 `unreachable!`: the arm is kept for Workshop with a debug_assert, and a test guards it.
- B's registry in private ui/review with ui types: the registry is ui-free crate::review_scenes, and the expected Screen/Tab live in a test table.
- B's core stage_theme_with change: night-checkpoint uses a predicate search.
- B's campaign.equip bypass: Command::Equip, with only entitle_* using staging.
- B's scope creep: 3 scripts allowlisted, no check_views port, only 3 extra scenes, sidecar trimmed.
- B's observer figure: corrected to the 86 pt floor.
- A's Stop-Process: replaced with taskkill /T.
- A's W1 snapshot assertion: W1 asserts the equipped class and W5 adds the snapshot.
- A's same-commit lint claim: the lint lands with the port, and package/verify are on PENDING_W7.
- A's 240-tick rate from 24 frames: now a vfx unit test.
- A's missing before-evidence: the supplemental set.
- A's thin docs inventory: the table in 1.13.

---

## Integration rulings

This section was added on 2026-11-05 when the synthesized spec above was copied into the plan. The spec above is kept verbatim as the design record. **Where this section and the spec disagree, this section wins.** The rulings come from four sources, applied in this precedence order: user decisions (U), orchestrator rulings (R, RL), master-planner reconciliations (P) and the five integration cluster rulings (C, D, E, S, W). The final saved-field table is `schema-v7.md`, the commit order is `waves.md` and the proof for every criterion is `acceptance.md`, all in this folder. The plan itself is `docs/plans/2026-11-05-production-pass.md`; any reference above to `docs/plans/2026-11-04-production-pass.md` means that file.

Each entry gives the binding ruling and then what it supersedes in **this** spec.

### User decisions

#### U3. Loudness gate (replaces the brief's -16..-14 LUFS)

- **Owner / wave:** audio, wave 6.
- **Ruling:** A 60 s mix at DEFAULT sliders reads -23 +/-2 LUFS integrated with true peak &lt;= 0.95 FS. Recorded in ACCEPTANCE.md, AUDIO_SURVIVAL.md and the plan (W7-03).
- **Supersedes in this spec:** ACCEPTANCE.md rows must carry -23 +/-2 LUFS / TP &lt;= 0.95.

#### U6. Camera

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** Uniform zoom-to-fit that keeps the lane:yard proportion; fewer lanes means more zoom; construction never rescales the arena. Detail in R1 and the camera-layout cluster.
- **Supersedes in this spec:** battle::fit_field name and formula.

#### U7. Archive list

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** Archive to horde-defense/archive/: the 12 unreferenced WAVs, the 6 unreferenced root sheets in assets/art/, the 5 unreferenced animation strips in assets/art/animation/, pixel/keyart-v1.png, and survival_npc_shot_0..2.wav (Cue::NpcShot stays as a retired row). Nothing is deleted.
- **Supersedes in this spec:** "Retain strips and keyart-v1 as source-only" and the source-only package exclusion; the open archive question is closed.

### Orchestrator rulings

#### R1. Camera formula, owner and construction overlay

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** Base = today's per-geometry clearing_rect for (lane_count, width_yards) including its clamps; uniform zoom z = min(arena.w/base.w, arena.h/base.h) (&gt;= 1), centred; no texel or magnification clamp; 17x30 and every other already-filling field stays pixel-identical; the worker observer window uses the same fit function. Owner vfx-lighting, implemented in WAVE 1 because later captures depend on it. Construction is an overlay panel drawn over the field (no arena rescale), with an INCOMING occlusion chevron when the panel covers spawns; Save/Clear plan stay in the panel.
- **Supersedes in this spec:** s = min(30/width, 11/lanes) formula and the name battle::fit_field.

#### R6. Root files

- **Owner / wave:** qa-captures, wave all.
- **Ruling:** No root-repo file is touched. Drop any dev.ps1 annotation; the `pwsh -NoProfile -File ../scripts/dev.ps1 verify -Project horde-defense` line in README.md and docs/VALIDATION.md (README.md:165 at 3d492cd, :166 after the wave-1 review; VALIDATION.md:147) stays byte-identical, pinned by `repo_checks::root_helper_line_is_unannotated`.
- **Supersedes in this spec:** §1.12 "annotate README.md:165 and VALIDATION.md:147 (root dev.ps1)".

#### R8. Single owners

- **Owner / wave:** progression-tree, wave 1-4.
- **Ruling:** Pacing harness v2 = progression-tree (wave 2; CD and EB add profiles on its API); per-archetype telemetry = enemy-balance; SHA-256 / provenance / orphan helpers and scripts/lib/Native.ps1 = qa-captures (wave 1; audio, vfx and perf reuse them); measure_performance.ps1 = perf-architecture on top of Native.ps1; storm tagging = vfx presentation table only (no world.json change); regional checkpoints are derived (no saved field), presentation lines by CD plus EB's Regional boss rank (HP and reward x1.2) in wave-4 balance, no theme-rule change; save_v7.json is cut at the wave-1 close and frozen, later cases use inline JSON or new fixtures; stress scene raises the staged enemy cap only behind a review/test-only seam (no production save accepts &gt; cap+1), owner perf.
- **Supersedes in this spec:** storm: bool on StageThemeDefinition; measure_performance plumbing port.

#### RL. Lightning pacing and reduced storm grade

- **Owner / wave:** vfx-lighting, wave 5.
- **Ruling:** Lightning pulses use wall-clock pacing (&lt;= 1 per 4 s wall time at any observation speed). Storm stages in non-storm themes get a static storm grade in reduced mode.
- **Supersedes in this spec:** "one pulse per 240 ticks" test.

### Cluster: camera-layout

#### C-1. Camera owner, wave and file placement

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** vfx-lighting implements camera and battle layout in wave 1 in four commits: C1 crates/horde_game/src/art/camera.rs (pure Rect math, re-exported as art::{clearing_size, fit_field, FieldFit}); C2 ui/battle/layout.rs BattleLayout plus main-battle wiring; C3 observer window; C4 INCOMING chevron. perf's ui/battle split (W1-39) lands before C2. Presentation-only: no saved field, no digest move, no Command change. perf measures the commit before C2 and C2. qa verifies by captures and does not write camera tests. C2 deletes clearing_rect and battle_arena (grep guard).
- **Supersedes in this spec:** The name battle::fit_field (it is art::fit_field).

#### C-2. Camera formula

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** art/camera.rs: REFERENCE_LANES 11.0, REFERENCE_WIDTH_YARDS 30.0, MIN_CLEARING vec2(160, 86); FieldFit{rect, zoom}; clearing_size(reference, lanes, width) is today's clearing_rect size moved verbatim from ui/battle.rs:475-489; fit_field(reference, area, lanes, width): zx = area.w/base.x, zy = area.h/base.y, the smaller wins and the limiting axis snaps exactly to the area; centred. No texel clamp. Every field with &gt;= 11 lanes is bit-identical to today. Pinned zooms: arena 1392x692: 3x12 1392x471.8 z2.5, 5x14 1392x674.0 z2.143, 7x16 1166.6x692 z1.571, 9x18 1020.8x692 z1.222; arena 729.8x310.5: 3x12 729.8x215.0, 5x14 729.8x302.5, 7x16 611.6x310.5, 9x18 535.2x310.5. EXPANSION.md:99 and PIXEL_PRESENTATION.md:77 get a dated supersession note. Tests: fresh_three_lane_battle_fills_the_arena_width_at_small_size (fail-first), fit_field_zoom_table_at_both_target_sizes, arena_fits_full_lanes_and_width_on_the_limiting_axis, fit_field_keeps_every_filling_geometry_bit_identical (pin with two mutation proofs).
- **Supersedes in this spec:** s = min(30/width, 11/lanes) formula.

#### C-10. HUD at most 2 rows

- **Owner / wave:** ux-flow, wave 3.
- **Ruling:** The HUD row metric is the bottom bar. BattleUi.bar_rows is measured per frame by a BarRows accumulator; ReviewFrameFacts.hud_rows = Some(bar_rows) from then on. Reload, Strike contact lane and Toggle firing move to Management &gt; Field actions (keys kept); the status line is removed and its message goes to the battle toast in layout.receipt_slot. The construction palette is not moved (R1). Tests: battle_bar_rows_measure_todays_bar (characterisation, 3) then battle_bar_stays_within_two_rows_at_small_size.
- **Supersedes in this spec:** "hud_rows comes from the hud layout" (unspecified).

#### C-12. Camera capture review and human check

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** qa captures battle, construction, max-lanes (17x30), peak/progressed-peak and operations-observer at both sizes, normal and reduced, after C4, and reviews every flagged pair with the written reason "uniform zoom-to-fit, construction overlay, effects headroom". 17x30 and 11x20 pairs must show no arena move. ReviewFrameFacts arena_rect/effects_rect come from f.battle; observer facts from observer_arena_rect. Human checks "3x12 at 2.5x zoom feel" and "sheet upscale on 3-lane sprites" are logged pending if not done.
- **Supersedes in this spec:** Confirms the W1-60 review.

### Cluster: settings-audio-tooling

#### S7. SHA-256: one implementation

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** One std-only FIPS 180-4 SHA-256 at crates/horde_core/src/sha256.rs, #[cfg(any(test, feature = "test-support"))] pub mod sha256 with digest() and hex(). No new crate, dependency or Cargo.lock change. PowerShell uses Get-FileHash via Native.ps1 Get-FileSha256; Python keeps hashlib.
- **Supersedes in this spec:** In-test SHA in asset_audit_tests.rs.

#### S8. Provenance and orphan audit helpers

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** One #[cfg(test)] mod asset_audit at crates/horde_game/src/asset_audit.rs (qa): workspace_root, memoised file_sha256, shipped_files, audio_manifests (both shipped manifests), art_manifests, archive_manifest. Runtime sets: visual_assets::loading::PATHS (pub(crate)), enemy_visuals sheets, player_models PNGs, #[cfg(test)] audio::runtime_wav_paths() (Cue::ALL minus retired x variants plus beds). vfx's provenance walker folds in; later owners extend only the runtime-set functions in the commit that adds files. Tests include every_shipped_wav_is_loaded_and_matches_provenance (78 then 75), shipped_art_has_provenance_with_matching_sha256, shipped_art_is_referenced_at_runtime, archived_files_keep_matching_provenance, player_model_png_sha256_matches_file, every_manifest_entry_exists.
- **Supersedes in this spec:** The asset_audit_tests.rs name.

#### S9. Archive layout

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** One qa commit with git mv mirrored under horde-defense/archive/: 6 root sheets to archive/art/, 5 strips to archive/art/animation/, pixel/keyart-v1.png to archive/art/pixel/; one archive/art/PROVENANCE.json (12 entries keyed relative to archive/art, original record plus sha256, bytes, archived_from, archived, reason). The keyart pin (visual_assets/tests.rs:363) retargets to the archive path. 12 legacy WAVs to archive/audio/ with archive/audio/PROVENANCE.json (12 records plus 8 preserved_originals). archive/README.md: "Not shipped; retained by production-pass decision 10; restore with git mv". archive/ is outside assets/, so packaging needs no change.
- **Supersedes in this spec:** "Retain strips/keyart-v1 as source-only", source-only tagging and package exclusion, flat archive/art/.

#### S10. NpcShot retirement

- **Owner / wave:** audio, wave 1.
- **Ruling:** Separate audio commit (W1-58) right after the archive: runner/runtime.rs:390-392 drops the Npc =&gt; NpcShot arm so NPC shots use profile.weapon.class; Cue::NpcShot keeps its slot and gains Cue::retired(); retired cues are excluded from runtime paths and gate tables. survival_npc_shot_{0,1,2}.wav move to archive/audio/ with their records in a new archive/audio/SURVIVAL_PROVENANCE.json (reason "Cue::NpcShot retired; NPCs use their weapon class (user 2026-11-04)"). Shipped WAV count becomes 75.
- **Supersedes in this spec:** The 78-WAV final count.

#### S11. Shipped audio provenance layout

- **Owner / wave:** audio, wave 6.
- **Ruling:** Two shipped manifests forever, one writer each: assets/audio/PROVENANCE.json (Python family: the 6 beds, written by the atmosphere script; LF-normalised generator_sha256) and assets/audio/SURVIVAL_PROVENANCE.json (Rust family: exporter). In wave 1 the survival file keeps its schema minus 3 npc_shot records (69) and documents its stale generator_sha256 in a note key. In wave 6 the exporter rewrites it as schema 2 per file, after copying v1 verbatim to archive/audio/SURVIVAL_PROVENANCE.v1.json.
- **Supersedes in this spec:** "Both PROVENANCE files written by the atmosphere script".

#### S12. assets/CREDITS.md lines

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** A generated block between &lt;!-- asset-index:begin --&gt; and &lt;!-- asset-index:end --&gt; with one line per shipped .wav/.png, rendered from the provenance manifests by the ignored writer test asset_audit::regenerate_credits_index; guard credits_index_matches_provenance. qa writes it in the archive commit and rewrites the stale prose plus an "Archived (not shipped)" paragraph; audio refreshes it in W1-58 and A6-05; vfx in any art commit.
- **Supersedes in this spec:** Wording-only CREDITS edit.

#### S13. Generator retargeting and --check

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** In the archive commit both Python generators get ASSETS and ARCHIVE roots and explicit owned-file lists; generate_audio.py renders ambience to assets and the 8 legacy clips to archive; generate_atmosphere_audio.py renders loops to assets and its 4 cues to archive and writes only the two Python manifests. --check is the default (render to a temp dir, compare sha256 and manifest fields, exit 1 on mismatch, write nothing); --write is explicit. Invoked as python -I -B; both --check runs join the full verify.
- **Supersedes in this spec:** --check as a new flag with --write the default.

#### S14. Native.ps1: one PowerShell 5.1 helper

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** One dot-sourced scripts/lib/Native.ps1 (ASCII, LF, no BOM, #requires -Version 5.1), qa names only: Resolve-HordeTarget, Get-HordeExecutable, ConvertTo-NativeArgument, Invoke-Native (Start-Process with redirected streams, or, as amended by R24 (plan §2.4, user 2026-11-07), `[Diagnostics.Process]::Start` with a `ProcessStartInfo`, UseShellExecute off and stdout and stderr redirected to the logs; never `>`; taskkill /T /F on timeout, returns ExitCode/Id/Seconds/Out/Err), Invoke-Cargo, Resolve-Tool, Reset-/Restore-LastlightEnv, Write-Utf8NoBom, Get-FileSha256. Timeouts cargo 1800 s, capture 300 s, smoke 900 s, --version 30 s, git/rustc 30 s. Tested by scripts/tests/run_script_tests.ps1.
- **Supersedes in this spec:** Nothing beyond the ruling itself; this remit implements it.

#### S15. Script porting order and the 5.1 lint

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** qa step 1 Native.ps1 plus tests; qa step 2 ports capture_review.ps1 and capture_motion.ps1 only; perf W1-36 is the single owner of measure_performance.ps1 (full rewrite with -Profile, -VsyncOff, -Pipelined, stress, budget compare, exit codes, -SelfTest). repo_checks::powershell_scripts_are_51_safe keeps an explicit PENDING list (measure_performance removed by W1-36; package/verify by W7-02); capture_player_models, trace_npc and verify_player_models stay on a PS 7 allowlist.
- **Supersedes in this spec:** Step 2 porting measure_performance "harness only".

#### S16. Root dev.ps1, AGENTS.md target rule, __pycache__

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** Nothing annotates dev.ps1 (R6); the root dev.ps1 line in README.md and docs/VALIDATION.md stays byte-identical to 3d492cd (named by content in R6, since its README line number moved from 165 to 166); the docs lint matches only "-File scripts/". AGENTS.md:10 becomes "Each workspace owns target/ unless CARGO_TARGET_DIR is set; every script under scripts/ honours it." horde-defense/.gitignore gains /scripts/__pycache__/.
- **Supersedes in this spec:** §1.12 annotation of root dev.ps1.

### Cluster: enemies-world-pacing

#### W-8. Storm tagging, reduced storm grade and wall-clock lightning

- **Owner / wave:** vfx-lighting, wave 5.
- **Ruling:** art::lighting::STORM_THEME_IDS = ["depot"]; is_storm(theme) = weather == "storm" (case-insensitive) or id in the table; no StageThemeDefinition.storm, no world.json or theme-order change. Reduced effects on any storm stage: sky_flash 0 and the static Lighting::Storm grade. sky_flash takes wall_ms from an unscaled real-time accumulator (paused frames excluded): SKY_SLOT_MS 8000, SKY_OFFSET_RANGE_MS 4000, SKY_RISE_MS 33, SKY_DECAY_MS 233, SKY_QUIET_BOSS_MS 1000, SKY_QUIET_CUE_MS 750; pulse starts &gt;= 4000 ms apart at 1x/2x/4x; schedule hash splitmix(variation ^ slot ^ SKY_TAG), never core RNG.
- **Supersedes in this spec:** serde-default storm: bool on StageThemeDefinition for light_rain and drizzle; storm_pulse_schedule_at_most_one_per_240_ticks.

#### W-9. Regional checkpoints and Capstone

- **Owner / wave:** enemy-balance, wave 2, 4.
- **Ruling:** One derivation: FrontPosition{stage, count}::boss_rank() (Capstone if stage+1 == count, else Regional if (stage+1) % 6 == 0, else Sector); Country::regional_checkpoints() from it (&lt;= 3 for 24 stages). Nothing saved per stage; only attempt-scoped snapshot.boss_rank and Report/RunRecord.boss_rank. No theme change, no checkpoint coin grant. Wave 2: FrontPosition, stamping, Capstone +1 transition. Wave 4: Regional HP x1.2, reward x1.2; Capstone HP x1.4, armor x1.25, NPC armor +2, reward x1.4, damage x1.10. CD owns presentation lines in wave 4 reading Game::stage_boss_rank.
- **Supersedes in this spec:** night-checkpoint keeps the predicate search.

#### W-12. CPU p99 10 ms gate and integrated-GPU pending log

- **Owner / wave:** perf-architecture, wave 1.
- **Ruling:** docs/PERF_BUDGET.json is the single gate source: scenes.progressed-peak and stress carry cpu_frame_p99_ms_max 10.0 and frames_over_50ms_max 0, evaluated only on -VsyncOff runs (median of 3 for p99, worst repeat for &gt; 50 ms); 12.0 ms is only the pipelined-decision threshold. perf_budget.rs mirrors CPU_FRAME_P99_MS_MAX / FRAMES_OVER_50MS_MAX with a bidirectional test. hardware.integrated_gpu = {status "pending", adapter "Intel(R) Arc(TM) Graphics", report null} plus the ACCEPTANCE.md human-check row.
- **Supersedes in this spec:** The human-check list references the perf row.

#### W-15. Enemy and pacing documentation ownership

- **Owner / wave:** enemy-balance, wave 4.
- **Ruling:** enemy-balance writes ENEMY_COMPOSITION.md, PROGRESSION_SCALING.md, the goal-6 section of BALANCE_AND_RUN_STATS.md and the WORLD.md band paragraph at the wave-4 close (W4-10); progression-tree writes the goal-5 section and the AGENTS.md pacing_targets section (W2-03, W4-11); qa's docs inventory checks presence only.
- **Supersedes in this spec:** Docs inventory assigns no authors.

### Wave-1 close rulings (2026-11-07)

Orchestrator rulings R12-R23, recorded verbatim in plan §2.4. The ones that touch this remit:

#### R13. Review recipes may advance their own Game

- **Owner / wave:** save-determinism and qa-captures, wave-1 close.
- **Ruling:** Review recipes (crates/horde_game/src/ui/review/fixtures.rs, ui/review/fixtures/production.rs, ui/review/matrix.rs; review feature only) may call .tick(; record the exception in design-save-determinism.md Integration rulings and cite it in the architecture_tests.rs allowlist comment.
- **Supersedes in this spec:** Any reading that only the runner may call `.tick(` in horde_game. The `.tick(` rule in `crates/horde_game/src/architecture_tests.rs` allows `runner/` and the three review recipe files and cites this ruling.

#### R14. Per-commit capture reviews

- **Owner / wave:** qa-captures and perf-architecture, wave-1 close.
- **Ruling:** Per-commit capture reviews named in a done_when (e.g. W1-48 batching) are satisfied by the wave-close pair review (target/production-after/wave1 vs target/production-before, 356 captures, CF ledger rows). Note this on the W1-48 row in waves.md.
- **Supersedes in this spec:** A separate per-commit pair review for W1-48 (and any other wave-1 done_when that names one). The evidence is plan §10.3 and the CF ledger.

#### R17. Capture rows CF-1-01, CF-1-02, CF-1-03 and CF-1-07

- **Owner / wave:** vfx-lighting, wave 3 (W3-29, W3-30, W3-31).
- **Ruling:** CF-1-01, CF-1-02, CF-1-03 and CF-1-07 get new wave-3 ids appended in waves.md (owner vfx-lighting, before W3-27, beside W3-14 for BattleLayout), each with its CF fail-first test; update the CF ledger rows with those ids.
- **Supersedes in this spec:** CF-1-03's qa-captures owner: W3-30 is a vfx-lighting commit. The ledger rows in plan §10 name the ids and fail-first tests.

#### R20. PowerShell 7 allowlist

- **Owner / wave:** qa-captures, whole pass.
- **Ruling:** capture_player_models, trace_npc and verify_player_models stay on the PS7 allowlist (ruling S15); no port in this pass.
- **Supersedes in this spec:** Any port of these three scripts in this pass; S15 is unchanged.

### Rulings owned by this remit

U7, R6, C-12, S7, S8, S9, S12, S13, S14, S15, S16, R14, R20
