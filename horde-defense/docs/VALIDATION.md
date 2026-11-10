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

Amended 8 November 2026 (W2-29 review): one exception to "actual terminals stop/reap/pay in their resolving fixed tick" now exists. A hostile a death burst defeats (ruling W-7) stops acting and cannot be targeted, but it stays on the field until the next tick's first reap, which removes and pays it. A save between the burst and that reap holds it defeated. Every other terminal still reaps and pays in its resolving tick.

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

**Debug recordings made under ruleset 1 are not expected to reproduce across the strict-sale change.** Replay still loads their initial state through the save migration, applies every recorded command and reaches the end line, but a recorded sale of an equipped, preset or recruit-equipped item is now a typed rejection, so the final digest can differ. Replay warns that the recording was made under ruleset 1 (and, for v6 headers, save version 6). `pre_strict_recording_diverges_with_ruleset_warning` pins this with the v6 equipped-sale fixture. The sale is not that fixture's only divergence: its 2,592 combat ticks no longer replay as on v6 either (expected cause: W1-69, enemies never block each other), so rebuilding the old sale legally still misses the recorded End digest. The test therefore also pins the current digest of the replay up to the sale, so any further drift fails it. Since W2-39 (country difficulty bands) the replayed Germany start plays difficulty 1.04 instead of 1.06 and its defender falls before the recorded withdrawal, so the recorded buy is refused for want of coins and the recorded equip and sale name an item that was never bought (`SaleBlocked(Unknown)`); the test then replays the recorded buy, equip and sale on that state with the coins in hand and checks that the equipped sale is refused (`SaleBlocked(EquippedPersonal)`) and the item kept. The pinned replay digest moved from fa528d88f6639802 to 711acf17d72357c2 at W2-39 and from 711acf17d72357c2 to 0e558e53029c6c41 at W2-09 (the weighted DropTable: the replayed deploys' drops are picked by drop-tier and trained-class weight with the same one draw; with neutral weights the replay digests to 711acf17d72357c2 again). Recording FORMAT stays 1.

**W1-69 does not bump the ruleset (R15, wave-1 close).** Enemies stopped queueing in W1-69, so a debug recording made before W1-69 can diverge in combat on replay. It still carries ruleset 2, so replay prints no ruleset warning for it. The W2-49 bump (2 -> 3) names both W1-69 (no lane queue) and the wave-2 change. From then on these recordings warn like any older ruleset.

**Ruleset 3 (W2-49, 10 November 2026).** `RULESET_VERSION` and `TRACE_RULESET` are 3. The bump names W1-69 (no lane queue) and the wave-2 rules ("Wave-2 re-bless and ruleset 3" below). A debug recording made under ruleset 2 (from W1-20 up to W2-49) now replays with the warning "recorded under ruleset 2, this build uses 3; the replay may diverge", and a save written under ruleset 2 (among them the frozen `save_v7.json`) loads with one drift warning per process. Neither is fatal. Recording FORMAT stays 1 and `SAVE_VERSION` stays 7.

## save_v6.json as cut — 5 November 2026 (W1-01, recorded 7 November in the W1 review)

`crates/horde_core/tests/fixtures/save_v6.json` does not match the plan in schema-v7.md §11 on four points. The plan has not been changed. Ruling R12 (wave-1 close, 7 November 2026) accepts the differences: the fixture is frozen evidence and the §11 numbers were estimates. schema-v7.md §11 carries the ruling as a note.

- **Size:** 120,842 bytes, against the planned "at most 64 KB". No test bounds the v6 fixture size; only `save_v7.json` has a byte bound (`V7_FIXTURE_MAX_BYTES`, 96 KB).
- **Player level:** `campaign.player.level` is 33, not the planned 12. The live Personal attempt is at rank 14 and tick 423, which meets "≥ 10" and "about tick 420".
- **Run history:** `campaign.run_history` holds 14 records, not 1 main and 1 worker: 13 main records (one Custom and twelve Hard; 12 Personal and 1 Npc) and 1 Normal worker record. All 14 are victories, and `campaign.victories` is 14.
- **Parent:** it was cut on parent `3d492cd`, not `7bd5de8`. `3d492cd` adds only wave-0 documents on top of `7bd5de8` and changes nothing under `crates/`, so the fixture is still a generation of untouched v6 code (SAVE_VERSION 6, RULESET 1).

These differences come from the `--smoke` bot driving the writer through `Game::apply` (b82658a). The v6 pins and load tests were recorded on this file, and v6 saves can never be generated again ("v6 fixture timing" in design-save-determinism.md), so it stays as cut.

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

`Campaign.run_history` is now an `Arc<Vec<RunRecord>>` (serde `rc`). A save snapshot shares it with the live game. Recording a run appends through `Arc::make_mut`: in place while no snapshot holds the history (`recording_a_run_without_a_snapshot_appends_in_place`), and as a copy while one does, so snapshots still in flight keep the old records (`consecutive_snapshots_share_unchanged_component`). (The measurements below were taken with the first form, an `Arc<[RunRecord]>` rebuilt on every recorded run; a snapshot clones either form as one reference count, and the W1 review switched to `make_mut` so that a settlement no longer copies the whole unbounded history.) It serializes as the same plain sequence. The v6 fixture payload is pinned by an FNV-1a hash cut before the change, and the checked-in `save_v7.json` (W1-63) re-saves to the payload its own envelope holds; both round-trip byte-identical (`save_v6_and_v7_fixtures_round_trip_byte_identical`). The synthetic stress fixture is in no digest. The benchmark's byte counts are unchanged, and `GOLDEN` and `GAMEPLAY_TRACE` are unchanged.

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

From now on a new saved field must be serde-defaulted, skipped at its default and load `save_v7.json` unchanged; otherwise the work stops and the user is asked (the only pre-approved exception is R5, `SkillModifiers.heavy_target_damage_multiplier`). Later cases use inline JSON or new `save_v7_w2_*.json` / `save_v7_w4_*.json` files. Through W1-68, `GOLDEN` and `GAMEPLAY_TRACE` moved 0/6 each. W1-69 (the user's 2026-11-06 decision that enemies overlap) is the wave-1 re-bless: `GOLDEN` 1/6 and `GAMEPLAY_TRACE` 3/6, class G, logged in `tests/determinism.rs`. `PROVISIONAL` is None, and the V1-V6 continuation pins, `V6_DROP_TRACE_FNV` and the `save_v7.json` pins did not move.

## The isolated 0.5 s frames (gate 3) — 6 November 2026 (W1-68)

**Finding, in two halves (narrowed by the W1-68 review).**
- **VSync on (the W1-67 stalls): a swap-chain acquire stall in the graphics driver or compositor of this hybrid-graphics laptop.** In 47 of the 56 stalled W1-67 frames the time sat in `get_current_texture`. These stalls come and go with the machine's state, not with the code.
- **VSync off (the W1-61 playtest stalls of 446-532 ms, the spec's own observation): not attributed.** They were not in the acquire call: all 16 frames over 50 ms had a present_wait of 0.09-0.20 ms. No span that the v3 report recorded at the time holds them. They did not recur under the W1-68 instrumentation, so the new gap fields have never seen one. Their cause is not named, and W1-68's done_when ("the cause is named with evidence") is not met for this half. **Closed by W1-61 (2026-11-06):** this half is the same swap-chain acquire stall. With `slowest_present`, all 220 VSync-off frames over 50 ms in the W1-61 A/B, pipelined or not, were acquire waits (acquire shares 0.981-0.992, in-process work 7.8-19.2 ms). Pipelined, the render frame that stalled was replaced before the main thread read it; W1-61 gives that as the reason these 16 frames showed a present_wait of 0.09-0.20 ms. See "Goal-1 acceptance" below.

Nothing in process was found to cause either. Gate 3 (`frames_over_50ms` 0) is unchanged. It needs a user ruling on two separate open items: the environmental VSync-on acquire stall, and the unattributed VSync-off stall, which needs a reproduction under the W1-68 probe. (Both settled since: the user ruling "Exclude driver stalls" of 2026-11-06 and W1-61 naming the VSync-off half.)

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

**Status: W1-68 is not done for the VSync-off half** (as recorded on 2026-11-06; closed by W1-61, which named it as the same swap-chain acquire stall, see "Goal-1 acceptance" below). Every fresh probe meets the done_when coverage (every frame over 50 ms covered to at least 95%) only trivially, because there were 0 such frames.
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
- `driver_stall_needs_a_frame_over_50ms` (W1 review): a 16.7 ms and a 45 ms frame that hold 90% of their window in the acquire are not stalls and stay in `outside`; 50.0 ms is not a stall, 50.1 ms is.
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

**Span boundaries changed after this acceptance (W1 review, 2026-11-07).** Every number in this section, `PERF_BUDGET.json baseline.v3` (W1-37, `2339a3d`) and every measurement from W1-37 to W1-61 used the probe spans from before review commit `7abb0ab` (2026-11-07 02:30, after `3f48d1a` and `6ddd359`):
- `egui_pass_ms` opened `.before(EguiPostUpdateSet::EndPass)`, at the start of PostUpdate, so it also held every PostUpdate system unordered against `EndPass` that ran first. From `7abb0ab` it opens in an exclusive system inside `EndPass`, directly before bevy_egui's `run_egui_context_pass_loop_system`. `7abb0ab`'s failing test measured 8.59 ms of egui_pass for a pass that held a sleeping unrelated system.
- `present_wait_ms` was not ordered against `create_surfaces`, so surface (re)configuration and the `PrepareViews` work before it could count as present wait. That work was then subtracted from render-world CPU. From `7abb0ab` the span opens `.after(create_surfaces)`, so that work counts as render-world CPU.

So `interface_total` (egui_pass + tessellate) and `cpu_frame` (main + extract + Render schedule - present wait) from `7abb0ab` on are not like-for-like with the numbers above or with baseline.v3. Under the new spans, interface_total can only be the same or lower. That makes the `interface_total_not_above_baseline_v3` gate looser than it was at W1-61. cpu_frame can only be the same or higher, so the W1-61 cpu_frame p99 values (6.26-8.85 ms) may leave out work that the gate now counts. The driver-stall acquire wait reads the same present span. The first acceptance measurement on the new spans is "Goal-1 acceptance re-run at HEAD" below; baseline.v3 itself has not been re-measured. Before the next interface_total re-gate (W2-47, W5-11, W7-09), the orchestrator or the user must decide whether baseline.v3 is re-measured on the new spans. Plan §2.5 tracks this. (R29, 2026-11-09: re-measured in W2-47; see "baseline.v3 on the 7abb0ab spans" below.)

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

**Open (W1-61 review, 2026-11-07).** That repeat ran the shipped default configuration (playtest, pipelined on, VSync on). Outside driver stalls it had 6 frames over 50 ms, 407 frames over 33 ms, a p99 interval of 47.1 ms and a mean of 46.3 FPS. That is a sustained 15-20 FPS episode, not an isolated stall. Its twins were clean, it is not attributed (4-6 ms per frame outside App::update), and gate 3 on HEAD is not claimed clean while it stands. It is an open item (W1-68's VSync-off half is closed by W1-61, above; plan 9.4), scheduled as W2-50 (perf-architecture, before the W2-47 re-gate; waves.md) and tracked in plan §2.5.

**The acceptance passes and the A/B disagree on driver stalls (W1-61 review, 2026-11-07).** Both ran the same commit, 1,800 stress frames, pipelining on. The 12 acceptance stress runs with -VsyncOff (dev and playtest) had 0 driver stalls; the 12 pipelined-on A/B stress runs had 58 (0-12 each). The harness differs only in `-Pipelined on` (`LASTLIGHT_PERF_PIPELINED=1`, which sets the same plan value; `present.source` is `launch-override` either way because of -VsyncOff) and `-Repeat 1` per invocation. One discriminating pass ran both, interleaved, on 2026-11-06 23:10-23:16 UTC (`target/perf/w1-61/review/`; playtest, -VsyncOff, stress, both sizes, 1,800 frames):

| Pass | Harness | Runs | Frames > 50 ms | Driver stalls |
|---|---|---|---|---|
| `a1-on-r1`, `a2-on-r1` | `-Pipelined on -Repeat 1` (A/B) | 4 | 0 | 0 |
| `b1-default-r3` | default, `-Repeat 3` (acceptance) | 6 | 0 | 0 |
| `c1-on-r3` | `-Pipelined on -Repeat 3` | 6 | 0 | 0 |
| `d1-default-r1` | default, `-Repeat 1` | 2 | 0 | 0 |

No harness variable brings the stalls in. They come and go with time. The last driver stall in the W1-61 evidence fell in the VSync-on A/B at about 22:22 UTC; none followed in the 54 acceptance runs from 22:32 or in these 18 runs. W1-68 saw the same thing: 4-14 per stress run at 18:09, then 0 in 66 probes. So the A/B on arm stands for the default. The acceptance pass's 0 driver stalls shows a stall-free hour, not that the stall is gone; on another hour the same passes would exclude stalls under the ruling. Caveats: the discriminating pass ran the W1-61 review binary (it differs only in the stall rule) and had no CPU-load sampler. (W2-50, 2026-11-08: the stalls come from the display being off, not from the hour; see "Gate 3 in the shipped default configuration" below.)

## Goal-1 acceptance re-run at HEAD — 7 November 2026 (W1 review)

**Conditions.**
- Commit 867f786, a clean tree. The W1-61 passes with the same harness and gates, on the probe spans from `7abb0ab` (see "Span boundaries changed after this acceptance" above).
- Intel Core Ultra 9 185H, NVIDIA RTX 1000 Ada Generation Laptop GPU, Vulkan, mains, power plan Balanced, a focused window at 1440x900 @ 1.0 and 1050x700 @ 1.35. 2026-11-07, 17:07-17:30 UTC.
- Before each pass, `tasklist` showed no horde-defense.exe, and no other cargo command ran. A 5 s `Win32_Processor.LoadPercentage` sampler ran beside every pass (`target/perf/w1-review/<pass>.load.tsv`). Samples of 97% and 94% fell in cargo builds, before the first run of their pass.
- Every run was valid at its first attempt. Evidence: `target/perf/w1-review/acc-*`, each with summary.json and summary.md.

| Pass | Command | Exit | Gate | normal | small | Limit | Driver stalls excluded | Load during runs, max / mean of pass |
|---|---|---|---|---|---|---|---|---|
| `acc-dev-vsync-progressed-peak` | `-Scene progressed-peak -Profile dev -Repeat 3` | 0 | interface_draw mean (ms) | 1.0386 | 1.0117 | ≤ 1.129 / 1.030 | 0 | 47% / 29% |
| | | | interface_total mean (ms) | 1.9647 | 1.8655 | ≤ 4.6447 / 4.5895 (v3, old spans) | | |
| `acc-dev-novsync` | `-Scene all -Profile dev -VsyncOff -Repeat 3` | 0 | cpu_frame p99 progressed-peak / stress (ms) | 6.80 / 7.58 | 6.47 / 7.13 | ≤ 10.0 | 0 | 67% / 33% |
| `acc-playtest-novsync` | `-Scene all -Profile playtest -VsyncOff -Repeat 3 -ShippedSmoke` | 0 | cpu_frame p99 progressed-peak / stress (ms) | 5.91 / 6.48 | 5.61 / 6.09 | ≤ 10.0 | 35 | 59% / 35% |
| `acc-dev-vsync-peak` (extra) | `-Scene peak -Profile dev -Repeat 3 -NoBuild` | 0 | interface_draw mean (ms) | 0.7837 | 0.7738 | ≤ 1.1904 / 1.0752 | 5 | 51% / 22% |
| `acc-dev-vsync-stress` (extra) | `-Scene stress -Profile dev -Repeat 3 -NoBuild` | 1 | frames > 50 ms outside stalls (worst) | 1 | 0 | 0 | 40 | 46% / 28% |

No CPU p99 cell missed, so no pass was re-run. Peak cpu_frame p99 with -VsyncOff (not gated) was dev 6.34 / 6.16 and playtest 5.51 / 5.26 ms.

**Gate 3.** The three gate passes had 0 frames over 50 ms outside driver stalls in every repeat. Unlike the W1-61 acceptance hour, the stall was active:
- The playtest -VsyncOff pass excluded 35 stalls: 494-531 ms, acquire shares 0.987-0.990, in-process work 7.3-13.9 ms, named coverage at least 0.999. The dev -VsyncOff pass, which ran just before it, had none.
- The extra dev VSync-on passes excluded 5 (peak, 525-567 ms) and 40 (stress, 84-1,001 ms, acquire shares 0.868-0.994). Seven of the stress stalls were about 1,000 ms, wgpu-core's acquire timeout.

**The one gated frame** is in `acc-dev-vsync-stress/stress-normal-r1.json`, sample 932:
- 54.74 ms, with a present wait of 50.61 ms, a channel wait of 46.76 ms and an acquire wait of 43.46 ms. That is an acquire share of 0.794, under the 0.80 threshold.
- Named coverage 0.989. In-process work about 11 ms (main 7.05, render 3.92). No window, asset, audio or texture-upload events.
- It comes directly after the stalls at samples 929-931 (998-1,001 ms each). Outside stalls, that repeat had 9 frames over 33 ms, a p99 interval of 19.4 ms and 59.6 FPS.

Its five twins (normal r2-r3, small r1-r3) were clean. It is the tail of a driver stall, not in-process work. The gate was not moved. With the W1-61 episode (`head-ab-playtest-vsync/stress-on-r2`), it is the second gated VSync-on stress frame set on this branch, so gate 3 on HEAD is still not claimed clean. Both are for W2-50.

**Against W1-61** (`199d823`, old spans):
- progressed-peak VSync-on interface_draw rose 4.9% / 6.3% (0.990 / 0.952 to 1.039 / 1.012). That leaves 8.0% / 1.8% headroom. The same scene's -VsyncOff twins fell (dev 0.952 / 0.880 to 0.909 / 0.872 ms; playtest 0.738 / 0.739 to 0.671 / 0.671 ms), and peak with VSync on rose 1.2% / 0.7%. So the rise shows only in the VSync-on cell, and is not a draw regression on this evidence. The small size is close to its gate, so W2-47's re-gate should watch it.
- interface_total (now narrower spans) is 1.965 / 1.866 against 1.952 / 1.805.
- cpu_frame p99 (now wider spans) is lower in every cell but stress normal: dev 7.50 to 7.58 ms, playtest 6.477 to 6.481 ms.

**Shipped smoke** (`acc-playtest-novsync/shipped`): 2.79, 1.23 and 1.19 s, median 1.23 s, the same as W1-61. The first, cold run is again the slow one. Low latency was not re-run; its W1-61 evidence (`head-low-latency-playtest`) stands. The numbers are in `PERF_BUDGET.json` `acceptance` (the W1-61 ones under `acceptance.w1_61`) and plan 9.1 / 9.4.

## Goal-1 acceptance re-run at HEAD, attempt 2 — 7 November 2026 (W1 review)

**Conditions.**
- Commit bee4236, a clean tree. The commits since 867f786 change docs only; the dev executable hashed the same as in the first re-run (`77b67433…`). The three W1-61 gate passes, same harness, gates and probe spans (`7abb0ab`).
- Same hardware and state as the first re-run: RTX 1000 Ada, Vulkan, mains, power plan Balanced, a focused window at 1440x900 @ 1.0 and 1050x700 @ 1.35. 2026-11-07, 17:42-17:56 UTC.
- Before each pass, `tasklist` showed no horde-defense.exe, and no other cargo command ran. A 5 s `Win32_Processor.LoadPercentage` sampler ran beside every pass (`target/perf/w1-review2/<pass>.load.tsv`). The 81% and 94% samples of the playtest pass fell before its first measured run (shipped build and smoke, review build).
- Every run was valid at its first attempt. Evidence: `target/perf/w1-review2/acc-*`, each with summary.json and summary.md.

| Pass | Command | Exit | Gate | normal | small | Limit | Driver stalls excluded | Load during runs, max / mean of pass |
|---|---|---|---|---|---|---|---|---|
| `acc-dev-vsync-progressed-peak` | `-Scene progressed-peak -Profile dev -Repeat 3` | 0 | interface_draw mean (ms) | 0.9976 | 0.9889 | ≤ 1.129 / 1.030 | 15 | 55% / 25% |
| | | | interface_total mean (ms) | 1.8981 | 1.8237 | ≤ 4.6447 / 4.5895 (v3, old spans) | | |
| `acc-dev-novsync` | `-Scene all -Profile dev -VsyncOff -Repeat 3` | 0 | cpu_frame p99 progressed-peak / stress (ms) | 6.77 / 7.30 | 6.63 / 7.28 | ≤ 10.0 | 48 | 60% / 28% |
| `acc-playtest-novsync` | `-Scene all -Profile playtest -VsyncOff -Repeat 3 -ShippedSmoke` | 0 | cpu_frame p99 progressed-peak / stress (ms) | 6.25 / 6.43 | 5.92 / 6.28 | ≤ 10.0 | 48 | 52% / 27% |

No CPU p99 cell missed, so no pass was re-run. Peak cpu_frame p99 with -VsyncOff (not gated) was dev 6.54 / 5.89 and playtest 5.79 / 5.23 ms. The extra dev VSync-on peak and stress passes of the first re-run were not repeated.

**Gate 3.** All three passes had 0 frames over 50 ms outside driver stalls in every repeat, with the stall active in every pass:
- dev VSync on excluded 15: 69-1,000 ms, acquire shares 0.867-0.993, in-process work 8.8-11.3 ms, named coverage at least 0.992. Five were 955-1,000 ms, wgpu-core's acquire timeout. The 85 and 69 ms ones were the frames directly after a 1,000 ms stall (normal r2 sample 596, small r2 sample 328), the same position as the 867f786 sample-932 frame, but with acquire shares of 0.869 and 0.867 they cleared the 0.80 rule.
- dev and playtest -VsyncOff excluded 48 each: 487-612 ms, acquire shares 0.983-0.991, in-process work 6.7-12.5 ms, named coverage at least 0.999.

This is a pass of the three gates, not a clean gate 3 on HEAD: the 867f786 stress frame (sample 932) and the W1-61 `stress-on-r2` episode stay open for W2-50.

**Against the first re-run** (867f786, same dev executable):
- progressed-peak VSync-on interface_draw fell 3.9% / 2.3% (1.039 / 1.012 to 0.998 / 0.989), leaving 11.6% / 4.0% headroom. With the executable unchanged, the 867f786 rise over W1-61 was run-to-run spread, as that section suspected; the small size remains the closest cell to its gate.
- cpu_frame p99 moved by -0.28 to +0.34 ms per cell; the worst cell (dev stress normal, 7.30 ms) is 2.70 ms under the gate.

**Shipped smoke** (`acc-playtest-novsync/shipped`): 2.72, 1.25 and 1.26 s, median 1.26 s. The cold first run is again the slow one. Low latency was not re-run. The numbers are in `PERF_BUDGET.json` `acceptance` (the first re-run under `acceptance.w1_review_867f786`, W1-61 under `acceptance.w1_61`) and plan 9.1 / 9.4.

## Gate 3 in the shipped default configuration — 8 November 2026 (W2-50)

W2-50 owns two open gate-3 items: the W1-61 `head-ab-playtest-vsync/stress-on-r2` episode and the 867f786 sample-932 stall tail (plan §2.5). No gate, threshold or rule was changed. Evidence: `target/perf/w2-50/` (not committed). It holds the reports and summaries, a 5 s CPU-load sampler (`<run>.load.tsv`) and a 1 s `nvidia-smi` sampler (`<run>.gpu.csv`) beside every invocation, the Kernel-Power 506/507 export (`kernel-power-506-507*.tsv`, display-off intervals in `standby-2026-11-06-to-09-b.tsv`), the generated tables (`w2-50-repeats.md`, `w2-50-hold-12.md`, `w1-evidence-display-state*.md`, `w1-67-display-state.md`, `w1-68-display-state.md`) and the driver scripts (`tools/`).

**Conditions.**
- Commit ddea5a6 (clean tree) for the first 20 invocations; `hold-r5..r12` ran at 8efd9ff with uncommitted review edits to docs only, so on the same playtest executable `2de00a0b…` (built once, `-NoBuild` after). RTX 1000 Ada, Vulkan, mains, power plan Balanced, 1440x900 @ 1.0 and 1050x700 @ 1.35.
- The shipped default configuration: `measure_performance.ps1 -Scene stress -Profile playtest -Pipelined on -Repeat 1`, VSync on (Fifo), stress normal, 180 warmup and 1,800 frames. This is the W1-61 A/B harness, one invocation per repeat.
- 2026-11-08, 22:32-23:10 and 23:25-23:36 UTC. No other horde-defense.exe and no other cargo command ran. CPU load during the measured runs was at most 61% (100% and 90% samples fell in the first invocation's cargo build).
- Every run passed the harness's validity checks at its first attempt. Under W2-50's own recommendation below (a run that overlaps a display-off interval is invalid evidence), only the 14 per size that ran with the display on throughout would count.

**Runs.** There were 28 invocations, so 28 repeats per size, 56 reports in all:
- `stress-on-r1..r12`: 12 plain repeats.
- `stress-on-pointer-r1..r4`: meant to move the pointer over the window. The lock screen was in front (`GetForegroundWindow` returned LockApp, `SendInput` returned 0), so no move was made. These 4 are plain repeats too.
- `stress-on-hold-r1..r12`: the same harness with a display-required power request (`SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED)`) held by the driver process (`tools/w2-50-hold.ps1`). `r1..r4` ran at 23:04-23:10, `r5..r12` at 23:25-23:36 (added in the W2-50 review, so that at least 12 repeats per size ran with the display on).

The machine was unattended. Windows' Kernel-Power log shows that Modern Standby (display off) was entered on the idle timeout (event 506) and left on input (event 507) several times during the plain runs: 22:37:53-22:39:29, 22:39:59-22:53:06, 22:54:00-22:56:16 and 22:57:08-22:58:28 UTC. Of the 16 plain repeats per size, only `r1` (both sizes), `r2` normal and `pointer-r3` small ran with the display on throughout; `r2` small ended at the 22:37:53 display-off. With the 12 held repeats, 14 repeats per size ran with the display on throughout. `w2-50-repeats.md` places every listed frame on that timeline. Each instant is estimated from the report's file time; "transition" means within 5 s of a 506/507 event.

**Per repeat** (`w2-50-repeats.md`, `w2-50-hold-12.md`; all VSync on). The frame, p99 and FPS columns are outside driver stalls. The last three columns are the report's `slow_frames`, which covers every frame over 50 ms, driver stalls included:

| Runs | Display | Frames > 50 ms (worst) | Frames > 33 ms | p99 interval ms | Mean FPS | Driver stalls | covered_95_named / all frames > 50 ms, stalls included | min named_coverage, stalls included | max_outside_update_ms, stalls included |
|---|---|---|---|---|---|---|---|---|---|
| r1-r12 normal | on in r1, r2; off or transition in r3-r12 | 0 | 0-1 | 17.41-18.92 | 59.85-60.03 | 0-6 (34) | all | ≥ 0.998 | 0.85 |
| r1-r12 small | on in r1; transition in r2; off in r3-r12 | 0 | 0-1 | 19.70-20.18 | 59.93-60.05 | 0-6 (34) | all | ≥ 0.998 | 0.77 |
| pointer r1-r4 normal | off, with transitions in r1 and r3 | 1 (r1, r3) | 0-2 | 17.64-18.14 | 59.58-60.06 | 3-5 (16) | 3 of 4 (r1), all | 0.932 | 15.81 |
| pointer r1-r4 small | r1 transition; r2 off; r3 on; r4 off, ending in a transition | 1 (r1) | 0-1 | 19.90-20.11 | 58.42-60.01 | 0-7 (11) | 7 of 8 (r1), all | 0.000 | 818.71 |
| hold r1-r12 normal | on (held) | 0 | 0 | 17.58-18.14 | 60.00 | 0 | - | - | 0.00 |
| hold r1-r12 small | on (held) | 0 | 0 | 19.50-20.20 | 59.99-60.00 | 0 | - | - | 0.00 |

Every frame over 33 ms outside driver stalls (all 6 are in `longest_frames`; window events are 0 in every W2-50 frame and run):

| Run | Sample | ms | Acquire share | Named coverage | Outside update ms | Display | Reading |
|---|---|---|---|---|---|---|---|
| pointer-r1 normal | 602 | 234.0 | 0.847 | 0.932 | 15.8 | transition (22:53:06, display on) | second frame of a two-frame stall (s601 291.7 ms excluded); the 15.8 ms the main thread spent outside App::update after s601 falls in its window, so named coverage misses 0.95 |
| pointer-r3 normal | 1481 | 65.9 | 0.660 | 0.951 | 3.2 | transition (22:56:16, display on) | stall tail like sample 932: two frames after a 277 ms stall, present wait 52.9 ms, of which only 43.5 ms overlaps a hand-off, because the main thread ran two schedules (16.2 ms) in the window |
| pointer-r1 small | 1345 | 819.0 | 0.000 | 0.000 | 818.7 | transition (4 s before the 22:54:00 display-off) | the render thread waited 815 ms in acquire while the main thread sat outside App::update the whole time, never in a hand-off, so the rule sees no acquire wait |
| r4 normal | 1393 | 37.3 | 0.000 | 0.321 | 25.4 | transition (4 s before 22:39:59) | two frames after a 917 ms stall: the main thread waited outside the update |
| pointer-r3 normal | 1480 | 39.1 | 0.000 | 0.736 | 10.3 | transition (22:56:16) | between the 277 ms stall and s1481 |
| r1 small | 628 | 45.2 | 0.855 | 0.989 | 0.4 | on | one three-vblank acquire, the episode's per-frame signature; isolated |

**The driver stall is the display being off.** `w1-evidence-display-state*.md` puts the W1-61 and W1-review evidence on the same Kernel-Power timeline:
- W1-61's VSync-off A/B ran 21:57-22:15 UTC on 2026-11-06, in a display-off interval (21:33:17-22:17:09). So did the first VSync-on A/B progressed-peak runs (22:15-22:17). Every one of their stalls is display-off.
- The user woke the laptop by touchpad at 22:17:09. The display then stayed on through 2026-11-07 09:08, which covers the acceptance passes (22:32 onwards) and the discriminating pass (23:10-23:16). That is the "stall-free hour" of "The acceptance passes and the A/B disagree on driver stalls" above.
- On 2026-11-07 the display went off at 17:18:23, during the 867f786 re-run. The passes before it (dev VSync-on progressed-peak, dev -VsyncOff) had 0 stalls; every pass after it had stalls, and attempt 2 (17:42-17:56) ran with the display off throughout. The extra dev VSync-on peak pass of the first re-run (`w1-review/acc-dev-vsync-peak`, 17:24-17:26, `w1-evidence-display-state-3.md`) ran with the display off; its 5 stalls are display-off.
- W1-67/W1-68 on 2026-11-06 (`w1-67-display-state.md`, `w1-68-display-state.md`; local 18:06-19:06 is 17:06-18:06 UTC). The W1-67 dev VSync-on passes (`w1-67/after-dev-vsync`, `after-dev-vsync-stress`, 17:06-17:13 UTC) ran in a display-off interval (16:45:29-17:27:58): all 56 of their frames over 50 ms are display-off. These are v3 reports with no driver-stall class, so they are counted as frames over 50 ms, not as classed stalls. All 66 W1-68 probes (17:31-18:06 UTC: 54 on the W1-68 build and 12 on the W1-67 binary itself) ran with the display on throughout and had 0 frames over 50 ms. That is the "4-14 per stress run at 18:09, then 0 in 66 probes" of the W1-61 review.

Across those 186 reports (the W1-61 A/B, the W1-61 playtest acceptance and every pass of both 7 November re-runs) and W2-50's 56, 521 frames were classed as driver stalls:
- 518 were display-off frames or fell within 5 s of a display power transition.
- The 3 others are the 116, 52 and 53 ms frames of `stress-on-r2`.
- No stall of the 500 ms or 1,000 ms class (wgpu's acquire timeout) occurred with the display on. The 56 W1-67 frames over 50 ms add no display-on frame either.

The perturbation proof is the hold arm. Before it, the lock-screen idle timeout turned the display off four times in 7 minutes, and between the two hold batches (23:10-23:25, no request held) it did so 11 more times. With the display-required request held, no 506 event fired from 23:04:46 to 23:10:06 or from 23:25:31 to 23:36:21. The 24 held runs had 0 driver stalls and 0 frames over 33 ms, against 95 stalls in the 32 plain runs. So the excluded stalls are environmental, as the ruling assumed. They come from unattended evidence passes on which Windows turns the panel off, not from the game. Frames still run at a 16.7 ms median then, but the swap-chain acquire stalls for about 500 or 1,000 ms at a time.

**Sample 932 (867f786 re-run).** It fell at about 17:26:44 UTC, with the display off since 17:18:23. It is a display-off stall tail. W2-50 found the same class again (pointer-r3 s1481, share 0.660), plus two other ways a stall escapes the rule at a display power transition:
- the main thread outside the update for 15.8 ms in a stall's second frame (named coverage 0.932);
- the main thread outside the update for all of an 819 ms acquire (acquire share 0).

Counting the whole present wait toward the share would catch s1481 (52.9 / 65.9 = 0.80) but neither of the other two. With the display held on, none of these frames occurs. The rule is unchanged and sample 932 stays gated (the §2.5 default). The decision is the user's. W2-50's recommendation is to keep the `driver_stall` rule and instead hold the display on during perf passes and treat a run that overlaps a display-off interval as invalid evidence. That is a harness change to evidence validity and needs its own ruling (plan §2.5).

**The `stress-on-r2` episode is not reproduced.** None of the 28 repeats per size had a stretch of 47-67 ms frames: not the 14 per size with the display on throughout (12 of them held on, 2 plain), nor the 14 per size that were display-off or crossed a transition. The longest run of slow frames outside a stall transition was one isolated 45.2 ms frame (r1 small s628). What the evidence attributes:
- The time is held by the swap-chain acquire on the render thread. present_wait was 41.8-64.1 ms per frame, so the frame waited about three vblanks. In-process work was 6-8 ms (main 3.2-5.0, render 2.5-3.4), with no asset, audio or texture-upload events.
- The 4-6 ms outside App::update that kept named coverage at 0.89-0.94 comes only in frames that carry window input events (21-52 per frame). Frames of the episode without events had 0.4 ms outside the update and named coverage 0.99 (s971, s1081, s1288, s1301, s1319, 47.2-48.1 ms). So that residual is winit dispatching input, not the cause of the slow cadence.
- `stress-on-r2` is the only report on the branch with sustained window input: 4.74 events per frame on average and 52 at most. Every other W1-61 VSync-on stress report has a mean of 0.00-0.01, and every W2-50 report has 0. It ran at 22:22 UTC, 5 minutes after the user woke the laptop (Kernel-Power 507, "Input Touchpad", 22:17:09), with the display on.

The episode therefore coincides with someone using the machine during the run. Input alone is not enough: `progressed-peak-on-r3` (22:18, 7.65 events per frame, 22 at most) stayed at 60 FPS. What made the compositor present on every third vblank for those 400 frames is not named, and the W2-50 runs had no input, because the session was locked. A supervised reproduction, with the session unlocked and pointer movement over the window during `-Scene stress -Profile playtest -Pipelined on` runs, is the remaining step. Until the §2.5 row is answered, the episode stays open (the default). Gate 3 is not claimed clean in the shipped default configuration.

**Rulings (2026-11-09, user decisions R26 and R27; plan §2.4).** Both §2.5 rows are answered:
- **R26: the `stress-on-r2` episode closes as environmental.** It was user activity during an evidence run: the laptop had been woken 5 minutes before, and the run carried window input. It was not reproduced in 28 repeats per size, 14 of them with the display on throughout. No supervised reproduction is run. Gate 3 in the shipped default configuration can be claimed once the W2-47 re-gate passes; until then it is not claimed.
- **R27: the 80% driver-stall rule stays unchanged, and sample 932 is closed as display-off.** Instead, `measure_performance.ps1` holds the display on for the whole run (`SetThreadExecutionState` with ES_CONTINUOUS, ES_SYSTEM_REQUIRED and ES_DISPLAY_REQUIRED, the W2-50 hold arm's request, released in a finally block). A run that overlaps a display-off interval, meaning a System log Kernel-Power event 506 or 507 between its start and end, is invalid evidence and is rerun once like the script's other invalid runs (exit 2 if the rerun is invalid too). The summary records the display hold and any 506/507 events per run. W2-50's Done is met once that harness change lands, and the W2-47 re-gate runs on it.
- **R27 landed (2026-11-09, `feat(horde): measure_performance holds the display on and rejects display-off runs [W2-50]`).** `scripts/lib/DisplayPower.ps1` holds the request and queries the System log (`Get-DisplayPowerEvent`, the function the script tests shim). Each run's window is its process start to exit, and a 506 or 507 inside it, or a log that cannot be read, adds the problem `display off during the run (Kernel-Power <id> at <UTC>)` or `display power unknown (...)`. summary.json gains `display_hold` (request, set and release times, and `released_from`, the state the release replaced: 0x80000003 while held) and per run `display_held` and `display_power_events` (with the attempt); summary.md gains a Display column. `scripts/tests/run_script_tests.ps1` covers it with six cases (37 in all): the hold's flags and single release, the event query and its no-match answer, the problem text, and three end-to-end runs with `Get-WinEvent` shimmed (an overlapping run invalid and rerun, or exit 2 when invalid twice; a run without an event valid; the hold released when a run throws). W2-50's Done is met.

## Machine load in perf evidence — 9 November 2026 (R30, W2-47)

**Ruling (R30, user decision, plan §2.4).** Until now the operator kept the machine quiet, and the W1-61 to W2-50 passes ran a hand-made 5 s CPU-load sampler beside them (`target/perf/<pass>/<run>.load.tsv`). `measure_performance.ps1` now checks both itself. Gates are unchanged.

**R30 landed (2026-11-09, `feat(horde): perf passes wait for a quiet machine and reject loaded runs [W2-47]`).**
- **Quiet window.** After each build and before what it measures (the runs, the `-ShippedSmoke` timing and the `-SaveClone` example), the script waits for a quiet window. No foreign cargo, rustc, cargo-nextest, clippy, rustdoc, linker or MSBuild process may run; the script's own process tree (its build, the game) is ignored. The mean of the last 13 CPU-load samples (`Win32_Processor.LoadPercentage`, every 5 s, so 60 s) must be under 35%. A sample with a foreign build or an unreadable query restarts the window.
- **Switches.** The window is on by default (`-WaitQuiet` may be given; `-NoWaitQuiet` skips it). `-QuietTimeoutMinutes` defaults to 240 (1-1440).
- **Log and timeout.** The script prints a `quiet window:` line whenever what it waits for changes (the foreign processes with their ids, the settling window, the mean load). A window that has not come in time prints `INVALID quiet window timed out ...` and exits 2.
- **Load validity.** `Invoke-Native -Sample` samples the load right after each run starts and every 5 s until it exits. A run with a sample of 90% or more is invalid evidence (`CPU load <max>% during the run (...; samples ...)`), and so is one with an unreadable sample. It is rerun once like the other invalid runs, and exit 2 follows if the rerun is invalid too.
- **Summary.** summary.json gains `quiet_window` (the rule, the timeout and every wait: before what, quiet or timed out, samples, seconds, the window's mean and max, what it waited for and the log lines). Each run gains `load_samples`, `load_mean_percent` and `load_max_percent` (the kept attempt) and `load_attempts` (every attempt). summary.md gains a line per wait and a CPU load column.
- **Tests.** `scripts/lib/MachineLoad.ps1` holds the process and load queries (`Get-ProcessTable` and `Get-CpuLoadPercent`, both `Get-CimInstance` calls), the window (`Wait-QuietMachine`) and the run check (`Test-RunLoad`). `scripts/tests/run_script_tests.ps1` shims `Get-CimInstance` and `Start-Sleep`, the `DisplayPower.ps1` pattern. It adds 12 cases (49 in all): the process-tree filter, the load mean, a quiet start, waiting out a foreign build and a loaded CPU, a timeout, the 90% rule, and the sampler in `Invoke-Native`. The end-to-end runs cover a quiet start, a foreign build waited out while the script's own children are ignored, a timeout exiting 2, a run invalid on a 95% sample and rerun (and exit 2 when every sample is 95%), and the switches. The measure runs of the earlier cases now go through the same shim wrapper, on a quiet machine (load 10%, no foreign build).

## interface_total A/B against the wave-1 executable — 9 November 2026 (R31, W2-47)

**Ruling (R31, user decision, plan §2.4).** The interface_total re-gate is an interleaved A/B against the wave-1-close executable, not the absolute `baseline.v3` (R29 still re-measures and records it). In one sitting A (sources at ab03da9) and B (HEAD) alternate, 5 pairs per size, on progressed-peak, dev, VSync on and the default pipelined state. Per size, B's median interface_total must be at most 1.05 × A's median. This answers W2-47's "interface_total re-gate needs a tolerance" item.

**Landed (2026-11-09, `feat(horde): interleaved A/B interface_total gate against the wave-1 executable [W2-47]`).** No A/B tooling existed under `scripts/` (the W1-61 head-ab passes were run by hand), so it is a new script, `scripts/ab_interface_total.ps1`.
- **Build.** A is `git archive --format=zip ab03da9 horde-defense`, extracted into `<target>/ab-w1/src` (reused while `.ab-commit` names the same commit). It is built there with the same CARGO_TARGET_DIR, and both builds write `<target>/debug/horde-defense.exe`. (Corrected in W2-47: the workspace crates do not get their own fingerprints by path. Cargo keeps one fingerprint for both arms' packages, so without a clean the second build is fresh and leaves the first arm's exe in place; see "The first A/B measured A twice" below.) So A's exe is copied to `<out>/exe-a/debug/` before B is built and copied to `<out>/exe-b/debug/`. A copied exe has no `assets` folder beside it, so each reads the assets of the tree it was built from. `-NoBuild` with `-ExecutableA` and `-ExecutableB` skips both builds.
- **Sitting.** One R30 quiet window first (`-NoWaitQuiet` skips it, `-QuietTimeoutMinutes` 240, a timeout exits 2), with the display held for the whole sitting. Then, for each pair, A then B: each arm is one `measure_performance.ps1 -Scene progressed-peak -Profile dev -NoBuild -NoBudget -NoWaitQuiet` pass at both sizes, in this process, with CARGO_TARGET_DIR at the arm's folder. So every run keeps that script's validity (report, window size, the R27 display check, the R30 load sampler) and its one rerun. `Exit-DisplayHold` now restores the state its hold replaced, so a pass's release keeps the sitting's hold.
- **Decision.** `scripts/lib/AbInterfaceTotal.ps1`. A run counts when measure_performance found it valid and it echoes `baseline.v3`'s present mode (Fifo) and pipelined state (true) with an interface_total mean. Per size: pass when B's median ≤ 1.05 × A's median, over (exit 1) above it, and invalid evidence (exit 2) when any pair has an invalid or missing run. A build or run failure exits 3.
- **Evidence.** `<out>/ab.json` and `ab.md` record the arms (commit, sha256), the quiet window, the order, every run and each size's A and B values, medians, ratio, limit and status. Each arm's pass keeps its own `pair-<n>-<a|b>/summary.json`.
- **Tests.** `scripts/tests/run_script_tests.ps1` adds 9 cases (58 in all). Fixture decisions cover a pass (one size at exactly 1.05 × A), a fail and an invalid or missing pair. The summary conversion rejects another present mode or pipelined state. The nested display hold is covered. End to end, with both arms copies of the fake game reporting their own interface_total, a pass, a fail, an invalid pair (B's normal run at 95% CPU twice), the bad arguments and a quiet-window timeout are covered.
- **Not run here.** The A/B gate itself (a cold build of the ab03da9 tree and 20 runs) runs with the W2-47 re-gate on a quiet machine.

**The first A/B measured A twice (W2-47, 2026-11-09 23:20-23:48 UTC, `target/perf/w2-47/ab-interface-total`).** It exited 0, but `exe-a` and `exe-b` have the same sha256 (`93997478…`), and every B report lacks the battlefield counts HEAD writes (W2-47's live battlefield gates), so both arms ran ab03da9. A's build compiled the ab03da9 tree (5 min 24 s); B's build of HEAD then finished in 3.3 s as fresh and left A's `<target>/debug/horde-defense.exe` in place. Cargo keeps one fingerprint and one uplifted exe for the same workspace package names and profile, whatever tree they come from, so the second arm's build checks the first arm's sources. The result is not evidence and is not used.
- **Fix** (`fix(horde): ab_interface_total cleans each arm's packages and refuses identical arms [W2-47]`): `Invoke-AbArmBuild` (`scripts/lib/AbInterfaceTotal.ps1`) runs `cargo clean -p horde-defense -p horde_core` in the arm's tree before `cargo build --locked -p horde-defense`, for A and for B, still in the same CARGO_TARGET_DIR (R31). `Assert-AbArmsDiffer` makes an A and B with the same sha256 a run failure (exit 3) before the quiet window and any pair.
- **Tests.** `scripts/tests/run_script_tests.ps1` adds 3 cases (61 in all): the clean-then-build order in the arm's tree, the guard, and an end-to-end run with one executable for both arms that exits 3 with no pair run. The end-to-end A/B cases now give B a second build of the fake game, so the arms differ.
- **The workspace target after the sitting (W2-47 review, `fix(horde): ab_interface_total leaves no wave-1 build in the workspace target [W2-47 review]`).** The fix above still left one path where the shared CARGO_TARGET_DIR kept A's fingerprints and exe: A built and B not built after it. That happens with `-ExecutableB` without `-ExecutableA` and without `-NoBuild`, or when B's clean or build throws. The next plain workspace build (`measure_performance.ps1`'s own build, `cargo run --smoke`) would then be fresh and measure ab03da9 as HEAD, the same failure as the first A/B. `-ExecutableB` now needs `-ExecutableA` (exit 3). The finally block calls `Restore-AbWorkspaceBuild`, which runs `cargo clean -p horde-defense -p horde_core` in the workspace when A was built and B's build did not follow; `ab.json` records `workspace_cleaned`, and a clean that fails exits 3 with the manual command in `failure`. `run_script_tests.ps1` adds 1 case (62 in all): the clean call when A was built and B was not, and no call otherwise. The bad-arguments case refuses `-ExecutableB` alone, and the pass case checks `workspace_cleaned` false. The W2-47 measurements are not affected: the first sitting's leftover A build was cleaned before bd12d1c's verify, `ab-interface-total-b` built B after A, and `-c` used `-NoBuild`; every gate rerun's report carries the battlefield counts only HEAD writes.
- **The baseline.v3 re-measure is not affected.** Its builds compiled the 2339a3d tree (the cargo logs name `target\v3-remeasure\src`), its exes (dev `0026b146…`, playtest `ebf38b01…`) differ from HEAD's, and its reports are the 2339a3d renderer's: none of the fields added after 2339a3d, and 514-515 paint jobs per progressed-peak frame as at W1-37 (HEAD draws 35). The W2-47 gate passes ran the 43184d1 exes (dev `dc17010c…`, playtest `ccd7c287…`), whose reports carry the battlefield counts.

## baseline.v3 on the 7abb0ab spans — 9 November 2026 (R29, W2-47)

**Ruling (R29, orchestrator, plan §2.4).** `PERF_BUDGET.json baseline.v3` is re-measured on the 7abb0ab egui_pass and present_wait spans before any interface_total re-gate; the old values stay under a previous key, and under R22 a gate the re-measure would loosen keeps its old value.

**What was measured.** baseline.v3 is W1-37's frame report v3 on the unchanged renderer (`2339a3d`). So the re-measure keeps that renderer and changes only the spans: `git archive 2339a3d horde-defense` extracted into `target/v3-remeasure/src` (not committed), with the two 7abb0ab probe edits applied by hand (`egui_probe.rs`: `begin_pass` exclusive, in `EguiPostUpdateSet::EndPass`, before `run_egui_context_pass_loop_system`; `render_probe.rs`: `begin_present_wait` after `create_surfaces`; the diffs are `target/v3-remeasure/applied-*.patch`). It was built dev and playtest with the same CARGO_TARGET_DIR and the exes copied to `target/v3-remeasure/exe/{debug,playtest}` (dev `0026b146…`, playtest `ebf38b01…`). HEAD's `measure_performance.ps1 -Scene all -Repeat 3 -NoBuild -NoBudget` ran each profile and VSync state with CARGO_TARGET_DIR at that folder. A copied exe has no assets folder, so it read the 2339a3d tree's assets.

**Conditions.** 2026-11-09, 19:59-22:00 UTC. RTX 1000 Ada, Vulkan, mains, power plan Balanced, Fifo or AutoNoVsync, pipelined, at 1440x900 @ 1.0 and 1050x700 @ 1.35. Every pass held the display on (R27) and started after an R30 quiet window. Another session's cargo builds and tests and user input ran during the evening, so runs were rejected: an unfocused or 0x0 window, or CPU load samples of 90-100% during a foreign build. Only run validity counts here, since the old reports have no driver-stall, texture-upload or battlefield fields and each pass's own checks are invalid by design (exit 2). Three scene groups still had an invalid run after the one rerun and were re-measured as whole groups, as W1-37 did: dev VSync-on peak (`v3-spans-dev-vsync-peak-b`), dev VSync-on stress (`-stress-b`) and playtest VSync-on stress (`v3-spans-playtest-vsync-stress-c`; `-stress-b` lost two small runs to a foreign build). Mean CPU load over the kept runs was 27-41% per pass, max 74%.

**Result** (medians of 3 valid repeats; plan 9.4 has the full table):
- interface_total fell in 17 of 24 cells (up to 8.6%) and rose 0.8-3.8% in 7, among them the gated progressed-peak dev VSync-on pair: 4.8218 / 4.7083 ms against W1-37's 4.6447 / 4.5895. The narrower span cannot add work, so this is the measurement's day-to-day spread on this machine, at least as large as the span change itself. That is the case for R31's interleaved A/B, which decides the gate.
- Under R22 both gated cells would loosen the gate, so `baseline.v3.interface_total_mean_ms.progressed-peak` keeps 4.6447 / 4.5895 (`kept_previous`). Every other cell, and every re-measured median in `measured_interface_total_mean_ms`, is the re-measure. `baseline.v3.spans` is `7abb0ab`, and the W1-37 baseline stays verbatim under `baseline.v3.previous`.
- interface_draw (same span as before): dev VSync-on progressed-peak 1.7098 / 1.5365 (W1-37 1.655 / 1.518), +6.0% over wave 0 at the normal size. Plan 9.4 records the drift as run-to-run spread; the other progressed-peak normal cells move -1.8% to +6.7% against W1-37 on the same unchanged ui::draw. No gate reads baseline.v3's interface_draw.
- cpu_frame p99 (wider spans) moved -4.4 to +1.2 ms per cell; informational.

**Test.** `perf_budget_v3_baseline_is_re_measured_on_the_7abb0ab_spans` pins this: the spans marker, the same renderer commit as `previous`, the W1-37 values verbatim under `previous` under the same conditions, W2-47 evidence for the gate run and every `runs` state, and for each cell `interface_total_mean_ms` = the measured median, except a gated cell above the previous value, which keeps it and is listed in `kept_previous`.

## W2-47 re-gate on the wave-2 stress fixture — 9-10 November 2026

W2-47's Done is `-Scene stress -VsyncOff` (dev and playtest) exiting 0 on the new stress fixture (every catalogue archetype, elites, the last wave tier and the boss's phase state). The re-gate also covers the W1-61 gate passes, the R31 interface_total A/B and the R18 items. No gate, threshold or rule was changed. Evidence: `target/perf/w2-47/gate-*` and `ab-interface-total-*` (not committed); the numbers are in `PERF_BUDGET.json acceptance.w2_47`, which `perf_budget_records_the_w2_47_regate` pins.

**Conditions.**
- The bd12d1c / 43184d1 sources (the bin was unchanged from 0213760 to bd12d1c; 43184d1 and bd12d1c change only tests, docs and scripts). Later wave-2 commits (3d4e881, 1cb775a, e3f52fa) change the binary, so these are not HEAD's executables, and the wave-2 close perf acceptance at HEAD (R34) is a new measurement (corrected in the W2 review). The first four invocations (`gate-dev-novsync`, `gate-playtest-novsync`, `gate-dev-vsync`, `gate-playtest-vsync`, 22:15-23:16 UTC) ran at 43184d1; the three reruns and the A/B `-b` and `-c` sittings (00:36-02:27 UTC) ran at bd12d1c. `acceptance.w2_47` records the commit, start and executable per invocation.
- RTX 1000 Ada, Vulkan, mains, power plan Balanced, 1440x900 @ 1.0 and 1050x700 @ 1.35. 2026-11-09 22:15 to 2026-11-10 02:27 UTC (corrected in the W2-47 review: `gate-dev-novsync` started at 22:15:11).
- Every pass held the display on (R27; no Kernel-Power 506/507 event fell in a kept run) and started after an R30 quiet window. Another session ran cargo builds, tests and cargo-nextest on this machine all night, so windows took up to 19 minutes and some runs were rejected on CPU load samples of 90-100% (or an unreadable sample). A scene group with an invalid run after the one rerun was re-measured as a whole group, as W1-37 did; the kept runs' load maxima were 69-83% per pass. The one exception is dev -VsyncOff, which was re-measured as a whole pass although one of its groups was valid and over (below).
- The dev and playtest exes were rebuilt once, after the A/B fix's `cargo clean`: dev `dc17010c…` (43184d1) then `79f0cbc8…` (bd12d1c), playtest `ccd7c287…` (43184d1) then `8076c139…` (bd12d1c), from the same sources.

| Pass | Command | Exit | Invocations | Gate | normal | small | Limit |
|---|---|---|---|---|---|---|---|
| dev -VsyncOff | `-Scene all -Profile dev -VsyncOff -Repeat 3` | 0 (rerun; not clean, see below) | `gate-dev-novsync` 2 (three invalid groups, and progressed-peak small valid and over: 12.7565 ms), `gate-dev-novsync-b` 0 | cpu_frame p99 progressed-peak / stress (ms) | 7.16 / 8.46 | 7.21 / 9.65 | ≤ 10.0 |
| playtest -VsyncOff | `-Scene all -Profile playtest -VsyncOff -Repeat 3 -ShippedSmoke` | 0 | `gate-playtest-novsync` 0 | cpu_frame p99 progressed-peak / stress (ms) | 6.80 / 7.68 | 6.98 / 7.17 | ≤ 10.0 |
| dev VSync on | `-Scene all -Profile dev -Repeat 3` | 0 | `gate-dev-vsync` 2 (one stress run with an unreadable load sample), `gate-dev-vsync-stress-b` 0 | progressed-peak interface_draw mean (ms) | 1.0264 | 0.9917 | ≤ 1.129 / 1.030 |
| | | | | peak interface_draw mean (ms) | 0.8162 | 0.7825 | ≤ 1.1904 / 1.0752 |
| | | | | progressed-peak interface_total mean (ms) | 1.9663 | 1.7993 | ≤ 4.6447 / 4.5895 (baseline.v3, R22) |
| playtest VSync on | `-Scene all -Profile playtest -Repeat 3` | 0 | `gate-playtest-vsync` 2 (progressed-peak small, 100% load three times), `gate-playtest-vsync-progressed-peak-b` 0 | frames over 50 ms, all scenes (worst) | 0 | 0 | 0 |

- **Done is met.** `-Scene stress -VsyncOff` exits 0 in dev and playtest: dev stress 8.46 / 9.65 ms, playtest 7.68 / 7.17 ms. No invocation had a stress cell over the gate: in `gate-dev-novsync`, stress normal was valid at a median of 9.23 ms and stress small was invalid.
- **A valid over in the first dev -VsyncOff pass (corrected in the W2-47 review).** `gate-dev-novsync` (22:15-22:25 UTC, 43184d1, `dc17010c…`) exited 2: four runs in three groups (peak normal r2 and r3, progressed-peak normal r2, stress small r2) had CPU load samples of 90-100% during a foreign build. Its progressed-peak small group was valid under R30: repeats 24.55, 12.76 and 10.51 ms at load maxima of 72, 52 and 44% (no sample at 90% or more). Its median cpu_frame p99 was 12.7565 ms against the 10.0 ms gate, status `over`. The exit was 2 only because measure_performance ranks invalid evidence above an over budget. The whole pass was then rerun (`gate-dev-novsync-b`, 01:15-01:27 UTC, bd12d1c), where progressed-peak small is 7.21 ms. The reason given at the time, a mean load of 44-49% and a wide repeat spread, is not an R30 validity criterion: R30 and the W1-37 method re-measure groups with an invalid run, not a valid group that was over. The 338919b commit body describes the first invocation as invalid only, and the record it pinned listed it as `gate-dev-novsync: 2` with no over. `acceptance.w2_47.passes.dev-novsync` now lists the over with its cell, repeats and loads, and `gate_clean` is false. Plan §2.5 has the row "A valid over in a pass masked by invalid runs (W2-47 review)". Until it is answered, by its default the dev -VsyncOff progressed-peak gate is not claimed clean at W2-47, and W5-11 re-gates it. W2-47's Done (stress) is not affected. **R34 (2026-11-10, plan §2.4) keeps that default:** the over stands and a valid gated over may not be re-measured away, whatever the machine load below the R30 threshold; the wave-2 close perf acceptance at HEAD is a new measurement. From R34 on, measure_performance prints `VALID OVER: <scene> <size> <metric> <value> > <gate>` for every valid gated over and lists them in summary.json `valid_overs`, so an over can no longer hide behind exit 2.
- Every pass had 0 frames over 50 ms and 0 over 33 ms outside driver stalls in every kept repeat, and 0 driver stalls: with the display held on, the stall class W2-50 attributed to display-off is gone.
- The battlefield egui gates (live, W2-47) held in every pass: 18 / 19 / 21 primitives (peak / progressed-peak / stress) of 24, at most 38,524 progressed-peak vertices of 40,000 and 50,614 stress vertices of 61,098 (46,702 with VSync off).
- **The headless egui evidence on the new fixture (W2 review, perf-diagnostics).** `PERF_BUDGET.json` `evidence.egui_measured` still held the 4d599a5 maxima from the wave-1 stress fixture. `measure_battlefield_egui_load -- --ignored` at 776075c (normal arena): 19 primitives in every scene; vertices peak 32,624, progressed-peak 38,524, stress 51,014, stress-5000-corpses 52,325. No gate moves (19 x 1.25 -> 24; 40,000 ceiling; 52,325 x 1.25 is above 61,098, which stays). The stress vertex maxima now fall at ticks 1980 and 1700, so "every maximum by tick 420" no longer held, and `live_battlefield_runs_stay_within_the_egui_gates` missed them. It now also paints each recorded maximum tick, running to the latest of them, and `live_battlefield_gate_paints_every_recorded_maximum_tick` pins that. The 4d599a5 figures are kept under `egui_measured.w1_69`. W2 review (tests): that pin held by construction (the plan is built from the ticks it checks), so it could not catch stale evidence; `live_battlefield_runs_stay_within_the_egui_gates` now also requires its (primitives, tick, vertices, tick) to equal `battlefield_*_by_scene` and `max_tick_by_scene` per scene, and the pin is reworded to cover only the plan builder. Mutation proof: stress `max_tick_by_scene.vertices` set back to the w1_69 tick 300 leaves the pin green and fails the live gate with `left: (19, 2, 47154, 120)` / `right: (19, 2, 51014, 300)`.
- Against the W1 review attempt 2 (bee4236, wave-1 content): cpu_frame p99 dev stress rose from 7.30 / 7.28 to 8.46 / 9.65 ms and playtest from 6.43 / 6.28 to 7.68 / 7.17 ms; progressed-peak is about the same (dev 6.77 / 6.63 to 7.16 / 7.21). The new fixture draws 18 archetypes and 69 elites at the last wave tier. Dev stress small is 0.35 ms under the gate; W5-11 should watch it.
- progressed-peak dev VSync-on interface_draw is 1.026 / 0.992 ms (W1 review 0.998 / 0.989), 9.1% / 3.7% under the gates.
- Shipped smoke (`gate-playtest-novsync/shipped`): 2.61, 0.91 and 0.93 s, median 0.93 s (W1 review 1.26). The first, cold run is again the slow one.

**interface_total A/B (R31).** `ab-interface-total-c` (2026-11-10 02:19-02:27 UTC), A `5ce1c87a…` (ab03da9) and B `79f0cbc8…` (bd12d1c), 5 pairs per size, every run valid:

| Size | A median | B median | B / A | Limit |
|---|---|---|---|---|
| normal | 2.133 | 2.052 | 0.962 | ≤ 1.05 |
| small | 2.067 | 1.992 | 0.963 | ≤ 1.05 |

The two earlier sittings are not evidence: `ab-interface-total` measured A twice (the build bug in "The first A/B measured A twice" above), and `ab-interface-total-b` (the fixed script, its own builds) lost pairs 4 and 5 to 100% CPU load samples. `-c` reused `-b`'s two executables with `-NoBuild`. B's reports carry the battlefield counts and A's do not, so the arms are the ones named. interface_total at bd12d1c is 4% under the wave-1 close at both sizes, so the wave-2 content up to bd12d1c added no interface_total cost on the gate's scene. The A/B result holds for bd12d1c; 3d4e881, 1cb775a and e3f52fa change the binary after it, so the wave-2 close acceptance at HEAD (R34) is a new measurement.

**Gate 3 in the shipped default configuration (R26).** R26 lets gate 3 be claimed in the shipped default configuration once this re-gate passes. It passes: `gate-playtest-vsync` stress (playtest, pipelined on, VSync on, the shipped default) ran at 60.00 FPS at both sizes with 0 frames over 50 ms and 0 over 33 ms outside driver stalls, 0 driver stalls, p99 intervals 18.00 / 19.53 ms. Every other VSync-on and VSync-off pass above also had 0 frames over 50 ms. Gate 3 is claimed for bd12d1c (the executables measured here) in dev and playtest, VSync on and off. It is not claimed for HEAD: 3d4e881, 1cb775a and e3f52fa change the binary after bd12d1c, so the wave-2 close perf acceptance at HEAD (R34) is a new measurement.

**The R18 items.**
- The sustained 15-20 FPS VSync-on stress episode: closed by R26 (environmental). None of the 17 valid VSync-on stress repeats here showed it.
- The 1050x700@1.35 VSync-on p99 interval of 19.1-19.7 ms: measured, and it is not a property of the small size. In the 22:48-23:16 UTC passes the small size had 19.53-19.95 ms against the normal size's 17.61-18.72 ms. In the 01:27-02:18 UTC reruns it was 17.56 ms (dev stress) and 17.70 ms (playtest progressed-peak), against 17.54 / 17.48 ms at the normal size. The extra tail is in the swap-chain acquire, not in the process: dev stress small present_wait p99 16.2-16.6 ms in the wide runs and 13.4-13.5 ms in the narrow ones, while main and render-world CPU p99 stayed at 4.3-5.6 ms. Mean FPS was 60.00 throughout. What makes the presentation of the smaller-than-the-panel window wider at some hours is not named. It gates nothing: 17.7 ms is only the pipelined decision's threshold, and that decision stays as recorded.
- The script-level battlefield primitive and vertex gates: landed earlier in W2-47 and held live in every pass (above).
- The egui_probe/present_wait span ordering, including baseline.v3: re-measured (see "baseline.v3 on the 7abb0ab spans" above). No other span-ordering item is open.

## Legacy guards and fixture behaviour — 10 November 2026 (W2-48)

The save-determinism guards that keep the wave-2 systems away from encounters saved before them (design-save-determinism §5 and §11, plan EB14 and SD6), and behaviour tests that play the v6 fixture forward through the wave-2 rules. Each guards behaviour an earlier wave-2 commit landed, so each passed on the parent (ad61a22); the proof is a mutation of the production code, run against the named test and reverted:

| Test | Where | Mutation | Failure |
|---|---|---|---|
| `elite_roll_consumes_no_combat_rng` (rates 1, 200, 850, 1275, 5000, 9999 and 10000 bp against rate 0, both modes; the reseeded verdicts and affixes match the keyed hashes) | `src/sim/elite_tests.rs` | an elite spawn draws `a.rng.below(2)`; separately, the ELITE key mixes in `a.rng.state` | `Personal 200: the combat RNG moved at pass` `Some(9)`; `Personal 200: enemy 42 (brute)` `None` vs `Some("hardened")` |
| `rank_zero_deploy_spawns_no_elites_and_has_no_new_archetypes` (Easy/Normal/Hard, both modes, 3 seeds, 3 minutes) | `tests/legacy_guards.rs` | the director's rate floored at 850 bp; separately, its wave pool tier raised by 6 | `Easy Personal 0x1: elite enemy 32`; `Easy Personal 0x1: new archetype exploder` |
| `legacy_attempt_snapshots_have_zero_elite_rate_and_no_boss_phases` (v1-v6 roots and the v6 expedition; the v7 freeze shows both marks) | same | load gives an attempt at rank 3 or more one boss transition | `v1 root` `(0, 1, 0, 0, 0, 0)` vs `(0, 0, 0, 0, 0, 0)` |
| `v6_active_attempt_spawns_no_elites_for_3600_ticks` | same | the 850 bp floor | `expedition 16` (telemetry counts elites) |
| `v6_legacy_boss_has_no_phases` (a frozen-0 boss under both thresholds; the same boss on a 2-transition snapshot phases) | same | the load mutation above, on the root attempt only (W2 review: the precondition reads the root, where the boss spawns) | `the v6 root snapshot froze no boss transitions` `(Personal, 14, 1)` vs `(Personal, 14, 0)` |
| `continue_journey_after_migrated_companion_report_redeploys_companion` (inline JSON: the v6 payload without its live defense, its keyless Mara report a Belarus stage 1 loss) | same | `next_defender` ignores the report | `Player` vs `Mara` |
| `v6_fixture_recruit_can_take_over_the_next_defense` | same | `ContinueJourney` keeps the choice; separately, the takeover finish never releases the recruit | `Continue clears the choice`; `Defending` vs `Ready` |
| `worker_level_up_does_not_pay_the_level_grant_twice` (the live Polish expedition levels 33 -> 34) | same | a worker settles its own ledger | `a worker never settles` `(34, 34)` vs `(34, 33)` |
| `migrated_threat_jump_pays_no_milestone` (the v6 payload pre-threat: victories 0 jump to its clear rank 14 at load) | same | step (d) claims by the saved victories | claims `{}` vs `{5, 10}` |
| `commands_round_trip_through_serde_for_replay` (the 9 appended variants at non-default values) | `src/command.rs` | `AcknowledgeOutcomes.folded` `#[serde(default, skip_serializing)]` | `{"AcknowledgeOutcomes":{"battles":[9,12]}}` reads `folded: 0`, not 3 |

No production code changed and no digest moved.

## Wave-2 re-bless and ruleset 3 — 10 November 2026 (W2-49)

This is the wave's one save-determinism re-bless (rulings P3, R7). Six wave-2 commits had moved `GOLDEN` provisionally, each with a cause in `PROVISIONAL_CAUSES`. W2-49 folds those digests into `GOLDEN`, then resets `PROVISIONAL` to None and `PROVISIONAL_CAUSES` to empty. The reasons now sit in the re-bless log of `crates/horde_core/tests/determinism.rs`. `GOLDEN` moved 6/6 and `GAMEPLAY_TRACE` 0/6. These did not move: the V1-V6 continuation pins, `V6_DROP_TRACE_FNV`, the `save_v7.json` pins, the frozen-catalogue pin and `SAVE_VERSION` 7.

| Seed / mode | GOLDEN, wave-1 close → W2-49 | Causes in commit order (class) | Net (`support::classify`) |
|---|---|---|---|
| 0x1 Personal | 0x303ea36fac6a070b → 0xbed5e2714b6590fe | W2-05 T, W2-06 F, W2-52 F, W2-54 F | F |
| 0x1 Npc | 0xeede2391290bb381 → 0xcffe0be4463d46d6 | W2-05 T, W2-06 F | F |
| 0x1a571167 Personal | 0x2902d75f0238e1a3 → 0x3368f18097f7fb5e | W2-05 T, W2-06 F, W2-08 C, W2-52 F, W2-09 G (R33; `support::classify` labels it C), W2-54 F | F |
| 0x1a571167 Npc | 0x6a2ba00f7e4018d8 → 0x97d93e67bd2bed2f | W2-05 T, W2-06 F | F |
| 0xdeadbeef2026 Personal | 0x5aaf1f080eb1da92 → 0x7be5c84d229d8d71 | W2-05 T, W2-06 F, W2-52 F, W2-54 F | F |
| 0xdeadbeef2026 Npc | 0xc1bb9ef207925afc → 0x6dcebbedcc7582c9 | W2-05 T, W2-06 F | F |

What each cause changed:
- **W2-05 (T):** the attempt telemetry carries the per-archetype spawned, killed and damage-taken maps.
- **W2-06 (F):** every attempt saves its frozen `drop_tier` (Some(1) in all six runs), and a root attempt lists its drops in `drop_ids`.
- **W2-08 (C):** the 39 `loot_only` items grow the tier-1 pool from 12 to 18 definitions, so the one drop of 0x1a571167 Personal picks another definition.
- **W2-52 (F):** a Personal level-up from 1 to 2 under the neutral grant saves `level_grant_paid` 2.
- **W2-09:** the weighted single draw moves that drop again. It is class G under R33, because the pick is a gameplay mechanic, although `support::classify` labels the move C (items only).
- **W2-54 (F):** the authored 0.25 grant makes the W2-52 shim inert, so `level_grant_paid` is no longer saved.

**Anatomy.** Before is `determinism_anatomy` on the wave-1 close tree ab03da9, built from an exported tree with its own target directory (`target/production-after-wave2/anatomy_before_w1close_ab03da9.txt`). After is W2-49 (`anatomy_after.txt`, identical to W2-54's `anatomy_after.txt`).
- In all six runs the key paths and the telemetry-stripped payload move. The items hash, the outcome summary (kills, coins, hp, loot, spawned and `rng.state` at ticks 600, 1200 and 1800) and the trace do not, so each run nets F.
- The keys hash of the five runs without a drop goes 0x8a4c8226e2df0f7a → 0xbdf9c3b221052103, and 0x1a571167 Personal's goes 0x4bbabacd6a590564 → 0x72f5e366ed803b2a.
- 0x1a571167 Personal keeps its items hash 0x48f43caeb9493765. W2-08 and W2-09 each moved its one drop, and the weighted draw lands on the wave-1 definition again.

**Ruleset 3.** `RULESET_VERSION` and `TRACE_RULESET` go from 2 to 3 (rulings P4 and R15). The bump names both changes since ruleset 2:
- W1-69: no lane queue. Every enemy at the line or a structure attacks.
- The wave-2 rules: auto-sell, takeover, the weighted drop table, the relative quality roll and loot-only drops, threat-scaled camp prices, the in-country front ramp, the late growth slopes and floors, elites from rank 3, boss phases past rank 4, the new archetypes, the country difficulty bands, the weapon-mastery nodes, and the authored level grant and rank milestones.

`GAMEPLAY_TRACE` keeps W1-69's three wave-1 re-blesses, and wave 2 moves none of the six traces. The traced runs are rank 0 with no country, and the weighted pick still makes one draw. Replay and load behaviour are in "Strict sale and ruleset 2" above.

Fail-first: `envelopes_carry_version_ruleset_and_fingerprint_and_reject_unknown_versions` now asserts the literal ruleset 3 and failed on the parent with `left: Some(2)` `right: Some(3)`. It replaces the removed `ruleset_version_is_2`.

**Smoke and pacing after wave 2** (`target/production-after-wave2/`, not committed). The bump changes no rule, so both outputs equal the last recorded ones:
- `--smoke` (`smoke.txt`, PASS, exit 0) is byte-identical to `w2-54/smoke.txt` (sha256 846f0b4c...3425fe50).
- `pacing 60 --runs` (`pacing.txt`) is byte-identical to `w2-54/pacing.txt` (sha256 902cd895...c7f7d738).
- The 20-seed release acceptance (`pacing_targets.txt`) is in plan §9.2 and §9.6.

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
cargo test --locked -p horde-defense --no-default-features
cargo test --locked -p horde-defense --features dev
cargo run --locked -p horde-defense -- --smoke
python -I -B scripts/check_views.py
python -I -B scripts/tests/test_check_views.py
python -I -B scripts/generate_audio.py --check
python -I -B scripts/generate_atmosphere_audio.py --check
python -I -B scripts/generate_world.py --check
python -I -B scripts/generate_world.py --self-test
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/tests/run_script_tests.ps1
```

`check_views.py` prints `PASS: <n> Views` and `PASS: <n> modules to depth 2 each have an ARCHITECTURE.md ownership row` and exits 0; it exits 1 when a crate module or directory to depth 2 has no row in the ARCHITECTURE.md ownership table, a row names a missing module or file, a row's first cell backticks nothing or a `core`/`game` row names a path outside that crate, or a named guard test is not a Rust `#[test]` fn (a production fn of the same name does not count); `scripts/tests/test_check_views.py` exercises those rules on a throwaway workspace and exits 0 or 1. `run_script_tests.ps1` exercises `scripts/lib/Native.ps1`, `measure_performance.ps1`, `capture_review.ps1` (window-size and sidecar checks) and the `-Executable` resolution of `capture_review.ps1` and `capture_motion.ps1` against a fake game executable under Windows PowerShell 5.1; it prints PASS or FAIL and exits 0 or 1. Documented `scripts/` commands use the Windows PowerShell 5.1 that ships with Windows (`powershell`, not `pwsh`). The three PowerShell 7 scripts (`capture_player_models.ps1`, `trace_npc.ps1`, `verify_player_models.ps1`) say so where they are documented. `repo_checks::docs_invoke_scripts_with_windows_powershell` enforces both outside `docs/plans/`.

The two generator checks re-render every clip the Python generators own into a temporary folder outside the workspace and compare SHA-256 hashes with the committed WAVs in `assets/audio/` and `archive/audio/`; the atmosphere check also compares both Python manifests (`assets/audio/PROVENANCE.json` and `archive/audio/PROVENANCE.json`) byte for byte. Each prints `PASS: ...` and exits 0, or prints one `FAIL:` line per mismatch and exits 1; neither writes anything, so `git status` stays clean. `--check` is the default; `--write` regenerates in place.

`generate_world.py --check` regenerates `crates/horde_core/src/world.json` in memory and offline from the committed file (only `difficulty` is recomputed) and prints `PASS: ...` and exits 0 when the bytes match, or prints `FAIL: ...` and exits 1; it writes nothing. `--self-test` exercises the writer, `--check`, the default `--from-existing` mode and the `--rebuild` sha256 (exit 3), stage-count (exit 4) and country-id (exit 5, a vanished or renamed id; new ids are appended) guards and its carry-over of `level_override` and `stage_theme_ids` on throwaway files in the temporary directory, prints `PASS: <n> self-test cases` and exits 0 or 1. See WORLD.md for the modes.
