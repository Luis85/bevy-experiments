# Lastlight: Horde Defense

A native Bevy pixel-art survival game based on the supplied [Horde Defense GDD](docs/GDD.md). Hold a deployment-frozen defense line that grows from a short three-lane opening, develop a survivor, delegate real NPC battles and build a connected campaign.

This is the monorepo's fifth independent Cargo workspace. It uses Rust **1.99.1**, Bevy **0.20.0**, its own lockfile and its own `target/`. It does not change the other games' workspaces or numerical models.

## Run

From this directory:

```powershell
cargo run --locked -p horde-defense
```

The first graphical build is substantial. On Windows, install the Visual Studio C++ Build Tools and Windows SDK, and use a supported graphics driver. The game opens at 1440 × 900; the window can be resized down to 1050 × 700. Keep the battlefield visible when selecting a text scale.

For a bounded **headless** smoke check, without opening a game window:

```powershell
cargo run --locked -p horde-defense -- --smoke
```

The smoke check validates authored content and an identical save/resume round trip, checks the fresh companion lock atomically, and runs an earned-shop Personal route plus a separate explicit legacy-compatible NPC route through five encounter stages. The compatibility route is not proof of fresh companion unlock progression. Compilation or this headless check does not establish visual correctness or a tested frame-rate target.

## Packaged playtest

```powershell
pwsh -NoProfile -File scripts/package_playtest.ps1
```

The script builds the `playtest` profile (release optimisation with line tables for readable crash backtraces) by default; pass `-Profile debug` or `-Profile release` to choose another. Packages are built with `--no-default-features`, leaving out the `review` tooling (practice scenes still work); pass `-Review` to include it, which `verify_playtest.ps1 -NativeCapture` needs. It creates a versioned Windows x64 folder and ZIP under `target/playtest/`, with the executable, pixel art, audio, font/geography notices, editable content JSON, manifest and playtest documentation. `Play Lastlight.cmd` opens the ordinary persistent campaign; `Battle practice.cmd` and `Strategy practice.cmd` open separate resumable practice profiles with clearly staged resources. No Rust installation is needed to run this build. See the [playtest guide](docs/PLAYTEST_GUIDE.md).

The packaged launchers load `content/world.json`, `content/encounters.json`, `content/skills.json`, `content/enemy_catalog.json` `content/ordnance.json`, `content/progression.json`, `content/quality.json` and `content/balance.json` through the same validated override interfaces as development. Changes load on restart; use separate saves when testing incompatible content changes.

The optional **Play Lastlight (DirectX12).cmd** launcher selects DirectX 12 for that process; the ordinary launcher keeps automatic backend selection. Backend evidence and limitations are recorded by build in [VALIDATION.md](docs/VALIDATION.md).

## First session

Choose New game, confirm protection of an existing journey if prompted, then choose Rook or June and an infected starting country. The country begins live Personal combat. Every settled main round—victory, defeat or withdrawal—returns to Journey upgrades: one complete connected tree, Equipment/analysis routes and explicit Continue or Leave. New game archives before replacing a full profile; leaving keeps earned progression. Details: [JOURNEYS.md](docs/JOURNEYS.md).

The survivor stays at the left-center of the frozen arena (initial Normal center lane 2). Aim/fire/reload and answer any contact lane with targeted melee. Faster approaches may overtake moving peers inside their assigned lane; intact walls/contact still stop and queue. Conscious zero/negative-HP NPC hostiles remain explicitly critical under the retained injury rules.

The timer brings the boss onto the field. It does not award victory. Defeat the boss to clear the stage; ordinary enemies continue to arrive during the boss phase. Coins and equipment are banked immediately. Retreat, defeat or recall preserves them; the survivor returns to rest and training rather than permanent death. Retry starts a live new encounter, while preparation allows spending coins through the unified tree, comparing equipment and selling unwanted loot. Conserved old CP remains an eligible legacy payment rather than a fresh allocation flow.

Journey upgrades is the main home between rounds. Continue retries the reported stage after defeat/withdrawal or advances after victory; a liberated country returns to eligible map fronts. Leave pauses and saves before menu. Preparation routes do not buy/equip automatically, and worker outcomes keep their independent orchestration/history.

Combat uses continuous pixel ground and compact instruments. The clearing grows physically from Normal 3/12, adding two lanes/two yards every three character levels, capped 17/30. Active settings/geometry and legacy 11/30 fields stay frozen. Persistent corpses settle in batches, without acting/blocking/paying again; positive injury keeps six-tick movement hit stop without changing attack/turn clocks.

One connected 169-node tree contains personal body development, firearm training/abilities, fortification blueprints/ranks and feature access. Branch focus pans to a path rather than filtering it into tabs. Select to inspect actual requirements/payment, then learn explicitly. Fresh body development uses earned coins; XP develops level/geometry without minting personal CP. Legacy points/allocations use a conserved adapter in the same graph. [JOURNEYS.md](docs/JOURNEYS.md) and [SKILLS_AND_WEAPONS.md](docs/SKILLS_AND_WEAPONS.md) define authoring/compatibility.

Mara/advanced controls appear through learned feature nodes and existing liberation milestones, with old saved capabilities retained. Mara/recruits keep independent one-second rules maneuvers and NPC advancement ledgers. The personal tree does not grant NPC attribute/skill improvements; mouse aim does not fire her weapon. Inspect the actual decisions/injury states.

## Default controls

| Control | Action |
|---|---|
| Mouse | Aim or inspect; select a construction cell while building |
| Left mouse / hold | Fire equipped gun in personal mode |
| Fire press with throwable selected | Throw once at the highlighted ground cell |
| Right mouse | Melee against the front enemy in the selected contact lane |
| R | Reload |
| 1 | Emergency aid, after purchasing the personal skill |
| 2 | Brace, after purchasing the personal skill |
| B | Open/close live construction; combat continues |
| Escape | Pause |
| F | Toggle firing |
| F12 | Capture the native framebuffer to the save directory's screenshots folder |

The settings screen supports rebinding all essential actions, toggle firing, automatic reload, aim assistance, outlines, damage numbers, text scaling, reduced effects, blood and separate sound levels. Mouse controls provide essential combat alternatives. Fire cadence is simulation-limited; rapidly clicking cannot increase it. Melee commits to recovery, blocks firing and reloading during that recovery, and cancels an unfinished personal reload without granting ammunition.

## Equipment, training and construction

Four slots share an armory: ranged weapon, melee weapon, outfit and accessory. Items have unique persistent identities; presets reference items rather than duplicating them. Protected starter equipment always remains available. Lock and favorite controls protect equipment. Sale previews expose affected presets; bulk selling excludes protected, favorited, equipped and preset-referenced items. Deployed or garrison-reserved items cannot be sold or equipped by a second active survivor. Loot storage has no gameplay capacity limit.

Fresh Personal XP and completion XP develop level and future geometry; body upgrades are coin-paid nodes in the complete tree. Eligible legacy CP remains a distinct payment, and NPC training remains separate. Proficiency only permits an owned eligible gun; it never manufactures or equips one. Six weapon classes, ordnance and JSON authoring are described in [SKILLS_AND_WEAPONS.md](docs/SKILLS_AND_WEAPONS.md).

Blueprint and structure ranks use the same learned prerequisites, victory/liberation gates and payment preflight. Construction access is learned through engineering. Placements spend renewed supply in the actual left half, keep combat live and preserve country plans; old blueprint/rank fields remain ownership floors. No separate body or fortification purchase screen bypasses the graph.

## Difficulty and run analysis

Settings → Gameplay offers Easy/Normal/Hard/Custom and a validated draft. Apply to next run commits it; active main/worker fields retain copied balance/dimensions. Normal movement is1.25× its authored base. Character MAX 50 and every-victory threat remain separate.

Live DPS is resolved injury over five simulation seconds (zero-padded initially), with whole-run average/peak and source totals. Pause freezes the clock;4× playback does not multiply it. History is reachable from menu/camp/results, pauses fields and restores prior pause on Back. It filters all recorded outcomes, exposes frozen gear/balance and plots the last 40 matching records without truncating the ledger or inventing past runs. See [BALANCE_AND_RUN_STATS.md](docs/BALANCE_AND_RUN_STATS.md).

## Equipped component survivors

Actual owned/equipped definition, level, quality, style and class are captured in the defender profile. A shared sixteen-part body and two-bone arm/socket graph drives battlefield rendering, displayed muzzle and the immutable equipped preview; stats and hit outcomes remain core-owned. The menu retains its separate cohesive low-ready artwork. [CHARACTER_COMPONENTS.md](docs/CHARACTER_COMPONENTS.md) documents authoring, ordering and compatibility.

## Country campaign and later strategy

The later [country and strategy requirements](docs/EXPANSION.md) extend the original GDD's single-expedition scope. They are the authority for recruitment, parallel country operations and plague pressure.

Country population determines its configurable local stage count. Stages rotate the five encounter templates, while each country supplies biome, cultural theme, difficulty, boosts, buffs and debuffs. Clearing every local stage liberates the country and unlocks its neighboring operations. The map uses authored travel links as well as land borders so island campaigns remain connected. Population and boundary sources, licensing and the interpretation of country records are documented with the world dataset.

Later milestones add recruiting local survivors, parallel autonomous country battles and strategic garrisons. Recruits carry a persistent origin, identity, individual NPC training history and independently owned equipment. An assigned survivor cannot guard and deploy at the same time. Plague alerts threaten liberated countries; respond through a successful deployment or assign equipped guards. An unresolved alert reopens the country's final stage, while already earned travel routes remain accessible. These clocks run only while the game simulation runs; there are no offline earnings or attacks.

[STRATEGY.md](docs/STRATEGY.md) explains the unlock thresholds, reservations, guard resolver and outbreak tuning. The default configuration places recruitment after one liberated country, parallel operations after two and strategic plague pressure after four. The world catalogue and thresholds are data-driven. Set `LASTLIGHT_WORLD` to a custom world JSON path and `LASTLIGHT_CONTENT` to a custom encounter/equipment JSON path before launch to load validated overrides without rebuilding. `LASTLIGHT_SKILLS`, `LASTLIGHT_ENEMIES` and `LASTLIGHT_ORDNANCE` select skill, compositional enemy/wave and throwable catalogs; see the [authoring guide](docs/SKILLS_AND_WEAPONS.md) and [enemy composition](docs/ENEMY_COMPOSITION.md). LASTLIGHT_PROGRESSION, LASTLIGHT_QUALITY and LASTLIGHT_BALANCE select victory, quality and difficulty preset catalogues. First boss clears unlock ordinary boss-derived variants in future waves, including worker expeditions. Otherwise bundled defaults are used; each catalogue is loaded once at startup. Copy the existing JSON as a schema-compatible starting point. Invalid overrides fail with an explanatory error rather than silently changing content.

## Saves

On Windows, saves and settings live under `%LOCALAPPDATA%\LastlightHordeDefense`. Set `LASTLIGHT_SAVE_DIR` to use a separate campaign directory, useful for testing. If `LOCALAPPDATA` is absent, the fallback is `saves/` relative to the working directory. Runtime saves and build products are not source assets.

The game serializes campaign, armory, active encounters, action timers, random state, reward accounting, recruited survivors and strategic pressure together. Saves use a versioned envelope, checksum, synchronized temporary write and a previous-generation backup. Save and quit acknowledges a completed save before closing. No progress is simulated while closed. Unreadable generations are preserved before replacement. Format v1 has a technical 64 MiB encoded-generation ceiling: oversized writes fail before touching existing saves, and no collected loot is discarded. See the acceptance ledger for exact persistence evidence.

## Logging and crash reports

The game logs through Bevy's `LogPlugin` to the console and to a plain-text file, `<save dir>\logs\lastlight-<unix-timestamp>.log` (the save directory is described above; the newest 10 log files are kept). The default filter is `warn,horde_core=info,horde_defense=info,wgpu=error,naga=warn`. Set `RUST_LOG` (for example `RUST_LOG=horde_core=debug,warn`) to replace it entirely. A panic also writes `<save dir>\logs\crash-<unix-timestamp>.txt` with the version, OS, panic message and location, and a backtrace. Attach the newest crash file and log to any bug report. `--smoke` keeps its plain stdout report, which `scripts/verify_playtest.ps1` reads.

## Develop and verify

```powershell
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked -p horde-defense --all-targets --no-default-features -- -D warnings
cargo test --locked --workspace
```

### Launch options and features

Every option the game reads is parsed once at startup; print the full list with:

```powershell
cargo run --locked -p horde-defense -- --help
```

Arguments are `--smoke`, `--help` and (with the `dev` feature) `--record <file>` / `--replay <file>`; everything else is a `LASTLIGHT_*` environment variable (empty values count as unset). Incompatible combinations, such as a capture sequence outside an isolated motion review, exit with an error before a window opens.

Cargo features of `horde-defense`:

- `review` (default): model workshop and parity screens, review fixtures, automated captures (`LASTLIGHT_REVIEW_CAPTURE`/`_EXIT`), capture sequences and frame reports. A build without it ignores review options (and non-practice scenes) with a logged warning; practice scenes (`LASTLIGHT_PRACTICE=1` with `LASTLIGHT_REVIEW_SCREEN`) and F12 screenshots work in every build.
- `dev`: Bevy `file_watcher` (asset hot reload), `track_location` and `sysinfo_plugin`, plus the debug tools below, e.g. `cargo run -p horde-defense --features dev`. (`bevy_dev_tools` is not included: no `bevy_dev_tools` release matches the locked `bevy_internal` 0.20.1, so the overlays are hand-rolled egui.) Default and `--no-default-features` builds contain none of it.

### Debug tools (`dev` feature)

| Key | Action |
| --- | --- |
| F2 | Frame-time / FPS overlay |
| F3 | Simulation panel: attempt id, seed, tick, enemies/cap, corpses, speed, runner/persistence queue state, last event sequence, settlement-issue count, recording state |
| F5 | While paused in a battle, run exactly one fixed tick. The runner performs it (unpause, tick, re-pause as recorded commands); nothing mutates the game from the overlay |
| F6 | Dump the game (the save payload JSON) to `<save dir>/dumps/game-<unix-ts>.json` and log the path |
| F7 | Start or stop command recording to `<save dir>/recordings/rec-<unix-ts>.jsonl` |

F12 remains the screenshot key. These keys are read directly from the keyboard, so avoid rebinding a gameplay action to F2, F3, F5, F6 or F7 in a `dev` build.

Recording: `--record <file>` or `LASTLIGHT_RECORD=<file>` starts with the session. A recording is JSONL: a `header` (`seed`, save `version`, `ruleset`, `content_fingerprint`, the starting `start_tick` and the full starting game state, so a recording begun mid-session replays exactly), then `{"tick", "kind":"command"}` lines for every command the runner applies, `{"tick", "kind":"input"}` lines whenever the per-tick input differs from the previous tick's, and an `end` line with the final state digest. Lines are buffered and flushed once per frame (no per-line fsync); an unclean stop leaves a recording without an `end` line, which still replays to its last input. Starting a new journey ends the recording, because the state swap cannot be placed on a tick.

Replay: `horde-defense --replay <file>` (no window) rebuilds the game from the header, replays it, prints `state_digest: <16 hex digits>` and exits 0. A different ruleset, save version or content fingerprint is reported as a warning. If the recording has an `end` digest and the replay differs, it prints `MISMATCH` to stderr and exits 1.

The root helper also recognizes this experiment:

```powershell
pwsh -NoProfile -File ../scripts/dev.ps1 verify -Project horde-defense
```

To reproduce the native framebuffer matrix, including every screen at the minimum window and 135% UI scale:

```powershell
pwsh -NoProfile -File scripts/capture_review.ps1
```

The [capture script](scripts/capture_review.ps1) stages isolated native fixtures in its requested output directory (default target/playtest-review/screenshots), at both supported sizes. Its scene lists and [VALIDATION.md](docs/VALIDATION.md) define current coverage/results; older matrix counts are historical. Framebuffers do not establish an OS playthrough.

The follow-up [animation](docs/REVIEW_ANIMATION.md), [atmosphere](docs/REVIEW_ATMOSPHERE.md) and [UX](docs/REVIEW_UX.md) pass adds original four-pose enemy gaits, contact/recoil/reload feedback, persistent fallen bodies and bounded impact effects, regional weather and layered environmental audio. The [survival audio guide](docs/AUDIO_SURVIVAL.md) covers the new 72-file WAV library and its separate listening-evidence limits. Reduced effects retain tactical cues. Camp offers explicit world pause/speed controls, and debriefs identify banked rewards and the next preparation step.

`scripts/capture_motion.ps1` exports 24 native frames into a fresh directory, with a normal/reduced-effects comparison option. Its isolated fixed-timestep export is animation evidence, never a real-time frame-rate measurement.

`horde_core` owns validated balance/catalogues, saved geometry, fixed-tick rules, DPS, run records, economy and persistence. `horde-defense` owns input, projections, UI, graphics and sound. Contact stays x=1.5; shared helpers derive build bounds/centers from frozen width. The schedule remains60Hz with legal one-second NPC opportunities. Rendering never rerolls attacks or advances metrics.

View documentation: [one file per View, with parent hierarchy](docs/VIEW_HIERARCHY.md).

Useful documents: [architecture](docs/ARCHITECTURE.md), [combat review](docs/REVIEW_COMBAT.md), [campaign review](docs/REVIEW_CAMPAIGN.md), [playtest guide](docs/PLAYTEST_GUIDE.md), [GDD](docs/GDD.md), [later expansion requirements](docs/EXPANSION.md), [progression](docs/PROGRESSION.md), [world data](docs/WORLD.md), [strategy](docs/STRATEGY.md), [rules coverage](docs/RULES.md), [design system](DESIGN.md), and [acceptance evidence](docs/ACCEPTANCE.md). The [asset credits](assets/CREDITS.md) and bundled font OFL notice cover third-party font and geographic sources. The rules coverage document records supported procedures and explicit adapters. The GDD's distribution-rights dependency remains relevant: access to the free reference is not a digital-game license. No proprietary rule tables or rulebook PDFs are bundled.

The simulation tests are regression evidence, not a claim that every build, culture modifier or country campaign has been balanced. GUI playtesting, accessibility perception, long-session balance, representative peak performance and target-hardware measurements require actual interactive evidence.

The current survivor renderer uses the source-owned [player model facade](docs/CHARACTER_COMPONENTS.md): original Rook/June bind reconstruction has exact native parity, with separately accepted static equipment/action confirmation. Near-vertical rifle/head-pitch poses remain stylized 2D, and exact still parity is not a claim for changed poses or garments. [Model authoring](docs/PLAYER_MODEL_AUTHORING.md) and [model validation](docs/PLAYER_MODEL_VALIDATION.md) document that boundary. The source View hierarchy also includes isolated developer parity/parts/matrix/repair tasks. Older validation counts and archives describe earlier revisions, not delivery of this model increment.
