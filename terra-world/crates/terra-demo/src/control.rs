//! Command selection and possession are presentation policy; the sandbox owns
//! the units, permissions, motion and inventory behind every interaction.
use crate::{
    camera::{CameraRig, ExplorerCamera},
    interaction::{Placement, pick_ground},
    interface::UiState,
};
use bevy::prelude::*;
use std::collections::{BTreeSet, VecDeque};
use terra_bevy::{
    LivingMotion, NetworkStatus, PoliticalWorld, ResidentChunks, SandboxIntent, SandboxQueue,
    SandboxRuntime, WorldRuntime, WorldSet,
    rules::{Command, LivingCommand},
};
use terra_core::{ActorKind, GeoPosition, LocalFrame};

pub struct ActorControlPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlSet {
    Input,
}

impl Plugin for ActorControlPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::player_input::PlayerInput>()
            .init_resource::<crate::command_input::WorldPointer>()
            .init_resource::<PlayerControl>()
            .init_resource::<ControlQueue>()
            .init_resource::<WorldViewport>()
            .add_systems(Startup, setup_selection_box)
            .add_systems(
                Update,
                (
                    synchronize_control,
                    control_shortcuts,
                    selection_input,
                    process_control_intents,
                )
                    .chain()
                    .in_set(ControlSet::Input)
                    .before(WorldSet::Stream),
            )
            .add_systems(Update, selection_overlays.after(WorldSet::Render));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlMode {
    #[default]
    Rts,
    ThirdPerson,
    FirstPerson,
}

impl ControlMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rts => "Command",
            Self::ThirdPerson => "Third person",
            Self::FirstPerson => "First person",
        }
    }
    pub fn is_possessed(self) -> bool {
        self != Self::Rts
    }
}

#[derive(Resource, Debug)]
pub struct PlayerControl {
    pub mode: ControlMode,
    pub selected: BTreeSet<u64>,
    pub possessed: Option<u64>,
    pub interact_requested: bool,
    pub last_order: Option<GeoPosition>,
    pub drag_start: Option<Vec2>,
    pub status: String,
    pub(crate) drag_end: Option<Vec2>,
    pub(crate) pending_possession: Option<u64>,
    pub(crate) pending_seconds: f32,
    pub(crate) order_seconds: f32,
    initialized_authority: Option<(u64, u64)>,
    input_authority: Option<(u64, u64)>,
}

impl Default for PlayerControl {
    fn default() -> Self {
        Self {
            mode: ControlMode::Rts,
            selected: BTreeSet::new(),
            possessed: None,
            interact_requested: false,
            last_order: None,
            drag_start: None,
            drag_end: None,
            status: "Select an actor. Right-click terrain to move owned units.".into(),
            pending_possession: None,
            pending_seconds: 0.,
            order_seconds: 0.,
            initialized_authority: None,
            input_authority: None,
        }
    }
}

impl PlayerControl {
    pub fn primary(&self) -> Option<u64> {
        self.possessed.or_else(|| self.selected.first().copied())
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ControlIntent {
    SetMode(ControlMode),
    PossessSelected,
    Release,
    SelectAll,
    CycleActor,
    FocusSelection,
    StopSelected,
    Interact,
}

#[derive(Resource, Default)]
pub struct ControlQueue(pub VecDeque<ControlIntent>);

/// Shared input region. The interface updates these insets whenever a drawer
/// opens, so a click on UI never becomes a terrain order or a map vertex.
#[derive(Resource, Clone, Copy, Debug)]
pub struct WorldViewport {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Default for WorldViewport {
    fn default() -> Self {
        Self {
            left: 14.,
            right: 314.,
            top: 76.,
            bottom: 122.,
        }
    }
}

impl WorldViewport {
    pub fn rect(&self, window: &Window, atlas: bool) -> Rect {
        let min = Vec2::new(self.left, self.top + if atlas { 100. } else { 0. });
        let max = Vec2::new(
            (window.width() - self.right).max(min.x),
            (window.height() - self.bottom - if atlas { 80. } else { 0. }).max(min.y),
        );
        Rect::from_corners(min, max)
    }
    pub fn contains(&self, window: &Window, point: Vec2, atlas: bool) -> bool {
        self.rect(window, atlas).contains(point)
    }
}

fn synchronize_control(
    time: Res<Time>,
    sandbox: Res<SandboxRuntime>,
    mut control: ResMut<PlayerControl>,
    mut motion: ResMut<LivingMotion>,
    mut intents: ResMut<ControlQueue>,
    mut ui_actions: Option<ResMut<crate::interface::UiQueue>>,
    mut placement: Option<ResMut<Placement>>,
    flow: Option<Res<crate::flow::GameFlow>>,
) {
    let life = &sandbox.state.living;
    control.selected.retain(|id| life.units.contains_key(id));
    let authority = (sandbox.player_id, sandbox.restore_revision);
    if control
        .input_authority
        .is_some_and(|previous| previous != authority)
    {
        // Retire action-only backlogs before the bounded control/UI drains.
        // Record this replacement even if its body is still loading or menus
        // defer camera adoption, so new input is not repeatedly discarded.
        intents.0.clear();
        if let Some(actions) = ui_actions.as_mut() {
            actions.0.clear();
        }
        if let Some(placement) = placement.as_mut() {
            placement.0 = None;
        }
        control.selected.clear();
        control.possessed = None;
        control.pending_possession = None;
        control.pending_seconds = 0.;
        control.mode = ControlMode::Rts;
        control.interact_requested = false;
        control.last_order = None;
        control.order_seconds = 0.;
        control.drag_start = None;
        control.drag_end = None;
        control.input_authority = Some(authority);
    }
    if !life.accounts.contains_key(&sandbox.player_id) {
        control.initialized_authority = None;
    }
    if control.initialized_authority != Some(authority)
        && !flow.as_ref().is_some_and(|f| f.blocks_input())
    {
        if let Some(account) = life.accounts.get(&sandbox.player_id) {
            let previously_initialized = control.initialized_authority.is_some();
            control.selected = BTreeSet::from([account.active_unit]);
            control.possessed = None;
            control.pending_possession = None;
            control.pending_seconds = 0.;
            control.mode = ControlMode::Rts;
            control.initialized_authority = Some(authority);
            // Initial startup and ordinary state revisions keep their input.
            control.input_authority = Some(authority);
            // Restore the authoritative body once, after loading/menus unblock
            // input. Camera mode itself is presentation state, so resume in the
            // stable chase view. Ordinary revisions and F1 never trigger this.
            if account.possessed {
                intents
                    .0
                    .push_front(ControlIntent::SetMode(ControlMode::ThirdPerson));
            } else if previously_initialized {
                intents
                    .0
                    .push_front(ControlIntent::SetMode(ControlMode::Rts));
            }
        }
    }
    if let Some(id) = control.pending_possession {
        control.pending_seconds += time.delta_secs().min(0.25);
        let accepted = life
            .accounts
            .get(&sandbox.player_id)
            .is_some_and(|a| a.possessed && a.active_unit == id);
        if accepted {
            control.pending_possession = None;
        } else if control.pending_seconds > 4. {
            control.pending_possession = None;
            control.possessed = None;
            control.mode = ControlMode::Rts;
            control.status =
                "Possession was not accepted. Select one of your actors and retry.".into();
        }
    } else if control.mode.is_possessed()
        && !life
            .accounts
            .get(&sandbox.player_id)
            .is_some_and(|a| a.possessed && Some(a.active_unit) == control.possessed)
    {
        control.mode = ControlMode::Rts;
        control.possessed = None;
        intents.0.push_back(ControlIntent::FocusSelection);
    }
    control.order_seconds = (control.order_seconds - time.delta_secs()).max(0.);
    if control.order_seconds == 0. {
        control.last_order = None;
    }
    motion.active = false;
    motion.velocity_enu_m_s = [0.; 3];
    motion.drive = None;
}

fn control_shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    input: Res<crate::player_input::PlayerInput>,
    ui: Res<UiState>,
    politics: Res<PoliticalWorld>,
    placement: Res<Placement>,
    control: Res<PlayerControl>,
    mut queue: ResMut<ControlQueue>,
    flow: Option<Res<crate::flow::GameFlow>>,
    pointer: Res<crate::command_input::WorldPointer>,
) {
    if input.world_blocked()
        || ui.editing_gps
        || politics.atlas
        || placement.is_active()
        || flow.as_ref().is_some_and(|f| f.blocks_input())
    {
        return;
    }
    if pointer.active {
        if input.menu_shift {
            queue
                .0
                .push_back(ControlIntent::SetMode(ControlMode::ThirdPerson));
        }
        if input.pad_cycle {
            queue.0.push_back(ControlIntent::CycleActor);
        }
        if input.menu_back {
            queue.0.push_back(ControlIntent::StopSelected);
        }
    }
    for (key, intent) in [
        (KeyCode::F1, ControlIntent::SetMode(ControlMode::Rts)),
        (
            KeyCode::F2,
            ControlIntent::SetMode(ControlMode::ThirdPerson),
        ),
        (
            KeyCode::F3,
            ControlIntent::SetMode(ControlMode::FirstPerson),
        ),
        (KeyCode::Escape, ControlIntent::Release),
        (KeyCode::KeyX, ControlIntent::StopSelected),
    ] {
        if keys.just_pressed(key) && !(key == KeyCode::Escape && flow.is_some()) {
            queue.0.push_back(intent);
        }
    }
    if input.just_pressed(crate::player_input::Binding::Interact) {
        queue.0.push_back(ControlIntent::Interact);
    }
    if input.release_pressed {
        queue.0.push_back(ControlIntent::Release);
    }
    if input.just_pressed(crate::player_input::Binding::Camera) && control.mode.is_possessed() {
        queue
            .0
            .push_back(ControlIntent::SetMode(match control.mode {
                ControlMode::FirstPerson => ControlMode::ThirdPerson,
                _ => ControlMode::FirstPerson,
            }));
    }
    if control.mode == ControlMode::Rts
        && keys.just_pressed(KeyCode::KeyA)
        && (keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight))
    {
        queue.0.push_back(ControlIntent::SelectAll);
    }
}

fn enqueue_living(queue: &mut SandboxQueue, action: LivingCommand) {
    queue
        .0
        .push_back(SandboxIntent::Command(Command::Living { action }));
}

fn owned_selection(control: &PlayerControl, sandbox: &SandboxRuntime) -> Vec<u64> {
    control
        .selected
        .iter()
        .copied()
        .filter(|id| {
            sandbox
                .state
                .living
                .units
                .get(id)
                .is_some_and(|u| u.owner == Some(sandbox.player_id))
        })
        .take(32)
        .collect()
}

fn actor_control_status(
    unit: &terra_bevy::rules::LivingUnit,
    mode: ControlMode,
    registered_vehicle: bool,
) -> String {
    let detail = if mode.is_possessed() {
        if registered_vehicle {
            format!("Driving · {}", mode.label())
        } else {
            mode.label().into()
        }
    } else if registered_vehicle {
        "Vehicle selected".into()
    } else {
        format!("{:?}", unit.role)
    };
    format!("{} · {detail}", unit.name)
}

fn process_control_intents(
    mut intents: ResMut<ControlQueue>,
    mut control: ResMut<PlayerControl>,
    sandbox: Res<SandboxRuntime>,
    mut queue: ResMut<SandboxQueue>,
    mut rig: ResMut<CameraRig>,
    mut politics: ResMut<PoliticalWorld>,
    mut net: Option<ResMut<NetworkStatus>>,
    flow: Option<Res<crate::flow::GameFlow>>,
) {
    if flow.as_ref().is_some_and(|f| f.blocks_input()) {
        intents.0.clear();
        return;
    }
    for _ in 0..16 {
        let Some(intent) = intents.0.pop_front() else {
            break;
        };
        let life = &sandbox.state.living;
        let account = life.accounts.get(&sandbox.player_id);
        match intent {
            ControlIntent::SetMode(ControlMode::Rts) | ControlIntent::Release => {
                if account.is_some_and(|a| a.possessed) || control.pending_possession.is_some() {
                    enqueue_living(&mut queue, LivingCommand::Release);
                }
                control.mode = ControlMode::Rts;
                control.possessed = None;
                control.pending_possession = None;
                control.drag_start = None;
                if let Some(unit) = control.primary().and_then(|id| life.units.get(&id)) {
                    // A location/atlas action can request release and set a
                    // spectator destination in the same frame. Keep that
                    // explicit destination instead of replacing it with camp.
                    let focus_body = matches!(intent, ControlIntent::SetMode(ControlMode::Rts))
                        || rig.teleport.is_none()
                            && rig.position.surface_distance_m(unit.position) < 80.
                            && (rig.position.altitude_m - unit.position.altitude_m).abs() < 70.;
                    if focus_body {
                        rig.teleport = LocalFrame::new(unit.position).to_geo([0., 0., 85.]).ok();
                        rig.altitude_offset = 65.;
                        rig.yaw = 0.;
                        rig.pitch = -0.65;
                    }
                }
                control.status =
                    "Command mode · click or drag to select · right-click to move".into();
                if let Some(net) = net.as_mut() {
                    net.desired_velocity_enu_m_s = [0.; 3];
                    net.follow_player = false;
                }
            }
            ControlIntent::SetMode(mode) => {
                let target = control
                    .possessed
                    .or_else(|| control.selected.first().copied())
                    .or_else(|| account.map(|a| a.avatar_unit));
                let Some(unit) = target.and_then(|id| life.units.get(&id)) else {
                    control.status =
                        "Wait for the settlement to load, then select an actor.".into();
                    continue;
                };
                if unit.owner != Some(sandbox.player_id) {
                    control.status =
                        "You can inspect this actor. Possession requires ownership.".into();
                    continue;
                }
                let entering_body =
                    control.mode == ControlMode::Rts || control.possessed != Some(unit.id);
                if entering_body
                    || mode != control.mode
                        && sandbox.state.transport.vehicles.contains_key(&unit.id)
                {
                    rig.yaw = -unit.heading_deg.to_radians();
                    rig.pitch = if mode == ControlMode::FirstPerson {
                        -0.04
                    } else {
                        -0.18
                    };
                }
                if !account.is_some_and(|a| a.possessed && a.active_unit == unit.id) {
                    enqueue_living(&mut queue, LivingCommand::Possess { unit_id: unit.id });
                    control.pending_possession = Some(unit.id);
                    control.pending_seconds = 0.;
                }
                control.mode = mode;
                control.selected = BTreeSet::from([unit.id]);
                control.possessed = Some(unit.id);
                rig.teleport = None;
                politics.atlas = false;
                control.status = actor_control_status(
                    unit,
                    mode,
                    sandbox.state.transport.vehicles.contains_key(&unit.id),
                );
                if let Some(net) = net.as_mut() {
                    net.follow_player = false;
                }
            }
            ControlIntent::PossessSelected => {
                intents
                    .0
                    .push_front(ControlIntent::SetMode(ControlMode::ThirdPerson));
            }
            ControlIntent::SelectAll => {
                control.selected = life
                    .units
                    .values()
                    .filter(|u| u.owner == Some(sandbox.player_id))
                    .take(32)
                    .map(|u| u.id)
                    .collect();
                if let Some(unit_id) = control.selected.first().copied() {
                    enqueue_living(&mut queue, LivingCommand::SelectActive { unit_id });
                }
                control.status = format!("{} owned units selected", control.selected.len());
            }
            ControlIntent::CycleActor => {
                let ids: Vec<_> = life
                    .units
                    .values()
                    .filter(|u| u.owner == Some(sandbox.player_id))
                    .map(|u| u.id)
                    .collect();
                if !ids.is_empty() {
                    let index = control
                        .primary()
                        .and_then(|id| ids.iter().position(|x| *x == id))
                        .map(|i| (i + 1) % ids.len())
                        .unwrap_or(0);
                    if control.mode.is_possessed() {
                        enqueue_living(&mut queue, LivingCommand::Release);
                    }
                    control.mode = ControlMode::Rts;
                    control.possessed = None;
                    control.pending_possession = None;
                    control.selected = BTreeSet::from([ids[index]]);
                    enqueue_living(
                        &mut queue,
                        LivingCommand::SelectActive {
                            unit_id: ids[index],
                        },
                    );
                    intents.0.push_front(ControlIntent::FocusSelection);
                }
            }
            ControlIntent::FocusSelection => {
                if let Some(unit) = control.primary().and_then(|id| life.units.get(&id)) {
                    if !control.mode.is_possessed() {
                        rig.teleport = LocalFrame::new(unit.position).to_geo([0., 0., 85.]).ok();
                        rig.altitude_offset = 65.;
                        rig.yaw = 0.;
                        rig.pitch = -0.65;
                    }
                    politics.atlas = false;
                    control.status = actor_control_status(
                        unit,
                        control.mode,
                        sandbox.state.transport.vehicles.contains_key(&unit.id),
                    );
                }
            }
            ControlIntent::StopSelected => {
                let units = owned_selection(&control, &sandbox);
                if units.is_empty() {
                    control.status = "Select one or more owned units first.".into();
                } else {
                    enqueue_living(&mut queue, LivingCommand::StopUnits { unit_ids: units });
                }
            }
            ControlIntent::Interact => control.interact_requested = true,
        }
    }
}

/// Screen-space hit choice remains stable when origin rebasing changes every
/// transform: each point is projected from its authoritative GPS position.
fn nearest_projected_unit(
    point: Vec2,
    candidates: impl IntoIterator<Item = (u64, Vec2, f32)>,
) -> Option<u64> {
    candidates
        .into_iter()
        .filter_map(|(id, screen, radius)| {
            let distance = point.distance_squared(screen);
            (distance <= radius * radius).then_some((id, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
        .map(|(id, _)| id)
}

fn selection_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<ExplorerCamera>>,
    viewport: Res<WorldViewport>,
    ui: Res<UiState>,
    placement: Res<Placement>,
    politics: Res<PoliticalWorld>,
    sandbox: Res<SandboxRuntime>,
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    mut control: ResMut<PlayerControl>,
    mut queue: ResMut<SandboxQueue>,
    flow: Option<Res<crate::flow::GameFlow>>,
    input: Res<crate::player_input::PlayerInput>,
    pointer: Res<crate::command_input::WorldPointer>,
) {
    if !window.focused
        || input.world_blocked()
        || control.mode != ControlMode::Rts
        || ui.editing_gps
        || politics.atlas
        || placement.is_active()
        || flow.as_ref().is_some_and(|f| f.blocks_input())
    {
        control.drag_start = None;
        control.drag_end = None;
        return;
    }
    let Some(cursor) = pointer.cursor(&window) else {
        return;
    };
    let select_pressed = if pointer.active {
        pointer.select_pressed
    } else {
        buttons.just_pressed(MouseButton::Left)
    };
    let select_held = if pointer.active {
        pointer.select_held
    } else {
        buttons.pressed(MouseButton::Left)
    };
    let select_released = if pointer.active {
        pointer.select_released
    } else {
        buttons.just_released(MouseButton::Left)
    };
    let order_pressed = if pointer.active {
        pointer.order_pressed
    } else {
        buttons.just_pressed(MouseButton::Right)
    };
    let inside = viewport.contains(&window, cursor, false);
    if select_pressed && inside {
        control.drag_start = Some(cursor);
        control.drag_end = Some(cursor);
    }
    if select_held && control.drag_start.is_some() {
        control.drag_end = Some(cursor.clamp(
            viewport.rect(&window, false).min,
            viewport.rect(&window, false).max,
        ));
    }
    if select_released {
        if let Some(start) = control.drag_start.take() {
            let previous_primary = control.primary();
            let end = control.drag_end.take().unwrap_or(cursor);
            let additive = pointer.active && pointer.additive
                || keys.pressed(KeyCode::ShiftLeft)
                || keys.pressed(KeyCode::ShiftRight);
            let mut projected = Vec::new();
            for unit in sandbox.state.living.units.values() {
                let mut local =
                    Vec3::from_array(runtime.frame.to_local(unit.position).map(|v| v as f32));
                local.y += if matches!(unit.kind, ActorKind::Human | ActorKind::Creature) {
                    1.
                } else {
                    0.8
                };
                if let Ok(screen) = camera.0.world_to_viewport(camera.1, local) {
                    if viewport.contains(&window, screen, false) {
                        projected.push((unit.id, screen, unit.owner));
                    }
                }
            }
            if !additive {
                control.selected.clear();
            }
            if start.distance(end) > 7. {
                let rect = Rect::from_corners(start, end);
                for (id, point, owner) in projected {
                    if owner == Some(sandbox.player_id)
                        && rect.contains(point)
                        && control.selected.len() < 32
                    {
                        control.selected.insert(id);
                    }
                }
            } else if let Some(id) =
                nearest_projected_unit(end, projected.into_iter().map(|(id, p, _)| (id, p, 22.)))
            {
                if additive && control.selected.contains(&id) {
                    control.selected.remove(&id);
                } else {
                    control.selected.insert(id);
                }
            }
            let owns = |id| {
                sandbox
                    .state
                    .living
                    .units
                    .get(&id)
                    .is_some_and(|unit| unit.owner == Some(sandbox.player_id))
            };
            let selected = control.selected.len();
            let owned = control.selected.iter().filter(|id| owns(**id)).count();
            control.status = if selected == 0 {
                "Click an actor to inspect it; drag a box to select your units.".into()
            } else if owned == 0 {
                format!(
                    "{selected} selected · Inspect only · Select an owned actor for control or orders."
                )
            } else if control.primary().is_some_and(owns) {
                format!(
                    "{selected} selected · {owned} owned · F2 possess · Right-click moves owned units"
                )
            } else {
                format!(
                    "{selected} selected · {owned} owned · Right-click moves owned units · Select an owned actor to possess."
                )
            };
            if control.primary() != previous_primary {
                if let Some(unit_id) = control.primary().filter(|id| {
                    sandbox
                        .state
                        .living
                        .units
                        .get(id)
                        .is_some_and(|u| u.owner == Some(sandbox.player_id))
                }) {
                    enqueue_living(&mut queue, LivingCommand::SelectActive { unit_id });
                }
            }
        }
    }
    if order_pressed
        && inside
        && !keys.pressed(KeyCode::AltLeft)
        && !keys.pressed(KeyCode::AltRight)
    {
        let units = owned_selection(&control, &sandbox);
        if units.is_empty() {
            control.status = "Select an owned actor before issuing a move order.".into();
            return;
        }
        let Some(destination) = pick_ground(camera.0, camera.1, cursor, &runtime, &resident) else {
            control.status =
                "That location has no loaded terrain. Move closer or wait for streaming.".into();
            return;
        };
        let count = units.len();
        enqueue_living(
            &mut queue,
            LivingCommand::MoveUnits {
                unit_ids: units,
                destination,
            },
        );
        control.last_order = Some(destination);
        control.order_seconds = 4.;
        control.status = format!(
            "Move order sent for {count} unit{}",
            if count == 1 { "" } else { "s" }
        );
    }
}

#[derive(Component)]
struct SelectionBox;

fn setup_selection_box(mut commands: Commands) {
    commands.spawn((
        SelectionBox,
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            border: UiRect::all(px(1.)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.25, 0.8, 0.70, 0.09)),
        BorderColor::all(Color::srgb(0.50, 0.90, 0.75)),
        GlobalZIndex(100),
        Pickable::IGNORE,
    ));
}

fn selection_overlays(
    control: Res<PlayerControl>,
    sandbox: Res<SandboxRuntime>,
    runtime: Res<WorldRuntime>,
    politics: Res<PoliticalWorld>,
    mut selection_box: Query<&mut Node, With<SelectionBox>>,
    mut gizmos: Gizmos,
) {
    if let Ok(mut node) = selection_box.single_mut() {
        if let (Some(a), Some(b)) = (control.drag_start, control.drag_end) {
            let rect = Rect::from_corners(a, b);
            node.display = Display::Flex;
            node.left = px(rect.min.x);
            node.top = px(rect.min.y);
            node.width = px(rect.width());
            node.height = px(rect.height());
        } else {
            node.display = Display::None;
        }
    }
    if politics.atlas {
        return;
    }
    for id in &control.selected {
        let Some(unit) = sandbox.state.living.units.get(id) else {
            continue;
        };
        if control.mode == ControlMode::FirstPerson && control.possessed == Some(*id) {
            continue;
        }
        let center = Vec3::from_array(runtime.frame.to_local(unit.position).map(|v| v as f32))
            + Vec3::Y * 0.12;
        let radius = match unit.kind {
            ActorKind::Vehicle | ActorKind::Ship => 2.8,
            ActorKind::Helicopter | ActorKind::Airplane | ActorKind::Balloon => 4.5,
            _ => 1.0,
        };
        let color = if unit.owner == Some(sandbox.player_id) {
            Color::srgb(0.44, 0.91, 0.73)
        } else {
            Color::srgb(0.98, 0.76, 0.35)
        };
        gizmos.circle(
            Isometry3d::new(center, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            radius,
            color,
        );
    }
    if let Some(position) = control.last_order {
        let p =
            Vec3::from_array(runtime.frame.to_local(position).map(|v| v as f32)) + Vec3::Y * 0.2;
        let color = Color::srgb(0.98, 0.78, 0.4);
        gizmos.circle(
            Isometry3d::new(p, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            2.5,
            color,
        );
        gizmos.line(p - Vec3::X * 3.5, p + Vec3::X * 3.5, color);
        gizmos.line(p - Vec3::Z * 3.5, p + Vec3::Z * 3.5, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_loss_cancels_pending_rts_drag_without_selecting_or_issuing_orders() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(terra_bevy::TerraWorldPlugin::new(terra_bevy::WorldConfig {
                chunk_radius: 0,
                terrain_resolution: 9,
                live_weather: false,
                ..default()
            }))
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<WorldViewport>()
            .init_resource::<UiState>()
            .init_resource::<Placement>()
            .init_resource::<PlayerControl>()
            .init_resource::<crate::player_input::PlayerInput>()
            .init_resource::<crate::command_input::WorldPointer>()
            .add_systems(Update, selection_input);
        app.world_mut().spawn(Window {
            focused: false,
            ..default()
        });
        app.world_mut().spawn((
            ExplorerCamera,
            Camera::default(),
            GlobalTransform::default(),
        ));
        {
            let mut control = app.world_mut().resource_mut::<PlayerControl>();
            control.selected.insert(17);
            control.drag_start = Some(Vec2::new(10., 10.));
            control.drag_end = Some(Vec2::new(500., 500.));
        }
        {
            let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
            mouse.press(MouseButton::Left);
            mouse.release(MouseButton::Left);
        }
        app.update();
        let control = app.world().resource::<PlayerControl>();
        assert_eq!(control.selected, BTreeSet::from([17]));
        assert!(control.drag_start.is_none() && control.drag_end.is_none());
        assert!(app.world().resource::<SandboxQueue>().0.is_empty());
    }

    #[test]
    fn restore_retires_native_backlogs_and_transient_orders_without_eating_new_actions() {
        use crate::interface::{Action, UiQueue};
        let origin = GeoPosition::new(48.137, 11.575, 10.).unwrap();
        let mut sandbox = SandboxRuntime::new(origin);
        sandbox.state.add_player(2, origin).unwrap();
        sandbox
            .state
            .seed_living_demo(origin, &[], &sandbox.catalog)
            .unwrap();
        for player in [1, 2] {
            let avatar = sandbox.state.living.accounts[&player].avatar_unit;
            sandbox
                .state
                .apply(
                    player,
                    Command::Living {
                        action: LivingCommand::Possess { unit_id: avatar },
                    },
                    &sandbox.catalog,
                )
                .unwrap();
        }
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(sandbox)
            .init_resource::<PlayerControl>()
            .init_resource::<LivingMotion>()
            .init_resource::<ControlQueue>()
            .init_resource::<UiQueue>()
            .init_resource::<SandboxQueue>()
            .init_resource::<Placement>()
            .insert_resource(PoliticalWorld::load(None).unwrap())
            .insert_resource(CameraRig {
                position: origin,
                yaw: 1.,
                pitch: -0.7,
                needs_grounding: false,
                teleport: None,
                altitude_offset: 3.,
                follow_distance: 5.5,
                last_mode: ControlMode::Rts,
            })
            .add_systems(
                Update,
                (synchronize_control, process_control_intents).chain(),
            );
        // Startup is not a replacement of a previously reviewed world.
        app.world_mut()
            .resource_mut::<UiQueue>()
            .0
            .push_back(Action::Buy(0));
        app.world_mut()
            .resource_mut::<ControlQueue>()
            .0
            .push_back(ControlIntent::SetMode(ControlMode::FirstPerson));
        app.update();
        assert_eq!(app.world().resource::<UiQueue>().0.len(), 1);
        assert_eq!(
            app.world().resource::<PlayerControl>().mode,
            ControlMode::FirstPerson
        );
        app.world_mut().resource_mut::<UiQueue>().0.clear();

        // Exercise a restore under the same player and a principal replacement
        // independently. Both queues exceed the normal per-frame input limit.
        for principal_replacement in [false, true] {
            app.world_mut()
                .resource_mut::<UiQueue>()
                .0
                .extend(std::iter::repeat_n(Action::Buy(0), 33));
            app.world_mut()
                .resource_mut::<ControlQueue>()
                .0
                .extend(std::iter::repeat_n(ControlIntent::Release, 33));
            {
                let mut control = app.world_mut().resource_mut::<PlayerControl>();
                control.interact_requested = true;
                control.drag_start = Some(Vec2::new(10., 10.));
                control.drag_end = Some(Vec2::new(50., 50.));
                control.last_order = Some(origin);
                control.order_seconds = 4.;
            }
            app.world_mut().resource_mut::<Placement>().0 =
                Some(crate::interaction::PlaceKind::Building("house".into()));
            {
                let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
                if principal_replacement {
                    sandbox.player_id = 2;
                } else {
                    sandbox.restore_revision += 1;
                }
            }
            app.update();
            let sandbox = app.world().resource::<SandboxRuntime>();
            let avatar = sandbox.state.living.accounts[&sandbox.player_id].avatar_unit;
            let control = app.world().resource::<PlayerControl>();
            assert_eq!(control.mode, ControlMode::ThirdPerson);
            assert_eq!(control.possessed, Some(avatar));
            assert!(!control.interact_requested);
            assert!(control.last_order.is_none());
            assert_eq!(control.order_seconds, 0.);
            assert!(control.drag_start.is_none() && control.drag_end.is_none());
            assert!(!app.world().resource::<Placement>().is_active());
            assert!(app.world().resource::<UiQueue>().0.is_empty());
            assert!(app.world().resource::<ControlQueue>().0.is_empty());
            assert!(app.world().resource::<SandboxQueue>().0.is_empty());

            // A normal state revision (or a failed restore with no authority
            // replacement) must accept freshly supplied input exactly once.
            {
                let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
                sandbox.revision += 1;
                sandbox.last_message = "Action rejected: save could not be restored".into();
            }
            app.world_mut()
                .resource_mut::<UiQueue>()
                .0
                .push_back(Action::Buy(0));
            app.world_mut()
                .resource_mut::<ControlQueue>()
                .0
                .push_back(ControlIntent::SetMode(ControlMode::FirstPerson));
            app.update();
            assert_eq!(app.world().resource::<UiQueue>().0.len(), 1);
            assert_eq!(
                app.world().resource::<PlayerControl>().mode,
                ControlMode::FirstPerson
            );
            assert!(app.world().resource::<SandboxQueue>().0.is_empty());
            app.world_mut().resource_mut::<UiQueue>().0.clear();
        }
    }

    #[test]
    fn restored_possession_is_adopted_once_and_ordinary_revisions_do_not_undo_rts() {
        let origin = GeoPosition::new(48.137, 11.575, 10.).unwrap();
        let mut sandbox = SandboxRuntime::new(origin);
        sandbox
            .state
            .seed_living_demo(origin, &[], &sandbox.catalog)
            .unwrap();
        sandbox
            .state
            .ensure_transport_demo(&sandbox.catalog)
            .unwrap();
        let vehicle = *sandbox.state.transport.vehicles.keys().next().unwrap();
        sandbox
            .state
            .apply(
                1,
                Command::Living {
                    action: LivingCommand::Possess { unit_id: vehicle },
                },
                &sandbox.catalog,
            )
            .unwrap();
        let saved = sandbox.state.clone();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(sandbox)
            .init_resource::<PlayerControl>()
            .init_resource::<LivingMotion>()
            .init_resource::<ControlQueue>()
            .init_resource::<SandboxQueue>()
            .insert_resource(PoliticalWorld::load(None).unwrap())
            .insert_resource(CameraRig {
                position: origin,
                yaw: 1.,
                pitch: -0.7,
                needs_grounding: false,
                teleport: None,
                altitude_offset: 3.,
                follow_distance: 5.5,
                last_mode: ControlMode::Rts,
            })
            .add_systems(
                Update,
                (synchronize_control, process_control_intents).chain(),
            );
        app.update();
        assert_eq!(
            app.world().resource::<PlayerControl>().mode,
            ControlMode::ThirdPerson
        );
        assert_eq!(
            app.world().resource::<PlayerControl>().possessed,
            Some(vehicle)
        );
        assert!(app.world().resource::<SandboxQueue>().0.is_empty());

        app.world_mut()
            .resource_mut::<ControlQueue>()
            .0
            .push_back(ControlIntent::SetMode(ControlMode::Rts));
        app.update();
        // A delayed authority acknowledgement must not make synchronization
        // re-possess a body after the player explicitly requested F1.
        app.world_mut().resource_mut::<SandboxRuntime>().revision += 1;
        app.update();
        assert_eq!(
            app.world().resource::<PlayerControl>().mode,
            ControlMode::Rts
        );
        let queued: Vec<_> = app
            .world_mut()
            .resource_mut::<SandboxQueue>()
            .0
            .drain(..)
            .collect();
        {
            let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
            let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
            for intent in queued {
                if let SandboxIntent::Command(command) = intent {
                    state.apply(1, command, catalog).unwrap();
                }
            }
        }
        app.update();
        assert_eq!(
            app.world().resource::<PlayerControl>().mode,
            ControlMode::Rts
        );

        {
            let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
            sandbox.state = saved;
            sandbox.revision += 1;
            sandbox.restore_revision += 1;
        }
        app.update();
        assert_eq!(
            app.world().resource::<PlayerControl>().mode,
            ControlMode::ThirdPerson
        );
        assert_eq!(
            app.world().resource::<PlayerControl>().possessed,
            Some(vehicle)
        );
        assert!(app.world().resource::<SandboxQueue>().0.is_empty());
        app.world_mut().resource_mut::<SandboxRuntime>().revision += 1;
        app.world_mut()
            .resource_mut::<ControlQueue>()
            .0
            .push_back(ControlIntent::SetMode(ControlMode::FirstPerson));
        app.update();
        app.update();
        assert_eq!(
            app.world().resource::<PlayerControl>().mode,
            ControlMode::FirstPerson
        );
        // Body handoffs can append FocusSelection after SetMode. Focusing must
        // retain the actual driving mode instead of exposing the generic Worker
        // simulation role shared by vehicle bodies.
        for mode in [ControlMode::ThirdPerson, ControlMode::FirstPerson] {
            app.world_mut()
                .resource_mut::<ControlQueue>()
                .0
                .extend([ControlIntent::SetMode(mode), ControlIntent::FocusSelection]);
            app.update();
            let control = app.world().resource::<PlayerControl>();
            assert_eq!(control.mode, mode);
            assert_eq!(control.possessed, Some(vehicle));
            assert!(control.status.contains("Driving"));
            assert!(control.status.ends_with(mode.label()));
            assert!(!control.status.contains("Worker"));
        }
    }

    #[test]
    fn possession_round_trip_keeps_identity_and_explicit_spectator_destination() {
        let origin = GeoPosition::new(48.137, 11.575, 10.).unwrap();
        let mut sandbox = SandboxRuntime::new(origin);
        sandbox
            .state
            .seed_living_demo(origin, &[], &sandbox.catalog)
            .unwrap();
        let avatar = sandbox.state.living.accounts[&1].avatar_unit;
        let worker = sandbox
            .state
            .living
            .units
            .values()
            .find(|u| u.owner == Some(1) && u.id != avatar)
            .unwrap()
            .id;
        sandbox
            .state
            .living
            .units
            .get_mut(&worker)
            .unwrap()
            .heading_deg = 280.3;
        let body_position = sandbox.state.living.units[&worker].position;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(sandbox)
            .insert_resource(PlayerControl {
                selected: BTreeSet::from([worker]),
                ..default()
            })
            .init_resource::<ControlQueue>()
            .init_resource::<SandboxQueue>()
            .insert_resource(PoliticalWorld::load(None).unwrap())
            .insert_resource(CameraRig {
                position: origin,
                yaw: 0.,
                pitch: -0.2,
                needs_grounding: false,
                teleport: None,
                altitude_offset: 3.,
                follow_distance: 5.5,
                last_mode: ControlMode::Rts,
            })
            .add_systems(Update, process_control_intents);
        app.world_mut()
            .resource_mut::<ControlQueue>()
            .0
            .push_back(ControlIntent::SetMode(ControlMode::ThirdPerson));
        app.update();
        assert_eq!(
            app.world().resource::<PlayerControl>().mode,
            ControlMode::ThirdPerson
        );
        let apply_pending = |app: &mut App| {
            let queued: Vec<_> = app
                .world_mut()
                .resource_mut::<SandboxQueue>()
                .0
                .drain(..)
                .collect();
            let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
            let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
            for intent in queued {
                if let SandboxIntent::Command(command) = intent {
                    state.apply(1, command, catalog).unwrap();
                }
            }
        };
        apply_pending(&mut app);
        assert_eq!(
            app.world()
                .resource::<SandboxRuntime>()
                .state
                .living
                .accounts[&1]
                .active_unit,
            worker
        );
        assert_eq!(
            app.world().resource::<SandboxRuntime>().state.players.len(),
            1
        );
        // F2 inherits the body's heading. Both F1 and a nearby Escape must
        // then aim the south-offset command camera back at that same body.
        for release in [
            ControlIntent::SetMode(ControlMode::Rts),
            ControlIntent::Release,
        ] {
            assert_eq!(
                app.world().resource::<CameraRig>().yaw,
                -280.3_f32.to_radians()
            );
            {
                let mut rig = app.world_mut().resource_mut::<CameraRig>();
                rig.position = body_position;
                rig.teleport = None;
            }
            app.world_mut()
                .resource_mut::<ControlQueue>()
                .0
                .push_back(release);
            app.update();
            assert_eq!(
                app.world().resource::<PlayerControl>().mode,
                ControlMode::Rts
            );
            let rig = app.world().resource::<CameraRig>();
            let offset = LocalFrame::new(body_position).to_local(rig.teleport.unwrap());
            assert!(offset[0].abs() < 0.001 && (offset[2] - 85.).abs() < 0.001);
            let toward_body = Vec3::new(
                -offset[0] as f32,
                -rig.altitude_offset as f32,
                -offset[2] as f32,
            )
            .normalize();
            let forward = Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.) * Vec3::NEG_Z;
            assert!(
                forward.dot(toward_body) > 28_f32.to_radians().cos(),
                "the selected body must remain inside the command camera's 56-degree field of view"
            );
            apply_pending(&mut app);
            assert_eq!(
                app.world().resource::<SandboxRuntime>().state.living.units[&worker].position,
                body_position
            );
            app.world_mut()
                .resource_mut::<ControlQueue>()
                .0
                .push_back(ControlIntent::SetMode(ControlMode::ThirdPerson));
            app.update();
            apply_pending(&mut app);
        }
        app.world_mut()
            .resource_mut::<ControlQueue>()
            .0
            .push_back(ControlIntent::SetMode(ControlMode::FirstPerson));
        app.update();
        assert_eq!(
            app.world().resource::<PlayerControl>().mode,
            ControlMode::FirstPerson
        );
        assert!(app.world().resource::<SandboxQueue>().0.is_empty());
        let destination = GeoPosition::new(-33.9249, 18.4241, 0.).unwrap();
        {
            let mut rig = app.world_mut().resource_mut::<CameraRig>();
            rig.teleport = Some(destination);
            rig.yaw = 1.37;
            rig.pitch = -0.42;
            rig.altitude_offset = 42.;
        }
        app.world_mut()
            .resource_mut::<ControlQueue>()
            .0
            .push_back(ControlIntent::Release);
        app.update();
        assert_eq!(
            app.world().resource::<CameraRig>().teleport,
            Some(destination)
        );
        let rig = app.world().resource::<CameraRig>();
        assert_eq!(
            (rig.yaw, rig.pitch, rig.altitude_offset),
            (1.37, -0.42, 42.)
        );
        apply_pending(&mut app);
        let sandbox = app.world().resource::<SandboxRuntime>();
        assert_eq!(sandbox.state.living.accounts[&1].active_unit, worker);
        assert!(!sandbox.state.living.accounts[&1].possessed);
        assert_eq!(sandbox.state.living.accounts[&1].avatar_unit, avatar);
    }

    #[test]
    fn hit_testing_is_nearest_bounded_and_tie_stable() {
        let hit = nearest_projected_unit(
            Vec2::new(10., 10.),
            [
                (3, Vec2::new(40., 10.), 12.),
                (2, Vec2::new(12., 10.), 12.),
                (1, Vec2::new(12., 10.), 12.),
            ],
        );
        assert_eq!(hit, Some(1));
        assert_eq!(
            nearest_projected_unit(Vec2::ZERO, [(4, Vec2::splat(100.), 10.)]),
            None
        );
    }

    #[test]
    fn viewport_excludes_drawers_toolbar_and_caption() {
        let window = Window {
            resolution: (1100, 700).into(),
            ..default()
        };
        let viewport = WorldViewport::default();
        assert!(viewport.contains(&window, Vec2::new(400., 300.), false));
        assert!(!viewport.contains(&window, Vec2::new(900., 300.), false));
        assert!(!viewport.contains(&window, Vec2::new(400., 150.), true));
        assert!(!viewport.contains(&window, Vec2::new(400., 520.), true));
        assert_eq!(viewport.rect(&window, false).min, Vec2::new(14., 76.));
    }
}
