//! The cottage model's chimney in metres. The kit draws the chimney into the
//! house's one mesh, 0.35 model units wide at its foot and 0.21 at its stack
//! against a wall box 0.56 wide; fitted to a 4 m body (7.14 metres a unit
//! across) that is a 2.5 m breast, a third of the house and more. On the
//! island each home model gets a copy of the mesh whose chimney is narrowed
//! across to a breast [`BREAST_M`] wide (the stack in proportion), the rest of
//! the house untouched. The copies are cached by the home's scale across, and
//! the kit's own mesh (legacy scenes draw it) is never changed.
use std::collections::BTreeMap;

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;

/// The chimney breast's width on a home, metres.
pub(crate) const BREAST_M: f32 = 0.9;
/// The chimney's foot across, model units, and where it stands: behind
/// the back wall's plane (z -0.315) and above the roof (0.77 at the ridge,
/// falling one to one to the eaves).
const FOOT_UNITS: f32 = 0.35;
const FOOTPRINT_HALF_X: f32 = 0.18;
const FOOTPRINT_Z: f32 = -0.24;
const BEHIND_WALL_Z: f32 = -0.32;
const RIDGE_Y: f32 = 0.77;

/// On a cottage model: its scale across, for the narrowed copy.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct NarrowChimney {
    pub(crate) scale_x: f32,
}

/// Narrowed copies of the kit's mesh, by original and scale across (cm).
#[derive(Resource, Default)]
pub(crate) struct ChimneyMeshes(BTreeMap<(AssetId<Mesh>, i32), Handle<Mesh>>);

/// How much a model drawn `scale_x` metres a unit across narrows its
/// chimney so the foot is [`BREAST_M`] wide; never widens it.
pub(crate) fn squeeze(scale_x: f32) -> f32 {
    (BREAST_M / (FOOT_UNITS * scale_x.max(1e-3))).min(1.0)
}

/// Whether a vertex of the cottage mesh belongs to its chimney: in the
/// chimney's footprint and either behind the back wall or above the roof.
pub(crate) fn in_chimney(at: Vec3) -> bool {
    at.z < FOOTPRINT_Z
        && at.x.abs() < FOOTPRINT_HALF_X
        && (at.z < BEHIND_WALL_Z || at.y > RIDGE_Y - at.x.abs() + 0.003)
}

/// The cottage mesh's positions with the chimney narrowed by `factor`
/// towards its own middle (x 0).
pub(crate) fn narrowed(positions: &[[f32; 3]], factor: f32) -> Vec<[f32; 3]> {
    positions
        .iter()
        .map(|&[x, y, z]| {
            if in_chimney(Vec3::new(x, y, z)) {
                [x * factor, y, z]
            } else {
                [x, y, z]
            }
        })
        .collect()
}

/// Observer on a cottage model: swap each mesh for the narrowed copy.
pub(crate) fn on_ready(
    ready: On<WorldInstanceReady>,
    narrow: Query<&NarrowChimney>,
    children: Query<&Children>,
    mut meshes_on: Query<&mut Mesh3d, Without<super::snow::SnowCap>>,
    mut meshes: ResMut<Assets<Mesh>>,
    cache: Option<ResMut<ChimneyMeshes>>,
) {
    let Ok(narrow) = narrow.get(ready.entity) else {
        return;
    };
    let Some(mut cache) = cache else { return };
    let factor = squeeze(narrow.scale_x);
    if factor >= 0.999 {
        return;
    }
    let key_scale = (narrow.scale_x * 100.0).round() as i32;
    for entity in children.iter_descendants(ready.entity) {
        let Ok(mut mesh) = meshes_on.get_mut(entity) else {
            continue;
        };
        let key = (mesh.0.id(), key_scale);
        let copy = match cache.0.get(&key) {
            Some(handle) => handle.clone(),
            None => {
                let Some(source) = meshes.get(&mesh.0) else {
                    continue;
                };
                let mut copy = source.clone();
                let Some(VertexAttributeValues::Float32x3(positions)) =
                    copy.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    continue;
                };
                let positions = narrowed(positions, factor);
                copy.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                let handle = meshes.add(copy);
                cache.0.insert(key, handle.clone());
                handle
            }
        };
        mesh.0 = copy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chimney_is_a_breast_of_about_ninety_centimetres_on_every_home() {
        // On a 4 m and a 5 m wide home the foot is 0.9 m across.
        for scale_x in [4.0 / 0.56, 5.0 / 0.56] {
            let foot = FOOT_UNITS * scale_x * squeeze(scale_x);
            assert!((foot - BREAST_M).abs() < 1e-4, "{foot}");
        }
        // A model drawn small is never widened.
        assert_eq!(squeeze(1.0), 1.0);
        // The stack above the roof and the foot behind the back wall
        // narrow; the walls, the roof and the gable do not.
        let stack = [0.105, 0.9, -0.3];
        let foot = [-0.175, 0.1, -0.45];
        let back_wall = [0.1, 0.2, -0.315];
        let roof = [0.1, 0.67, -0.3];
        let side_wall = [0.28, 0.2, -0.3];
        let front = [0.1, 0.5, 0.385];
        let out = narrowed(&[stack, foot, back_wall, roof, side_wall, front], 0.5);
        assert_eq!(out[0], [0.0525, 0.9, -0.3]);
        assert_eq!(out[1], [-0.0875, 0.1, -0.45]);
        assert_eq!(&out[2..], &[back_wall, roof, side_wall, front]);
    }
}
