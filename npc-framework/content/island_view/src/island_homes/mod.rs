//! The island's homes and its inn as places somebody can be inside.
//!
//! Three things, all view entities that read the simulation and never write
//! it:
//!
//! - **The shell.** Every home's house model is fitted to the body the
//!   simulation gives it (`view::home`): its walls cover the body's cells,
//!   its door faces the entrance and is a door's height. The inn wears the
//!   two-storey hall, fitted the same way.
//! - **Who is drawn.** A resident the simulation has inside a building walks
//!   the last step from the entrance into the doorway and is then hidden;
//!   coming out is the reverse.
//! - **The cutaway.** A home that is selected, whose occupant is selected or
//!   that a close camera looks at loses its roof and the walls on the
//!   camera's side, and shows its beds, table and hearth with the people
//!   inside. Interior entities exist only while a home is cut away.
mod chimney;
pub(crate) mod model;
pub(crate) mod snow;
#[cfg(test)]
mod tests;
mod upgrade;

use std::collections::{BTreeMap, BTreeSet};
use std::f32::consts::{FRAC_PI_2, PI};

use bevy::{ecs::query::QueryState, prelude::*, world_serialization::WorldAsset};
use island_content::village::{
    Role, VillagePlace, VillageState,
    colony::{AwaitingArrival, Household},
    frontier::world_spec::LivingWorldSpec,
    homestead::Cottage,
    indoors::{Indoors, Repose},
    view,
};
use npc_framework::prelude::{SimClock, SimConfig, SimId, SimIndex, SimPos};
use npc_framework::view_helpers::{Selected, ViewSystems, VisualIndex, VisualOf};

use crate::IslandMode;
use crate::camera::RtsCamera;
use crate::fog::{Fogged, Look};
use crate::island_atmosphere::daylight::{ViewDayOverride, lamp_level, view_daylight};
use crate::island_atmosphere::palette::{self, RoofPalette, RoofTarget};
use crate::village_world::StaticId;
use crate::visuals::ActorVisual;
use model::{BodyFrame, Drawn, Interior, ModelFit, Phase, Tone, Wall};

/// On the visual root of a home or an inn: the body it stands on and how
/// its model is fitted.
#[derive(Component, Clone, Debug)]
pub(crate) struct HomeShell {
    pub(crate) building: SimId,
    pub(crate) frame: BodyFrame,
    pub(crate) fit: ModelFit,
    /// Child of the root at the body's centre, turned with the body: the
    /// model, the lights and the inside hang from it.
    pub(crate) frame_entity: Entity,
    /// That child's transform under the root.
    pub(crate) local: Transform,
    model_entity: Option<Entity>,
    pub(crate) inn: bool,
}

/// On a visual root the shell pass has looked at.
#[derive(Component)]
struct ShellChecked;

/// On the visual root of a building that can stand between the camera and
/// somebody: the box it fills and the entity that is hidden to take it out
/// of the way.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct Occluder {
    pub(crate) frame: BodyFrame,
    pub(crate) height: f32,
    pub(crate) drawn: Entity,
}

/// On the root of a tree that can stand between the camera and the played
/// character: its trunk and crown over the root, and the model that is
/// squashed to take it out of the way.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct Canopy {
    pub(crate) shape: model::TreeShape,
    pub(crate) drawn: Entity,
}

/// One building or tree being taken out of the way.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Cut {
    /// The entity squashed and hidden.
    drawn: Entity,
    /// Seconds since it last blocked the view.
    idle: f32,
    /// 0 standing, 1 squashed to its footing and hidden.
    cut: f32,
    /// Its own vertical scale, put back when it stands again.
    base_y: Option<f32>,
}

/// What `unblock` remembers between frames: whether it reports and what it
/// reported last, and the last line of sight with what blocked it.
#[derive(Default)]
struct UnblockLocals {
    report: (Option<bool>, usize),
    memo: Option<(Vec3, Vec3, Vec<(Entity, Entity)>)>,
}

/// Buildings and trees taken out of the way now, by root.
#[derive(Resource, Default)]
pub(crate) struct Unblocked(BTreeMap<Entity, Cut>);

/// Homes standing between the camera and whoever is followed, with the
/// seconds since they last did: these are cut away (the room stays), not
/// squashed like a workshop or a tree.
#[derive(Resource, Default)]
pub(crate) struct HomeBlocks(pub(crate) BTreeMap<SimId, f32>, pub(crate) Option<Vec3>);

impl Unblocked {
    /// Whether the building on this visual root is going or gone: its smoke
    /// and its lights go with it.
    pub(crate) fn hides(&self, root: Entity) -> bool {
        self.0.get(&root).is_some_and(|cut| cut.cut > 0.0)
    }
}

/// Visual roots that carry a [`HomeShell`], by building.
#[derive(Resource, Default)]
pub(crate) struct Shells(BTreeMap<SimId, Entity>);

/// Where somebody inside a cut-away home is drawn, in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Seat {
    /// Asleep in a bed: the middle of the mattress top and the way the
    /// head points.
    Bed { centre: Vec3, head_yaw: f32 },
    /// Up, on the floor.
    Floor { at: Vec3, yaw: f32 },
}

struct Open {
    interior: Entity,
    /// 0 when it opened, 1 once the model has sunk away.
    ease: f32,
    /// Per wall: the standing and the lowered version.
    walls: [(Entity, Entity); 4],
    lowered: [bool; 4],
    light: Entity,
    ember: Entity,
    /// Sleeping figures in children's beds, shown at night.
    sleepers: Vec<Entity>,
    /// One entry per bed: whether it is a child's.
    beds: Vec<bool>,
    /// The hearth burns: read when the occupants are seated, once a tick.
    heated: bool,
    layout: Interior,
    frame: BodyFrame,
    ground: f32,
}

/// The homes that are cut away now, and where their occupants are drawn.
#[derive(Resource, Default)]
pub(crate) struct Cutaways {
    open: BTreeMap<SimId, Open>,
    seats: BTreeMap<Entity, Seat>,
    /// The building the close camera holds open.
    near: Option<SimId>,
    /// Models rising again after their cutaway closed: model, how far up
    /// (0 sunk, 1 standing) and their own vertical scale.
    rising: BTreeMap<SimId, (Entity, f32, f32)>,
    tick: Option<u64>,
}

impl Cutaways {
    pub(crate) fn is_open(&self, building: SimId) -> bool {
        self.open.contains_key(&building)
    }

    /// Where a caption over somebody drawn inside a cut-away home belongs:
    /// over their bed, or `lift` above the spot of floor they stand on. The
    /// simulation keeps them on the entrance point; this is where they are
    /// seen. `None` for anybody who is not drawn indoors.
    pub(crate) fn caption_anchor(&self, sim: Entity, lift: f32) -> Option<Vec3> {
        self.seats.get(&sim).map(|seat| match seat {
            Seat::Bed { centre, .. } => *centre + Vec3::Y * BED_CAPTION_LIFT_M,
            Seat::Floor { at, .. } => *at + Vec3::Y * lift,
        })
    }
}

/// How far over a mattress the caption of somebody lying on it sits.
const BED_CAPTION_LIFT_M: f32 = 0.9;

/// The box a home's or an inn's model fills, for picking: the body's low and
/// high corner on the ground plane (world x, z) and how high the model
/// stands, to its chimney. `None` for anything else.
pub(crate) fn body_box(world: &World, sim: Entity, id: SimId) -> Option<(Vec2, Vec2, f32)> {
    let (frame, inn) = subject(world, sim, id)?;
    let fit = model::house_model(inn).fit(&frame);
    let half = (frame.across() * frame.width * 0.5).abs() + (frame.dir * frame.depth * 0.5).abs();
    Some((frame.centre - half, frame.centre + half, fit.chimney().y))
}

struct Track {
    phase: Phase,
    /// The building entered last; kept while stepping out of it.
    building: Option<SimId>,
    /// This pass has the resident hidden.
    hidden: bool,
}

/// How far each resident is between outdoors and indoors.
#[derive(Resource, Default)]
struct IndoorViews(BTreeMap<Entity, Track>);

#[derive(Resource)]
struct ShellAssets {
    cube: Handle<Mesh>,
    ball: Handle<Mesh>,
    stone: Handle<StandardMaterial>,
    plaster: Handle<StandardMaterial>,
    timber: Handle<StandardMaterial>,
    floor: Handle<StandardMaterial>,
    soot: Handle<StandardMaterial>,
    ember_lit: Handle<StandardMaterial>,
    ember_cold: Handle<StandardMaterial>,
    blanket: Handle<StandardMaterial>,
    linen: Handle<StandardMaterial>,
    skin: Handle<StandardMaterial>,
    /// A woven rug's red and a clay pot's.
    rug: Handle<StandardMaterial>,
    clay: Handle<StandardMaterial>,
    bed: Option<Handle<WorldAsset>>,
}

impl ShellAssets {
    fn tone(&self, tone: Tone) -> &Handle<StandardMaterial> {
        match tone {
            Tone::Stone => &self.stone,
            Tone::Plaster => &self.plaster,
            Tone::Timber => &self.timber,
            Tone::Dark => &self.soot,
        }
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<Shells>()
        .init_resource::<Cutaways>()
        .init_resource::<IndoorViews>()
        .init_resource::<Stepping>()
        // Capture aid, read once: `ISLAND_VIEW_DOOR_HOLD=8` keeps a door
        // open that many seconds after a step, long enough to frame it.
        .insert_resource(DoorHold(
            std::env::var("ISLAND_VIEW_DOOR_HOLD")
                .ok()
                .and_then(|seconds| seconds.parse().ok())
                .unwrap_or(model::DOOR_LINGER_SECS),
        ))
        .init_resource::<OpenDoors>()
        .init_resource::<Unblocked>()
        .init_resource::<HomeBlocks>()
        .init_resource::<chimney::ChimneyMeshes>()
        .add_systems(
            Update,
            (upgrade::refit, dress, upgrade::scaffold)
                .chain()
                .in_set(ViewSystems::Sync),
        )
        .add_systems(
            Update,
            keep_hidden
                .in_set(ViewSystems::Animate)
                .after(crate::fog::FogSet),
        )
        .add_systems(
            Update,
            (cutaways, residents, doors, unblock, snow::settle)
                .chain()
                .in_set(ViewSystems::Present)
                .after(crate::fog::FogSet)
                .after(crate::colony_scene::living_scene::elevation),
        );
}

/// The body of a home or an inn; `None` for anything else. A homestead
/// cottage's is the 2 by 2 m the simulation blocks for it, which need not
/// lie on whole cells.
pub(crate) fn subject(world: &World, sim: Entity, id: SimId) -> Option<(BodyFrame, bool)> {
    if world.get::<Cottage>(sim).is_some() {
        return view::home(world, id).map(|home| {
            (
                BodyFrame::of_bounds(home.body_min, home.body_max, home.facing, home.entrance),
                false,
            )
        });
    }
    if world.get::<Household>(sim).is_some() {
        return view::home(world, id).map(|home| (BodyFrame::of_home(&home), false));
    }
    if world.get::<VillagePlace>(sim)?.role != Role::Innkeeper {
        return None;
    }
    // An inn: the body the simulation blocks for it, founding or built.
    let inn = view::inn(world, id)?;
    Some((
        BodyFrame::of_bounds(inn.body_min, inn.body_max, inn.facing, inn.entrance),
        true,
    ))
}

fn shell_assets(world: &mut World) -> bool {
    if world.contains_resource::<ShellAssets>() {
        return true;
    }
    if !world.contains_resource::<Assets<Mesh>>()
        || !world.contains_resource::<Assets<StandardMaterial>>()
    {
        return false;
    }
    let (cube, ball) = {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        (
            meshes.add(Cuboid::default()),
            meshes.add(Sphere::new(0.5).mesh().ico(2).expect("head sphere")),
        )
    };
    let bed = world.get_resource::<AssetServer>().map(|server| {
        server.load(GltfAssetLabel::Scene(0).from_asset("furniture/bed_single_A.gltf"))
    });
    let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
    let mut matte = |r: u8, g: u8, b: u8| {
        materials.add(StandardMaterial {
            base_color: Color::srgb_u8(r, g, b),
            perceptual_roughness: 0.95,
            ..default()
        })
    };
    // Stone, plaster and timber as the building atlas has them.
    let assets = ShellAssets {
        cube,
        ball,
        stone: matte(111, 119, 122),
        plaster: matte(226, 230, 233),
        timber: matte(150, 85, 64),
        floor: matte(133, 100, 66),
        soot: matte(34, 30, 28),
        ember_cold: matte(58, 44, 38),
        blanket: matte(74, 128, 186),
        linen: matte(222, 222, 214),
        skin: matte(226, 178, 142),
        rug: matte(150, 52, 44),
        clay: matte(176, 104, 66),
        ember_lit: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.45, 0.12),
            emissive: LinearRgba::new(6.5, 1.7, 0.2, 1.0),
            ..default()
        }),
        bed,
    };
    world.insert_resource(assets);
    true
}

fn block(
    world: &mut World,
    parent: Entity,
    material: &Handle<StandardMaterial>,
    mesh: &Handle<Mesh>,
    centre: Vec3,
    size: Vec3,
) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(centre).with_scale(size),
        ))
        .id()
}

fn group(world: &mut World, parent: Entity, shown: bool) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Transform::default(),
            if shown {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        ))
        .id()
}

/// Give every new home and inn root its fitted model.
fn dress(
    world: &mut World,
    fresh: &mut QueryState<
        (Entity, &VisualOf, &Transform),
        (With<StaticId>, Without<ShellChecked>),
    >,
) {
    let candidates: Vec<(Entity, Entity, Transform)> = fresh
        .iter(world)
        .map(|(root, of, transform)| (root, of.0, *transform))
        .collect();
    if candidates.is_empty() {
        return;
    }
    let drawn = shell_assets(world);
    for (root, sim, transform) in candidates {
        world.entity_mut(root).insert(ShellChecked);
        let Some(id) = world.get::<SimId>(sim).copied() else {
            continue;
        };
        let Some((frame, inn)) = subject(world, sim, id) else {
            continue;
        };
        let fit = model::house_model(inn).fit(&frame);
        let cottage = world.get::<Cottage>(sim).is_some();
        if cottage {
            // The homestead pass gives the root a small model of its own:
            // the fitted one takes its place.
            let own: Vec<Entity> = world
                .get::<Children>(root)
                .map(|children| {
                    children
                        .iter()
                        .filter(|child| world.get::<WorldAssetRoot>(*child).is_some())
                        .collect()
                })
                .unwrap_or_default();
            for model in own {
                world.entity_mut(model).despawn();
            }
        }
        // The body's frame under the root, wherever the root stands.
        let turn = transform.rotation.inverse();
        let local = Transform::from_translation(
            turn * (Vec3::new(frame.centre.x, transform.translation.y, frame.centre.y)
                - transform.translation),
        )
        .with_rotation(turn * frame.rotation());
        let frame_entity = world
            .spawn((
                Name::new("home frame"),
                ChildOf(root),
                local,
                Visibility::default(),
            ))
            .id();
        // The player's own cottage keeps the kit's blue roof.
        let roof = world
            .get_resource::<LivingWorldSpec>()
            .zip(world.get::<SimPos>(sim))
            .filter(|_| !cottage)
            .map_or(RoofPalette::Blue, |(spec, at)| {
                palette::settlement_palette(spec, *at)
            });
        let scene = world
            .get_resource::<AssetServer>()
            .map(|server| server.load(GltfAssetLabel::Scene(0).from_asset(fit.model.path())));
        let model_entity = scene.map(|scene: Handle<WorldAsset>| {
            let model = world
                .spawn((
                    Name::new("house model"),
                    ChildOf(frame_entity),
                    WorldAssetRoot(scene),
                    fit.transform(),
                ))
                .id();
            if roof != RoofPalette::Blue {
                world.entity_mut(model).insert(RoofTarget(roof));
            }
            if fit.model == model::HouseModel::Cottage {
                snow::cap(world, model);
                world
                    .entity_mut(model)
                    .insert(chimney::NarrowChimney {
                        scale_x: fit.scale.x,
                    })
                    .observe(chimney::on_ready);
            }
            model
        });
        if drawn {
            let (cube, stone, timber, plaster) = {
                let assets = world.resource::<ShellAssets>();
                (
                    assets.cube.clone(),
                    assets.stone.clone(),
                    assets.timber.clone(),
                    assets.plaster.clone(),
                )
            };
            // A footing under the walls, so a body on a slope has no gap
            // under its low side. It hangs from the root, not the frame:
            // it stays when the house is taken out of the way.
            world.spawn((
                Name::new("home footing"),
                ChildOf(root),
                Mesh3d(cube.clone()),
                MeshMaterial3d(stone),
                local
                    * Transform::from_xyz(0.0, -0.57, 0.0).with_scale(Vec3::new(
                        frame.width + 0.06,
                        1.2,
                        frame.depth + 0.06,
                    )),
            ));
            if inn {
                // A sign on a post at the corner, towards the door.
                let post = Vec3::new(frame.width * 0.5 - 0.25, 0.0, -frame.depth * 0.5 - 0.5);
                for (centre, size, material) in [
                    (
                        Vec3::new(0.0, 1.45, 0.0),
                        Vec3::new(0.14, 2.9, 0.14),
                        &timber,
                    ),
                    (
                        Vec3::new(-0.4, 2.78, 0.0),
                        Vec3::new(0.95, 0.09, 0.09),
                        &timber,
                    ),
                    (
                        Vec3::new(-0.52, 2.38, 0.0),
                        Vec3::new(0.62, 0.5, 0.07),
                        &timber,
                    ),
                    (
                        Vec3::new(-0.52, 2.38, 0.0),
                        Vec3::new(0.46, 0.34, 0.1),
                        &plaster,
                    ),
                ] {
                    block(world, frame_entity, material, &cube, post + centre, size);
                }
                // And a red flag on a pole at the other front corner,
                // streaming in the wind.
                if let Some(flag) = world.get_resource::<AssetServer>().map(|server| {
                    server.load(GltfAssetLabel::Scene(0).from_asset("living/props/flag_red.gltf"))
                }) {
                    world.spawn((
                        Name::new("inn flag"),
                        ChildOf(frame_entity),
                        WorldAssetRoot(flag),
                        crate::island_world::ambient::WindFlag { seed: 2.3 },
                        Transform::from_xyz(
                            -frame.width * 0.5 + 0.25,
                            0.0,
                            -frame.depth * 0.5 - 0.5,
                        )
                        .with_scale(Vec3::splat(8.0)),
                    ));
                }
            }
        }
        world.entity_mut(root).insert((
            HomeShell {
                building: id,
                frame,
                fit,
                frame_entity,
                local,
                model_entity,
                inn,
            },
            // Everything of the house but its footing hangs from the frame.
            // To the ridge: a line of sight past the chimney alone keeps
            // the house.
            Occluder {
                frame,
                height: fit.ridge(),
                drawn: frame_entity,
            },
        ));
        world.resource_mut::<Shells>().0.insert(id, root);
    }
}

/// The chest model for a homestead cottage whose resident has a chest.
fn homestead_chest(world: &World, building: SimId) -> Option<Handle<WorldAsset>> {
    let sim = world.get_resource::<SimIndex>()?.get(building)?;
    world.get::<Cottage>(sim)?;
    let residents = &world.get::<Household>(sim)?.residents;
    residents
        .iter()
        .any(|resident| view::chest(world, *resident).is_some())
        .then(|| world.get_resource::<crate::assets::ViewAssets>())
        .flatten()
        .map(|assets| assets.models.chest.clone())
}

/// Which beds a building shows: one per bed of a home, a child's marked;
/// a fixed number in an inn.
fn bed_kinds(world: &World, building: SimId, inn: bool) -> Vec<bool> {
    if inn {
        // One guest bed per bed the simulation rents.
        return view::inn(world, building).map_or_else(Vec::new, |inn| vec![false; inn.beds.len()]);
    }
    view::home(world, building).map_or_else(Vec::new, |home| {
        home.beds.iter().map(|bed| bed.child.is_some()).collect()
    })
}

fn spawn_interior(world: &mut World, shell: &HomeShell, ground: f32, beds: Vec<bool>) -> Open {
    let (frame, fit) = (shell.frame, shell.fit);
    let (width, depth) = (frame.width, frame.depth);
    let layout = model::interior(width, depth, fit.door_x(), beds.len());
    let interior = world
        .spawn((
            Name::new("home interior"),
            ChildOf(shell.frame_entity),
            Transform::default(),
            Visibility::default(),
        ))
        .id();
    let assets = world
        .remove_resource::<ShellAssets>()
        .expect("shell assets");
    let cube = assets.cube.clone();
    let floor_top = 0.08;
    block(
        world,
        interior,
        &assets.floor,
        &cube,
        Vec3::new(0.0, floor_top * 0.5, 0.0),
        Vec3::new(width - 0.04, floor_top, depth - 0.04),
    );
    let walls = Wall::ALL.map(|wall| {
        let standing = group(world, interior, true);
        let low = group(world, interior, false);
        for (parent, lowered) in [(standing, false), (low, true)] {
            for part in model::wall_parts(
                wall,
                width,
                depth,
                fit.door_x(),
                fit.door_opening(),
                lowered,
            ) {
                block(
                    world,
                    parent,
                    assets.tone(part.tone),
                    &cube,
                    part.centre,
                    part.size,
                );
            }
        }
        if wall == Wall::Front {
            // The door's leaf, standing open into the room.
            let (opening, tall) = (fit.door_opening(), model::DOOR_HEIGHT_M - 0.06);
            let hinge = Vec3::new(
                fit.door_x() - opening * 0.5,
                0.0,
                -depth * 0.5 + model::WALL_THICKNESS_M,
            );
            let swing = 100_f32.to_radians();
            world.spawn((
                ChildOf(standing),
                Mesh3d(cube.clone()),
                MeshMaterial3d(assets.timber.clone()),
                Transform::from_translation(
                    hinge
                        + Vec3::new(swing.cos(), 0.0, swing.sin()) * opening * 0.5
                        + Vec3::Y * (tall * 0.5 + floor_top),
                )
                .with_rotation(Quat::from_rotation_y(-swing))
                .with_scale(Vec3::new(opening, tall, 0.06)),
            ));
        }
        (standing, low)
    });

    // The hearth: a stone surround with a dark mouth, the fire in it and a
    // breast up to the wall's top.
    let hearth = layout.hearth;
    let mouth = hearth.lo().y;
    // A room too small for a hearth has a brazier where the layout puts it.
    let built = hearth.size.x > 0.0;
    let fire = if built {
        (
            Vec3::new(0.0, floor_top + 0.13, mouth - 0.06),
            Vec3::new(hearth.size.x - 0.56, 0.14, 0.2),
            Vec3::new(0.0, 1.05, mouth - 0.55),
        )
    } else {
        (
            Vec3::new(hearth.centre.x, floor_top + 0.27, hearth.centre.y),
            Vec3::new(0.2, 0.1, 0.2),
            Vec3::new(hearth.centre.x - 0.2, 0.9, hearth.centre.y + 0.2),
        )
    };
    if !built {
        block(
            world,
            interior,
            &assets.soot,
            &cube,
            Vec3::new(hearth.centre.x, floor_top + 0.11, hearth.centre.y),
            Vec3::new(0.26, 0.22, 0.26),
        );
    }
    for (index, (material, centre, size)) in [
        (
            &assets.stone,
            Vec3::new(0.0, 0.62, hearth.centre.y),
            Vec3::new(hearth.size.x, 1.24, hearth.size.y),
        ),
        (
            &assets.stone,
            Vec3::new(
                0.0,
                1.24 + (model::WALL_HEIGHT_M - 1.24) * 0.5,
                hearth.centre.y + 0.08,
            ),
            Vec3::new(
                hearth.size.x * 0.6,
                model::WALL_HEIGHT_M - 1.24,
                hearth.size.y - 0.16,
            ),
        ),
        (
            &assets.soot,
            Vec3::new(0.0, 0.5, mouth + 0.02),
            Vec3::new(hearth.size.x - 0.36, 0.74, 0.08),
        ),
        (
            &assets.stone,
            Vec3::new(0.0, floor_top + 0.03, mouth - 0.16),
            Vec3::new(hearth.size.x, 0.06, 0.32),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        // The chimney breast stands with the back wall: when that wall comes
        // down towards the camera, so does the breast, leaving the hearth's
        // surround at its own height.
        let parent = if index == 1 {
            walls[Wall::Back.index()].0
        } else {
            interior
        };
        if built {
            block(world, parent, material, &cube, centre, size);
        }
    }
    let ember = block(world, interior, &assets.ember_cold, &cube, fire.0, fire.1);
    // A home's furnishing round the fire: a timber mantel over the hearth,
    // firewood stacked beside it, a woven rug before it and a shelf of pots
    // on a side wall. The room reads as lived in, not a box of blocks.
    if built {
        block(
            world,
            interior,
            &assets.timber,
            &cube,
            Vec3::new(0.0, 1.27, hearth.centre.y - 0.04),
            Vec3::new(hearth.size.x + 0.16, 0.08, hearth.size.y + 0.1),
        );
        let side = hearth.size.x * 0.5 + 0.22;
        if side + 0.2 < width * 0.5 - model::WALL_THICKNESS_M {
            for (i, (x, y)) in [(-0.06, 0.07), (0.06, 0.07), (0.0, 0.19)]
                .into_iter()
                .enumerate()
            {
                block(
                    world,
                    interior,
                    if i == 2 {
                        &assets.timber
                    } else {
                        &assets.floor
                    },
                    &cube,
                    Vec3::new(side + x, floor_top + y, hearth.centre.y - 0.05),
                    Vec3::new(0.11, 0.11, 0.42),
                );
            }
        }
        let rug_z = (mouth - 0.95).max(-depth * 0.5 + 0.9);
        block(
            world,
            interior,
            &assets.rug,
            &cube,
            Vec3::new(0.0, floor_top + 0.01, rug_z),
            Vec3::new((hearth.size.x + 0.4).min(width - 1.2), 0.02, 1.0),
        );
    }
    // A shelf with pots on the wall away from the door.
    let wall_x = width * 0.5 - model::WALL_THICKNESS_M - 0.12;
    let shelf_x = if fit.door_x() > 0.0 { -wall_x } else { wall_x };
    if depth > 3.0 {
        block(
            world,
            walls[if shelf_x < 0.0 {
                Wall::Left.index()
            } else {
                Wall::Right.index()
            }]
            .0,
            &assets.timber,
            &cube,
            Vec3::new(shelf_x, 1.55, 0.0),
            Vec3::new(0.24, 0.05, 1.1),
        );
        for (i, z) in [-0.35_f32, 0.0, 0.32].into_iter().enumerate() {
            block(
                world,
                walls[if shelf_x < 0.0 {
                    Wall::Left.index()
                } else {
                    Wall::Right.index()
                }]
                .0,
                &assets.clay,
                &cube,
                Vec3::new(shelf_x, 1.66 + 0.02 * i as f32, z),
                Vec3::new(0.16, 0.18 + 0.04 * i as f32, 0.16),
            );
        }
    }
    let light = world
        .spawn((
            Name::new("hearth light"),
            ChildOf(interior),
            PointLight {
                color: Color::srgb(1.0, 0.6, 0.3),
                intensity: 0.0,
                range: 7.5,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(fire.2),
        ))
        .id();

    // The played character's chest (`view::chest`), once a storage tier
    // stands: in the front corner away from the door.
    if let Some(scene) = homestead_chest(world, shell.building) {
        let spot = model::chest_spot(&layout, fit.door_x());
        world.spawn((
            Name::new("homestead chest"),
            ChildOf(interior),
            WorldAssetRoot(scene),
            Transform::from_xyz(spot.centre.x, floor_top, spot.centre.y)
                .with_rotation(Quat::from_rotation_y(PI))
                .with_scale(Vec3::splat(0.6)),
        ));
    }

    // The table and its two stools, where there is room for them.
    let table = layout.table;
    let furnished = table.size.x > 0.0;
    if furnished {
        block(
            world,
            interior,
            &assets.timber,
            &cube,
            Vec3::new(table.centre.x, 0.74, table.centre.y),
            Vec3::new(table.size.x, 0.06, table.size.y),
        );
    }
    let legs: &[(f32, f32)] = if furnished {
        &[(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
    } else {
        &[]
    };
    for &(x, z) in legs {
        block(
            world,
            interior,
            &assets.timber,
            &cube,
            Vec3::new(
                table.centre.x + x * (table.size.x * 0.5 - 0.07),
                floor_top + 0.33,
                table.centre.y + z * (table.size.y * 0.5 - 0.07),
            ),
            Vec3::new(0.07, 0.66, 0.07),
        );
    }
    for stool in layout.stools.into_iter().filter(|_| furnished) {
        for (y, size) in [
            (floor_top + 0.2, Vec3::new(0.22, 0.4, 0.22)),
            (floor_top + 0.42, Vec3::new(0.34, 0.05, 0.34)),
        ] {
            block(
                world,
                interior,
                &assets.timber,
                &cube,
                Vec3::new(stool.x, y, stool.y),
                size,
            );
        }
    }

    // Beds, heads to the back wall.
    let mut sleepers = Vec::new();
    for (spot, child) in layout.beds.iter().zip(&beds) {
        let scale = spot.scale * if *child { model::CHILD_BED_SCALE } else { 1.0 };
        let length = model::BED_LENGTH_M * scale;
        let base = floor_top + spot.lift;
        // A child's bed stands at the head end of its place: the back wall,
        // or +X for a berth that lies across the room.
        let (middle, head, turn) = if spot.across {
            (
                Vec3::new(spot.rect.hi().x - length * 0.5, base, spot.rect.centre.y),
                Vec3::X,
                PI + FRAC_PI_2,
            )
        } else {
            (
                Vec3::new(spot.rect.centre.x, base, spot.rect.hi().y - length * 0.5),
                Vec3::Z,
                PI,
            )
        };
        // A size given as (width, height, length) of a bed, along its lie.
        let laid = |size: Vec3| {
            if spot.across {
                Vec3::new(size.z, size.y, size.x)
            } else {
                size
            }
        };
        match &assets.bed {
            Some(bed) => {
                world.spawn((
                    ChildOf(interior),
                    WorldAssetRoot(bed.clone()),
                    Transform::from_translation(middle)
                        .with_rotation(Quat::from_rotation_y(turn))
                        .with_scale(Vec3::splat(model::BED_MODEL_SCALE * scale)),
                ));
            }
            None => {
                block(
                    world,
                    interior,
                    &assets.blanket,
                    &cube,
                    middle + Vec3::Y * model::MATTRESS_HEIGHT_M * scale * 0.5,
                    laid(Vec3::new(
                        model::BED_WIDTH_M * scale,
                        model::MATTRESS_HEIGHT_M * scale,
                        length,
                    )),
                );
            }
        }
        if spot.lift > 0.0 {
            // An upper berth stands on a board at its head and at its foot.
            for end in [-1.0, 1.0] {
                block(
                    world,
                    interior,
                    &assets.timber,
                    &cube,
                    middle.with_y(floor_top + spot.lift * 0.5) + head * end * (length * 0.5 - 0.03),
                    laid(Vec3::new(model::BED_WIDTH_M * scale, spot.lift, 0.06)),
                );
            }
        }
        if *child {
            // A small sleeping figure: a mound under the blanket and a head
            // on the pillow.
            let figure = group(world, interior, false);
            let top = base + model::MATTRESS_HEIGHT_M * scale;
            block(
                world,
                figure,
                &assets.blanket,
                &cube,
                middle.with_y(top + 0.07) - head * length * 0.08,
                laid(Vec3::new(0.36, 0.17, length * 0.5)),
            );
            block(
                world,
                figure,
                &assets.linen,
                &cube,
                middle.with_y(top + 0.04) + head * length * 0.3,
                laid(Vec3::new(0.34, 0.06, 0.26)),
            );
            block(
                world,
                figure,
                &assets.skin,
                &assets.ball,
                middle.with_y(top + 0.13) + head * length * 0.3,
                Vec3::splat(0.22),
            );
            sleepers.push(figure);
        }
    }
    world.insert_resource(assets);
    Open {
        interior,
        ease: 0.0,
        walls,
        lowered: [false; 4],
        light,
        ember,
        sleepers,
        beds,
        heated: false,
        layout,
        frame,
        ground,
    }
}

fn show(world: &mut World, entity: Entity, on: bool) {
    let want = if on {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if let Some(mut visibility) = world.get_mut::<Visibility>(entity)
        && *visibility != want
    {
        *visibility = want;
    }
}

/// Where everybody inside one cut-away building is drawn.
fn seat_occupants(
    world: &World,
    building: SimId,
    inn: bool,
    open: &Open,
    seats: &mut BTreeMap<Entity, Seat>,
) {
    let Some(index) = world.get_resource::<SimIndex>() else {
        return;
    };
    let top = 0.08;
    let bed_seat = |bed: usize, child: bool| {
        let spot = open.layout.beds.get(bed)?;
        let scale = spot.scale * if child { model::CHILD_BED_SCALE } else { 1.0 };
        let centre = open.frame.to_world(
            Vec3::new(
                spot.rect.centre.x,
                top + spot.lift + model::MATTRESS_HEIGHT_M * scale,
                spot.rect.centre.y,
            ),
            open.ground,
        );
        Some(Seat::Bed {
            centre,
            // A berth across the room has its head at the body's +X.
            head_yaw: open.frame.yaw() + if spot.across { FRAC_PI_2 } else { 0.0 },
        })
    };
    let mut up = 0;
    let mut floor_seat = || {
        let spots = &open.layout.spots;
        let spot = spots[up % spots.len()];
        // More people than places: the next round stands a step aside.
        let round = (up / spots.len()) as f32;
        up += 1;
        Seat::Floor {
            at: open.frame.to_world(
                Vec3::new(spot.at.x + round * 0.3, top, spot.at.y - round * 0.3),
                open.ground,
            ),
            yaw: open.frame.yaw() + spot.yaw,
        }
    };
    let occupants = view::occupants(world, building);
    // Somebody asleep lies in their own bed. Anybody else who is in, resting
    // awake or up, is by the hearth, at the table or inside the door.
    let own_bed: BTreeMap<SimId, usize> = if inn {
        view::inn(world, building).map_or_else(BTreeMap::new, |inn| {
            inn.beds
                .iter()
                .enumerate()
                .filter_map(|(place, bed)| Some((bed.guest?, place)))
                .collect()
        })
    } else {
        view::home(world, building).map_or_else(BTreeMap::new, |home| {
            home.beds
                .iter()
                .enumerate()
                .filter_map(|(place, bed)| Some((bed.resident?, place)))
                .collect()
        })
    };
    for id in &occupants {
        let Some(entity) = index.get(*id) else {
            continue;
        };
        let seat = own_bed
            .get(id)
            .filter(|_| view::repose(world, entity) == Repose::Asleep)
            .and_then(|bed| bed_seat(*bed, false))
            .unwrap_or_else(&mut floor_seat);
        seats.insert(entity, seat);
    }
}

/// Open and close cutaways, and keep the open ones facing the camera, lit
/// and seated. Returns at once while nothing asks for one.
fn cutaways(
    world: &mut World,
    cameras: &mut QueryState<&RtsCamera>,
    shells: &mut QueryState<(&HomeShell, &Fogged, &Transform)>,
) {
    let camera = cameras.iter(world).next().copied();
    let mode = world.get_resource::<IslandMode>().copied();
    let living = mode == Some(IslandMode::Character);
    let selected = world
        .get_resource::<Selected>()
        .and_then(|selected| selected.0);
    let card = crate::island_build::card_subject(world);
    let played = living
        .then(|| {
            world
                .get_resource::<VillageState>()
                .map(|state| state.played)
        })
        .flatten();
    let close = !living && camera.is_some_and(|camera| camera.distance <= model::CLOSE_RELEASE_M);
    let blocking: Vec<SimId> = world
        .get_resource::<HomeBlocks>()
        .map(|blocks| blocks.0.keys().copied().collect())
        .unwrap_or_default();
    let followed = world
        .get_resource::<HomeBlocks>()
        .and_then(|blocks| blocks.1)
        .map(|at| Vec2::new(at.x, at.z));
    if world.resource::<Cutaways>().open.is_empty()
        && world.resource::<Cutaways>().rising.is_empty()
        && selected.is_none()
        && card.is_none()
        && played.is_none()
        && blocking.is_empty()
        && !close
    {
        return;
    }
    let delta = world
        .get_resource::<Time>()
        .map_or(0.0, |time| time.delta_secs());
    let step = delta / model::CUTAWAY_EASE_SECS;
    // Every building with an inside, and whether the fog shows it live.
    let known: BTreeMap<SimId, (HomeShell, bool, f32)> = shells
        .iter(world)
        .map(|(shell, fog, root)| {
            (
                shell.building,
                (shell.clone(), fog.0 == Look::Live, root.translation.y),
            )
        })
        .collect();
    let inside = |id: Option<SimId>| {
        id.and_then(|id| {
            if known.contains_key(&id) {
                Some(id)
            } else {
                view::inside(world, id)
            }
        })
    };
    let held = world.resource::<Cutaways>().near;
    let near = camera.filter(|_| close).and_then(|camera| {
        model::near_home(
            camera.distance,
            Vec2::new(camera.focus.x, camera.focus.z),
            held,
            known.iter().map(|(id, (shell, ..))| (*id, shell.frame)),
        )
    });
    let wanted = model::wanted(
        model::Signals {
            card,
            selected: inside(selected),
            played: inside(played),
            near,
            blocking: [blocking.first().copied(), blocking.get(1).copied()],
        },
        |id| known.get(&id).is_some_and(|(_, live, _)| *live),
    );
    if !shell_assets(world) {
        return;
    }
    let mut state = world.remove_resource::<Cutaways>().expect("cutaway state");
    state.near = near;
    let tick = world.get_resource::<SimClock>().map(|clock| clock.tick);
    let mut reseat = state.tick != tick;
    state.tick = tick;

    // Close what is no longer asked for, and what changed its beds.
    let stale: Vec<SimId> = state
        .open
        .iter()
        .filter(|(id, open)| {
            !wanted.contains(id)
                || (reseat
                    && known
                        .get(id)
                        .is_some_and(|(shell, ..)| bed_kinds(world, **id, shell.inn) != open.beds))
        })
        .map(|(id, _)| *id)
        .collect();
    for id in stale {
        if let Some(open) = state.open.remove(&id) {
            if world.get_entity(open.interior).is_ok() {
                world.entity_mut(open.interior).despawn();
            }
            if let Some((shell, ..)) = known.get(&id)
                && let Some(model) = shell.model_entity
            {
                // The roof and walls rise again from where they had sunk.
                show(world, model, true);
                state
                    .rising
                    .insert(id, (model, 1.0 - open.ease, shell.fit.scale.y));
            }
            reseat = true;
        }
    }
    // Models rising after a cutaway closed: up over the ease, then done.
    let mut risen = Vec::new();
    for (id, (model, up, base)) in &mut state.rising {
        if wanted.contains(id) {
            continue;
        }
        *up = (*up + step).min(1.0);
        if let Some(mut transform) = world.get_mut::<Transform>(*model) {
            transform.scale.y = *base * model::squash(1.0 - *up);
        }
        if *up >= 1.0 {
            risen.push(*id);
        }
    }
    for id in risen {
        state.rising.remove(&id);
    }
    for id in &wanted {
        if state.open.contains_key(id) {
            continue;
        }
        let Some((shell, _, ground)) = known.get(id) else {
            continue;
        };
        let beds = bed_kinds(world, *id, shell.inn);
        let mut open = spawn_interior(world, shell, *ground, beds);
        // A model still rising from a cutaway that just closed sinks from
        // where it is: one state, never two.
        if let Some((_, up, _)) = state.rising.remove(id) {
            open.ease = 1.0 - up;
        }
        state.open.insert(*id, open);
        reseat = true;
    }

    // The walls on the camera's side come down; the hearth lights the room.
    let lamp = world
        .get_resource::<SimClock>()
        .zip(world.get_resource::<SimConfig>())
        .map_or(0.0, |(clock, config)| {
            lamp_level(view_daylight(
                clock.tick,
                config,
                world.get_resource::<ViewDayOverride>(),
            ))
        });
    let night = world
        .get_resource::<SimClock>()
        .zip(world.get_resource::<SimConfig>())
        .is_some_and(|(clock, config)| config.is_night(clock.tick));
    let (lit, cold) = {
        let assets = world.resource::<ShellAssets>();
        (assets.ember_lit.clone(), assets.ember_cold.clone())
    };
    for (id, open) in &mut state.open {
        let Some((shell, ..)) = known.get(id) else {
            continue;
        };
        // The roof and the walls sink away over the ease, uncovering the
        // room; then the model is hidden.
        if let Some(model) = shell.model_entity
            && open.ease < 1.0
        {
            open.ease = (open.ease + step).min(1.0);
            if let Some(mut transform) = world.get_mut::<Transform>(model) {
                transform.scale.y = shell.fit.scale.y * model::squash(open.ease);
            }
            if open.ease >= 1.0 {
                show(world, model, false);
            }
        }
        if let Some(camera) = camera {
            for wall in Wall::ALL {
                let was = open.lowered[wall.index()];
                let down = model::wall_lowered(wall, &open.frame, camera.yaw, was)
                    || (blocking.contains(id)
                        && followed.is_some_and(|at| model::wall_hides(wall, &open.frame, at)));
                open.lowered[wall.index()] = down;
                let (standing, low) = open.walls[wall.index()];
                show(world, standing, !down);
                show(world, low, down);
            }
        }
        if reseat {
            // An inn keeps its fire in; a home's is the simulation's hearth.
            open.heated = shell.inn || view::home(world, *id).is_some_and(|home| home.heated);
        }
        let heated = open.heated;
        // The fire flickers gently on the view clock.
        let flame = world
            .get_resource::<crate::island_world::ambient::Wind>()
            .map_or(1.0, |wind| {
                crate::island_world::ambient::flicker(wind.time, (id.0 % 97) as f32)
            });
        let lumens = model::hearth_lumens(heated, lamp) * flame;
        if let Some(mut light) = world.get_mut::<PointLight>(open.light)
            && light.intensity != lumens
        {
            light.intensity = lumens;
        }
        let want = if heated { &lit } else { &cold };
        if let Some(mut material) = world.get_mut::<MeshMaterial3d<StandardMaterial>>(open.ember)
            && material.0 != *want
        {
            material.0 = want.clone();
        }
        for sleeper in open.sleepers.clone() {
            show(world, sleeper, night);
        }
    }
    if reseat {
        state.seats.clear();
        for (id, open) in &state.open {
            let inn = known.get(id).is_some_and(|(shell, ..)| shell.inn);
            seat_occupants(world, *id, inn, open, &mut state.seats);
        }
    }
    world.insert_resource(state);
}

/// The fog pass shows every resident it can see again on each tick. Whoever
/// the last frame had hidden indoors is hidden again before anything in the
/// present pass (markers, captions) looks at who is drawn; [`residents`]
/// then decides this frame.
fn keep_hidden(
    views: Res<IndoorViews>,
    index: Res<VisualIndex>,
    mut roots: Query<&mut Visibility, With<ActorVisual>>,
) {
    for (entity, track) in &views.0 {
        if track.hidden
            && let Some(mut visibility) = index
                .map
                .get(entity)
                .and_then(|root| roots.get_mut(*root).ok())
            && *visibility != Visibility::Hidden
        {
            *visibility = Visibility::Hidden;
        }
    }
}

/// Where a body lying in a bed puts its root, and how it is turned.
fn lying(centre: Vec3, head_yaw: f32, civilian: bool) -> (Vec3, Quat) {
    let head = Vec3::new(head_yaw.sin(), 0.0, head_yaw.cos());
    if civilian {
        // No lying clip: the standing body is laid on its back, feet at the
        // foot of the bed.
        (
            centre - head * 0.86 + Vec3::Y * 0.14,
            Quat::from_rotation_y(head_yaw + PI) * Quat::from_rotation_x(-FRAC_PI_2),
        )
    } else {
        // `Lie_Idle` lies back from the root: the head ends up behind it.
        (
            centre + head * 0.1 + Vec3::Y * 0.02,
            Quat::from_rotation_y(head_yaw + PI),
        )
    }
}

/// Draw each resident where the simulation has them: outdoors as before,
/// stepping through the doorway, hidden indoors, or in a cut-away home.
#[allow(clippy::too_many_arguments)]
fn residents(
    time: Res<Time>,
    selected: Option<Res<Selected>>,
    mode: Option<Res<IslandMode>>,
    village: Option<Res<VillageState>>,
    index: Res<VisualIndex>,
    known: Res<Shells>,
    open: Res<Cutaways>,
    people: Query<(Entity, &SimId, &Indoors), Without<AwaitingArrival>>,
    shells: Query<(&HomeShell, &Transform), Without<ActorVisual>>,
    mut roots: Query<(&mut Transform, &mut Visibility, &Fogged, &ActorVisual)>,
    mut views: ResMut<IndoorViews>,
    mut stepping: ResMut<Stepping>,
) {
    let delta = time.delta_secs();
    let mut through: BTreeSet<SimId> = BTreeSet::new();
    let played = (mode.as_deref() == Some(&IslandMode::Character))
        .then(|| village.as_deref().map(|state| state.played))
        .flatten();
    let chosen = selected.as_deref().and_then(|selected| selected.0);
    // Somebody who left the island or whose body was taken away a day after
    // their death is no longer tracked: their visual goes with the entity.
    if views.0.keys().any(|entity| !people.contains(*entity)) {
        views.0.retain(|entity, _| people.contains(*entity));
    }
    for (entity, id, indoors) in &people {
        let inside = indoors.building.is_some();
        let track = views.0.entry(entity).or_insert_with(|| Track {
            phase: Phase::first(inside),
            building: indoors.building,
            hidden: false,
        });
        let before = track.phase;
        track.phase = before.advance(inside, delta);
        if inside {
            track.building = indoors.building;
        }
        if track.phase == Phase::Outside && before == Phase::Outside && !track.hidden {
            continue;
        }
        // Somebody on the way in or out: that door stands open.
        if matches!(track.phase, Phase::Entering(_) | Phase::Leaving(_))
            && let Some(building) = track.building
        {
            through.insert(building);
        }
        let Some((mut transform, mut visibility, fog, actor)) = index
            .map
            .get(&entity)
            .and_then(|root| roots.get_mut(*root).ok())
        else {
            continue;
        };
        let shell = track
            .building
            .and_then(|building| known.0.get(&building))
            .and_then(|root| shells.get(*root).ok());
        let cut_away = track
            .building
            .is_some_and(|building| open.is_open(building));
        let focused = chosen == Some(*id) || played == Some(*id);
        let mut look = model::drawn(track.phase, shell.is_some(), cut_away, focused);
        let seat = open.seats.get(&entity);
        if look == Drawn::Indoors && seat.is_none() {
            look = Drawn::Hidden;
        }
        let mut hide = false;
        match (look, shell) {
            (Drawn::Step(along), Some((shell, root))) => {
                let (frame, fit) = (&shell.frame, &shell.fit);
                let doorway = frame.to_world(
                    Vec3::new(fit.door_x(), 0.0, -frame.depth * 0.5 + 0.1),
                    root.translation.y,
                );
                transform.translation = transform.translation.lerp(doorway, along);
                if matches!(track.phase, Phase::Entering(_)) {
                    // Upright and facing the door, whatever the rest that
                    // has begun would lay them down as.
                    transform.rotation = Quat::from_rotation_y(frame.yaw());
                }
            }
            (Drawn::Hidden, _) => hide = true,
            (Drawn::Indoors, _) => match seat.copied() {
                Some(Seat::Bed { centre, head_yaw }) => {
                    // Asleep: both rigs lie down (`island_poses`).
                    let (at, turn) = lying(centre, head_yaw, actor.kind.civilian_rig());
                    transform.translation = at;
                    transform.rotation = turn;
                }
                Some(Seat::Floor { at, yaw }) => {
                    transform.translation = at;
                    transform.rotation = Quat::from_rotation_y(yaw);
                }
                None => hide = true,
            },
            _ => {}
        }
        if hide {
            // Out of reach of a click as well as out of sight.
            transform.translation.y -= 60.0;
        }
        track.hidden = hide;
        let want = if hide || !fog.0.shown() {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *visibility != want {
            *visibility = want;
        }
    }
    if stepping.0 != through {
        stepping.0 = through;
    }
}

/// Take a building or a tree out of the way when it stands between the
/// camera and the played character or, at a close camera, the selected
/// resident. It is squashed down to its footing over
/// [`model::UNBLOCK_EASE_SECS`] and then hidden, as a cut-away home loses its
/// roof, but its inside is not opened; its smoke and window lights go with
/// it. It rises again a moment after it stops blocking. Returns at once
/// while there is nobody to keep in view and nothing is out of the way.
#[allow(clippy::too_many_arguments)]
fn unblock(
    time: Res<Time>,
    mode: Option<Res<IslandMode>>,
    village: Option<Res<VillageState>>,
    selected: Option<Res<Selected>>,
    sims: Option<Res<SimIndex>>,
    index: Res<VisualIndex>,
    open: Res<Cutaways>,
    rigs: Query<&RtsCamera>,
    eyes: Query<&GlobalTransform, With<Camera3d>>,
    people: Query<&Indoors>,
    actors: Query<&Transform, With<ActorVisual>>,
    buildings: Query<
        (Entity, &Occluder, &Transform, &StaticId, Has<HomeShell>),
        Without<ActorVisual>,
    >,
    canopies: Query<(Entity, &Canopy, &Transform), (Without<ActorVisual>, Without<Occluder>)>,
    mut drawn: Query<
        (&mut Visibility, &mut Transform),
        (Without<ActorVisual>, Without<Occluder>, Without<Canopy>),
    >,
    (mut state, mut homes): (ResMut<Unblocked>, ResMut<HomeBlocks>),
    mut locals: Local<UnblockLocals>,
) {
    let UnblockLocals { report, memo } = &mut *locals;
    let living = mode.as_deref() == Some(&IslandMode::Character);
    let followed = if living {
        village.as_deref().map(|state| state.played)
    } else {
        selected
            .as_deref()
            .and_then(|selected| selected.0)
            .filter(|_| {
                rigs.iter()
                    .next()
                    .is_some_and(|camera| camera.distance <= model::UNBLOCK_DISTANCE_M)
            })
    };
    // Somebody outdoors: whoever is indoors is the business of the cutaway.
    let target = followed
        .zip(sims.as_deref())
        .and_then(|(id, sims)| sims.get(id))
        .filter(|sim| {
            people
                .get(*sim)
                .map_or(true, |indoors| indoors.building.is_none())
        })
        .and_then(|sim| index.map.get(&sim))
        .and_then(|root| actors.get(*root).ok())
        .map(|root| root.translation + Vec3::Y * 1.2);
    if target.is_none() && state.0.is_empty() && homes.0.is_empty() {
        return;
    }
    let delta = time.delta_secs();
    if let Some(target) = target {
        homes.1 = Some(target);
    }
    for idle in homes.0.values_mut() {
        *idle += delta;
    }
    homes.0.retain(|_, idle| *idle < model::UNBLOCK_LINGER_SECS);
    let mut cut_now = BTreeSet::new();
    for cut in state.0.values_mut() {
        cut.idle += delta;
        cut_now.insert(cut.drawn);
    }
    let still = |(eye, target): (Vec3, Vec3)| {
        memo.as_ref().is_some_and(|(was_eye, was_target, _)| {
            was_eye.distance_squared(eye) < 1e-4 && was_target.distance_squared(target) < 1e-4
        })
    };
    let sight = eyes
        .iter()
        .next()
        .map(GlobalTransform::translation)
        .zip(target);
    if let Some(found) = sight
        .filter(|sight| still(*sight))
        .and_then(|_| memo.as_ref().map(|(.., found)| found.clone()))
    {
        // Neither the camera nor the character has moved: the same things
        // block the view, without testing every tree again.
        for (root, entity) in found {
            if let Ok((.., id, true)) = buildings.get(root) {
                homes.0.insert(id.0, 0.0);
                continue;
            }
            state
                .0
                .entry(root)
                .and_modify(|cut| cut.idle = 0.0)
                .or_insert(Cut {
                    drawn: entity,
                    idle: 0.0,
                    cut: 0.0,
                    base_y: None,
                });
        }
    } else if let Some((target, eye)) = sight.map(|(eye, target)| (target, eye)) {
        let mut found: Vec<(Entity, Entity)> = Vec::new();
        let mut blocking = |root: Entity, entity: Entity| {
            found.push((root, entity));
            state
                .0
                .entry(root)
                .and_modify(|cut| cut.idle = 0.0)
                .or_insert(Cut {
                    drawn: entity,
                    idle: 0.0,
                    cut: 0.0,
                    base_y: None,
                });
        };
        let standing = |entity: Entity| {
            drawn
                .get(entity)
                .is_ok_and(|(visibility, _)| *visibility != Visibility::Hidden)
        };
        let mut home_hits = Vec::new();
        for (root, occluder, at, id, home) in &buildings {
            // A home is tested whether or not it is cut away already: its
            // cutaway is how it stays out of the way.
            if (home || !open.is_open(id.0))
                && model::blocks(
                    &occluder.frame,
                    at.translation.y,
                    occluder.height,
                    eye,
                    target,
                )
            {
                if home {
                    home_hits.push((root, occluder.drawn, id.0));
                } else {
                    blocking(root, occluder.drawn);
                }
            }
        }
        // Trees only near the line of sight: a cheap box test first.
        let (lo, hi) = (eye.min(target), eye.max(target));
        let canopy_cut = |entity: Entity| cut_now.contains(&entity);
        for (root, canopy, at) in &canopies {
            let p = at.translation;
            let reach = canopy.shape.reach() + model::TREE_MARGIN_M;
            if p.x < lo.x - reach || p.x > hi.x + reach || p.z < lo.z - reach || p.z > hi.z + reach
            {
                continue;
            }
            // A tree already cleared for a building stays cleared.
            if (standing(canopy.drawn) || canopy_cut(canopy.drawn))
                && model::blocks_tree(p, &canopy.shape, eye, target)
            {
                blocking(root, canopy.drawn);
            }
        }
        for (root, drawn_entity, id) in home_hits {
            found.push((root, drawn_entity));
            homes.0.insert(id, 0.0);
        }
        *memo = Some((eye, target, found));
    }
    let step = delta / model::UNBLOCK_EASE_SECS;
    state.0.retain(|root, cut| {
        // A home that has been cut away meanwhile shows itself.
        let cut_away = buildings
            .get(*root)
            .is_ok_and(|(.., id, _)| open.is_open(id.0));
        let away = cut.idle < model::UNBLOCK_LINGER_SECS && !cut_away;
        cut.cut = if away {
            (cut.cut + step).min(1.0)
        } else {
            (cut.cut - step).max(0.0)
        };
        let Ok((mut visibility, mut transform)) = drawn.get_mut(cut.drawn) else {
            return false;
        };
        let base = *cut.base_y.get_or_insert(transform.scale.y);
        let keep = away || cut.cut > 0.0;
        let (want, height) = if !keep {
            (Visibility::Inherited, base)
        } else if cut.cut >= 1.0 {
            (Visibility::Hidden, base * model::squash(1.0))
        } else {
            (Visibility::Inherited, base * model::squash(cut.cut))
        };
        if *visibility != want {
            *visibility = want;
        }
        if transform.scale.y != height {
            transform.scale.y = height;
        }
        keep
    });
    // Review aid (`ISLAND_VIEW_STATS=1`): a line whenever the number of
    // buildings and trees out of the way changes, for capture scripts.
    let stats = *report
        .0
        .get_or_insert_with(|| std::env::var_os("ISLAND_VIEW_STATS").is_some());
    if stats && report.1 != state.0.len() {
        report.1 = state.0.len();
        let trees = state
            .0
            .keys()
            .filter(|root| canopies.contains(**root))
            .count();
        info!(
            "island unblock: {} out of the way ({} trees)",
            state.0.len(),
            trees
        );
    }
}

/// Homes somebody is stepping into or out of this frame.
#[derive(Resource, Default)]
struct Stepping(BTreeSet<SimId>);

/// Seconds a door stays open after the last step through it.
#[derive(Resource)]
struct DoorHold(f32);

/// Doors drawn open: the entity and how long since anybody used the door.
#[derive(Resource, Default)]
pub(crate) struct OpenDoors(BTreeMap<SimId, (Entity, f32)>);

impl OpenDoors {
    /// The drawn open doors (their leaf entities), for the sound of a door.
    pub(crate) fn leaves(&self) -> impl Iterator<Item = Entity> + '_ {
        self.0.values().map(|(leaf, _)| *leaf)
    }
}

/// A closed house's door is the model's painted, shut one. While somebody
/// steps in or out, and for a moment after, a dark opening and a leaf swung
/// outwards are drawn over it. Nothing exists for a door nobody uses.
#[allow(clippy::too_many_arguments)]
fn doors(
    mut commands: Commands,
    time: Res<Time>,
    stepping: Res<Stepping>,
    known: Res<Shells>,
    cut_away: Res<Cutaways>,
    assets: Option<Res<ShellAssets>>,
    hold: Res<DoorHold>,
    shells: Query<&HomeShell>,
    mut open: ResMut<OpenDoors>,
) {
    if stepping.0.is_empty() && open.0.is_empty() {
        return;
    }
    let Some(assets) = assets else { return };
    let delta = time.delta_secs();
    // A door nobody has used for a moment, or of a home now cut away
    // (whose own door stands open), is shut again.
    open.0.retain(|building, (entity, idle)| {
        *idle = if stepping.0.contains(building) {
            0.0
        } else {
            *idle + delta
        };
        let keep = *idle < hold.0 && !cut_away.is_open(*building);
        if !keep {
            commands.entity(*entity).try_despawn();
        }
        keep
    });
    for building in &stepping.0 {
        if open.0.contains_key(building) || cut_away.is_open(*building) {
            continue;
        }
        let Some(shell) = known
            .0
            .get(building)
            .and_then(|root| shells.get(*root).ok())
        else {
            continue;
        };
        let door = commands
            .spawn((
                Name::new("open door"),
                ChildOf(shell.frame_entity),
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        for part in model::door_parts(&shell.fit, shell.frame.depth) {
            commands.spawn((
                ChildOf(door),
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(match part.tone {
                    Tone::Dark => assets.soot.clone(),
                    tone => assets.tone(tone).clone(),
                }),
                Transform::from_translation(part.centre)
                    .with_rotation(Quat::from_rotation_y(part.turn))
                    .with_scale(part.size),
                bevy::light::NotShadowCaster,
            ));
        }
        open.0.insert(*building, (door, 0.0));
    }
}
