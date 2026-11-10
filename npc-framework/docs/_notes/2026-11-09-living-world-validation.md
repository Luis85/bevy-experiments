# Living-world iteration: review and validation

Updated 2026-11-10. Worktree: `codex/living-world-expansion`, based on the integrated
player flow. **Final full-gate verdict: `ok: true`, exit 0.** The completed
[gate report](../../target/npc-out/living-world/verify-final.json), native captures
and bounded simulation evidence below cover different aspects of this iteration.

## Research and review corrections

Subagents researched [world/ecology](../research/2026-11-09-living-world-expansion.md),
[resident/economy architecture](../research/2026-11-09-adaptive-regional-residents.md)
and [graphics/experience](../research/2026-11-09-world-fidelity.md), using primary
developer sources, asset rights records and installed Bevy APIs. Implementation
then received independent review and a correction pass.

| Reviewed issue | Resulting correction |
| --- | --- |
| Global allocation and conflicting home/work catchments | Regional production, feasible recipe alternatives and supply/labor queries following the workplace; housing and identity remain separate |
| Competing job control and interrupted committed work | Reassignment uses the existing Work boundary; queued/active restoration participates in native handoff |
| Incomplete or duplicated saved restoration inputs | Exact positive, unique, ordered material claims with ownership, reservation and strict-load checks |
| Missing legacy terrain resource during regional Continue; fallback to the wrong profile | Profile-specific registration, frozen construction, named shipped-profile fallback and save/resume regressions |
| Native world-editor capture reconstructed from Plain and discarded regional construction | Draft retains its complete typed source; passing regressions cover unrelated edits, cloning, saving and both revert paths |
| Arrival inside the well visual; overlapping buildings, sources or plots | Arrival moved to `(500, -43000)` mm, matching model/structure footprints, deterministic source clearance, plot corrections and aggregated access checks |
| Authored layout and ecology could accept inconsistent state | Edge/footprint/entrance validation; frozen sites and plots; exact resource identity, conservation and restoration-history checks |
| Births could use flooded or newly obstructed habitat cells | Current terrain and authoritative footprint checks; defer births when no valid cell remains |
| Static district projection ignored authored changes | District names, coordinates and biomes derive from frozen settlement definitions |
| Social destinations could be unreachable | Reachable-route ranking with a 15-metre walking limit |
| Continue/New replaced entities underlying scenery | Stable-ID rebinding for homes, resources and hearths |
| Terrain controls, previews and travel UI assumed the smaller island | Dynamic bounds, elevation-aware previews, DDA picking and compact settlement travel controls |
| Repeated scenery, cloud grid and weak overview framing | Varied licensed models, revised clouds, closer framing and concealed finite ocean boundary |

Asset import evidence covers 59 pinned files and 31 models, with source hashes and
rights records. Initial accessibility failures remained failures until the actual
source/plot collisions were corrected; assertions and protected oracles were preserved.

## Autonomous release-world audit

The latest [audit log](../../target/npc-out/living-world/extended-release.stdout.txt)
ends with `REGIONAL_AUDIT_COMPLETE {"ok":true,"seed":7,"ticks":336000}` and
`1 passed; 0 failed`. Runtime was 2,246.57 seconds; this is test runtime, not a
rendering benchmark. The ordinary day remained 24,000 ticks, with samples every
300 ticks and no player intervention.

| Evidence | Result |
| --- | --- |
| Coverage | 336,000 ticks / 14 days, calendar day 19 autumn through day 33 spring, including winter |
| Founding residents and housing | All 42 survived and remained housed; 14 per settlement |
| Currency | 8,400 conserved |
| Physical trade and work | 2,846 transactions; 200 observed occupation changes |
| Produced garments | 94 warm coats |
| Other production | 324 bread, 178 cooked meals, 596 logs, 114 fish, 33 hides and 66 raw meat |
| Finite ore | 12 harvested; 35,988 remaining; no regeneration |
| Late winter window, ticks 144,000–216,000 | 72 bread, 44 meals and 18 coats produced |
| Final three days, ticks 264,000–336,000 | 72 bread, 40 meals, 13 coats and 598 transactions |
| Lowest sampled needs, out of 100,000 | Energy 48,778; Hunger 49,532; Thirst 54,250; Warmth 6,392 |
| Sampled disruptions | No observed blocked-movement ticks; 16 actor-samples below 15,000 Warmth |

This proves the reported seed and interval, including continued late-window
production. It does **not** establish a full 28-day year, every seed, absence of
brief unsampled stalls, or indefinite sustainability. Survival did not mean that
every need stayed comfortable. Fish and woven clothing provide real alternative
chains; zero hides in a particular window is not itself evidence of failure.

A separate bounded wildlife diagnostic found all 24 birth positions clear of the
authored structures and on land. At tick 12,000, every habitat had three recorded
births; populations were 6/5/6. Scheduling and initial replenishment worked in all
three regions. The long audit does not record final per-habitat birth counters.

## Native confirmation

[Native confirmation summary](../../target/playtest-living-world-f902beb92f/summary.json)
reports `ok: true`, 54 PNGs and no native errors. Its executable SHA-256 is
`f9d3ba2b091022ce7dc48151f011a59eeb5535a17ebb77a66222b6647bde79a9`.

- Captures cover actual 1280×720 and 1024×768 window/image sizes, with recorded
  viewport geometry, the title/New/play/save/title/Continue flow and inspector/editor views.
- Native chart widgets completed three real journeys: Harbour → Alderwood →
  Stone Ridge → Harbour. Recorded movement outcomes confirm arrival; no teleports
  or material grants were used.
- All 42 authored home visuals remained after Continue and after a second New game.
  The arriving character was visibly clear of the well; civilian bodies/materials,
  compact travel controls and night readability were inspected.
- A recorded operator Terraform command produced terrain revision 1. A second
  inspector-first world reached night at tick 17,713 while preserving its unarrived visitor.

Input was BRP widget activation plus recorded operator clock/editor commands.
**Physical keyboard/mouse and physical brush input were not verified.** The
eight-second CPU/system capture ran under concurrent verification load; GPU timing
and FPS were not measured. The first cold 1280×720 title capture lacked ground that
appeared in subsequent title/New captures. The transient was observed, but its
cause was not instrumented or established.

## Full gate and final confirmation

The first `cargo xtask verify --pretty` returned **`ok: false`** in
[verify.json](../../target/npc-out/living-world/verify.json). Formatting, Clippy,
architecture, replay and dependency-policy stages passed, but the test build
failed with missing `.rlib` artifacts / `E0463` (including `npc_framework`), before
the affected tests could run. This was not a passing test suite.

A clean rebuild of generated artifacts ran the same full gate with
process-local `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=line-tables-only`
and `CARGO_PROFILE_TEST_DEBUG=line-tables-only`. That recovery changed generated
build artifacts and debug-symbol settings only: no test filters, optimization
levels, assertions or protected replay/snapshot oracles were changed.

The clean run compiled and executed 1,946 tests: **1,944 passed and two failed**.
These were actual test failures, distinct from the earlier missing-artifact build:

- `worlds::tests::all_world_fields_round_trip_and_revalidate` found that the native
  world editor dropped `living_world`. Typed-source preservation and two focused
  lifecycle regressions are now on disk; the original round-trip assertion is unchanged.
- `village_world::label_tests::root_rts_caption_projection_uses_this_frames_transform_even_before_global_propagation`
  failed its caption-position assertion. The corrected test checks the intentional
  22-pixel caption height plus four-pixel clearance for both caption heights,
  while retaining exact current-frame projection, separation from stale global
  projection and unchanged simulation/global-transform state.

All four [focused regressions](../../target/npc-out/living-world/final-regressions.log)
passed: the original world round-trip test, two additional editor lifecycle tests,
and the caption projection test. The final full gate then ran:

```powershell
$env:CARGO_INCREMENTAL = '0'
$env:CARGO_PROFILE_DEV_DEBUG = 'line-tables-only'
$env:CARGO_PROFILE_TEST_DEBUG = 'line-tables-only'
$env:NEXTEST_TEST_THREADS = '2'
cargo run --locked -p xtask -- verify --pretty
```

The [final JSON report](../../target/npc-out/living-world/verify-final.json) returned
**`ok: true`, exit 0**, with all seven stages passing and no tool stage skipped:

| Stage | Final result |
| --- | --- |
| Format | All files formatted |
| Clippy | No warnings |
| Tests | 1,948 passed, zero failures, 14 skipped; Nextest reported one slow and five leaky tests |
| Documentation tests | One passed, zero failed, ten ignored |
| Architecture fitness | Seven passed |
| Replays | Eight verified: one session, three testbed, four village |
| Goldens | Six snapshots, none pending, with `INSTA_UPDATE=no` |
| Dependency policy | Advisories, bans, licenses and sources all passed |

The final JUnit report independently records 1,948 tests, zero failures and zero
errors. The skipped/ignored counts above are retained from the ordinary full gate;
the separately executed 336,000-tick release-world audit is documented above.

Source also corrects the regional debug-navigation guard and shading of gentle
terrain risers. The 54-image confirmation predates those two presentation fixes.
A [targeted confirmation](../../target/playtest-ridge-check-aae74715bf/summary.json)
then passed with six inspected PNGs covering Ridge, Harbour and the editor at both
sizes. The fine blue grid is resolved, banks remain visible, navigation paths stay
hidden without explicit diagnostics, and all 42 home visuals remain. Cell geometry,
terrain heights, picking and navigation were unchanged by the shading correction.

This targeted run used the same frontend host through a copied `actor_testbed`
binary, SHA-256 `943618c3ed120f4808dfba82968387ac842835867df05c4a9a2f4eebe3c05412`,
so verification could continue without a Windows executable-file lock. The final
full-gate result above verifies the corrected source. Native evidence remains
bounded by the recorded capture flow and input/performance limitations.

The subsequent `cargo build --locked --workspace --all-targets` used the same four
process-local settings and **passed, exit 0**, in 1 minute 20 seconds. Its
[build log](../../target/npc-out/living-world/build-final.log) records the normal
`game_menu` executable build. No source changes followed the passing gate; the
remaining edits only record this evidence. A separate changed-path audit found no
core, protected oracle/policy, dependency, save, build-output or credential changes.

## Playtest handoff

The freshly built `target/debug/game_menu.exe` was launched visibly from this
workspace with no arguments or renderer override. Process **90076** exposed a
responding **Island Village | NPC Framework** window. Its SHA-256 is
`3a6a150ee1e478d21555914ffad31d77407e569580bb89fc4bb84160229dba7f`.
The [handoff record](../../target/npc-out/living-world/player-handoff.json) and
[startup log](../../target/npc-out/living-world/player-handoff.stderr.log) record
the launch. Startup confirmed the remote interface was off and logged the three
settlements and 42 residents. No error or panic was found; startup did emit 48
empty-mesh warnings (`contains no vertices`), preserved in the log.

At the title handoff, the new `living-world.world.json` still did not exist and the
existing `local-island.world.json` retained its prelaunch SHA-256. No save was
created or overwritten. `git diff --check` passed, with no protected/core paths,
saves or generated outputs in the change inventory. The process was left open for
the user's playtest. No commit, push, merge or pull request was created.

The [2026-11-10 startup follow-up](2026-11-10-terrain-startup-polish.md) supersedes
that handoff: it addresses the 48 empty-mesh warnings and initial terrain reveal,
records a new passing full gate and normal relaunch, and preserves the earlier
log as historical evidence.
