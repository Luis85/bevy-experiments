//! What the village builds, drawn: construction sites that fill in as
//! materials arrive and work proceeds, the homes and workplaces residents
//! finish, stores with their stock and market stalls with goods on display.
//!
//! One visual root per site or building, linked by `VisualOf`. A root is
//! spawned when the civic index gains an entry, replaced when a site becomes
//! its building, and despawned when the entry goes: nothing is rebuilt per
//! frame. Progress and stock change a few child transforms when the numbers
//! change. Homes and workplaces use the same models and the same layout
//! around their root as the founding ones, so window light, chimney smoke,
//! forge embers and the settlement's roof colour reach them unchanged.
use super::BuildTool;
use super::model::{self, Tint};
use crate::colony_scene::living_scene::{self, BuildingModel};
use crate::fog::{FogDim, Fogged, Look};
use crate::island_atmosphere::goods::{self, PileKind};
use crate::island_atmosphere::palette;
use crate::village_world::StaticId;
use bevy::{prelude::*, world_serialization::WorldAsset};
use island_content::village::{
    Role, VillagePlace, civic,
    colony::Household,
    frontier::{
        economy_spec::EconomyRegistry,
        settlements::{HOME_OFFSET_MM, WORKPLACE_OFFSET_MM},
        terrain::TerrainState,
        world_spec::LivingWorldSpec,
    },
};
use npc_framework::prelude::{SimId, SimIndex};
use npc_framework::view_helpers::{ViewSystems, VisualIndex, VisualOf};
use std::collections::BTreeMap;

/// Height of a solid building's frame while it is being raised, metres.
const FRAME_HEIGHT: f32 = 2.6;
/// The same for an open yard.
const YARD_HEIGHT: f32 = 1.1;
/// Props beside a site and in front of a barn or a stall.
const PILE_SLOTS: usize = 6;
/// Props inside an open store.
const YARD_SLOTS: usize = 9;

#[derive(Resource)]
struct CivicAssets {
    cube: Handle<Mesh>,
    wood: Handle<StandardMaterial>,
    plank: Handle<StandardMaterial>,
    soil: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    ghost: [Handle<StandardMaterial>; 2],
    roofs: BTreeMap<palette::RoofPalette, Handle<StandardMaterial>>,
    market: Handle<WorldAsset>,
    lumber: Handle<WorldAsset>,
    crate_model: Handle<WorldAsset>,
    sack: Handle<WorldAsset>,
    barrel: Handle<WorldAsset>,
    /// The kit's building stages: foundation, frame, scaffolded walls.
    stages: [Handle<WorldAsset>; 3],
    /// The yellow flag a civic building flies.
    flag: Handle<WorldAsset>,
}

impl CivicAssets {
    fn prop(&self, kind: PileKind) -> Handle<WorldAsset> {
        match kind {
            PileKind::Lumber => self.lumber.clone(),
            PileKind::Crate => self.crate_model.clone(),
            PileKind::Sack => self.sack.clone(),
            PileKind::Barrel => self.barrel.clone(),
        }
    }
}

/// What a root was built from. A different signature means a different
/// drawing: most often a site that has become its building.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Signature {
    built: bool,
    building: String,
    plot: civic::Plot,
}

#[derive(Component)]
struct CivicVisual(Signature);

#[derive(Resource, Default)]
struct CivicVisuals {
    /// Site or building → its root.
    map: BTreeMap<SimId, Entity>,
}

/// The parts of a site that change with deliveries and work.
#[derive(Component)]
struct SiteVisual {
    source: SimId,
    frame: Entity,
    walls: Entity,
    piles: Vec<Entity>,
    height: f32,
    /// The kit's stage models, one shown at a time by the work done.
    stages: [Entity; 3],
    observed: Option<(Vec<(PileKind, u32)>, u32, bool)>,
}

/// The kit's stage model widths across, model units.
const STAGE_WIDTH_UNITS: [f32; 3] = [1.047, 1.021, 1.16];

/// Which building stage a site shows for the share of its work done:
/// none before work starts, then the foundation, the frame and the
/// scaffolded walls by thirds.
pub(crate) fn stage_for(share: f32, started: bool) -> Option<usize> {
    if !started {
        return None;
    }
    Some(if share < 1.0 / 3.0 {
        0
    } else if share < 2.0 / 3.0 {
        1
    } else {
        2
    })
}

/// How an open site is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SiteLook {
    /// A building's site: earth bed, doorway, scaffold and the kit's
    /// stages rising inside.
    Raised,
    /// A prop's or a field's site: stakes and a rail round the footprint
    /// and the delivered materials in front; no scaffold. The finished
    /// prop (`island_world::props`) or field (`island_world::fields`) takes
    /// its place when the site finishes.
    Staked,
}

/// The look of a site: staked for a prop or a field (or garden) plot,
/// raised for every other building.
pub(crate) fn site_look(prop: bool, field: bool) -> SiteLook {
    if prop || field {
        SiteLook::Staked
    } else {
        SiteLook::Raised
    }
}

/// The stock shown at a store or a stall.
#[derive(Component)]
struct DepotVisual {
    source: SimId,
    slots: Vec<Entity>,
    observed: Option<(u32, Vec<(PileKind, u32)>)>,
}

/// One place a prop can stand. The prop is swapped when the good changes.
#[derive(Component)]
struct Slot {
    shown: Option<PileKind>,
    /// Size relative to a workplace pile's prop.
    factor: f32,
}

/// The two meshes of the placement ghost: the body's volume and a marker
/// that stands on the entrance cell.
#[derive(Component)]
struct GhostPart {
    entrance: bool,
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<CivicVisuals>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            sync.in_set(ViewSystems::Sync)
                .after(crate::visuals::despawn_visuals),
        )
        .add_systems(
            Update,
            (site_progress, depot_stock)
                .in_set(ViewSystems::Animate)
                .after(crate::fog::FogSet),
        )
        .add_systems(
            Update,
            ghost_box.in_set(ViewSystems::Present).after(super::process),
        );
}

fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let mut matte = |color: Color| {
        materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: 0.92,
            reflectance: 0.08,
            ..default()
        })
    };
    let wood = matte(Color::srgb(0.42, 0.29, 0.17));
    let plank = matte(Color::srgb(0.71, 0.55, 0.34));
    let soil = matte(Color::srgb(0.45, 0.36, 0.24));
    let dark = matte(Color::srgb(0.16, 0.12, 0.09));
    let roofs = [
        palette::RoofPalette::Blue,
        palette::RoofPalette::Moss,
        palette::RoofPalette::Terracotta,
        palette::RoofPalette::Ochre,
        palette::RoofPalette::Plum,
    ]
    .into_iter()
    .map(|roof| (roof, matte(palette::roof_colour(roof))))
    .collect();
    let mut glass = |color: Color| {
        materials.add(StandardMaterial {
            base_color: color,
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })
    };
    let ghost = [
        glass(Color::srgba(0.36, 0.92, 0.46, 0.30)),
        glass(Color::srgba(1.0, 0.24, 0.18, 0.46)),
    ];
    let ghost_entrance = glass(Color::srgba(0.98, 0.80, 0.32, 0.62));
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    for entrance in [false, true] {
        commands.spawn((
            Name::new(if entrance {
                "placement ghost entrance"
            } else {
                "placement ghost"
            }),
            GhostPart { entrance },
            Mesh3d(cube.clone()),
            MeshMaterial3d(if entrance {
                ghost_entrance.clone()
            } else {
                ghost[0].clone()
            }),
            Transform::default(),
            Visibility::Hidden,
            bevy::light::NotShadowCaster,
            bevy::light::NotShadowReceiver,
        ));
    }
    commands.insert_resource(CivicAssets {
        cube,
        wood,
        plank,
        soil,
        dark,
        ghost,
        roofs,
        market: living_scene::scene(&server, "living/buildings/building_market_blue.gltf"),
        lumber: living_scene::scene(&server, "hex/resource_lumber.gltf"),
        crate_model: living_scene::scene(&server, "hex/crate_A_big.gltf"),
        sack: living_scene::scene(&server, "hex/sack.gltf"),
        barrel: living_scene::scene(&server, "hex/barrel.gltf"),
        stages: [
            living_scene::scene(&server, "living/props/building_stage_A.gltf"),
            living_scene::scene(&server, "living/props/building_stage_B.gltf"),
            living_scene::scene(&server, "living/props/building_stage_C.gltf"),
        ],
        flag: living_scene::scene(&server, "living/props/flag_yellow.gltf"),
    });
}

/// Where a plot's body stands, in world metres on the ground plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    /// Centre of the body.
    pub(crate) centre: Vec3,
    /// From the entrance towards the body.
    pub(crate) dir: Vec3,
    /// Across the entrance, and behind it.
    pub(crate) width: f32,
    pub(crate) depth: f32,
    /// Centre of the entrance cell.
    pub(crate) door: Vec3,
}

pub(crate) fn frame(plot: &civic::Plot) -> Frame {
    let dir = match plot.rotation % 4 {
        0 => Vec3::Z,
        1 => Vec3::X,
        2 => Vec3::NEG_Z,
        _ => Vec3::NEG_X,
    };
    let (size_x, size_z) = (
        (plot.hi_x - plot.lo_x + 1) as f32,
        (plot.hi_z - plot.lo_z + 1) as f32,
    );
    let (width, depth) = if plot.rotation.is_multiple_of(2) {
        (size_x, size_z)
    } else {
        (size_z, size_x)
    };
    Frame {
        centre: Vec3::new(
            (plot.lo_x + plot.hi_x + 1) as f32 * 0.5,
            0.0,
            (plot.lo_z + plot.hi_z + 1) as f32 * 0.5,
        ),
        dir,
        width,
        depth,
        door: Vec3::new(plot.x as f32 + 0.5, 0.0, plot.z as f32 + 0.5),
    }
}

/// The turn about the vertical that points a root's local +Z along `dir`.
pub(crate) fn yaw_to(dir: Vec3) -> f32 {
    dir.x.atan2(dir.z)
}

impl Frame {
    /// A root at the body's centre whose local +Z runs from the entrance
    /// into the body: sites, stores and stalls are laid out in this frame.
    fn centred(&self) -> Transform {
        Transform::from_translation(self.centre)
            .with_rotation(Quat::from_rotation_y(yaw_to(self.dir)))
    }
    /// The entrance in the centred root's own coordinates.
    fn door_local(&self) -> Vec3 {
        self.centred().rotation.inverse() * (self.door - self.centre)
    }
    /// A home's root: the founding homes' layout, turned so the model, placed
    /// at the founding offset, stands on the body's centre.
    pub(crate) fn home_root(&self) -> Transform {
        let reach = (HOME_OFFSET_MM as f32 / 1000.0).abs();
        Transform::from_translation(self.centre - self.dir * reach)
            .with_rotation(Quat::from_rotation_y(yaw_to(-self.dir)))
    }
    /// A workplace's root, likewise for the founding workplaces' layout.
    pub(crate) fn workplace_root(&self) -> Transform {
        let reach = (WORKPLACE_OFFSET_MM as f32 / 1000.0).abs();
        Transform::from_translation(self.centre - self.dir * reach)
            .with_rotation(Quat::from_rotation_y(yaw_to(self.dir)))
    }
}

fn ground(terrain: &TerrainState, at: Vec3) -> f32 {
    terrain
        .cell(at.x.floor() as i32, at.z.floor() as i32)
        .map_or(0.0, |cell| cell.height_mm.max(0) as f32 / 1000.0)
}

fn block(
    commands: &mut Commands,
    parent: Entity,
    assets: &CivicAssets,
    material: &Handle<StandardMaterial>,
    at: Vec3,
    size: Vec3,
) -> Entity {
    commands
        .spawn((
            ChildOf(parent),
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(at).with_scale(size),
        ))
        .id()
}

fn group(commands: &mut Commands, parent: Entity, visibility: Visibility) -> Entity {
    commands
        .spawn((ChildOf(parent), Transform::default(), visibility))
        .id()
}

fn slots(
    commands: &mut Commands,
    parent: Entity,
    places: impl IntoIterator<Item = Vec3>,
    factor: f32,
) -> Vec<Entity> {
    places
        .into_iter()
        .enumerate()
        .map(|(index, at)| {
            commands
                .spawn((
                    ChildOf(parent),
                    Slot {
                        shown: None,
                        factor,
                    },
                    Transform::from_translation(at)
                        .with_rotation(Quat::from_rotation_y((index as f32 * 2.4).sin() * 0.2)),
                    Visibility::Hidden,
                ))
                .id()
        })
        .collect()
}

/// A row of props outside the front edge, beside the entrance: three on the
/// ground and three on top of them.
fn front_pile(frame: &Frame) -> Vec<Vec3> {
    let door = frame.door_local();
    // On the side with more room between the entrance and the corner.
    let side = if door.x > 0.0 { -1.0 } else { 1.0 };
    (0..PILE_SLOTS)
        .map(|index| {
            let (column, level) = (index % 3, index / 3);
            Vec3::new(
                door.x + side * (1.05 + column as f32 * 0.62 + level as f32 * 0.31),
                level as f32 * 0.34,
                -frame.depth * 0.5 - 0.55,
            )
        })
        .collect()
}

/// Rails at `height` around the body, with stakes at the corners.
fn outline(
    commands: &mut Commands,
    parent: Entity,
    assets: &CivicAssets,
    frame: &Frame,
    height: f32,
) {
    let (hw, hd) = (frame.width * 0.5, frame.depth * 0.5);
    for z in [-hd, hd] {
        block(
            commands,
            parent,
            assets,
            &assets.wood,
            Vec3::new(0.0, height, z),
            Vec3::new(frame.width, 0.07, 0.07),
        );
    }
    for x in [-hw, hw] {
        block(
            commands,
            parent,
            assets,
            &assets.wood,
            Vec3::new(x, height, 0.0),
            Vec3::new(0.07, 0.07, frame.depth),
        );
        for z in [-hd, hd] {
            block(
                commands,
                parent,
                assets,
                &assets.wood,
                Vec3::new(x, (height + 0.15) * 0.5, z),
                Vec3::new(0.11, height + 0.15, 0.11),
            );
        }
    }
}

/// Two posts and a lintel on the front edge, where the entrance is.
fn doorway(commands: &mut Commands, parent: Entity, assets: &CivicAssets, frame: &Frame) {
    let door = frame.door_local();
    let z = -frame.depth * 0.5;
    for side in [-0.55, 0.55] {
        block(
            commands,
            parent,
            assets,
            &assets.wood,
            Vec3::new(door.x + side, 0.75, z),
            Vec3::new(0.13, 1.5, 0.13),
        );
    }
    block(
        commands,
        parent,
        assets,
        &assets.wood,
        Vec3::new(door.x, 1.5, z),
        Vec3::new(1.36, 0.12, 0.13),
    );
    // A plank threshold on the entrance cell itself.
    block(
        commands,
        parent,
        assets,
        &assets.plank,
        Vec3::new(door.x, 0.03, door.z),
        Vec3::new(0.9, 0.06, 0.9),
    );
}

fn spawn_site(
    commands: &mut Commands,
    assets: &CivicAssets,
    root: Entity,
    id: SimId,
    frame: &Frame,
    solid: bool,
) {
    let height = if solid { FRAME_HEIGHT } else { YARD_HEIGHT };
    let (hw, hd) = (frame.width * 0.5, frame.depth * 0.5);
    // The foundation: a bed of turned earth, sunk so a slope shows no gap.
    block(
        commands,
        root,
        assets,
        &assets.soil,
        Vec3::new(0.0, -0.17, 0.0),
        Vec3::new(frame.width, 0.5, frame.depth),
    );
    outline(commands, root, assets, frame, 0.42);
    doorway(commands, root, assets, frame);
    // The scaffold: corner posts, top beams and a rail half way up.
    let scaffold = group(commands, root, Visibility::Hidden);
    let (ix, iz) = (hw - 0.22, hd - 0.22);
    for x in [-ix, ix] {
        for z in [-iz, iz] {
            block(
                commands,
                scaffold,
                assets,
                &assets.wood,
                Vec3::new(x, height * 0.5, z),
                Vec3::new(0.15, height, 0.15),
            );
        }
        for level in [height * 0.5, height] {
            block(
                commands,
                scaffold,
                assets,
                &assets.wood,
                Vec3::new(x, level, 0.0),
                Vec3::new(0.1, 0.1, iz * 2.0),
            );
        }
    }
    for z in [-iz, iz] {
        block(
            commands,
            scaffold,
            assets,
            &assets.wood,
            Vec3::new(0.0, height, z),
            Vec3::new(ix * 2.0, 0.1, 0.1),
        );
    }
    // The walls rise inside the scaffold as the work is done.
    let walls = commands
        .spawn((
            ChildOf(root),
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(assets.plank.clone()),
            Transform::from_xyz(0.0, 0.1, 0.0).with_scale(Vec3::new(
                (frame.width - 0.8).max(0.3),
                0.1,
                (frame.depth - 0.8).max(0.3),
            )),
            Visibility::Hidden,
        ))
        .id();
    let piles = slots(commands, root, front_pile(frame), 0.85);
    // The kit's stage models, fitted across the body, inside the scaffold.
    let stages = std::array::from_fn(|stage| {
        let scale = (frame.width - 0.5).max(0.8) / STAGE_WIDTH_UNITS[stage];
        commands
            .spawn((
                Name::new("building stage"),
                ChildOf(root),
                WorldAssetRoot(assets.stages[stage].clone()),
                Transform::from_xyz(0.0, 0.04, 0.0).with_scale(Vec3::splat(scale)),
                Visibility::Hidden,
            ))
            .id()
    });
    commands.entity(root).insert(SiteVisual {
        source: id,
        frame: scaffold,
        walls,
        piles,
        height,
        stages,
        observed: None,
    });
}

/// A prop's or a field's site: a thin bed of turned earth, stakes and a low
/// rail round the footprint, and the delivered materials in front. It
/// shares `SiteVisual` with building sites so deliveries fill the same pile
/// slots; it has no scaffold, walls or stages (those entities are
/// placeholders the progress system skips).
fn spawn_staked_site(
    commands: &mut Commands,
    assets: &CivicAssets,
    root: Entity,
    id: SimId,
    frame: &Frame,
) {
    block(
        commands,
        root,
        assets,
        &assets.soil,
        Vec3::new(0.0, -0.2, 0.0),
        Vec3::new(frame.width - 0.1, 0.44, frame.depth - 0.1),
    );
    outline(commands, root, assets, frame, 0.3);
    let piles = slots(commands, root, front_pile(frame), 0.6);
    commands.entity(root).insert(SiteVisual {
        source: id,
        frame: Entity::PLACEHOLDER,
        walls: Entity::PLACEHOLDER,
        piles,
        height: YARD_HEIGHT,
        stages: [Entity::PLACEHOLDER; 3],
        observed: None,
    });
}

/// An open store: a fenced yard with the stock inside.
fn spawn_yard(
    commands: &mut Commands,
    assets: &CivicAssets,
    root: Entity,
    id: SimId,
    frame: &Frame,
) {
    let (hw, hd) = (frame.width * 0.5, frame.depth * 0.5);
    block(
        commands,
        root,
        assets,
        &assets.soil,
        Vec3::new(0.0, -0.19, 0.0),
        Vec3::new(frame.width, 0.5, frame.depth),
    );
    let door = frame.door_local();
    for x in [-hw + 0.1, hw - 0.1] {
        for z in [-hd + 0.1, hd - 0.1] {
            block(
                commands,
                root,
                assets,
                &assets.wood,
                Vec3::new(x, 0.55, z),
                Vec3::new(0.16, 1.1, 0.16),
            );
        }
    }
    for level in [0.42, 0.86] {
        // Back and sides are closed; the front leaves the entrance open.
        block(
            commands,
            root,
            assets,
            &assets.wood,
            Vec3::new(0.0, level, hd - 0.1),
            Vec3::new(frame.width - 0.2, 0.08, 0.08),
        );
        for x in [-hw + 0.1, hw - 0.1] {
            block(
                commands,
                root,
                assets,
                &assets.wood,
                Vec3::new(x, level, 0.0),
                Vec3::new(0.08, 0.08, frame.depth - 0.2),
            );
        }
        for (from, to) in [(-hw + 0.1, door.x - 0.65), (door.x + 0.65, hw - 0.1)] {
            if to - from > 0.15 {
                block(
                    commands,
                    root,
                    assets,
                    &assets.wood,
                    Vec3::new((from + to) * 0.5, level, -hd + 0.1),
                    Vec3::new(to - from, 0.08, 0.08),
                );
            }
        }
    }
    for side in [-0.65, 0.65] {
        block(
            commands,
            root,
            assets,
            &assets.wood,
            Vec3::new(door.x + side, 0.55, -hd + 0.1),
            Vec3::new(0.14, 1.1, 0.14),
        );
    }
    // Three rows of three, filled from the back.
    let (step_x, step_z) = ((frame.width - 1.5) / 2.0, (frame.depth - 1.6) / 2.0);
    let places = (0..YARD_SLOTS).map(|index| {
        let (column, row) = (index % 3, index / 3);
        Vec3::new(
            (column as f32 - 1.0) * step_x,
            0.06,
            (1.0 - row as f32) * step_z + 0.1,
        )
    });
    let slots = slots(commands, root, places, 1.0);
    commands.entity(root).insert(DepotVisual {
        source: id,
        slots,
        observed: None,
    });
}

/// Whether a workplace is drawn as its trade's building: always, except a
/// trade only the world's data defines on an open site, which is a yard
/// (the simulation's fallback); a solid one is the generic workshop fitted
/// to its body.
pub(crate) fn draws_workplace(role: Role, solid: bool) -> bool {
    solid || !matches!(role, Role::Trade(_))
}

/// Where a civic building's flag stands, in its root's frame: on the ground
/// at the front corner away from the door, half a metre out from the body
/// (`width` across, `depth` deep, the entrance on the -Z side).
pub(crate) fn flag_corner(width: f32, depth: f32, door_x: f32) -> Vec3 {
    let side = if door_x > 0.0 { -1.0 } else { 1.0 };
    Vec3::new(side * (width * 0.5 + 0.2), 0.0, -depth * 0.5 - 0.6)
}

/// A civic building's flag, streaming in the wind.
fn civic_flag(commands: &mut Commands, assets: &CivicAssets, root: Entity, at: Vec3, id: SimId) {
    commands.spawn((
        Name::new("civic flag"),
        ChildOf(root),
        WorldAssetRoot(assets.flag.clone()),
        crate::island_world::ambient::WindFlag {
            seed: (id.0 % 31) as f32 * 0.41,
        },
        Transform::from_translation(at).with_scale(Vec3::splat(8.0)),
    ));
}

/// A closed store: a timber barn under the settlement's roof colour, with
/// what it holds stacked by the door.
fn spawn_barn(
    commands: &mut Commands,
    assets: &CivicAssets,
    root: Entity,
    id: SimId,
    frame: &Frame,
    roof: palette::RoofPalette,
) {
    let (width, depth) = (frame.width - 0.5, frame.depth - 0.5);
    let eaves = 2.3;
    block(
        commands,
        root,
        assets,
        &assets.plank,
        Vec3::new(0.0, eaves * 0.5 - 0.15, 0.0),
        Vec3::new(width, eaves + 0.3, depth),
    );
    // Stepped gables under a roof of two slopes; the ridge runs front to back.
    let rise = width * 0.36;
    for (step, share) in [(0, 0.74), (1, 0.46), (2, 0.2)] {
        block(
            commands,
            root,
            assets,
            &assets.plank,
            Vec3::new(0.0, eaves + rise * (step as f32 + 0.5) / 3.0, 0.0),
            Vec3::new(width * share, rise / 3.0, depth),
        );
    }
    let slope = (rise / (width * 0.5)).atan();
    let run = (width * 0.5).hypot(rise) + 0.35;
    let roof = assets.roofs.get(&roof).unwrap_or(&assets.wood);
    for side in [-1.0_f32, 1.0] {
        commands.spawn((
            ChildOf(root),
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(roof.clone()),
            Transform::from_xyz(side * width * 0.25, eaves + rise * 0.5 + 0.06, 0.0)
                .with_rotation(Quat::from_rotation_z(-side * slope))
                .with_scale(Vec3::new(run, 0.14, depth + 0.5)),
        ));
    }
    let door = frame.door_local();
    block(
        commands,
        root,
        assets,
        &assets.dark,
        Vec3::new(door.x, 0.95, -depth * 0.5 - 0.02),
        Vec3::new(1.5, 1.9, 0.08),
    );
    civic_flag(
        commands,
        assets,
        root,
        flag_corner(width, depth, door.x),
        id,
    );
    let slots = slots(commands, root, front_pile(frame), 1.0);
    commands.entity(root).insert(DepotVisual {
        source: id,
        slots,
        observed: None,
    });
}

/// A market stall: the market model with its goods laid out in front.
fn spawn_stall(
    commands: &mut Commands,
    assets: &CivicAssets,
    root: Entity,
    id: SimId,
    frame: &Frame,
) {
    commands.spawn((
        ChildOf(root),
        WorldAssetRoot(assets.market.clone()),
        Transform::from_xyz(0.0, 0.0, 0.1).with_scale(Vec3::splat(
            (frame.width.min(frame.depth + 1.0) / 2.1).clamp(0.9, 2.4),
        )),
    ));
    let door = frame.door_local();
    let front = -frame.depth * 0.5 - 0.15;
    // Either side of the entrance, two deep, then two on top.
    let places = [
        Vec3::new(door.x - 0.95, 0.0, front),
        Vec3::new(door.x + 0.95, 0.0, front),
        Vec3::new(door.x - 1.45, 0.0, front + 0.1),
        Vec3::new(door.x + 1.45, 0.0, front + 0.1),
        Vec3::new(door.x - 1.2, 0.3, front + 0.05),
        Vec3::new(door.x + 1.2, 0.3, front + 0.05),
    ];
    // Beyond the goods laid out either side of the door.
    let mut flag_at = flag_corner(frame.width, frame.depth, door.x);
    flag_at.x = flag_at.x.signum() * flag_at.x.abs().max(door.x.abs() + 2.1);
    civic_flag(commands, assets, root, flag_at, id);
    let slots = slots(commands, root, places, 0.62);
    commands.entity(root).insert(DepotVisual {
        source: id,
        slots,
        observed: None,
    });
}

#[allow(clippy::too_many_arguments)]
fn sync(
    mut commands: Commands,
    assets: Option<Res<CivicAssets>>,
    server: Res<AssetServer>,
    state: Option<Res<civic::CivicState>>,
    index: Res<SimIndex>,
    terrain: Option<Res<TerrainState>>,
    spec: Option<Res<LivingWorldSpec>>,
    sites: Query<&civic::ConstructionSite>,
    buildings: Query<(
        &civic::Building,
        Option<&Household>,
        Option<&VillagePlace>,
        Option<&civic::Depot>,
    )>,
    fields: Query<(), With<island_content::village::fields::Field>>,
    registry: Option<Res<EconomyRegistry>>,
    roots: Query<&CivicVisual>,
    mut visuals: ResMut<VisualIndex>,
    mut shown: ResMut<CivicVisuals>,
    mut primed: Local<bool>,
) {
    let (Some(assets), Some(state), Some(terrain)) = (assets, state, terrain) else {
        return;
    };
    if *primed && !state.is_changed() && !index.is_changed() {
        return;
    }
    *primed = true;
    let wanted: Vec<(SimId, bool)> = state
        .sites
        .values()
        .flatten()
        .map(|id| (*id, false))
        .chain(state.buildings.values().flatten().map(|id| (*id, true)))
        .collect();
    let gone: Vec<SimId> = shown
        .map
        .keys()
        .filter(|id| !wanted.iter().any(|(wanted, _)| wanted == *id))
        .copied()
        .collect();
    for id in gone {
        if let Some(root) = shown.map.remove(&id) {
            visuals.map.retain(|_, visual| *visual != root);
            if roots.contains(root) {
                commands.entity(root).despawn();
            }
        }
    }
    for (id, built) in wanted {
        let Some(entity) = index.get(id) else {
            continue;
        };
        let (key, plot) = if built {
            let Ok((building, ..)) = buildings.get(entity) else {
                continue;
            };
            (&building.building, building.plot)
        } else {
            let Ok(site) = sites.get(entity) else {
                continue;
            };
            (&site.building, site.plot)
        };
        // Unchanged: the common case costs a lookup and a comparison.
        if let Some(root) = shown.map.get(&id)
            && roots.get(*root).is_ok_and(|visual| {
                visual.0.built == built && visual.0.plot == plot && visual.0.building == *key
            })
        {
            continue;
        }
        let signature = Signature {
            built,
            building: key.clone(),
            plot,
        };
        // A site that became its building, or another world's entity under
        // the same identity: a new root, so everything that dresses homes
        // and workplaces sees it as new.
        if let Some(root) = shown.map.remove(&id) {
            visuals.map.retain(|_, visual| *visual != root);
            if roots.contains(root) {
                commands.entity(root).despawn();
            }
        }
        let frame = frame(&signature.plot);
        let (home, place, depot) = buildings
            .get(entity)
            .map_or((None, None, None), |(_, home, place, depot)| {
                (home, place, depot)
            });
        let name = if built {
            buildings
                .get(entity)
                .map(|(building, ..)| building.label.clone())
        } else {
            sites
                .get(entity)
                .map(|site| format!("{} (site)", site.label))
        }
        .unwrap_or_default();
        let mut transform = if !built {
            frame.centred()
        } else if home.is_some() {
            frame.home_root()
        } else if place.is_some() {
            frame.workplace_root()
        } else {
            frame.centred()
        };
        transform.translation.y = ground(&terrain, transform.translation);
        let root = commands
            .spawn((
                Name::new(name),
                VisualOf(entity),
                StaticId(id),
                CivicVisual(signature.clone()),
                transform,
                Visibility::default(),
                Fogged::default(),
                FogDim::default(),
            ))
            .id();
        if !built {
            let look = sites.get(entity).map_or(SiteLook::Raised, |site| {
                let field = registry
                    .as_deref()
                    .and_then(|registry| registry.spec.building(&site.building))
                    .is_some_and(|def| def.field.is_some());
                site_look(site.is_prop(), field)
            });
            match look {
                SiteLook::Raised => spawn_site(
                    &mut commands,
                    &assets,
                    root,
                    id,
                    &frame,
                    signature.plot.solid,
                ),
                SiteLook::Staked => spawn_staked_site(&mut commands, &assets, root, id, &frame),
            }
        } else if home.is_some() || place.is_some_and(|place| place.role == Role::Innkeeper) {
            // A home and an inn are places somebody can be inside:
            // `island_homes` fits their house to the plot's body.
        } else if fields.contains(entity) {
            // A field's plots are drawn by `island_world::fields`.
        } else if let Some(place) =
            place.filter(|place| draws_workplace(place.role, signature.plot.solid))
        {
            // Fitted to the plot's body like a founding workplace; an open
            // one keeps its scale.
            let plot = &signature.plot;
            let body = crate::island_homes::model::BodyFrame::of_cells(
                (plot.lo_x, plot.lo_z),
                (plot.hi_x, plot.hi_z),
                plot.rotation,
                plot.door(),
            );
            let (placed, fitted) = crate::island_world::workplaces::model_transform(
                place.role,
                plot.solid,
                &transform,
                &body,
                WORKPLACE_OFFSET_MM as f32 / 1000.0,
            );
            let model = commands
                .spawn((
                    ChildOf(root),
                    BuildingModel {
                        station: id,
                        visual: root,
                    },
                    WorldAssetRoot(living_scene::scene(
                        &server,
                        living_scene::building(place.role),
                    )),
                    placed,
                ))
                .observe(living_scene::building_ready)
                .id();
            if fitted {
                commands.entity(root).insert((
                    crate::island_world::workplaces::WorkplaceFit(placed),
                    crate::island_world::workplaces::occluder(place.role, body, &placed, model),
                ));
            }
        } else if depot.is_some_and(|depot| depot.public) {
            spawn_stall(&mut commands, &assets, root, id, &frame);
        } else if signature.plot.solid {
            let roof = spec.as_deref().map_or(palette::RoofPalette::Blue, |spec| {
                palette::settlement_palette(
                    spec,
                    npc_framework::prelude::SimPos::new(
                        (frame.door.x * 1000.0) as i32,
                        (frame.door.z * 1000.0) as i32,
                    ),
                )
            });
            spawn_barn(&mut commands, &assets, root, id, &frame, roof);
        } else {
            spawn_yard(&mut commands, &assets, root, id, &frame);
        }
        visuals.map.insert(entity, root);
        shown.map.insert(id, root);
    }
}

fn show(visibility: &mut Visibility, on: bool) {
    let want = if on {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if *visibility != want {
        *visibility = want;
    }
}

/// Show `kinds` in the first slots and hide the rest.
fn fill(
    commands: &mut Commands,
    assets: &CivicAssets,
    slots: &[Entity],
    kinds: &[PileKind],
    props: &mut Query<(&mut Slot, &mut Transform, &mut Visibility)>,
) {
    for (index, entity) in slots.iter().enumerate() {
        let Ok((mut slot, mut transform, mut visibility)) = props.get_mut(*entity) else {
            continue;
        };
        let kind = kinds.get(index).copied();
        show(&mut visibility, kind.is_some());
        let Some(kind) = kind else { continue };
        if slot.shown != Some(kind) {
            slot.shown = Some(kind);
            transform.scale = Vec3::splat(goods::prop_scale(kind) * slot.factor);
            commands
                .entity(*entity)
                .insert(WorldAssetRoot(assets.prop(kind)));
        }
    }
}

/// Counted goods as counted props, in a stable order.
fn by_prop<'a>(stock: impl Iterator<Item = (&'a str, u32)>) -> Vec<(PileKind, u32)> {
    let mut units = [0_u32; 4];
    for (kind, count) in stock {
        let slot = match goods::stored_kind(kind) {
            PileKind::Lumber => 0,
            PileKind::Crate => 1,
            PileKind::Sack => 2,
            PileKind::Barrel => 3,
        };
        units[slot] = units[slot].saturating_add(count);
    }
    [
        PileKind::Lumber,
        PileKind::Crate,
        PileKind::Sack,
        PileKind::Barrel,
    ]
    .into_iter()
    .zip(units)
    .filter(|(_, units)| *units > 0)
    .collect()
}

/// Deliveries lay materials beside the site; work raises the scaffold and
/// the walls and uses the pile up. A site the fog is not showing live keeps
/// the look it had when it was last seen.
fn site_progress(
    mut commands: Commands,
    assets: Option<Res<CivicAssets>>,
    index: Res<SimIndex>,
    sites: Query<&civic::ConstructionSite>,
    mut visuals: Query<(&mut SiteVisual, &Fogged)>,
    mut parts: Query<(&mut Transform, &mut Visibility), Without<Slot>>,
    mut props: Query<(&mut Slot, &mut Transform, &mut Visibility)>,
) {
    let Some(assets) = assets else { return };
    for (mut visual, fog) in &mut visuals {
        if fog.0 != Look::Live {
            continue;
        }
        let Some(site) = index
            .get(visual.source)
            .and_then(|site| sites.get(site).ok())
        else {
            continue;
        };
        let delivered = by_prop(
            site.delivered
                .iter()
                .map(|(kind, units)| (kind.as_str(), *units)),
        );
        let state = (delivered, site.work_done, site.supplied());
        if visual.observed.as_ref() == Some(&state) {
            continue;
        }
        let (delivered, work_done, supplied) = &state;
        if let Ok((_, mut visibility)) = parts.get_mut(visual.frame) {
            show(&mut visibility, *supplied || *work_done > 0);
        }
        let share = *work_done as f32 / site.work_ticks.max(1) as f32;
        let stage = stage_for(share, *work_done > 0);
        for (index, entity) in visual.stages.into_iter().enumerate() {
            if let Ok((_, mut visibility)) = parts.get_mut(entity) {
                show(&mut visibility, stage == Some(index));
            }
        }
        if let Ok((mut transform, mut visibility)) = parts.get_mut(visual.walls) {
            // The stage models stand for the walls rising.
            show(&mut visibility, false);
            let tall = (share * (visual.height - 0.3)).max(0.1);
            transform.scale.y = tall;
            transform.translation.y = 0.08 + tall * 0.5;
        }
        // What lies beside the site is what the work has not used yet.
        let total: u32 = delivered.iter().map(|(_, units)| *units).sum();
        let lying = model::pile_units(total, *work_done, site.work_ticks);
        let kinds = model::spread(delivered, goods::pile_size(lying).min(visual.piles.len()));
        fill(&mut commands, &assets, &visual.piles, &kinds, &mut props);
        visual.observed = Some(state);
    }
}

/// A store's or a stall's props follow its stock: how many by how full it
/// is, which by what lies there.
fn depot_stock(
    mut commands: Commands,
    assets: Option<Res<CivicAssets>>,
    index: Res<SimIndex>,
    depots: Query<&civic::Depot>,
    mut visuals: Query<(&mut DepotVisual, &Fogged)>,
    mut props: Query<(&mut Slot, &mut Transform, &mut Visibility)>,
) {
    let Some(assets) = assets else { return };
    for (mut visual, fog) in &mut visuals {
        if fog.0 != Look::Live {
            continue;
        }
        let Some(depot) = index
            .get(visual.source)
            .and_then(|depot| depots.get(depot).ok())
        else {
            continue;
        };
        let used = depot.used();
        let stock = by_prop(
            depot
                .lots
                .keys()
                .map(|kind| (kind.as_str(), depot.stock(kind))),
        );
        if visual
            .observed
            .as_ref()
            .is_some_and(|(seen_used, seen)| *seen_used == used && *seen == stock)
        {
            continue;
        }
        let filled = model::filled_slots(used, depot.capacity, visual.slots.len());
        let kinds = model::spread(&stock, filled);
        fill(&mut commands, &assets, &visual.slots, &kinds, &mut props);
        visual.observed = Some((used, stock));
    }
}

/// The translucent volume of the building in hand, green or red, and a gold
/// door-high marker on its entrance cell.
fn ghost_box(
    tool: Res<BuildTool>,
    assets: Option<Res<CivicAssets>>,
    terrain: Option<Res<TerrainState>>,
    mut ghosts: Query<(
        &GhostPart,
        &mut Transform,
        &mut Visibility,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
) {
    let (Some(assets), Some(terrain)) = (assets, terrain) else {
        return;
    };
    let plot = tool.ghost.as_ref().and_then(|ghost| {
        ghost
            .plot
            .map(|plot| (plot, usize::from(ghost.tint() == Tint::Blocked)))
    });
    for (part, mut transform, mut visibility, mut material) in &mut ghosts {
        let Some((plot, tint)) = plot else {
            show(&mut visibility, false);
            continue;
        };
        show(&mut visibility, true);
        let frame = frame(&plot);
        let wanted = if part.entrance {
            let base = ground(&terrain, frame.door);
            Transform::from_xyz(frame.door.x, base + 1.0, frame.door.z)
                .with_scale(Vec3::new(0.7, 1.9, 0.7))
        } else {
            if material.0 != assets.ghost[tint] {
                material.0 = assets.ghost[tint].clone();
            }
            // Stand on the highest cell of the body, so no slope hides it.
            let base = plot
                .cells()
                .filter_map(|cell| terrain.cell(cell.x, cell.z))
                .map(|cell| cell.height_mm.max(0) as f32 / 1000.0)
                .fold(ground(&terrain, frame.door), f32::max);
            let height = if plot.solid {
                FRAME_HEIGHT
            } else {
                YARD_HEIGHT
            };
            let (size_x, size_z) = (
                (plot.hi_x - plot.lo_x + 1) as f32,
                (plot.hi_z - plot.lo_z + 1) as f32,
            );
            Transform::from_xyz(frame.centre.x, base + height * 0.5 + 0.06, frame.centre.z)
                .with_scale(Vec3::new(size_x - 0.1, height, size_z - 0.1))
        };
        if *transform != wanted {
            *transform = wanted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_open_data_only_workplace_is_a_yard_and_a_solid_one_a_workshop() {
        let weaver = Role::from_key("weaver");
        assert!(draws_workplace(weaver, true));
        assert!(!draws_workplace(weaver, false));
        // Shipped trades keep their buildings, open or not.
        assert!(draws_workplace(Role::Smith, false) && draws_workplace(Role::Smith, true));
    }

    #[test]
    fn a_civic_flag_stands_outside_the_front_corner_away_from_the_door() {
        for (width, depth, door_x) in [(4.0, 3.0, 0.0), (5.5, 4.0, 1.0), (5.5, 4.0, -1.0)] {
            let at = flag_corner(width, depth, door_x);
            assert!(at.x.abs() > width * 0.5 && at.z < -depth * 0.5, "{at}");
            assert!(at.x * door_x <= 0.0, "{at} door {door_x}");
            assert!(at.y == 0.0);
        }
    }

    #[test]
    fn a_site_shows_the_kits_stages_as_the_work_goes_on() {
        assert_eq!(stage_for(0.0, false), None);
        assert_eq!(stage_for(0.05, true), Some(0));
        assert_eq!(stage_for(0.5, true), Some(1));
        assert_eq!(stage_for(0.9, true), Some(2));
        assert_eq!(stage_for(1.0, true), Some(2));
    }

    /// A barrel placed in build mode is a site drawn with stakes and no
    /// prop; once it stands it is the prop and no site. Gardens and fields
    /// are staked, other buildings raised.
    #[test]
    fn a_prop_site_is_staked_and_never_drawn_as_the_prop_until_it_stands() {
        use crate::island_world::props as drawn;
        use island_content::village::frontier::{
            props::{self, PlaceProp, RemoveProp},
            settlements,
            shape::{PlaceMode, SetEditorMode},
        };
        use npc_framework::kernel::commands::ContentCommand;
        let mut app = island_content::new_sim_app(7, "living_world");
        npc_framework::prelude::run_ticks(app.world_mut(), 2);
        let barrel = |(x, z): (i32, i32), mode| PlaceProp {
            prop: "barrel".into(),
            x,
            z,
            rotation: 0,
            mode,
        };
        let world = app.world();
        let harbour = world
            .resource::<island_content::IslandIds>()
            .id("settlement_harbour");
        let centre = settlements::definition(world, harbour)
            .expect("harbour")
            .center;
        let spot = (-30..=30)
            .step_by(3)
            .flat_map(|dz| (-30..=30).step_by(3).map(move |dx| (dx, dz)))
            .map(|(dx, dz)| (centre.x / 1000 + dx, centre.z / 1000 + dz))
            .find(|spot| barrel(*spot, PlaceMode::Build).check(world).is_ok())
            .expect("a spot for a barrel");
        let site = barrel(spot, PlaceMode::Build)
            .perform(app.world_mut())
            .expect("a site");
        // While it is a site: the staked look, and nothing for the prop
        // layer to draw.
        let world = app.world();
        let entity = world.resource::<SimIndex>().get(site).expect("site entity");
        let state = world
            .get::<civic::ConstructionSite>(entity)
            .expect("a construction site");
        assert!(state.is_prop());
        assert_eq!(site_look(state.is_prop(), false), SiteLook::Staked);
        let open = drawn::open_sites(world);
        assert!(open.contains(&site));
        let (draw, _) = drawn::changes(&BTreeMap::new(), &props::props(world));
        assert!(draw.is_empty(), "a site is not drawn as its prop");
        // Standing (the sim's finish and an editor placement both leave a
        // prop and no site): the prop alone, and the open-site list changed,
        // so the prop layer redraws in the frame the site look goes.
        RemoveProp { prop: site }
            .perform(app.world_mut())
            .expect("site cancelled");
        SetEditorMode { on: true }.apply(app.world_mut());
        let standing = barrel(spot, PlaceMode::Editor)
            .perform(app.world_mut())
            .expect("stands at once");
        let world = app.world();
        let now = drawn::open_sites(world);
        assert!(!now.contains(&site) && now != open);
        let (draw, gone) = drawn::changes(&BTreeMap::new(), &props::props(world));
        assert_eq!(
            draw.iter().map(|view| view.id).collect::<Vec<_>>(),
            vec![standing]
        );
        assert!(gone.is_empty());
        // Plots with a field are staked; every other building is raised.
        let garden = civic::definition(world, "garden").expect("the garden");
        assert!(garden.field.is_some());
        assert_eq!(site_look(false, true), SiteLook::Staked);
        assert_eq!(site_look(false, false), SiteLook::Raised);
    }
}
