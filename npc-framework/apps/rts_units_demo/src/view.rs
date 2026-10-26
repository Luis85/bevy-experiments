//! The host's presentation: primitive visuals (capsules = Infantry, cubes =
//! Engineers, coloured by player and squad), walls, ground, RTS camera, HUD,
//! and the RTS input built on `npc_view_helpers::command_ui` (through the
//! facade's `view-helpers` + `command` features).
//!
//! Read-only on the sim: visuals are separate entities linked by
//! `VisualOf`, positions are interpolated `PrevSimPos → SimPos`, and every
//! order is a recorded `IssueOrder` submitted through the kernel.
//!
//! Input (all bindings live here, in the app):
//!
//! - `command_ui` (as shipped): left click / drag selects own units (Shift
//!   adds), Ctrl+1–9 stores a control group, 1–9 recalls it, right click
//!   orders the selection (Shift queues), selection rings and order-chain
//!   gizmos.
//! - This module: the right-click order kind (`command_ui::RightClickMode`,
//!   set every frame before the UI's input): `Move` (the context order for a
//!   unit under the cursor), with **A** held `AttackMove`, with **P** held
//!   `Patrol` (Shift+click collects points, a plain click issues them).
//!   With **R** held, `RightClickKind::Custom("rts.repair")`: a right click
//!   on (or next to) a damaged wall orders the selection's engineers to
//!   repair it (`UnitOrder::Interact { verb: "repair" }`); pressing R
//!   narrows the selection to its engineers, since only they take the order.
//!   **Z / X / C** set `command_ui::CommandFormation` for group moves (Line /
//!   Box / Wedge), **V** clears it (the squad's / sim default). **S** stops,
//!   **H** holds (`command_ui::issue_to_selection`).
//! - **F2** path gizmos, **F3** journal, **F4** perf, **Space** pause, **.**
//!   step, arrows pan, **Q / E** rotate, wheel zooms.

use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use npc_framework::prelude::{
    Commandable, Formation, FormationKind, GridBlocker, OrderQueue, PrevSimPos, SimClock,
    SimControl, SimIndex, SimPos, SimRate, SimTime, UnitOrder,
};
use npc_framework::view_helpers::command_ui::{
    CommandFormation, CommandSelection, CommandUi, CommandUiPlugin, ControlGroups, DragBox,
    NpcCommandUiSet, RightClick, RightClickKind, RightClickMode, RightClickOrders,
    issue_to_selection, select_units, unit_screen_positions,
};
use npc_framework::view_helpers::interp::interp_alpha;
use npc_framework::view_helpers::nav_gizmos::NavGizmos;
use npc_framework::view_helpers::{
    DebugPanels, NpcViewHelpersPlugin, Selectable, ViewSystems, VisualIndex, VisualOf, sim_to_world,
};

use crate::host::{HOST_FIXED_HZ, HostBody, HostSteps};
use crate::sim::{
    CELL_MM, DemoWorld, MAP_CELLS, REPAIR_VERB, RtsUnit, SQUADS, UnitKind, Wall, WallHealth,
    nearest_damaged_wall,
};

/// The custom right-click kind for repair orders (R held).
pub const REPAIR_CLICK: &str = "rts.repair";
/// How far (mm, to the wall's edge) a repair click may land from the wall.
const REPAIR_CLICK_MM: i32 = 3_000;

/// R + right click: repair the damaged wall nearest the ground point.
pub fn repair_click(world: &World, click: &RightClick) -> Option<UnitOrder> {
    let at = click
        .target
        .and_then(|t| world.get::<SimPos>(t).copied())
        .or(click.ground)?;
    let wall = nearest_damaged_wall(world, at, REPAIR_CLICK_MM)?;
    Some(UnitOrder::Interact {
        verb: REPAIR_VERB.to_owned(),
        target: wall,
    })
}

/// Wall colour: grey when whole, towards rust red as hit points drop.
fn wall_color(health: Option<&WallHealth>) -> Color {
    let whole = Color::srgb(0.55, 0.55, 0.58);
    let Some(h) = health.filter(|h| h.max > 0) else {
        return whole;
    };
    let damage = 1.0 - h.hp as f32 / h.max as f32;
    whole.mix(&Color::srgb(0.75, 0.22, 0.12), damage)
}

/// Squad colours: player 0 blues, player 1 reds.
const SQUAD_COLORS: [Color; SQUADS as usize] = [
    Color::srgb(0.20, 0.45, 0.95),
    Color::srgb(0.10, 0.80, 0.85),
    Color::srgb(0.92, 0.25, 0.20),
    Color::srgb(0.97, 0.62, 0.15),
];
/// Screenshot mode: a scripted box-select over squad 1 (`command_ui`'s
/// `select_units` + its `DragBox` marquee), so the capture shows a
/// selection box and a formation move in progress.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct DemoDrag {
    pub squad: usize,
    /// Real seconds before the drag appears.
    pub after_secs: f32,
}

/// Shared mesh and material handles.
#[derive(Resource, Clone)]
struct Palette {
    capsule: Handle<Mesh>,
    cube: Handle<Mesh>,
    squads: Vec<Handle<StandardMaterial>>,
    engineers: Vec<Handle<StandardMaterial>>,
}

/// A wall visual's own material (tinted by the wall's health).
#[derive(Component, Clone)]
struct WallMaterial(Handle<StandardMaterial>);

/// RTS camera: looks at `focus` from `distance` at `pitch`, yawed `yaw`.
#[derive(Component, Clone, Copy, Debug)]
pub struct RtsCamera {
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Default for RtsCamera {
    fn default() -> Self {
        Self {
            focus: Vec3::new(0.0, 0.0, 9.0),
            yaw: 0.0,
            pitch: 64f32.to_radians(),
            distance: 106.0,
        }
    }
}

impl RtsCamera {
    pub fn transform(&self) -> Transform {
        let offset = Vec3::new(
            self.pitch.cos() * self.yaw.sin(),
            self.pitch.sin(),
            self.pitch.cos() * self.yaw.cos(),
        ) * self.distance;
        Transform::from_translation(self.focus + offset).looking_at(self.focus, Vec3::Y)
    }
}

#[derive(Component)]
struct Hud;

/// The view (see the module docs).
pub struct RtsViewPlugin;

impl Plugin for RtsViewPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<NpcViewHelpersPlugin>() {
            app.add_plugins(NpcViewHelpersPlugin);
        }
        app.insert_resource(NavGizmos { enabled: false })
            .add_plugins(CommandUiPlugin)
            .insert_resource(GlobalAmbientLight {
                color: Color::WHITE,
                brightness: 600.0,
                ..default()
            })
            .insert_resource(ClearColor(Color::srgb(0.10, 0.12, 0.14)))
            .add_systems(Startup, setup_scene)
            .add_systems(
                Update,
                (
                    camera_input,
                    hotkeys,
                    order_keys,
                    demo_drag.run_if(resource_exists::<DemoDrag>),
                )
                    .chain()
                    .in_set(ViewSystems::Input)
                    .before(NpcCommandUiSet),
            )
            .add_systems(
                Update,
                (spawn_visuals, tint_walls)
                    .chain()
                    .in_set(ViewSystems::Sync),
            )
            .add_systems(Update, move_visuals.in_set(ViewSystems::Animate))
            .add_systems(
                Update,
                (update_hud, draw_ground).in_set(ViewSystems::Present),
            );
        // `CommandUiPlugin` initialised the resolver table above.
        app.world_mut()
            .resource_mut::<RightClickOrders>()
            .add(REPAIR_CLICK, repair_click);
    }
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let rts = RtsCamera::default();
    commands.spawn((
        Name::new("camera"),
        Camera3d::default(),
        rts.transform(),
        rts,
    ));
    commands.spawn((
        Name::new("sun"),
        DirectionalLight {
            illuminance: 9_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 0.6, -0.95, 0.0)),
    ));
    let size = (MAP_CELLS as i32 * CELL_MM) as f32 / 1000.0;
    commands.spawn((
        Name::new("ground"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(size, size))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.30, 0.36, 0.26),
            perceptual_roughness: 0.95,
            ..default()
        })),
    ));
    let mat = |m: &mut Assets<StandardMaterial>, c: Color| {
        m.add(StandardMaterial {
            base_color: c,
            perceptual_roughness: 0.6,
            ..default()
        })
    };
    let squads = SQUAD_COLORS
        .iter()
        .map(|&c| mat(&mut materials, c))
        .collect();
    let engineers = SQUAD_COLORS
        .iter()
        .map(|&c| mat(&mut materials, c.mix(&Color::srgb(1.0, 0.95, 0.4), 0.35)))
        .collect();
    let palette = Palette {
        capsule: meshes.add(Capsule3d::new(0.3, 0.8)),
        cube: meshes.add(Cuboid::new(0.75, 0.75, 0.75)),
        squads,
        engineers,
    };
    // Host-simulated supply drops, bouncing in the host's FixedUpdate.
    let drop = materials.add(StandardMaterial {
        base_color: Color::srgb(0.95, 0.85, 0.3),
        emissive: LinearRgba::rgb(0.6, 0.45, 0.05),
        ..default()
    });
    let sphere = meshes.add(Sphere::new(0.6));
    for (i, (x, z)) in [
        (-44.0, -44.0),
        (44.0, -44.0),
        (-44.0, 44.0),
        (44.0, 44.0),
        (0.0, 0.0),
    ]
    .into_iter()
    .enumerate()
    {
        let pos = Vec3::new(x, 1.0 + i as f32 * 1.5, z);
        commands.spawn((
            Name::new("supply drop"),
            HostBody {
                pos,
                vel: Vec3::ZERO,
                radius: 0.6,
            },
            Mesh3d(sphere.clone()),
            MeshMaterial3d(drop.clone()),
            Transform::from_translation(pos),
        ));
    }
    commands.insert_resource(palette);
    commands.spawn((
        Name::new("hud"),
        Hud,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::srgb(0.93, 0.95, 0.9)),
        BackgroundColor(Color::srgba(0.03, 0.04, 0.06, 0.72)),
        Node {
            position_type: PositionType::Absolute,
            top: px(8),
            left: px(8),
            padding: UiRect::all(px(8)),
            ..default()
        },
    ));
}

/// Preset control groups 1 and 2 = the local player's squads.
pub fn preset_groups(demo: Res<DemoWorld>, ui: Res<CommandUi>, mut groups: ResMut<ControlGroups>) {
    let own =
        (0..demo.members.len()).filter(|&s| crate::sim::squad_player(s as u32) == ui.player.0);
    for (slot, squad) in own.enumerate().take(9) {
        groups.groups[slot].clone_from(&demo.members[squad]);
    }
}

/// Visuals for new units and walls.
fn spawn_visuals(
    mut commands: Commands,
    palette: Option<Res<Palette>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut index: ResMut<VisualIndex>,
    units: Query<(Entity, &SimPos, &RtsUnit)>,
    walls: Query<(Entity, &SimPos, &GridBlocker, Option<&WallHealth>), With<Wall>>,
) {
    let Some(palette) = palette else { return };
    for (sim, pos, unit) in &units {
        if index.map.contains_key(&sim) {
            continue;
        }
        let squad = unit.squad as usize % SQUAD_COLORS.len();
        let (mesh, material, y) = match unit.kind {
            UnitKind::Infantry => (palette.capsule.clone(), palette.squads[squad].clone(), 0.7),
            UnitKind::Engineer => (
                palette.cube.clone(),
                palette.engineers[squad].clone(),
                0.375,
            ),
        };
        let root = commands
            .spawn((
                Name::new(unit.kind.name()),
                VisualOf(sim),
                Selectable,
                Transform::from_translation(sim_to_world(*pos)),
                Visibility::default(),
            ))
            .with_children(|c| {
                c.spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    Transform::from_xyz(0.0, y, 0.0),
                ));
            })
            .id();
        index.map.insert(sim, root);
    }
    for (sim, pos, blocker, health) in &walls {
        if index.map.contains_key(&sim) {
            continue;
        }
        let fp = blocker.0;
        let w = (2 * fp.half_x_mm + CELL_MM) as f32 / 1000.0;
        let d = (2 * fp.half_z_mm + CELL_MM) as f32 / 1000.0;
        let material = materials.add(StandardMaterial {
            base_color: wall_color(health),
            perceptual_roughness: 0.6,
            ..default()
        });
        let root = commands
            .spawn((
                Name::new("wall"),
                VisualOf(sim),
                Mesh3d(meshes.add(Cuboid::new(w, 2.0, d))),
                MeshMaterial3d(material.clone()),
                WallMaterial(material),
                Transform::from_translation(sim_to_world(*pos) + Vec3::Y * 1.0),
            ))
            .id();
        index.map.insert(sim, root);
    }
}

/// Wall visuals show their wall's health (read-only on the sim).
fn tint_walls(
    walls: Query<&WallHealth, (With<Wall>, Changed<WallHealth>)>,
    visuals: Query<(&VisualOf, &WallMaterial)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (of, mat) in &visuals {
        if let Ok(health) = walls.get(of.0)
            && let Some(mut m) = materials.get_mut(&mat.0)
        {
            m.base_color = wall_color(Some(health));
        }
    }
}

/// Visual roots follow their sim entity, interpolated between ticks.
fn move_visuals(
    time: Option<Res<SimTime>>,
    rate: Option<Res<SimRate>>,
    control: Option<Res<SimControl>>,
    sims: Query<(&SimPos, Option<&PrevSimPos>), With<RtsUnit>>,
    mut visuals: Query<(&VisualOf, &mut Transform)>,
) {
    let alpha = interp_alpha(time.as_deref(), rate.as_deref(), control.as_deref());
    for (of, mut t) in &mut visuals {
        let Ok((pos, prev)) = sims.get(of.0) else {
            continue;
        };
        let to = sim_to_world(*pos);
        let from = prev.map_or(to, |p| sim_to_world(p.0));
        let at = from.lerp(to, alpha);
        let step = to - from;
        if step.length_squared() > 1e-6 {
            t.rotation = Quat::from_rotation_y(f32::atan2(step.x, step.z));
        }
        t.translation = at;
    }
}

fn camera_input(
    time: Res<Time>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    scroll: Option<Res<AccumulatedMouseScroll>>,
    mut cameras: Query<(&mut RtsCamera, &mut Transform)>,
) {
    let dt = time.delta_secs();
    let limit = (MAP_CELLS as f32) / 2.0;
    for (mut rts, mut transform) in &mut cameras {
        if let Some(keys) = keys.as_deref() {
            let mut input = Vec2::ZERO;
            if keys.pressed(KeyCode::ArrowUp) {
                input.y -= 1.0;
            }
            if keys.pressed(KeyCode::ArrowDown) {
                input.y += 1.0;
            }
            if keys.pressed(KeyCode::ArrowLeft) {
                input.x -= 1.0;
            }
            if keys.pressed(KeyCode::ArrowRight) {
                input.x += 1.0;
            }
            if input != Vec2::ZERO {
                let world =
                    Quat::from_rotation_y(rts.yaw) * Vec3::new(input.x, 0.0, input.y).normalize();
                let speed = rts.distance * 0.8;
                rts.focus =
                    (rts.focus + world * speed * dt).clamp(Vec3::splat(-limit), Vec3::splat(limit));
                rts.focus.y = 0.0;
            }
            if keys.pressed(KeyCode::KeyQ) {
                rts.yaw -= 1.5 * dt;
            }
            if keys.pressed(KeyCode::KeyE) {
                rts.yaw += 1.5 * dt;
            }
        }
        if let Some(scroll) = scroll.as_deref() {
            let lines = match scroll.unit {
                MouseScrollUnit::Line => scroll.delta.y,
                MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
            };
            if lines != 0.0 {
                rts.distance = (rts.distance * 0.9f32.powf(lines)).clamp(8.0, 140.0);
            }
        }
        let want = rts.transform();
        if *transform != want {
            *transform = want;
        }
    }
}

/// Panels, pause/step, path gizmos, formation choice, right-click kind.
#[allow(clippy::too_many_arguments)]
fn hotkeys(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    index: Option<Res<SimIndex>>,
    units: Query<&RtsUnit>,
    mut selection: ResMut<CommandSelection>,
    mut panels: ResMut<DebugPanels>,
    mut control: ResMut<SimControl>,
    mut nav: ResMut<NavGizmos>,
    mut formation: ResMut<CommandFormation>,
    mut right_click: ResMut<RightClickMode>,
) {
    let Some(keys) = keys else { return };
    // The order kind of the next right click, while A / P is held.
    let kind = if keys.pressed(KeyCode::KeyR) {
        RightClickKind::Custom(REPAIR_CLICK)
    } else if keys.pressed(KeyCode::KeyA) {
        RightClickKind::AttackMove
    } else if keys.pressed(KeyCode::KeyP) {
        RightClickKind::Patrol
    } else {
        RightClickKind::Move
    };
    if right_click.kind != kind {
        *right_click = RightClickMode::held(kind);
    }
    // Repair mode: only engineers take repair orders, so keep just them.
    if keys.just_pressed(KeyCode::KeyR)
        && let Some(index) = index.as_deref()
    {
        let engineers: Vec<_> = selection
            .units
            .iter()
            .copied()
            .filter(|id| {
                index
                    .get(*id)
                    .and_then(|e| units.get(e).ok())
                    .is_some_and(|u| u.kind == UnitKind::Engineer)
            })
            .collect();
        if engineers != selection.units {
            selection.units = engineers;
        }
    }
    if keys.just_pressed(KeyCode::F2) {
        nav.enabled = !nav.enabled;
    }
    if keys.just_pressed(KeyCode::F3) {
        panels.journal = !panels.journal;
    }
    if keys.just_pressed(KeyCode::F4) {
        panels.perf = !panels.perf;
    }
    if keys.just_pressed(KeyCode::Space) {
        control.paused = !control.paused;
    }
    if keys.just_pressed(KeyCode::Period) && control.is_stopped() {
        control.step();
    }
    let pick = |kind, spacing_mm| Some(Formation::new(kind, spacing_mm));
    if keys.just_pressed(KeyCode::KeyZ) {
        formation.0 = pick(FormationKind::Line, 1500);
    }
    if keys.just_pressed(KeyCode::KeyX) {
        formation.0 = pick(FormationKind::Box, 1500);
    }
    if keys.just_pressed(KeyCode::KeyC) {
        formation.0 = pick(FormationKind::Wedge, 1500);
    }
    if keys.just_pressed(KeyCode::KeyV) {
        formation.0 = None;
    }
}

/// S / H → a recorded `Stop` / `Hold` for the selection (Shift queues).
fn order_keys(world: &mut World) {
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return;
    };
    let order = if keys.just_pressed(KeyCode::KeyS) {
        UnitOrder::Stop
    } else if keys.just_pressed(KeyCode::KeyH) {
        UnitOrder::Hold
    } else {
        return;
    };
    let queue = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if world
        .get_resource::<CommandUi>()
        .is_some_and(|ui| ui.enabled)
        && let Err(err) = issue_to_selection(world, order, queue)
    {
        warn!("order rejected: {err}");
    }
}

/// Screenshot mode: the scripted box-select (see [`DemoDrag`]).
fn demo_drag(world: &mut World) {
    let drag = *world.resource::<DemoDrag>();
    if world.resource::<Time<Real>>().elapsed_secs() < drag.after_secs {
        return;
    }
    let Some(members) = world
        .get_resource::<DemoWorld>()
        .and_then(|d| d.members.get(drag.squad).cloned())
    else {
        return;
    };
    let screen = unit_screen_positions(world, &members);
    if screen.is_empty() {
        return;
    }
    let lo = screen.iter().fold(Vec2::MAX, |a, (_, p)| a.min(*p)) - Vec2::splat(24.0);
    let hi = screen.iter().fold(Vec2::MIN, |a, (_, p)| a.max(*p)) + Vec2::new(24.0, 10.0);
    select_units(world, screen.iter().map(|(id, _)| *id), false);
    *world.resource_mut::<DragBox>() = DragBox {
        start: Some(lo),
        current: hi,
    };
}

/// Ground grid every 8 m and the map border.
fn draw_ground(mut gizmos: Gizmos) {
    let half = MAP_CELLS as f32 / 2.0;
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    gizmos.grid(
        Isometry3d::new(Vec3::Y * 0.02, flat),
        UVec2::splat(MAP_CELLS / 8),
        Vec2::splat(8.0),
        Color::srgba(1.0, 1.0, 1.0, 0.06),
    );
    gizmos.rect(
        Isometry3d::new(Vec3::Y * 0.03, flat),
        Vec2::splat(2.0 * half),
        Color::srgba(1.0, 1.0, 1.0, 0.35),
    );
}

#[allow(clippy::too_many_arguments)]
fn update_hud(
    time: Res<Time<Real>>,
    mut last: Local<f32>,
    clock: Option<Res<SimClock>>,
    rate: Option<Res<SimRate>>,
    control: Option<Res<SimControl>>,
    steps: Option<Res<HostSteps>>,
    fixed: Res<Time<Fixed>>,
    sel: Res<CommandSelection>,
    formation: Res<CommandFormation>,
    queues: Query<&OrderQueue, (With<RtsUnit>, With<Commandable>)>,
    walls: Query<&WallHealth, With<Wall>>,
    mut hud: Query<&mut Text, With<Hud>>,
) {
    let now = time.elapsed_secs();
    if now - *last < 0.25 && *last > 0.0 {
        return;
    }
    *last = now;
    let (mut units, mut moving) = (0u32, 0u32);
    for q in &queues {
        units += 1;
        moving += u32::from(q.has_order());
    }
    let damaged = walls.iter().filter(|h| h.is_damaged()).count();
    let tick = clock.map_or(0, |c| c.tick);
    let hz = rate.map_or(0, |r| r.hz);
    let paused = control.is_some_and(|c| c.paused);
    let formation = formation.0.map_or_else(
        || "squad default".to_owned(),
        |f| format!("{:?} {} mm", f.kind, f.spacing_mm),
    );
    let text = format!(
        "RTS units demo - npc_framework facade only\n\
         sim tick {tick} @ {hz} Hz{} | host FixedUpdate {HOST_FIXED_HZ} Hz ({:.1} ms), {} steps\n\
         units {units}, moving {moving}, idle {} | selected {} | formation: {formation} | damaged walls {damaged}\n\
         [LMB] select / box  [Shift] add / queue  [RMB] move  [A+RMB] attack-move  [P+RMB] patrol (+Shift: waypoint)\n\
         [R+RMB] repair (engineers)  [S] stop  [H] hold  [Z/X/C/V] line / box / wedge / squad  [Ctrl+1-9] store  [1-9] recall\n\
         [F2] paths  [F3] journal  [F4] perf  [Space] pause  [.] step  [arrows] pan  [Q/E] rotate  [wheel] zoom",
        if paused { " (paused)" } else { "" },
        fixed.timestep().as_secs_f64() * 1000.0,
        steps.map_or(0, |s| s.0),
        units - moving,
        sel.units.len(),
    );
    for mut t in &mut hud {
        if t.0 != text {
            t.0.clone_from(&text);
        }
    }
}
