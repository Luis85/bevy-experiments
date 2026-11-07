# Playtest validation evidence

Recorded for the comprehensive GDD, country/strategy and architecture pass begun on 1 November 2026 and the animation, atmosphere and UX follow-up on 2 November. Automated behavior, native rendering, human interaction and balance answer different questions; the evidence below keeps those scopes separate.

## Reference-driven pixel survival pass — 2 November

The user supplied seven fantasy horde-defense reference images and explicitly
approved pixel-art **survival** presentation. The active renderer now uses five
original ImageGen PNGs with measured alpha/UV/foot/muzzle metadata and nearest
sampling, Pixelify Sans, full-window biome ground/peripheral dressing, compact
corner status and visual upgrade canvases. Historical assets and licenses remain
retained. Exact prompts are embedded and in `assets/art/pixel/PROVENANCE.json`.
All sixteen repository rasters passed the prompt scan with zero missing entries.

The user's settings navigation issue is corrected with destination-specific
actions and an explicit durable main-menu choice from an active game. Regressions
exercise fresh and existing menu profiles, active reload preservation, Camp
pause restoration, and visible campaign/settings write failures followed by
retry. No failure can announce a successful return.

The additional impact requirement is authoritative six-fixed-tick movement
hit stop on positive injury, with a bounded alpha-derived silhouette flash and
frozen gait. Misses, defenses and full armor absorption do not stagger. Attack
clocks and NPC decisions retain their cadence; queue/contact states consume the
same fixed ticks. Old saves default the counter to zero; new saves validate and
retain it. Seven new core regressions cover sources, timing, persistence and
nonstacking refresh. A separate independent source reviewer found no concrete
P1/P2 in these boundaries. Corpse history stays capped and holds/fades over twenty
seconds without changing simulation outcomes.

The final full helper gate passed **223 tests**: **102 app**, **73 core unit** and
**48 simulation integration**, plus format and strict all-target Clippy, with
zero failures or ignored tests. Original behavioral assertions were preserved;
presentation-specific tests were updated to the user-approved full-window field
and contextual grid. New cases cover source RGBA hashes, alpha/fringe-safe flash
geometry/budgets, grounded/theme-safe scenery, graph layout under horizontal
parents, compact visible choices, measured one-line menu ink/action bounds and
player-facing defense explanations retaining actual numbers.

An initial independent review inspected seven references, all 46 native states
and six motion samples. Its bounded four P2 findings were addressed in one batch:
measured compact menu fitting, vertically allocated immersive upgrade canvases,
quiet forward-UV terrain with grounded biome-specific dressing, and readable NPC
defense text. Fresh final evidence is in `target/pixel-review/screenshots-final/`
(46 PNGs: 23 states at both supported sizes) and `motion-final/` (24 actual native
frames + tick manifest). All final PNG dimension/nonblank checks passed. The
silent `pixel-combat-final.mp4` encodes those frames at nominal 15 FPS; its export
uses one fixed step/native frame and is **not** a real-time FPS measurement.

DESIGN.md and its structured sidecar were refreshed from current native source;
color/token/component/path/narrative checks passed. No web detector was applied
to native Bevy/egui. Independent correction confirmation, real-time samples,
portable verification and actual Windows settings checks are recorded after
execution; no complete human campaign/listening/balance study is inferred.

The focused correction verdict was **SHIP at the exact four-item scope**: all
sixteen named normal/compact confirmation views scored resolved for the menu,
upgrade composition, terrain/scenery and NPC defense text. No broader interactive
or audio conclusion was inferred by the reviewer.

Separate active dense-renderer samples on the Intel Core Ultra 9 185H / NVIDIA
RTX 1000 Ada Laptop GPU, Vulkan, VSync, Windows scale 2, averaged **60.00 FPS** at
2880×1800/UI1.0 (p95 **17.768 ms**, p99 **18.292 ms**) and **60.00 FPS** at
2100×1400/UI1.35 (p95 **17.747 ms**, p99 **18.650 ms**). Reports use 90 warmup
frames and 300 real native intervals; each ended with 71 enemies and ten defenses.
These short application/OS samples are not minimum-spec or long-session claims.
Reports: `target/pixel-review/active-peak.json` and `active-peak-small.json`.

### Verified pixel package and actual settings return

The immutable pixel build is `target/playtest/Lastlight-playtest-20261102-160645.zip`,
**64,552,083 bytes**, SHA-256
`d8b3a489c76beca990fa34b37530fc996142e58ce8615456bd7eeccaa088291b`, with an
unpacked sibling folder. Fresh extraction passed all **70 manifest hashes/sizes**,
the copied executable's full earned-equipment campaign smoke in both modes, and
a native peak capture. Evidence is under
`target/playtest-verification/940295a10ebe4015af8c665edc2ee822/` and
`target/pixel-review/DELIVERY.json`.

Actual Windows Computer Use exercised that packaged executable with a separate
`target/pixel-review/windows-profile/` practice root. From an active defense:
Pause -> Management -> Settings -> **Save and return to main menu** reached the
actual main menu and acknowledged a saved generation. Main menu -> Settings ->
its explicitly named main-menu return also succeeded. Return to active defense
restored the still-paused German field at **tick 8666, HP 135, ammunition 2**.
The checkpoint JSON matched across the settings/menu loop. The ordinary user
profile was not used. This is direct confirmation of the reported navigation
bug's active-game and menu-origin paths, in addition to the IO-failure and
pending-action regression tests.

This narrow interaction check and the immutable package verification do not
claim a full human campaign, subjective impact feel, listening quality or
long-session balance. The archive was verified before this later interaction
record and remains byte-identical; the record refers to that same binary.

## UI overhaul — 2 November

The subsequent UI-focused overhaul replaces the flat feature navigation with Campaign, Prepare and Operations. A new briefing collects country/stage, defender, field loadout and encounter conditions before an explicit paused deployment. Preparation retains the planned mission; debriefs route to the next briefing or exact recommended improvement. Inventory and live expeditions lead their respective pages. Battle management actions are grouped, mouse combat alternatives remain direct, and settings have consistent groups. The engine-free simulation and save format were unchanged.

The full workspace helper passed formatting, strict all-target Clippy and **194 tests**: 81 app tests, 72 core unit tests and 41 simulation integrations, with zero failures or ignored tests. Existing assertions were retained while intentional UI labels/routes were updated. Added cases exercise equipment/briefing round trips, replay/mode preservation, explicit recommendation purchase, next-stage preparation without automatic deployment/payment, saved world pause, active-field return, debrief pause and popup input isolation.

The initial independent review inspected **44 native views**, all 22 states at normal and minimum/high-scale sizes. Its bounded P2 finding was a long NPC disclosure label expanding the tactical scroller. Explicit content bounds, wrapping, a fitting “Latest resolution” label and observation controls placed first corrected it. A new regression opens a real NPC resolution at both sizes and checks label/control bounds and unchanged full authoritative state. A separate source review identified premature outbreak navigation; success now opens the requested briefing through the command notice, while failure retains Operations and outbreak context. The exact active-Germany/French-outbreak regression passes.

`target/ui-redesign-review/screenshots-final/` contains **46 final actual native views**, adding expanded NPC disclosures at both sizes. Normal windows are 1440×900 logical, minimum windows 1050×700 at UI scale 1.35, with Windows scale 2.0. Read-only PNG dimension/nonblank checks passed all 46. Independent fix confirmation scored the NPC issue **resolved — ship at that scope**, including wrapped expanded details and reachable speed controls. The source reviewer separately confirmed the outbreak issue resolved. Neither reviewer claimed an OS playthrough from fixtures.

Separate automatic-time active dense-combat samples on the Intel Core Ultra 9 185H / NVIDIA RTX 1000 Ada Laptop GPU (Vulkan, VSync, Windows scale 2) averaged **60.00 FPS** at 2880×1800/UI1.0 (p95 17.380 ms, p99 17.665 ms) and **60.01 FPS** at 2100×1400/UI1.35 (p95 17.400 ms, p99 17.759 ms). Each report uses 90 warmup frames and 300 measured native intervals; final fields contained 71 enemies and ten defenses. These short real-time application/OS intervals are separate from screenshots and CPU benchmarks, and do not establish other-hardware or long-session performance. Reports are `target/ui-redesign-review/active-peak.json` and `active-peak-small.json`.

The direction contract is [UI_REDESIGN.md](UI_REDESIGN.md). DESIGN.md and `.impeccable/design.json` now agree with actual native tokens and composition; JSON/YAML/narrative/component-reference checks passed. Existing eleven painted PNGs and their two provenance manifests were preserved. No HTML/CSS detector or image mockup was used for this native, code-first redesign. Package and actual Windows interaction results for this build are recorded separately when executed.

### Verified UI package and Windows journey

The immutable UI build is `target/playtest/Lastlight-playtest-20261102-111513.zip`, **58,362,022 bytes**, SHA-256 `b345e7e7627bd244188637bb88b99b2aa6e1265d1be1f951c7e9f8830cba2b30`, with an unpacked sibling folder. Fresh extraction passed all **61 manifest hashes/sizes**, the copied executable's full campaign smoke using bundled content, and an actual native peak capture. Evidence: `target/playtest-verification/10427b3eceed46cf9d4c2a6172b2e859/` and `target/ui-redesign-review/DELIVERY.json`.

Actual Windows Computer Use then exercised the packaged normal launcher under the separate `target/ui-redesign-review/windows-profile/` root: fresh menu -> Campaign -> select France directly on the map -> starting-country commit -> French briefing -> Equipment -> Back to briefing -> explicit paused deployment -> Begin defense -> Settings -> durable Save and return to the same paused field -> Management -> save/menu -> save/exit -> relaunch -> Return to active defense. The restored checkpoint was still **FRA, stage 0, tick 1624, paused, HP 100, ammunition 12**, matching the pre-exit checkpoint. Settings/return and repeated UI navigation did not advance the paused clock.

The same real window then exercised Management withdrawal -> retained-reward debrief -> the recommended Reload drill's Skills destination -> Back to briefing. The selected French mission and You defender remained intact; the wallet stayed at zero and the world stayed paused. This is a successful narrow Windows workflow and save/resume check, not a complete human country campaign, usability study, audio listening session or balance proof. No ordinary user profile was used. The archive predates this later interaction record and remains byte-identical; this source entry records evidence for that same verified binary.

## Follow-up animation and polish evidence — 2 November

The final workspace helper passed formatting, strict all-target Clippy and **185 tests**: 72 application tests, 72 core unit tests and 41 simulation integrations, with zero failures or ignored cases. The original simulation assertions remain intact. Added cases cover tick-driven poses, bounded histories/particles, padded atlas feet, reduced-effects isolation, finite ambient mixing and mute, source-aware critical cues, UI guidance/action state, camp pause and fresh capture directories. Five final visual regressions cover single-line HUD stability, calibrated turret aiming/mounts, resolved shot-target commitment, and small-field terrain/lane geometry.

`target/animation-playtest-review/screenshots-polished/` contains **27 final actual native framebuffers**: 18 normal and nine minimum-window/high-scale views, including a genuinely fresh first defense. `motion-peak-polished/`, `motion-peak-reduced-polished/` and `motion-tropical-polished/` each contain 24 native PNGs and a tick manifest. All 24 normal/reduced peak samples have identical authoritative tick and enemy counters. This counter comparison complements the full-state no-mutation animation tests; it does not replace them.

The silent `target/animation-playtest-review/combat-animation.mp4` encodes the polished peak frames at nominal 15 FPS. These exports intentionally use one authoritative fixed step per native frame and are **visual evidence, not real-time performance measurements**. Output directories are exclusively created, so interrupted captures cannot silently supply stale frames. Earlier phase-two folders without the `polished` suffix are superseded evidence from before the final visual correction.

Separate automatic-time, VSync-enabled native measurements used 90 warmup frames and 300 observed intervals. Hardware was Intel Core Ultra 9 185H and NVIDIA RTX 1000 Ada Generation Laptop GPU with Vulkan, development profile, Windows scale 2.0. Samples ran separately from this pass's build and screenshot batches.

| Workload | Physical pixels / UI scale | Mean FPS | p95 frame | p99 frame | Final enemies / defenses |
|---|---|---:|---:|---:|---:|
| Active dense combat, animations and atmosphere | 2880 × 1800 / 1.0 | 60.01 | 17.275 ms | 17.815 ms | 70 / 10 |
| Active dense combat, minimum window | 2100 × 1400 / 1.35 | 60.01 | 17.205 ms | 17.561 ms | 71 / 10 |
| Paused dense renderer | 2880 × 1800 / 1.0 | 60.00 | 17.245 ms | 17.565 ms | 73 / 11 |

The active fixture starts at 73 enemies and 11 defenses and resolves real combat; counts are not artificially held constant. Final reports are `active-peak-polished.json`, `active-peak-small-polished.json` and `paused-peak-polished.json` in the same review directory. These short measured native application intervals include VSync and OS scheduling; they are neither GPU timestamps nor support claims for other hardware or long campaigns.

All **11 painted PNGs** have embedded exact prompts; the scan found zero missing entries. Read-only RGBA comparisons confirmed all five animation strips retain their exact generated pixels after metadata embedding. All **18 WAVs** pass mono 16-bit, 22,050 Hz PCM and individual no-clipping checks; the original nine remain unchanged. Five environment loops pass the documented boundary-impulse check. Asset and waveform checks do not establish audible mix quality.

The fresh independent architecture review confirmed read-only presentation, bounded caches/voices and explicit scheduling. Its two concrete findings were fixed and regression-tested: trap shots can no longer fabricate turret fire, and animation exports reject stale output directories. See [REVIEW_ANIMATION.md](REVIEW_ANIMATION.md), [REVIEW_ATMOSPHERE.md](REVIEW_ATMOSPHERE.md) and [REVIEW_UX.md](REVIEW_UX.md). Native interaction, listening, human comprehension and full-session balance retain their separate evidence boundaries.

A fresh independent visual review inspected the phase-two flow and motion, then confirmed its bounded three-item correction: ellipsized NPC HUD text retains full detail; calibrated turret barrels and muzzle-origin traces align at steep angles with fixed tripods; quieter terrain and continuous seams expose small targets and all eleven approach lanes. Its final scoped verdict was **ship**, with all three P2 findings resolved. This was source/framebuffer confirmation, not an OS-window playthrough or listening session.

## Initial comprehensive build and test gate — 1 November

From `horde-defense/`:

```powershell
pwsh -NoProfile -File ../scripts/dev.ps1 verify -Project horde-defense
cargo build --locked -p horde-defense
./target/debug/horde-defense.exe --smoke
```

The workspace helper **passed** formatting, all-target Clippy with warnings denied, and **148 tests**: 72 core unit tests, 41 simulation integrations and 35 native application tests. There were zero failures or ignored tests. The native build and executable smoke also **passed**.

The application cases exercise actual egui pointer/key events and the named Bevy fixed schedule. They include country selection and a saved deployment, pause/restart, remapping and persistence, quick fire/melee taps across render/fixed boundaries, press-time aiming, cadence conservation, outer-lane mouse-only melee, body allocation/refund, sale confirmation and preset warnings, a 1,004-item virtualized armory, isolated practice profiles, corrupt-generation preservation, layout-pass transaction deduplication, durable quit, mode speed equivalence, atlas/map geometry and NPC panel bounds at both supported sizes. They are not an OS-window playthrough claim.

The original 75 core cases and three-seed, five-gate progression proof remain asserted. New integrations cover relative boss timing, warning/reinforcement telegraphs, traceable contact, monotonic queues, visible-information targeting, object durability, real turret skill, maneuvers and Strength-derived melee, exact-once terminal ordering, invalid state rejection, inventory settlement and checkpoint boundaries.

## Campaign smoke

The executable validates 242 region records and five encounter templates, restores an identical active save, and completes both five-template campaigns using earned currency, ordinary market gear and legal training. Optional drops are not required for the progression proof.

| Template | Personal clear | NPC clear |
|---|---:|---:|
| 1 | 188.8 s, first attempt | 190 s, second attempt |
| 2 | 182.9 s, first attempt | 197 s, first attempt |
| 3 | 214.6 s, first attempt | 223 s, first attempt |
| 4 | 214.9 s, first attempt | 217 s, first attempt |
| 5 | 245.9 s, first attempt | 256 s, first attempt |

The initial neutral-template NPC defeat banked 181 coins after 208 seconds. A separately selected Germany starter deployment banked 214 coins before defeat at 217 seconds. These are different seeded/country contexts, not duplicate reward transactions. Subsequent preparation in the neutral campaign spent actual earnings. Regression seeds `1`, `2026` and `0x1A571167` repeat the full progression constraint; this is not an exhaustive balance study.

## Native presentation

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/capture_review.ps1
```

Actual Bevy framebuffers are generated under `target/playtest-review/screenshots/` (ignored review artifacts). The 25-view matrix covers menu, map, personal/NPC combat, boss, construction, character, survivor, inventory, skills, engineering, results, operations, settings, arid/tropical regions and peak pressure, plus minimum-window variants. Normal resolution is 1440 by 900 logical pixels; minimum is 1050 by 700 with UI scale 1.35. Windows pixel density on this machine is 2x. Fixtures are labeled and isolated; their supplied resources are not balance evidence. Original prototype captures in `docs/screenshots/` remain historical evidence rather than regression oracles.

A fresh visual reviewer inspected the full flow and identified one required fix: NPC policy controls expanded under the arena. The recall label/slider are now stacked and bounded, observation controls wrap and the sidebar scrolls independently. A strict geometry regression and normal/minimum NPC/peak recaptures verified separation and all eleven visible lanes. The follow-up verdict was **ship for native playtesting**, at the scope of that review. See [REVIEW_INDEPENDENT.md](REVIEW_INDEPENDENT.md).

## Native frame measurements

Optional `LASTLIGHT_PERF_REPORT` writes a bounded frame report: 90 warmup frames followed by 300 observed intervals. The tested scene is explicitly **paused peak rendering**, with 73 enemies and 11 affordable defenses, not active-battle throughput. VSync and OS scheduling are included; these are not GPU timestamp measurements.

Hardware: Intel Core Ultra 9 185H; selected NVIDIA RTX 1000 Ada Generation Laptop GPU, Vulkan backend. Development profile uses application optimization level 1 and dependency level 2.

| View | Physical pixels / UI scale | Mean FPS | Median frame | p95 | p99 |
|---|---|---:|---:|---:|---:|
| Desktop | 2880 by 1800 / 1.0 | 60.00 | 16.69 ms | 17.24 ms | 17.50 ms |
| Minimum | 2100 by 1400 / 1.35 | 60.01 | 16.66 ms | 17.15 ms | 17.41 ms |

Reports: `target/playtest-review/render-peak-1440.json` and `render-peak-small.json`. They record viewport, adapter, sample count, entity counts and the paused state. No minimum-spec guarantee or full active-battle FPS result is inferred from them.

## Simulation CPU measurement

```powershell
cargo run --locked -p horde_core --example peak_benchmark --target-dir target/core-check -- --output target/peak-benchmark.json
```

The benchmark used 600 warmup steps and 12,000 timed complete authoritative steps per case. Peak population replenishment is included; fixture creation and terminal reset clones are excluded. This is a stress fixture, not earned progression or survival evidence.

| Case | Peak actors / structures | Mean tick | p99 tick |
|---|---:|---:|---:|
| Personal | 73 / 10 | 6.17 us | 10.1 us |
| NPC | 73 / 10 | 6.17 us | 25.9 us |
| Four parallel fields | 292 / 40 | 23.63 us | 83.0 us |

The CPU report is `target/peak-benchmark.json`, on the same Intel processor. Rendering, audio and UI are excluded, so these numbers are not frame rates.

## Assets, data and storage

All six painted PNGs have exact prompt provenance embedded and recorded in `assets/art/PROVENANCE.json`; the metadata scan reported zero missing entries. Atlas regions are bounds/aspect checked. All nine original WAVs passed mono 16-bit PCM, 22,050 Hz, nonempty-sample and no-clipping checks. The original six WAVs remained byte-identical after adding three cues. Neither file checks nor screenshots establish listening/mix quality.

Country records retain source years (mainly 2019), attribution, symmetric travel links, actual boundaries and validated connectivity. Content validation rejects unsupported/missing profiles, bad ordering, nonfinite/overflow values and unsafe geometry. Culture/architecture is original regional dressing, not bespoke research or art for every country.

Save generations retain RNG, actions, equipment reservations, pending boss mechanics, workers and settlement cursors coherently. Corrupt primary/backup bytes are preserved before replacement; failed writes never acknowledge quit. The save format (envelope v7, frozen by `save_v7.json`; the limit is version-independent) has an inclusive **64 MiB encoded-generation ceiling**. Oversized saves fail before touching any existing generation or temporary file. Inventory has no gameplay capacity limit and collected items are never discarded, but a future storage format is needed for persistence beyond that technical ceiling.

## Remaining playtest questions

- Human understanding of shared outer-lane engagement and visible melee eligibility.
- Aiming feel, audio discrimination/mix, accessibility perception and window focus behavior.
- Clear rates, earning rates, purchase pacing and strategic pressure across all countries, policies, builds and long sessions.
- Active-battle frame distributions and other machines; the measured paused render result is not a minimum-spec claim.
- Cross-release content migration and compatibility with arbitrary edited catalogues. Overrides load once at startup; incompatible data is rejected rather than silently merged.
- Only the documented Fourth Edition Lite subset and scenario adapters are implemented. The reference PDF does not establish a digital-game distribution license.

The [playtest guide](PLAYTEST_GUIDE.md) provides ordinary and isolated practice routes. Package and native interaction checks are recorded below when executed.

## Verified local playtest package

`scripts/package_playtest.ps1` created `target/playtest/Lastlight-playtest-20261101-225239.zip` and its unpacked folder. ZIP SHA-256: `f6056ba47d91285c453c10c6b203000ec112a6ea1a7d06b8acea2296613fa3f6`.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify_playtest.ps1 -Package target/playtest/Lastlight-playtest-20261101-225239.zip -NativeCapture
```

Verification **passed** all 41 manifest hashes and sizes from a fresh extraction, rejected unexpected/path-escaping entries by construction, ran the copied executable's full campaign smoke using the bundled editable content and assets, and captured its actual peak-render framebuffer. Evidence is under `target/playtest-verification/5da55c154535430498dd52e583e8089f/`. The portable copy is a Windows x64 development-profile playtest, not a claim of compatibility with every PC. No ordinary campaign was used for verification.

Manual Windows input and listening remain separate from these successful package and framebuffer checks.

## Final follow-up package and actual Windows interaction

The unchanged final executable and assets are available in `target/playtest/Lastlight-playtest-20261102-010403.zip` (58,322,661 bytes), with an unpacked sibling folder and normal/battle-practice/strategy-practice launchers. SHA-256: `f63e1d1814b0a1e0c5fe37c42216193df477985c7469fa2563fdc2b66b959c04`. Fresh extraction verification passed all **60 manifest hashes/sizes**, the copied executable's full campaign smoke using bundled content, and an actual peak framebuffer. Verification evidence is under `target/playtest-verification/ba9751ed23844b3d84fa0da3413cec60/`; `target/animation-playtest-review/DELIVERY.json` records the release.

The packaged **Battle practice.cmd** was then exercised through actual Windows Computer Use with a separate `target/animation-playtest-review/manual-profile/` save root. Window capture and input succeeded. Starting the paused defense advanced the actual battle, mouse firing consumed ammunition from 8 to 7, keyboard R restored the magazine to 8, and the pause button froze the displayed timer at 126 seconds. Save deployment acknowledged a committed generation; save/return/menu exit closed the app. Relaunching the same packaged launcher and continuing restored paused tick **7531**, HP **135**, ammunition **8**, **38** kills and **127** earned attempt coins, matching the pre-exit deployment. F12 generated a real game screenshot under that isolated root.

This is a narrow successful OS-window interaction/save-resume check, in addition to widget and simulation regressions. It does not establish a complete manual campaign, aiming skill/feel, speaker/headphone mix or long-session balance. No ordinary campaign was used. The immutable ZIP was verified before this interaction session; this source entry adds the later evidence for that same packaged binary without altering the archive or its manifest.


## Survivor, live-recovery and continuing-progression increment — 2 November 2026

This entry records the current increment. Earlier dated sections remain historical evidence for their named builds; their six-independent-skill, paused-deployment, five-active-PNG and capped/fading-corpse descriptions are superseded by the current source and user overrides in EXPANSION.md. Their old package hashes and Windows journeys do not identify this build.

The current flow confirms Rook or June before the first country, then starts live personal combat. The two identities have equal initial attributes/pistol; Mara remains the NPC companion. The selected appearance/current weapon is layered over keyart-v2 in the animated menu, and reduced effects uses a static pose. Live construction, country-specific plans, retained rewards, retreat-to-training and live retry remain separate from explicit pause. The shipped DAG has 32 nodes/four branches; class training, personal weapon acquisition and NPC-issued gear have distinct eligibility. Saved corpses persist for the attempt and settle into batched rendering. Ground-cell ordnance uses saved finite charges, flight and burning state.

Campaign threat counts new secured-defense high-water marks, not attempts or XP, and freezes a resolved snapshot per encounter. It governs bounded groups, population, pressure, rewards, tactical equipment and new gear levels beyond the rotating five templates. Owned items retain actual saved acquisition levels and effective comparisons/prices. The numerical, compatibility and authoring contract is [PROGRESSION_SCALING.md](PROGRESSION_SCALING.md). The staged maximum fixture has rank 50, groups of four and 112 ordinary enemies plus one boss, all in view; its funded progression and equipment are explicitly synthetic.

The final workspace gate passed formatting, strict all-target Clippy and **288 tests: 123 application and 165 core**, with zero failures and **one intentionally ignored audio asset-export test**. Ordinary tests do not rewrite the audio library. Evidence: `target/increment-review/release-workspace-gate.log`. The final native development build also passed. Existing behavioral assertions remain intact; the last save-validation fix separately rejects ordinary hostiles above the frozen cap while preserving the reserved boss slot, with valid, invalid and checkpoint cases. The independent source audit's concrete population finding is therefore covered by the final regression/gate, rather than being waived.

The refreshed earned-reward country proof covers DEU/BRA/EGY/USA/ISL at seeds `1`, `2026` and `0x1A571167`: two failed personal attempts retain earnings, then explicit shop/skill purchases and a Compact SMG clear the third attempt without equipping loot. This is a deterministic controller reading visible enemies, not a manual retreat or human aiming study. The five-template campaign proof still passes in both modes/three seeds. One neutral NPC starter case at seed `0x1A571167` takes six attempts, so a universal 3–5-run result is not claimed. The separate actual USA eight-defense route clears the first stage on personal attempt two/NPC attempt four, then stages two through eight on their first attempts; a level-four gun is equipped by stage seven. Stage-eight clear times are 252.9 seconds personal and 245.0 seconds NPC. Logs: `target/core-country-proof.log`, `target/core-campaign-proof.log` and `target/world-progression-proof.log`. These establish scoped viability beyond template five, not every country's balance or all player/policy/build choices.

`target/increment-review/screenshots-final/` contains **64 native static views** (32 states at both supported sizes). The independent native review confirmed the prior compact-graph readability, prerequisite routing, bounded attack selector and Ordnance guidance corrections, and found the new identity/tree presentation readable at both sizes. Its sole additional P2 was an oversized menu pistol. The presentation scale changed from 0.44 to 0.22; all **13 visual-asset regressions** and strict application Clippy passed afterward. `screenshots-confirmed/` contains the eight affected normal/compact recaptures. The original 64-view batch and these replacement views are distinct evidence; the affected replacements take precedence. The independent reviewer inspected all eight normal/compact replacements and confirmed the pistol sits correctly for both characters. The final scoped static verdict is **SHIP: all five findings closed**, with no further static audit requested. This was framebuffer/source confirmation, not an OS-window playthrough.

DESIGN.md was merged with the incumbent pixel-survival identity. Its entire normative frontmatter remains byte-identical (SHA-256 `c6c26f20a71a91ccfbad318e9805a1241f3665559809d5d6f62a12fc436811d6`); all 17 color primitives were checked against native source. YAML schema/references, the eight canonical headings, JSON native metadata/component references/source symbols/catalog paths and narrative agreement passed. No HTML/CSS rendering audit was used for this native application. The [View hierarchy](VIEW_HIERARCHY.md) now contains **56 Implemented Views**, including survivor selection and the next-clear preview: unique IDs, one root, valid parents, no cycles, complete reachability, declared source files/symbols and local links all passed documentation checks. Implemented is a source inventory status, not an OS interaction claim.

Six nearest-sampled pixel textures are active; seven pixel PNGs remain in provenance, including the replaced keyart-v1. The new survival library retains 24 sound families with three original variations each (72 WAVs), six concurrent effects and at most two new cues per update. Its synthesis/waveform/scheduling evidence is separate from device listening; no human listening or audible-mix conclusion is claimed.

All four final native motion jobs completed successfully, each with **24 PNGs, a manifest and preview.mp4**, under `target/increment-review/`: `motion-menu-final`, `motion-menu-reduced-final`, `motion-combat-final` and `motion-ordnance-final`. The ordinary menu has 24 distinct frames and null simulation ticks; all 24 reduced-menu frames are identical and also have null ticks. Combat advances from tick 14458 to 14550 in the staged maximum field; ordnance advances from 2383 to 2475. The independent scoped motion verdict is **SHIP, no P1/P2 findings**. These stepped exports show motion/state correspondence; their encoded playback is not a real-time frame-rate measurement or a human aiming/impact-feel study.

Separate active real-time measurements in `target/increment-review/performance-final/` use **90 warmup frames and 300 measured native intervals per case**, with 113 hostiles, 14 structures and `paused=false`. Hardware is Intel Core Ultra 9 185H / NVIDIA RTX 1000 Ada Laptop GPU, Vulkan, development profile. VSync and OS scheduling are included; these are observed application/OS intervals, not GPU timestamps.

| Staged maximum workload | Physical pixels / UI scale | Mean FPS | p95 frame | p99 frame |
| --- | --- | ---: | ---: | ---: |
| Normal window | 2880 × 1800 / 1.0 | 57.8696 | 17.4594 ms | 43.2642 ms |
| Minimum window | 2100 × 1400 / 1.35 | 60.0125 | 17.6339 ms | 18.2584 ms |

The normal-window sample includes a visible tail hitch at p99; it is not a locked-60 result. This is a short measurement of one staged maximum scene on one device, not minimum-spec support, long-session performance or proof that maximum progression was earned.

All 72 new survival WAV headers, provenance hashes and signal levels were checked. No human listening session or final-device audible-mix result was performed.

**Final-build Windows OS interaction remains unperformed in this source record.** An older candidate activated once after the user foregrounded it, then activation was unreliable; its foundation-flow work does not validate the current survivor/progression build. Archive creation and copied-package verification remain separate from these completed prebundle checks. All source documentation is now frozen before the immutable final archive. Its hash, copied executable/native verification and any later OS interaction evidence belong only in the external `target/increment-review/DELIVERY.json` record and delivery report. No complete human campaign or universal progression guarantee is inferred.


## Quality, critical hostiles and victory-scaled arenas — 3 November 2026

This is the latest frozen source record for the pass spanning 2–3 November. Earlier entries, their 113-actor performance results, static/motion sets and archives remain historical. The pre-quality archive is provisional and must not be presented as this final build. Unique-clear-only threat, fixed-eleven arenas, a total threat cap at fifty and the earlier foreground-right menu placement are superseded.

The source now has six named quality tiers independent of item level and definition unlock tier. Effective stats/value use the rolled/base quality ratio after level scaling; None or explicit base quality retains old numerical identity. Quality rolls use a separate keyed counter and consume no additional combat RNG. Model receipts acknowledge loot already inserted into inventory; a quality-outlined class icon/name rises from actual death position for 1.5 real UI seconds, with at most twelve active flyups per stream, never another award. The user explicitly retained GURPS injury rules: conscious zero/negative-HP NPC enemies can fight and are labeled CRIT · FIGHTING with signed inspection. Actual terminals stop/reap/pay in their resolving fixed tick. Saved within-lane overtaking displacement drives rendering, targeting, AoE, corpses and loot; walls/contact still block and traps cover the full lane.

Every committed victory—including replay, outbreak and parallel/worker success—raises future threat once and grants up to 120 once-only Personal completion XP in either mode; NPC kills still grant no Personal kill XP. Character MAX is 50. New deployments unlock 11/13/15/17 lanes at levels 1/11/21/31, while active difficulty/geometry and old eleven-lane checkpoints stay frozen. Threat 50 is a growth knee, not the health/damage ceiling; the explicit numerical support guard is 10,000. Independent population bounds allow 224 ordinary actors plus one boss; item level 27 is available at victory 52 and cap 30 at victory 58. The 225-actor/17-lane maximum scene is staged, not earned progression.

The final root gate `target/increment-review/ship-gate.log` passed **324 tests (184 core, 140 application), zero failures and one intentionally ignored offline sound exporter**, plus workspace formatting and strict all-target Clippy. The native build passed. The independent rules/source audit found no actionable issue in quality, overtaking, critical/terminal state, victory/XP/parallel settlement, persistence or frozen projection. Diagnostic loot replay tests verify cache-only behavior without Game coins/items/RNG mutation. The opening 15 country/seed purchase routes still clear on attempt three without equipping loot. The refreshed earned USA eight-stage route clears its first defense on personal attempt two/NPC attempt five, then later stages on their first attempts; final clear times are 253.1 and 284.0 seconds respectively. These are scoped deterministic proofs, not universal player or country balance.

**Native static and motion reviews: SHIP, no P1/P2 findings.** `target/increment-review/wrap-screens-ready` contains 72 completed native PNGs: 36 scenes at 1440×900 logical/UI1.0 and 1050×700/UI1.35. The review covers named quality versus Lv/unlock tier, upward banked loot, original menu cabin placement, clean six-atlas alpha, critical state, overtaking and all seventeen compact rows. All six final motion jobs contain 24 PNGs, manifests and MP4 previews:

| Motion directory under target/increment-review | Frames | Unique PNGs | First → last simulation tick |
| --- | ---: | ---: | --- |
| wrap-menu-motion | 24 | 24 | null / null |
| wrap-menu-reduced | 24 | 1 | null / null |
| wrap-combat-motion | 24 | 24 | 14458 → 14550 |
| wrap-critical-motion | 24 | 24 | 2512 → 2880 |
| wrap-ordnance-motion | 24 | 24 | 2383 → 2475 |
| wrap-loot-motion | 24 | 24 | 2338 → 2430 |

Menu ticks are null; reduced menu frames are identical. Critical observation is 4×, while ordinary combat uses the staged 225-actor/17-lane scene. Stepped exports and encoded playback are visual evidence, not real-time FPS or a human control-feel study.

All six atlas uploads now use cached managed `ColorImage.from_rgba_unmultiplied` with NEAREST filtering; original source pixels remain unchanged. The active RenderDevice sets both egui texture budgets, fixing the 2172-wide terrain startup rejection from the default 2048 budget; oversize rejection/fallback is covered separately. Seven pixel hashes passed; all eighteen repository rasters retain prompts with zero missing. No image regeneration/pixel edit was used to correct alpha. The menu hero uses source foot anchor (0.400,0.807), at 21% source-image height.

The full 72-view run exited successfully. One additional compact loot replacement later failed with Vulkan DeviceLost (`wrap-loot-confirmed.log`); the single retry `wrap-loot-retry` exited zero and wrote its PNG. The failure is retained as a separate transient renderer/context result, not silently discarded or attributed to an unproven cause. Process-local DirectX 12 fallback passed three native captures—menu, maximum scene and compact quality loot—with Dx12 confirmed in adapter logs. The optional `Play Lastlight (DirectX12).cmd` launcher is packaged; ordinary automatic backend selection is unchanged. This is native fallback evidence, not a performance fix claim.

**Current foreground performance is unverified and the raw measurements are poor.** The Vulkan `wrap-performance` and DirectX 12 `dx12-performance` reports retain 90 warmup frames and 300 observed intervals per case on Intel Core Ultra 9 185H / NVIDIA RTX 1000 Ada Laptop GPU. Reports contain 225 hostiles, seventeen lanes and paused=false; VSync and OS scheduling are included.

| Backend | Physical pixels / UI scale | Raw observed FPS | p95 frame | p99 frame |
| --- | --- | ---: | ---: | ---: |
| Vulkan | 2880 × 1800 / 1.0 | 3.74869 | 265.6605 ms | 767.6677 ms |
| Vulkan | 2100 × 1400 / 1.35 | 3.85728 | 267.6797 ms | 269.6439 ms |
| DirectX 12 | 2880 × 1800 / 1.0 | 7.30867 | 260.6470 ms | 263.5668 ms |
| DirectX 12 | 2100 × 1400 / 1.35 | 6.30406 | 264.6651 ms | 265.7097 ms |

Sky captured wallpaper and activation failed to report the foreground process ID. Low process CPU time suggests possible background throttling, but that cause is unproven. These numbers neither establish active-window performance nor support a 60-FPS, minimum-spec or long-session claim. Earlier 113-actor measurements are historical and must not substitute for the current 225-actor result.

All 72 new WAV provenance hashes, headers and signal levels passed. **No human listening or current final-build OS journey is verified.** Final archive creation/copy integrity/native verification and any later OS result belong only in external `target/increment-review/DELIVERY.json` and the delivery report after source-doc freeze. No post-archive source amendment cycle is intended.

DESIGN preserves the original seventeen color values and legitimately extends its normative palette with six quality roles extracted through the actual native byte-rounding helper. YAML/JSON/native references and narrative, source-backed View graph and local-link checks are recorded with the final documentation handoff. Core-owned [ITEM_QUALITY.md](ITEM_QUALITY.md), [OVERTAKING_AND_CRITICAL.md](OVERTAKING_AND_CRITICAL.md) and [PROGRESSION_SCALING.md](PROGRESSION_SCALING.md) remain the detailed authoring contracts.


## Short opening, gameplay balance and run analysis — 2–3 November 2026

This is the latest prebundle record for all current requests. Earlier 329/324/288 gates, fixed-eleven fresh defaults, capture sets and archives—including the 230624 build—remain earlier revisions, not this delivery. Current source guidance is DESIGN, the 62-View inventory and [BALANCE_AND_RUN_STATS.md](BALANCE_AND_RUN_STATS.md).

Normal now starts at **3 lanes / 12 yards**, with 1.25× authored movement and 1.25× defender health. Every three character levels adds two lanes/two yards, capped at 17/30. The clearing grew physically from the small opening at that revision; since the production pass (W1-43 to W1-46) the battle camera instead zooms the clearing uniformly to fit the arena area, keeping the lane:yard proportion, and a field that already fills an axis is unchanged (see ARCHITECTURE.md, `art/camera.rs`). Model input/collision remain its actual projected bounds while a separate bounded effects region permits readable feedback. Validated Easy/Normal/Hard/Custom drafts commit only through Apply to next run. Active main/worker settings and geometry remain copied; old active 11/30 saves keep their dimensions and multipliers. Every-victory threat, character MAX 50, completion XP, quality, overtaking and the user-retained GURPS critical-state rules remain separate and intact.

Actual DPS is positive post-protection resolved injury over the last 300 simulation ticks, divided by five, with a zero-padded opening, run average/peak and defender/structure/ordnance totals. It includes logged overkill; missed/absorbed/defended attacks and presentation add nothing. Pause/save/4× retain the model clock. Saved run history retains every newly settled victory/defeat/withdrawal, with no record-count truncation; parent-owned worker outboxes settle once without cloning the parent's ledger. Records capture applied context/gear/balance/rewards/XP/damage/DPS, and old finalized runs are not fabricated. The 64MiB generation ceiling stays explicit. History filters/details/trends are read-only, with a last forty display window rather than deletion, and restore prior pause on Back.

**Final root verification passed 348 tests: 191 core and 157 application, zero failures and one intentionally ignored offline audio exporter**, plus workspace formatting and strict all-target Clippy. Native build passed. Evidence: `target/increment-review/geometry-confirmed-gate.log`. Core source review found no actionable balance/width/DPS/history/worker-settlement issue. Seven new model regressions cover settings snapshots/presets, actual source damage and fixed-window aggregate validation, pause/save behavior, legacy geometry,302 retained all-outcome records and once-only parent settlement.

Across the selected DEU/BRA/EGY/USA/ISL cases and seeds 1/2026/0x1A571167, every fresh Normal opening failed while banking 282–464 coins. Fourteen earned-purchase routes cleared on attempt three; one on attempt four, with no rare-drop dependency. The current USA eight-defense route clears the first stage on attempt three in both modes, with later stages first-try; both-mode five-template proofs remain covered. These are scoped deterministic controller results, not guaranteed human clear rates or proof of every Custom profile. Logs include `target/balance-stats-proof.log`, `target/core-country-proof.log`, `target/world-progression-proof.log` and `target/core-campaign-proof.log`.

**Final static review: SHIP.** The complete `target/increment-review/geometry-stats-screens` matrix contains 82 native DX12 PNGs—41 scenes at both supported sizes, 1440×900 logical/UI1.0 and 1050×700/UI1.35—and exited zero. Its only two P2 findings were unclear short-clearing perimeter dressing and a missing history-caption arrow glyph. The bounded correction anchors full sprite bounds near actual projected edges and uses ASCII “oldest to newest”. Eight affected normal/compact replacements in `geometry-confirmed-screens` exited zero; scoped independent confirmation closed both findings with no other static audit requested. Replacement views take precedence for those affected states; they are not eight additional unique screens. Cohesive menu anatomy/anchor, reticle/reload, impact/DPS, settings/history and prior upgrade fixes were accepted at that review scope.

All eight final native motion jobs completed with **24 PNGs, a manifest and MP4 each**. Independent review sampled frames 0/8/16/23 across every set and returned **SHIP, no P1/P2**:

| Directory under target/increment-review | Arena / context | First → last simulation tick |
| --- | --- | --- |
| final-menu-motion | Cohesive chosen menu survivor | null / null |
| final-menu-reduced | Exact static reduced scene | null / null |
| final-opening-motion | 3 lanes / 12 yards | 58 → 150 |
| final-reticle-motion | 7 lanes / 16 yards | 2412 → 2504 |
| final-damage-motion | 7 lanes / 16 yards,1× | 2339 → 2431 |
| final-damage4-motion | 7 lanes / 16 yards,4× | 2512 → 2880 |
| final-max-motion | Staged maximum,17 lanes /30 yards | 14458 → 14550 |
| final-loot-motion | 7 lanes / 16 yards | 2338 → 2430 |

All animated sets have 24 distinct PNGs; the reduced menu has one unique PNG across 24 frames. The review confirmed cohesive hands/planted boots, static reduction, reload-to-full-ammo transitions, readable 1×/4× impact feedback, loot rise/fade and clear small/maximum bounds. These stepped exports show presentation/state correspondence, not real-time FPS, control feel or an OS cursor journey. The maximum fixture remains staged rather than earned.

The presentation uses **7 active managed textures,8 pixel provenance entries and19 retained repository PNGs**. The new cohesive low-ready menu sheet leaves the calibrated battle rig separate, at source-image foot (0.400,0.807), 21% source height. Original source files remain retained; all eight pixel SHA-256 values match provenance. Native cursor visibility is an explicit after-egui-output CursorOptions policy, with standard pointer restoration outside focused field ownership; its resource badge clamps separately from true aim. Larger resolved impact numbers use the bounded real-time cache, without another attack, injury, reward or combat-RNG draw.

Current source-document checks passed: the original seventeen plus six actual quality color values, canonical YAML/schema/references, native JSON/source/component/narrative agreement, **62 Implemented Views**, unique IDs/root/parents/reachability/source symbols, current local links/UTF-8, actual Normal growth metadata and asset catalogue/hashes. Core-owned detailed defaults were preserved; the approved overtaking-guide range/encoding correction was narrow.

**Foreground performance, a final-build Windows input journey and human listening remain unverified.** Earlier poor Vulkan/DX12 raw 225-actor reports and the uncertain background/wallpaper context remain recorded above; they are not replaced by a 60-FPS or minimum-spec claim. No fresh foreground-performance result is inferred from static or stepped motion exports. Audio remains the existing 72-variation library with separate header/hash/level evidence, not a listening study.

All source documentation is frozen before the immutable archive. Its final hash, copied executable smoke/native verification and any later OS evidence belong only in external `target/increment-review/DELIVERY.json` and the delivery report. No post-archive source amendment cycle or current package-verification claim is made here.


## Unified journey and source-owned player model — source checkpoint, 3 November 2026

This is a current **source-verification checkpoint**, not final visual acceptance or a documentation/package freeze. The earlier 348/389 gates and archives above describe earlier revisions. No old ZIP is the deliverable for this source-model increment.

The current root verification log `target/journey-review/source-model-verified-workspace-gate.log` records **430 passed tests: 201 core and 229 application, zero failures and one intentionally ignored offline audio exporter**. The root verify helper also passed workspace formatting and strict all-target Clippy with warnings denied. The native build passed in `target/journey-review/source-model-confirm-build.log`. Documentation work ran no Cargo builder and performed no GUI interaction.

The source implements the protected New game → survivor → country → live main round → Journey upgrades route, a connected 169-node DAG with seven focus paths, explicit continuation/leave, conserved legacy CP payment and atomic rejection of an orphaning refund. NPC advancement and owned-item acquisition remain independent. The single storage worker accepts immutable snapshots with one in-flight/latest pending, matching durable acknowledgments and an epoch barrier for archive/reset. Neither these interfaces nor the test gate establish zero stutter or universal performance.

Production battle and equipped previews now share the source-owned player_model appearance/pose facade. Original RGBA remains unchanged: `menu-survivors-v1.png` SHA256 `4feb21b05c1ad5d281d4f0134fdfe546356ca71a204dba04b7c81c48bd356a3a`. The initially accepted native source bind in `target/journey-review/source-model-native-1/` had 22 Rook / 23 June parts and zero differing pixels/bytes on both black and white. The current source has 23/24 after the approved wrist-owner split. It retains all original ownership; hidden surfaces, alternative garments and active hand replacements are explicitly authored. Initial still parity does not establish current posed appearance, integration or acceptance of new clothes.

The first action/equipment native batch exposed hidden-fill/occlusion/grasp defects; these are undergoing the bounded confirmation in `target/journey-review/source-model-confirmation/` and actual-game `source-model-game-confirmation/`. **Current posed visual approval is pending.** The new saved projectile release cues preserve the release tick, shot/hit pulses and brace fact even when the brace ends or event history expires before an airborne save/resume. Regression coverage includes the ended-brace case, old None behavior and NPC serialization. Visual sockets remain presentation only and never award a hit/reward or consume combat RNG.

Source documentation preserves the original seventeen color values and six quality roles, Pixelify Sans and the approved pixel-survival identity. It records **69 Views**, preserving 64 existing IDs and adding the isolated developer review shell plus parity/parts/matrix/repairs tasks. `python scripts/check_views.py` passed 69 with unique IDs, one root, valid parents, no cycles and existing sources/symbols; current local Markdown links, native sidecar source paths, canonical DESIGN headings and all 23 normative color values also passed. Detailed model authoring/validation documents retain their own ownership and dated acceptance scope.

**Still pending:** current corrected native/model and motion verdicts, final renderer NPC timing/equality, current CLI/package smoke and immutable archive/copied-native verification. Earlier controlled async-IO timing concerns different renderer revisions; earlier 225-actor poor-context reports remain historical rather than substituted as current proof. No current final-build OS journey, human listening, minimum-spec or zero-stutter claim is made. Root will supply final current evidence; immutable archive/copy and any later OS records belong in its external delivery record rather than an unverified source claim.


## Source-model static confirmation and focused NPC timing — 3 November 2026

This updates the source checkpoint above with completed native/static and measured performance evidence. The current production source gate remains **430 passed / 0 failed / 1 ignored (201 core, 229 application)** with formatting, strict Clippy and native build. Additional isolated gesture-QA cases are in progress; a later gate count is not inferred. Final archive/copied-package verification remains pending and no older ZIP is this delivery.

**Static confirmation accepted by Root and independent review:** eight model PNGs in `target/journey-review/source-model-confirmation/` plus six actual-game/preview PNGs in `source-model-game-confirmation/`. These resolve the hidden-fill ghost, repeated patch grid, raised-pistol occlusion and connected closed melee grasp/wrist bridge. The two model matrices cover 68 supported cases with zero unsupported cases. The current original bind remains exact on black and white: zero differing pixels, bytes and maximum channel difference, including independent original opaque RGB checks of 952 Rook / 567 June pixels per background. Original source SHA256 remains `4feb21b05c1ad5d281d4f0134fdfe546356ca71a204dba04b7c81c48bd356a3a`; the current 23/24 ownership split does not edit its RGBA. Newly revealed/alternate surfaces remain disclosed authored repairs, not recovered invisible pixels. Near-vertical rifles and the limited head pitch are accepted stylized 2D limitations, not a claim of fully natural 3D anatomy or pixel-identical changed poses/new garments.

The source CLI smoke passed in `target/journey-review/source-model-source-smoke.log`: authored content and atomic resume, fresh companion gate rejection without partial mutation, an earned-shop Personal first clear on attempt three and subsequent five-template stages, plus a separate explicitly legacy-compatible NPC route. The NPC compatibility route must not be described as freshly earned companion access. Asset checks passed 15 pixel-provenance hashes and a 26-PNG prompt scan; the original source files remain retained. The 69-View validator passed.

**Measured current-renderer checkpoint comparison:** `target/journey-review/source-model-worker-comparison.json` records the same copied input SHA256 `3e0be1d7222bd73fea20bc91c35c752176eeb402ada925a640ffae7a0fe987e8`. Both traces use the final source-owned models, ordinary scheduler, Vulkan and audio, with every measured frame focused. Full decoded Game state is exactly identical between 1x/4x and the prior worker result, including random state, outcomes, rewards and history. The remaining NPC round ends at 1772 simulation ticks before the requested 1800-frame target; neither trace is falsely marked target-complete.

| Current renderer trace | Measured focused frames | Observed FPS | p95 / p99 frame ms | Main persistence max ms | Background writes / failures | Background write max ms |
| --- | ---: | ---: | ---: | ---: | --- | ---: |
| source-model-npc-1x | 1657 | 59.8917 | 17.4164 / 18.5217 | 0.1970 | 2 / 0 | 19.0837 |
| source-model-npc-4x | 315 | 59.8085 | 17.4809 / 19.3262 | 0.2554 | 2 / 0 | 27.6419 |

Each trace has two intervals over 33 ms. The 1x trace has zero over 50 ms; 4x has one over 50 ms and zero over 100 ms. Pending writes at sample end drain through durable Quit before comparing committed profiles. These intervals include OS/VSync and are not GPU timestamps. They establish this short, actual remaining checkpoint on one device, not universal 60 FPS, zero stutter, peak-population/minimum-spec support or a long campaign benchmark. Earlier poor-background-context and pre-current-renderer measurements remain historical; they are not erased or substituted for this scoped comparison.

Four current motion directories under `target/journey-review/source-model-motion/`—`npc`, `battle-female`, `reticle-reload` and `ordnance`—each contain 24 native PNGs, a manifest and MP4. Review observed firing, reload/idle and the world/corpse presentation. The ordnance warmup skipped the actual throw gesture, so those frames do not prove the release animation. Two minimal actual-gesture fixtures are being added without a production visual change; their outcomes and any resulting test count are pending. Motion exports are stepped visual evidence, not real-time FPS or human input validation.

The current source/Views/design metadata reflect the source-owned facade, 169-node tree, durable journey transitions and storage seam. No current complete Windows OS journey or human listening result is claimed. Final QA completion and immutable archive/copied-executable/native integrity will be recorded when actually supplied, with post-freeze delivery facts in the external delivery record.


## Final source-model prebundle record — 3 November 2026

This is the final source-document record for the unified journey/source-owned model increment. It supersedes the pending states in the two checkpoint entries above without deleting their earlier failures, missed gestures or tested revisions. Older archives remain earlier builds, not this delivery.

**Final workspace verification: 432 passed, 0 failed, 1 intentionally ignored offline audio exporter—201 core and 231 application tests.** Formatting and strict all-target Clippy with warnings denied passed in `target/journey-review/playtest-verified-workspace-gate.log`; the final native build passed in `playtest-final-native-build.log`. The final two application additions are isolated review timing/metadata regressions; production model pixels, renderer and assets are unchanged from the accepted static/performance evidence. The source CLI smoke passed in `source-model-source-smoke.log` as scoped above, including separate fresh companion rejection and explicit legacy NPC compatibility.

**Current native/static and temporal coverage is accepted by Root and independent verification.** The eight model confirmation PNGs, six actual-game/preview PNGs, 68 supported matrix cases and exact original-bind black/white proof are recorded above. Current ownership remains 23/24 parts; original RGBA/SHA is unchanged. The source bind is exactly reconstructed, while changed poses, hidden repairs and garments have separate visual evidence. Stylized near-vertical rifles and limited head pitch remain declared 2D limitations, not natural 3D anatomy.

All six directories under `target/journey-review/source-model-motion/` contain 24 native PNGs, a manifest and MP4:

| Clip | Actual simulation ticks | Covered temporal evidence |
| --- | --- | --- |
| npc | 2338–2430 | NPC firing/idle and persistent world bodies |
| battle-female | 2338–2430 | June field pose and firing/world feedback |
| reticle-reload | 2412–2504 | Reload/resource transition and held recovery |
| ordnance | 2383–2475 | Existing projectile/world effects; its earlier warmup missed the hand release |
| gesture-throws | 2338–2430 | Real grenade release 2343 and Molotov release 2375, stowed gun, throw-hand departure and held-rifle return |
| gesture-melee | 2338–2430 | June contact event 2343 against target 13, closed fist/axe and hit flash, held-rifle return; commitment 27→0 |

The final two clips close the earlier missing-gesture coverage. The verifier inspected native first/release/middle/recovery PNGs plus decoded MP4 samples and found no material issue in the covered clips. Both encode 24 H.264 frames at 15 fps/1.6 seconds using the fixed-step export; neither is a performance measurement. Bodies remain nondecreasing, 11→12 in throws and 11→11 in melee. Gameplay events, charge consumption and recovery assertions passed through legal isolated fixture actions; no production pose change or reward shortcut was added for this review.

The current focused renderer timing/equality results above remain applicable: 1x 1657 focused frames at 59.8917 observed FPS / p99 18.5217 ms/main persistence maximum 0.1970 ms; 4x 315 focused frames at 59.8085 FPS / p99 19.3262 ms/main persistence maximum 0.2554 ms. Each completed two background writes with zero failures. Full decoded Game state equals both speeds and the prior worker result. The remaining round ends at 1772 simulation ticks before the 1800-frame target, and 4x includes one interval over 50 ms. These are scoped OS/VSync-inclusive observations, not universal 60 FPS, zero-stutter, GPU timestamps or minimum-spec/peak-population proof.

Final documentation checks passed **69 source-backed Views**, retaining all 64 prior IDs plus five developer review tasks: unique IDs, one root, valid parents, no cycles and actual files/symbols. Sidecar count/source paths, current local links, UTF-8, canonical DESIGN headings and all 23 exact normative colors passed. The original seventeen values, six authorized quality roles and Pixelify Sans identity remain preserved. Fifteen pixel-provenance SHA256 values and the 26-PNG inventory passed; Root's prompt scan also passed. Detailed model authoring/validation documents remain with their owning verification stream.

**Source documentation is now frozen before packaging.** Final archive path/hash, copied-executable smoke/native verification and any later Windows OS journey belong only in Root's external delivery record and delivery report. No package-integrity result is claimed before that verification. No complete current human OS campaign, human listening, long-session or universal balance/performance claim is made. No further source amendment cycle is required for immutable delivery hashes.

## Strict sale and ruleset 2 — 5 November 2026

`SellItem` is strict from ruleset 2 (`RULESET_VERSION` 2). An equipped, preset-referenced, recruit-equipped, reserved or NPC-issue item is refused with a typed `SaleBlocked` reason instead of being sold with its loadout slots replaced by starter equipment; `DeletePreset` removes a preset so its items can be sold. Seeded runs never sell, so `GOLDEN` and `GAMEPLAY_TRACE` are unchanged and `TRACE_RULESET` moved to 2 with them.

**Debug recordings made under ruleset 1 are not expected to reproduce across the strict-sale change.** Replay still loads their initial state through the save migration, applies every recorded command and reaches the end line, but a recorded sale of an equipped, preset or recruit-equipped item is now a typed rejection, so the final digest can differ. Replay warns that the recording was made under ruleset 1 (and, for v6 headers, save version 6). `pre_strict_recording_diverges_with_ruleset_warning` pins this with the v6 equipped-sale fixture. Recording FORMAT stays 1.

## Save clone — 6 November 2026 (W1-50)

Every save copies the game on the main thread and hands the copy to the storage worker. That copy is now `Game::save_snapshot`, used at `runner/apply.rs` instead of `Game::clone`. It leaves out the never-saved state: the typed event outbox, the live attempt's queued records and the settlement issues, in the root game and in every expedition's worker game. It serializes to exactly the game's bytes (`save_snapshot_skips_transient_outbox_and_equals_serialized_state`).

```powershell
cargo run --locked -p horde_core --example save_clone -- --out target/perf/save-clone/dev.json
cargo run --locked -p horde_core --example save_clone --profile playtest -- --profile-name playtest --out target/perf/save-clone/playtest.json
cargo run --locked -p horde_core --example save_clone --release -- --out target/perf/save-clone/release.json
```

`scripts/measure_performance.ps1 -SaveClone` runs the same example in the run's profile and adds its gate to the budget checks. The fixture is `save_clone_fixture(1000 | 2500 | 5000)`: the synthetic review-only stress game (three unsettled expeditions, 150 run records, 1,500 corpses, a 400-drawn main field) with its stash sized to that many items. Each series is 50 untimed and 1,000 timed iterations. Fixture creation is excluded. The gate (`PERF_BUDGET.json` `save_clone`) is `save_snapshot` p99 ≤ 1.0 ms at 1,000 items with 3 workers, in dev and playtest. Release is informational. `to_vec` is the storage worker's `serde_json::to_vec`, off the main thread, for scale.

Before any remedy, three runs per profile, Intel Core Ultra 9 185H with other desktop work running (about 28% CPU load), medians of the three:

| Profile | Items | `save_snapshot` p99 per run (ms) | p99 median | mean | `to_vec` p99 | Bytes |
|---|---:|---|---:|---:|---:|---:|
| dev | 1,000 | 1.140, 0.956, 0.981 | 0.981 | 0.526 | 6.60 | 1,408,714 |
| dev | 2,500 | 1.266, 1.106, 1.054 | 1.106 | 0.553 | 7.12 | 1,586,397 |
| dev | 5,000 | 1.604, 1.323, 1.410 | 1.410 | 0.724 | 9.81 | 1,882,516 |
| playtest | 1,000 | 1.049, 1.159, 2.445 | **1.159** | 0.501 | 5.57 | 1,408,714 |
| playtest | 2,500 | 0.994, 0.953, 1.491 | 0.994 | 0.493 | 6.58 | 1,586,397 |
| playtest | 5,000 | 1.317, 1.215, 2.345 | 1.317 | 0.640 | 7.62 | 1,882,516 |
| release | 1,000 | 1.735, 5.219, 0.979 | 1.735 | 0.767 | 11.04 | 1,408,714 |
| release | 2,500 | 1.825, 1.923, 1.127 | 1.825 | 0.738 | 11.22 | 1,586,397 |
| release | 5,000 | 3.010, 2.064, 1.551 | 2.064 | 1.014 | 10.77 | 1,882,516 |

Component means at 1,000 items in playtest: run history 0.109 ms, attempt 0.086 ms, expeditions 0.078 ms, stash items 0.052 ms. From 2,500 items the stash is the largest. **The gate fails in playtest (p99 above 1.0 ms in every run) and is marginal in dev (one run of three above).** So the conditional remedy applies, aimed at the run history, the largest component at the gated 1,000 items. The timings are wall-clock on a shared desktop, so single runs vary by up to 5x at p99, release included. The means are stable to about 10%.

### Remedy: the run history is shared copy-on-write

`Campaign.run_history` is now an `Arc<Vec<RunRecord>>` (serde `rc`). A save snapshot shares it with the live game. Recording a run appends through `Arc::make_mut`: in place while no snapshot holds the history (`recording_a_run_without_a_snapshot_appends_in_place`), and as a copy while one does, so snapshots still in flight keep the old records (`consecutive_snapshots_share_unchanged_component`). (The measurements below were taken with the first form, an `Arc<[RunRecord]>` rebuilt on every recorded run; a snapshot clones either form as one reference count, and the W1 review switched to `make_mut` so that a settlement no longer copies the whole unbounded history.) It serializes as the same plain sequence. The save payloads of the v6 fixture and of the saveable stress game (`stress_authored_fixture`, 150 records, standing in for `save_v7.json`, which save-determinism cuts at the wave-1 close) are pinned by FNV-1a hashes cut before the change. Both still round-trip byte-identical (`save_v6_and_v7_fixtures_round_trip_byte_identical`). The benchmark's byte counts are unchanged, and `GOLDEN` and `GAMEPLAY_TRACE` are unchanged.

Re-run, three runs per profile, same machine (about 53% CPU load from other work):

| Profile | Items | `save_snapshot` p99 per run (ms) | p99 median | mean | `to_vec` p99 | Bytes |
|---|---:|---|---:|---:|---:|---:|
| dev | 1,000 | 0.649, 0.654, 0.764 | **0.654** | 0.339 | 7.47 | 1,408,714 |
| dev | 2,500 | 0.706, 1.003, 0.900 | 0.900 | 0.427 | 7.76 | 1,586,397 |
| dev | 5,000 | 1.272, 1.011, 1.144 | 1.144 | 0.562 | 9.47 | 1,882,516 |
| playtest | 1,000 | 0.552, 0.692, 0.672 | **0.672** | 0.303 | 6.02 | 1,408,714 |
| playtest | 2,500 | 0.739, 0.657, 0.806 | 0.739 | 0.333 | 6.99 | 1,586,397 |
| playtest | 5,000 | 1.187, 1.133, 1.157 | 1.157 | 0.538 | 7.40 | 1,882,516 |
| release | 1,000 | 0.469, 0.604, 0.690 | 0.604 | 0.303 | 5.51 | 1,408,714 |
| release | 2,500 | 0.910, 0.635, 0.698 | 0.698 | 0.323 | 6.70 | 1,586,397 |
| release | 5,000 | 0.994, 0.961, 1.070 | 0.994 | 0.479 | 7.44 | 1,882,516 |

**The gate passes in every dev and playtest run (p99 at most 0.764 ms at 1,000 items).** 5,000 items remain above 1.0 ms at p99. That size is informational, and from 2,500 items the stash itself (0.13 to 0.30 ms mean) is the largest component. If it ever becomes gated, the next lever is `ItemDefId(Arc<str>)`. Reports: `target/perf/save-clone/` (before) and `target/perf/save-clone-remedy/` (after).

The `save_clone` example JSON after the remedy, one run per profile: the run with the median `save_snapshot` p99 of its three. Each is quoted verbatim from `target/perf/save-clone-remedy/<run>.json` (git-ignored) up to the end of its gated 1,000-item report; the 2,500- and 5,000-item entries that follow it in `reports` are left out here (the table above gives them), so each excerpt closes the array after the first entry.

Dev (`dev-2.json`):

```json
{
  "version": 1,
  "profile": "dev",
  "debug_assertions": true,
  "cpu": "Intel64 Family 6 Model 170 Stepping 4, GenuineIntel",
  "notes": [
    "CPU-only: Game::save_snapshot on the main thread, fixture creation excluded.",
    "Synthetic save_clone_fixture: the review-only stress game, never a gameplay claim.",
    "serialize is the storage worker's serde_json::to_vec, off the main thread."
  ],
  "gate": {
    "items": 1000,
    "workers": 3,
    "p99_ms_max": 1.0,
    "profiles": [
      "dev",
      "playtest"
    ],
    "gated": true,
    "p99_ms": 0.6541,
    "pass": true
  },
  "reports": [
    {
      "items": 1000,
      "workers": 3,
      "run_records": 150,
      "warmup": 50,
      "iterations": 1000,
      "save_snapshot": {
        "samples": 1000,
        "mean_ms": 0.34606470000000045,
        "p50_ms": 0.34450000000000003,
        "p95_ms": 0.5524,
        "p99_ms": 0.6541,
        "max_ms": 0.9420999999999999
      },
      "components": {
        "campaign_items": {
          "samples": 1000,
          "mean_ms": 0.04656950000000008,
          "p50_ms": 0.0453,
          "p95_ms": 0.0519,
          "p99_ms": 0.0697,
          "max_ms": 0.11170000000000001
        },
        "run_history": {
          "samples": 1000,
          "mean_ms": 0.000056000000000000575,
          "p50_ms": 0.00009999999999999999,
          "p95_ms": 0.00009999999999999999,
          "p99_ms": 0.00009999999999999999,
          "max_ms": 0.00009999999999999999
        },
        "attempt": {
          "samples": 1000,
          "mean_ms": 0.09423580000000009,
          "p50_ms": 0.0883,
          "p95_ms": 0.1282,
          "p99_ms": 0.1825,
          "max_ms": 0.4347
        },
        "battles": {
          "samples": 1000,
          "mean_ms": 0.08204480000000028,
          "p50_ms": 0.0792,
          "p95_ms": 0.0917,
          "p99_ms": 0.134,
          "max_ms": 0.4729
        }
      },
      "serialize": {
        "samples": 1000,
        "mean_ms": 5.077570599999998,
        "p50_ms": 4.3863,
        "p95_ms": 7.4043,
        "p99_ms": 8.086400000000001,
        "max_ms": 23.5182
      },
      "bytes": 1408714
    }
  ]
}
```

Playtest (`playtest-3.json`):

```json
{
  "version": 1,
  "profile": "playtest",
  "debug_assertions": false,
  "cpu": "Intel64 Family 6 Model 170 Stepping 4, GenuineIntel",
  "notes": [
    "CPU-only: Game::save_snapshot on the main thread, fixture creation excluded.",
    "Synthetic save_clone_fixture: the review-only stress game, never a gameplay claim.",
    "serialize is the storage worker's serde_json::to_vec, off the main thread."
  ],
  "gate": {
    "items": 1000,
    "workers": 3,
    "p99_ms_max": 1.0,
    "profiles": [
      "dev",
      "playtest"
    ],
    "gated": true,
    "p99_ms": 0.6718,
    "pass": true
  },
  "reports": [
    {
      "items": 1000,
      "workers": 3,
      "run_records": 150,
      "warmup": 50,
      "iterations": 1000,
      "save_snapshot": {
        "samples": 1000,
        "mean_ms": 0.30348860000000033,
        "p50_ms": 0.3244,
        "p95_ms": 0.448,
        "p99_ms": 0.6718,
        "max_ms": 1.0708
      },
      "components": {
        "campaign_items": {
          "samples": 1000,
          "mean_ms": 0.04807510000000013,
          "p50_ms": 0.0458,
          "p95_ms": 0.0584,
          "p99_ms": 0.0909,
          "max_ms": 0.4081
        },
        "run_history": {
          "samples": 1000,
          "mean_ms": 0.00005580000000000057,
          "p50_ms": 0.00009999999999999999,
          "p95_ms": 0.00009999999999999999,
          "p99_ms": 0.00009999999999999999,
          "max_ms": 0.00009999999999999999
        },
        "attempt": {
          "samples": 1000,
          "mean_ms": 0.0794075999999995,
          "p50_ms": 0.0738,
          "p95_ms": 0.1067,
          "p99_ms": 0.1754,
          "max_ms": 0.36900000000000005
        },
        "battles": {
          "samples": 1000,
          "mean_ms": 0.07524969999999966,
          "p50_ms": 0.0707,
          "p95_ms": 0.0964,
          "p99_ms": 0.1614,
          "max_ms": 0.4041
        }
      },
      "serialize": {
        "samples": 1000,
        "mean_ms": 3.695138299999995,
        "p50_ms": 3.3604,
        "p95_ms": 5.3569,
        "p99_ms": 6.032,
        "max_ms": 9.290899999999999
      },
      "bytes": 1408714
    }
  ]
}
```

Release (`release-2.json`):

```json
{
  "version": 1,
  "profile": "release",
  "debug_assertions": false,
  "cpu": "Intel64 Family 6 Model 170 Stepping 4, GenuineIntel",
  "notes": [
    "CPU-only: Game::save_snapshot on the main thread, fixture creation excluded.",
    "Synthetic save_clone_fixture: the review-only stress game, never a gameplay claim.",
    "serialize is the storage worker's serde_json::to_vec, off the main thread."
  ],
  "gate": {
    "items": 1000,
    "workers": 3,
    "p99_ms_max": 1.0,
    "profiles": [
      "dev",
      "playtest"
    ],
    "gated": false,
    "p99_ms": 0.6037,
    "pass": true
  },
  "reports": [
    {
      "items": 1000,
      "workers": 3,
      "run_records": 150,
      "warmup": 50,
      "iterations": 1000,
      "save_snapshot": {
        "samples": 1000,
        "mean_ms": 0.3058237000000003,
        "p50_ms": 0.31520000000000004,
        "p95_ms": 0.4398,
        "p99_ms": 0.6037,
        "max_ms": 0.7374999999999999
      },
      "components": {
        "campaign_items": {
          "samples": 1000,
          "mean_ms": 0.04997120000000006,
          "p50_ms": 0.0455,
          "p95_ms": 0.0652,
          "p99_ms": 0.075,
          "max_ms": 0.2086
        },
        "run_history": {
          "samples": 1000,
          "mean_ms": 0.000056000000000000575,
          "p50_ms": 0.00009999999999999999,
          "p95_ms": 0.00009999999999999999,
          "p99_ms": 0.00009999999999999999,
          "max_ms": 0.00009999999999999999
        },
        "attempt": {
          "samples": 1000,
          "mean_ms": 0.08144290000000028,
          "p50_ms": 0.07490000000000001,
          "p95_ms": 0.1141,
          "p99_ms": 0.1842,
          "max_ms": 0.4469
        },
        "battles": {
          "samples": 1000,
          "mean_ms": 0.0719779,
          "p50_ms": 0.0697,
          "p95_ms": 0.07880000000000001,
          "p99_ms": 0.1211,
          "max_ms": 0.39609999999999995
        }
      },
      "serialize": {
        "samples": 1000,
        "mean_ms": 3.483183499999998,
        "p50_ms": 3.2752,
        "p95_ms": 4.522,
        "p99_ms": 5.6297999999999995,
        "max_ms": 10.418999999999999
      },
      "bytes": 1408714
    }
  ]
}
```

## Save format 7 frozen — 6 November 2026 (W1-63)

**Save format 7 is frozen.** `crates/horde_core/tests/fixtures/save_v7.json` is a real `Game::save` generation: envelope version 7, ruleset 2, 95,266 bytes (budget 96 KB), envelope checksum `8527615155064533385`. It was cut by the `#[ignore]`d writer `write_save_v7_fixture` in `tests/save_fixtures.rs` (`LASTLIGHT_WRITE_SAVE_V7_FIXTURE=<path>`; it refuses to overwrite and asserts `SAVE_VERSION == 7`), re-cut once in the W1-63 review before the freeze closed, and is never re-cut again: its checksum and byte length are pinned. Two cuts from the same tree were byte-identical.

Its scenario (`stage_v7_scenario`, seed `0x5AFE_0007`) is played through `Game::apply` and the smoke bot up to a live Npc defense of Romania stage 1 at tick 1740, beside one settled and one live expedition. The v7 fields that have no wave-1 writer are then staged through the test-support seams: `Staging::stage_takeover` hands the defense to recruit 1, and the staging handle and the `*_mut` accessors (plus the new test-support `Game::pending_runs_mut`) stage drop tracking, receipts, the expedition summary and pending sale, ledgers, a Capstone snapshot on wave tier 11, per-class skill modifiers, the new enemy-side state (an elite, a spitter mid-windup, a splitter and its child, exploders, a phased boss) and per-archetype telemetry. Combat logs are trimmed to their last four lines to fit the budget. Every `V7_SAVED_KEYS` key occurs at a non-default value, and the file loads through `validate_game`.

Guards in `tests/save_migration.rs`:

- `v7_fixture_is_a_real_v7_generation`: the frozen checksum and byte length, the envelope, the size budget and every v7 key at a non-default value (root or worker game).
- `v7_fixture_holds_every_defender_key_at_non_default`: the defender state by JSON pointer (seat, Defending recruit, reserved loadout, appearance, next defender, expedition seats, the unacknowledged summary, folded outcomes, containments, the main-run takeover record with its receipts and a report naming the same recruit, and each pending record in its worker's country under an attempt id absent from `run_history`).
- `v7_fixture_loads_validates_and_resaves_byte_identically`: the payload resaves unchanged, a second resave is byte-identical, and one flipped seed digit (still valid JSON) is refused with `ChecksumMismatch`.
- `v7_fixture_round_trips_every_enemy_balance_field_to_equal_digest`: the 37 enemy and balance keys keep their saved text, and the loaded game digests to the saved checksum.

Proofs recorded at the cut: removing one key (`pending_runs[0].boss_rank`, checksum recomputed) fails `v7_fixture_is_a_real_v7_generation` with `never at a non-default value: ["3.5 RunRecord /pending_runs/boss_rank"]`; flipping one payload byte without the checksum fails the three loading tests with `ChecksumMismatch`. W1-63 review: with the checksum check removed from `decode_generation`, the seed-digit flip is no longer refused with `ChecksumMismatch` and the resave test fails; the re-cut fails the identity pin with `left: (Some(8527615155064533385), 95266)`, `right: (Some(9320268661059722176), 94965)`.

From now on a new saved field must be serde-defaulted, skipped at its default and load `save_v7.json` unchanged; otherwise the work stops and the user is asked (the only pre-approved exception is R5, `SkillModifiers.heavy_target_damage_multiplier`). Later cases use inline JSON or new `save_v7_w2_*.json` / `save_v7_w4_*.json` files. Over the whole of wave 1, `GOLDEN` and `GAMEPLAY_TRACE` moved 0/6 each and `PROVISIONAL` is None.

## The isolated 0.5 s frames (gate 3) — 6 November 2026 (W1-68)

**Finding, in two halves (narrowed by the W1-68 review).**
- **VSync on (the W1-67 stalls): a swap-chain acquire stall in the graphics driver or compositor of this hybrid-graphics laptop.** In 47 of the 56 stalled W1-67 frames the time sat in `get_current_texture`. These stalls come and go with the machine's state, not with the code.
- **VSync off (the W1-61 playtest stalls of 446-532 ms, the spec's own observation): not attributed.** They were not in the acquire call: all 16 frames over 50 ms had a present_wait of 0.09-0.20 ms. No span that the v3 report recorded at the time holds them. They did not recur under the W1-68 instrumentation, so the new gap fields have never seen one. Their cause is not named, and W1-68's done_when ("the cause is named with evidence") is not met for this half.

Nothing in process was found to cause either. Gate 3 (`frames_over_50ms` 0) is unchanged. It needs a user ruling on two separate open items: the environmental VSync-on acquire stall, and the unattributed VSync-off stall, which needs a reproduction under the W1-68 probe.

**What the frame report now records (W1-68).** These fields are still report version 3; every earlier key keeps its meaning.
- `phases.gap_ms`: the main thread from the previous frame's end of `Last` to this frame's First.
- `phases.handoff_ms`: the first part of the gap, the render hand-off. With pipelining this is the `RenderExtractApp` extract: channel receive, `RenderApp` extract, send. Without pipelining it is the `RenderApp` extract plus the Render schedule.
- `phases.channel_wait_ms`: the hand-off minus the extract, that is, the pipelined wait for the render thread. The extract wrapper of today's report excludes it.
- `phases.outside_update_ms`: the rest of the gap (winit event loop, OS).
- `phases.render_idle_ms`: the render thread from one render frame's end to the next frame's begin.
- `events`: per-frame counts of winit window messages, `Image` and `AudioSource` asset events, and spawned `AudioPlayer` voices.
- `longest_frames`:
  - Wall-clock instants, in ms since the probe epoch: the `Time<Real>` instant that closed the frame, First begin, end of `Last`, and the render sample's begin, present begin and end.
  - The frame's events and egui texture-upload bytes.
  - An `attribution` that overlaps the frame's window (`time_ms - frame_ms .. time_ms`) with every sample's main-thread spans: main schedule, channel wait, the rest of the hand-off, and outside update. It does the same with every distinct render sample: present wait, the rest of the render frame, and time with no render frame. `coverage` is the covered share of the window. `outside_update` is a residual (gap minus hand-off). Consecutive samples tile the main thread, so `coverage` is 1.0 by construction for any window inside the recorded range; only missing samples (the first sample, warmup) lower it. `named_coverage` (W1-68 review) leaves the residual out: (main schedule + channel wait + rest of the hand-off) / frame. An unexplained stall shows as a large `outside_update_ms` and a low `named_coverage`.
- `slow_frames`: how many frames were over 50 ms, how many were timed, and how many of those are covered to at least 95%. `covered_95_named`, `min_named_coverage` and `max_outside_update_ms` give the same over the named spans only.

Pins: `slow_frame_reports_unattributed_time`, `slow_frame_outside_every_named_span_has_low_named_coverage`, `perf_launch_records_the_between_frames_gap_and_events`, `pipelined_handoff_holds_the_channel_wait_and_the_extract` and `unpipelined_handoff_holds_the_extract_and_the_render_schedule`.

**Fresh probes.** Same hardware and power state as before (mains, Balanced). The machine was shared, and a 5 s `Win32_Processor.LoadPercentage` sampler ran beside every pass (up to 100% load in two passes). Each pass is `measure_performance.ps1 -Repeat 3` at both sizes. Every run was valid, and every pass exited 0.

| Pass (target/perf/w1-68/) | Build | Scene, frames | Present, pipelined, adapter, backend | Runs | Frames > 50 ms | cpu_frame p99 (median, normal / small) | gap p50 / p99 (normal) | channel_wait mean (normal) |
|---|---|---|---|---|---|---|---|---|
| playtest-novsync-pipelined-default | W1-68 | progressed-peak, 600 | AutoNoVsync, on, RTX, Vulkan | 6 | 0 | 6.89 / 6.17 | 0.66 / 1.42 | 0.08 |
| playtest-novsync-pipelined-off | W1-68 | progressed-peak, 600 | AutoNoVsync, off, RTX, Vulkan | 6 | 0 | 7.83 / 6.23 | 2.86 / 5.81 | 0 |
| playtest-novsync-arc | W1-68 | progressed-peak, 600 | AutoNoVsync, on, Intel Arc (integrated), Vulkan | 6 | 0 | 6.84 / 6.26 | 0.81 / 3.12 | 0.33 |
| playtest-novsync-dx12 | W1-68 | progressed-peak, 600 | AutoNoVsync, on, RTX, Dx12 (`WGPU_BACKEND=dx12`) | 6 | 0 | 6.57 / 6.31 | 2.81 / 4.92 | 2.17 |
| playtest-novsync-stress | W1-68 | stress, 1800 | AutoNoVsync, on, RTX, Vulkan | 6 | 0 | 6.72 / 6.55 | 0.60 / 1.16 | 0.04 |
| playtest-novsync-progressed-peak-3600 | W1-68 | progressed-peak, 3600 | AutoNoVsync, on, RTX, Vulkan | 6 | 0 | 7.75 / 6.55 | 0.64 / 1.34 | 0.05 |
| dev-vsync-progressed-peak | W1-68 | progressed-peak, 600 | Fifo, on, RTX, Vulkan | 6 | 0 | 9.33 / 7.50 | 13.32 / 14.39 | 12.43 |
| dev-vsync-stress | W1-68 | stress, 1800 | Fifo, on, RTX, Vulkan | 6 | 0 | 8.27 / 8.63 | 13.05 / 14.19 | 12.33 |
| playtest-vsync-stress | W1-68 | stress, 1800 | Fifo, on, RTX, Vulkan | 6 | 0 | 7.75 / 7.84 | 13.64 / 14.67 | 12.93 |
| head-control-dev-vsync-stress | HEAD 99850aa | stress, 1800 | Fifo, on, RTX, Vulkan | 6 | 0 | 8.42 / 8.60 | - | - |
| dev-vsync-stress-3600 | HEAD 99850aa | stress, 3600 | Fifo, on, RTX, Vulkan | 6 | 0 | 8.38 / 7.96 | - | - |

The last two passes ran the W1-67 binary itself. In the first, the W1-68 files were set aside so the build was HEAD. The second ran an unrebuilt HEAD dev build, because restoring the files with their older timestamps did not trigger a rebuild. Its report has no W1-68 fields, which confirms it was the HEAD build.

**Evidence.** Items 1 and 2 are about the VSync-on stalls only.
1. **The same code stalls or not depending on the hour.**
   - At 18:09 the W1-67 binary (this HEAD) had 4-14 frames over 50 ms in every dev VSync-on stress run (`target/perf/w1-67/after-dev-vsync-stress`).
   - From 18:50 to 19:04 the same binary had 0 in 12 runs (6 x 1,800 and 6 x 3,600 frames).
   - The W1-68 build had 0 in 54 runs from 18:31 to 19:06.
   - Scene, window sizes and script were the same throughout. Nothing in the session changed the power plan (mains and Balanced when checked at 18:49).
2. **The stalls sat in the driver's acquire call.** In the stalled W1-67 frames:
   - In 47 of the 56 stalled W1-67 frames, the frame's own render sample shows it: present_wait was 497-547 ms or 995-998 ms (two tail frames had 99 and 348 ms). present_wait times `prepare_windows`, where `get_current_texture` blocks on the swap chain.
   - Main CPU was 5.6-12.9 ms, and render-world CPU was a normal 2.3-3.5 ms.
   - In the other 9, the frame's render sample was a normal one (Render schedule 2.7-10 ms). The v3 report keeps only the latest render sample per frame, and the stall had landed in a render frame it did not keep. The W1-68 per-sample instants and the attribution now place such a frame.
   - The 1.0 s frames equal wgpu-core's surface acquire timeout (`FRAME_TIMEOUT_MS = 1000`, wgpu-core 30.0.1 present.rs:32, :178). The render thread waited out that whole timeout in the driver's image acquire, sometimes five frames in a row (stress-small-r3, samples 1024-1028).
3. **No in-process work was busy in the fresh probes, but they had no stalls.** These counts come from the W1-68 probes, which had 0 frames over 50 ms. They rule nothing out for a stalled frame; they show that none of these sources was busy in the passes run.
   - egui texture uploads were 0 in every sampled frame of every pass, so the W1-64 churn is gone.
   - `Image` and `AudioSource` asset events were 0 in every frame.
   - Window messages were 0, except 2 in one stress run.
   - No audio voice was ever spawned. The review scenes run with master volume 0 (`ui/review/matrix.rs:21`), so `play_sounds` returns before spawning. Audio voice creation therefore cannot have caused the stalls measured in these scenes. Ordinary play with sound is not covered by this.
   - OS descheduling cannot be excluded in general. For the VSync-on stalls it would not land, time after time, inside the one swap-chain call, in multiples of about 0.5 s and at the 1.0 s timeout. For the VSync-off stalls it is not excluded.
4. **The VSync-off stalls (W1-61) were not in the acquire call, and are not attributed.** `target/perf/w1-61/probe-playtest-novsync` and `-stress` (AutoNoVsync, pipelined, RTX, Vulkan) had 16 frames over 50 ms: 4 in stress-normal, 8 in stress-small, 2 in progressed-peak-normal and 2 in progressed-peak-small. All but one (58 ms) were 445.5-510.2 ms. In every one of them:
   - present_wait was 0.09-0.20 ms;
   - main CPU was 5.8-11.7 ms and the Render schedule 2.0-7.4 ms.

   The v3 report keeps only the latest render sample per frame, so for any single frame a stall in a render frame it did not keep cannot be excluded. In the VSync-on W1-67 runs that applied to 9 of 56 stalled frames; here it would have to apply to 16 of 16. So the VSync-off stalls sat outside every span that report recorded: in the main-thread gap (winit, the OS) or in render-thread idle. W1-61 predates both the W1-64 egui texture-upload counter and the W1-68 gap and event fields. So egui texture churn (fixed in W1-64), window or asset events, and OS descheduling are all still open for them. Audio voices are not: the review scenes were already muted then (`ui/review/matrix.rs`, since 4 November). Since W1-64, no VSync-off probe has had a frame near 0.5 s. The W1-64 review pass (`target/perf/w1-64/review-playtest-novsync`, 18 runs) and the six W1-68 VSync-off passes (36 runs) had 0 frames over 50 ms. A deliberately unfocused W1-64 stress run (`playtest-novsync-stress-unfocused`) had 1-2 frames of 52-157 ms, with present_wait of 0.06-0.37 ms and 0 texture uploads. That is consistent with W1-64's fix but does not show it, because the VSync-on stall also stopped in the same hours.
5. **What the new fields show on real frames, and what they cannot.** Over the 20 longest frames of each instrumented run (1,080 frames, the longest 23.4 ms), the attribution `coverage` was 100% for 1,077. The other 3 are a run's first samples, whose windows begin before the first kept sample. That 100% is an identity, not evidence: `outside_update` takes whatever the named spans leave, so `coverage` is 1.0 for any window inside the recorded range. These probes were recorded before `named_coverage` existed. With VSync on, the gap of about 13.3 ms is the channel wait (about 12.4 ms) plus 0.8 ms outside the update: the main thread waits on a render thread that is held by the present. If a stall returns, a frame over 50 ms will show whether it was:
   - the channel wait, with the render thread's present wait (high `named_coverage`);
   - time outside App::update, winit or the OS (large `outside_update_ms`, low `named_coverage`);
   - the main schedule.
6. **The A/B probes cannot tell the arms apart today.** Pipelined off, the integrated Intel Arc adapter and the DX12 backend all had 0 frames over 50 ms. So did the default arm in the same hour. Without pipelining the gap holds the render work (2.9 ms) and the median frame rises from 3.2 to 5.5 ms. DX12 has a longer channel wait (2.2 ms against 0.1 ms on Vulkan) at a similar cpu_frame p99.

**Status: W1-68 is not done for the VSync-off half.** Every fresh probe meets the done_when coverage (every frame over 50 ms covered to at least 95%) only trivially, because there were 0 such frames.
- VSync on: the cause is named, an environmental acquire stall, so no in-process fix follows. The intermittent stall makes gate 3 unreachable on demand on this machine.
- VSync off: the cause is not named. The W1-61 stalls were not in the acquire call, and none has been seen under the W1-68 probe.

Two open items need a user ruling; the gate was not moved:
1. the environmental VSync-on acquire stall;
2. the unattributed VSync-off stall, which needs a reproduction under the W1-68 probe (`named_coverage`, `outside_update_ms`, events).

In the same probes, cpu_frame p99 with VSync off was 6.2-7.8 ms at progressed-peak and 6.6-6.7 ms in stress, against ≤ 10.0. That is informational for W1-61, which re-runs the gates.

## Gate 3 driver stalls: the user ruling — 6 November 2026 (W1-61)

**The ruling, verbatim (user, in chat, 2026-11-06): "Exclude driver stalls".** Frames over 50 ms in which the swap-chain acquire wait (present_wait, or channel_wait while the render thread is in present) holds at least 80% of the frame are classified as environmental driver stalls and excluded from frames_over_50ms. They must still be attributed (named coverage), the in-process work in that frame (main_cpu and render CPU) must stay below 50 ms, and the excluded stalls are counted and listed in the report and summary (frames_over_50ms_driver_stall). The same exclusion applies to the pipelined rule's frames_over_33ms and VSync-on FPS and p99-interval checks, computed on frames outside stalls.

The gate stays at 0 frames over 50 ms; what it counts changed. The thresholds are in `docs/PERF_BUDGET.json` `driver_stall`: over 50 ms, acquire share at least 0.8, named coverage at least 0.95, in-process work below 50 ms.

In-process work (`in_process_ms`) is main CPU plus render CPU. A frame's window is `time_ms - frame_ms .. time_ms` and closes at First, so the sample's own First..Last runs after it; the main schedule inside the stalled frame is the previous sample's. Main CPU is therefore the larger of `attribution.main_cpu_ms` (the main schedules overlapping the window) and the sample's own `phases.main_cpu_ms` (W1-61 review; before it, only the sample's own was read). Render CPU is the larger of the latest and the stalled render frame's extract + Render schedule − present wait. Re-classified with this rule, every recorded W1-61 report (`target/perf/w1-61/head-*`, 127 reports) keeps exactly its driver stalls: 235 of 235, in-process work at most 19.2 ms.

**Report fields (report version 3; `frames_over_50ms` and every other earlier key keep their meaning).**
- `longest_frames[].attribution.acquire_wait_ms`: the part of the frame's window where a main-thread render hand-off overlaps a render frame's `prepare_windows`. Not pipelined, the hand-off holds the frame's own Render schedule, so this is its present_wait. Pipelined, it is the channel wait while the render thread is in present.
- `slowest_present` in the probe: each close also receives the render frame with the longest present wait since the previous close. Pipelined, the render frame that stalled finishes, and the next one replaces it before the main thread closes the frame. The first W1-61 VSync-off A/B runs (before W1-69) showed this in all 94 pipelined frames over 50 ms: the main thread sat 481-625 ms in the channel wait, and no recorded render frame overlapped the window. Their pipelined-off twins had 160 such frames, each with 467-503 ms of present_wait. The attribution now counts that render frame too.
- `frames_over_50ms_driver_stall` and `driver_stalls`: `rule`, `frames` (every stall: `sample_index`, `frame_ms`, `acquire_wait_ms`, `acquire_share`, `in_process_ms`, `named_coverage`) and `outside` (`sample_frames`, `mean_observed_fps`, `p99_frame_ms`, `frames_over_33ms` and `frames_over_50ms` of every other sampled frame).

**Gate.** `measure_performance.ps1` compares the worst repeat of `driver_stalls.outside.frames_over_50ms` with `frames_over_50ms_max`. A report without that field is invalid evidence (exit 2). summary.json rows add `frames_over_50ms_driver_stall`, `frames_over_50ms_outside_stalls` and `driver_stalls`. summary.md shows every frame over 50 ms with its stalls in brackets.

Pins:
- `driver_stalls_are_excluded_from_the_gated_frame_counts`: a 500 ms present-wait frame passes; a 500 ms main-schedule frame, a 60 ms frame with 40% present wait, a stall with 60 ms of render CPU and an untimed frame still count.
- `pipelined_channel_wait_is_a_stall_only_while_the_render_thread_presents`.
- `a_replaced_render_frame_still_attributes_the_channel_wait` and `close_keeps_the_slowest_present_since_the_previous_close`.
- `driver_stall_in_process_work_reads_the_main_schedule_inside_the_window` (W1-61 review): a 500 ms frame with a 420 ms acquire and 70 ms of the previous sample's main schedule inside its window stays gated; with 30 ms it is a stall listed at 37.3 ms.
- `driver_stall_needs_named_coverage_and_the_acquire_share` (W1-61 review): named coverage 0.90 stays gated; acquire share 0.79 is gated, 0.81 is a stall.
- `driver_stall_rule_matches_the_ruling`.
- The `-SelfTest` fixtures `driver-stall` (exit 0) and `missing-driver-stalls` (exit 2).

## Goal-1 acceptance, the pipelined decision and the shipped smoke — 6 November 2026 (W1-61)

**Conditions.**
- Commit 199d823, which includes W1-69 (enemies no longer queue, so more of them are in contact). The working tree added only tests and docs.
- Intel Core Ultra 9 185H, NVIDIA RTX 1000 Ada Generation Laptop GPU, Vulkan, mains power, a focused window at 1440x900 @ 1.0 and 1050x700 @ 1.35.
- A 5 s `Win32_Processor.LoadPercentage` sampler ran beside every pass (`target/perf/w1-61/<pass>.load.tsv`). During the measured runs the load stayed below 90%, except one A/B pass (`head-ab-dev-novsync/progressed-peak-on-r1`, 93%), whose cells are in line with their repeats.
- Every run was valid at its first attempt.

**Definitions (frame report v3).**
- `interface_draw`: `work_mean_ms[2]`, the `ui::draw` wall time; it is v2-compatible and gated against wave 0.
- `interface_total`: the egui pass loop plus tessellation.
- `cpu_frame`: per sample, main First..Last plus (extract + Render schedule − present wait) of the latest completed render sample. Quantiles are taken over those sums. It is gated only with `-VsyncOff`.
- `frames_over_50ms`: the gate counts the frames outside driver stalls (`driver_stalls.outside.frames_over_50ms`, see above), worst repeat.
- Medians are taken over 3 repeats.

**Acceptance passes** (`target/perf/w1-61/head-acc-*`; every pass exited 0, every repeat had 0 frames over 50 ms, and 0 driver stalls were excluded, in a stall-free hour: see "The acceptance passes and the A/B disagree on driver stalls" below):

| Pass | Gate | normal | small | Limit |
|---|---|---|---|---|
| dev, VSync on, progressed-peak | interface_draw mean (ms) | 0.990 | 0.952 | ≤ 1.129 / 1.030 |
| dev, VSync on, progressed-peak | interface_total mean (ms) | 1.952 | 1.805 | ≤ 4.645 / 4.590 (v3) |
| dev, VSync on, peak | interface_draw mean (ms) | 0.775 | 0.768 | ≤ 1.190 / 1.075 |
| dev, -VsyncOff | cpu_frame p99 progressed-peak / stress (ms) | 8.85 / 7.50 | 7.01 / 7.84 | ≤ 10.0 |
| playtest, -VsyncOff | cpu_frame p99 progressed-peak / stress (ms) | 6.26 / 6.48 | 6.44 / 6.54 | ≤ 10.0 |

The dev VSync-on stress pass (frames over 50 ms and texture uploads only) also exited 0. In the playtest pass, the one 100% load sample fell in its shipped cargo build, before the first run; during the runs the load was at most 78%.

**Low latency** (`head-low-latency-playtest`). This was a playtest review launch without a scenario. Its `LASTLIGHT_SAVE_DIR` held `settings.json` `{"version":1,"low_latency":true,"master":0}`, and `LASTLIGHT_PERF_REPORT` was set. The report shows `present {requested_mode Mailbox, source settings, pipelined_rendering false, desired_maximum_frame_latency 1, low_latency true}`: 300 frames on the title screen, median 16.65 ms.

**Shipped smoke.** `-ShippedSmoke` builds `--profile playtest --no-default-features`, copies the exe out and times `--smoke` three times: 2.43, 1.19 and 1.23 s. shipped_smoke_seconds is 1.23 (the median), recorded in `PERF_BUDGET.json` `acceptance`. The first, cold run is the slow one.

**Integrated GPU.** Still pending: `hardware.integrated_gpu.status` is `pending`, with the ACCEPTANCE.md human-check row.

**Pipelined A/B on this commit** (`head-ab-playtest-novsync`, `head-ab-dev-novsync`, `head-ab-playtest-vsync`). The first W1-61 A/B (`ab-*`) predates W1-69 and the driver-stall rule, so it was re-run rather than re-classified. Protocol: interleaved on/off at the run level; playtest and dev; -VsyncOff; progressed-peak and stress at both sizes; 3 repeats per arm; 1,800 frames. Then the playtest VSync-on pair at the budget frame counts. That makes 12 valid runs per arm in each group. Values are medians of 3, frame counts are the worst repeat, and driver stalls are summed over the 3 repeats.

| Profile, VSync off | Scene | Size | cpu_frame p99 on / off | median frame on / off | frames > 33 ms outside stalls on / off | driver stalls on / off |
|---|---|---|---|---|---|---|
| playtest | progressed-peak | normal | 6.15 / 6.32 | 2.99 / 4.99 | 0 / 0 | 13 / 13 |
| playtest | progressed-peak | small | 6.12 / 5.73 | 2.91 / 4.89 | 0 / 0 | 7 / 16 |
| playtest | stress | normal | 7.30 / 6.09 | 3.06 / 5.15 | 0 / 0 | 17 / 16 |
| playtest | stress | small | 6.08 / 6.03 | 3.02 / 5.01 | 0 / 0 | 8 / 9 |
| dev | progressed-peak | normal | 7.11 / 6.93 | 3.52 / 5.84 | 0 / 0 | 9 / 21 |
| dev | progressed-peak | small | 6.49 / 6.54 | 3.40 / 5.67 | 0 / 0 | 10 / 13 |
| dev | stress | normal | 7.55 / 6.87 | 3.72 / 6.00 | 0 / 0 | 20 / 21 |
| dev | stress | small | 7.03 / 6.73 | 3.59 / 5.85 | 0 / 0 | 13 / 14 |

| Playtest, VSync on | Size | mean FPS outside stalls on / off | p99 interval outside stalls on / off | frames > 50 ms outside stalls on / off | driver stalls on / off |
|---|---|---|---|---|---|
| progressed-peak | normal | 60.00 / 60.00 | 17.45 / 17.40 | 0 / 0 | 1 / 5 |
| progressed-peak | small | 60.00 / 59.99 | 19.69 / 19.13 | 0 / 0 | 3 / 3 |
| stress | normal | 60.00 / 60.00 | 17.82 / 17.35 | 6 / 0 | 3 / 0 |
| stress | small | 60.00 / 60.00 | 19.60 / 19.66 | 0 / 0 | 0 / 0 |

**Decision: pipelined rendering stays on by default.** The rule is to switch it off only if, with pipelining off:
- every VSync-off cpu_frame p99 is ≤ 12.0 ms with 0 frames over 33 ms;
- the VSync-on runs keep a mean FPS ≥ 59.5, a p99 interval ≤ 17.7 ms and 0 frames over 50 ms.

The counts, FPS and p99 interval are taken outside driver stalls. The off arm meets every VSync-off limit, but its VSync-on p99 interval at the small size is 19.13 ms (progressed-peak) and 19.66 ms (stress). The on arm misses that interval too, so the small size has a wider VSync interval tail either way. With VSync off, pipelining roughly halves the median frame at a similar cpu_frame p99. The decision is recorded in `PERF_BUDGET.json` `decisions.pipelined_rendering`, which `pipelined_default_matches_recorded_decision` re-judges against `PIPELINED_DEFAULT`. Mutating the recorded default to false fails that test with "the recorded default does not follow the rule from the recorded off-arm numbers". Low latency still turns pipelining off.

**The VSync-off stall is named.** Every one of the 220 VSync-off frames over 50 ms in this A/B, with pipelining on or off, was a driver stall:
- frames of 480-623 ms;
- acquire shares of 0.981-0.992, named coverage of at least 0.95;
- in-process work of 7.8-19.2 ms.

Pipelined, the stalled render frame finished before the main thread closed its frame, so the earlier reports never saw its present wait. `slowest_present` now keeps it. W1-68's open VSync-off half is therefore the same swap-chain acquire stall as the VSync-on half.

One VSync-on stress repeat with pipelining (`head-ab-playtest-vsync/stress-on-r2`) had a stretch of about 400 frames of 47-67 ms. The six of them over 50 ms were mostly acquire wait (acquire shares 0.78-0.86), but their named coverage was only 0.89-0.94, with 4-6 ms outside App::update. So the rule kept them gated, and that repeat exited 1. It stays visible as evidence, and the gate was not moved.

**Open (W1-61 review, 2026-11-07).** That repeat ran the shipped default configuration (playtest, pipelined on, VSync on). Outside driver stalls it had 6 frames over 50 ms, 407 frames over 33 ms, a p99 interval of 47.1 ms and a mean of 46.3 FPS. That is a sustained 15-20 FPS episode, not an isolated stall. Its twins were clean, it is not attributed (4-6 ms per frame outside App::update), and gate 3 on HEAD is not claimed clean while it stands. It is an open item next to W1-68's VSync-off half (plan 9.4, waves.md) and a candidate new finding for the orchestrator.

**The acceptance passes and the A/B disagree on driver stalls (W1-61 review, 2026-11-07).** Both ran the same commit, 1,800 stress frames, pipelining on. The 12 acceptance stress runs with -VsyncOff (dev and playtest) had 0 driver stalls; the 12 pipelined-on A/B stress runs had 58 (0-12 each). The harness differs only in `-Pipelined on` (`LASTLIGHT_PERF_PIPELINED=1`, which sets the same plan value; `present.source` is `launch-override` either way because of -VsyncOff) and `-Repeat 1` per invocation. One discriminating pass ran both, interleaved, on 2026-11-06 23:10-23:16 UTC (`target/perf/w1-61/review/`; playtest, -VsyncOff, stress, both sizes, 1,800 frames):

| Pass | Harness | Runs | Frames > 50 ms | Driver stalls |
|---|---|---|---|---|
| `a1-on-r1`, `a2-on-r1` | `-Pipelined on -Repeat 1` (A/B) | 4 | 0 | 0 |
| `b1-default-r3` | default, `-Repeat 3` (acceptance) | 6 | 0 | 0 |
| `c1-on-r3` | `-Pipelined on -Repeat 3` | 6 | 0 | 0 |
| `d1-default-r1` | default, `-Repeat 1` | 2 | 0 | 0 |

No harness variable brings the stalls in. They come and go with time. The last driver stall in the W1-61 evidence fell in the VSync-on A/B at about 22:22 UTC; none followed in the 54 acceptance runs from 22:32 or in these 18 runs. W1-68 saw the same thing: 4-14 per stress run at 18:09, then 0 in 66 probes. So the A/B on arm stands for the default. The acceptance pass's 0 driver stalls shows a stall-free hour, not that the stall is gone; on another hour the same passes would exclude stalls under the ruling. Caveats: the discriminating pass ran the W1-61 review binary (it differs only in the stall rule) and had no CPU-load sampler.

## Settled corpses in stress — 7 November 2026 (W1-49)

W1-49 is done when `battlefield_paint` does not grow over the stress run, and the perf design asks for `settled_corpses_drawn` in stress. Neither number had been written down: the W1-49 commits had only headless cache tests.

**Conditions.** Commit `fa4c6f9` (W1-49 landed in `e208c55` and `5f733c4`). `measure_performance.ps1 -Scene stress -Profile dev -VsyncOff -NoBudget`, pipelining at its default (on), 180 warmup frames, one repeat per size. The runs used three windows: `-Frames 300` twice and `-Frames 3600` once. Intel Core Ultra 9 185H, mains, Balanced, 2026-11-07 01:47-01:49 UTC. Evidence is in `target/perf/w1-49-review/{frames-300,frames-300-b,frames-3600}`. Every run had 1 attempt, 0 frames over 50 ms, `present.source` `launch-override` (from -VsyncOff) and `present.pipelined_source` `settings`.

**Scene.** The stress run starts with 1,500 corpses and keeps up to 400 enemies on the field. It gains about 3 corpses in a 300-frame window and 13-14 in the 3,600-frame window.

| Window | Size | `battlefield_paint` mean ms | `settled_corpses_drawn` mean / max | `collapsing_corpses_drawn` mean / max | `scene_load.corpses` mean / max |
|---|---|---|---|---|---|
| 300 (`frames-300`) | normal | 0.557 | 1,501.1 / 1,503 | 0.69 / 3 | 1,501.8 / 1,503 |
| 300 (`frames-300`) | small | 0.503 | 1,500.4 / 1,503 | 0.77 / 3 | 1,501.2 / 1,503 |
| 300 (`frames-300-b`) | normal | 0.505 | 1,500.4 / 1,503 | 0.75 / 3 | 1,501.1 / 1,503 |
| 300 (`frames-300-b`) | small | 0.492 | 1,500.4 / 1,503 | 0.76 / 3 | 1,501.2 / 1,503 |
| 3,600 (`frames-3600`) | normal | 0.516 | 1,507.9 / 1,513 | 0.28 / 3 | 1,508.2 / 1,514 |
| 3,600 (`frames-3600`) | small | 0.477 | 1,507.3 / 1,513 | 0.29 / 3 | 1,507.6 / 1,513 |

**`battlefield_paint` does not grow over the stress run.** The 3,600-frame mean includes 3,300 later frames and 13-14 more corpses. It is 0.516 ms at the normal size and 0.477 ms at the small size, inside the 300-frame range at each size (0.505-0.557 and 0.492-0.503). The W1-61 acceptance stress runs (1,800 frames, dev, VSync off, `target/perf/w1-61/acceptance-dev-novsync` and `head-acc-dev-novsync`) agree, at 0.480-0.510 ms. The report gives only a mean for each window, not a per-frame series, so this compares windows of different lengths that start at the same frame.

**`settled_corpses_drawn` in stress.** Every corpse is drawn: about 1,500-1,508 on average, up to 1,513, from the cached settled mesh. The sampled collapsing set is at most 3 per frame. The difference between corpses and drawn corpses is the collapsing bodies (0.3-0.8 on average).

For context, the last stress runs before W1-48 and W1-49 (`target/perf/w1-37/dev-novsync`, dev, VSync off, about 1,514 corpses) painted the battlefield in 1.22-1.33 ms. Both commits landed between those runs and these, so the drop is not W1-49's alone.

## Full verify

From `horde-defense/`, before every production-pass commit:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked -p horde-defense --all-targets --no-default-features -- -D warnings
cargo clippy --locked -p horde-defense --all-targets --features dev -- -D warnings
cargo test --locked --workspace
cargo test --locked -p horde-defense --features dev
cargo run --locked -p horde-defense -- --smoke
python -I -B scripts/check_views.py
python -I -B scripts/tests/test_check_views.py
python -I -B scripts/generate_audio.py --check
python -I -B scripts/generate_atmosphere_audio.py --check
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/tests/run_script_tests.ps1
```

`check_views.py` prints `PASS: <n> Views` and `PASS: <n> modules to depth 2 each have an ARCHITECTURE.md ownership row` and exits 0; it exits 1 when a crate module or directory to depth 2 has no row in the ARCHITECTURE.md ownership table, a row names a missing module or file, a row's first cell backticks nothing or a `core`/`game` row names a path outside that crate, or a named guard test is not a Rust `#[test]` fn (a production fn of the same name does not count); `scripts/tests/test_check_views.py` exercises those rules on a throwaway workspace and exits 0 or 1. `run_script_tests.ps1` exercises `scripts/lib/Native.ps1` and `measure_performance.ps1` against a fake game executable under Windows PowerShell 5.1; it prints PASS or FAIL and exits 0 or 1. Documented `scripts/` commands use the Windows PowerShell 5.1 that ships with Windows (`powershell`, not `pwsh`). The three PowerShell 7 scripts (`capture_player_models.ps1`, `trace_npc.ps1`, `verify_player_models.ps1`) say so where they are documented. `repo_checks::docs_invoke_scripts_with_windows_powershell` enforces both outside `docs/plans/`.

The two generator checks re-render every clip the Python generators own into a temporary folder outside the workspace and compare SHA-256 hashes with the committed WAVs in `assets/audio/` and `archive/audio/`; the atmosphere check also compares both Python manifests (`assets/audio/PROVENANCE.json` and `archive/audio/PROVENANCE.json`) byte for byte. Each prints `PASS: ...` and exits 0, or prints one `FAIL:` line per mismatch and exits 1; neither writes anything, so `git status` stays clean. `--check` is the default; `--write` regenerates in place.
