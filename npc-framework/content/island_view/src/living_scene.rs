//! Regional world presentation. Meshes, dressing and atmosphere never own world rules.
use crate::fog::{FogArea, FogDim, Fogged, Look};
use crate::island_atmosphere::ground;
use bevy::{
    asset::RenderAssetUsages,
    camera::primitives::Aabb,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    world_serialization::WorldAsset,
};
use island_content::village::{
    ResourcePatch, Role, Season, VillagePlace,
    colony::Household,
    frontier::{
        terrain::{TerrainState, TerrainSurface},
        world_spec::{Biome, LivingWorldSpec},
    },
};
use npc_framework::prelude::{SimClock, SimConfig, SimId, SimIndex, SimPos};
use npc_framework::view_helpers::{ViewSystems, VisualIndex, VisualOf};

const CHUNK: i32 = 16;
/// Gentle dry steps share slope lighting; banks and larger cliffs stay faceted.
const GENTLE_RISER_MM: i32 = 350;
#[derive(Component)]
pub(crate) struct TerrainChunk {
    x: i32,
    z: i32,
    signature: u64,
}
/// The translucent shallow-water tint of one chunk, kept beside its land.
#[derive(Component)]
pub(crate) struct ShoreChunk {
    x: i32,
    z: i32,
    signature: u64,
}
#[derive(Component)]
struct WindPlant {
    phase: f32,
    yaw: f32,
}
#[derive(Component)]
pub(crate) struct SceneryCell {
    x: i32,
    z: i32,
    model: Entity,
}
#[derive(Component)]
pub(crate) struct BuildingModel {
    pub station: SimId,
    pub visual: Entity,
}
#[derive(Component)]
pub(crate) struct Machine {
    station: SimId,
    visual: Entity,
    axis: Vec3,
    spin: Spin,
}

/// A machine's turn on the sim clock: it spins up while someone works the
/// building, runs down after, and stands still while the game is paused.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Spin {
    /// Turns so far (radians) and turns a tick now.
    pub(crate) angle: f32,
    pub(crate) speed: f32,
    /// The sim time (ticks, smoothed) last seen.
    pub(crate) at: Option<f32>,
}

/// A working machine's full pace: one turn in 240 ticks.
const MACHINE_SPEED: f32 = std::f32::consts::TAU / 240.0;
/// Ticks over which a machine comes up to pace or runs down.
const MACHINE_EASE_TICKS: f32 = 90.0;

impl Spin {
    /// Advance to sim time `now` (ticks): nothing moves unless sim time
    /// has passed.
    /// `scale` is the ambient motion's: at 0 the machine stands.
    pub(crate) fn advance(&mut self, now: f32, working: bool, scale: f32) {
        let passed = self.at.map_or(0.0, |at| (now - at).max(0.0)) * scale;
        self.at = Some(now);
        if passed <= 0.0 {
            return;
        }
        let target = if working { MACHINE_SPEED } else { 0.0 };
        // Up to pace or down to rest at an even rate over the ease.
        let step = MACHINE_SPEED * passed / MACHINE_EASE_TICKS;
        let speed = self.speed + (target - self.speed).clamp(-step, step);
        self.angle = (self.angle + (self.speed + speed) * 0.5 * passed) % std::f32::consts::TAU;
        self.speed = if speed < 1e-5 { 0.0 } else { speed };
    }
}
#[derive(Component)]
struct Cloud {
    anchor: Vec3,
    phase: f32,
    /// Its drawn size, the square it wraps round in (metres) and its place
    /// in the order the sky fills (0..1).
    size: f32,
    span: f32,
    order: f32,
}
#[derive(Component)]
struct Ocean {
    half_extent: f32,
}
#[derive(Resource)]
pub(crate) struct TerrainStyle {
    season: Season,
}
#[derive(Component)]
pub(crate) struct ResourceVisual {
    source: SimId,
    full_scale: f32,
    stage: u8,
    observed: Option<(u32, u32, usize)>,
}
impl ResourceVisual {
    pub(crate) fn new(source: SimId, full_scale: f32) -> Self {
        Self {
            source,
            full_scale,
            stage: 0,
            observed: None,
        }
    }
}
#[derive(Resource, Default)]
struct WaterFrame(f32);
/// The shallows' material: its foam breathes with the swell.
#[derive(Resource)]
struct ShoreFoam(Handle<StandardMaterial>);
/// The simulation terrain revision the land meshes were last built from.
/// View state: the Shape tools read it to time an edit up to its mesh.
#[derive(Resource, Default)]
pub(crate) struct TerrainMeshRevision(pub(crate) u64);

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<WaterFrame>()
        .init_resource::<KenneyMaterials>()
        .init_resource::<SnowMeshes>()
        .add_systems(
            Update,
            (kenney_season, dust_with_snow)
                .chain()
                .in_set(ViewSystems::Sync),
        )
        .init_resource::<TerrainMeshRevision>()
        .add_systems(Startup, (spawn_landscape, spawn_homes))
        .add_systems(
            Update,
            (sync_terrain, sync_shore, sync_dressing, install_camera_fog).in_set(ViewSystems::Sync),
        )
        .add_systems(Update, wind_and_water.in_set(ViewSystems::Present))
        .add_systems(
            Update,
            elevation
                .in_set(ViewSystems::Present)
                .after(crate::village_world::corpse_pose),
        )
        .add_systems(
            Update,
            resource_lifecycle
                .in_set(ViewSystems::Animate)
                .after(crate::fog::FogSet),
        );
    app.add_systems(
        Update,
        working_machinery
            .in_set(ViewSystems::Animate)
            .after(crate::fog::FogSet),
    );
    // `ISLAND_VIEW_STATS=1`: log scene size and frame time once, for reviews.
    if std::env::var_os("ISLAND_VIEW_STATS").is_some() {
        app.add_systems(Last, scene_stats);
    }
}

/// Scenery keeps this far from the walls of a home or a workplace: a tree's
/// crown, and the roof's overhang.
const HOUSE_CLEAR_M: f32 = 2.2;
/// Scenery trees keep this far (mm) from a fishing spot: the bank stays
/// open for the fisher and the camera.
const FISHING_BANK_MM: i64 = 9_000;

/// A scenery model: its path, its scale on the island, and whether it is
/// one of Kenney's (recoloured) or sways.
pub(crate) struct Scenery {
    pub(crate) path: &'static str,
    pub(crate) scale: f32,
    pub(crate) kenney: bool,
    pub(crate) sways: bool,
}

/// The wood's trees, in [`crate::island_homes::model::TreeShape::of_model`]
/// order: the kit's firs and clusters, then Kenney's pines and broadleaves
/// (scaled to 4 to 9 m).
pub(crate) const FOREST: [Scenery; 12] = [
    Scenery {
        path: "living/nature/tree_single_A.gltf",
        scale: 4.2,
        kenney: false,
        sways: true,
    },
    Scenery {
        path: "living/nature/tree_single_B.gltf",
        scale: 4.2,
        kenney: false,
        sways: true,
    },
    Scenery {
        path: "living/nature/trees_A_small.gltf",
        scale: 4.0,
        kenney: false,
        sways: true,
    },
    Scenery {
        path: "living/nature/trees_B_small.gltf",
        scale: 4.0,
        kenney: false,
        sways: true,
    },
    Scenery {
        path: "living/nature/trees_A_medium.gltf",
        scale: 3.6,
        kenney: false,
        sways: true,
    },
    Scenery {
        path: "living/nature/trees_B_medium.gltf",
        scale: 3.6,
        kenney: false,
        sways: true,
    },
    Scenery {
        path: "living/nature/tree_pineTallA.glb",
        scale: 5.0,
        kenney: true,
        sways: true,
    },
    Scenery {
        path: "living/nature/tree_pineDefaultA.glb",
        scale: 4.8,
        kenney: true,
        sways: true,
    },
    Scenery {
        path: "living/nature/tree_pineRoundA.glb",
        scale: 4.8,
        kenney: true,
        sways: true,
    },
    Scenery {
        path: "living/nature/tree_oak.glb",
        scale: 5.4,
        kenney: true,
        sways: true,
    },
    Scenery {
        path: "living/nature/tree_default.glb",
        scale: 4.4,
        kenney: true,
        sways: true,
    },
    Scenery {
        path: "living/nature/tree_thin.glb",
        scale: 4.6,
        kenney: true,
        sways: true,
    },
];

/// Which tree of [`FOREST`] stands in a cell: firs and pines in the wood,
/// broadleaves in the open, a mix everywhere; stable in the cell's hash.
pub(crate) fn forest_tree(biome: Biome, h: u32) -> usize {
    let roll = (h / 11) % 100;
    match biome {
        // The kit's firs and clusters (0 to 5) are smooth cones that read as
        // placeholders among Kenney's faceted trees: the woods use only the
        // latter, pines in the wood and broadleaves in the open.
        Biome::Woodland => match roll {
            0..=64 => [6, 7, 8][(h % 3) as usize],
            _ => [9, 10, 11][(h % 3) as usize],
        },
        _ => match roll {
            0..=34 => [6, 8][(h % 2) as usize],
            _ => [9, 10, 11][(h % 3) as usize],
        },
    }
}

/// The undergrowth: bushes, grass tufts, flowers, mushrooms, stumps,
/// fallen logs and stones.
pub(crate) const UNDERGROWTH: [Scenery; 14] = [
    Scenery {
        path: "living/nature/plant_bush.glb",
        scale: 3.3,
        kenney: true,
        sways: true,
    },
    Scenery {
        path: "living/nature/plant_bushLarge.glb",
        scale: 3.5,
        kenney: true,
        sways: true,
    },
    Scenery {
        path: "living/nature/plant_bushSmall.glb",
        scale: 3.0,
        kenney: true,
        sways: true,
    },
    Scenery {
        path: "living/nature/grass.glb",
        scale: 1.6,
        kenney: true,
        sways: true,
    },
    Scenery {
        path: "living/nature/grass_large.glb",
        scale: 1.6,
        kenney: true,
        sways: true,
    },
    Scenery {
        path: "living/nature/grass_leafs.glb",
        scale: 1.6,
        kenney: true,
        sways: true,
    },
    Scenery {
        path: "living/nature/flower_redA.glb",
        scale: 1.6,
        kenney: true,
        sways: true,
    },
    Scenery {
        path: "living/nature/flower_yellowA.glb",
        scale: 1.6,
        kenney: true,
        sways: true,
    },
    Scenery {
        path: "living/nature/flower_purpleA.glb",
        scale: 1.6,
        kenney: true,
        sways: true,
    },
    Scenery {
        path: "living/nature/mushroom_redGroup.glb",
        scale: 1.0,
        kenney: true,
        sways: false,
    },
    Scenery {
        path: "living/nature/stump_old.glb",
        scale: 2.0,
        kenney: true,
        sways: false,
    },
    Scenery {
        path: "living/nature/log.glb",
        scale: 2.6,
        kenney: true,
        sways: false,
    },
    Scenery {
        path: "living/nature/rock_smallA.glb",
        scale: 2.0,
        kenney: true,
        sways: false,
    },
    Scenery {
        path: "living/nature/mushroom_tanGroup.glb",
        scale: 1.0,
        kenney: true,
        sways: false,
    },
];

/// Which undergrowth grows in a cell: under the trees bushes, mushrooms,
/// stumps and logs; in the open grass and flowers.
pub(crate) fn undergrowth_for(biome: Biome, h: u32) -> usize {
    let roll = ((h / 7) % 100) as usize;
    match biome {
        Biome::Woodland => [0, 1, 2, 5, 9, 10, 11, 13, 3, 0][roll % 10],
        Biome::Meadow => [3, 4, 5, 6, 7, 8, 3, 4, 2, 6][roll % 10],
        Biome::Ridge => [12, 3, 12, 2, 4][roll % 5],
        _ => [3, 4, 5, 2, 3][roll % 5],
    }
}

/// Whether scenery at `p` is kept off: near a door (`homes_mm2`, squared
/// mm) or a site (`sites_mm2`), a landmark, a founding body, a fishing
/// spot's bank or a road's walkable strip. Scenery is the view's own,
/// placed from the world's description and the cell's hash, so the same
/// world always gets the same woods.
pub(crate) fn scenery_blocked(
    spec: &LivingWorldSpec,
    landmarks: &[SimPos],
    fishing: &[SimPos],
    paths: &ground::Roads,
    p: SimPos,
    homes_mm2: i64,
    sites_mm2: i64,
) -> bool {
    spec.settlements.iter().any(|s| {
        s.sites
            .iter()
            .any(|site| site.position.dist2(p) < sites_mm2)
            || s.homes.iter().any(|home| home.door.dist2(p) < homes_mm2)
    }) || landmarks.iter().any(|landmark| {
        landmark.dist2(p) < crate::island_atmosphere::landmark::FOLIAGE_CLEAR_MM.pow(2)
    }) || crate::island_homes::model::on_founding_body(spec, p, HOUSE_CLEAR_M)
        || fishing
            .iter()
            .any(|spot| spot.dist2(p) < FISHING_BANK_MM * FISHING_BANK_MM)
        || !paths.clear_of(Vec2::new(p.x as f32 / 1000.0, p.z as f32 / 1000.0), 0.8)
}

/// Where a settlement's signpost stands: on the road in from `start`,
/// 13 m out from the centre and 2.2 m to the left of the road, its board
/// turned to the arrivals; `None` when the start is the centre.
pub(crate) fn signpost(centre: Vec2, start: Vec2) -> Option<(Vec2, f32)> {
    let out = (start - centre).try_normalize()?;
    let left = Vec2::new(out.y, -out.x);
    let at = centre + out * 13.0 + left * 2.2;
    // The board faces along the road towards the arrivals (+Z to `out`).
    Some((at, out.x.atan2(out.y)))
}

/// On a Kenney nature model: its teal greens are recoloured to the island's
/// palette once it is in the scene.
#[derive(Component, Clone, Copy, Debug)]
struct KenneyGreens;

/// On a Kenney shrub (the bushes of the undergrowth and the berry patches):
/// its leaves stay a dark evergreen through the winter, unlike the
/// broadleaf trees that share their materials.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct KenneyShrub;

/// The colour of a Kenney material on a shrub (`shrub`) or anything else:
/// a shrub's leaves are a dark, muted evergreen in winter.
pub(crate) fn kenney_colour_on(name: &str, season: Season, shrub: bool) -> Option<Color> {
    if shrub && season == Season::Winter && name.starts_with("leafs") {
        return Some(if name == "leafsDark" {
            Color::srgb(0.13, 0.21, 0.15)
        } else {
            Color::srgb(0.17, 0.25, 0.17)
        });
    }
    kenney_colour(name, season)
}

/// Whether a Kenney material carries snow on its upward faces in winter:
/// the evergreens' needles and the shrubs' leaves.
pub(crate) fn snow_lies_on(name: &str, shrub: bool) -> bool {
    name == "leafsDark" || (shrub && name.starts_with("leafs"))
}

/// On a mesh that carries snow in winter: its own mesh and, once made, the
/// snow-dusted copy.
#[derive(Component, Clone, Debug)]
pub(crate) struct Dusted {
    plain: Handle<Mesh>,
    snowy: Option<Handle<Mesh>>,
    /// The material's winter colour, linear.
    base: LinearRgba,
}

/// Snow-dusted copies of meshes, by original mesh and winter colour.
#[derive(Resource, Default)]
pub(crate) struct SnowMeshes(std::collections::BTreeMap<(AssetId<Mesh>, [u32; 3]), Handle<Mesh>>);

/// In winter the evergreens and shrubs wear their snow-dusted meshes, and
/// take them off again when the season turns.
fn dust_with_snow(
    drawn: Option<Res<crate::island_world::ambient::DrawnSeason>>,
    mut plants: Query<(&mut Dusted, &mut Mesh3d)>,
    mut cache: ResMut<SnowMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(drawn) = drawn else { return };
    let winter = drawn.0 == Season::Winter;
    let changed = drawn.is_changed();
    let snow = Color::srgb(
        crate::island_world::tint::SNOW[0],
        crate::island_world::tint::SNOW[1],
        crate::island_world::tint::SNOW[2],
    )
    .to_linear();
    for (mut dusted, mut mesh) in &mut plants {
        if !changed && !dusted.is_added() {
            continue;
        }
        if winter && dusted.snowy.is_none() {
            let key = (
                dusted.plain.id(),
                [dusted.base.red, dusted.base.green, dusted.base.blue].map(f32::to_bits),
            );
            dusted.snowy = match cache.0.get(&key) {
                Some(handle) => Some(handle.clone()),
                None => meshes
                    .get(&dusted.plain)
                    .and_then(|plain| {
                        crate::island_world::tint::snow_dusted(plain, dusted.base, snow)
                    })
                    .map(|copy| {
                        let handle = meshes.add(copy);
                        cache.0.insert(key, handle.clone());
                        handle
                    }),
            };
        }
        let want = match (&dusted.snowy, winter) {
            (Some(snowy), true) => snowy.clone(),
            _ => dusted.plain.clone(),
        };
        if mesh.0 != want {
            mesh.0 = want;
        }
    }
}

/// The island's colour for a Kenney nature material, by its name, in a
/// season: the kit's teal greens become the KayKit greens (autumn turns the
/// broadleaves, winter dulls them); bark, earth and flowers keep warm tones.
pub(crate) fn kenney_colour(name: &str, season: Season) -> Option<Color> {
    let green = |light: f32| match season {
        Season::Autumn => Color::srgb(0.72 * light, 0.5 * light, 0.2 * light),
        // Winter: dark and nearly grey, the green all but gone.
        Season::Winter => Color::srgb(0.27 * light, 0.3 * light, 0.26 * light),
        _ => Color::srgb(0.36 * light, 0.62 * light, 0.27 * light),
    };
    Some(match name {
        "leafsGreen" => green(1.0),
        "leafsDark" => match season {
            // The firs keep their colour through autumn, and stay a dark
            // evergreen under their snow in winter.
            Season::Autumn => Color::srgb(0.2, 0.42, 0.25),
            Season::Winter => Color::srgb(0.13, 0.22, 0.15),
            _ => green(0.72),
        },
        // Winter's grass is dead straw above the snow.
        "grass" if season == Season::Winter => Color::srgb(0.46, 0.41, 0.3),
        "grass" => green(1.08),
        // Oak browns: they read brown, not salmon, under the bright sun.
        "woodBark" => Color::srgb(0.3, 0.2, 0.14),
        "woodBarkDark" => Color::srgb(0.22, 0.15, 0.11),
        "woodInner" => Color::srgb(0.5, 0.38, 0.26),
        // Kenney's rocks are all "dirt": a weathered grey-brown stone.
        "dirt" => Color::srgb(0.52, 0.49, 0.44),
        "_defaultMat" => Color::srgb(0.88, 0.84, 0.76),
        // The signpost's planks.
        "wood" => Color::srgb(0.42, 0.3, 0.2),
        "woodDark" => Color::srgb(0.27, 0.18, 0.12),
        "colorTan" => Color::srgb(0.78, 0.62, 0.42),
        // Flowers keep their colour until winter browns them.
        "colorRed" | "colorYellow" | "colorPurple" if season == Season::Winter => {
            Color::srgb(0.42, 0.33, 0.24)
        }
        "colorRed" => Color::srgb(0.84, 0.24, 0.2),
        "colorYellow" => Color::srgb(0.95, 0.78, 0.25),
        "colorPurple" => Color::srgb(0.6, 0.38, 0.75),
        _ => return None,
    })
}

/// The recoloured materials, one per Kenney material name, and the season
/// they are coloured for: when the season turns, so do they.
#[derive(Resource, Default)]
pub(crate) struct KenneyMaterials(
    std::collections::BTreeMap<String, Handle<StandardMaterial>>,
    Option<Season>,
);

/// Turn the Kenney materials with the season the land is drawn in.
fn kenney_season(
    style: Option<Res<TerrainStyle>>,
    mut cache: ResMut<KenneyMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    drawn: Option<Res<crate::island_world::ambient::DrawnSeason>>,
    mut commands: Commands,
) {
    let Some(style) = style else { return };
    if drawn.is_none_or(|drawn| drawn.0 != style.season) {
        commands.insert_resource(crate::island_world::ambient::DrawnSeason(style.season));
    }
    if cache.1 == Some(style.season) {
        return;
    }
    cache.1 = Some(style.season);
    for (key, handle) in &cache.0 {
        let (name, shrub) = match key.strip_prefix("shrub/") {
            Some(name) => (name, true),
            None => (key.as_str(), false),
        };
        if let (Some(colour), Some(mut material)) = (
            kenney_colour_on(name, style.season, shrub),
            materials.get_mut(handle),
        ) {
            material.base_color = colour;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn recolour_kenney(
    ready: On<bevy::world_serialization::WorldInstanceReady>,
    children: Query<&Children>,
    named: Query<(
        &bevy::gltf::GltfMaterialName,
        &MeshMaterial3d<StandardMaterial>,
        Option<&Mesh3d>,
    )>,
    shrubs: Query<(), With<KenneyShrub>>,
    style: Option<Res<TerrainStyle>>,
    mut cache: ResMut<KenneyMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let season = style.map_or(Season::Summer, |style| style.season);
    let shrub = shrubs.contains(ready.entity);
    for e in children.iter_descendants(ready.entity) {
        let Ok((name, current, mesh)) = named.get(e) else {
            continue;
        };
        let name: &str = &name.0;
        if let (Some(mesh), true) = (mesh, snow_lies_on(name, shrub))
            && let Some(winter) = kenney_colour_on(name, Season::Winter, shrub)
        {
            commands.entity(e).insert(Dusted {
                plain: mesh.0.clone(),
                snowy: None,
                base: winter.to_linear(),
            });
        }
        let key = if shrub && name.starts_with("leafs") {
            format!("shrub/{name}")
        } else {
            name.to_string()
        };
        let handle = match cache.0.get(&key) {
            Some(handle) => handle.clone(),
            None => {
                let Some(colour) = kenney_colour_on(name, season, shrub) else {
                    continue;
                };
                let mut material = materials.get(&current.0).cloned().unwrap_or_default();
                material.base_color = colour;
                material.perceptual_roughness = 0.9;
                material.metallic = 0.0;
                let handle = materials.add(material);
                cache.0.insert(key, handle.clone());
                handle
            }
        };
        commands.entity(e).insert(MeshMaterial3d(handle));
    }
}

/// Seconds after startup at which sampling begins and ends: well after the
/// scene is ready. Pair it with `--screenshot .. --after-secs 18` or longer.
const STATS_WINDOW: (f32, f32) = (12.0, 16.0);

#[derive(Default)]
struct StatsSample {
    frames: u32,
    seconds: f32,
    slowest: f32,
    done: bool,
}

#[allow(clippy::too_many_arguments)]
fn scene_stats(
    time: Res<Time<Real>>,
    mut sample: Local<StatsSample>,
    entities: Query<Entity>,
    meshes: Query<&ViewVisibility, With<Mesh3d>>,
    scenes: Query<(), With<WorldAssetRoot>>,
    foliage: Query<(), With<SceneryCell>>,
    chunks: Query<Has<Mesh3d>, With<TerrainChunk>>,
    shore: Query<Has<Mesh3d>, With<ShoreChunk>>,
    lights: Query<(&PointLight, &ViewVisibility)>,
) {
    let now = time.elapsed_secs();
    if sample.done || now < STATS_WINDOW.0 {
        return;
    }
    if now < STATS_WINDOW.1 {
        sample.frames += 1;
        sample.seconds += time.delta_secs();
        sample.slowest = sample.slowest.max(time.delta_secs());
        return;
    }
    sample.done = true;
    let lit = lights
        .iter()
        .filter(|(light, visible)| light.intensity > 0.0 && visible.get())
        .count();
    info!(
        "island scene stats: entities={} mesh_entities={} meshes_in_view={} scene_roots={}          foliage_roots={} terrain_chunks={}/{} shore_chunks={}/{} point_lights={} lit_point_lights={}          frames={} mean_frame_ms={:.2} slowest_frame_ms={:.2}",
        entities.iter().count(),
        meshes.iter().count(),
        meshes.iter().filter(|visible| visible.get()).count(),
        scenes.iter().count(),
        foliage.iter().count(),
        chunks.iter().filter(|mesh| *mesh).count(),
        chunks.iter().count(),
        shore.iter().filter(|mesh| *mesh).count(),
        shore.iter().count(),
        lights.iter().count(),
        lit,
        sample.frames,
        sample.seconds * 1000.0 / sample.frames.max(1) as f32,
        sample.slowest * 1000.0,
    );
}

pub(crate) fn building_ready(
    ready: On<bevy::world_serialization::WorldInstanceReady>,
    models: Query<&BuildingModel>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut commands: Commands,
) {
    let Ok(building) = models.get(ready.entity) else {
        return;
    };
    for child in children.iter_descendants(ready.entity) {
        let Ok(name) = names.get(child) else { continue };
        // The rotors' hubs sit at their nodes' origins: the windmill's sails
        // and the watermill's wheel turn about local Z, the saw about X.
        let axis = if name.as_str().contains("windmill_top_fan")
            || name.as_str().contains("watermill_wheel")
        {
            Some(Vec3::Z)
        } else if name.as_str().contains("lumbermill_saw") {
            Some(Vec3::X)
        } else {
            None
        };
        if let Some(axis) = axis {
            commands.entity(child).insert(Machine {
                station: building.station,
                visual: building.visual,
                axis,
                spin: Spin::default(),
            });
        }
    }
}

fn working_machinery(
    clock: Res<SimClock>,
    (sim_time, rate, control): (
        Option<Res<npc_framework::prelude::SimTime>>,
        Option<Res<npc_framework::prelude::SimRate>>,
        Option<Res<npc_framework::prelude::SimControl>>,
    ),
    crafts: Query<&npc_framework::crafting::Crafting>,
    fog: Query<&Fogged>,
    mut machines: Query<(&mut Machine, &mut Transform)>,
    motion: Option<Res<crate::island_world::ambient::AmbientMotion>>,
) {
    let scale = motion.map_or(1.0, |motion| motion.scale.clamp(0.0, 1.0));
    let active: std::collections::BTreeSet<_> = crafts
        .iter()
        .filter_map(|craft| craft.workstation)
        .collect();
    let alpha = npc_framework::view_helpers::interp::interp_alpha(
        sim_time.as_deref(),
        rate.as_deref(),
        control.as_deref(),
    );
    let now = clock.tick as f32 - 1.0 + alpha;
    for (mut machine, mut transform) in &mut machines {
        if !fog.get(machine.visual).is_ok_and(|fog| fog.0 == Look::Live) {
            // Out of sight: what was last seen stays.
            machine.spin.at = Some(now);
            continue;
        }
        let working = active.contains(&machine.station);
        let mut spin = machine.spin;
        spin.advance(now, working, scale);
        if spin != machine.spin {
            machine.spin = spin;
            transform.rotation = Quat::from_axis_angle(machine.axis, spin.angle);
        }
    }
}

fn cloud_ready(
    ready: On<bevy::world_serialization::WorldInstanceReady>,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
    mut commands: Commands,
) {
    for child in children.iter_descendants(ready.entity) {
        if meshes.contains(child) {
            commands
                .entity(child)
                .insert((bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver));
        }
    }
}

fn hash(x: i32, z: i32) -> u32 {
    let n = (x as u32).wrapping_mul(374_761_393) ^ (z as u32).wrapping_mul(668_265_263);
    (n ^ (n >> 13)).wrapping_mul(1_274_126_177)
}

/// Shared projection for captions and saved map landmarks. Uses the supplied
/// known position, never a lookup of an unseen entity's live position.
pub(crate) fn raised_point(terrain: Option<&TerrainState>, pos: SimPos, lift: f32) -> Vec3 {
    let y = terrain
        .and_then(|t| t.cell(pos.x.div_euclid(1000), pos.z.div_euclid(1000)))
        .map_or(0.0, |cell| cell.height_mm.max(0) as f32 / 1000.0);
    Vec3::new(pos.x as f32 / 1000.0, y + lift, pos.z as f32 / 1000.0)
}

/// First visible heightfield cell under a pointer. Walks at most width+height
/// cells; unlike a full-grid search its cost does not scale with map area.
pub(crate) fn terrain_hit(world: &World, ray: Ray3d) -> Option<(i32, i32)> {
    let terrain = world.get_resource::<TerrainState>()?;
    let point = raycast(terrain, ray)?;
    Some((point.x.floor() as i32, point.z.floor() as i32))
}

pub(crate) fn raycast(terrain: &TerrainState, ray: Ray3d) -> Option<Vec3> {
    if ray.direction.y >= -0.000_001 {
        return None;
    }
    let bounds = terrain.bounds();
    let min = Vec2::new(bounds.min.x as f32, bounds.min.z as f32);
    let max = min + Vec2::new(bounds.width as f32, bounds.height as f32);
    let origin = Vec2::new(ray.origin.x, ray.origin.z);
    let direction = Vec2::new(ray.direction.x, ray.direction.z);
    let mut enter = 0.0_f32;
    let mut exit = f32::INFINITY;
    for axis in 0..2 {
        if direction[axis].abs() < 0.000_001 {
            if origin[axis] < min[axis] || origin[axis] >= max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] - origin[axis]) / direction[axis];
            let b = (max[axis] - origin[axis]) / direction[axis];
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
        }
    }
    if enter > exit || exit < 0.0 {
        return None;
    }
    let start = ray.get_point(enter + 0.0001);
    let (mut x, mut z) = (start.x.floor() as i32, start.z.floor() as i32);
    let step_x = if direction.x >= 0.0 { 1 } else { -1 };
    let step_z = if direction.y >= 0.0 { 1 } else { -1 };
    let delta_x = if direction.x.abs() < 0.000_001 {
        f32::INFINITY
    } else {
        1.0 / direction.x.abs()
    };
    let delta_z = if direction.y.abs() < 0.000_001 {
        f32::INFINITY
    } else {
        1.0 / direction.y.abs()
    };
    let mut next_x = if delta_x.is_infinite() {
        f32::INFINITY
    } else {
        ((x + if step_x > 0 { 1 } else { 0 }) as f32 - origin.x) / direction.x
    };
    let mut next_z = if delta_z.is_infinite() {
        f32::INFINITY
    } else {
        ((z + if step_z > 0 { 1 } else { 0 }) as f32 - origin.y) / direction.y
    };
    for _ in 0..bounds.width + bounds.height + 2 {
        let cell = terrain.cell(x, z)?;
        let y = if cell.surface == TerrainSurface::Water {
            -0.11
        } else {
            cell.height_mm as f32 / 1000.0
        };
        let hit = (y - ray.origin.y) / ray.direction.y;
        let leave = next_x.min(next_z).min(exit);
        if hit >= enter && hit <= leave {
            return Some(ray.get_point(hit));
        }
        // An elevated cell's vertical side can be the first visible surface.
        if hit < enter && ray.get_point(enter).y >= -0.7 {
            return Some(ray.get_point(enter + 0.0001));
        }
        if leave >= exit {
            return None;
        }
        enter = leave;
        if next_x <= next_z {
            x += step_x;
            next_x += delta_x;
        } else {
            z += step_z;
            next_z += delta_z;
        }
    }
    None
}

/// Whether corners 0 and 2 of a quad are at least as alike as corners 1 and 3.
fn split_first_diagonal(colors: [[f32; 4]; 4]) -> bool {
    let apart = |a: [f32; 4], b: [f32; 4]| {
        (0..4)
            .map(|channel| (a[channel] - b[channel]).abs())
            .sum::<f32>()
    };
    apart(colors[0], colors[2]) <= apart(colors[1], colors[3])
}

#[derive(Default)]
struct Geometry {
    vertices: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    /// Coordinates of the ground grain (`ground::detail_uv`).
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

/// Grain coordinate of a vertex. Height shifts it, so a bank or a beach
/// carries the grain down its face instead of smearing one row of texels.
fn grain_uv(point: [f32; 3]) -> [f32; 2] {
    ground::detail_uv(point[0] + point[1] * 0.7, point[2] + point[1] * 0.7)
}

impl Geometry {
    fn quad(&mut self, points: [[f32; 3]; 4], normal: [f32; 3], color: [f32; 4]) {
        self.blended_quad(points, normal, [color; 4]);
    }
    /// A water cell's beach: two flat-shaded triangles, split along the
    /// diagonal whose ends are closer in height, so a stair step of the
    /// coast becomes one slanted face.
    fn beach_quad(&mut self, points: [[f32; 3]; 4], colors: [[f32; 4]; 4]) {
        let level = |a: usize, b: usize| (points[a][1] - points[b][1]).abs();
        let triangles = if level(0, 2) <= level(1, 3) {
            [[0, 1, 2], [0, 2, 3]]
        } else {
            [[0, 1, 3], [1, 2, 3]]
        };
        for triangle in triangles {
            let [a, b, c] = triangle.map(|corner| Vec3::from_array(points[corner]));
            let mut normal = (b - a).cross(c - a).normalize_or(Vec3::Y);
            if normal.y < 0.0 {
                normal = -normal;
            }
            let start = self.vertices.len() as u32;
            for corner in triangle {
                self.vertices.push(points[corner]);
                self.normals.push(normal.to_array());
                self.colors.push(colors[corner]);
                self.uvs.push(grain_uv(points[corner]));
            }
            self.indices.extend([start, start + 1, start + 2]);
        }
    }
    /// One colour per vertex, so a quad can carry a gradient. The quad is
    /// split along the diagonal whose two ends are closer in colour: the
    /// crease then runs along a colour edge instead of across it, which is
    /// what keeps a diagonal road from looking serrated.
    fn blended_quad(&mut self, points: [[f32; 3]; 4], normal: [f32; 3], colors: [[f32; 4]; 4]) {
        let start = self.vertices.len() as u32;
        self.vertices.extend(points);
        self.normals.extend([normal; 4]);
        self.colors.extend(colors);
        self.uvs.extend(points.map(grain_uv));
        self.indices.extend(
            if split_first_diagonal(colors) {
                [0, 1, 2, 0, 2, 3]
            } else {
                [0, 1, 3, 1, 2, 3]
            }
            .map(|corner| start + corner),
        );
    }
    fn mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.vertices)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

fn chunk_signature(terrain: &TerrainState, x: i32, z: i32) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325_u64;
    // Nearby cells participate: cliff faces depend on the next cell and the
    // blended ground colour reads a few cells further. Biome borders are
    // feathered over a wider band, so biomes participate from further out.
    let (near, far) = (ground::NEAR_MARGIN, ground::MARGIN);
    let close = |c: i32, origin: i32| c >= origin - near && c < origin + CHUNK + near;
    for cz in z - far..z + CHUNK + far {
        for cx in x - far..x + CHUNK + far {
            if let Some(cell) = terrain.cell(cx, cz) {
                value ^= (cell.biome as u64) << 40;
                if close(cx, x) && close(cz, z) {
                    value ^= cell.height_mm as u64 ^ ((cell.surface as u64) << 32);
                }
                value = value.wrapping_mul(0x100_0000_01b3);
            }
        }
    }
    value
}

/// Shading only: the integer top and riser vertices remain unchanged. Reject
/// shoreline/cliff neighbours so their existing hard face lighting is preserved.
fn terrain_surface_normal(terrain: &TerrainState, x: i32, z: i32) -> [f32; 3] {
    let Some(center) = terrain.cell(x, z) else {
        return Vec3::Y.to_array();
    };
    let mut heights = [0_i32; 4];
    for (i, (dx, dz)) in [(-1, 0), (1, 0), (0, -1), (0, 1)].into_iter().enumerate() {
        let Some(neighbor) = terrain.cell(x + dx, z + dz) else {
            return Vec3::Y.to_array();
        };
        if neighbor.surface == TerrainSurface::Water
            || (neighbor.height_mm - center.height_mm).abs() > GENTLE_RISER_MM
        {
            return Vec3::Y.to_array();
        }
        heights[i] = neighbor.height_mm;
    }
    Vec3::new(
        (heights[0] - heights[1]) as f32 / 2000.0,
        1.0,
        (heights[2] - heights[3]) as f32 / 2000.0,
    )
    .normalize()
    .to_array()
}

/// Shade of a hard cliff or bank face against its top edge.
const CLIFF_SHADE: f32 = 0.83;
/// Just above the highest ocean wave (-0.049 m) and below any dry cell.
const SHALLOWS_Y: f32 = -0.03;

fn terrain_mesh(
    terrain: &TerrainState,
    roads: &ground::Roads,
    x: i32,
    z: i32,
    season: Season,
) -> Option<Mesh> {
    let dry = |cx: i32, cz: i32| {
        terrain
            .cell(cx, cz)
            .is_some_and(|cell| cell.surface != TerrainSurface::Water)
    };
    // The ocean owns water rendering. Keep open-water chunks as bookkeeping
    // entities, without ever uploading a zero-vertex asset to the renderer.
    // A beach reaches one cell into the water, so land beside the chunk counts.
    if !(z - 1..z + CHUNK + 1).any(|cz| (x - 1..x + CHUNK + 1).any(|cx| dry(cx, cz))) {
        return None;
    }
    // One colour per grid corner, shared by the up to four cells around it.
    let patch = ground::Patch::around(terrain, roads, x, z, CHUNK);
    let side = CHUNK as usize + 1;
    let corners: Vec<[f32; 4]> = (0..side * side)
        .map(|i| ground::corner_color(&patch, x + (i % side) as i32, z + (i / side) as i32, season))
        .collect();
    let corner = |cx: i32, cz: i32| corners[(cz - z) as usize * side + (cx - x) as usize];
    let mut geometry = Geometry::default();
    for cz in z..z + CHUNK {
        for cx in x..x + CHUNK {
            let Some(cell) = terrain.cell(cx, cz) else {
                continue;
            };
            if cell.surface == TerrainSurface::Water {
                continue;
            }
            let (a, b, h) = (cx as f32, cz as f32, cell.height_mm as f32 / 1000.0);
            let surface_normal = terrain_surface_normal(terrain, cx, cz);
            geometry.blended_quad(
                [
                    [a, h, b],
                    [a, h, b + 1.0],
                    [a + 1.0, h, b + 1.0],
                    [a + 1.0, h, b],
                ],
                surface_normal,
                [
                    corner(cx, cz),
                    corner(cx, cz + 1),
                    corner(cx + 1, cz + 1),
                    corner(cx + 1, cz),
                ],
            );
            for (dx, dz, normal, edge) in [
                (-1, 0, [-1.0, 0.0, 0.0], [[0, 0], [0, 1]]),
                (1, 0, [1.0, 0.0, 0.0], [[1, 1], [1, 0]]),
                (0, -1, [0.0, 0.0, -1.0], [[1, 0], [0, 0]]),
                (0, 1, [0.0, 0.0, 1.0], [[0, 1], [1, 1]]),
            ] {
                let neighbor = terrain.cell(cx + dx, cz + dz);
                let low = neighbor.map_or(-0.7, |n| n.height_mm as f32 / 1000.0);
                if low >= h {
                    continue;
                }
                let [[i0, j0], [i1, j1]] = edge;
                let (x0, z0, x1, z1) = (a + i0 as f32, b + j0 as f32, a + i1 as f32, b + j1 as f32);
                let (c0, c1) = (corner(cx + i0, cz + j0), corner(cx + i1, cz + j1));
                let gentle = neighbor.is_some_and(|n| {
                    n.surface != TerrainSurface::Water
                        && cell.height_mm - n.height_mm <= GENTLE_RISER_MM
                });
                let shade = |c: [f32; 4]| {
                    [
                        c[0] * CLIFF_SHADE,
                        c[1] * CLIFF_SHADE,
                        c[2] * CLIFF_SHADE,
                        1.0,
                    ]
                };
                let (face_normal, c0, c1) = if gentle {
                    (surface_normal, c0, c1)
                } else {
                    (normal, shade(c0), shade(c1))
                };
                geometry.blended_quad(
                    [[x0, h, z0], [x0, low, z0], [x1, low, z1], [x1, h, z1]],
                    face_normal,
                    [c0, c0, c1, c1],
                );
            }
        }
    }
    // The beach: every water cell touching land slopes from the land's edge
    // to below the surface, in front of the stepped bank. Cells are read
    // only; the shore the simulation knows is unchanged.
    let wet = ground::wet_sand(season);
    let dry_sand = ground::dry_sand(season);
    for cz in z..z + CHUNK {
        for cx in x..x + CHUNK {
            if terrain
                .cell(cx, cz)
                .is_none_or(|cell| cell.surface != TerrainSurface::Water)
            {
                continue;
            }
            let corners = [(0, 0), (0, 1), (1, 1), (1, 0)];
            let heights = corners.map(|(dx, dz)| ground::beach_corner(terrain, cx + dx, cz + dz));
            if heights.iter().all(Option::is_none) {
                continue;
            }
            let mut points = [[0.0; 3]; 4];
            let mut colors = [wet; 4];
            for (i, (dx, dz)) in corners.into_iter().enumerate() {
                points[i] = [
                    (cx + dx) as f32,
                    heights[i].unwrap_or(ground::BEACH_FOOT),
                    (cz + dz) as f32,
                ];
                if let Some(height) = heights[i] {
                    // The land's own colour at its edge: half sand on a low
                    // shore, the shaded bank on a high one.
                    let land = corner(cx + dx, cz + dz);
                    let sand = ground::sandiness(height);
                    // Snow lies on the land, not on the strand: in winter
                    // the beach is the wet sand's, so the waterline reads.
                    let strand = if season == Season::Winter { 0.75 } else { 0.0 };
                    let blend = |channel: usize| {
                        let bank = land[channel] * (CLIFF_SHADE + 0.08);
                        let beach = (land[channel] + dry_sand[channel]) * 0.5;
                        let edge = bank + (beach - bank) * sand;
                        edge + (wet[channel] - edge) * strand * sand
                    };
                    colors[i] = [blend(0), blend(1), blend(2), 1.0];
                }
            }
            geometry.beach_quad(points, colors);
        }
    }
    (!geometry.vertices.is_empty()).then(|| geometry.mesh())
}

/// A translucent band over the water cells nearest the shore: strongest at
/// the waterline and gone three metres out. View geometry only.
fn shore_mesh(terrain: &TerrainState, x: i32, z: i32) -> Option<Mesh> {
    let mut geometry = Geometry::default();
    for cz in z..z + CHUNK {
        for cx in x..x + CHUNK {
            if terrain
                .cell(cx, cz)
                .is_some_and(|cell| cell.surface != TerrainSurface::Water)
            {
                continue;
            }
            let (a, b) = (cx as f32, cz as f32);
            let quad = [
                [a, SHALLOWS_Y, b],
                [a, SHALLOWS_Y, b + 1.0],
                [a + 1.0, SHALLOWS_Y, b + 1.0],
                [a + 1.0, SHALLOWS_Y, b],
            ];
            // An inland pond (wetland water) is still water, not the sea's
            // shallows: one dark, even tint over the whole pond. Drawn as
            // shallows, every pond cell was near land, so the whole pond
            // turned the pale turquoise of a beach and breathed white with
            // the swell: pale rectangles on the island.
            if terrain
                .cell(cx, cz)
                .is_some_and(|cell| cell.biome == Biome::Wetland)
            {
                geometry.blended_quad(quad, [0.0, 1.0, 0.0], [POND_TINT; 4]);
                continue;
            }
            let alpha = [(0, 0), (0, 1), (1, 1), (1, 0)]
                .map(|(dx, dz)| ground::shallows(terrain, cx + dx, cz + dz));
            if alpha.iter().all(|alpha| *alpha <= 0.0) {
                continue;
            }
            geometry.blended_quad(
                quad,
                [0.0, 1.0, 0.0],
                alpha.map(|alpha| [1.0, 1.0, 1.0, alpha]),
            );
        }
    }
    (!geometry.vertices.is_empty()).then(|| geometry.mesh())
}

/// A pond's vertex colour over the shallows material: its light turquoise
/// and foam taken down to a still, dark green-teal, laid fully over the
/// water beneath.
const POND_TINT: [f32; 4] = [0.36, 0.40, 0.40, 1.0];

pub(crate) fn scene(server: &AssetServer, path: &'static str) -> Handle<WorldAsset> {
    server.load(GltfAssetLabel::Scene(0).from_asset(path))
}

fn spawn_terrain_chunks(
    commands: &mut Commands,
    terrain: &TerrainState,
    roads: &ground::Roads,
    meshes: &mut Assets<Mesh>,
    material: &Handle<StandardMaterial>,
    season: Season,
) {
    let bounds = terrain.bounds();
    for z in (bounds.min.z..bounds.min.z + bounds.height as i32).step_by(CHUNK as usize) {
        for x in (bounds.min.x..bounds.min.x + bounds.width as i32).step_by(CHUNK as usize) {
            let mut chunk = commands.spawn((
                Name::new(format!("regional terrain {x},{z}")),
                TerrainChunk {
                    x,
                    z,
                    signature: chunk_signature(terrain, x, z),
                },
                MeshMaterial3d(material.clone()),
                Transform::default(),
            ));
            if let Some(mesh) = terrain_mesh(terrain, roads, x, z, season) {
                chunk.insert(Mesh3d(meshes.add(mesh)));
            }
        }
    }
}

fn spawn_shore_chunks(
    commands: &mut Commands,
    terrain: &TerrainState,
    meshes: &mut Assets<Mesh>,
    material: &Handle<StandardMaterial>,
) {
    let bounds = terrain.bounds();
    for z in (bounds.min.z..bounds.min.z + bounds.height as i32).step_by(CHUNK as usize) {
        for x in (bounds.min.x..bounds.min.x + bounds.width as i32).step_by(CHUNK as usize) {
            let mut chunk = commands.spawn((
                Name::new(format!("regional shallows {x},{z}")),
                ShoreChunk {
                    x,
                    z,
                    signature: chunk_signature(terrain, x, z),
                },
                MeshMaterial3d(material.clone()),
                Transform::default(),
                bevy::light::NotShadowCaster,
            ));
            if let Some(mesh) = shore_mesh(terrain, x, z) {
                chunk.insert(Mesh3d(meshes.add(mesh)));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_landscape(
    mut commands: Commands,
    terrain: Res<TerrainState>,
    spec: Res<LivingWorldSpec>,
    clock: Res<SimClock>,
    config: Res<SimConfig>,
    server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    images: Option<ResMut<Assets<Image>>>,
    patches: Query<(&ResourcePatch, &SimPos)>,
) {
    let (_, season, _) = island_content::village::calendar_at(clock.tick, &config, true);
    let season = crate::island_world::ambient::view_season().unwrap_or(season);
    // Fishing spots: the banks round them are kept clear of scenery trees.
    let fishing: Vec<SimPos> = patches
        .iter()
        .filter(|(patch, _)| patch.kind == island_content::village::catalog::FISH)
        .map(|(_, at)| *at)
        .collect();
    commands.insert_resource(TerrainStyle { season });
    // The grain multiplies the vertex colours; the base colour cancels its
    // mean, so the palette is unchanged from any distance.
    let grain = images.map(|mut images| images.add(ground::detail_image()));
    let gain = if grain.is_some() {
        1.0 / ground::DETAIL_MEAN
    } else {
        1.0
    };
    let land = materials.add(StandardMaterial {
        base_color: Color::linear_rgb(gain, gain, gain),
        base_color_texture: grain,
        perceptual_roughness: 0.96,
        reflectance: 0.08,
        ..default()
    });
    // The weather multiplies this base colour (wet, snowed on, parched).
    commands.insert_resource(crate::island_world::weather::LandMaterial {
        handle: land.clone(),
        base: LinearRgba::rgb(gain, gain, gain),
    });
    let roads = ground::Roads::from_spec(&spec);
    spawn_terrain_chunks(&mut commands, &terrain, &roads, &mut meshes, &land, season);
    let paths = roads.clone();
    commands.insert_resource(roads);
    // Sunlit sand seen through a little water; the vertex alpha fades it out.
    let shallows = materials.add(StandardMaterial {
        base_color: Color::srgba(0.42, 0.74, 0.70, 0.62),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.32,
        reflectance: 0.45,
        ..default()
    });
    spawn_shore_chunks(&mut commands, &terrain, &mut meshes, &shallows);
    commands.insert_resource(ShoreFoam(shallows.clone()));
    let water = materials.add(StandardMaterial {
        base_color: Color::srgb(0.075, 0.31, 0.39),
        // A little rough, so a low sun spreads into soft glitter rather
        // than hard spots.
        perceptual_roughness: 0.36,
        reflectance: 0.5,
        metallic: 0.08,
        ..default()
    });
    let half_extent = terrain.width.max(terrain.height) as f32 * 0.5 + 96.0;
    // Continue the sea beyond the detailed animated patch; overview framing
    // must never expose a rectangular ocean floating against the clear colour.
    let horizon = terrain.width.max(terrain.height) as f32 * 16.0;
    commands.spawn((
        Name::new("distant sea"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(horizon, horizon))),
        MeshMaterial3d(water.clone()),
        Transform::from_xyz(0.0, -0.22, 0.0),
    ));
    commands.spawn((
        Name::new("living ocean"),
        Ocean { half_extent },
        Mesh3d(meshes.add(ocean_mesh(0.0, half_extent))),
        MeshMaterial3d(water),
        Transform::default(),
    ));
    // A mixed wood: the kit's firs and clusters with Kenney's pines, oak and
    // broadleaves (their teal greens recoloured to the island's).
    let trees: Vec<(Handle<WorldAsset>, f32, bool)> = FOREST
        .iter()
        .map(|tree| (scene(&server, tree.path), tree.scale, tree.kenney))
        .collect();
    let rocks = [
        scene(&server, "living/nature/rock_single_B.gltf"),
        scene(&server, "living/nature/rock_single_D.gltf"),
        scene(&server, "living/nature/rock_single_E.gltf"),
    ];
    let reed = scene(&server, "living/nature/waterplant_A.gltf");
    let undergrowth: Vec<(Handle<WorldAsset>, f32, bool)> = UNDERGROWTH
        .iter()
        .map(|plant| (scene(&server, plant.path), plant.scale, plant.kenney))
        .collect();
    let landmarks = crate::island_atmosphere::landmark::landmark_sites(&spec, &terrain);
    let kept_clear = |p: SimPos, homes_mm2: i64, sites_mm2: i64| {
        scenery_blocked(&spec, &landmarks, &fishing, &paths, p, homes_mm2, sites_mm2)
    };
    for (coord, cell) in terrain.iter() {
        if cell.surface != TerrainSurface::Grass {
            continue;
        }
        let h = hash(coord.x, coord.z);
        // Undergrowth on its own lattice, between the trees'.
        if coord.x.rem_euclid(3) == 1 && coord.z.rem_euclid(3) == 2 {
            let chance = match cell.biome {
                Biome::Woodland => 55,
                Biome::Meadow => 28,
                Biome::Coast => 10,
                Biome::Ridge => 12,
                Biome::Wetland => 20,
            };
            if h % 100 < chance {
                let p = SimPos::new(
                    coord.x * 1000 + 250 + (h % 500) as i32,
                    coord.z * 1000 + 250 + ((h / 500) % 500) as i32,
                );
                if !kept_clear(p, 9_000_000, 16_000_000) {
                    let pick = undergrowth_for(cell.biome, h);
                    let (model, scale, kenney) = &undergrowth[pick];
                    let at = Vec3::new(
                        p.x as f32 / 1000.0,
                        cell.height_mm as f32 / 1000.0,
                        p.z as f32 / 1000.0,
                    );
                    let yaw = (h % 628) as f32 / 100.0;
                    let size = scale * (0.85 + (h % 7) as f32 * 0.05);
                    let root = commands
                        .spawn((
                            Name::new("undergrowth"),
                            Transform::from_translation(at),
                            Visibility::default(),
                            FogArea::around(at, Vec3::splat(0.6)),
                            FogDim::default(),
                        ))
                        .id();
                    let mut plant = commands.spawn((
                        ChildOf(root),
                        WorldAssetRoot(model.clone()),
                        Transform::from_rotation(Quat::from_rotation_y(yaw))
                            .with_scale(Vec3::splat(size)),
                    ));
                    if *kenney {
                        if UNDERGROWTH[pick].path.contains("plant_bush") {
                            plant.insert(KenneyShrub);
                        }
                        plant.insert(KenneyGreens).observe(recolour_kenney);
                    }
                    if UNDERGROWTH[pick].sways {
                        plant.insert(WindPlant {
                            phase: (h % 29) as f32,
                            yaw,
                        });
                    }
                    let model = plant.id();
                    commands.entity(root).insert(SceneryCell {
                        x: coord.x,
                        z: coord.z,
                        model,
                    });
                }
            }
        }
        if coord.x.rem_euclid(3) != 0 || coord.z.rem_euclid(3) != 0 {
            continue;
        }
        let chance = match cell.biome {
            Biome::Woodland => 58,
            Biome::Meadow => 10,
            Biome::Coast => 8,
            Biome::Ridge => 20,
            Biome::Wetland => 32,
        };
        if h % 100 >= chance {
            continue;
        }
        let p = SimPos::new(coord.x * 1000 + 500, coord.z * 1000 + 500);
        if kept_clear(p, 24_000_000, 30_000_000) {
            continue;
        }
        let at = Vec3::new(
            p.x as f32 / 1000.0,
            cell.height_mm as f32 / 1000.0,
            p.z as f32 / 1000.0,
        );
        // The tree line frays at the woodland border instead of tracing it.
        if cell.biome == Biome::Woodland
            && crate::island_atmosphere::dressing::tree_thinned(&terrain, coord.x, coord.z)
        {
            continue;
        }
        let yaw = (h % 628) as f32 / 100.0;
        let tree = forest_tree(cell.biome, h);
        let (model, scale, wind, kenney) = match cell.biome {
            Biome::Ridge => (
                rocks[(h as usize / 7) % rocks.len()].clone(),
                3.0 + (h % 9) as f32 * 0.25,
                false,
                false,
            ),
            Biome::Wetland => (reed.clone(), 3.0 + (h % 6) as f32 * 0.2, true, false),
            _ => (
                trees[tree].0.clone(),
                trees[tree].1 * (0.85 + (h % 10) as f32 * 0.04),
                true,
                trees[tree].2,
            ),
        };
        let root = commands
            .spawn((
                Name::new("regional foliage"),
                Transform::from_translation(at),
                Visibility::default(),
                FogArea::around(at, Vec3::splat(1.0)),
                FogDim::default(),
            ))
            .id();
        let mut plant = commands.spawn((
            ChildOf(root),
            WorldAssetRoot(model),
            Transform::from_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale)),
        ));
        if kenney {
            plant.insert(KenneyGreens).observe(recolour_kenney);
        }
        if wind {
            plant.insert(WindPlant {
                phase: (h % 31) as f32 + (h % 997) as f32 * 0.013,
                yaw,
            });
        }
        let model = plant.id();
        if !matches!(cell.biome, Biome::Ridge | Biome::Wetland) {
            // A tree can stand between the camera and the played character:
            // its trunk and crown, measured on the models.
            commands.entity(root).insert(crate::island_homes::Canopy {
                shape: crate::island_homes::model::TreeShape::of_model(tree, scale),
                drawn: model,
            });
        }
        commands.entity(root).insert(SceneryCell {
            x: coord.x,
            z: coord.z,
            model,
        });
    }
    // A signpost where the road from the landing enters each settlement,
    // beside the road on the left as one arrives.
    let sign = scene(&server, "living/props/sign.glb");
    let start = Vec2::new(
        spec.actor_start.x as f32 / 1000.0,
        spec.actor_start.z as f32 / 1000.0,
    );
    for settlement in &spec.settlements {
        let centre = Vec2::new(
            settlement.center.x as f32 / 1000.0,
            settlement.center.z as f32 / 1000.0,
        );
        let Some(spot) = signpost(centre, start) else {
            continue;
        };
        let (at, yaw) = spot;
        let height = terrain
            .cell(at.x.floor() as i32, at.y.floor() as i32)
            .filter(|cell| cell.surface != TerrainSurface::Water)
            .map(|cell| cell.height_mm as f32 / 1000.0);
        let Some(height) = height else { continue };
        let at = Vec3::new(at.x, height, at.y);
        commands
            .spawn((
                Name::new("settlement signpost"),
                WorldAssetRoot(sign.clone()),
                Transform::from_translation(at)
                    .with_rotation(Quat::from_rotation_y(yaw))
                    .with_scale(Vec3::splat(4.0)),
                FogArea::around(at, Vec3::splat(0.6)),
                FogDim::default(),
            ))
            .observe(recolour_kenney);
    }
    let cloud = scene(&server, "living/nature/cloud_big.gltf");
    let radius = terrain.width.max(terrain.height) as f32 * 0.5;
    for (i, (x, y, z)) in [
        (-1.14, 86.0, -0.91),
        (0.61, 102.0, -1.22),
        (-1.27, 94.0, 0.48),
        (1.20, 89.0, -0.29),
        (0.18, 108.0, 1.26),
        (-0.76, 98.0, 1.13),
        // Over the island: with the clouds drifting they cross it.
        (-0.35, 96.0, -0.2),
        (0.42, 91.0, 0.55),
        (0.05, 104.0, -0.62),
        (-0.62, 88.0, 0.12),
    ]
    .into_iter()
    .enumerate()
    {
        let anchor = Vec3::new(x * radius, y, z * radius);
        let size = 10.0 + (i % 3) as f32;
        commands
            .spawn((
                Name::new("coastal cloud"),
                Cloud {
                    anchor,
                    phase: i as f32,
                    size,
                    span: radius * 2.9,
                    order: [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.62, 0.72, 0.82, 0.9][i],
                },
                WorldAssetRoot(cloud.clone()),
                Transform::from_translation(anchor).with_scale(Vec3::splat(10.0 + (i % 3) as f32)),
            ))
            .observe(cloud_ready);
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_terrain(
    mut commands: Commands,
    terrain: Res<TerrainState>,
    roads: Option<Res<ground::Roads>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut chunks: Query<(Entity, &mut TerrainChunk, Option<&Mesh3d>)>,
    clock: Res<SimClock>,
    config: Res<SimConfig>,
    mut style: ResMut<TerrainStyle>,
    built: Option<ResMut<TerrainMeshRevision>>,
) {
    let (_, season, _) = island_content::village::calendar_at(clock.tick, &config, true);
    let season = crate::island_world::ambient::view_season().unwrap_or(season);
    let season_changed = style.season != season;
    if !terrain.is_changed() && !season_changed {
        return;
    }
    if let Some(mut built) = built {
        built.0 = terrain.revision;
    }

    let roads = roads.map(|roads| roads.clone()).unwrap_or_default();
    for (entity, mut chunk, handle) in &mut chunks {
        let signature = chunk_signature(&terrain, chunk.x, chunk.z);
        if signature != chunk.signature || season_changed {
            match terrain_mesh(&terrain, &roads, chunk.x, chunk.z, season) {
                Some(mesh) => {
                    if let Some(handle) = handle
                        && let Some(mut asset) = meshes.get_mut(&handle.0)
                    {
                        *asset = mesh;
                    } else {
                        commands.entity(entity).insert(Mesh3d(meshes.add(mesh)));
                    }
                }
                None => {
                    if let Some(handle) = handle {
                        commands.entity(entity).remove::<(Mesh3d, Aabb)>();
                        // Each chunk owns its asset. Remove it immediately so
                        // no stale land or empty replacement reaches rendering.
                        meshes.remove(handle.id());
                    }
                }
            }
            chunk.signature = signature;
        }
    }
    style.season = season;
}

fn sync_shore(
    mut commands: Commands,
    terrain: Res<TerrainState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut chunks: Query<(Entity, &mut ShoreChunk, Option<&Mesh3d>)>,
) {
    if !terrain.is_changed() {
        return;
    }
    for (entity, mut chunk, handle) in &mut chunks {
        let signature = chunk_signature(&terrain, chunk.x, chunk.z);
        if signature == chunk.signature {
            continue;
        }
        chunk.signature = signature;
        match shore_mesh(&terrain, chunk.x, chunk.z) {
            Some(mesh) => {
                if let Some(handle) = handle
                    && let Some(mut asset) = meshes.get_mut(&handle.0)
                {
                    *asset = mesh;
                } else {
                    commands.entity(entity).insert(Mesh3d(meshes.add(mesh)));
                }
            }
            None => {
                if let Some(handle) = handle {
                    commands.entity(entity).remove::<(Mesh3d, Aabb)>();
                    meshes.remove(handle.id());
                }
            }
        }
    }
}

fn sync_dressing(
    terrain: Res<TerrainState>,
    civic: Option<Res<island_content::village::civic::CivicState>>,
    buildings: Query<&island_content::village::civic::Building>,
    sites: Query<&island_content::village::civic::ConstructionSite>,
    mut roots: Query<(&SceneryCell, &mut Transform)>,
    mut models: Query<&mut Visibility>,
) {
    if !terrain.is_changed() && !civic.is_some_and(|civic| civic.is_changed()) {
        return;
    }
    // Scenery gives way to whatever is built after the founding, and to
    // its site while it is being built.
    let plots: Vec<island_content::village::civic::Plot> = buildings
        .iter()
        .map(|building| building.plot)
        .chain(sites.iter().map(|site| site.plot))
        .collect();
    for (cell, mut transform) in &mut roots {
        let Some(ground) = terrain.cell(cell.x, cell.z) else {
            continue;
        };
        let height = ground.height_mm as f32 / 1000.0;
        if transform.translation.y != height {
            transform.translation.y = height;
        }
        let shown = ground.surface == TerrainSurface::Grass
            && !plots.iter().any(|plot| {
                crate::island_world::on_plot(
                    plot,
                    cell.x,
                    cell.z,
                    crate::island_world::plot_clearance(plot),
                )
            });
        let wanted = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Ok(mut visibility) = models.get_mut(cell.model)
            && *visibility != wanted
        {
            *visibility = wanted;
        }
    }
}

fn spawn_homes(
    mut commands: Commands,
    homes: Query<
        (Entity, &SimId, &SimPos, &Household),
        (
            Without<island_content::village::homestead::Cottage>,
            // Homes built after the founding are drawn by `island_build`.
            Without<island_content::village::civic::Building>,
        ),
    >,
    terrain: Res<TerrainState>,
    mut index: ResMut<VisualIndex>,
) {
    for (entity, id, position, house) in &homes {
        let y = terrain
            .cell(position.x.div_euclid(1000), position.z.div_euclid(1000))
            .map_or(0.0, |c| c.height_mm as f32 / 1000.0);
        let root = commands
            .spawn((
                Name::new(house.label.clone()),
                VisualOf(entity),
                crate::village_world::StaticId(*id),
                Transform::from_xyz(position.x as f32 / 1000.0, y, position.z as f32 / 1000.0),
                Visibility::default(),
                Fogged::default(),
                FogDim::default(),
            ))
            .id();
        // The house itself is fitted to the home's body by `island_homes`.
        index.map.insert(entity, root);
    }
}

fn install_camera_fog(
    mut commands: Commands,
    cameras: Query<Entity, (With<Camera3d>, Without<DistanceFog>)>,
) {
    for entity in &cameras {
        commands.entity(entity).insert(DistanceFog {
            color: Color::srgb(0.58, 0.70, 0.73),
            directional_light_color: Color::srgba(1.0, 0.88, 0.68, 0.25),
            directional_light_exponent: 18.0,
            falloff: FogFalloff::from_visibility_colors(
                210.0,
                Color::srgb(0.38, 0.52, 0.61),
                Color::srgb(0.73, 0.82, 0.86),
            ),
        });
    }
}

/// The sea's surface with the island's wind: long swells roll downwind,
/// their crests lighter and catching the light, a cross-sea under them.
/// Plants nearer the camera than this (metres, ground plane) sway every
/// frame; farther ones every [`SWAY_FAR_EVERY`] frames.
const SWAY_NEAR_M: f32 = 70.0;
const SWAY_FAR_EVERY: u32 = 4;

/// Half the width, in grid steps, over which the swell's normals take their
/// slope (see [`swell_mesh`]).
const NORMAL_REACH_STEPS: f32 = 2.0;

fn swell_mesh(time: f32, half_extent: f32, downwind: Vec2, strength: f32) -> Mesh {
    let cells = 32;
    let step = half_extent / cells as f32;
    let k = downwind.normalize_or(Vec2::X);
    let cross = Vec2::new(-k.y, k.x);
    let sea = 0.7 + 0.3 * strength.clamp(0.0, 3.0);
    let wave = |x: f32, z: f32| {
        let p = Vec2::new(x, z);
        let along = p.dot(k) / 34.0 - time * 0.42;
        let across = p.dot(cross) / 23.0 + time * 0.23;
        let chop = (p.dot(Vec2::new(0.6, 0.8)) / 11.0 - time * 0.9).sin();
        (along * std::f32::consts::TAU).sin()
            + 0.45 * (across * std::f32::consts::TAU).sin()
            + 0.25 * chop
    };
    // Each grid point once (height, normal, tone), shared by the four
    // quads that meet there: a quarter of the wave sums of working it out
    // per quad corner, for the same surface.
    let side = (2 * cells + 1) as usize;
    let mut points: Vec<(f32, [f32; 3], [f32; 4])> = Vec::with_capacity(side * side);
    for z in -cells..=cells {
        for x in -cells..=cells {
            let (x, z) = (x as f32 * step, z as f32 * step);
            let h = wave(x, z);
            // The slope over the grid's own spacing, so the light runs along
            // the swells the mesh can draw. A slope taken over a metre or two
            // keeps ripples a few times shorter than a grid step: the
            // vertices then catch a low sun in a lattice of bright diamonds
            // (one per vertex) instead of one glittering surface. Averaging
            // over four steps keeps the long, slow undulation the grid draws
            // smoothly and drops the swells only a few steps long.
            let reach = step * NORMAL_REACH_STEPS;
            let dx = (wave(x + reach, z) - wave(x - reach, z)) / (2.0 * reach);
            let dz = (wave(x, z + reach) - wave(x, z - reach)) / (2.0 * reach);
            let normal = Vec3::new(-dx * 0.55 * sea, 1.0, -dz * 0.55 * sea)
                .normalize()
                .to_array();
            let crest = (h * 0.5).max(0.0).powi(2);
            let tone = 0.9 + 0.22 * crest + 0.05 * h;
            points.push((
                -0.11 + h * 0.03 * sea,
                normal,
                [tone, tone * 1.01, tone * 1.02, 1.0],
            ));
        }
    }
    let at = |x: i32, z: i32| points[(z + cells) as usize * side + (x + cells) as usize];
    let mut geometry = Geometry::default();
    for z in -cells..cells {
        for x in -cells..cells {
            let a = x as f32 * step;
            let b = z as f32 * step;
            let corners = [(x, z), (x, z + 1), (x + 1, z + 1), (x + 1, z)];
            let quad = [
                [a, 0.0, b],
                [a, 0.0, b + step],
                [a + step, 0.0, b + step],
                [a + step, 0.0, b],
            ];
            let start = geometry.vertices.len();
            geometry.quad(quad, [0.0, 1.0, 0.0], [1.0; 4]);
            for (offset, (cx, cz)) in corners.into_iter().enumerate() {
                let (y, normal, colour) = at(cx, cz);
                geometry.vertices[start + offset][1] = y;
                geometry.normals[start + offset] = normal;
                geometry.colors[start + offset] = colour;
            }
        }
    }
    geometry.mesh()
}

/// The shallows' tint at view time `time`: lighter and denser as the swell
/// runs up the beach, back to clear water as it draws off.
pub(crate) fn foam_breath(time: f32, strength: f32) -> (f32, f32) {
    let surge = ((time * 0.75).sin() * 0.5 + 0.5).powf(1.6);
    let sea = (0.6 + 0.4 * strength.clamp(0.0, 3.0)).min(1.6);
    (surge * 0.45 * sea, 0.5 + 0.22 * surge * sea)
}

fn ocean_mesh(time: f32, half_extent: f32) -> Mesh {
    let step = half_extent / 24.0;
    let mut geometry = Geometry::default();
    for z in -24..24 {
        for x in -24..24 {
            let a = x as f32 * step;
            let b = z as f32 * step;
            let height = |x: f32, z: f32| {
                -0.11
                    + (x * 0.29 + time * 0.55).sin() * 0.035
                    + (z * 0.31 - time * 0.43).sin() * 0.026
            };
            let points = [
                [a, height(a, b), b],
                [a, height(a, b + step), b + step],
                [a + step, height(a + step, b + step), b + step],
                [a + step, height(a + step, b), b],
            ];
            let start = geometry.vertices.len();
            geometry.quad(points, [0.0, 1.0, 0.0], [1.0; 4]);
            for (offset, [x, _, z]) in points.into_iter().enumerate() {
                // Shared positions receive identical analytic normals/colour.
                // Per-quad normals made a highly visible specular checkerboard.
                geometry.normals[start + offset] = Vec3::new(
                    -0.01015 * (x * 0.29 + time * 0.55).cos(),
                    1.0,
                    -0.00806 * (z * 0.31 - time * 0.43).cos(),
                )
                .normalize()
                .to_array();
                let brightness = 0.985 + 0.015 * (x * 0.07 + z * 0.06 + time * 0.3).sin();
                geometry.colors[start + offset] = [brightness, brightness, brightness, 1.0];
            }
        }
    }
    geometry.mesh()
}

#[allow(clippy::too_many_arguments)]
fn wind_and_water(
    time: Res<Time>,
    (weather, wind): (
        Option<Res<crate::island_world::weather::WeatherLook>>,
        Option<Res<crate::island_world::ambient::Wind>>,
    ),
    mut plants: Query<(&WindPlant, &mut Transform, &GlobalTransform)>,
    mut clouds: Query<(&Cloud, &mut Transform), Without<WindPlant>>,
    water: Query<(&Ocean, &Mesh3d)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut frame: ResMut<WaterFrame>,
    (foam, mut materials): (
        Option<Res<ShoreFoam>>,
        Option<ResMut<Assets<StandardMaterial>>>,
    ),
    (camera, mut turn): (Query<&GlobalTransform, With<Camera3d>>, Local<u32>),
) {
    let Some(wind) = wind else {
        legacy_wind_and_water(
            time.elapsed_secs(),
            weather.as_deref(),
            &mut plants,
            &mut clouds,
            &water,
            &mut meshes,
            &mut frame,
        );
        return;
    };
    // The island's one wind on the view clock: it keeps blowing while the
    // game is paused and is not sped up with it.
    let seconds = wind.time;
    // Plants near the camera sway every frame; far ones, too small to see a
    // frame's difference, in turn, a quarter of them a frame. A plant whose
    // turn does not change (Reduce motion) is not written, so its model is
    // not moved again.
    let near = camera.iter().next().map(|eye| {
        let at = eye.translation();
        Vec2::new(at.x, at.z)
    });
    *turn = turn.wrapping_add(1);
    for (index, (plant, mut transform, placed)) in plants.iter_mut().enumerate() {
        let at = placed.translation();
        let close = near.is_none_or(|eye| {
            eye.distance_squared(Vec2::new(at.x, at.z)) < SWAY_NEAR_M * SWAY_NEAR_M
        });
        if !close && !(index as u32 + *turn).is_multiple_of(SWAY_FAR_EVERY) {
            continue;
        }
        let next = sway(&wind, plant.phase, plant.yaw);
        if transform.rotation != next {
            transform.rotation = next;
        }
    }
    let cover = weather.as_deref().map_or(0.45, |look| {
        crate::island_world::ambient::cloud_cover(look.kind, look.amount)
    });
    for (cloud, mut transform) in &mut clouds {
        let (at, shown) = cloud_drift(cloud, &wind, cover);
        transform.translation = at;
        transform.scale = Vec3::splat(cloud.size * shown);
    }
    if seconds - frame.0 >= 0.10 || seconds < frame.0 {
        frame.0 = seconds;
        for (ocean, handle) in &water {
            if let Some(mut mesh) = meshes.get_mut(&handle.0) {
                *mesh = swell_mesh(seconds, ocean.half_extent, wind.direction, wind.strength);
            }
        }
        // The shore's foam breathes with the swell.
        if let (Some(foam), Some(materials)) = (foam.as_deref(), materials.as_deref_mut())
            && let Some(mut material) = materials.get_mut(&foam.0)
        {
            let (white, alpha) = foam_breath(seconds, wind.strength);
            let mix = |water: f32, froth: f32| water + (froth - water) * white;
            material.base_color = Color::srgba(
                mix(0.42, 0.9),
                mix(0.74, 0.96),
                mix(0.70, 0.95),
                alpha + 0.12,
            );
        }
    }
}

/// A tree's turn in the wind: its own yaw, a lean downwind with the
/// wind's strength, and a sway along the wind at its own phase. The trunk's
/// foot stays put (the model's origin), the crown moves.
pub(crate) fn sway(wind: &crate::island_world::ambient::Wind, phase: f32, yaw: f32) -> Quat {
    let strength = wind.strength;
    let downwind = Vec3::new(wind.direction.x, 0.0, wind.direction.y);
    let axis = Vec3::Y.cross(downwind).normalize_or(Vec3::Z);
    let lean = 0.015 * strength;
    // Each tree at its own pace and phase, so a wood shimmers rather than
    // swaying as one block; visible from 14 to 40 m (a crown 6 m up moves
    // some 25 to 40 cm in a fresh breeze).
    let own = 0.82 + 0.36 * (phase * 0.618).fract();
    let pace = 1.1 * strength.max(0.2).sqrt() * own;
    let swing =
        (wind.time * pace + phase).sin() + 0.4 * (wind.time * pace * 2.3 + phase * 1.7).sin();
    let amplitude = 0.05 * (0.4 + 0.6 * strength.min(2.6));
    Quat::from_axis_angle(axis, lean + swing * amplitude) * Quat::from_rotation_y(yaw)
}

/// Where a cloud is and how much of it shows (0..1): carried by the wind
/// and wrapped round the island, shrinking to nothing near the wrap so it
/// never pops; the clouds past the weather's cover thin away.
fn cloud_drift(
    cloud: &Cloud,
    wind: &crate::island_world::ambient::Wind,
    cover: f32,
) -> (Vec3, f32) {
    let at = crate::island_world::ambient::cloud_at(cloud.anchor, wind, cloud.span);
    let along = (Vec2::new(at.x, at.z).dot(wind.direction) / cloud.span + 0.5).clamp(0.0, 1.0);
    let edge = (along.min(1.0 - along) / 0.12).clamp(0.0, 1.0);
    let edge = edge * edge * (3.0 - 2.0 * edge);
    // The cloud's place in the order the sky fills: the first ones stay,
    // the rest come in as the cover builds.
    let thin = ((cover - cloud.order) / 0.12).clamp(0.0, 1.0);
    (at, edge * thin)
}

fn legacy_wind_and_water(
    seconds: f32,
    weather: Option<&crate::island_world::weather::WeatherLook>,
    plants: &mut Query<(&WindPlant, &mut Transform, &GlobalTransform)>,
    clouds: &mut Query<(&Cloud, &mut Transform), Without<WindPlant>>,
    water: &Query<(&Ocean, &Mesh3d)>,
    meshes: &mut Assets<Mesh>,
    frame: &mut WaterFrame,
) {
    // A storm bends the trees one way and shakes them faster.
    let strength = weather.map_or(1.0, crate::island_world::weather::wind_strength);
    let (sway, lean, pace) = (
        0.018 * strength,
        0.02 * (strength - 1.0),
        0.7 * strength.sqrt(),
    );
    for (plant, mut transform, _) in plants.iter_mut() {
        // The lean is in the world's +x, whichever way the model faces.
        transform.rotation = Quat::from_rotation_z(-lean)
            * Quat::from_rotation_y(plant.yaw)
            * Quat::from_rotation_z((seconds * pace + plant.phase).sin() * sway);
    }
    for (cloud, mut transform) in clouds.iter_mut() {
        transform.translation =
            cloud.anchor + Vec3::new((seconds * 0.018 + cloud.phase).sin() * 18.0, 0.0, 0.0);
    }
    if seconds - frame.0 >= 0.10 {
        frame.0 = seconds;
        for (ocean, handle) in water {
            if let Some(mut mesh) = meshes.get_mut(&handle.0) {
                *mesh = ocean_mesh(seconds, ocean.half_extent);
            }
        }
    }
}

pub(crate) fn elevation(
    terrain: Res<TerrainState>,
    positions: Query<&SimPos>,
    statics: Query<
        (),
        Or<(
            With<VillagePlace>,
            With<ResourcePatch>,
            With<Household>,
            With<npc_framework::prelude::Item>,
            With<npc_framework::prelude::Vehicle>,
            With<island_content::Prop>,
            With<island_content::village::hunting::Wildlife>,
            With<island_content::village::homestead::HomesteadPlot>,
            With<island_content::village::homestead::Cottage>,
            With<island_content::village::civic::ConstructionSite>,
            With<island_content::village::civic::Building>,
        )>,
    >,
    mut roots: Query<(&VisualOf, &mut Transform, Has<crate::visuals::ActorVisual>)>,
) {
    for (of, mut transform, actor) in &mut roots {
        if !actor && !statics.contains(of.0) {
            continue;
        }
        let Ok(_position) = positions.get(of.0) else {
            continue;
        };
        if let Some(cell) = terrain.cell(
            transform.translation.x.floor() as i32,
            transform.translation.z.floor() as i32,
        ) {
            let y = cell.height_mm.max(0) as f32 / 1000.0;
            if actor {
                transform.translation.y += y;
            } else {
                transform.translation.y = y;
            }
        }
    }
}

fn resource_lifecycle(
    index: Res<SimIndex>,
    patches: Query<(
        &ResourcePatch,
        Option<&island_content::village::frontier::ecology::EcologicalPatch>,
    )>,
    parents: Query<&Fogged>,
    mut visuals: Query<(
        &mut ResourceVisual,
        &ChildOf,
        &mut Transform,
        &mut Visibility,
    )>,
) {
    for (mut visual, parent, mut transform, mut visibility) in &mut visuals {
        if parents
            .get(parent.parent())
            .is_ok_and(|fog| fog.0 != Look::Live)
        {
            continue;
        }
        let Some(entity) = index.get(visual.source) else {
            continue;
        };
        let Ok((patch, ecology)) = patches.get(entity) else {
            continue;
        };
        let biomass = ecology.map_or(100_000, |state| state.biomass_milli);
        let growing = ecology.map_or(0, |state| state.pending_growth.len());
        let state = (patch.remaining, biomass, growing);
        if visual.observed == Some(state) {
            continue;
        }
        visual.observed = Some(state);
        let fraction =
            (patch.remaining as f32 / patch.capacity.max(1) as f32).min(biomass as f32 / 100_000.0);
        let visible = match visual.stage {
            0 => patch.kind != "log" || fraction > 0.02,
            1 => fraction > 0.42,
            2 => fraction > 0.72,
            3 => fraction < 0.10,
            stage => growing > usize::from(stage - 4),
        };
        *visibility = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        transform.scale = Vec3::splat(
            visual.full_scale
                * if visual.stage >= 3 {
                    1.0
                } else {
                    0.40 + fraction.sqrt() * 0.60
                },
        );
    }
}

pub(crate) fn resource_details(
    commands: &mut Commands,
    server: &AssetServer,
    root: Entity,
    source: SimId,
    kind: &str,
) {
    if kind != "log" {
        return;
    }
    for (stage, path, offset, scale) in [
        (
            1,
            "living/nature/tree_single_B.gltf",
            Vec3::new(-1.2, 0.0, 0.8),
            2.5,
        ),
        (
            2,
            "living/nature/tree_single_A.gltf",
            Vec3::new(1.1, 0.0, 0.6),
            2.1,
        ),
        (3, "living/nature/tree_single_A_cut.gltf", Vec3::ZERO, 3.0),
        (
            4,
            "living/nature/tree_single_B.gltf",
            Vec3::new(-0.65, 0.0, -0.7),
            0.7,
        ),
        (
            5,
            "living/nature/tree_single_A.gltf",
            Vec3::new(0.60, 0.0, -0.8),
            0.6,
        ),
        (
            6,
            "living/nature/tree_single_B.gltf",
            Vec3::new(0.0, 0.0, -1.2),
            0.8,
        ),
    ] {
        commands.spawn((
            ChildOf(root),
            WorldAssetRoot(scene(server, path)),
            Transform::from_translation(offset).with_scale(Vec3::splat(scale)),
            Visibility::Hidden,
            ResourceVisual {
                source,
                full_scale: scale,
                stage,
                observed: None,
            },
        ));
    }
}

/// Purpose-built silhouettes; selection is a visual interpretation of site role.
pub(crate) fn building(role: Role) -> &'static str {
    match role {
        Role::Smith => "living/buildings/building_blacksmith_blue.gltf",
        Role::Miller => "living/buildings/building_windmill_blue.gltf",
        Role::Woodcutter | Role::Carpenter => "living/buildings/building_lumbermill_blue.gltf",
        Role::Miner => "living/buildings/building_mine_blue.gltf",
        Role::Innkeeper | Role::Baker => "living/buildings/building_tavern_blue.gltf",
        Role::WaterKeeper => "living/buildings/building_well_blue.gltf",
        Role::Farmer | Role::Tailor | Role::Tanner => "living/buildings/building_market_blue.gltf",
        Role::Fisher => "living/buildings/building_watermill_blue.gltf",
        // A trade only the world's data defines: the generic workshop.
        Role::Trade(_) => "living/buildings/building_market_blue.gltf",
        _ => "living/buildings/building_home_B_blue.gltf",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_data_only_trade_gets_the_generic_workshop() {
        let trade = Role::from_key("weaver");
        assert_eq!(
            building(trade),
            "living/buildings/building_market_blue.gltf"
        );
        assert_eq!(
            crate::island_world::workplaces::WorkModel::of(trade),
            Some(crate::island_world::workplaces::WorkModel::Market)
        );
        // An unknown good lies as a crate in a store.
        assert_eq!(
            crate::island_atmosphere::goods::stored_kind("silk_bolt"),
            crate::island_atmosphere::goods::PileKind::Crate
        );
        assert_eq!(
            crate::island_atmosphere::goods::pile_kind(trade),
            crate::island_atmosphere::goods::PileKind::Sack
        );
    }
    use crate::island_world::ambient::{self, AmbientMotion, Wind};
    use island_content::village::frontier::terrain::TerrainCell;

    /// An app with the island's wind and the ambient consumers here: real
    /// time steps of a quarter second, the sim never ticking (paused).
    fn ambient_app(scale: f32) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<WaterFrame>()
            .init_resource::<Wind>()
            .insert_resource(AmbientMotion { scale })
            .insert_resource(SimClock { tick: 4_000 })
            .insert_resource(SimConfig::default())
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(250),
            ))
            .add_systems(PreUpdate, ambient::update_wind)
            .add_systems(Update, (wind_and_water, working_machinery));
        app.world_mut().spawn((
            WindPlant {
                phase: 0.4,
                yaw: 1.0,
            },
            Transform::default(),
        ));
        app.world_mut().spawn((
            Cloud {
                anchor: Vec3::new(10.0, 95.0, -30.0),
                phase: 0.0,
                size: 10.0,
                span: 400.0,
                order: 0.0,
            },
            Transform::default(),
        ));
        app
    }

    fn moved(app: &mut App) -> Vec<Transform> {
        let mut query = app
            .world_mut()
            .query_filtered::<&Transform, Or<(With<WindPlant>, With<Cloud>, With<Machine>)>>();
        query.iter(app.world()).copied().collect()
    }

    #[test]
    fn winter_evergreens_and_shrubs_stay_dark_green_and_broadleaves_and_grass_go_bare() {
        let hsv = |colour: Color| {
            let c = colour.to_srgba();
            let (high, low) = (
                c.red.max(c.green).max(c.blue),
                c.red.min(c.green).min(c.blue),
            );
            ((high - low) / high.max(1e-3), high, c)
        };
        // Pines and shrubs: a dark evergreen, green leading, not grey and
        // not the summer's bright green.
        for (name, shrub) in [
            ("leafsDark", false),
            ("leafsDark", true),
            ("leafsGreen", true),
        ] {
            let (sat, high, c) = hsv(kenney_colour_on(name, Season::Winter, shrub).unwrap());
            assert!(c.green > c.red && c.green > c.blue, "{name}: {c:?}");
            assert!(
                (0.25..0.6).contains(&sat) && high < 0.3,
                "{name}: {sat} {high}"
            );
            assert!(snow_lies_on(name, shrub));
            let (summer, summer_high, _) =
                hsv(kenney_colour_on(name, Season::Summer, shrub).unwrap());
            assert!(summer_high > high && summer >= sat, "{name}");
        }
        // Broadleaves: bare and near grey, no snow on their leaves.
        let (sat, high, _) = hsv(kenney_colour_on("leafsGreen", Season::Winter, false).unwrap());
        assert!(sat < 0.2 && high < 0.35, "{sat} {high}");
        assert!(!snow_lies_on("leafsGreen", false));
        // Grass: dead straw above the snow.
        let (sat, _, straw) = hsv(kenney_colour("grass", Season::Winter).unwrap());
        assert!(
            straw.red > straw.green && straw.green > straw.blue,
            "{straw:?}"
        );
        assert!(sat < 0.4);
    }

    #[test]
    fn clouds_and_trees_move_on_view_time_while_the_sim_is_paused() {
        let mut app = ambient_app(1.0);
        for _ in 0..3 {
            app.update();
        }
        let before = moved(&mut app);
        let tick = app.world().resource::<SimClock>().tick;
        for _ in 0..4 {
            app.update();
        }
        let after = moved(&mut app);
        // The sim did not tick; the trees swayed and the clouds drifted.
        assert_eq!(app.world().resource::<SimClock>().tick, tick);
        assert_eq!(before.len(), 2);
        for (a, b) in before.iter().zip(&after) {
            assert_ne!(a, b, "it moved with view time");
        }
        // The cloud drifted downwind.
        let wind = *app.world().resource::<Wind>();
        let cloud = |t: &[Transform]| t.iter().find(|t| t.translation.y > 50.0).copied();
        let (a, b) = (cloud(&before).unwrap(), cloud(&after).unwrap());
        let along = Vec2::new(
            b.translation.x - a.translation.x,
            b.translation.z - a.translation.z,
        );
        assert!(
            along.dot(wind.direction) > 0.0,
            "{along} {:?}",
            wind.direction
        );
    }

    #[test]
    fn no_ambient_motion_leaves_every_transform_as_it_is() {
        let mut app = ambient_app(0.0);
        app.world_mut().spawn((
            Machine {
                station: SimId(3),
                visual: Entity::PLACEHOLDER,
                axis: Vec3::X,
                spin: Spin {
                    angle: 0.4,
                    speed: MACHINE_SPEED,
                    at: None,
                },
            },
            Transform::default(),
        ));
        app.update();
        let before = moved(&mut app);
        for _ in 0..6 {
            app.world_mut().resource_mut::<SimClock>().tick += 7;
            app.update();
        }
        assert_eq!(before, moved(&mut app));
        assert_eq!(app.world().resource::<Wind>().time, 0.0);
    }

    #[test]
    fn one_wind_for_every_reader_in_a_frame() {
        // The wind is one resource written once a frame, before anything
        // reads it: every consumer (trees, clouds, smoke, rain, the boat,
        // lights, birds, shadows) takes `Res<Wind>`, none keeps its own.
        let mut app = ambient_app(1.0);
        app.update();
        app.update();
        let wind = *app.world().resource::<Wind>();
        // A tree's turn is the one wind's: recomputed from the resource, it
        // matches what was drawn.
        let mut plants = app.world_mut().query::<(&WindPlant, &Transform)>();
        for (plant, transform) in plants.iter(app.world()) {
            assert_eq!(transform.rotation, sway(&wind, plant.phase, plant.yaw));
        }
        // The smoke bends the same way the trees lean.
        let bend = crate::island_atmosphere::smoke::Bend::of(&wind).apply(Vec3::new(1.0, 2.0, 0.0));
        let downwind = Vec3::new(wind.direction.x, 0.0, wind.direction.y);
        assert!(Vec3::new(bend.x, 0.0, bend.z).normalize().dot(downwind) > 0.999);
        // The trees sway along the same wind: the crown moves only in the
        // plane of the wind, never across it.
        for step in 0..20 {
            let at = Wind {
                time: step as f32 * 0.3,
                ..wind
            };
            let top = sway(&at, 0.7, 0.0) * Vec3::Y;
            let across = Vec3::Y.cross(downwind);
            assert!(top.dot(across).abs() < 1e-4, "{top}");
        }
    }

    #[test]
    fn scenery_keeps_off_roads_doors_bodies_and_fishing_banks_and_is_stable() {
        let app = island_content::new_sim_app(7, "living_world");
        let world = app.world();
        let spec = world.resource::<LivingWorldSpec>();
        let terrain = world.resource::<TerrainState>();
        let paths = ground::Roads::from_spec(spec);
        let landmarks = crate::island_atmosphere::landmark::landmark_sites(spec, terrain);
        let fishing = vec![spec.settlements[0].center];
        let mut placed = Vec::new();
        for (coord, cell) in terrain.iter() {
            if cell.surface != TerrainSurface::Grass {
                continue;
            }
            let p = SimPos::new(coord.x * 1000 + 500, coord.z * 1000 + 500);
            if scenery_blocked(spec, &landmarks, &fishing, &paths, p, 9_000_000, 16_000_000) {
                continue;
            }
            let at = Vec2::new(p.x as f32 / 1000.0, p.z as f32 / 1000.0);
            // Off the paths, the doors, the bodies and the bank.
            assert!(paths.clear_of(at, 0.8), "{at}");
            assert!(!crate::island_homes::model::on_founding_body(
                spec,
                p,
                HOUSE_CLEAR_M
            ));
            for settlement in &spec.settlements {
                for home in &settlement.homes {
                    assert!(home.door.dist2(p) >= 9_000_000, "{at} by a door");
                }
            }
            assert!(fishing[0].dist2(p) >= FISHING_BANK_MM * FISHING_BANK_MM);
            let h = hash(coord.x, coord.z);
            placed.push((
                coord.x,
                coord.z,
                forest_tree(cell.biome, h),
                undergrowth_for(cell.biome, h),
            ));
        }
        assert!(placed.len() > 1000, "{}", placed.len());
        // The same world gets the same woods.
        let again: Vec<_> = placed
            .iter()
            .map(|(x, z, _, _)| {
                let cell = terrain.cell(*x, *z).expect("cell");
                let h = hash(*x, *z);
                (
                    *x,
                    *z,
                    forest_tree(cell.biome, h),
                    undergrowth_for(cell.biome, h),
                )
            })
            .collect();
        assert_eq!(placed, again);
        // A mixed wood: every tree of the forest is used.
        let kinds: std::collections::BTreeSet<_> = placed.iter().map(|row| row.2).collect();
        assert_eq!(kinds.len(), 6, "{kinds:?}");
        // No plain cones: none of the kit's smooth firs and clusters.
        assert!(kinds.iter().all(|kind| *kind >= 6), "{kinds:?}");
    }

    #[test]
    fn a_signpost_stands_beside_the_road_in_facing_the_arrivals() {
        let (centre, start) = (Vec2::new(10.0, 10.0), Vec2::new(10.0, -40.0));
        let (at, yaw) = signpost(centre, start).expect("a road in");
        // 13 m out along the road, a little to its side.
        let along = (at - centre).dot(Vec2::NEG_Y);
        assert!((along - 13.0).abs() < 1e-4, "{at}");
        assert!(((at - centre).x.abs() - 2.2).abs() < 1e-4, "{at}");
        // The board's +Z looks out towards the arrivals.
        let facing = Quat::from_rotation_y(yaw) * Vec3::Z;
        assert!(facing.z < -0.99, "{facing}");
        assert_eq!(signpost(centre, centre), None);
    }

    #[test]
    fn every_kenney_nature_material_is_recoloured() {
        // The material names in the Kenney models the island draws, read
        // from the files: each gets the island's colour in every season.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut names = std::collections::BTreeSet::new();
        for path in FOREST
            .iter()
            .chain(UNDERGROWTH.iter())
            .filter(|scenery| scenery.kenney)
            .map(|scenery| scenery.path)
        {
            let bytes = std::fs::read(root.join(path)).expect(path);
            let length = u32::from_le_bytes(bytes[12..16].try_into().expect("glb")) as usize;
            let json: serde_json::Value =
                serde_json::from_slice(&bytes[20..20 + length]).expect("glb json");
            for material in json["materials"].as_array().into_iter().flatten() {
                names.insert(material["name"].as_str().unwrap_or_default().to_string());
            }
        }
        assert!(!names.is_empty());
        for season in [
            Season::Spring,
            Season::Summer,
            Season::Autumn,
            Season::Winter,
        ] {
            for name in &names {
                assert!(
                    kenney_colour(name, season).is_some(),
                    "{name} has no island colour"
                );
            }
        }
        // The teal greens are gone: a summer leaf is a warm green.
        let leaf = kenney_colour("leafsGreen", Season::Summer)
            .expect("leaf")
            .to_srgba();
        assert!(leaf.green > leaf.blue + 0.2 && leaf.red > 0.25, "{leaf:?}");
    }

    #[test]
    fn machines_turn_only_while_worked_and_stop_with_the_sim() {
        let mut spin = Spin::default();
        spin.advance(100.0, true, 1.0);
        // The sim paused: the same sim time, nothing turns.
        for _ in 0..10 {
            spin.advance(100.0, true, 1.0);
        }
        assert_eq!(spin.angle, 0.0);
        // Worked and running: it comes up to pace.
        let mut last = spin.angle;
        for tick in 1..=200 {
            spin.advance(100.0 + tick as f32, true, 1.0);
            assert_ne!(spin.angle, last);
            last = spin.angle;
        }
        assert!((spin.speed - MACHINE_SPEED).abs() < MACHINE_SPEED * 0.15);
        // Nobody working: it runs down and stops, not at once.
        spin.advance(301.0, false, 1.0);
        assert!(spin.speed > 0.0, "a spin-down, not a stop");
        for tick in 302..700 {
            spin.advance(tick as f32, false, 1.0);
        }
        assert_eq!(spin.speed, 0.0);
        let stopped = spin.angle;
        spin.advance(800.0, false, 1.0);
        assert_eq!(spin.angle, stopped);
    }
    fn plateau(height_mm: i32) -> TerrainState {
        TerrainState {
            width: 64,
            height: 64,
            cells: vec![
                TerrainCell {
                    height_mm,
                    surface: TerrainSurface::Grass,
                    biome: Biome::Ridge
                };
                4096
            ],
            revision: 0,
        }
    }

    fn terrain_view(terrain: TerrainState) -> App {
        let mut app = App::new();
        let season = island_content::village::calendar_at(0, &SimConfig::default(), true).1;
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<SimClock>()
            .init_resource::<SimConfig>()
            .insert_resource(TerrainStyle { season })
            .insert_resource(terrain)
            .add_systems(
                Startup,
                |mut commands: Commands,
                 terrain: Res<TerrainState>,
                 mut meshes: ResMut<Assets<Mesh>>,
                 style: Res<TerrainStyle>| {
                    spawn_terrain_chunks(
                        &mut commands,
                        &terrain,
                        &ground::Roads::default(),
                        &mut meshes,
                        &Handle::default(),
                        style.season,
                    );
                },
            )
            .add_systems(Update, sync_terrain);
        app
    }

    fn update_terrain_view(app: &mut App) {
        let terrain = app.world().resource::<TerrainState>().clone();
        let clock = *app.world().resource::<SimClock>();
        app.update();
        assert_eq!(*app.world().resource::<TerrainState>(), terrain);
        assert_eq!(*app.world().resource::<SimClock>(), clock);
        assert!(
            app.world()
                .resource::<Assets<Mesh>>()
                .iter()
                .all(|(_, mesh)| mesh.count_vertices() > 0)
        );
    }

    #[test]
    fn water_chunks_keep_their_identity_without_allocating_empty_meshes() {
        let mut terrain = plateau(-200);
        for cell in &mut terrain.cells {
            cell.surface = TerrainSurface::Water;
        }
        let dry_index = terrain
            .bounds()
            .index(npc_framework::prelude::CellCoord::new(0, 0))
            .unwrap() as usize;
        terrain.cells[dry_index].surface = TerrainSurface::Grass;
        terrain.cells[dry_index].height_mm = 1000;
        let mut app = terrain_view(terrain);
        update_terrain_view(&mut app);
        let world = app.world_mut();
        let mut chunks = world.query::<(&TerrainChunk, Option<&Mesh3d>)>();
        assert_eq!(chunks.iter(world).count(), 16);
        // The islet's own chunk and the three whose water cells touch it.
        let mut with_mesh: Vec<(i32, i32, Handle<Mesh>)> = chunks
            .iter(world)
            .filter_map(|(chunk, mesh)| Some((chunk.x, chunk.z, mesh?.0.clone())))
            .collect();
        with_mesh.sort_by_key(|(x, z, _)| (*x, *z));
        let meshes = world.resource::<Assets<Mesh>>();
        assert_eq!(meshes.len(), 4);
        let vertices: Vec<(i32, i32, usize)> = with_mesh
            .iter()
            .map(|(x, z, handle)| (*x, *z, meshes.get(handle).unwrap().count_vertices()))
            .collect();
        // One dry top and its four shoreline banks, plus a beach of two
        // triangles on each of the eight water cells around it.
        assert_eq!(
            vertices,
            [(-16, -16, 6), (-16, 0, 12), (0, -16, 12), (0, 0, 20 + 18)]
        );
    }

    /// An inland wetland pond is drawn as still, dark water over all its
    /// cells; the sea's shallows by the same land keep their pale, fading
    /// shore tint.
    #[test]
    fn a_wetland_pond_is_dark_still_water_not_the_seas_shallows() {
        let mut terrain = plateau(150);
        // A 6 x 6 pond in the land, and the sea west of x = -20.
        for index in 0..4096_i32 {
            let (x, z) = (index % 64 - 32, index / 64 - 32);
            let cell = &mut terrain.cells[index as usize];
            if (0..6).contains(&x) && (0..6).contains(&z) {
                cell.surface = TerrainSurface::Water;
                cell.biome = Biome::Wetland;
                cell.height_mm = -200;
            } else if x < -20 {
                cell.surface = TerrainSurface::Water;
                cell.biome = Biome::Coast;
                cell.height_mm = -700;
            }
        }
        let colours = |x: i32, z: i32| -> Vec<[f32; 4]> {
            let mesh = shore_mesh(&terrain, x, z).expect("water here");
            let Some(bevy::mesh::VertexAttributeValues::Float32x4(colours)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                panic!("shore colours")
            };
            colours.clone()
        };
        let pond = colours(0, 0);
        assert_eq!(pond.len(), 36 * 4, "every pond cell is drawn");
        assert!(pond.iter().all(|colour| *colour == POND_TINT));
        let shore = colours(-32, 0);
        assert!(shore.iter().all(|colour| colour[0] == 1.0));
        assert!(
            shore.iter().any(|colour| colour[3] < 1.0),
            "the shore fades out"
        );
    }

    #[test]
    fn the_beach_fills_stair_steps_and_leaves_the_land_mesh_first() {
        // Land west of a diagonal coast, stepping one cell per row.
        let mut terrain = plateau(800);
        terrain.width = 16;
        terrain.height = 16;
        terrain.cells.truncate(256);
        for index in 0..256_i32 {
            let (x, z) = (index % 16 - 8, index / 16 - 8);
            if x >= z {
                terrain.cells[index as usize].surface = TerrainSurface::Water;
                terrain.cells[index as usize].height_mm = -700;
            }
        }
        let mesh =
            terrain_mesh(&terrain, &ground::Roads::default(), -8, -8, Season::Summer).unwrap();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("terrain positions")
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("terrain normals")
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("terrain grain coordinates")
        };
        assert_eq!(uvs.len(), positions.len());
        // The first vertex is still the first land cell's top, at its height.
        assert_eq!(positions[0][1], 0.8);
        // Beach vertices reach below the water and never above the land.
        let foot = positions
            .iter()
            .filter(|p| p[1] == ground::BEACH_FOOT)
            .count();
        assert!(foot > 10, "{foot} vertices at the beach's foot");
        // The inner corner of a stair step, the water cell (0, 0): land lies
        // west and north of it, so three corners sit at land height and one
        // at the foot, and one of its two triangles is level sand.
        let in_cell = |p: &[f32; 3]| p[0] >= 0.0 && p[0] <= 1.0 && p[2] >= 0.0 && p[2] <= 1.0;
        let beach: Vec<usize> = (0..positions.len())
            .filter(|i| in_cell(&positions[*i]) && normals[*i][1] > 0.3 && normals[*i][1] < 1.0)
            .collect();
        assert!(beach.len() >= 3, "a slanted face fills the step");
        for i in &beach {
            let normal = Vec3::from_array(normals[*i]);
            assert!((normal.length() - 1.0).abs() < 1e-4);
            // It faces the open water to the south-east... or the north-east.
            assert!(normal.x > 0.0, "{normal:?}");
        }
        // No change to the cells the simulation knows.
        assert_eq!(terrain.cell(0, 0).unwrap().surface, TerrainSurface::Water);
    }

    #[test]
    fn terrain_chunk_survives_land_water_land_edits_and_season_changes() {
        let mut terrain = plateau(-200);
        terrain.width = 16;
        terrain.height = 16;
        terrain.cells.truncate(256);
        for cell in &mut terrain.cells {
            cell.surface = TerrainSurface::Water;
        }
        let mut app = terrain_view(terrain);
        update_terrain_view(&mut app);
        let chunk = app
            .world_mut()
            .query_filtered::<Entity, With<TerrainChunk>>()
            .single(app.world())
            .unwrap();
        assert!(app.world().get::<Mesh3d>(chunk).is_none());
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);
        let material = app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(chunk)
            .unwrap()
            .clone();

        let dry_index = app
            .world()
            .resource::<TerrainState>()
            .bounds()
            .index(npc_framework::prelude::CellCoord::new(0, 0))
            .unwrap() as usize;
        {
            let mut terrain = app.world_mut().resource_mut::<TerrainState>();
            terrain.cells[dry_index].surface = TerrainSurface::Grass;
            terrain.cells[dry_index].height_mm = 1000;
            terrain.revision += 1;
        }
        update_terrain_view(&mut app);
        let land = app.world().get::<Mesh3d>(chunk).unwrap().clone();
        let autumn_mesh = app
            .world()
            .resource::<Assets<Mesh>>()
            .get(&land.0)
            .unwrap()
            .clone();
        app.world_mut().resource_mut::<SimClock>().tick = 2 * SimConfig::default().day_length;
        update_terrain_view(&mut app);
        assert_eq!(app.world().get::<Mesh3d>(chunk), Some(&land));
        let winter_mesh = app.world().resource::<Assets<Mesh>>().get(&land.0).unwrap();
        assert_eq!(
            autumn_mesh.attribute(Mesh::ATTRIBUTE_POSITION),
            winter_mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        assert_ne!(
            autumn_mesh.attribute(Mesh::ATTRIBUTE_COLOR),
            winter_mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        );

        app.world_mut().entity_mut(chunk).insert(Aabb::default());
        {
            let mut terrain = app.world_mut().resource_mut::<TerrainState>();
            terrain.cells[dry_index].surface = TerrainSurface::Water;
            terrain.cells[dry_index].height_mm = -200;
            terrain.revision += 1;
        }
        update_terrain_view(&mut app);
        assert!(app.world().get::<Mesh3d>(chunk).is_none());
        assert!(app.world().get::<Aabb>(chunk).is_none());
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);
        app.world_mut().resource_mut::<SimClock>().tick = 9 * SimConfig::default().day_length;
        update_terrain_view(&mut app);
        assert!(app.world().get::<Mesh3d>(chunk).is_none());
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);
        {
            let mut terrain = app.world_mut().resource_mut::<TerrainState>();
            terrain.cells[dry_index].surface = TerrainSurface::Grass;
            terrain.cells[dry_index].height_mm = 12000;
            terrain.revision += 1;
        }
        update_terrain_view(&mut app);
        let renewed = app.world().get::<Mesh3d>(chunk).unwrap();
        let meshes = app.world().resource::<Assets<Mesh>>();
        assert_eq!(meshes.len(), 1);
        let mesh = meshes.get(&renewed.0).unwrap();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("terrain positions")
        };
        assert_eq!(positions[0], [0.0, 12.0, 0.0]);
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("terrain colors")
        };
        // The lone cell's corners are its own blend: mostly beach here.
        let patch = ground::Patch::around(
            app.world().resource::<TerrainState>(),
            &ground::Roads::default(),
            -8,
            -8,
            CHUNK,
        );
        assert_eq!(
            colors[0],
            ground::corner_color(&patch, 0, 0, Season::Spring)
        );
        assert_eq!(
            colors[2],
            ground::corner_color(&patch, 1, 1, Season::Spring)
        );
        assert_eq!(
            app.world().get::<MeshMaterial3d<StandardMaterial>>(chunk),
            Some(&material)
        );
        assert_eq!(
            app.world().get::<Transform>(chunk).unwrap().translation,
            Vec3::ZERO
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<TerrainChunk>>()
                .iter(app.world())
                .collect::<Vec<_>>(),
            vec![chunk]
        );
    }

    #[test]
    fn wildlife_and_dropped_goods_follow_rendered_ground_without_revealing_hidden_positions() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, elevation);
        let mut terrain = plateau(4700);
        let seen_index = terrain
            .bounds()
            .index(npc_framework::prelude::CellCoord::new(-10, -10))
            .unwrap() as usize;
        terrain.cells[seen_index].height_mm = 200;
        app.insert_resource(terrain);
        let deer = app
            .world_mut()
            .spawn((
                SimPos::new(0, 0),
                island_content::village::hunting::Wildlife { habitat: SimId(7) },
            ))
            .id();
        let item = app
            .world_mut()
            .spawn((
                SimPos::new(10000, 10000),
                npc_framework::prelude::Item::new("log"),
            ))
            .id();
        let deer_visual = app
            .world_mut()
            .spawn((VisualOf(deer), Transform::default()))
            .id();
        let item_visual = app
            .world_mut()
            .spawn((VisualOf(item), Transform::from_xyz(-9.5, 0.0, -9.5)))
            .id();
        app.update();
        assert!(
            (app.world()
                .get::<Transform>(deer_visual)
                .unwrap()
                .translation
                .y
                - 4.7)
                .abs()
                < 0.0001
        );
        assert_eq!(
            app.world()
                .get::<Transform>(item_visual)
                .unwrap()
                .translation,
            Vec3::new(-9.5, 0.2, -9.5)
        );
        assert_eq!(
            *app.world().get::<SimPos>(item).unwrap(),
            SimPos::new(10000, 10000)
        );
    }
    #[test]
    fn restored_player_cottage_is_not_claimed_by_settlement_home_renderer() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<WorldAsset>()
            .init_resource::<VisualIndex>()
            .insert_resource(plateau(4700))
            .add_systems(Startup, spawn_homes);
        let household = || Household {
            label: "Home".into(),
            capacity: 2,
            residents: vec![],
            door: SimPos::new(0, 0),
            heated_until: 0,
            logs_burned: 0,
        };
        let authored = app
            .world_mut()
            .spawn((SimId(1), SimPos::new(0, 0), household()))
            .id();
        let cottage = app
            .world_mut()
            .spawn((
                SimId(2),
                SimPos::new(0, 0),
                household(),
                island_content::village::homestead::Cottage {
                    plot: SimId(3),
                    plot_id: "plot".into(),
                },
            ))
            .id();
        let cottage_visual = app.world_mut().spawn(Transform::default()).id();
        app.world_mut()
            .resource_mut::<VisualIndex>()
            .map
            .insert(cottage, cottage_visual);
        app.update();
        let index = app.world().resource::<VisualIndex>();
        assert!(index.map.contains_key(&authored));
        assert_eq!(index.map.get(&cottage), Some(&cottage_visual));
    }
    #[test]
    fn captions_cover_the_raised_surface() {
        let terrain = plateau(4700);
        let point = raised_point(Some(&terrain), SimPos::new(500, 500), 2.1);
        assert!((point.y - 6.8).abs() < 0.0001);
    }
    /// A low sun shows any normal that jumps from one swell vertex to the
    /// next as a lattice of bright diamonds. Neighbouring normals must turn
    /// gently, in any wind direction and on a small or large sea (7.7° with
    /// the slope taken over 3 m; under 4° over four grid steps).
    #[test]
    fn the_swell_turns_gently_from_vertex_to_vertex_so_a_low_sun_draws_no_lattice() {
        for half_extent in [246.0, 352.0] {
            for downwind in [Vec2::X, Vec2::new(0.6, 0.8), Vec2::new(-0.7, 0.7)] {
                for time in [0.0, 3.0, 7.7] {
                    let mesh = swell_mesh(time, half_extent, downwind, 1.0);
                    let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
                        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    else {
                        panic!("swell positions")
                    };
                    let Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)) =
                        mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
                    else {
                        panic!("swell normals")
                    };
                    let step = half_extent / 32.0;
                    let key =
                        |p: &[f32; 3]| ((p[0] / step).round() as i32, (p[2] / step).round() as i32);
                    let at: std::collections::BTreeMap<(i32, i32), Vec3> = positions
                        .iter()
                        .zip(normals)
                        .map(|(p, n)| (key(p), Vec3::from_array(*n)))
                        .collect();
                    let mut worst = 0.0_f32;
                    for (&(x, z), normal) in &at {
                        for next in [(x + 1, z), (x, z + 1)] {
                            if let Some(other) = at.get(&next) {
                                worst = worst.max(normal.angle_between(*other).to_degrees());
                            }
                        }
                    }
                    assert!(
                        worst < 4.0,
                        "{half_extent} {downwind} {time}: neighbours {worst:.2}°"
                    );
                }
            }
        }
    }

    #[test]
    fn seasons_recolour_without_changing_topology_and_large_oceans_stay_bounded() {
        let terrain = plateau(4700);
        let summer =
            terrain_mesh(&terrain, &ground::Roads::default(), 0, 0, Season::Summer).unwrap();
        let winter =
            terrain_mesh(&terrain, &ground::Roads::default(), 0, 0, Season::Winter).unwrap();
        assert_eq!(summer.count_vertices(), winter.count_vertices());
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(summer_colors)) =
            summer.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("summer colors")
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(winter_colors)) =
            winter.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("winter colors")
        };
        assert_ne!(summer_colors, winter_colors);
        let small = ocean_mesh(0.0, 224.0);
        let large = ocean_mesh(0.0, 352.0);
        assert_eq!(small.count_vertices(), large.count_vertices());
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
            large.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("water positions")
        };
        assert!(points.iter().any(|point| point[0] > 256.0));
        assert!(points.iter().any(|point| point[2] < -256.0));
    }
    #[test]
    fn gentle_dry_risers_share_lighting_without_moving_their_geometry() {
        let cells = vec![
            TerrainCell {
                height_mm: 200,
                surface: TerrainSurface::Grass,
                biome: Biome::Ridge,
            },
            TerrainCell {
                height_mm: 0,
                surface: TerrainSurface::Grass,
                biome: Biome::Ridge,
            },
        ];
        let terrain = TerrainState {
            width: 2,
            height: 1,
            cells,
            revision: 0,
        };
        let mesh =
            terrain_mesh(&terrain, &ground::Roads::default(), -1, 0, Season::Summer).unwrap();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("terrain positions")
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("terrain normals")
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("terrain colors")
        };
        assert_eq!(points.len(), 36);
        assert_eq!(
            &points[8..12],
            &[
                [0.0, 0.2, 1.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 0.0],
                [0.0, 0.2, 0.0]
            ]
        );
        assert_eq!(normals[8], normals[0]);
        assert!(normals[8][1] > 0.94);
        // The riser continues the colours of the top edge it hangs from.
        assert_eq!(colors[8], colors[2]);
        assert_eq!(colors[9], colors[2]);
        assert_eq!(colors[10], colors[3]);
        assert_eq!(colors[11], colors[3]);
        assert_eq!(terrain.cells[0].height_mm, 200);
        assert_eq!(terrain.cells[1].height_mm, 0);
    }

    #[test]
    fn shoreline_banks_and_large_cliffs_keep_hard_face_lighting() {
        for (height, surface) in [(200, TerrainSurface::Water), (600, TerrainSurface::Grass)] {
            let terrain = TerrainState {
                width: 2,
                height: 1,
                cells: vec![
                    TerrainCell {
                        height_mm: height,
                        surface: TerrainSurface::Grass,
                        biome: Biome::Ridge,
                    },
                    TerrainCell {
                        height_mm: 0,
                        surface,
                        biome: Biome::Ridge,
                    },
                ],
                revision: 0,
            };
            let mesh =
                terrain_mesh(&terrain, &ground::Roads::default(), -1, 0, Season::Summer).unwrap();
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("terrain normals")
            };
            let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                panic!("terrain colors")
            };
            assert_eq!(normals[8], Vec3::X.to_array());
            assert_eq!(
                colors[8],
                [
                    colors[2][0] * CLIFF_SHADE,
                    colors[2][1] * CLIFF_SHADE,
                    colors[2][2] * CLIFF_SHADE,
                    1.0
                ]
            );
        }
    }

    #[test]
    fn gentle_slope_normals_follow_the_height_gradient_and_remain_upward() {
        let mut terrain = plateau(1000);
        for z in -2..=2 {
            for x in -2..=2 {
                let index = terrain
                    .bounds()
                    .index(npc_framework::prelude::CellCoord::new(x, z))
                    .unwrap() as usize;
                terrain.cells[index].height_mm = 1000 + x * 100;
            }
        }
        let normal = Vec3::from_array(terrain_surface_normal(&terrain, 0, 0));
        assert!(normal.x < -0.09 && normal.x > -0.11);
        assert!(normal.y > 0.99);
        assert!(normal.z.abs() < 0.0001);
    }

    #[test]
    fn adjacent_water_quads_share_smooth_normals_and_colour() {
        let mesh = ocean_mesh(3.0, 192.0);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("water normals")
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("water colors")
        };
        assert_eq!(normals[3], normals[4]);
        assert_eq!(normals[2], normals[5]);
        assert_eq!(colors[3], colors[4]);
        assert_eq!(colors[2], colors[5]);
    }
    #[test]
    fn ecology_visual_resolves_stable_identity_after_world_replacement() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SimIndex>()
            .add_systems(Update, resource_lifecycle);
        let patch = |remaining| ResourcePatch {
            kind: "grain".into(),
            remaining,
            capacity: 100,
            replenish_at: 0,
            gather_ticks: 1,
        };
        let old = app.world_mut().spawn(patch(100)).id();
        let id = SimId(42);
        app.world_mut()
            .resource_mut::<SimIndex>()
            .map
            .insert(id, old);
        let root = app.world_mut().spawn(Fogged::default()).id();
        let visual = app
            .world_mut()
            .spawn((
                ChildOf(root),
                ResourceVisual::new(id, 2.0),
                Transform::default(),
                Visibility::Inherited,
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Transform>(visual).unwrap().scale,
            Vec3::splat(2.0)
        );
        app.world_mut().despawn(old);
        let replacement = app.world_mut().spawn(patch(0)).id();
        app.world_mut()
            .resource_mut::<SimIndex>()
            .map
            .insert(id, replacement);
        app.update();
        assert_eq!(
            app.world().get::<Transform>(visual).unwrap().scale,
            Vec3::splat(0.8)
        );
    }
    #[test]
    fn edited_cell_invalidates_only_touching_chunk_signatures() {
        let cell = TerrainCell {
            height_mm: 0,
            surface: TerrainSurface::Grass,
            biome: Biome::Meadow,
        };
        let mut terrain = TerrainState {
            width: 64,
            height: 64,
            cells: vec![cell; 4096],
            revision: 0,
        };
        let near = chunk_signature(&terrain, 0, 0);
        let far = chunk_signature(&terrain, -32, -32);
        let i = terrain
            .bounds()
            .index(npc_framework::prelude::CellCoord::new(5, 5))
            .unwrap() as usize;
        terrain.cells[i].height_mm = 500;
        assert_ne!(near, chunk_signature(&terrain, 0, 0));
        assert_eq!(far, chunk_signature(&terrain, -32, -32));
        // The blended colours read a little past the chunk, and no further.
        let west = chunk_signature(&terrain, -16, 0);
        let i = terrain
            .bounds()
            .index(npc_framework::prelude::CellCoord::new(
                ground::NEAR_MARGIN - 1,
                5,
            ))
            .unwrap() as usize;
        terrain.cells[i].height_mm = 700;
        assert_ne!(west, chunk_signature(&terrain, -16, 0));
        let west = chunk_signature(&terrain, -16, 0);
        let i = terrain
            .bounds()
            .index(npc_framework::prelude::CellCoord::new(
                ground::NEAR_MARGIN,
                5,
            ))
            .unwrap() as usize;
        terrain.cells[i].height_mm = 700;
        assert_eq!(west, chunk_signature(&terrain, -16, 0));
        // A biome is read from further away, for the feathered borders.
        let i = terrain
            .bounds()
            .index(npc_framework::prelude::CellCoord::new(
                ground::MARGIN - 1,
                5,
            ))
            .unwrap() as usize;
        terrain.cells[i].biome = Biome::Woodland;
        assert_ne!(west, chunk_signature(&terrain, -16, 0));
    }

    #[test]
    fn neighbouring_chunks_agree_on_the_colours_of_their_shared_edge() {
        let mut terrain = plateau(1000);
        for (i, cell) in terrain.cells.iter_mut().enumerate() {
            cell.surface = match i % 7 {
                0 | 1 => TerrainSurface::Soil,
                2 => TerrainSurface::Stone,
                _ => TerrainSurface::Grass,
            };
            if i % 5 == 0 {
                cell.biome = Biome::Meadow;
            }
        }
        let colours = |mesh: &Mesh| {
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("terrain positions")
            };
            let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                panic!("terrain colors")
            };
            points
                .iter()
                .zip(colors)
                .filter(|(point, _)| point[0] == 0.0 && point[1] == 1.0)
                .map(|(point, color)| (point[2] as i32, color.map(f32::to_bits)))
                .collect::<std::collections::BTreeSet<_>>()
        };
        let west = colours(
            &terrain_mesh(&terrain, &ground::Roads::default(), -16, 0, Season::Summer).unwrap(),
        );
        let east = colours(
            &terrain_mesh(&terrain, &ground::Roads::default(), 0, 0, Season::Summer).unwrap(),
        );
        assert_eq!(west.len(), 17, "one colour per shared corner");
        assert_eq!(west, east);
    }

    #[test]
    fn a_quad_is_creased_along_its_colour_edge() {
        let (dark, light) = ([0.1, 0.1, 0.1, 1.0], [0.8, 0.8, 0.8, 1.0]);
        // Corners 0 and 2 alike, 1 and 3 not: crease from 0 to 2.
        assert!(split_first_diagonal([light, dark, light, light]));
        // Corners 1 and 3 alike, 0 and 2 not: crease from 1 to 3.
        assert!(!split_first_diagonal([light, dark, dark, dark]));
        assert!(!split_first_diagonal([dark, light, light, light]));
        // A flat or evenly split quad keeps the first diagonal.
        assert!(split_first_diagonal([dark; 4]));
        assert!(split_first_diagonal([dark, light, dark, light]));
        let points = [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
        ];
        for colors in [[light, dark, light, light], [light, dark, dark, dark]] {
            let mut geometry = Geometry::default();
            geometry.blended_quad(points, [0.0, 1.0, 0.0], colors);
            assert_eq!(geometry.vertices.len(), 4);
            // Two triangles, both wound the same way (facing up), covering
            // the whole quad.
            let area: f32 = geometry
                .indices
                .chunks(3)
                .map(|triangle| {
                    let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(points[triangle[i] as usize]));
                    let normal = (b - a).cross(c - a);
                    assert!(normal.y > 0.0, "a triangle faces down");
                    normal.y * 0.5
                })
                .sum();
            assert!((area - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn shallows_follow_the_shore_and_are_rebuilt_only_when_it_moves() {
        let mut terrain = plateau(-200);
        for cell in &mut terrain.cells {
            cell.surface = TerrainSurface::Water;
        }
        assert!(
            shore_mesh(&terrain, 0, 0).is_none(),
            "open water has no band"
        );
        let dry = terrain
            .bounds()
            .index(npc_framework::prelude::CellCoord::new(4, 4))
            .unwrap() as usize;
        terrain.cells[dry].surface = TerrainSurface::Grass;
        terrain.cells[dry].height_mm = 400;
        let mesh = shore_mesh(&terrain, 0, 0).expect("a band around the islet");
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("shore positions")
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("shore colors")
        };
        // Water cells only, below the land and above the waves.
        assert!(points.iter().all(|point| point[1] == SHALLOWS_Y));
        assert!(
            !points
                .chunks(4)
                .any(|quad| quad[0][0] == 4.0 && quad[0][2] == 4.0)
        );
        assert!(colors.iter().any(|color| color[3] > 0.0));
        assert!(colors.iter().all(|color| (0.0..=1.0).contains(&color[3])));
        // Far from the islet nothing is drawn.
        assert!(points.iter().all(|point| (point[0] - 4.5).abs() < 5.0));

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .insert_resource(terrain)
            .add_systems(
                Startup,
                |mut commands: Commands,
                 terrain: Res<TerrainState>,
                 mut meshes: ResMut<Assets<Mesh>>| {
                    spawn_shore_chunks(&mut commands, &terrain, &mut meshes, &Handle::default());
                },
            )
            .add_systems(Update, sync_shore);
        app.update();
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 1);
        {
            let mut terrain = app.world_mut().resource_mut::<TerrainState>();
            terrain.cells[dry].surface = TerrainSurface::Water;
            terrain.cells[dry].height_mm = -200;
            terrain.revision += 1;
        }
        app.update();
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);
        let world = app.world_mut();
        let mut chunks = world.query::<(&ShoreChunk, Option<&Mesh3d>)>();
        assert_eq!(chunks.iter(world).count(), 16);
        assert!(chunks.iter(world).all(|(_, mesh)| mesh.is_none()));
    }
    #[test]
    fn pointer_hits_raised_cell_and_rejects_outside_map() {
        let cell = TerrainCell {
            height_mm: 5000,
            surface: TerrainSurface::Grass,
            biome: Biome::Ridge,
        };
        let terrain = TerrainState {
            width: 64,
            height: 64,
            cells: vec![cell; 4096],
            revision: 0,
        };
        let hit = raycast(&terrain, Ray3d::new(Vec3::new(3.5, 20.0, 4.5), Dir3::NEG_Y)).unwrap();
        assert!((hit.y - 5.0).abs() < 0.0001);
        assert!(
            raycast(
                &terrain,
                Ray3d::new(Vec3::new(80.0, 20.0, 4.5), Dir3::NEG_Y)
            )
            .is_none()
        );
    }
}
