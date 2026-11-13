//! What the regional island shows beyond its homes: workplaces fitted to the
//! bodies the simulation blocks, crop fields plot by plot, the merchant boat
//! and the weather. View entities only: everything here reads
//! `village::view` and never writes the simulation.
pub mod ambient;
pub(crate) mod boat;
pub(crate) mod cast;
pub(crate) mod fields;
pub(crate) mod harbour;
pub(crate) mod props;
pub(crate) mod tint;
pub(crate) mod weather;
pub(crate) mod wildlife;
pub(crate) mod workplaces;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use island_content::village::civic;
use npc_framework::view_helpers::ViewSystems;

pub(crate) fn plugin(app: &mut App) {
    if !island_content::village::frontier::enabled(app.world()) {
        return;
    }
    // `ISLAND_VIEW_NO_AMBIENT=1`: the island without its ambient motion
    // (the earlier fixed breeze), for measuring what the motion costs.
    if std::env::var_os("ISLAND_VIEW_NO_AMBIENT").is_none() {
        app.init_resource::<ambient::AmbientMotion>()
            .init_resource::<ambient::Wind>()
            .init_resource::<ambient::CloudShadows>()
            .init_resource::<ambient::Birds>()
            .init_resource::<ambient::Motes>()
            .add_systems(PreUpdate, ambient::update_wind)
            .add_systems(
                Update,
                (
                    ambient::cloud_shadows,
                    ambient::birds,
                    ambient::motes,
                    ambient::flags,
                    ambient::ripple,
                )
                    .after(crate::fog::FogSet)
                    .in_set(ViewSystems::Present),
            );
    }
    // The deer models need the asset server (not in headless tests).
    if app.world().contains_resource::<AssetServer>() {
        app.init_resource::<wildlife::DeerAssets>()
            .init_resource::<wildlife::WildlifeStates>()
            .add_systems(
                Update,
                (
                    (wildlife::read_wildlife, wildlife::build_graphs).in_set(ViewSystems::Sync),
                    wildlife::play.in_set(ViewSystems::Animate),
                    (wildlife::space_herd, wildlife::move_deer)
                        .chain()
                        .in_set(ViewSystems::Present),
                ),
            );
    }
    app.init_resource::<weather::WeatherLook>()
        .init_resource::<weather::WetBuildings>()
        .init_resource::<boat::BoatScene>()
        .init_resource::<tint::Recoloured>()
        .init_resource::<fields::FieldScene>()
        .init_resource::<props::PropScene>()
        .init_resource::<harbour::Harbour>()
        .init_resource::<cast::OutfitMaterials>()
        .add_systems(
            Update,
            (
                fields::sync,
                props::sync,
                (boat::sync, harbour::build).chain(),
                weather::read,
                cast::stature,
            )
                .in_set(ViewSystems::Sync),
        )
        .add_systems(
            Update,
            (
                boat::sail,
                harbour::bob,
                weather::fall_sheets,
                weather::ground,
                weather::wet_buildings,
            )
                .after(crate::fog::FogSet)
                .in_set(ViewSystems::Present),
        );
}

/// One mesh of axis-aligned boxes: `(centre, size)` each. Flat shaded.
pub(crate) fn boxes_mesh(boxes: &[(Vec3, Vec3)]) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for (centre, size) in boxes {
        let half = *size * 0.5;
        // Each face: its normal and the two axes that span it.
        for (normal, across, up) in [
            (Vec3::X, Vec3::NEG_Z, Vec3::Y),
            (Vec3::NEG_X, Vec3::Z, Vec3::Y),
            (Vec3::Y, Vec3::X, Vec3::NEG_Z),
            (Vec3::NEG_Y, Vec3::X, Vec3::Z),
            (Vec3::Z, Vec3::X, Vec3::Y),
            (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
        ] {
            let base = positions.len() as u32;
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let corner = *centre + (normal + across * a + up * b) * half;
                positions.push(corner.to_array());
                normals.push(normal.to_array());
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_indices(Indices::U32(indices))
}

/// Whether cell (x, z) lies on a plot, or within `margin` cells of it.
pub(crate) fn on_plot(plot: &civic::Plot, x: i32, z: i32, margin: i32) -> bool {
    (plot.lo_x - margin..=plot.hi_x + margin).contains(&x)
        && (plot.lo_z - margin..=plot.hi_z + margin).contains(&z)
}

/// Scenery keeps this many cells from a building placed after the founding:
/// two from a solid body (a crown, a roof's overhang), none from an open one.
pub(crate) fn plot_clearance(plot: &civic::Plot) -> i32 {
    if plot.solid { 2 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_mesh_has_six_outward_faces_a_box() {
        let mesh = boxes_mesh(&[
            (Vec3::ZERO, Vec3::ONE),
            (Vec3::new(3.0, 0.5, 0.0), Vec3::new(1.0, 2.0, 0.5)),
        ]);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|values| values.as_float3())
            .expect("positions");
        let normals = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(|values| values.as_float3())
            .expect("normals");
        assert_eq!(positions.len(), 48);
        // Every corner lies on its box, and its normal points away from the
        // box's middle.
        for (index, (at, normal)) in positions.iter().zip(normals).enumerate() {
            let (centre, half) = if index < 24 {
                (Vec3::ZERO, Vec3::splat(0.5))
            } else {
                (Vec3::new(3.0, 0.5, 0.0), Vec3::new(0.5, 1.0, 0.25))
            };
            let offset = Vec3::from_array(*at) - centre;
            assert!((offset.abs() - half).abs().max_element() < 1e-5, "{at:?}");
            assert!(offset.dot(Vec3::from_array(*normal)) > 0.0);
        }
        let Some(Indices::U32(indices)) = mesh.indices() else {
            panic!("indices")
        };
        assert_eq!(indices.len(), 72);
        // Counter-clockwise seen from outside.
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from_array(positions[triangle[i] as usize]));
            let face = (b - a).cross(c - a);
            assert!(face.dot(Vec3::from_array(normals[triangle[0] as usize])) > 0.0);
        }
    }

    #[test]
    fn scenery_keeps_off_a_plot_and_two_cells_from_a_solid_one() {
        let plot = civic::Plot {
            x: 10,
            z: 20,
            rotation: 0,
            lo_x: 8,
            lo_z: 21,
            hi_x: 12,
            hi_z: 25,
            solid: true,
        };
        assert_eq!(plot_clearance(&plot), 2);
        assert!(on_plot(&plot, 8, 21, 0) && on_plot(&plot, 12, 25, 0));
        assert!(!on_plot(&plot, 7, 21, 0) && on_plot(&plot, 6, 27, 2));
        assert!(!on_plot(&plot, 5, 21, 2));
        let open = civic::Plot {
            solid: false,
            ..plot
        };
        assert_eq!(plot_clearance(&open), 0);
    }
}
