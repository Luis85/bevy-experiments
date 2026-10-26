# Task 16 report: orders input and testbed controls

Commit: see `git log m1/t16-orders` (feat(fl_client): right-click orders, ...). Worktree C:\fl\t16.

## Implemented
- `crates/fl_client/src/orders.rs`: `OrderMode`, `OrderInput`, `PlannedOrder`, pure `order_for`, `ground_to_sim`, `TestbedKeys`, `SPEED_STEPS`, `next_speed`, `next_player`, `OrdersPlugin` (added to `ClientPlugins`).
  - `mouse_orders`: right-click release -> `RightClick`; while a mode is on it takes the selection's left `SelectInput::Click`s out of the stream and turns them into `LeftClickInMode` (so a rotate/follow click doesn't clear the selection).
  - `key_orders`: stop, rotate/follow mode toggles, pause, speed steps, cycle player (clears selection and mode). Gated by `InputGate` (console open / egui keyboard).
  - `apply_orders`: one `PlayerCommand` per order for the sorted selection as `LocalPlayer`, with `queue` = Shift. Mode resets to Normal after an order unless Shift. Encode failure (name missing from header) -> one `tracing::warn!`, no command, no panic. An enemy right-click with no ground hit moves to the enemy's snapshot position. Follow strips the leader from the unit list.
- Console `terrain <x0> <y0> <x1> <y1> <id>` -> `debug.set_terrain` as LocalPlayer (corners normalised to min/max).
- Time window lists the hotkeys and uses `SPEED_STEPS`.
- `selection.rs` (Task 15): visibility only (`mouse_input`, `InputGate` + methods, `PressState` -> `pub(crate)`), needed to order `mouse_orders` after it.

## Tests
`cargo test -p fl_client --test orders`: 17/17 pass (the 14 brief tests plus follow-mode click, mode click taken from selection with Shift keeps mode, ground_to_sim). Fake runner: no runner thread; the runner end is kept alive in a resource (a dropped end fails the match and pauses the driver), clock frozen with `ManualDuration(ZERO)`, so deterministic. `orders_while_paused_apply_on_resume` drives `LocalDriver::advance/poll` directly (tick 10 empty, tick 11 carries the move). Rotate/follow modes resolve via the real `mouse_orders` path in one test.
Gate: `cargo xtask gate task --track client` -> `gate task: PASS in 128s` (first run failed on an unfulfilled `expect(too_many_arguments)`, fixed).
TDD: tests written together with the implementation; RED run not captured separately.

## Concerns
- Default Rotate key `R` collides with the camera's snap-yaw on `R` (camera.rs). Brief mandates R; M6 rebinds.
- A selection over `MAX_COMMAND_UNITS` (256) would be rejected by the sim as one command (not split).
- Mouse/picking wiring (ClickedUnit.secondary, CursorGround) is not headless-testable; demo checklist.

## Fix round 1
- R87: camera snap-yaw moved from R to Home (`camera.rs`); `pan_camera` and `rotate_camera` now honour `InputGate::keyboard_blocked` (console open / egui keyboard). README, docs/debugging.md and the M0 checklist row mention Home (docs touched, so the gate ran with `--track client --track docs`). New tests `home_snaps_yaw_and_r_does_not`, `camera_keys_ignored_while_console_open`.
  RED: `assertion failed: yaw_after(KeyCode::Home, false).abs() < 1e-6` (camera::tests::home_snaps_yaw_and_r_does_not) before the change.
- R88: `apply_orders` splits the unit list into consecutive commands of <= `MAX_COMMAND_UNITS` (256), same payload/queue. Test `big_selection_is_split_into_commands_of_at_most_256` (300 units -> 256 + 44, ascending ids, consecutive seq).
  RED (cap temporarily set to 100_000): `assertion left == right failed  left: 1 right: 2`.
- `unknown_command_name_is_reported` now asserts exactly one WARN (a counting `tracing::Subscriber`, systems on a single-threaded executor so the thread-local subscriber sees it).
- Documented that any resulting order ends the input mode (doc on `apply_orders`).
- Gate: `cargo xtask gate task --track client --track docs` -> `gate task: PASS in 167s`.
