### Task 15: Selection — click, box, Shift-add, double-click, control groups

**Track:** client · **Refs:** spec §10, research §1.9 · **Lane 2** (from `main` at Phase I start; no contract dependency)

**Files:**
- Create: `crates/fl_client/src/selection.rs`, `crates/fl_client/tests/selection.rs`
- Modify:
  - `crates/fl_client/src/lib.rs` (`SelectionPlugin` in `ClientPlugins`; `MeshPickingPlugin` added if absent)
  - `crates/fl_client/src/debug/remote.rs` (`fl.select`, `fl.selection`)
  - `crates/fl_client/tests/remote.rs`

**Interfaces:**
- Consumes: `UnitView`, `ViewIndex`, `SnapshotBuffer`, `LocalPlayer`, `RtsCamera`, `ConsoleState`, `SpawnPanel`.
- Produces:
```rust
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)] pub struct Selection(pub BTreeSet<SimId>);
#[derive(Resource, Default, Debug)] pub struct ControlGroups(pub [BTreeSet<SimId>; 10]);
#[derive(Resource, Default, Debug, Clone, Copy)] pub struct ClickedUnit { pub primary: Option<SimId>, pub secondary: Option<SimId> }
    // set by the On<Pointer<Click>> observer on unit visual roots (attached by an Added<UnitView> system); cleared in Last
#[derive(Resource, Default, Debug)] pub struct OnScreen(pub Vec<(SimId, Vec2)>);
    // own units' viewport positions; filled lazily (only in frames with a box release or a double-click) via Camera::world_to_viewport
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub enum SelectInput { Click { sim_id: Option<SimId>, shift: bool, at: Duration }, Box { min: Vec2, max: Vec2, shift: bool },
                       Group { slot: u8, action: GroupAction } }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum GroupAction { Assign, Recall, Add, Center }
pub const DOUBLE_CLICK: Duration = Duration::from_millis(350);
pub const DRAG_THRESHOLD_PX: f32 = 6.0;
pub fn box_select(min: Vec2, max: Vec2, points: impl IntoIterator<Item = (SimId, Vec2)>) -> BTreeSet<SimId>;   // corners normalized; edges inclusive
pub fn classify_release(press: Vec2, release: Vec2) -> ReleaseKind;   // pub enum ReleaseKind { Click, Box { min: Vec2, max: Vec2 } }; < DRAG_THRESHOLD_PX → Click
pub struct SelectionPlugin;
// BRP (debug_remote): fl.select { "sim_ids": [..] } replaces the selection (own units only) → {"selected": [..]}; fl.selection → {"selected": [..]}
```
- **Rules.**
  - Only units owned by `LocalPlayer` are selected.
  - `Click(Some(own))`: with Shift, toggle that unit; otherwise, select only it.
  - A second click on the same unit within `DOUBLE_CLICK` selects every on-screen own unit of the same `UnitTypeId` (with Shift, adds them).
  - `Click(None)`: without Shift, clear; with Shift, keep.
  - `Box`: own units inside the box replace the selection, or are added with Shift.
  - Groups:
    - Ctrl+digit assigns (an empty selection clears the group).
    - Digit recalls (only units still in the snapshot).
    - Shift+digit adds the group to the selection.
    - Alt+digit recalls and sets `RtsCamera.focus` to the centroid of the group's visuals.
  - Pruning: units missing from `SnapshotBuffer.curr` leave the selection and the groups. A `LocalPlayer` change clears the selection.
  - Rings: a gizmo circle at every selected unit's `Transform`, radius = half the larger placeholder side + 0.5 m.

- [ ] **Step 1: Write the failing tests** (World level: unit visuals spawned by the fake-runner snapshot path, `ManualDuration` time, `SelectInput` messages written directly, `OnScreen` inserted directly):
```rust
#[test] fn click_selects_own_unit_only()               // click on enemy #6 → empty; click on own #1 → {#1}
#[test] fn shift_click_toggles()                        // {#1} + shift-click #2 → {#1,#2}; shift-click #1 → {#2}
#[test] fn click_empty_ground_clears_unless_shift()
#[test] fn double_click_selects_same_type_on_screen()  // OnScreen: own pz3 #1 #2 #3, own tiger #4, enemy pz3 #6; clicks on #1 at 0 and 200 ms → {#1,#2,#3}
#[test] fn slow_second_click_is_a_single_click()        // second click at 400 ms → {#1}
#[test] fn box_select_inclusive_and_normalized()        // pure: corners (10,10)-(0,0); points (0,0) and (10,5) in, (11,5) out
#[test] fn box_shift_adds()
#[test] fn tiny_drag_is_a_click()                       // classify_release((0,0), (3,4)) == Click; ((0,0), (6,0)) == Box
#[test] fn control_group_assign_recall_add()
#[test] fn control_group_drops_dead_units()             // #2 vanishes from the snapshot → group 1 {#1}, selection {#1}
#[test] fn ctrl_digit_with_empty_selection_clears_group()
#[test] fn alt_digit_centers_camera()                   // group visuals at x 10 and 30 → RtsCamera.focus.x == 20
#[test] fn player_switch_clears_selection()
#[test] fn input_ignored_while_console_open()           // ConsoleState.open + Digit1 press → no SelectInput written
// crates/fl_client/tests/remote.rs (brp_, debug_remote)
#[test] fn brp_select_and_read_selection()              // fl.select {sim_ids:[1,6]} → {"selected":[1]}; fl.selection → [1]
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client --test selection`. Expected: FAIL.
- [ ] **Step 3: Implement.** Check `Pointer<Click>`, `On<…>`, `MeshPickingPlugin`, `PointerButton` and `Camera::world_to_viewport` with the `bevy-api` skill.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_client`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: `gate task: PASS`.
- [ ] **Step 6: Commit** `feat(fl_client): selection — click and box picking, Shift-add, double-click same type, control groups, rings`.

---

## Phase II — sim navigation (lane 1); content, input, terrain view, AI host, overlays (lane 2)

