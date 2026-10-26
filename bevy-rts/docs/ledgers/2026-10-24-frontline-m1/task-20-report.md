# Task 20 report: AI seam controller host

Commit: 761b8ec feat(fl_client): AI controller host on PlayerView with a do-nothing Controller::Ai (branch m1/t20-ai, worktree C:\fl\t20)

## Implemented
- crates/fl_client/src/ai.rs: `AiHost { players: BTreeMap<PlayerId,(TeamId,Box<dyn AiController>)>, thinks }`, `AiPlugin`, `AiSystems`.
  `ai_think` runs in Update after SimViewSystems (needs RunnerLink + LocalDriver). It thinks every controller once when the newest
  snapshot's tick differs from the last thought-on tick (`!=` rather than `>` so a loaded match restarting at an earlier tick still works),
  on `PlayerView::from_snapshot(newest, player, team)` with `RunnerLink.header.string_tables`. Drafts whose `player` equals the AI's player go through
  `LocalDriver::issue`; others are dropped with `tracing::warn!`. `thinks` counts controller thinks (one per controller per new tick).
- lib.rs: `pub mod ai`, `pub use AiPlugin`, AiPlugin in ClientPlugins. builder.rs: `ai_host(&Players)` fills AiHost from `Controller::Ai(id)`
  players (before the sim thread starts); unknown id -> `unknown AI controller 'x' for player 1`; AiPlugin added to the headless shell; AiHost inserted.
- assets/base/scenarios/testbed_ai.scenario.ron (base.testbed_ai; player 1 `Ai("null")`).

## Tests (all pass)
- fl_client/tests/ai.rs (no runner thread: tick outputs are written into the runner end's channel before a frame, clock frozen -> deterministic):
  null_ai_issues_nothing (20 ticks, pending empty, thinks==20), no_snapshot_no_think, ai_thinks_once_per_new_tick (3 TickOutputs, one frame -> 1 think on newest tick;
  idle frame no think), drafts_are_stamped_for_the_ai_player, draft_for_another_player_is_dropped,
  team_beyond_the_visibility_bits_sees_nothing (team 8 -> `visible` empty, own kept), team_within_the_bits_sees_visible_records (control, team 7).
  The team>=8 test lives here (fl_view is outside my Files); it exercises fl_view's PlayerView through the host.
- fl_game/tests/ai.rs: ai_scenario_builds_host, human_scenario_has_no_ai, unknown_ai_is_error (temp mod layered on base with Ai("nope")).
- `cargo nextest run --profile agent -p fl_client -p fl_game`: 128/128. `fl_cli data-check assets/base`: exit 0 (scenario: 2).
- Gate: `gate task --track client --track content` -> `gate task: PASS in 374s` (WARN over 300s budget; all steps PASS, api SKIP).

## TDD note
Impl was written before the tests were first run; no separate RED run was captured. Tests were run once, green (10/10).

## Concerns
- AiHost is not refreshed by `fl.load_scenario`/`fl.load` (LoadedMatch in fl_client/src/debug/mod.rs, outside Files): after a mid-run load to a scenario with different
  AI players, the host keeps the original match's controllers. Follow-up: add players/AiHost to LoadedMatch.
- Gate took 374s vs 300s budget (cold test builds under the shared dir).

## Fix round 1
Commit 0f47cbe. Changes: foreign-player drafts now warn once per controller (private `warned` set in AiHost); every drop is counted in new `pub dropped_drafts`.
Added tests in fl_client/tests/ai.rs: repeated_foreign_drafts_are_all_dropped_and_counted, two_ai_players_each_think_once_per_tick_change, tick_regression_triggers_a_rethink (`!=` kept, R91).
RED: `cargo nextest run --profile agent -p fl_client --test ai` -> `error[E0609]: no field dropped_drafts on type &AiHost` (compile failure; the other two new tests already held).
GREEN: same command -> `10 tests run: 10 passed`.
Gate: `cargo xtask gate task --track client --track content` -> `gate task: PASS in 427s` (WARN over 300s budget).
