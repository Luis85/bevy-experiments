//! Unit selection: click, box, Shift-add, double-click for the same type on screen, control
//! groups and selection rings (spec §10).
//!
//! Input is turned into [`SelectInput`] messages, and one system applies them to [`Selection`]
//! and [`ControlGroups`]. Only units owned by the [`LocalPlayer`] can be selected. The selection
//! holds `SimId`s, never entities, and is pruned against the newest snapshot, so it survives a
//! unit's visual being rebuilt. Picking and `Camera::world_to_viewport` need a window and a GPU,
//! so their logic sits in the pure functions [`box_select`] and [`classify_release`] and in
//! [`apply_input`]; the wiring is covered by the demo checklist.
use std::collections::BTreeSet;
use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::gizmos::config::GizmoConfigStore;
use bevy::picking::events::PointerClick;
use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::input::EguiWantsInput;
use fl_content::{Placeholder, VisualDef};
use fl_protocol::{PlayerId, SimId};
use fl_view::SimSnapshot;

use crate::camera::RtsCamera;
use crate::debug::{ConsoleState, SpawnPanel};
use crate::link::SnapshotBuffer;
use crate::view::{ClientContent, LocalPlayer, SimViewSystems, UnitView, ViewIndex};

/// The longest gap between two clicks on one unit that still counts as a double-click.
pub const DOUBLE_CLICK: Duration = Duration::from_millis(350);
/// A press and release closer than this, in pixels, is a click; further apart is a box.
pub const DRAG_THRESHOLD_PX: f32 = 6.0;

/// Metres added to half a unit's larger side for its selection ring.
const RING_MARGIN: f32 = 0.5;
/// Ring radius for a unit whose type has no [`VisualDef`], matching the placeholder fallback box.
const FALLBACK_RING: f32 = 2.5 + RING_MARGIN;
/// Height of the rings above the ground, so they do not z-fight with it.
const RING_LIFT: f32 = 0.05;

/// The selected units, all owned by the local player.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct Selection(pub BTreeSet<SimId>);

/// The ten control groups, by digit key (0 to 9).
#[derive(Resource, Default, Debug)]
pub struct ControlGroups(pub [BTreeSet<SimId>; 10]);

/// The unit under the last click of each button, set by the picking observer on unit visuals in
/// `PreUpdate` and cleared in `Last`.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct ClickedUnit {
    pub primary: Option<SimId>,
    pub secondary: Option<SimId>,
}

/// The viewport positions of the local player's units, filled lazily: only in frames with a box
/// release or a click on a unit.
#[derive(Resource, Default, Debug)]
pub struct OnScreen(pub Vec<(SimId, Vec2)>);

/// What a control-group key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupAction {
    /// Ctrl+digit: the group becomes the selection (an empty selection empties the group).
    Assign,
    /// Digit: the group becomes the selection.
    Recall,
    /// Shift+digit: the group joins the selection.
    Add,
    /// Alt+digit: recall, and move the camera to the group.
    Center,
}

/// One selection gesture, as the input systems report it.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub enum SelectInput {
    /// A click on a unit, or on the ground (`sim_id: None`), at `at` on the real-time clock.
    Click {
        sim_id: Option<SimId>,
        shift: bool,
        at: Duration,
    },
    /// A drag released over the viewport rectangle `min..=max`.
    Box { min: Vec2, max: Vec2, shift: bool },
    /// A control-group key.
    Group { slot: u8, action: GroupAction },
}

/// How a mouse release ends a gesture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReleaseKind {
    Click,
    Box { min: Vec2, max: Vec2 },
}

/// The ids of `points` inside the box with corners `min` and `max` (either way round), edges
/// included.
pub fn box_select(
    min: Vec2,
    max: Vec2,
    points: impl IntoIterator<Item = (SimId, Vec2)>,
) -> BTreeSet<SimId> {
    let (lo, hi) = (min.min(max), min.max(max));
    points
        .into_iter()
        .filter(|&(_, p)| p.cmpge(lo).all() && p.cmple(hi).all())
        .map(|(id, _)| id)
        .collect()
}

/// A release less than [`DRAG_THRESHOLD_PX`] from the press is a click, otherwise a box.
#[must_use]
pub fn classify_release(press: Vec2, release: Vec2) -> ReleaseKind {
    if press.distance(release) < DRAG_THRESHOLD_PX {
        ReleaseKind::Click
    } else {
        ReleaseKind::Box {
            min: press.min(release),
            max: press.max(release),
        }
    }
}

/// The radius of the selection ring of a unit with `placeholder` at `scale`: half its larger
/// side plus [`RING_MARGIN`].
#[must_use]
pub fn ring_radius(placeholder: &Placeholder, scale: f32) -> f32 {
    let side = match *placeholder {
        Placeholder::Box { size: (w, _, l) } => w.max(l),
        Placeholder::Tank {
            hull: (w, _, l), ..
        } => w.max(l),
        Placeholder::Infantry { radius, .. } => 2.0 * radius,
        Placeholder::Truck { cab, bed } => cab.0.max(bed.0).max(cab.2 + bed.2),
    };
    side * scale * 0.5 + RING_MARGIN
}

/// The systems of [`SelectionPlugin`], in `Update`, after [`SimViewSystems`].
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectionSystems;

/// The system that fills [`OnScreen`]; tests with no camera order their own filler after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct FillOnScreen;

/// The system that applies [`SelectInput`] messages.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApplySelectInput;

/// Selection, control groups and rings.
pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        // Only an app with a renderer has meshes to pick; the game adds the plugin itself.
        if app.world().contains_resource::<Assets<Mesh>>()
            && !app.is_plugin_added::<MeshPickingPlugin>()
        {
            app.add_plugins(MeshPickingPlugin);
        }
        app.init_resource::<Selection>()
            .init_resource::<ControlGroups>()
            .init_resource::<ClickedUnit>()
            .init_resource::<OnScreen>()
            .init_resource::<PressState>()
            .init_resource::<LastClick>()
            .add_message::<SelectInput>()
            .add_systems(
                Update,
                (
                    attach_click_observers,
                    prune_selection,
                    mouse_input.run_if(resource_exists::<ButtonInput<MouseButton>>),
                    key_input.run_if(resource_exists::<ButtonInput<KeyCode>>),
                    fill_on_screen.in_set(FillOnScreen),
                    apply_select_input.in_set(ApplySelectInput),
                    draw_rings.run_if(resource_exists::<GizmoConfigStore>),
                )
                    .chain()
                    .after(SimViewSystems)
                    .in_set(SelectionSystems),
            )
            .add_systems(Last, clear_clicked);
    }
}

/// Where the left button went down, while it is down.
#[derive(Resource, Default)]
pub(crate) struct PressState(Option<Vec2>);

/// The last click on an own unit, for double-click detection.
#[derive(Resource, Default)]
struct LastClick(Option<(SimId, Duration)>);

/// Whether other input owns the keyboard or the pointer right now.
#[derive(SystemParam)]
pub(crate) struct InputGate<'w> {
    console: Option<Res<'w, ConsoleState>>,
    panel: Option<Res<'w, SpawnPanel>>,
    egui: Option<Res<'w, EguiWantsInput>>,
}

impl InputGate<'_> {
    pub(crate) fn keyboard_blocked(&self) -> bool {
        self.console.as_ref().is_some_and(|c| c.open)
            || self
                .egui
                .as_ref()
                .is_some_and(|e| e.wants_any_keyboard_input())
    }

    pub(crate) fn pointer_blocked(&self) -> bool {
        self.panel.as_ref().is_some_and(|p| p.placing)
            || self
                .egui
                .as_ref()
                .is_some_and(|e| e.wants_any_pointer_input())
    }
}

/// Gives every new unit visual the click observer. The picking events propagate from the child
/// meshes up to the visual root, where the observer sits.
fn attach_click_observers(mut commands: Commands, new: Query<Entity, Added<UnitView>>) {
    for entity in &new {
        commands.entity(entity).observe(on_unit_click);
    }
}

fn on_unit_click(
    mut click: On<PointerClick>,
    views: Query<&UnitView>,
    mut clicked: ResMut<ClickedUnit>,
) {
    click.propagate(false);
    let Ok(view) = views.get(click.entity) else {
        return;
    };
    match click.button {
        PointerButton::Primary => clicked.primary = Some(view.sim_id),
        PointerButton::Secondary => clicked.secondary = Some(view.sim_id),
        PointerButton::Middle => {}
    }
}

fn clear_clicked(mut clicked: ResMut<ClickedUnit>) {
    if clicked.primary.is_some() || clicked.secondary.is_some() {
        *clicked = ClickedUnit::default();
    }
}

/// Whether `id` is in `snapshot` and owned by `player`.
fn is_own(snapshot: &SimSnapshot, player: PlayerId, id: SimId) -> bool {
    snapshot.get(id).is_some_and(|r| r.owner == player)
}

/// Drops units that left the snapshot from the selection and the groups, drops units that
/// are no longer the local player's from the selection (a group keeps them: recalling one takes
/// only own units), and clears the selection when the local player changes.
fn prune_selection(
    buffer: Res<SnapshotBuffer>,
    player: Option<Res<LocalPlayer>>,
    mut selection: ResMut<Selection>,
    mut groups: ResMut<ControlGroups>,
    mut last_player: Local<Option<PlayerId>>,
) {
    let now = player.as_deref().map(|p| p.0);
    if *last_player != now {
        let switched = last_player.is_some();
        *last_player = now;
        if switched && !selection.0.is_empty() {
            selection.0.clear();
        }
    }
    if !buffer.is_changed() {
        return;
    }
    let Some(snapshot) = buffer.curr.as_deref() else {
        return;
    };
    let alive = |id: &SimId| snapshot.get(*id).is_some();
    if let Some(player) = now {
        let own = |id: &SimId| is_own(snapshot, player, *id);
        if !selection.0.iter().all(own) {
            selection.0.retain(own);
        }
    } else if !selection.0.iter().all(alive) {
        selection.0.retain(alive);
    }
    for group in &mut groups.0 {
        if !group.iter().all(alive) {
            group.retain(alive);
        }
    }
}

/// Records presses and turns a left release into a click or a box.
#[expect(
    clippy::too_many_arguments,
    reason = "a Bevy system; each parameter is one resource or query"
)]
pub(crate) fn mouse_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    time: Res<Time<Real>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    gate: InputGate,
    clicked: Res<ClickedUnit>,
    mut press: ResMut<PressState>,
    mut out: MessageWriter<SelectInput>,
) {
    let cursor = window.and_then(|w| w.cursor_position());
    if mouse.just_pressed(MouseButton::Left) {
        press.0 = if gate.pointer_blocked() { None } else { cursor };
    }
    if !mouse.just_released(MouseButton::Left) {
        return;
    }
    let Some(from) = press.0.take() else {
        return;
    };
    if gate.pointer_blocked() {
        return;
    }
    let shift = keys
        .as_deref()
        .is_some_and(|k| k.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]));
    match classify_release(from, cursor.unwrap_or(from)) {
        ReleaseKind::Click => {
            out.write(SelectInput::Click {
                sim_id: clicked.primary,
                shift,
                at: time.elapsed(),
            });
        }
        ReleaseKind::Box { min, max } => {
            out.write(SelectInput::Box { min, max, shift });
        }
    }
}

/// The digit keys, by control group slot.
const DIGITS: [KeyCode; 10] = [
    KeyCode::Digit0,
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

/// Turns digit presses into control-group messages: Ctrl assigns, Alt centers, Shift adds.
fn key_input(
    keys: Res<ButtonInput<KeyCode>>,
    gate: InputGate,
    mut out: MessageWriter<SelectInput>,
) {
    if gate.keyboard_blocked() {
        return;
    }
    let held = |pair: [KeyCode; 2]| keys.any_pressed(pair);
    let action = if held([KeyCode::ControlLeft, KeyCode::ControlRight]) {
        GroupAction::Assign
    } else if held([KeyCode::AltLeft, KeyCode::AltRight]) {
        GroupAction::Center
    } else if held([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
        GroupAction::Add
    } else {
        GroupAction::Recall
    };
    for (slot, key) in (0u8..).zip(DIGITS) {
        if keys.just_pressed(key) {
            out.write(SelectInput::Group { slot, action });
        }
    }
}

/// Projects the local player's units to the viewport, in frames where a box or a click on a unit
/// needs them. Without an [`RtsCamera`] it is left empty.
fn fill_on_screen(
    mut inputs: MessageReader<SelectInput>,
    buffer: Res<SnapshotBuffer>,
    player: Option<Res<LocalPlayer>>,
    index: Res<ViewIndex>,
    transforms: Query<&Transform, With<UnitView>>,
    cameras: Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
    mut on_screen: ResMut<OnScreen>,
) {
    let needed = inputs.read().any(|input| {
        matches!(
            input,
            SelectInput::Box { .. }
                | SelectInput::Click {
                    sim_id: Some(_),
                    ..
                }
        )
    });
    if !needed {
        return;
    }
    // Without a camera or a snapshot nothing is on screen; never leave stale positions.
    on_screen.0.clear();
    let (Some(player), Some(snapshot)) = (player, buffer.curr.as_deref()) else {
        return;
    };
    let Some((camera, camera_transform)) = cameras.iter().find(|(camera, _)| camera.is_active)
    else {
        return;
    };
    for record in snapshot.entities.iter().filter(|r| r.owner == player.0) {
        let Some(&entity) = index.0.get(&record.sim_id) else {
            continue;
        };
        let Ok(transform) = transforms.get(entity) else {
            continue;
        };
        if let Ok(point) = camera.world_to_viewport(camera_transform, transform.translation) {
            on_screen.0.push((record.sim_id, point));
        }
    }
}

/// What [`apply_input`] reads besides the selection state.
struct Context<'a> {
    snapshot: &'a SimSnapshot,
    player: PlayerId,
    on_screen: &'a [(SimId, Vec2)],
}

impl Context<'_> {
    fn own_on_screen(&self) -> impl Iterator<Item = (SimId, Vec2)> {
        self.on_screen
            .iter()
            .copied()
            .filter(|&(id, _)| is_own(self.snapshot, self.player, id))
    }

    /// The members of `group` still in the snapshot and owned by the local player.
    fn live(&self, group: &BTreeSet<SimId>) -> BTreeSet<SimId> {
        group
            .iter()
            .copied()
            .filter(|&id| is_own(self.snapshot, self.player, id))
            .collect()
    }
}

/// Applies one gesture. Returns the group to center the camera on, for [`GroupAction::Center`].
fn apply_input(
    input: SelectInput,
    cx: &Context,
    selection: &mut BTreeSet<SimId>,
    groups: &mut [BTreeSet<SimId>; 10],
    last_click: &mut Option<(SimId, Duration)>,
) -> Option<BTreeSet<SimId>> {
    match input {
        SelectInput::Click { sim_id, shift, at } => {
            let own = sim_id.filter(|&id| is_own(cx.snapshot, cx.player, id));
            let Some(id) = own else {
                *last_click = None;
                if !shift {
                    selection.clear();
                }
                return None;
            };
            let double = matches!(
                *last_click,
                Some((prev, t)) if prev == id && at.saturating_sub(t) <= DOUBLE_CLICK
            );
            if double {
                *last_click = None;
                let unit = cx.snapshot.get(id).map(|r| r.unit);
                if !shift {
                    selection.clear();
                }
                selection.insert(id);
                selection.extend(
                    cx.own_on_screen()
                        .filter(|&(other, _)| cx.snapshot.get(other).map(|r| r.unit) == unit)
                        .map(|(other, _)| other),
                );
            } else {
                *last_click = Some((id, at));
                if !shift {
                    selection.clear();
                    selection.insert(id);
                } else if !selection.remove(&id) {
                    selection.insert(id);
                }
            }
        }
        SelectInput::Box { min, max, shift } => {
            let inside = box_select(min, max, cx.own_on_screen());
            if shift {
                selection.extend(inside);
            } else {
                *selection = inside;
            }
        }
        SelectInput::Group { slot, action } => {
            let group = groups.get_mut(usize::from(slot))?;
            match action {
                GroupAction::Assign => group.clone_from(selection),
                GroupAction::Recall | GroupAction::Add | GroupAction::Center => {
                    let live = cx.live(group);
                    if live.is_empty() {
                        return None;
                    }
                    if action == GroupAction::Add {
                        selection.extend(live.iter().copied());
                    } else {
                        selection.clone_from(&live);
                    }
                    if action == GroupAction::Center {
                        return Some(live);
                    }
                }
            }
        }
    }
    None
}

#[expect(
    clippy::too_many_arguments,
    reason = "a Bevy system; each parameter is one resource or query"
)]
fn apply_select_input(
    mut inputs: MessageReader<SelectInput>,
    buffer: Res<SnapshotBuffer>,
    player: Option<Res<LocalPlayer>>,
    on_screen: Res<OnScreen>,
    index: Res<ViewIndex>,
    transforms: Query<&Transform, With<UnitView>>,
    mut selection: ResMut<Selection>,
    mut groups: ResMut<ControlGroups>,
    mut last_click: ResMut<LastClick>,
    mut rigs: Query<&mut RtsCamera>,
) {
    let (Some(player), Some(snapshot)) = (player, buffer.curr.as_deref()) else {
        inputs.clear();
        return;
    };
    let cx = Context {
        snapshot,
        player: player.0,
        on_screen: &on_screen.0,
    };
    for &input in inputs.read() {
        let mut next = selection.0.clone();
        let center = apply_input(input, &cx, &mut next, &mut groups.0, &mut last_click.0);
        if next != selection.0 {
            selection.0 = next;
        }
        let Some(members) = center else {
            continue;
        };
        let spots: Vec<Vec3> = members
            .iter()
            .filter_map(|id| transforms.get(*index.0.get(id)?).ok())
            .map(|t| t.translation)
            .collect();
        if spots.is_empty() {
            continue;
        }
        #[expect(clippy::cast_precision_loss, reason = "a unit count is far below 2^24")]
        let centroid = spots.iter().sum::<Vec3>() / spots.len() as f32;
        for mut rig in &mut rigs {
            rig.focus = Vec3::new(centroid.x, rig.focus.y, centroid.z);
        }
    }
}

/// Draws a ring on the ground around every selected unit's visual.
fn draw_rings(
    mut gizmos: Gizmos,
    selection: Res<Selection>,
    index: Res<ViewIndex>,
    views: Query<(&UnitView, &Transform)>,
    content: Option<Res<ClientContent>>,
) {
    let flat = Quat::from_rotation_x(FRAC_PI_2);
    for id in &selection.0 {
        let Some(&entity) = index.0.get(id) else {
            continue;
        };
        let Ok((view, transform)) = views.get(entity) else {
            continue;
        };
        let radius = content
            .as_deref()
            .and_then(|content| {
                let db = &*content.0;
                let visual = db.kind::<VisualDef>().get(&db.unit(view.unit)?.id)?;
                Some(ring_radius(&visual.placeholder, visual.scale))
            })
            .unwrap_or(FALLBACK_RING);
        let center = transform.translation + Vec3::Y * RING_LIFT;
        gizmos.circle(
            Isometry3d::new(center, flat),
            radius,
            Color::srgb(0.2, 1.0, 0.2),
        );
    }
}

/// Replaces `world`'s selection with those of `ids` that are the local player's units in
/// `snapshot`; returns the new selection, or `None` without a selection or a local player.
/// Used by the remote `fl.select` method.
pub fn select_own(
    world: &mut World,
    snapshot: &SimSnapshot,
    ids: impl IntoIterator<Item = SimId>,
) -> Option<Vec<SimId>> {
    let player = world.get_resource::<LocalPlayer>()?.0;
    let mut selection = world.get_resource_mut::<Selection>()?;
    let chosen: BTreeSet<SimId> = ids
        .into_iter()
        .filter(|&id| is_own(snapshot, player, id))
        .collect();
    if selection.0 != chosen {
        selection.0.clone_from(&chosen);
    }
    Some(chosen.into_iter().collect())
}
