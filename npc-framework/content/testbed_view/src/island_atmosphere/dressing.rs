//! View-only dressing that gives the ground its relief: low rocks standing
//! on the ridge outcrops and bushes along the woodland edge.
//!
//! Placement is a pure function of a cell's coordinates and the terrain read
//! at startup: a view-side hash, never the simulation's random numbers. The
//! entities are scenery of their own. Nothing here is a resource patch, an
//! obstacle or a sim entity: rocks are not ore, bushes are not timber, and
//! residents walk through both.
use bevy::prelude::*;
use bevy::world_serialization::WorldAsset;
use npc_framework::prelude::SimPos;
use testbed_content::village::frontier::{
    terrain::{TerrainState, TerrainSurface},
    world_spec::{Biome, LivingWorldSpec},
};

use super::ground::{self, Ground};
use crate::fog::{FogArea, FogDim};

/// Cells searched for the woodland border, each way.
const EDGE_REACH: i32 = 7;
/// Rocks stand on every second cell of an outcrop at most.
const ROCK_LATTICE: i32 = 2;
/// Height of a rock against its width: low slabs, not boulders, because
/// outcrop cells stay walkable.
const ROCK_FLATTEN: f32 = 0.6;

const ROCK_MODELS: [&str; 5] = [
    "living/nature/rock_single_B.gltf",
    "living/nature/rock_single_D.gltf",
    "living/nature/rock_single_E.gltf",
    "hex/rock_single_A.gltf",
    "hex/rock_single_C.gltf",
];

/// A rock of a ridge outcrop, placed by cell.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct OutcropRock {
    x: i32,
    z: i32,
}

/// A bush at the woodland edge, placed by cell.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct EdgeBush {
    x: i32,
    z: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RockSpot {
    pub(crate) model: usize,
    /// Offset from the cell centre, metres.
    pub(crate) offset: Vec2,
    pub(crate) scale: f32,
    pub(crate) yaw: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BushSpot {
    pub(crate) offset: Vec2,
    pub(crate) scale: f32,
    pub(crate) yaw: f32,
    /// Which of the two greens.
    pub(crate) tone: usize,
}

/// A repeatable number in `0..1` for a cell and a purpose.
fn chance(x: i32, z: i32, salt: u32) -> f32 {
    ground::lattice(x, z, salt)
}

fn dry(terrain: &TerrainState, x: i32, z: i32) -> Option<Biome> {
    terrain
        .cell(x, z)
        .filter(|cell| cell.surface != TerrainSurface::Water)
        .map(|cell| cell.biome)
}

/// Where a cell lies against the woodland border: `(inside the woodland,
/// distance in metres to the nearest dry cell on the other side)`, or `None`
/// further than `EDGE_REACH` from it.
pub(crate) fn woodland_edge(terrain: &TerrainState, x: i32, z: i32) -> Option<(bool, f32)> {
    let inside = dry(terrain, x, z)? == Biome::Woodland;
    let mut nearest: Option<i32> = None;
    for dz in -EDGE_REACH..=EDGE_REACH {
        for dx in -EDGE_REACH..=EDGE_REACH {
            let squared = dx * dx + dz * dz;
            if squared > EDGE_REACH * EDGE_REACH || nearest.is_some_and(|best| squared >= best) {
                continue;
            }
            if dry(terrain, x + dx, z + dz)
                .is_some_and(|biome| (biome == Biome::Woodland) != inside)
            {
                nearest = Some(squared);
            }
        }
    }
    nearest.map(|squared| (inside, (squared as f32).sqrt()))
}

/// Whether the scenery tree of a woodland cell is left out, so the tree
/// line frays instead of following the biome border cell for cell. More
/// trees go the nearer the border, in ragged bays several metres across.
pub(crate) fn tree_thinned(terrain: &TerrainState, x: i32, z: i32) -> bool {
    let Some((true, distance)) = woodland_edge(terrain, x, z) else {
        return false;
    };
    let bay = ground::value_noise(x as f32, z as f32, 12.0, 51);
    let share = ((1.0 - distance / 6.5).max(0.0) * (0.35 + bay)).min(0.92);
    chance(x, z, 52) < share
}

/// The bush of a cell, if it has one: scattered outside the woodland for a
/// few metres, thinning with distance, and a few between the thinned trees.
pub(crate) fn edge_bush(terrain: &TerrainState, x: i32, z: i32) -> Option<BushSpot> {
    let cell = terrain.cell(x, z)?;
    if cell.surface != TerrainSurface::Grass {
        return None;
    }
    let (inside, distance) = woodland_edge(terrain, x, z)?;
    let clump = ground::value_noise(x as f32, z as f32, 9.0, 53);
    let share = if inside {
        0.16 * (1.0 - distance / 4.0).max(0.0)
    } else {
        (1.0 - distance / 7.5).max(0.0).powi(2) * (0.12 + 0.7 * clump)
    };
    (chance(x, z, 54) < share).then(|| BushSpot {
        offset: Vec2::new(chance(x, z, 55) - 0.5, chance(x, z, 56) - 0.5) * 0.8,
        scale: 0.75 + chance(x, z, 57) * 0.85,
        yaw: chance(x, z, 58) * std::f32::consts::TAU,
        tone: usize::from(chance(x, z, 59) > 0.55),
    })
}

/// The rock of a cell, if it has one: on untouched ridge ground inside an
/// outcrop, clear of every path. Painted stone, painted turf and anything
/// beside soil keep the ground the player gave them.
pub(crate) fn outcrop_rock(terrain: &TerrainState, x: i32, z: i32) -> Option<RockSpot> {
    if x.rem_euclid(ROCK_LATTICE) != 0 || z.rem_euclid(ROCK_LATTICE) != 0 {
        return None;
    }
    if ground::classify(terrain, x, z) != Ground::Ridge
        || ground::outcrop(x as f32 + 0.5, z as f32 + 0.5) < 0.6
    {
        return None;
    }
    let beside_path = (-1..=1).any(|dz| {
        (-1..=1).any(|dx| {
            terrain.cell(x + dx, z + dz).is_none_or(|cell| {
                matches!(cell.surface, TerrainSurface::Soil | TerrainSurface::Water)
            })
        })
    });
    if beside_path || chance(x, z, 61) > 0.82 {
        return None;
    }
    Some(RockSpot {
        model: (chance(x, z, 62) * ROCK_MODELS.len() as f32) as usize % ROCK_MODELS.len(),
        offset: Vec2::new(chance(x, z, 63) - 0.5, chance(x, z, 64) - 0.5) * 1.3,
        scale: 2.2 + chance(x, z, 65).powi(2) * 2.4,
        yaw: chance(x, z, 66) * std::f32::consts::TAU,
    })
}

/// Kept clear like the scenery trees: workplaces, doors, watchtowers and
/// the bodies of homes and workplaces.
fn clear_of_sites(spec: &LivingWorldSpec, landmarks: &[SimPos], x: i32, z: i32) -> bool {
    let p = SimPos::new(x * 1000 + 500, z * 1000 + 500);
    !spec.settlements.iter().any(|settlement| {
        settlement
            .sites
            .iter()
            .any(|site| site.position.dist2(p) < 30_000_000)
            || settlement
                .homes
                .iter()
                .any(|home| home.door.dist2(p) < 24_000_000)
    }) && !landmarks
        .iter()
        .any(|landmark| landmark.dist2(p) < super::landmark::FOLIAGE_CLEAR_MM.pow(2))
        // Nor on the floor of a home or an inn, or against its walls.
        && !crate::island_homes::model::on_founding_body(spec, p, 1.0)
}

/// A rounded shrub of four facetted lobes, about a metre across: plainly a
/// bush, with no trunk and none of the cone shape of the island's trees.
pub(crate) fn bush_mesh() -> Mesh {
    let lobe = |radius: f32, at: Vec3, squash: f32| {
        Sphere::new(radius)
            .mesh()
            .ico(1)
            .expect("bush lobe")
            .scaled_by(Vec3::new(1.0, squash, 1.0))
            .translated_by(at)
    };
    let mut mesh = lobe(0.50, Vec3::new(0.0, 0.30, 0.0), 0.74);
    for (radius, at, squash) in [
        (0.36, Vec3::new(0.38, 0.22, 0.10), 0.80),
        (0.31, Vec3::new(-0.30, 0.19, 0.26), 0.82),
        (0.27, Vec3::new(-0.05, 0.17, -0.37), 0.85),
    ] {
        mesh.merge(&lobe(radius, at, squash))
            .expect("bush lobes share a layout");
    }
    mesh.with_duplicated_vertices().with_computed_flat_normals()
}

/// Two muted greens (sRGB), darker than meadow turf so a bush reads on it.
pub(crate) const BUSH_TONES: [[f32; 3]; 2] = [[0.24, 0.40, 0.20], [0.36, 0.44, 0.19]];

/// A bush tone in a season (sRGB): autumn browns it, winter takes it to a
/// dark, nearly grey twig colour (the leaves are down), spring and summer
/// keep the green.
pub(crate) fn bush_tone([r, g, b]: [f32; 3], season: testbed_content::village::Season) -> [f32; 3] {
    use testbed_content::village::Season;
    let light = 0.3 * r + 0.59 * g + 0.11 * b;
    match season {
        Season::Autumn => [light * 1.35, light * 0.95, light * 0.45],
        // A dark, muted evergreen under its snow: still a plant beside the
        // grey rocks.
        Season::Winter => [light * 0.5, light * 0.78, light * 0.55],
        _ => [r, g, b],
    }
}

/// The edge bushes' tone materials, and the season they are drawn in.
#[derive(Resource, Clone, Debug)]
pub(crate) struct BushTones {
    materials: [Handle<StandardMaterial>; 2],
    /// The bush mesh, and per tone its copy with snow on the upward faces.
    plain: Handle<Mesh>,
    snowy: [Handle<Mesh>; 2],
    drawn: Option<testbed_content::village::Season>,
}

/// The bush mesh with snow on its tops, under the winter colour of `tone`.
fn snowy_bush(tone: [f32; 3]) -> Mesh {
    let [r, g, b] = bush_tone(tone, testbed_content::village::Season::Winter);
    let [sr, sg, sb] = crate::island_world::tint::SNOW;
    let plain = bush_mesh();
    crate::island_world::tint::snow_dusted(
        &plain,
        Color::srgb(r, g, b).to_linear(),
        Color::srgb(sr, sg, sb).to_linear(),
    )
    .unwrap_or(plain)
}

/// Retint the edge bushes when the drawn season changes.
pub(crate) fn bush_season(
    season: Option<Res<crate::island_world::ambient::DrawnSeason>>,
    tones: Option<ResMut<BushTones>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut bushes: Query<(&MeshMaterial3d<StandardMaterial>, &mut Mesh3d), With<EdgeBush>>,
) {
    let (Some(season), Some(mut tones)) = (season, tones) else {
        return;
    };
    if tones.drawn == Some(season.0) {
        return;
    }
    tones.drawn = Some(season.0);
    for (handle, base) in tones.materials.iter().zip(BUSH_TONES) {
        if let Some(mut material) = materials.get_mut(handle) {
            let [r, g, b] = bush_tone(base, season.0);
            material.base_color = Color::srgb(r, g, b);
        }
    }
    let winter = season.0 == testbed_content::village::Season::Winter;
    for (material, mut mesh) in &mut bushes {
        let tone = usize::from(material.0 == tones.materials[1]);
        let want = if winter {
            tones.snowy[tone].clone()
        } else {
            tones.plain.clone()
        };
        if mesh.0 != want {
            mesh.0 = want;
        }
    }
}

pub(crate) fn spawn(
    mut commands: Commands,
    terrain: Res<TerrainState>,
    spec: Res<LivingWorldSpec>,
    server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let landmarks = super::landmark::landmark_sites(&spec, &terrain);
    let rocks = ROCK_MODELS
        .map(|path| server.load::<WorldAsset>(GltfAssetLabel::Scene(0).from_asset(path)));
    let bush = meshes.add(bush_mesh());
    let tones = BUSH_TONES.map(|[r, g, b]| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            perceptual_roughness: 0.95,
            reflectance: 0.12,
            ..default()
        })
    });
    commands.insert_resource(BushTones {
        materials: tones.clone(),
        plain: bush.clone(),
        snowy: BUSH_TONES.map(|tone| meshes.add(snowy_bush(tone))),
        drawn: None,
    });
    for (coord, cell) in terrain.iter() {
        if cell.surface == TerrainSurface::Water
            || !clear_of_sites(&spec, &landmarks, coord.x, coord.z)
        {
            continue;
        }
        let ground_y = cell.height_mm as f32 / 1000.0;
        let centre = Vec3::new(coord.x as f32 + 0.5, ground_y, coord.z as f32 + 0.5);
        if let Some(spot) = outcrop_rock(&terrain, coord.x, coord.z) {
            let at = centre + Vec3::new(spot.offset.x, 0.0, spot.offset.y);
            commands
                .spawn((
                    Name::new("outcrop rock"),
                    OutcropRock {
                        x: coord.x,
                        z: coord.z,
                    },
                    Transform::from_translation(at),
                    Visibility::default(),
                    FogArea::around(at, Vec3::splat(1.0)),
                    FogDim::default(),
                ))
                .with_child((
                    WorldAssetRoot(rocks[spot.model].clone()),
                    Transform::from_rotation(Quat::from_rotation_y(spot.yaw))
                        .with_scale(Vec3::new(spot.scale, spot.scale * ROCK_FLATTEN, spot.scale)),
                ));
        }
        if let Some(spot) = edge_bush(&terrain, coord.x, coord.z) {
            let at = centre + Vec3::new(spot.offset.x, 0.0, spot.offset.y);
            commands.spawn((
                Name::new("woodland edge bush"),
                EdgeBush {
                    x: coord.x,
                    z: coord.z,
                },
                Mesh3d(bush.clone()),
                MeshMaterial3d(tones[spot.tone].clone()),
                Transform::from_translation(at)
                    .with_rotation(Quat::from_rotation_y(spot.yaw))
                    .with_scale(Vec3::splat(spot.scale)),
                Visibility::default(),
                FogArea::around(at, Vec3::splat(1.0)),
                FogDim::default(),
            ));
        }
    }
}

/// After a terrain edit: dressing follows the ground's height and steps
/// aside where its cell was painted over or dug out.
pub(crate) fn sync(
    terrain: Res<TerrainState>,
    civic: Option<Res<testbed_content::village::civic::CivicState>>,
    buildings: Query<&testbed_content::village::civic::Building>,
    sites: Query<&testbed_content::village::civic::ConstructionSite>,
    mut rocks: Query<(&OutcropRock, &mut Transform, &mut Visibility), Without<EdgeBush>>,
    mut bushes: Query<(&EdgeBush, &mut Transform, &mut Visibility), Without<OutcropRock>>,
) {
    if !terrain.is_changed() && !civic.is_some_and(|civic| civic.is_changed()) {
        return;
    }
    // Rocks and bushes give way to whatever is built after the founding.
    let plots: Vec<testbed_content::village::civic::Plot> = buildings
        .iter()
        .map(|building| building.plot)
        .chain(sites.iter().map(|site| site.plot))
        .collect();
    let free = |x: i32, z: i32| {
        !plots.iter().any(|plot| {
            crate::island_world::on_plot(
                plot,
                x,
                z,
                crate::island_world::plot_clearance(plot).min(1),
            )
        })
    };
    let place =
        |x: i32, z: i32, shown: bool, transform: &mut Transform, visibility: &mut Visibility| {
            if let Some(cell) = terrain.cell(x, z) {
                let y = cell.height_mm as f32 / 1000.0;
                if transform.translation.y != y {
                    transform.translation.y = y;
                }
            }
            let wanted = if shown {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *visibility != wanted {
                *visibility = wanted;
            }
        };
    for (rock, mut transform, mut visibility) in &mut rocks {
        let shown = outcrop_rock(&terrain, rock.x, rock.z).is_some() && free(rock.x, rock.z);
        place(rock.x, rock.z, shown, &mut transform, &mut visibility);
    }
    for (bush, mut transform, mut visibility) in &mut bushes {
        let shown = terrain
            .cell(bush.x, bush.z)
            .is_some_and(|cell| cell.surface == TerrainSurface::Grass)
            && free(bush.x, bush.z);
        place(bush.x, bush.z, shown, &mut transform, &mut visibility);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use npc_framework::prelude::CellCoord;

    fn saturation([r, g, b]: [f32; 3]) -> f32 {
        let (high, low) = (r.max(g).max(b), r.min(g).min(b));
        (high - low) / high.max(1e-3)
    }

    #[test]
    fn winter_bushes_are_a_dark_evergreen_with_snow_on_top() {
        use testbed_content::village::Season;
        for tone in BUSH_TONES {
            let winter = bush_tone(tone, Season::Winter);
            // Green leads, muted and dark: not grey, not the summer green.
            assert!(winter[1] > winter[0] && winter[1] > winter[2], "{winter:?}");
            assert!((0.25..0.5).contains(&saturation(winter)), "{winter:?}");
            assert!(winter.iter().all(|c| *c < 0.32), "{winter:?}");
            assert!(winter[1] < tone[1]);
            // Snow on its tops, not its sides.
            let snowy = snowy_bush(tone);
            let Some(bevy::mesh::VertexAttributeValues::Float32x4(colours)) =
                snowy.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                panic!("snow colours");
            };
            assert!(colours.iter().any(|c| c[1] > 2.0), "some face carries snow");
            assert!(colours.contains(&[1.0; 4]), "the sides keep their colour");
            assert!(saturation(tone) > 0.4);
            assert_eq!(bush_tone(tone, Season::Summer), tone);
            let autumn = bush_tone(tone, Season::Autumn);
            assert!(autumn[0] > autumn[1] && autumn[1] > autumn[2], "{autumn:?}");
        }
    }
    use testbed_content::village::frontier::terrain::TerrainCell;

    fn field(side: u32, biome: Biome) -> TerrainState {
        TerrainState {
            width: side,
            height: side,
            cells: vec![
                TerrainCell {
                    height_mm: 1000,
                    surface: TerrainSurface::Grass,
                    biome,
                };
                (side * side) as usize
            ],
            revision: 0,
        }
    }

    fn set(terrain: &mut TerrainState, x: i32, z: i32, edit: impl Fn(&mut TerrainCell)) {
        let index = terrain.bounds().index(CellCoord::new(x, z)).unwrap() as usize;
        edit(&mut terrain.cells[index]);
    }

    /// Woodland east of x = 0, meadow west of it: a dead-straight border.
    fn bordered() -> TerrainState {
        let mut terrain = field(96, Biome::Meadow);
        for z in -48..48 {
            for x in 0..48 {
                set(&mut terrain, x, z, |cell| cell.biome = Biome::Woodland);
            }
        }
        terrain
    }

    #[test]
    fn the_woodland_border_is_measured_from_both_sides() {
        let terrain = bordered();
        assert_eq!(woodland_edge(&terrain, 0, 0), Some((true, 1.0)));
        assert_eq!(woodland_edge(&terrain, 3, 0), Some((true, 4.0)));
        assert_eq!(woodland_edge(&terrain, -1, 0), Some((false, 1.0)));
        assert_eq!(woodland_edge(&terrain, -5, 0), Some((false, 5.0)));
        assert_eq!(woodland_edge(&terrain, 20, 0), None);
        assert_eq!(woodland_edge(&terrain, -20, 0), None);
        assert_eq!(woodland_edge(&terrain, 500, 0), None, "off the map");
        // Water is not the other side of a wood.
        let mut pond = field(32, Biome::Woodland);
        set(&mut pond, 0, 0, |cell| {
            cell.surface = TerrainSurface::Water;
            cell.biome = Biome::Coast;
        });
        assert_eq!(woodland_edge(&pond, 2, 0), None);
    }

    #[test]
    fn the_tree_line_frays_and_bushes_scatter_only_near_it() {
        let terrain = bordered();
        let column = |x: i32, test: &dyn Fn(i32, i32) -> bool| {
            (-40..40).filter(|z| test(x, *z)).count() as f32 / 80.0
        };
        let thinned = |x: i32, z: i32| tree_thinned(&terrain, x, z);
        let bush = |x: i32, z: i32| edge_bush(&terrain, x, z).is_some();
        // Trees thin out towards the border and are left alone further in.
        let (first, third, deep) = (
            column(0, &thinned),
            column(3, &thinned),
            column(9, &thinned),
        );
        assert!(first > 0.35 && first < 0.97, "first row {first}");
        assert!(third < first && third > 0.02, "third row {third}");
        assert_eq!(deep, 0.0);
        assert_eq!(
            column(-3, &thinned),
            0.0,
            "there are no meadow trees to thin"
        );
        // The first row is neither all gone nor all kept, in runs: a ragged line.
        let runs = (-40..40)
            .filter(|z| thinned(0, *z) != thinned(0, *z + 1))
            .count();
        assert!((6..70).contains(&runs), "{runs} changes along the border");
        // Bushes: densest just outside, fewer further out, none far away.
        let (near, mid, far) = (column(-1, &bush), column(-5, &bush), column(-9, &bush));
        assert!(near > 0.12 && near < 0.75, "beside the wood {near}");
        assert!(mid < near, "{mid} five metres out");
        assert_eq!(far, 0.0);
        assert!(column(1, &bush) < near, "a few between the thinned trees");
        assert_eq!(column(8, &bush), 0.0);
        // Deterministic, inside its cell, and never on a path or in water.
        let spot = (-40..40)
            .find_map(|z| edge_bush(&terrain, -1, z).map(|spot| (z, spot)))
            .unwrap();
        assert_eq!(edge_bush(&terrain, -1, spot.0), Some(spot.1));
        assert!(spot.1.offset.abs().max_element() <= 0.4);
        assert!((0.75..=1.6).contains(&spot.1.scale) && spot.1.tone < BUSH_TONES.len());
        let mut paved = terrain.clone();
        set(&mut paved, -1, spot.0, |cell| {
            cell.surface = TerrainSurface::Soil
        });
        assert_eq!(edge_bush(&paved, -1, spot.0), None);
    }

    /// A ridge exactly as the generator leaves it (see `ground`'s tests).
    fn generated_ridge() -> TerrainState {
        let mut terrain = field(128, Biome::Ridge);
        for index in 0..terrain.cells.len() {
            if index as u32 % 19 < 4 {
                terrain.cells[index].surface = TerrainSurface::Stone;
            }
        }
        terrain
    }

    #[test]
    fn rocks_cluster_on_outcrops_and_leave_paths_and_painted_ground_alone() {
        let terrain = generated_ridge();
        let mut rocks = Vec::new();
        for z in -60..60 {
            for x in -60..60 {
                if let Some(spot) = outcrop_rock(&terrain, x, z) {
                    rocks.push((x, z, spot));
                }
            }
        }
        assert!(rocks.len() > 60, "{} rocks on a 120 m ridge", rocks.len());
        assert!(rocks.len() < 1400, "{} rocks is a quarry", rocks.len());
        for (x, z, spot) in &rocks {
            assert!(ground::outcrop(*x as f32 + 0.5, *z as f32 + 0.5) >= 0.6);
            assert!(x % 2 == 0 && z % 2 == 0);
            assert!(spot.model < ROCK_MODELS.len());
            assert!((2.2..=4.6).contains(&spot.scale));
            assert_eq!(outcrop_rock(&terrain, *x, *z), Some(*spot), "repeatable");
        }
        // Clustered: most rocks have another within one lattice step.
        let near = rocks
            .iter()
            .filter(|(x, z, _)| {
                rocks.iter().any(|(ox, oz, _)| {
                    (ox, oz) != (x, z) && (ox - x).abs() <= 2 && (oz - z).abs() <= 2
                })
            })
            .count();
        assert!(
            near * 10 >= rocks.len() * 7,
            "{near} of {} clustered",
            rocks.len()
        );
        // Every model is used.
        for model in 0..ROCK_MODELS.len() {
            assert!(rocks.iter().any(|(_, _, spot)| spot.model == model));
        }
        // A path laid past a rock, or turf painted over it, removes it.
        let (x, z, _) = rocks[rocks.len() / 2];
        let mut path = terrain.clone();
        set(&mut path, x + 1, z, |cell| {
            cell.surface = TerrainSurface::Soil
        });
        assert_eq!(outcrop_rock(&path, x, z), None);
        let mut painted = terrain.clone();
        for dz in -3..=3 {
            for dx in -3..=3 {
                set(&mut painted, x + dx, z + dz, |cell| {
                    cell.surface = TerrainSurface::Grass
                });
            }
        }
        assert_eq!(outcrop_rock(&painted, x, z), None);
        // Other biomes have no outcrops.
        let meadow = field(64, Biome::Meadow);
        assert!((-30..30).all(|x| outcrop_rock(&meadow, x, 0).is_none()));
    }

    #[test]
    fn dressing_follows_terrain_edits_and_is_never_a_simulation_entity() {
        let terrain = generated_ridge();
        let (x, z) = (-60..60)
            .flat_map(|z| (-60..60).map(move |x| (x, z)))
            .find(|(x, z)| outcrop_rock(&terrain, *x, *z).is_some())
            .unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(terrain)
            .add_systems(Update, sync);
        let rock = app
            .world_mut()
            .spawn((
                OutcropRock { x, z },
                Transform::from_xyz(x as f32, 1.0, z as f32),
                Visibility::Inherited,
            ))
            .id();
        let bush = app
            .world_mut()
            .spawn((
                EdgeBush { x: x + 5, z },
                Transform::from_xyz(x as f32 + 5.0, 1.0, z as f32),
                Visibility::Inherited,
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(rock),
            Some(&Visibility::Inherited)
        );
        // Raise the rock's cell and pave the bush's.
        {
            let mut terrain = app.world_mut().resource_mut::<TerrainState>();
            set(&mut terrain, x, z, |cell| cell.height_mm = 2500);
            set(&mut terrain, x + 5, z, |cell| {
                cell.surface = TerrainSurface::Soil
            });
        }
        app.update();
        assert_eq!(
            app.world().get::<Transform>(rock).unwrap().translation.y,
            2.5
        );
        assert_eq!(
            app.world().get::<Visibility>(bush),
            Some(&Visibility::Hidden)
        );
        // Paint the outcrop over with turf: the rock steps aside.
        {
            let mut terrain = app.world_mut().resource_mut::<TerrainState>();
            for dz in -3..=3 {
                for dx in -3..=3 {
                    set(&mut terrain, x + dx, z + dz, |cell| {
                        cell.surface = TerrainSurface::Grass
                    });
                }
            }
        }
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(rock),
            Some(&Visibility::Hidden)
        );
        // A bush is one mesh of a few hundred triangles, and stands on the ground.
        let mesh = bush_mesh();
        let vertices = mesh.count_vertices();
        assert!((200..2000).contains(&vertices), "{vertices} vertices");
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("bush positions")
        };
        let low = positions.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
        let high = positions
            .iter()
            .map(|p| p[1])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            low > -0.12 && low < 0.05 && high > 0.5 && high < 0.8,
            "{low}..{high}"
        );
    }
}
