//! Visual entities for the testbed's sim entities. Each sim entity with a
//! `Prop`, an `Item` or a `TestbedUnit` gets its own visual root carrying
//! `VisualOf(sim)`; nothing is ever inserted on the sim entity itself.
//!
//! Scale (`docs/_notes/testbed-assets-measured.md`): every model is brought
//! to KayKit 1:1 (×KK), then scaled by the view factor [`V`] = 0.75 (a
//! Knight is 1.74 m, a 4-unit Dungeon wall piece 3 m).
//!
//! **Doors vs. the Dungeon doorway.** The sim's doors are one 1 m cell in a
//! 1 m-thick wall, but the Dungeon `wall_doorway` piece is 4 units wide
//! (3 m at `V`) with a 2-unit (1.5 m) leaf. The view squeezes the doorway
//! piece along the wall to [`DOOR_SX`] = 0.5 (a 2 m piece whose opening and
//! leaf are exactly the 1 m door cell; its solid ends overlap half of each
//! neighbouring wall cell) and keeps the height at `V` (a 2.06 m opening).
//! The leaf node `wall_doorway_door` is re-parented onto a hinge pivot that
//! carries the non-uniform scale itself, so rotating it never shears.
//! Wall runs are tiled with `wall` pieces stretched to the run's length.
//!
//! Fog of war ([`crate::fog`]): every visual root carries the look decided
//! for it (`Fogged`); trees, doors and items lying on the ground follow a
//! last-seen look, and each wall piece is fogged by its own cells.

use std::f32::consts::PI;

use bevy::{prelude::*, world_serialization::WorldInstanceReady};
use npc_framework::interaction::items::{MAIN_HAND, OFF_HAND};
use npc_framework::perception::SeenState;
use npc_framework::prelude::{
    CarriedBy, Door, GridBlocker, Item, MountedIn, Owner, PrevSimPos, SimClock, SimControl, SimId,
    SimIndex, SimPos, SimRate, SimTime, ToolTarget, Vehicle,
};
use npc_framework::view_helpers::interp::interp_alpha;
use npc_framework::view_helpers::{Selectable, ViewSystems, VisualIndex, VisualOf, sim_to_world};
use testbed_content::content::{
    AXE, BOOK, BREAD, HAMMER, INGOT, LOG, ORE, PICKAXE, PLANK, SCROLL, STORE_KEY,
};
use testbed_content::{Prop, PropKind, TestbedUnit};

use crate::assets::{ModelKind, Models, ViewAssets};
use crate::fog::{FogArea, FogDim, FogSet, Fogged, Look};

/// The view factor: KayKit 1:1 model units → metres.
pub const V: f32 = 0.75;
/// Hex-kit models are ¼ KayKit scale.
const HEX: f32 = 4.0 * V;
/// Kenney Survival models (workbench, anvil, campfire, tools).
const SURVIVAL: f32 = 4.5 * V;
const KENNEY_TOOL: f32 = 4.7;
const CART_SCALE: f32 = 3.0 * V;
const RAM_SCALE: f32 = 2.5 * V;
/// Length of a Dungeon wall piece in model units.
const WALL_PIECE: f32 = 4.0;
/// Wall pieces: 3 m high, 1 m thick (the `GridBlocker` cell).
const WALL_SCALE_Y: f32 = V;
const WALL_SCALE_Z: f32 = 1.0;
/// Doorway along the wall (see the module docs).
pub const DOOR_SX: f32 = 0.5;
/// Doorway height / depth: a hair over the walls' so faces never coincide.
const DOOR_SY: f32 = V * 1.01;
const DOOR_SZ: f32 = 1.08;
/// The leaf's own scale (it is re-parented off the squeezed frame).
const LEAF_SCALE: Vec3 = Vec3::new(DOOR_SX, DOOR_SY, V);
/// `wall_doorway_door`'s node x (model units): the hinge line.
const DOOR_HINGE_X: f32 = -0.820_098_9;
/// Negative: doors swing away from the yard side (local -z) into the rooms
/// and, for the gate, into the compound.
/// The hinge sits near the room-side face of the 1 m wall (local +z), so the
/// open leaf lies in the room instead of inside the wall's thickness.
const DOOR_HINGE_Z: f32 = 0.33;
const DOOR_OPEN: f32 = -95.0 * PI / 180.0;
/// Door swing speed, radians per second.
const DOOR_RATE: f32 = 2.4;
/// Yaw smoothing rate (1/s).
const TURN_RATE: f32 = 10.0;

/// A static prop (walls, workstations, the chest).
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct StaticVisual;

/// A tree: its two looks, swapped when its `ToolTarget` is used up.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct TreeVisual {
    tree: Entity,
    stump: Entity,
    depleted: Option<bool>,
}

/// The ore rock: shrinks as its uses run out.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct OreVisual {
    model: Entity,
    shown: Option<Option<u32>>,
}

/// A door: its current leaf angle and the hinge pivot (once the model is in).
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct DoorVisual {
    pub angle: f32,
    pivot: Option<Entity>,
}

/// On the doorway frame model, for its ready observer.
#[derive(Component, Clone, Copy, Debug)]
struct DoorFrame {
    root: Entity,
}

/// An item: where its model is and what holds it.
#[derive(Component, Clone, Debug)]
pub(crate) struct ItemVisual {
    model: Entity,
    look: ItemLook,
    /// The hand bone it is attached to.
    attached: Option<Entity>,
}

/// Interpolation and facing state of a moving visual.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Motion {
    /// Current (smoothed) yaw, radians; model +Z faces this way.
    pub yaw: f32,
    from: Vec3,
    to: Vec3,
    tick: Option<u64>,
    /// Whether the sim entity moved during the latest tick.
    pub moving: bool,
}

impl Motion {
    fn at(p: Vec3) -> Self {
        Self {
            from: p,
            to: p,
            ..default()
        }
    }

    /// The interpolated position for this frame (see the village view: at
    /// 1× exactly `PrevSimPos → SimPos`, continuous at higher speeds).
    fn step(&mut self, tick: Option<u64>, prev: Vec3, now: Vec3, alpha: f32) -> Vec3 {
        match (tick, self.tick) {
            (Some(t), Some(seen)) if t == seen => {}
            (Some(t), Some(seen)) if t == seen + 1 => {
                self.from = prev;
                self.to = now;
            }
            (Some(_), Some(_)) => {
                self.from = self.to;
                self.to = now;
            }
            _ => {
                self.from = prev;
                self.to = now;
            }
        }
        self.tick = tick;
        if self.to != now {
            self.from = prev;
            self.to = now;
        }
        self.moving = (self.to - self.from).length_squared() > 1e-8;
        self.from.lerp(self.to, alpha)
    }

    fn turn_towards(&mut self, target: f32, t: f32) {
        self.yaw = lerp_angle(self.yaw, target, t);
    }

    fn heading(&self) -> Option<f32> {
        let d = self.to - self.from;
        (d.length_squared() > 1e-8).then(|| d.x.atan2(d.z))
    }
}

/// An actor's visual root.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct ActorVisual {
    pub kind: ModelKind,
    pub motion: Motion,
    pub hand_r: Option<Entity>,
    pub hand_l: Option<Entity>,
}

/// On an actor's model (the `WorldAssetRoot` child of its root).
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct ActorModel {
    pub root: Entity,
}

/// A vehicle's visual root.
#[derive(Component, Clone, Debug)]
pub(crate) struct VehicleVisual {
    kind: PropKind,
    pub motion: Motion,
    wheels: Vec<Entity>,
    spin: f32,
    last: Option<Vec3>,
}

#[derive(Component, Clone, Copy, Debug)]
struct VehicleModel {
    root: Entity,
    kind: PropKind,
}

/// Faction ring mesh and materials.
#[derive(Resource, Clone)]
pub(crate) struct Rings {
    mesh: Handle<Mesh>,
    owner: [Handle<StandardMaterial>; 3],
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Startup, make_rings)
        .add_systems(
            Update,
            (despawn_visuals, spawn_visuals)
                .chain()
                .in_set(ViewSystems::Sync),
        )
        .add_systems(
            Update,
            (
                (tree_states, ore_states, swing_doors),
                vehicle_transforms,
                spin_wheels,
                actor_transforms,
                item_transforms,
            )
                .chain()
                .after(FogSet)
                .in_set(ViewSystems::Animate),
        );
}

fn make_rings(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut mat = |c: Color| {
        materials.add(StandardMaterial {
            base_color: c,
            unlit: true,
            ..default()
        })
    };
    commands.insert_resource(Rings {
        mesh: meshes.add(Annulus::new(0.42, 0.52)),
        owner: [
            mat(Color::srgb(0.75, 0.75, 0.75)),
            mat(Color::srgb(0.2, 0.5, 1.0)),
            mat(Color::srgb(0.95, 0.18, 0.15)),
        ],
    });
}

/// Ring colour index of an owner (0 = none/other, 1, 2).
pub(crate) fn owner_slot(owner: Option<&Owner>) -> usize {
    match owner.map(|o| o.0.0) {
        Some(1) => 1,
        Some(2) => 2,
        _ => 0,
    }
}

fn model(scene: &Handle<bevy::world_serialization::WorldAsset>, t: Transform) -> impl Bundle {
    (WorldAssetRoot(scene.clone()), t)
}

fn yaw_of(prop: &Prop) -> Quat {
    Quat::from_rotation_y((prop.yaw_deg as f32).to_radians())
}

/// A deterministic per-entity variation in `0..n` (from its `SimId`).
fn vary(id: Option<&SimId>, n: u32) -> u32 {
    id.map_or(0, |i| i.0.wrapping_mul(2_654_435_761) >> 16) % n.max(1)
}

#[allow(clippy::too_many_arguments)]
fn spawn_visuals(
    mut commands: Commands,
    assets: Option<Res<ViewAssets>>,
    rings: Option<Res<Rings>>,
    mut index: ResMut<VisualIndex>,
    alive: Query<(), With<VisualOf>>,
    props: Query<(Entity, Option<&SimId>, &Prop, &SimPos, Option<&GridBlocker>)>,
    items: Query<(Entity, &Item, &SimPos), (Without<Prop>, Without<TestbedUnit>)>,
    actors: Query<(Entity, &TestbedUnit, &SimPos, Option<&Owner>), Without<Prop>>,
) {
    let (Some(assets), Some(rings)) = (assets, rings) else {
        return;
    };
    let m = &assets.models;
    let fresh = |index: &mut VisualIndex, sim: Entity| match index.map.get(&sim) {
        Some(v) if alive.contains(*v) => false,
        _ => {
            index.map.remove(&sim);
            true
        }
    };

    for (sim, id, prop, pos, blocker) in &props {
        if !fresh(&mut index, sim) {
            continue;
        }
        let at = sim_to_world(*pos);
        let root_id = commands
            .spawn((
                Name::new(format!("visual {}", prop.kind.name())),
                VisualOf(sim),
                Transform::from_translation(at).with_rotation(yaw_of(prop)),
                Visibility::default(),
                Fogged::default(),
                FogDim::default(),
            ))
            .id();
        match prop.kind {
            PropKind::Wall => {
                commands.entity(root_id).insert(StaticVisual);
                spawn_wall(&mut commands, (root_id, at), m, blocker, prop);
            }
            PropKind::Doorway => {
                commands.entity(root_id).insert((
                    DoorVisual {
                        angle: 0.0,
                        pivot: None,
                    },
                    Selectable,
                ));
                commands
                    .spawn((
                        ChildOf(root_id),
                        DoorFrame { root: root_id },
                        model(
                            &m.doorway,
                            Transform::from_scale(Vec3::new(DOOR_SX, DOOR_SY, DOOR_SZ)),
                        ),
                    ))
                    .observe(on_doorway_ready);
            }
            PropKind::Tree => {
                let yaw = Quat::from_rotation_y(vary(id, 360) as f32 * PI / 180.0);
                let b = vary(id, 2) == 1;
                let scale = Vec3::splat(HEX * if b { 1.05 } else { 1.0 });
                let tree = commands
                    .spawn((
                        ChildOf(root_id),
                        model(
                            if b { &m.tree_b } else { &m.tree_a },
                            Transform::from_rotation(yaw).with_scale(scale),
                        ),
                    ))
                    .id();
                let stump = commands
                    .spawn((
                        ChildOf(root_id),
                        model(
                            if b { &m.stump_b } else { &m.stump_a },
                            Transform::from_rotation(yaw).with_scale(scale),
                        ),
                        Visibility::Hidden,
                    ))
                    .id();
                commands.entity(root_id).insert((
                    TreeVisual {
                        tree,
                        stump,
                        depleted: None,
                    },
                    Selectable,
                ));
            }
            PropKind::OreRock => {
                let model_id = commands
                    .spawn((
                        ChildOf(root_id),
                        model(&m.stone, Transform::from_scale(Vec3::splat(HEX))),
                    ))
                    .id();
                commands.entity(root_id).insert((
                    OreVisual {
                        model: model_id,
                        shown: None,
                    },
                    Selectable,
                ));
            }
            PropKind::Workbench | PropKind::Anvil | PropKind::Forge => {
                commands.entity(root_id).insert((StaticVisual, Selectable));
                let scene = match prop.kind {
                    PropKind::Workbench => &m.workbench,
                    PropKind::Anvil => &m.anvil,
                    _ => &m.forge,
                };
                commands.spawn((
                    ChildOf(root_id),
                    model(scene, Transform::from_scale(Vec3::splat(SURVIVAL))),
                ));
            }
            PropKind::Chest => {
                commands.entity(root_id).insert((StaticVisual, Selectable));
                commands.spawn((
                    ChildOf(root_id),
                    model(
                        &m.chest,
                        Transform::from_rotation(Quat::from_rotation_y(PI))
                            .with_scale(Vec3::splat(0.6)),
                    ),
                ));
            }
            PropKind::Cart | PropKind::Wagon => {
                commands.entity(root_id).insert((
                    VehicleVisual {
                        kind: prop.kind,
                        motion: Motion::at(at),
                        wheels: Vec::new(),
                        spin: 0.0,
                        last: None,
                    },
                    Selectable,
                ));
                let (scene, t) = if prop.kind == PropKind::Cart {
                    // A hand-cart leads with its handles (model -z).
                    (
                        &m.cart,
                        Transform::from_rotation(Quat::from_rotation_y(PI))
                            .with_scale(Vec3::splat(CART_SCALE)),
                    )
                } else {
                    // The ram is long along model x: turn it so it drives
                    // along +Z like the cart.
                    (
                        &m.ram,
                        Transform::from_rotation(Quat::from_rotation_y(-PI / 2.0))
                            .with_scale(Vec3::splat(RAM_SCALE)),
                    )
                };
                commands
                    .spawn((
                        ChildOf(root_id),
                        VehicleModel {
                            root: root_id,
                            kind: prop.kind,
                        },
                        model(scene, t),
                    ))
                    .observe(on_vehicle_ready);
            }
        }
        index.map.insert(sim, root_id);
    }

    for (sim, item, pos) in &items {
        if !fresh(&mut index, sim) {
            continue;
        }
        let look = ItemLook::of(&item.kind);
        let root = commands
            .spawn((
                Name::new(format!("visual item {}", item.kind)),
                VisualOf(sim),
                Selectable,
                Transform::from_translation(sim_to_world(*pos)),
                Visibility::default(),
                Fogged::default(),
                FogDim::default(),
            ))
            .id();
        let model_id = commands
            .spawn((ChildOf(root), model(look.scene(m), look.lying())))
            .id();
        commands.entity(root).insert(ItemVisual {
            model: model_id,
            look,
            attached: None,
        });
        index.map.insert(sim, root);
    }

    for (sim, unit, pos, owner) in &actors {
        if !fresh(&mut index, sim) {
            continue;
        }
        let kind = ModelKind::for_actor(&unit.template, owner.map(|o| o.0.0));
        let at = sim_to_world(*pos);
        let root = commands
            .spawn((
                Name::new(format!("visual actor {}", unit.template)),
                VisualOf(sim),
                Selectable,
                ActorVisual {
                    kind,
                    motion: Motion::at(at),
                    hand_r: None,
                    hand_l: None,
                },
                Transform::from_translation(at),
                Visibility::default(),
                Fogged::default(),
                FogDim::default(),
            ))
            .id();
        commands
            .spawn((
                ChildOf(root),
                ActorModel { root },
                model(
                    &assets.actor[kind.index()],
                    Transform::from_scale(Vec3::splat(V)),
                ),
            ))
            .observe(crate::anim::on_actor_ready);
        commands.spawn((
            ChildOf(root),
            Name::new("faction ring"),
            Mesh3d(rings.mesh.clone()),
            MeshMaterial3d(rings.owner[owner_slot(owner)].clone()),
            Transform::from_xyz(0.0, 0.03, 0.0).with_rotation(Quat::from_rotation_x(-PI / 2.0)),
        ));
        index.map.insert(sim, root);
    }
}

/// Tiles a wall run (`GridBlocker` footprint, local x along the run) with
/// `wall` pieces stretched to its length. Each piece is fogged by the cells
/// it covers (`FogArea`).
fn spawn_wall(
    commands: &mut Commands,
    (root, at): (Entity, Vec3),
    m: &Models,
    blocker: Option<&GridBlocker>,
    prop: &Prop,
) {
    let Some(b) = blocker else { return };
    let half = if prop.yaw_deg == 0 {
        b.0.half_x_mm
    } else {
        b.0.half_z_mm
    };
    // A run of n cells has a footprint half extent of (n-1)/2 m.
    let len = 2.0 * half as f32 / 1000.0 + 1.0;
    let pieces = (len / (WALL_PIECE * V)).ceil().max(1.0);
    let piece = len / pieces;
    let yaw = yaw_of(prop);
    for i in 0..pieces as i32 {
        let x = -len / 2.0 + piece * (i as f32 + 0.5);
        commands.spawn((
            ChildOf(root),
            model(
                &m.wall,
                Transform::from_xyz(x, 0.0, 0.0).with_scale(Vec3::new(
                    piece / WALL_PIECE,
                    WALL_SCALE_Y,
                    WALL_SCALE_Z,
                )),
            ),
            Visibility::default(),
            FogArea::around(
                at + yaw * Vec3::new(x, 0.0, 0.0),
                yaw * Vec3::new(piece / 2.0, 0.0, 0.5),
            ),
            FogDim::default(),
        ));
    }
}

/// Re-parents the door leaf onto a hinge pivot under the visual root (see
/// the module docs).
fn on_doorway_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    frames: Query<&DoorFrame>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut doors: Query<&mut DoorVisual>,
) {
    let Ok(frame) = frames.get(ready.entity) else {
        return;
    };
    let Some(leaf) = children.iter_descendants(ready.entity).find(|e| {
        names
            .get(*e)
            .is_ok_and(|n| n.as_str() == "wall_doorway_door")
    }) else {
        warn!("testbed_view: doorway model has no `wall_doorway_door` node");
        return;
    };
    let pivot = commands
        .spawn((
            Name::new("door hinge"),
            ChildOf(frame.root),
            Transform::from_xyz(DOOR_HINGE_X * DOOR_SX, 0.0, DOOR_HINGE_Z).with_scale(LEAF_SCALE),
            Visibility::default(),
        ))
        .id();
    commands
        .entity(leaf)
        .insert((ChildOf(pivot), Transform::IDENTITY));
    if let Ok(mut door) = doors.get_mut(frame.root) {
        door.pivot = Some(pivot);
    }
}

/// Wheel re-pivoting: `(node name, centre offset from the node origin, model
/// units)` per vehicle model (measured).
fn wheel_spec(kind: PropKind) -> (&'static str, Vec3) {
    match kind {
        PropKind::Cart => ("Group", Vec3::new(-0.061, 0.234, 0.234)),
        _ => ("wheel", Vec3::new(-0.2015, 0.2015, 0.0665)),
    }
}

/// Inserts a pivot at each wheel's centre (the nodes' origins sit on a rim
/// corner) so wheels spin about their hub.
#[allow(clippy::too_many_arguments)]
fn on_vehicle_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    models: Query<&VehicleModel>,
    children: Query<&Children>,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
    transforms: Query<&Transform>,
    mut vehicles: Query<&mut VehicleVisual>,
) {
    let Ok(vm) = models.get(ready.entity) else {
        return;
    };
    let (name, centre) = wheel_spec(vm.kind);
    let mut wheels = Vec::new();
    for e in children.iter_descendants(ready.entity) {
        if !names.get(e).is_ok_and(|n| n.as_str() == name) {
            continue;
        }
        let (Ok(parent), Ok(t)) = (parents.get(e), transforms.get(e)) else {
            continue;
        };
        let pivot = commands
            .spawn((
                Name::new("wheel hub"),
                ChildOf(parent.parent()),
                Transform::from_translation(t.translation + centre),
                Visibility::default(),
            ))
            .id();
        commands
            .entity(e)
            .insert((ChildOf(pivot), Transform::from_translation(-centre)));
        wheels.push(pivot);
    }
    if let Ok(mut v) = vehicles.get_mut(vm.root) {
        v.wheels = wheels;
    }
}

/// Despawns visuals whose sim entity is gone.
fn despawn_visuals(
    mut commands: Commands,
    mut index: ResMut<VisualIndex>,
    sims: Query<(), Or<(With<Prop>, With<Item>, With<TestbedUnit>)>>,
    visuals: Query<(), With<VisualOf>>,
) {
    index.map.retain(|&sim, &mut visual| {
        if sims.contains(sim) {
            return true;
        }
        if visuals.contains(visual) {
            commands.entity(visual).despawn();
        }
        false
    });
}

/// The last-seen fact (`fact` of its `SeenState`) a fogged visual shows
/// instead of the live one, if any.
fn last_seen(fogged: Option<&Fogged>, fact: impl Fn(&SeenState) -> Option<bool>) -> Option<bool> {
    match fogged {
        Some(Fogged(Look::LastSeen(s))) => fact(s),
        _ => None,
    }
}

fn tree_states(
    sims: Query<&ToolTarget>,
    mut trees: Query<(&VisualOf, &mut TreeVisual, Option<&Fogged>)>,
    mut vis: Query<&mut Visibility>,
) {
    for (of, mut t, fogged) in &mut trees {
        let depleted = last_seen(fogged, |s| s.felled)
            .unwrap_or_else(|| sims.get(of.0).is_ok_and(|tt| tt.remaining == Some(0)));
        if t.depleted == Some(depleted) {
            continue;
        }
        t.depleted = Some(depleted);
        let (show, hide) = if depleted {
            (t.stump, t.tree)
        } else {
            (t.tree, t.stump)
        };
        if let Ok(mut v) = vis.get_mut(show) {
            *v = Visibility::Inherited;
        }
        if let Ok(mut v) = vis.get_mut(hide) {
            *v = Visibility::Hidden;
        }
    }
}

fn ore_states(
    sims: Query<&ToolTarget>,
    mut ores: Query<(&VisualOf, &mut OreVisual)>,
    mut transforms: Query<&mut Transform>,
) {
    for (of, mut ore) in &mut ores {
        let remaining = sims.get(of.0).ok().and_then(|t| t.remaining);
        if ore.shown == Some(remaining) {
            continue;
        }
        ore.shown = Some(remaining);
        // Six uses: full size down to a pebble pile.
        let f = remaining.map_or(1.0, |r| 0.35 + 0.65 * (r.min(6) as f32 / 6.0));
        if let Ok(mut t) = transforms.get_mut(ore.model) {
            t.scale = Vec3::splat(HEX * f);
        }
    }
}

fn swing_doors(
    time: Res<Time>,
    sims: Query<&Door>,
    mut doors: Query<(&VisualOf, &mut DoorVisual, Option<&Fogged>)>,
    mut transforms: Query<&mut Transform>,
) {
    let step = DOOR_RATE * time.delta_secs();
    for (of, mut door, fogged) in &mut doors {
        let open =
            last_seen(fogged, |s| s.open).unwrap_or_else(|| sims.get(of.0).is_ok_and(|d| d.open));
        let want = if open { DOOR_OPEN } else { 0.0 };
        let d = want - door.angle;
        door.angle += d.clamp(-step, step);
        let Some(pivot) = door.pivot else { continue };
        if let Ok(mut t) = transforms.get_mut(pivot) {
            // Ease the swing: slow at both ends.
            let eased = DOOR_OPEN * smooth(door.angle / DOOR_OPEN);
            let r = Quat::from_rotation_y(eased);
            if t.rotation != r {
                t.rotation = r;
            }
        }
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(crate) fn lerp_angle(from: f32, to: f32, t: f32) -> f32 {
    let mut d = (to - from) % (2.0 * PI);
    if d > PI {
        d -= 2.0 * PI;
    } else if d < -PI {
        d += 2.0 * PI;
    }
    from + d * t
}

type MotionInputs<'w, 's> = (
    Res<'w, Time>,
    Option<Res<'w, SimTime>>,
    Option<Res<'w, SimRate>>,
    Option<Res<'w, SimControl>>,
    Option<Res<'w, SimClock>>,
);

fn frame_params(inputs: &MotionInputs) -> (f32, Option<u64>, f32) {
    let (time, sim_time, rate, control, clock) = inputs;
    let alpha = interp_alpha(sim_time.as_deref(), rate.as_deref(), control.as_deref());
    let turn = 1.0 - (-TURN_RATE * time.delta_secs()).exp();
    (alpha, clock.as_ref().map(|c| c.tick), turn)
}

fn vehicle_transforms(
    inputs: MotionInputs,
    sims: Query<(&SimPos, Option<&PrevSimPos>)>,
    mut visuals: Query<(&VisualOf, &mut Transform, &mut VehicleVisual)>,
) {
    let (alpha, tick, turn) = frame_params(&inputs);
    for (of, mut transform, mut vis) in &mut visuals {
        let Ok((pos, prev)) = sims.get(of.0) else {
            continue;
        };
        let now = sim_to_world(*pos);
        let prev = prev.map_or(now, |p| sim_to_world(p.0));
        let at = vis.motion.step(tick, prev, now, alpha);
        if let Some(h) = vis.motion.heading() {
            vis.motion.turn_towards(h, turn * 0.5);
        }
        transform.translation = at;
        transform.rotation = Quat::from_rotation_y(vis.motion.yaw);
    }
}

fn spin_wheels(
    mut vehicles: Query<(&Transform, &mut VehicleVisual)>,
    mut transforms: Query<&mut Transform, Without<VehicleVisual>>,
) {
    for (t, mut v) in &mut vehicles {
        let here = t.translation;
        let moved = v
            .last
            .map_or(0.0, |last| (here - last).dot(t.rotation * Vec3::Z));
        v.last = Some(here);
        if moved.abs() < 1e-6 {
            continue;
        }
        let (axis, radius, sign) = match v.kind {
            PropKind::Cart => (Vec3::X, 0.234 * CART_SCALE, 1.0),
            _ => (Vec3::Z, 0.2015 * RAM_SCALE, -1.0),
        };
        v.spin = (v.spin + sign * moved / radius) % (2.0 * PI);
        let r = Quat::from_axis_angle(axis, v.spin);
        for &w in &v.wheels {
            if let Ok(mut wt) = transforms.get_mut(w) {
                wt.rotation = r;
            }
        }
    }
}

/// Where a rider sits on a vehicle, in the vehicle's frame (+Z forward).
/// The sim's seat offsets are world-axis mm (vehicles have no heading in
/// the sim); the view reads them as vehicle-local (forward = +z), lifts the
/// rider onto the bed and spreads riders that share an offset sideways.
fn seat_local(kind: PropKind, vehicle: &Vehicle, seat: u8) -> Vec3 {
    let seat = seat as usize;
    let offset = vehicle
        .seats
        .get(seat)
        .map_or(SimPos::new(0, 0), |s| s.offset);
    let same: Vec<usize> = vehicle
        .seats
        .iter()
        .enumerate()
        .filter(|(_, s)| s.offset == offset)
        .map(|(i, _)| i)
        .collect();
    let k = same.iter().position(|&i| i == seat).unwrap_or(0) as f32;
    let lateral = (k - (same.len() as f32 - 1.0) / 2.0) * 0.8;
    let (lift, forward) = match kind {
        // In the bed, behind the handles.
        PropKind::Cart => (0.42, -0.35),
        _ => (0.8, 0.5),
    };
    Vec3::new(
        offset.x as f32 / 1000.0 + lateral,
        lift,
        offset.z as f32 / 1000.0 + forward,
    )
}

#[allow(clippy::type_complexity)]
fn actor_transforms(
    inputs: MotionInputs,
    sims: Query<(
        &SimPos,
        Option<&PrevSimPos>,
        Option<&npc_framework::prelude::Facing>,
        Option<&MountedIn>,
    )>,
    index: Option<Res<SimIndex>>,
    vehicle_sims: Query<(&Vehicle, &Prop)>,
    vis_index: Res<VisualIndex>,
    vehicle_visuals: Query<&Transform, (With<VehicleVisual>, Without<ActorVisual>)>,
    mut visuals: Query<(&VisualOf, &mut Transform, &mut ActorVisual)>,
) {
    let (alpha, tick, turn) = frame_params(&inputs);
    for (of, mut transform, mut vis) in &mut visuals {
        let Ok((pos, prev, facing, mounted)) = sims.get(of.0) else {
            continue;
        };
        let now = sim_to_world(*pos);
        let prev = prev.map_or(now, |p| sim_to_world(p.0));
        let at = vis.motion.step(tick, prev, now, alpha);

        // Riders sit on their vehicle's visual, facing its way.
        if let Some(m) = mounted {
            let ride = index
                .as_deref()
                .and_then(|i| i.get(m.vehicle))
                .and_then(|ve| {
                    let (v, p) = vehicle_sims.get(ve).ok()?;
                    let vt = vis_index
                        .map
                        .get(&ve)
                        .and_then(|e| vehicle_visuals.get(*e).ok())?;
                    Some((vt, seat_local(p.kind, v, m.seat)))
                });
            if let Some((vt, local)) = ride {
                transform.translation = vt.translation + vt.rotation * local;
                transform.rotation = vt.rotation;
                vis.motion.yaw = vt.rotation.to_euler(EulerRot::YXZ).0;
                continue;
            }
        }

        let target = vis.motion.heading().or_else(|| {
            facing
                .filter(|f| f.x != 0 || f.z != 0)
                .map(|f| (f.x as f32).atan2(f.z as f32))
        });
        if let Some(t) = target {
            vis.motion.turn_towards(t, turn);
        }
        transform.translation = at;
        transform.rotation = Quat::from_rotation_y(vis.motion.yaw);
    }
}

/// How an item looks: model, size, how it lies on the ground and how a hand
/// holds it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ItemLook {
    model: fn(&Models) -> &Handle<bevy::world_serialization::WorldAsset>,
    /// World scale (metres per model unit).
    scale: f32,
    /// Rotation when lying, and the lift that puts it on the ground (model
    /// units).
    lie: Quat,
    lift: f32,
    /// Rotation and offset (hand-bone units) when held, if it can be held.
    hold: Option<(Quat, Vec3)>,
}

impl ItemLook {
    fn of(kind: &str) -> Self {
        let flat_x = Quat::from_rotation_x(PI / 2.0);
        let upright = Quat::IDENTITY;
        let (model, scale, lie, lift, hold): (
            fn(&Models) -> &Handle<bevy::world_serialization::WorldAsset>,
            f32,
            Quat,
            f32,
            Option<(Quat, Vec3)>,
        ) = match kind {
            AXE => (
                |m| &m.axe,
                V,
                flat_x,
                0.1,
                Some((Quat::from_rotation_y(PI), Vec3::new(0.0, 0.03, 0.0))),
            ),
            PICKAXE => (
                |m| &m.pickaxe,
                KENNEY_TOOL * V,
                flat_x,
                0.03,
                Some((Quat::from_rotation_y(PI), Vec3::new(0.0, -0.25, 0.0))),
            ),
            HAMMER => (
                |m| &m.hammer,
                KENNEY_TOOL * V,
                flat_x,
                0.03,
                Some((Quat::from_rotation_y(PI), Vec3::new(0.0, -0.18, 0.0))),
            ),
            STORE_KEY => (|m| &m.key, 0.5, flat_x, 0.08, None),
            BOOK => (|m| &m.book, V, Quat::from_rotation_z(PI / 2.0), 0.13, None),
            SCROLL => (|m| &m.scroll, 1.5 * V, upright, 0.0, None),
            BREAD => (|m| &m.food, 0.5 * V, upright, 0.0, None),
            LOG => (|m| &m.log, 2.0 * V, upright, 0.11, None),
            PLANK => (|m| &m.planks, 2.0 * V, upright, 0.0, None),
            ORE => (|m| &m.stone, 1.4 * V, upright, 0.0, None),
            INGOT => (|m| &m.ingot, 0.6 * V, upright, 0.0, None),
            _ => (|m| &m.crate_large, 0.25 * V, upright, 0.0, None),
        };
        Self {
            model,
            scale,
            lie,
            lift,
            hold,
        }
    }

    fn scene<'a>(&self, m: &'a Models) -> &'a Handle<bevy::world_serialization::WorldAsset> {
        (self.model)(m)
    }

    /// The model's transform under a root standing on the ground.
    fn lying(&self) -> Transform {
        Transform::from_xyz(0.0, self.lift * self.scale, 0.0)
            .with_rotation(self.lie)
            .with_scale(Vec3::splat(self.scale))
    }

    /// The model's transform under a hand bone (which already carries the
    /// actor's scale `V`).
    fn held(&self) -> Option<Transform> {
        self.hold.map(|(r, t)| {
            Transform::from_translation(t)
                .with_rotation(r)
                .with_scale(Vec3::splat(self.scale / V))
        })
    }
}

/// Items in the world lie at their `SimPos`; an equipped tool rides in its
/// holder's hand; anything else carried is hidden. Under fog an item lying
/// in ground not visible now lies where it was last seen, or is hidden.
#[allow(clippy::type_complexity)]
fn item_transforms(
    mut commands: Commands,
    sims: Query<(&SimPos, Option<&CarriedBy>)>,
    index: Option<Res<SimIndex>>,
    vis_index: Res<VisualIndex>,
    actors: Query<&ActorVisual>,
    mut items: Query<
        (
            Entity,
            &VisualOf,
            &mut ItemVisual,
            &mut Transform,
            &mut Visibility,
            Has<Selectable>,
            Option<&Fogged>,
        ),
        Without<ActorVisual>,
    >,
) {
    for (root, of, mut item, mut transform, mut visibility, selectable, fogged) in &mut items {
        let Ok((pos, carried)) = sims.get(of.0) else {
            continue;
        };
        let look = fogged.map_or(Look::Live, |f| f.0);
        let hand = carried.and_then(|c| {
            let slot = c.slot.as_deref()?;
            let holder = index.as_deref()?.get(c.holder)?;
            let actor = actors.get(*vis_index.map.get(&holder)?).ok()?;
            match slot {
                MAIN_HAND => actor.hand_r,
                OFF_HAND => actor.hand_l,
                _ => None,
            }
        });
        let held = hand.zip(item.look.held());
        match (carried, held) {
            (_, Some((hand, held))) => {
                if item.attached != Some(hand) {
                    item.attached = Some(hand);
                    commands.entity(root).insert(ChildOf(hand));
                    *transform = Transform::IDENTITY;
                    commands.entity(item.model).insert(held);
                    commands.entity(root).remove::<Selectable>();
                }
                if *visibility != Visibility::Inherited {
                    *visibility = Visibility::Inherited;
                }
            }
            (Some(_), None) => {
                if item.attached.is_some() {
                    item.attached = None;
                    commands.entity(root).remove::<ChildOf>();
                    commands.entity(item.model).insert(item.look.lying());
                }
                if selectable {
                    commands.entity(root).remove::<Selectable>();
                }
                if *visibility != Visibility::Hidden {
                    *visibility = Visibility::Hidden;
                }
            }
            (None, _) => {
                if item.attached.is_some() {
                    item.attached = None;
                    commands.entity(root).remove::<ChildOf>();
                    commands.entity(item.model).insert(item.look.lying());
                }
                if !selectable {
                    commands.entity(root).insert(Selectable);
                }
                let want_vis = if look.shown() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if *visibility != want_vis {
                    *visibility = want_vis;
                }
                let at = match look {
                    Look::LastSeen(s) => s.pos,
                    _ => *pos,
                };
                let want = Transform::from_translation(sim_to_world(at));
                if *transform != want {
                    *transform = want;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use npc_framework::prelude::Seat;
    use testbed_content::layout::{cart_vehicle, wagon_vehicle};

    use super::*;

    #[test]
    fn models_follow_faction_and_template() {
        assert_eq!(ModelKind::for_actor("smith", Some(1)), ModelKind::Knight);
        assert_eq!(ModelKind::for_actor("scout", Some(1)), ModelKind::Rogue);
        assert_eq!(
            ModelKind::for_actor("skeleton_warrior", Some(2)),
            ModelKind::SkeletonWarrior
        );
        assert_eq!(
            ModelKind::for_actor("skeleton_minion", None),
            ModelKind::SkeletonMinion
        );
        assert_eq!(
            ModelKind::for_actor("novice", Some(2)),
            ModelKind::SkeletonMinion
        );
    }

    #[test]
    fn riders_sit_in_the_bed_and_share_offsets_sideways() {
        let cart = cart_vehicle();
        let driver = seat_local(PropKind::Cart, &cart, 0);
        let passenger = seat_local(PropKind::Cart, &cart, 1);
        assert!(driver.y > 0.0 && passenger.y == driver.y);
        assert!(
            passenger.z < driver.z,
            "the passenger sits behind the driver"
        );
        let wagon = wagon_vehicle();
        let (a, b) = (
            seat_local(PropKind::Wagon, &wagon, 1),
            seat_local(PropKind::Wagon, &wagon, 2),
        );
        assert_eq!(a.z, b.z);
        assert!(
            (a.x + b.x).abs() < 1e-6 && a.x != b.x,
            "spread around the axis"
        );
        // A seat index past the end does not panic.
        let lone = Vehicle::direct(10).seat(Seat::driver());
        seat_local(PropKind::Wagon, &lone, 5);
    }

    #[test]
    fn door_scale_matches_the_one_metre_cell() {
        // The leaf is 2 model units wide; squeezed it fills the 1 m door cell.
        assert!((2.0 * DOOR_SX - 1.0).abs() < 1e-6);
        // A Dungeon wall piece is 3 m at V.
        assert!((WALL_PIECE * V - 3.0).abs() < 1e-6);
        assert!(smooth(0.0) == 0.0 && smooth(1.0) == 1.0 && smooth(0.5) == 0.5);
    }

    #[test]
    fn every_item_kind_has_a_look() {
        for kind in [
            AXE, PICKAXE, HAMMER, LOG, PLANK, ORE, INGOT, BREAD, BOOK, SCROLL, STORE_KEY,
        ] {
            let look = ItemLook::of(kind);
            assert!(look.scale > 0.0, "{kind}");
            assert!(look.lying().translation.y >= 0.0, "{kind}");
        }
        // Tools can be held; the rest is hidden while carried.
        assert!(ItemLook::of(AXE).held().is_some());
        assert!(ItemLook::of(PICKAXE).held().is_some());
        assert!(ItemLook::of(BOOK).held().is_none());
    }

    #[test]
    fn angles_take_the_short_way_round() {
        let a = lerp_angle(3.0, -3.0, 0.5);
        assert!(a > 3.0, "{a}");
        assert_eq!(lerp_angle(0.0, 1.0, 1.0), 1.0);
    }
}
