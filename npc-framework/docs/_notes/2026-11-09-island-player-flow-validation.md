# Island player flow validation — 2026-11-09

Status: the corrected source passed the complete workspace gate on 2026-11-09;
the verified binary is built and Island Village was launched for playtesting.
Final native evidence and its limits are recorded below. The previous
colony iteration's evidence is in
[the November 8 report](2026-11-08-island-colony-validation.md).

## Scope

The user confirmed the existing 3D presentation and survival/permanent-death rules.
This increment joins title, character setup, play, pause, save and return in one
native window; adds a coherent player workspace and material-backed first-home
guidance; and extends the existing crop contract with colony berry beds.

Application lifecycle, files and pause are host-owned. HUD state and previews are
view-owned. Guidance is a pure actor-observation projection. Gameplay uses the
existing recorded action adapters. No registry identity, protected replay or
golden baseline was changed for this increment.

## Checks observed so far

- Content unit tests: 122 passed, one existing ignored; crop tests: five passed.
- Focused homestead, agent and journey integration tests passed before the final
  work-preservation guidance refinement.
- Integrated host library suite: 84 passed, including paused-title isolation,
  transactional New failure, backup/Continue, mid-construction and crop persistence,
  keyboard navigation, and recovery from a failed final shutdown save.
- Launcher suite: 161 passed, one existing ignored; the process-timing test
  `explorer::tests::leaving_the_checking_screen_kills_the_check` exceeded its existing
  30-second assertion under concurrent test load. An isolated rerun passed in
  0.52 seconds. No assertion or timeout was weakened. The final gate will use two
  test threads.
- A new dialogue test initially indexed the project's `JsonObject` as a JSON value;
  the test now uses its existing `get` API. Production behavior was unaffected.
- Independent static review confirmed fixes for queued movement after restore,
  failed-shutdown recovery, fresh-world name validation, stale workspace rows and
  double Tab handling.
- The subsequent focused run with two test threads passed: 84 host, 122 content,
  seven journey, two dialogue and 149 view tests (364 total; two existing ignored).
  This includes the moderate-need work-preservation and foreign-focus Tab fixes.
- `cargo build --locked -p game_menu -p actor_testbed -p npcctl` succeeded before
  the first native capture.

## Baseline native observations

Two disposable runs produced captures at both requested sizes. The native checks
confirmed title inactivity, one persistent window, arrival, all six workspaces,
exact paused save tick, title return and Continue retaining the same actor.

The first life-loop run stopped after a conversation attempt: the NPC had moved
between arrival and dispatch while the harness advanced 40-tick chunks. This is
not proof of the same failure at ordinary 1× frame timing. Conversation stepping
was reduced to one tick; the shared adapter also gained bounded, observation-scoped
re-approach and explicit failure feedback, pending the new run.

The second run completed New → workspaces → Pause → Save → Title → Continue and
clicked Quit. Its harness then queried the deliberately closed BRP connection and
reported a connection reset. The harness now verifies Quit through process exit
and the saved file instead of requiring a live window afterward. The failed
reports remain intact; neither run has been relabeled successful.

Visual review of all 18 first-run captures found default diagnostic cones/rings,
poor hero framing near the toolbar, redundant scroll controls when content fits,
technical captions/save text, a thin Trade empty state and southward exploration
guidance listed after west. The baseline polish addresses these, with new native
captures still required. Success/error notice tone is now explicit as well.

## Native evidence plan

`scripts/playtest/capture_player_flow.py` uses a disposable save/settings folder,
the actual `game_menu` binary, explicit Vulkan or ordinary renderer selection,
enabled native widget activation, and
1280×720 / 1024×768 screenshots. Its optional life-gameplay route uses actual
gathering, crafting and building actions; it must not grant materials, teleport,
or use operator knowledge to choose actor actions. Debug widget activation does
not prove physical mouse/keyboard or assistive-technology operation.

## Remaining release work

The requested research and improvement pass is implemented and reviewed. The first
full gate exposed the failures recorded below; the corrected source passed the
complete rerun. Final binary and feedback checks are recorded in the last section.

## Verified baseline before web research

- Revised focused suite: **369 passed**, two existing ignored. Package Clippy
  (`--all-targets -- -D warnings`) and workspace formatting passed.
- Native `playtest-player-flow-6da86f62ea/summary.json`: `ok: true`. It gathered
  six logs, crafted two four-plank batches, spent eight planks and four logs on a
  cottage, consumed a real berry seed and water, grew for 9,000 ticks, harvested
  four berries, and retained the same life and possessions through saved title,
  Continue and clean Quit. Conversation succeeded after two moving residents
  left sight. No goods were granted and no actor was teleported.
- Exploration used explicitly enabled, recorded actor-relative movement when
  directional widgets were unavailable. Material, work, crop and menu actions
  used enabled native widgets. This is not physical keyboard/mouse evidence.
- No nearby buyer was found during that baseline's bounded post-harvest sample.
  A completed native sale **was unverified in that baseline**; its successful
  report does not establish one. The later purchase evidence is recorded below.
- Native `playtest-player-flow-74fd539496/summary.json`: `ok: true`. Inspector,
  accelerated inspection, Character and Editor shared the same window. A recorded
  terrain brush was applied and saved exactly once (`revision: 1`). Actual mouse
  painting was not exercised. A prior run's stale render-target resize failed its
  exact geometry check; the harness now retries once through a distinct size and
  still requires exact window, camera and PNG dimensions.

## Research and final pass

The requested web review is documented in the
[simulation](../research/2026-11-09-simulation-research.md),
[experience](../research/2026-11-09-experience-research.md), and
[implementation](../research/2026-11-09-implementation-research.md) briefs. Each
compares primary sources with the actual code and native evidence, and separates
applicable corrections from larger feature proposals.

The final pass owns these changes explicitly: content owns heating/social rules,
garden guidance and recovery evidence; content discovery providers own learned
static places; native control owns deferred work transfer and outcome feedback;
the player view owns compact selectors, nearby context and spectator dismissal;
the application owns inspector-first entry and persistence. Verification of this
pass is pending and must not be inferred from the baseline results above.

## Research-pass checks and native evidence

- Package Clippy passed after replacing one redundant test lookup with
  `contains_key`; assertions and lint policy remain intact.
- Combined unit checks passed: **85 host, 125 content, 168 view** (378 passed;
  two existing ignored). Six player-UX regressions were rerun after the final
  foreign-focus shortcut correction and passed.
- Targeted domain checks passed: five colony-AI, four life-polish, one shortage
  recovery and nine journey tests. Three discovery tests passed after correcting
  an invalid test fixture alias to a role-based stable-ID lookup.
- The explicitly requested extended audit passed **336,000 ticks at seed 7**:
  all twelve residents survived, 1,036 transactions completed, winter production
  and replacement clothing continued, and the sampled blocked-route criterion
  passed. This is bounded seasonal evidence, not proof of arbitrary disaster or
  specialist-loss recovery. See `target/research-seasonal-audit.log`.
- `playtest-player-flow-26004bff47`: the final farming/harvest/save/Continue/Quit
  run passed with trade explicitly excluded for a separate test.
- `playtest-player-flow-9622270e94`: inspector-first entry advanced twelve NPCs'
  needs while arrival remained pending; later Character entry recorded one join.
  Mode round trips and a saved terrain brush passed. In the isolated mortality
  phase, recorded `SetNeed` commands depleted the visitor's Hunger and Thirst;
  ordinary mortality then produced a dead actor, spectator access and a distinct
  successor. The old actor remained dead. This is controlled death-flow evidence,
  not a natural-survival claim.
- `playtest-player-flow-e113fbc2f5`: a copied pre-research snapshot continued
  successfully. The source SHA-256 remained unchanged; the harness wrote only its
  disposable copy.
- `playtest-player-flow-4dfec8d1b9`: a real one-log offer was purchased by an
  autonomous resident. The observed market quote changed from six to seven coins;
  the completed receipt recorded one log and seven coins. Carried logs changed
  **1 → 0**, the player's coins **0 → 7**, with zero money/stock grants. The run
  then saved, continued and quit successfully. The earlier trade-only failure
  was retained: its harness kept its sole gathered grain as a seed reserve; the
  corrected test gathers actual surplus beyond that reserve.
  Its generic `checks[]` wording said "quote-limited sale", but the actual
  receipt is a dynamically priced public offer with no minimum price. It proves
  autonomous purchase and exact settlement, not activation of a direct capped
  Sell button. The harness wording has been corrected for subsequent runs.
- These runs used ordinary renderer selection (`renderer_override: null`) and
  actual enabled native widgets. Recorded operator terrain/needs interventions
  are explicit in their reports. They do not verify physical keyboard/mouse,
  screen-reader operation or audio.

Independent static and screenshot reviews found no blocking issues in the
inspected flows. Two final status issues were identified: retained-native Continue
could still say it was joining, and a completed passive sale could leave its old
listing message. The Continue regression passed; passive-sale feedback and the
full workspace verdict remain to be verified below.

## First full gate and corrective work

`target/final-player-flow-verify.json` records **`ok: false`**. Formatting,
warnings-denied Clippy, all seven architecture checks, eight replays, six golden
snapshots, and dependency advisories/bans/licenses/sources passed. Nextest ran
1,884 tests: 1,881 passed, three failed, thirteen skipped; doc tests had one pass
and ten ignored. This result has not been relabeled successful.

- `commissioned_house_finishes_through_funded_autonomous_production` reproduced
  in isolation. Diagnostics rejected an initial navigation hypothesis: the site
  was empty and the builder had not claimed it. The carpenter's cash was wholly
  reserved for a future coat, leaving a financeable explicit commission without
  inputs. A colony-only production allocation change funds a complete commission
  bundle after urgent needs, before long-horizon reserves. The original two-day
  completion assertion passed after this change. Costs and actual money/goods
  were not changed. Follow-up checks also found that critical hearth fuel could
  lose funding to forecast clothing, and energy-only recovery could leave a cold
  NPC idle. Those fixes reuse existing finite fuel, navigation and rest mechanics.
- The provider failure-category test passed unchanged in isolation in 151.67 s.
- The registered homestead/crop/food/income rehearsal passed unchanged in
  isolation in 35.96 s, including physical production and a real grain sale. Its
  full-suite duration had been 241.594 s against an existing 240 s limit.

The repository now declares these two service-heavy CLI tests exclusive through
Nextest's `threads-required = "num-test-threads"` override. Selection, assertions,
retry policy and time limits are unchanged. The scheduling setting follows
[Nextest's resource-reservation guidance](https://nexte.st/docs/configuration/threads-required/).
JUnit failure output is retained at `target/nextest/default/junit.xml` so later
failures are not reduced to `xtask`'s short log tail.

The corrective simulation suite passed 34 tests: colony AI, architecture, clothing
forecasting, commission finance, warmth recovery and island colony behavior.
Seventeen legacy production tests also passed. The cottage completed at tick
8,800; all twelve residents survived through 120,000 and currency stayed exactly
2,400. One real garment was produced and traded during 96,000–120,000, and the
builder recovered warmth through actual home heat. An added point-in-time coat
expectation was corrected to this bounded flow contract after inspection showed
96,000 was just before daily redistribution, not evidence of permanent starvation.
All pre-existing completion, survival and conservation assertions remain intact.

## Complete corrected-source gate

`cargo xtask verify --pretty` completed with **`ok: true`** in
`target/final-player-flow-verify2.json`; the first run remains unchanged.

| Check | Observed result |
| --- | --- |
| Formatting | All files formatted |
| Clippy | No warnings |
| Nextest | 1,891 run; 1,891 passed; 13 skipped; notices: four slow, five leaky |
| Doc tests | One passed, zero failed, ten ignored |
| Architecture | Seven passed |
| Replays | Eight verified: one session, three testbed, four village |
| Golden snapshots | Six, none pending; `INSTA_UPDATE=no` |
| Dependency policy | Advisories, bans, licenses and sources passed |

All gate tools ran. No protected oracle, architecture-test, lint-policy, verify-tool or hook
file was changed to obtain this verdict. The Nextest scheduling declaration does
not skip tests or add retries. Its JUnit report retains failure details.

## Final binary and feedback checks

- `cargo build --locked -p game_menu -p actor_testbed -p npcctl` passed after the
  complete gate. The tested `game_menu.exe` SHA-256 is
  `f12e497b6eb3fe3fc9a2261fe3b8bebea693f98113c352fef3d2aaf3fa66d4d2`.
- Final-binary run `playtest-player-flow-e64b4a6163` passed copied-save Continue,
  all player workspaces, paused save, title return, Continue and Quit. The source
  snapshot stayed unchanged. The inspected Continue capture now shows ready
  character controls rather than the stale joining message.
- Final-binary market attempts `10742ea08a` and `f73cf05066` left a real grain
  offer unfilled; a further workshop-oriented harness route `687a2fc6dd` could not
  discover its target within the route budget. Those failed reports remain
  unchanged. They are not evidence of completed sales. The earlier native
  autonomous one-log/seven-coin purchase remains the live economic evidence;
  the final passive-income message is covered by the passing real-transaction
  regressions, but was not recaptured after the last feedback-only correction.
- The final-source extended seasonal audit passed again: **336,000 ticks, twelve
  surviving residents**, continued winter/replacement output, and the sampled
  navigation-stall criterion passed. Minimum needs were Energy 52,048, Hunger
  48,610, Thirst 54,382 and Warmth 23,468 (out of 100,000). It produced 27 warm
  garments, 277 logs, 104 planks, 108 bread and 58 meals. See
  `target/verified-seasonal-audit.log` for the full bounded audit.
- The ordinary `game_menu.exe` was launched from the workspace with no renderer
  override and no remote-interface flag. Process 38028 exposed a responding
  **Island Village | NPC Framework** window. Startup logs confirmed the remote
  interface was off. Launch evidence is in `target/player-handoff.json` and
  `target/player-handoff.log`. Opening the title does not advance or overwrite
  the user's saved island.

Physical keyboard/mouse input and assistive-technology behavior were not claimed
as tested. This increment implements the coherent island-life flow and the
documented simulation improvements; it is not a claim of commercial Stardew Valley
feature parity. No commit, push or pull request was created.
