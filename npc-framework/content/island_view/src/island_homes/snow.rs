//! Snow on the homes' roofs: while snow lies on the ground, a pale layer over
//! both slopes of every home model. One mesh in the model's own units (the
//! two roof planes, a hair above the tiles), hung under each house model so
//! it follows the fit, the cutaway and unblocking; one shared material whose
//! opacity follows the snow on the ground. The inn and lodge (the hall model)
//! and the workplaces have roofs of other shapes and get none.
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::island_world::weather::WeatherLook;

/// The cottage model's roof, model units: eaves at x ±0.40, 0.37 high,
/// ridge on x 0 at 0.77, from z -0.385 to 0.385 (read off the mesh).
const EAVE_X: f32 = 0.405;
const EAVE_Y: f32 = 0.365;
const RIDGE_Y: f32 = 0.77;
const HALF_Z: f32 = 0.39;
/// How far above the tiles the layer lies, model units (about 8 cm).
const LIFT: f32 = 0.012;
/// Snow on the ground from which the roofs show it, and full cover.
const SHOW_FROM: f32 = 0.05;
const FULL_AT: f32 = 0.6;
const OPACITY: f32 = 0.88;

/// On a roof's snow layer.
#[derive(Component)]
pub(crate) struct SnowCap;

/// The shared mesh and material.
#[derive(Resource)]
pub(crate) struct SnowCapAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

/// The two roof planes of the cottage model.
pub(crate) fn roof_mesh() -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for side in [-1.0_f32, 1.0] {
        let normal = Vec3::new(side * (RIDGE_Y - EAVE_Y), EAVE_X, 0.0).normalize();
        let base = positions.len() as u32;
        for (x, y, z) in [
            (side * EAVE_X, EAVE_Y, -HALF_Z),
            (0.0, RIDGE_Y, -HALF_Z),
            (0.0, RIDGE_Y, HALF_Z),
            (side * EAVE_X, EAVE_Y, HALF_Z),
        ] {
            positions.push((Vec3::new(x, y, z) + normal * LIFT).to_array());
            normals.push(normal.to_array());
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_indices(Indices::U32(indices))
}

/// Put a (hidden) snow layer on the cottage model `model`.
pub(crate) fn cap(world: &mut World, model: Entity) {
    if !world.contains_resource::<SnowCapAssets>() {
        if !world.contains_resource::<Assets<Mesh>>()
            || !world.contains_resource::<Assets<StandardMaterial>>()
        {
            return;
        }
        let mesh = world.resource_mut::<Assets<Mesh>>().add(roof_mesh());
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: Color::linear_rgba(0.9, 0.93, 0.98, 0.0),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.9,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
        world.insert_resource(SnowCapAssets { mesh, material });
    }
    let (mesh, material) = {
        let assets = world.resource::<SnowCapAssets>();
        (assets.mesh.clone(), assets.material.clone())
    };
    world.spawn((
        Name::new("roof snow"),
        SnowCap,
        ChildOf(model),
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::default(),
        Visibility::Hidden,
        bevy::light::NotShadowCaster,
    ));
}

/// The layer's opacity for `snow` (0 to 1) lying on the ground.
pub(crate) fn cover(snow: f32) -> f32 {
    let t = ((snow - SHOW_FROM) / (FULL_AT - SHOW_FROM)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t) * OPACITY
}

/// Show the layers while snow lies, as thick as it lies. Writes only when
/// the cover changes by a step, so a clear day costs a comparison.
pub(crate) fn settle(
    look: Option<Res<WeatherLook>>,
    assets: Option<Res<SnowCapAssets>>,
    mut caps: Query<&mut Visibility, With<SnowCap>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut last: Local<Option<(i32, usize)>>,
) {
    let (Some(look), Some(assets)) = (look, assets) else {
        return;
    };
    let alpha = cover(look.snow);
    let step = (alpha * 40.0).round() as i32;
    let count = caps.iter().len();
    if *last == Some((step, count)) {
        return;
    }
    *last = Some((step, count));
    let shown = if step > 0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut visibility in &mut caps {
        visibility.set_if_neq(shown);
    }
    if let Some(mut material) = materials.get_mut(&assets.material) {
        material.base_color = Color::linear_rgba(0.9, 0.93, 0.98, step as f32 / 40.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layer_covers_both_slopes_and_follows_the_snow() {
        let mesh = roof_mesh();
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|values| values.as_float3())
            .expect("positions");
        assert_eq!(positions.len(), 8);
        for at in positions {
            // Above the roof plane y = 0.77 - |x|, never below it.
            let roof = RIDGE_Y - (RIDGE_Y - EAVE_Y) * at[0].abs() / EAVE_X;
            assert!(at[1] > roof && at[1] < roof + 0.03, "{at:?}");
        }
        assert_eq!(cover(0.0), 0.0);
        assert_eq!(cover(SHOW_FROM), 0.0);
        assert!((cover(1.0) - OPACITY).abs() < 1e-6);
        assert!(cover(0.3) > 0.0 && cover(0.3) < OPACITY);
    }
}
