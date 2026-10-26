# Task 16 review

### Spec Compliance
- ✅ Spec compliant. Every brief file/interface is present (orders.rs, tests/orders.rs, lib.rs, stats.rs hotkey list, console `terrain`). All 14 brief tests exist, plus 3 extras.
- ⚠️ Not verifiable from the diff: the live mouse path (ClickedUnit.secondary, CursorGround, system ordering vs `FillOnScreen`/`ApplySelectInput`) is only covered by the demo checklist. Run it by hand: rotate/follow-mode left click must not clear the selection.

### Strengths
- Orders go only through `LocalDriver::issue(CommandDraft)` as `LocalPlayer`, with no local sim mutation (orders.rs:583-594). One command per order, units sorted via the BTreeSet.
- `order_for` is pure and table-testable. Shift sets `queue` on the draft (orders.rs:323-345). Stop is never queued.
- Mode clicks are drained from `SelectInput` after `mouse_input` and before `FillOnScreen`, so they cannot clear the selection (orders.rs:446-449, 485-501). Non-click select inputs are re-written.
- Pointer and keyboard are gated by `InputGate` (orders.rs:502, 530). A paused driver still queues orders. Tests use a frozen clock and no runner thread.
- Encode failure gives a warn and no panic. Follow strips the leader.

### Issues
#### Critical
- none
#### Important
- orders.rs:379-388 + camera.rs:279: default rotate key R also fires camera snap-yaw, so one press does both. Camera keys at camera.rs:122-127,278 (WASD/Q/E/R) are not gated by egui/console either (pre-existing). The 14-line fix is to rebind one side.
- orders.rs:583-593 / :642: a selection over 256 units is sent as a single command and the sim rejects it (`core.too_many_units`, fl_sim commands.rs:517). The client gives no feedback and the order silently fails. Split into chunks of `MAX_COMMAND_UNITS`, or at least warn.
#### Minor
- tests/orders.rs:1167-1196: the brief requires "one warn log" for an unknown command name, but the test asserts only that there is no command. The warn is not verified.
- orders.rs:666: any order (including Stop, or a right click that happens to produce an order) resets the mode. A right click in Rotate mode silently cancels it. Probably intended, but it is undocumented.
- orders.rs:565-571: `commands.insert_resource(LocalPlayer)` is deferred, so `apply_orders` in the same frame still uses the old player. This is harmless because the selection is cleared in the same frame.
- Report: "RED run not captured". TDD evidence is missing, though the tests are meaningful.
- selection.rs visibility widening (`pub(crate)`) is minimal and acceptable.

### Assessment
**Task quality:** Approved (after the lead rules on the two known items; neither is a spec violation)
**Reasoning:** The command path, Shift-queue semantics, input gating and mode-click consumption are correct and deterministically tested. The two known items are real usability and robustness gaps, not spec breaks.

### Recommendation on the known items
- R collision: do not leave it. The brief mandates R for rotate, so change the camera side. Move snap-yaw off R in camera.rs:279 (for example Home or Z) and update its test and doc. If the lead prefers to keep the camera as is, rebind `TestbedKeys.rotate` to a free key (for example T). It is a one-line default change, and the stats window already prints the actual bindings.
- >256 selection: chunk it in `apply_orders` (one command per 256 units, same payload and queue flag), plus a test. Alternatively accept it for M1 and log a warn. I recommend chunking because it is cheap and deterministic. A hard-fail without feedback is worse than the extra commands.
