### Task 20: AI seam — the controller host

**Track:** client + content · **Refs:** ADR-0016, spec §8 · **Lane 2** (after Task 2)

**Files:**
- Create: `crates/fl_client/src/ai.rs`, `crates/fl_client/tests/ai.rs`, `crates/fl_game/tests/ai.rs`, `assets/base/scenarios/testbed_ai.scenario.ron` (the testbed with player 1 `Ai("null")`)
- Modify: `crates/fl_client/src/lib.rs`, `crates/fl_game/src/builder.rs`

**Interfaces:**
- Consumes: `PlayerView`, `AiController`, `ai_controller` (Task 2); `SnapshotBuffer`, `LocalDriver`, `RunnerLink`.
- Produces:
```rust
#[derive(Resource, Default)] pub struct AiHost { pub players: BTreeMap<PlayerId, (TeamId, Box<dyn AiController>)>, pub thinks: u64 }
pub struct AiPlugin;   // Update, after SimViewSystems: when SnapshotBuffer.curr's tick advanced this frame, each controller thinks once on
                       // PlayerView::from_snapshot; each draft is issued through LocalDriver if draft.player is that AI's player, else dropped with a warn
// fl_game: GameBuilder fills AiHost from Players (Controller::Ai(id)); an unknown id fails prepare(): "unknown AI controller 'x' for player 1"
```

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_client/tests/ai.rs (fake runner)
#[test] fn null_ai_issues_nothing()             // 20 ticks: LocalDriver.pending() empty; AiHost.thinks == ticks observed
#[test] fn ai_thinks_once_per_new_tick()        // 3 TickOutputs in one frame → one think that frame
#[test] fn draft_for_another_player_is_dropped()   // a test AI for player 1 drafting for player 0 → nothing issued
// crates/fl_game/tests/ai.rs (prepare_headless)
#[test] fn ai_scenario_builds_host()            // scenario "base.testbed_ai": AiHost has player 1 with id "null"
#[test] fn unknown_ai_is_error()                // a scenario with Ai("nope") → Err containing "unknown AI controller 'nope'"
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client --test ai -p fl_game --test ai`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_client -p fl_game` and `cargo run -p fl_cli -- data-check assets/base`. Expected: PASS; exit 0.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client --track content`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_client): AI controller host on PlayerView with a do-nothing Controller::Ai` with trailer `Refs: ADR-0016`.

