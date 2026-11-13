//! One decorative watchtower per settlement, in its roof colour. It is a view
//! entity with no simulation counterpart: it never blocks, never appears in
//! navigation and is placed only on dry ground the world spec leaves empty.
use bevy::prelude::*;
use npc_framework::prelude::SimPos;
use testbed_content::village::{
    ResourcePatch, VillagePlace,
    colony::Household,
    frontier::{
        terrain::{TerrainState, TerrainSurface},
        world_spec::{LivingWorldSpec, SettlementDef},
    },
    homestead::HomesteadPlot,
};

use super::palette::{RoofPalette, RoofTarget, palette_for};
use crate::fog::{FogArea, FogDim};

const TOWER: &str = "living/buildings/building_tower_A_blue.gltf";
/// The tower mesh is 1 unit wide and 2.2 tall: 8.8 m at this scale.
const TOWER_SCALE: f32 = 4.0;
/// Clear ground required around the tower's centre.
const CLEAR_MM: i64 = 11_000;
/// Foliage is kept this far from a landmark.
pub(crate) const FOLIAGE_CLEAR_MM: i64 = 5_500;

fn clear_of(spec: &LivingWorldSpec, at: SimPos, clear_mm: i64) -> bool {
    let far = |other: SimPos, extra: i64| at.dist2(other) >= (clear_mm + extra).pow(2);
    far(spec.actor_start, 0)
        && spec.settlements.iter().all(|settlement| {
            settlement.sites.iter().all(|site| far(site.position, 0))
                && settlement.homes.iter().all(|home| far(home.door, 0))
        })
        && spec.plots.iter().all(|plot| {
            // Plot extents are respected whether x/z name a corner or the centre.
            far(
                SimPos::new(plot.x, plot.z),
                plot.width_mm.max(plot.depth_mm) as i64,
            )
        })
}

fn dry_and_level(terrain: &TerrainState, at: SimPos) -> bool {
    let (x, z) = (at.x.div_euclid(1000), at.z.div_euclid(1000));
    let Some(centre) = terrain.cell(x, z) else {
        return false;
    };
    (-2..=2).all(|dz| {
        (-2..=2).all(|dx| {
            terrain.cell(x + dx, z + dz).is_some_and(|cell| {
                cell.surface != TerrainSurface::Water
                    && cell.height_mm > 0
                    && (cell.height_mm - centre.height_mm).abs() <= 600
            })
        })
    })
}

/// Where a settlement's landmark stands, or `None` if no candidate is free.
/// Pure over the frozen spec and terrain, so foliage and the tower agree.
pub(crate) fn landmark_site(
    spec: &LivingWorldSpec,
    terrain: &TerrainState,
    settlement: &SettlementDef,
) -> Option<SimPos> {
    // The village edge first, widening only if that ring is taken.
    const DIRECTIONS: [(i32, i32); 8] = [
        (1000, 0),
        (-1000, 0),
        (707, 707),
        (-707, 707),
        (707, -707),
        (-707, -707),
        (0, 1000),
        (0, -1000),
    ];
    [36, 40, 44, 32, 48].into_iter().find_map(|radius_m| {
        DIRECTIONS.into_iter().find_map(|(dx, dz)| {
            let at = SimPos::new(
                settlement.center.x + dx * radius_m,
                settlement.center.z + dz * radius_m,
            );
            // Stay with this settlement rather than drifting to a neighbour.
            let nearest = spec
                .settlements
                .iter()
                .min_by_key(|other| (other.center.dist2(at), other.key.as_str()))?;
            (nearest.key == settlement.key
                && clear_of(spec, at, CLEAR_MM)
                && dry_and_level(terrain, at))
            .then_some(at)
        })
    })
}

pub(crate) fn landmark_sites(spec: &LivingWorldSpec, terrain: &TerrainState) -> Vec<SimPos> {
    spec.settlements
        .iter()
        .filter_map(|settlement| landmark_site(spec, terrain, settlement))
        .collect()
}

pub(crate) fn spawn(
    mut commands: Commands,
    spec: Res<LivingWorldSpec>,
    terrain: Res<TerrainState>,
    server: Res<AssetServer>,
    occupied: Query<
        &SimPos,
        Or<(
            With<VillagePlace>,
            With<Household>,
            With<ResourcePatch>,
            With<HomesteadPlot>,
        )>,
    >,
) {
    let tower = server.load(GltfAssetLabel::Scene(0).from_asset(TOWER));
    for settlement in &spec.settlements {
        let Some(site) = landmark_site(&spec, &terrain, settlement) else {
            continue;
        };
        // The spec is the plan; live statics (resource patches are placed at
        // world creation) get the final say.
        if occupied
            .iter()
            .any(|other| other.dist2(site) < (CLEAR_MM - 3_000).pow(2))
        {
            continue;
        }
        let y = terrain
            .cell(site.x.div_euclid(1000), site.z.div_euclid(1000))
            .map_or(0.0, |cell| cell.height_mm as f32 / 1000.0);
        let at = Vec3::new(site.x as f32 / 1000.0, y, site.z as f32 / 1000.0);
        let towards = Vec3::new(
            settlement.center.x as f32 / 1000.0 - at.x,
            0.0,
            settlement.center.z as f32 / 1000.0 - at.z,
        );
        let root = commands
            .spawn((
                Name::new(format!("{} watchtower", settlement.label)),
                Transform::from_translation(at)
                    .with_rotation(Quat::from_rotation_y(towards.x.atan2(towards.z))),
                Visibility::default(),
                FogArea::around(at, Vec3::splat(2.0)),
                FogDim::default(),
            ))
            .id();
        let mut model = commands.spawn((
            ChildOf(root),
            WorldAssetRoot(tower.clone()),
            Transform::from_scale(Vec3::splat(TOWER_SCALE)),
        ));
        let palette = palette_for(settlement.biome);
        if palette != RoofPalette::Blue {
            model.insert(RoofTarget(palette));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testbed_content::village::frontier::{terrain::TerrainCell, world_spec::Biome};

    fn land(spec: &LivingWorldSpec) -> TerrainState {
        TerrainState {
            width: spec.width_cells,
            height: spec.height_cells,
            cells: vec![
                TerrainCell {
                    height_mm: 1200,
                    surface: TerrainSurface::Grass,
                    biome: Biome::Meadow,
                };
                (spec.width_cells * spec.height_cells) as usize
            ],
            revision: 0,
        }
    }

    #[test]
    fn every_authored_settlement_gets_a_landmark_clear_of_all_sim_sites() {
        let spec = LivingWorldSpec::default();
        let terrain = land(&spec);
        let sites = landmark_sites(&spec, &terrain);
        assert_eq!(sites.len(), spec.settlements.len());
        for (site, settlement) in sites.iter().zip(&spec.settlements) {
            for other in &spec.settlements {
                for place in &other.sites {
                    assert!(place.position.dist2(*site) >= CLEAR_MM.pow(2));
                }
                for home in &other.homes {
                    assert!(home.door.dist2(*site) >= CLEAR_MM.pow(2));
                }
            }
            assert!(spec.actor_start.dist2(*site) >= CLEAR_MM.pow(2));
            // Close enough to read as part of its own settlement.
            assert!(settlement.center.dist2(*site) <= 48_000_i64.pow(2));
            assert!(
                spec.bounds()
                    .contains(npc_framework::prelude::CellCoord::new(
                        site.x.div_euclid(1000),
                        site.z.div_euclid(1000)
                    ))
            );
        }
        // Deterministic: the foliage pass and the spawn agree.
        assert_eq!(sites, landmark_sites(&spec, &terrain));
    }

    #[test]
    fn water_cliffs_and_crowding_reject_a_site() {
        let spec = LivingWorldSpec::default();
        let mut terrain = land(&spec);
        let settlement = &spec.settlements[0];
        let first = landmark_site(&spec, &terrain, settlement).unwrap();
        let index = terrain
            .bounds()
            .index(npc_framework::prelude::CellCoord::new(
                first.x.div_euclid(1000) + 1,
                first.z.div_euclid(1000),
            ))
            .unwrap() as usize;
        terrain.cells[index].surface = TerrainSurface::Water;
        let second = landmark_site(&spec, &terrain, settlement).unwrap();
        assert_ne!(first, second);
        for cell in &mut terrain.cells {
            cell.surface = TerrainSurface::Water;
        }
        assert_eq!(landmark_site(&spec, &terrain, settlement), None);
        assert!(!clear_of(&spec, settlement.sites[0].position, CLEAR_MM));
    }
}
