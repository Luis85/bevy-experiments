### Task 16: Orders input and testbed controls

**Track:** client · **Refs:** spec §10, §11, ADR-0008 · **Lane 2** (after Tasks 2 and 15)

**Files:**
- Create: `crates/fl_client/src/orders.rs`, `crates/fl_client/tests/orders.rs`
- Modify: `crates/fl_client/src/lib.rs`, `crates/fl_client/src/debug/stats.rs` (time-control window lists the hotkeys), `crates/fl_client/src/debug/console.rs` (console `terrain <x0> <y0> <x1> <y1> <id>` issues `debug.set_terrain` as `LocalPlayer`)

**Interfaces:**
- Consumes: `Selection`, `ClickedUnit` (Task 15); `CursorGround`; `RotateCmd`, `FollowCmd`, `MoveCmd`, `StopCmd`; `encode_command`, `LocalDriver`, `LocalPlayer`, `PlayerColors`.
- Produces:
```rust
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)] pub enum OrderMode { #[default] Normal, Rotate, Follow }
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub enum OrderInput { RightClick { ground: Option<Vec3>, unit: Option<SimId>, shift: bool, alt: bool },
                      LeftClickInMode { ground: Option<Vec3>, unit: Option<SimId>, shift: bool }, Stop }
#[derive(Clone, Copy, Debug, PartialEq)] pub enum PlannedOrder { Move { target: FxVec2, queue: bool }, Follow { leader: SimId, queue: bool },
                                                                 Rotate { toward: FxVec2, queue: bool }, Stop }
pub fn order_for(input: &OrderInput, mode: OrderMode, selection: &Selection, is_own: impl Fn(SimId) -> bool) -> Option<PlannedOrder>;
    // empty selection → None. RightClick: an own unit and !alt → Follow; otherwise the ground point → Move (an enemy unit's position when no ground hit).
    // LeftClickInMode: Rotate mode → Rotate at the ground point; Follow mode → Follow the clicked unit. Stop → Stop.
pub fn ground_to_sim(p: Vec3) -> FxVec2;   // (x, z) metres via Fx::from_f64
#[derive(Resource, Clone, Debug)] pub struct TestbedKeys { pub pause: KeyCode, pub speed_up: KeyCode, pub speed_down: KeyCode, pub cycle_player: KeyCode,
                                                           pub stop: KeyCode, pub rotate: KeyCode, pub follow: KeyCode }
    // defaults: P, Equal, Minus, F4, H, R, F (M6's command grid will rebind them)
pub const SPEED_STEPS: [f32; 6] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0];
pub struct OrdersPlugin;   // one PlayerCommand per order: every selected unit, sorted, as LocalPlayer; the mode resets to Normal after an order unless Shift
```

- [ ] **Step 1: Write the failing tests** (fake runner; header tables list `core.follow`, `core.move`, `core.rotate`, `core.stop`):
```rust
#[test] fn right_click_ground_issues_move()      // selection {#2,#1}: one core.move, units [#1,#2], target decodes to the ground point, queue false
#[test] fn shift_right_click_queues()
#[test] fn right_click_own_unit_follows()        // FollowCmd { leader: #3 }
#[test] fn alt_right_click_unit_forces_move()
#[test] fn right_click_enemy_moves_to_it()       // M1 has no attack: Move to the enemy's position
#[test] fn rotate_mode_left_click_issues_rotate()
#[test] fn stop_key_issues_stop()
#[test] fn no_selection_no_command()
#[test] fn orders_while_paused_apply_on_resume() // pause at next 10; right-click → exec_tick 11; resume → the TickInput for 11 carries it
#[test] fn speed_keys_step_through_speeds()      // 1 → 2 → 4 → 8 → 8; and down to 0.25, which holds
#[test] fn pause_key_toggles()
#[test] fn cycle_player_wraps()                  // PlayerColors {0, 1}: F4 → LocalPlayer 1 → 0; the selection clears
#[test] fn unknown_command_name_is_reported()    // a header without "core.rotate": rotate order → no command, one warn log, no panic
#[test] fn console_terrain_issues_debug_set_terrain()   // "terrain 70 122 72 127 base.water" → one debug.set_terrain { (70,122), (72,127), "base.water" }
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client --test orders`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_client`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_client): right-click orders, rotate/follow/stop keys, Shift-queue, testbed pause/speed/player hotkeys`.

