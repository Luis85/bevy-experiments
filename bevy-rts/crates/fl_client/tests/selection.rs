//! Selection tests: `SelectInput` messages are written straight into a headless client
//! (`MinimalPlugins` + `SimViewPlugin` + `SelectionPlugin`) whose snapshots are pushed into the
//! `SnapshotBuffer`. No runner thread is involved, so every frame is deterministic.
//! Picking and `Camera::world_to_viewport` need a window and a GPU; their logic is tested here
//! through the pure functions, and `OnScreen` is filled by hand.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::prelude::*;
use fl_client::camera::RtsCamera;
use fl_client::debug::{ConsoleState, LoadRequest, LoadedMatch, ScenarioLoader, replace_match};
use fl_client::driver::LocalDriver;
use fl_client::link::{RunnerLink, SnapshotBuffer};
use fl_client::selection::{
    ApplySelectInput, ControlGroups, FillOnScreen, GroupAction, OnScreen, ReleaseKind, SelectInput,
    Selection, SelectionPlugin, box_select, classify_release, ring_radius,
};
use fl_client::view::{LocalPlayer, SimViewPlugin};
use fl_content::{DefRegistry, Placeholder, load_dirs};
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{MatchHeader, PlayerId, SimId, StringTables, Tick, UnitTypeId};
use fl_view::{EntityRecord, RunnerEnd, SimSnapshot, runner_channel};

const PZ3: UnitTypeId = UnitTypeId(0);
const TIGER: UnitTypeId = UnitTypeId(1);
const ME: PlayerId = PlayerId(0);
const FOE: PlayerId = PlayerId(1);

fn rec(id: u32, unit: UnitTypeId, owner: PlayerId, x: i32) -> EntityRecord {
    EntityRecord {
        sim_id: SimId(id),
        unit,
        owner,
        pos: FxVec2::new(Fx::from_int(x), Fx::from_int(0)),
        heading: Angle::from_raw(0),
        turrets: Vec::new(),
        hp_frac: u16::MAX,
        flags: 0,
        visible_to: 0xFF,
    }
}

/// Own pz3 #1 #2 #3, own tiger #4, enemy pz3 #6.
fn standard() -> Vec<EntityRecord> {
    vec![
        rec(1, PZ3, ME, 10),
        rec(2, PZ3, ME, 20),
        rec(3, PZ3, ME, 30),
        rec(4, TIGER, ME, 40),
        rec(6, PZ3, FOE, 60),
    ]
}

/// The positions the test wants `OnScreen` to hold; without a camera the plugin's own fill
/// would leave it empty, so this runs right after it.
#[derive(Resource, Default)]
struct Injected(Vec<(SimId, Vec2)>);

fn inject(injected: Res<Injected>, mut on_screen: ResMut<OnScreen>) {
    on_screen.0.clone_from(&injected.0);
}

fn app_with(records: Vec<EntityRecord>) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, SimViewPlugin, SelectionPlugin))
        .init_resource::<Injected>()
        .add_systems(Update, inject.after(FillOnScreen).before(ApplySelectInput))
        .insert_resource(LocalPlayer(ME));
    set_world(&mut app, records);
    app
}

/// Makes `records` the newest snapshot and runs a frame.
fn set_world(app: &mut App, records: Vec<EntityRecord>) {
    let tick = app
        .world()
        .resource::<SnapshotBuffer>()
        .curr
        .as_ref()
        .map_or(0, |s| s.tick.0 + 1);
    app.world_mut()
        .resource_mut::<SnapshotBuffer>()
        .push(Arc::new(SimSnapshot {
            tick: Tick(tick),
            entities: records,
            channels: Vec::new(),
        }));
    app.update();
}

fn send(app: &mut App, input: SelectInput) {
    app.world_mut()
        .resource_mut::<Messages<SelectInput>>()
        .write(input);
    app.update();
}

fn at(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

/// A click on `id` (or the ground) at `ms`, without Shift.
fn click(app: &mut App, id: Option<u32>, ms: u64) {
    send(
        app,
        SelectInput::Click {
            sim_id: id.map(SimId),
            shift: false,
            at: at(ms),
        },
    );
}

fn shift_click(app: &mut App, id: Option<u32>, ms: u64) {
    send(
        app,
        SelectInput::Click {
            sim_id: id.map(SimId),
            shift: true,
            at: at(ms),
        },
    );
}

fn selected(app: &App) -> Vec<u32> {
    app.world()
        .resource::<Selection>()
        .0
        .iter()
        .map(|id| id.0)
        .collect()
}

fn group(app: &App, slot: usize) -> Vec<u32> {
    app.world().resource::<ControlGroups>().0[slot]
        .iter()
        .map(|id| id.0)
        .collect()
}

fn group_key(app: &mut App, slot: u8, action: GroupAction) {
    send(app, SelectInput::Group { slot, action });
}

/// `OnScreen` with each unit of `ids` at `(id, 0)`.
fn on_screen(app: &mut App, ids: &[u32]) {
    app.world_mut().resource_mut::<Injected>().0 = ids
        .iter()
        .map(|&id| (SimId(id), Vec2::new(id as f32, 0.0)))
        .collect();
}

#[test]
fn click_selects_own_unit_only() {
    let mut app = app_with(standard());
    click(&mut app, Some(6), 0);
    assert_eq!(selected(&app), Vec::<u32>::new());
    click(&mut app, Some(1), 1000);
    assert_eq!(selected(&app), [1]);
}

#[test]
fn shift_click_toggles() {
    let mut app = app_with(standard());
    click(&mut app, Some(1), 0);
    shift_click(&mut app, Some(2), 1000);
    assert_eq!(selected(&app), [1, 2]);
    shift_click(&mut app, Some(1), 2000);
    assert_eq!(selected(&app), [2]);
}

#[test]
fn click_empty_ground_clears_unless_shift() {
    let mut app = app_with(standard());
    click(&mut app, Some(1), 0);
    shift_click(&mut app, None, 1000);
    assert_eq!(selected(&app), [1]);
    click(&mut app, None, 2000);
    assert_eq!(selected(&app), Vec::<u32>::new());
}

#[test]
fn double_click_selects_same_type_on_screen() {
    let mut app = app_with(standard());
    on_screen(&mut app, &[1, 2, 3, 4, 6]);
    click(&mut app, Some(1), 0);
    assert_eq!(selected(&app), [1]);
    click(&mut app, Some(1), 200);
    assert_eq!(selected(&app), [1, 2, 3]);
}

#[test]
fn double_click_only_takes_units_on_screen() {
    let mut app = app_with(standard());
    on_screen(&mut app, &[1, 3]);
    click(&mut app, Some(1), 0);
    click(&mut app, Some(1), 100);
    assert_eq!(selected(&app), [1, 3]);
}

#[test]
fn double_click_on_an_enemy_selects_nothing() {
    let mut app = app_with(standard());
    on_screen(&mut app, &[1, 2, 3, 4, 6]);
    click(&mut app, Some(6), 0);
    click(&mut app, Some(6), 100);
    assert_eq!(selected(&app), Vec::<u32>::new());
}

#[test]
fn slow_second_click_is_a_single_click() {
    let mut app = app_with(standard());
    on_screen(&mut app, &[1, 2, 3, 4, 6]);
    click(&mut app, Some(1), 0);
    click(&mut app, Some(1), 400);
    assert_eq!(selected(&app), [1]);
}

#[test]
fn box_select_inclusive_and_normalized() {
    let points = [
        (SimId(1), Vec2::new(0.0, 0.0)),
        (SimId(2), Vec2::new(10.0, 5.0)),
        (SimId(3), Vec2::new(11.0, 5.0)),
    ];
    let picked = box_select(Vec2::new(10.0, 10.0), Vec2::new(0.0, 0.0), points);
    assert_eq!(picked, BTreeSet::from([SimId(1), SimId(2)]));
}

#[test]
fn box_replaces_and_ignores_enemies() {
    let mut app = app_with(standard());
    on_screen(&mut app, &[1, 2, 3, 4, 6]);
    click(&mut app, Some(4), 0);
    // OnScreen positions are (id, 0): this box covers ids 1 to 6.
    send(
        &mut app,
        SelectInput::Box {
            min: Vec2::new(0.0, -1.0),
            max: Vec2::new(7.0, 1.0),
            shift: false,
        },
    );
    assert_eq!(selected(&app), [1, 2, 3, 4]);
}

#[test]
fn box_shift_adds() {
    let mut app = app_with(standard());
    on_screen(&mut app, &[1, 2, 3, 4, 6]);
    send(
        &mut app,
        SelectInput::Box {
            min: Vec2::new(0.0, -1.0),
            max: Vec2::new(2.0, 1.0),
            shift: false,
        },
    );
    assert_eq!(selected(&app), [1, 2]);
    send(
        &mut app,
        SelectInput::Box {
            min: Vec2::new(3.5, -1.0),
            max: Vec2::new(4.5, 1.0),
            shift: true,
        },
    );
    assert_eq!(selected(&app), [1, 2, 4]);
}

#[test]
fn tiny_drag_is_a_click() {
    let origin = Vec2::ZERO;
    assert_eq!(
        classify_release(origin, Vec2::new(3.0, 4.0)),
        ReleaseKind::Click
    );
    assert_eq!(
        classify_release(origin, Vec2::new(6.0, 0.0)),
        ReleaseKind::Box {
            min: origin,
            max: Vec2::new(6.0, 0.0)
        }
    );
    // The corners come out normalized whichever way the drag went.
    assert_eq!(
        classify_release(Vec2::new(10.0, 10.0), Vec2::new(0.0, 20.0)),
        ReleaseKind::Box {
            min: Vec2::new(0.0, 10.0),
            max: Vec2::new(10.0, 20.0)
        }
    );
}

#[test]
fn control_group_assign_recall_add() {
    let mut app = app_with(standard());
    click(&mut app, Some(1), 0);
    shift_click(&mut app, Some(2), 1000);
    group_key(&mut app, 1, GroupAction::Assign);
    assert_eq!(group(&app, 1), [1, 2]);

    click(&mut app, None, 2000);
    assert_eq!(selected(&app), Vec::<u32>::new());
    group_key(&mut app, 1, GroupAction::Recall);
    assert_eq!(selected(&app), [1, 2]);

    click(&mut app, Some(3), 3000);
    group_key(&mut app, 1, GroupAction::Add);
    assert_eq!(selected(&app), [1, 2, 3]);
}

#[test]
fn control_group_drops_dead_units() {
    let mut app = app_with(standard());
    click(&mut app, Some(1), 0);
    shift_click(&mut app, Some(2), 1000);
    group_key(&mut app, 1, GroupAction::Assign);

    let mut records = standard();
    records.retain(|r| r.sim_id != SimId(2));
    set_world(&mut app, records);
    assert_eq!(selected(&app), [1]);
    assert_eq!(group(&app, 1), [1]);
}

#[test]
fn ctrl_digit_with_empty_selection_clears_group() {
    let mut app = app_with(standard());
    click(&mut app, Some(1), 0);
    group_key(&mut app, 1, GroupAction::Assign);
    assert_eq!(group(&app, 1), [1]);

    click(&mut app, None, 1000);
    group_key(&mut app, 1, GroupAction::Assign);
    assert_eq!(group(&app, 1), Vec::<u32>::new());
}

#[test]
fn alt_digit_centers_camera() {
    let records = vec![rec(1, PZ3, ME, 10), rec(2, PZ3, ME, 30)];
    let mut app = app_with(records);
    let camera = app.world_mut().spawn(RtsCamera::default()).id();
    click(&mut app, Some(1), 0);
    shift_click(&mut app, Some(2), 1000);
    group_key(&mut app, 3, GroupAction::Assign);
    click(&mut app, None, 2000);

    group_key(&mut app, 3, GroupAction::Center);
    assert_eq!(selected(&app), [1, 2]);
    let focus = app.world().get::<RtsCamera>(camera).unwrap().focus;
    assert_eq!(focus.x, 20.0);
}

#[test]
fn player_switch_clears_selection() {
    let mut app = app_with(standard());
    click(&mut app, Some(1), 0);
    assert_eq!(selected(&app), [1]);
    app.world_mut().insert_resource(LocalPlayer(FOE));
    app.update();
    assert_eq!(selected(&app), Vec::<u32>::new());
    // The enemy is now ours, and ours is the enemy.
    click(&mut app, Some(1), 1000);
    assert_eq!(selected(&app), Vec::<u32>::new());
    click(&mut app, Some(6), 2000);
    assert_eq!(selected(&app), [6]);
}

/// Every `SelectInput` written since the last [`seen`] call.
#[derive(Resource, Default)]
struct Seen(Vec<SelectInput>);

fn collect(mut reader: MessageReader<SelectInput>, mut seen: ResMut<Seen>) {
    seen.0.extend(reader.read().copied());
}

fn watch_inputs(app: &mut App) {
    app.init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ConsoleState>()
        .init_resource::<Seen>()
        .add_systems(PostUpdate, collect);
}

fn seen(app: &mut App) -> Vec<SelectInput> {
    std::mem::take(&mut app.world_mut().resource_mut::<Seen>().0)
}

fn press_only(app: &mut App, keys: &[KeyCode]) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    input.reset_all();
    for &key in keys {
        input.press(key);
    }
}

#[test]
fn input_ignored_while_console_open() {
    let mut app = app_with(standard());
    watch_inputs(&mut app);

    app.world_mut().resource_mut::<ConsoleState>().open = true;
    press_only(&mut app, &[KeyCode::Digit1]);
    app.update();
    assert_eq!(seen(&mut app), []);

    // The same press with the console closed is a group recall.
    app.world_mut().resource_mut::<ConsoleState>().open = false;
    press_only(&mut app, &[KeyCode::Digit1]);
    app.update();
    assert_eq!(
        seen(&mut app),
        [SelectInput::Group {
            slot: 1,
            action: GroupAction::Recall
        }]
    );
}

#[test]
fn modifiers_choose_the_group_action() {
    let mut app = app_with(standard());
    watch_inputs(&mut app);
    let cases = [
        (None, GroupAction::Recall),
        (Some(KeyCode::ControlLeft), GroupAction::Assign),
        (Some(KeyCode::ShiftRight), GroupAction::Add),
        (Some(KeyCode::AltLeft), GroupAction::Center),
    ];
    for (modifier, action) in cases {
        let mut keys = vec![KeyCode::Digit0];
        keys.extend(modifier);
        press_only(&mut app, &keys);
        app.update();
        assert_eq!(seen(&mut app), [SelectInput::Group { slot: 0, action }]);
    }
}

#[test]
fn recalling_an_empty_or_dead_group_does_nothing() {
    let mut app = app_with(standard());
    click(&mut app, Some(3), 0);
    // Slot 5 was never assigned.
    group_key(&mut app, 5, GroupAction::Recall);
    group_key(&mut app, 5, GroupAction::Add);
    assert_eq!(selected(&app), [3]);

    // Slot 2 holds only #1, which then dies.
    click(&mut app, Some(1), 1000);
    group_key(&mut app, 2, GroupAction::Assign);
    click(&mut app, Some(3), 2000);
    let mut records = standard();
    records.retain(|r| r.sim_id != SimId(1));
    set_world(&mut app, records);
    group_key(&mut app, 2, GroupAction::Recall);
    group_key(&mut app, 2, GroupAction::Center);
    assert_eq!(selected(&app), [3]);
}

#[test]
fn shift_double_click_adds_the_type_to_the_selection() {
    let mut app = app_with(standard());
    on_screen(&mut app, &[1, 2, 3, 4, 6]);
    click(&mut app, Some(4), 0);
    shift_click(&mut app, Some(1), 1000);
    shift_click(&mut app, Some(1), 1100);
    assert_eq!(selected(&app), [1, 2, 3, 4]);
}

#[test]
fn on_screen_is_emptied_without_a_camera() {
    let mut app = app_with(standard());
    app.world_mut().resource_mut::<OnScreen>().0 = vec![(SimId(1), Vec2::ZERO)];
    click(&mut app, Some(1), 0);
    assert!(app.world().resource::<OnScreen>().0.is_empty());
}

/// Presses and releases the left button at `from` and `to` on a window with no GPU.
fn drag(app: &mut App, from: Vec2, to: Vec2) {
    let move_to = |app: &mut App, at: Vec2| {
        let mut windows = app.world_mut().query::<&mut Window>();
        windows
            .single_mut(app.world_mut())
            .unwrap()
            .set_cursor_position(Some(at));
    };
    move_to(app, from);
    let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
    mouse.reset_all();
    mouse.press(MouseButton::Left);
    app.update();
    move_to(app, to);
    let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
    mouse.clear();
    mouse.release(MouseButton::Left);
    app.update();
}

#[test]
fn tiny_drag_reaches_the_sim_as_a_click_and_a_long_one_as_a_box() {
    let mut app = app_with(standard());
    watch_inputs(&mut app);
    app.init_resource::<ButtonInput<MouseButton>>()
        .world_mut()
        .spawn((Window::default(), bevy::window::PrimaryWindow));
    app.update();
    seen(&mut app);

    drag(&mut app, Vec2::new(100.0, 100.0), Vec2::new(103.0, 104.0));
    let got = seen(&mut app);
    assert!(
        matches!(
            got[..],
            [SelectInput::Click {
                sim_id: None,
                shift: false,
                ..
            }]
        ),
        "{got:?}"
    );

    drag(&mut app, Vec2::new(100.0, 100.0), Vec2::new(90.0, 140.0));
    let got = seen(&mut app);
    assert_eq!(
        got,
        [SelectInput::Box {
            min: Vec2::new(90.0, 100.0),
            max: Vec2::new(100.0, 140.0),
            shift: false
        }]
    );
}

#[test]
fn ring_is_half_the_larger_side_plus_half_a_metre() {
    let tank = Placeholder::Tank {
        hull: (3.0, 1.5, 6.0),
        turret: (2.0, 1.0, 2.5),
        barrel_len: 3.0,
    };
    assert_eq!(ring_radius(&tank, 1.0), 3.5);
    assert_eq!(ring_radius(&tank, 2.0), 6.5);
    let boxy = Placeholder::Box {
        size: (8.0, 2.0, 4.0),
    };
    assert_eq!(ring_radius(&boxy, 1.0), 4.5);
}

/// A paused [`LocalDriver`] and a [`ScenarioLoader`] that starts every match on a fresh
/// `runner_channel`; the runner ends are kept in the returned list, so no runner is gone.
fn swappable(app: &mut App) -> Arc<Mutex<Vec<RunnerEnd>>> {
    let mut driver = LocalDriver::new(Tick(0));
    driver.pause();
    app.insert_resource(driver);
    let ends = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&ends);
    app.insert_resource(ScenarioLoader(Box::new(move |_request| {
        let (port, end) = runner_channel();
        sink.lock().expect("not poisoned").push(end);
        Ok(LoadedMatch {
            link: RunnerLink {
                port,
                header: MatchHeader {
                    format_version: fl_protocol::FORMAT_VERSION,
                    build_id: "test".to_owned(),
                    sim_version: 1,
                    features: Vec::new(),
                    mods: Vec::new(),
                    rules_hash: 0,
                    cheats_allowed: true,
                    seed: 1,
                    string_tables: StringTables::default(),
                },
            },
            next: Tick(0),
            pending: Vec::new(),
            last_seq: BTreeMap::new(),
            map: None,
            ai: None,
            dropped: Vec::new(),
            hook: None,
        })
    })));
    ends
}

/// Selects #1 and #2 and puts them in group 1.
fn select_and_group(app: &mut App) {
    click(app, Some(1), 0);
    shift_click(app, Some(2), 1000);
    group_key(app, 1, GroupAction::Assign);
    assert_eq!(selected(app), [1, 2]);
    assert_eq!(group(app, 1), [1, 2]);
}

#[test]
fn loading_another_match_clears_selection_and_groups() {
    // Final review M9: the new match's sim ids name unrelated units.
    for request in [
        LoadRequest::Scenario("base.testbed".to_owned()),
        LoadRequest::Save(Vec::new()),
    ] {
        let mut app = app_with(standard());
        let _ends = swappable(&mut app);
        select_and_group(&mut app);
        replace_match(app.world_mut(), request).expect("loads");
        app.update();
        assert_eq!(selected(&app), Vec::<u32>::new());
        assert_eq!(group(&app, 1), Vec::<u32>::new());
    }
}

#[test]
fn a_rebuild_keeps_selection_and_groups() {
    let mut app = app_with(standard());
    let _ends = swappable(&mut app);
    select_and_group(&mut app);
    let content = load_dirs(DefRegistry::with_builtins(), &[]).expect("empty content loads");
    let request = LoadRequest::Rebuild {
        save: Vec::new(),
        content: Arc::new(content),
    };
    replace_match(app.world_mut(), request).expect("rebuilds");
    app.update();
    assert_eq!(selected(&app), [1, 2]);
    assert_eq!(group(&app, 1), [1, 2]);
}

#[test]
fn a_selected_unit_given_away_leaves_the_selection() {
    // `debug.set_owner` hands #2 to the enemy: it stays alive, but is not ours to select.
    let mut app = app_with(standard());
    select_and_group(&mut app);
    let mut records = standard();
    for record in &mut records {
        if record.sim_id == SimId(2) {
            record.owner = FOE;
        }
    }
    set_world(&mut app, records);
    assert_eq!(selected(&app), [1]);
    // A group keeps it, as recalling one takes only own units.
    assert_eq!(group(&app, 1), [1, 2]);
    group_key(&mut app, 1, GroupAction::Recall);
    assert_eq!(selected(&app), [1]);
}
