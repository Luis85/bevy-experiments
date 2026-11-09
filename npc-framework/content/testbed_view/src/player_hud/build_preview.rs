//! A separate translucent building preview, derived only from a known plot.
use super::*;
use npc_framework::view_helpers::{ViewSystems, sim_to_world};

#[derive(Component)]
struct Preview;
#[derive(Resource)]
struct PreviewAssets {
    material: Handle<StandardMaterial>,
}
pub(super) fn install(app: &mut App) {
    app.add_systems(Startup, spawn)
        .add_systems(Update, present.in_set(ViewSystems::Present).after(refresh));
}
fn spawn(
    mut commands: Commands,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.35, 0.85, 0.61, 0.30),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let root = commands
        .spawn((
            Name::new("Cottage construction preview"),
            Preview,
            Transform::default(),
            Visibility::Hidden,
        ))
        .id();
    let cube = meshes.add(Cuboid::new(1., 1., 1.));
    for (position, scale, rotation) in [
        (
            Vec3::new(0., 1.2, 0.),
            Vec3::new(3.2, 2.4, 3.2),
            Quat::IDENTITY,
        ),
        (
            Vec3::new(-0.85, 2.8, 0.),
            Vec3::new(2.3, 0.13, 3.6),
            Quat::from_rotation_z(0.6),
        ),
        (
            Vec3::new(0.85, 2.8, 0.),
            Vec3::new(2.3, 0.13, 3.6),
            Quat::from_rotation_z(-0.6),
        ),
    ] {
        commands.spawn((
            ChildOf(root),
            Mesh3d(cube.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(position)
                .with_scale(scale)
                .with_rotation(rotation),
        ));
    }
    commands.insert_resource(PreviewAssets { material });
}
fn present(world: &mut World) {
    let selected = world
        .get_resource::<PlayerState>()
        .filter(|state| state.tab == Some(Tab::Build))
        .and_then(|state| state.plot);
    let projection = if selected.is_some() && active(world) {
        player_actions::observe(world).ok().and_then(|observed| {
            let plot = model::array(&observed["sections"]["homestead"]["plots"])
                .iter()
                .find(|plot| model::id(&plot["id"]) == selected)?;
            if plot["use_kind"] != "housing" || plot["cottage_built"] == true {
                return None;
            }
            let position = model::position(&plot["cottage_center"])?;
            let allowed = (plot["state_known"] == true).then(|| {
                plot["own"] == true || plot["owner"].is_null() || plot["owner_alive"] == false
            });
            Some((position, allowed))
        })
    } else {
        None
    };
    for (mut transform, mut visible) in world
        .query_filtered::<(&mut Transform, &mut Visibility), With<Preview>>()
        .iter_mut(world)
    {
        if let Some((position, _)) = projection {
            transform.translation = sim_to_world(position);
            *visible = Visibility::Inherited;
        } else {
            *visible = Visibility::Hidden;
        }
    }
    let material = world
        .get_resource::<PreviewAssets>()
        .map(|assets| assets.material.clone());
    if let (Some(material), Some((_, allowed))) = (material, projection)
        && let Some(mut materials) = world.get_resource_mut::<Assets<StandardMaterial>>()
        && let Some(mut material) = materials.get_mut(&material)
    {
        material.base_color = match allowed {
            Some(true) => Color::srgba(0.35, 0.85, 0.61, 0.30),
            Some(false) => Color::srgba(0.95, 0.35, 0.30, 0.30),
            None => Color::srgba(0.95, 0.80, 0.45, 0.30),
        };
    }
}
