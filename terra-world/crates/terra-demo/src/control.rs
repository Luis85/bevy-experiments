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
        app.init_resource::<PlayerControl>()
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
    initialized_player: Option<u64>,
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
            initialized_player: None,
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
    RestSelected,
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
) {
    let life = &sandbox.state.living;
    control.selected.retain(|id| life.units.contains_key(id));
    if control.initialized_player != Some(sandbox.player_id)
        && let Some(account) = life.accounts.get(&sandbox.player_id)
    {
        control.selected = BTreeSet::from([account.active_unit]);
        control.initialized_player = Some(sandbox.player_id);
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
}

fn control_shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    ui: Res<UiState>,
    politics: Res<PoliticalWorld>,
    placement: Res<Placement>,
    control: Res<PlayerControl>,
    mut queue: ResMut<ControlQueue>,
) {
    if ui.editing_gps || politics.atlas || placement.is_active() {
        return;
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
        (KeyCode::KeyE, ControlIntent::Interact),
        (KeyCode::KeyX, ControlIntent::StopSelected),
    ] {
        if keys.just_pressed(key) {
            queue.0.push_back(intent);
        }
    }
    if keys.just_pressed(KeyCode::KeyC) && control.mode.is_possessed() {
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

fn process_control_intents(
    mut intents: ResMut<ControlQueue>,
    mut control: ResMut<PlayerControl>,
    sandbox: Res<SandboxRuntime>,
    mut queue: ResMut<SandboxQueue>,
    mut rig: ResMut<CameraRig>,
    mut politics: ResMut<PoliticalWorld>,
    mut net: Option<ResMut<NetworkStatus>>,
) {
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
                if !account.is_some_and(|a| a.possessed && a.active_unit == unit.id) {
                    enqueue_living(&mut queue, LivingCommand::Possess { unit_id: unit.id });
                    control.pending_possession = Some(unit.id);
                    control.pending_seconds = 0.;
                    rig.yaw = -unit.heading_deg.to_radians();
                    rig.pitch = if mode == ControlMode::FirstPerson {
                        -0.08
                    } else {
                        -0.18
                    };
                }
                control.mode = mode;
                control.selected = BTreeSet::from([unit.id]);
                control.possessed = Some(unit.id);
                rig.teleport = None;
                politics.atlas = false;
                control.status = format!(
                    "{} · WASD move · Shift sprint · right-drag look · E interact",
                    unit.name
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
                    control.status = format!("{} · {:?}", unit.name, unit.role);
                }
            }
            ControlIntent::StopSelected | ControlIntent::RestSelected => {
                let units = owned_selection(&control, &sandbox);
                if units.is_empty() {
                    control.status = "Select one or more owned units first.".into();
                } else {
                    enqueue_living(
                        &mut queue,
                        match intent {
                            ControlIntent::RestSelected => LivingCommand::Rest { unit_ids: units },
                            _ => LivingCommand::StopUnits { unit_ids: units },
                        },
                    );
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

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each ECS system parameter"
)]
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
) {
    if control.mode != ControlMode::Rts || ui.editing_gps || politics.atlas || placement.is_active()
    {
        control.drag_start = None;
        control.drag_end = None;
        return;
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let inside = viewport.contains(&window, cursor, false);
    if buttons.just_pressed(MouseButton::Left) && inside {
        control.drag_start = Some(cursor);
        control.drag_end = Some(cursor);
    }
    if buttons.pressed(MouseButton::Left) && control.drag_start.is_some() {
        control.drag_end = Some(cursor.clamp(
            viewport.rect(&window, false).min,
            viewport.rect(&window, false).max,
        ));
    }
    if buttons.just_released(MouseButton::Left)
        && let Some(start) = control.drag_start.take()
    {
        let previous_primary = control.primary();
        let end = control.drag_end.take().unwrap_or(cursor);
        let additive = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        let mut projected = Vec::new();
        for unit in sandbox.state.living.units.values() {
            let mut local =
                Vec3::from_array(runtime.frame.to_local(unit.position).map(|v| v as f32));
            local.y += if matches!(unit.kind, ActorKind::Human | ActorKind::Creature) {
                1.
            } else {
                0.8
            };
            if let Ok(screen) = camera.0.world_to_viewport(camera.1, local)
                && viewport.contains(&window, screen, false)
            {
                projected.push((unit.id, screen, unit.owner));
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
        control.status = if control.selected.is_empty() {
            "Click an actor to inspect it; drag a box to select your units.".into()
        } else {
            format!(
                "{} selected · F2 possess · right-click terrain to move owned units",
                control.selected.len()
            )
        };
        if control.primary() != previous_primary
            && let Some(unit_id) = control.primary().filter(|id| {
                sandbox
                    .state
                    .living
                    .units
                    .get(id)
                    .is_some_and(|u| u.owner == Some(sandbox.player_id))
            })
        {
            enqueue_living(&mut queue, LivingCommand::SelectActive { unit_id });
        }
    }
    if buttons.just_pressed(MouseButton::Right)
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
        app.world_mut().resource_mut::<CameraRig>().teleport = Some(destination);
        app.world_mut()
            .resource_mut::<ControlQueue>()
            .0
            .push_back(ControlIntent::Release);
        app.update();
        assert_eq!(
            app.world().resource::<CameraRig>().teleport,
            Some(destination)
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
