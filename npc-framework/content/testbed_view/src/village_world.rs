//! Island dressing and native world labels, using the game's existing KayKit assets.
use crate::{
    assets::ViewAssets,
    camera::{CameraStart, RtsCamera},
    fog::{FogDim, Fogged},
};
use bevy::{picking::Pickable, prelude::*, world_serialization::WorldAsset};
use npc_framework::prelude::{SimClock, SimConfig, SimId, SimPos};
use npc_framework::view_helpers::{ViewSystems, VisualIndex, VisualOf, sim_to_world};
use std::collections::BTreeMap;
use testbed_content::village::{
    ResourcePatch, Role, Season, VillagePlace, VillageState, Villager, Vitality,
};

#[derive(Resource)]
struct IslandMaterials {
    land: Handle<StandardMaterial>,
    sand: Handle<StandardMaterial>,
    last: Option<Season>,
}
#[derive(Component)]
struct Windmill;
#[derive(Component)]
struct Label {
    sim: SimId,
    visual: Entity,
    lift: f32,
}
#[derive(Resource, Default)]
struct Labels(BTreeMap<SimId, Entity>);
#[derive(Component)]
struct StaticId(SimId);

pub(crate) fn plugin(app: &mut App) {
    if !app.world().contains_resource::<VillageState>()
        && !app
            .world()
            .get_resource::<testbed_content::world_runtime::ActiveWorld>()
            .is_some_and(|w| w.0.scene == testbed_content::worlds::WorldScene::IslandVillage)
    {
        return;
    }
    if !app.world().contains_resource::<CameraStart>() {
        app.insert_resource(CameraStart(RtsCamera {
            focus: Vec3::ZERO,
            distance: 54.0,
            pitch: 58_f32.to_radians(),
            yaw: std::f32::consts::PI,
        }));
    }
    app.init_resource::<Labels>()
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            relink_static
                .in_set(ViewSystems::Sync)
                .before(crate::visuals::despawn_visuals),
        )
        .add_systems(
            Update,
            (actor_labels, project, weather, corpse_pose)
                .chain()
                .in_set(ViewSystems::Present),
        );
}
fn native_text(value: impl Into<String>, size: f32) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(Color::srgb(0.94, 0.96, 0.91)),
    )
}
fn label(commands: &mut Commands, sim: SimId, visual: Entity, text: &str, lift: f32) {
    commands
        .spawn((
            Name::new("village world label"),
            Label { sim, visual, lift },
            Pickable::IGNORE,
            GlobalZIndex(3),
            Node {
                position_type: PositionType::Absolute,
                padding: UiRect::axes(px(5), px(2)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.055, 0.07, 0.86)),
        ))
        .with_child(native_text(text, 11.0));
}
#[allow(clippy::too_many_arguments)]
fn spawn(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    server: Res<AssetServer>,
    assets: Option<Res<ViewAssets>>,
    places: Query<(Entity, &SimId, &SimPos, &VillagePlace)>,
    patches: Query<(Entity, &SimId, &SimPos, &ResourcePatch)>,
    mut index: ResMut<VisualIndex>,
) {
    let water = materials.add(StandardMaterial {
        base_color: Color::srgb(0.075, 0.32, 0.48),
        perceptual_roughness: 0.24,
        reflectance: 0.65,
        ..default()
    });
    commands.spawn((
        Name::new("island surrounding water"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(160., 160.))),
        MeshMaterial3d(water),
        Transform::from_xyz(0., -0.13, 0.),
    ));
    let sand = materials.add(StandardMaterial {
        base_color: Color::srgb(0.72, 0.64, 0.43),
        perceptual_roughness: 1.,
        ..default()
    });
    let land = materials.add(StandardMaterial {
        base_color: Color::srgb(0.63, 0.79, 0.40),
        base_color_texture: assets.as_ref().map(|a| a.grass.clone()),
        perceptual_roughness: 1.,
        ..default()
    });
    for (radius, zradius, y, height, mat, name) in [
        (24.2, 20.2, -0.06, 0.10, sand.clone(), "island shoreline"),
        (23.4, 19.4, -0.04, 0.08, land.clone(), "island grassland"),
    ] {
        commands.spawn((
            Name::new(name),
            Mesh3d(meshes.add(Cylinder::new(radius, height))),
            MeshMaterial3d(mat),
            Transform::from_xyz(0., y, 0.).with_scale(Vec3::new(1., 1., zradius / radius)),
        ));
    }
    let road = materials.add(StandardMaterial {
        base_color: Color::srgb(0.41, 0.32, 0.22),
        perceptual_roughness: 1.,
        ..default()
    });
    for (x, z, w, d) in [
        (0., 0., 2., 22.),
        (0., 0., 31., 2.),
        (-6., 3., 15., 1.3),
        (7., -4., 1.3, 14.),
    ] {
        commands.spawn((
            Name::new("village path"),
            Mesh3d(meshes.add(Cuboid::new(w, 0.01, d))),
            MeshMaterial3d(road.clone()),
            Transform::from_xyz(x, 0.005, z),
        ));
    }
    let wood = materials.add(StandardMaterial {
        base_color: Color::srgb(0.31, 0.20, 0.12),
        ..default()
    });
    let stone = materials.add(StandardMaterial {
        base_color: Color::srgb(0.53, 0.55, 0.55),
        ..default()
    });
    let house: Handle<WorldAsset> =
        server.load(GltfAssetLabel::Scene(0).from_asset("hex/building_home_A_blue.gltf"));
    let grain: Handle<WorldAsset> =
        server.load(GltfAssetLabel::Scene(0).from_asset("hex/building_grain.gltf"));
    let barrel: Handle<WorldAsset> =
        server.load(GltfAssetLabel::Scene(0).from_asset("hex/barrel.gltf"));
    let crate_model: Handle<WorldAsset> =
        server.load(GltfAssetLabel::Scene(0).from_asset("hex/crate_A_big.gltf"));
    let tree: Handle<WorldAsset> =
        server.load(GltfAssetLabel::Scene(0).from_asset("hex/tree_single_A.gltf"));
    let rock: Handle<WorldAsset> =
        server.load(GltfAssetLabel::Scene(0).from_asset("hex/resource_stone.gltf"));
    let bed: Handle<WorldAsset> =
        server.load(GltfAssetLabel::Scene(0).from_asset("furniture/bed_single_A.gltf"));
    for (e, id, pos, place) in &places {
        let root = commands
            .spawn((
                Name::new(format!("village {}", place.label)),
                VisualOf(e),
                StaticId(*id),
                Transform::from_translation(sim_to_world(*pos)),
                Visibility::default(),
                Fogged::default(),
                FogDim::default(),
            ))
            .id();
        index.map.insert(e, root);
        commands.entity(root).with_children(|p| {
            if place.role != Role::WaterKeeper {
                p.spawn((
                    WorldAssetRoot(house.clone()),
                    Transform::from_xyz(0., 0., 2.5).with_scale(Vec3::splat(4.)),
                ));
                p.spawn((
                    WorldAssetRoot(crate_model.clone()),
                    Transform::from_xyz(-1.7, 0.03, 0.1).with_scale(Vec3::splat(1.5)),
                ));
            } else {
                p.spawn((
                    Mesh3d(meshes.add(Cylinder::new(0.85, 0.65))),
                    MeshMaterial3d(stone.clone()),
                    Transform::from_xyz(0., 0.32, 1.3),
                ));
                p.spawn((
                    WorldAssetRoot(barrel.clone()),
                    Transform::from_xyz(1., 0., 0.2).with_scale(Vec3::splat(1.7)),
                ));
            }
            if place.role == Role::Miller {
                let pivot = p
                    .spawn((
                        Windmill,
                        Transform::from_xyz(0., 3.3, 0.8),
                        Visibility::default(),
                    ))
                    .id();
                for angle in [0., std::f32::consts::FRAC_PI_2] {
                    p.commands().spawn((
                        ChildOf(pivot),
                        Mesh3d(meshes.add(Cuboid::new(0.15, 3.5, 0.12))),
                        MeshMaterial3d(wood.clone()),
                        Transform::from_rotation(Quat::from_rotation_z(angle)),
                    ));
                }
            }
            if place.role == Role::Innkeeper {
                p.spawn((
                    WorldAssetRoot(bed.clone()),
                    Transform::from_xyz(1.9, 0., 0.2).with_scale(Vec3::splat(0.42)),
                ));
            }
            if let Some(assets) = &assets {
                let prop = match place.role {
                    Role::Carpenter => Some(&assets.models.workbench),
                    Role::Smith => Some(&assets.models.anvil),
                    Role::Woodcutter => Some(&assets.models.lumber),
                    _ => None,
                };
                if let Some(prop) = prop {
                    p.spawn((
                        WorldAssetRoot(prop.clone()),
                        Transform::from_xyz(-1.4, 0., -0.2).with_scale(Vec3::splat(0.8)),
                    ));
                }
            }
        });
        label(&mut commands, *id, root, &place.label, 4.7);
    }
    for (e, id, pos, patch) in &patches {
        let root = commands
            .spawn((
                Name::new(format!("village {} resource", patch.kind)),
                VisualOf(e),
                StaticId(*id),
                Transform::from_translation(sim_to_world(*pos)),
                Visibility::default(),
                Fogged::default(),
                FogDim::default(),
            ))
            .id();
        index.map.insert(e, root);
        let (model, scale) = match patch.kind.as_str() {
            testbed_content::village::catalog::GRAIN => (grain.clone(), 2.2),
            "log" => (tree.clone(), 3.0),
            "ore" => (rock.clone(), 3.0),
            _ => (barrel.clone(), 1.5),
        };
        commands.entity(root).with_child((
            WorldAssetRoot(model),
            Transform::from_scale(Vec3::splat(scale)),
        ));
        label(
            &mut commands,
            *id,
            root,
            match patch.kind.as_str() {
                "log" => "Fallen wood",
                "ore" => "Ore vein",
                testbed_content::village::catalog::GRAIN => "Grain field",
                _ => "Clean water",
            },
            2.8,
        );
    }
    commands.insert_resource(IslandMaterials {
        land,
        sand,
        last: None,
    });
}
fn actor_labels(
    mut commands: Commands,
    people: Query<(Entity, &SimId, &Villager)>,
    index: Res<VisualIndex>,
    mut seen: ResMut<Labels>,
) {
    for (e, id, person) in &people {
        if let Some(&visual) = index.map.get(&e) {
            if seen.0.get(id) == Some(&visual) {
                continue;
            }
            label(
                &mut commands,
                *id,
                visual,
                &format!("{} | {}", person.name, person.role.name()),
                2.1,
            );
            seen.0.insert(*id, visual);
        }
    }
}
/// Reset/load replaces sim entities while retaining view entities. Rebind immutable island scenery
/// before the generic visual cleanup sees its old entity generation.
fn relink_static(
    index: Res<npc_framework::prelude::SimIndex>,
    mut visuals: ResMut<VisualIndex>,
    mut roots: Query<(Entity, &StaticId, &mut VisualOf)>,
) {
    for (root, id, mut link) in &mut roots {
        if let Some(current) = index.get(id.0)
            && link.0 != current
        {
            visuals.map.remove(&link.0);
            visuals.map.insert(current, root);
            link.0 = current;
        }
    }
}
fn project(
    mut commands: Commands,
    index: Res<npc_framework::prelude::SimIndex>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    positions: Query<&SimPos>,
    visible: Query<&Visibility>,
    mut labels: Query<(Entity, &Label, &mut Node)>,
) {
    let Some((camera, transform)) = camera.iter().next() else {
        return;
    };
    for (entity, label, mut node) in &mut labels {
        if visible.get(label.visual).is_err() {
            commands.entity(entity).despawn();
            continue;
        }
        let shown = visible
            .get(label.visual)
            .is_ok_and(|v| *v != Visibility::Hidden);
        let pos = index
            .get(label.sim)
            .and_then(|e| positions.get(e).ok())
            .map(|p| sim_to_world(*p) + Vec3::Y * label.lift);
        if shown
            && let Some(pos) = pos
            && let Ok(point) = camera.world_to_viewport(transform, pos)
            && camera
                .logical_viewport_rect()
                .is_none_or(|r| r.contains(point))
        {
            node.left = px(point.x - 45.);
            node.top = px(point.y);
            node.display = Display::Flex;
        } else {
            node.display = Display::None;
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn weather(
    clock: Res<SimClock>,
    config: Res<SimConfig>,
    mut sun: Query<(&mut DirectionalLight, &mut Transform), Without<Windmill>>,
    mut mill: Query<&mut Transform, With<Windmill>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut sky: ResMut<ClearColor>,
    mut colors: ResMut<IslandMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Option<Res<ViewAssets>>,
) {
    let day = clock.tick / config.day_length.max(1);
    let season = match day / testbed_content::village::DAYS_PER_SEASON % 4 {
        0 => Season::Spring,
        1 => Season::Summer,
        2 => Season::Autumn,
        _ => Season::Winter,
    };
    if colors.last != Some(season) {
        if let Some(mut mat) = materials.get_mut(&colors.land) {
            mat.base_color = match season {
                Season::Spring => Color::srgb(0.63, 0.79, 0.40),
                Season::Summer => Color::srgb(0.58, 0.71, 0.31),
                Season::Autumn => Color::srgb(0.62, 0.48, 0.25),
                Season::Winter => Color::srgb(0.82, 0.88, 0.92),
            };
            mat.base_color_texture = if matches!(season, Season::Spring | Season::Summer) {
                assets.as_ref().map(|a| a.grass.clone())
            } else {
                None
            };
        }
        if let Some(mut mat) = materials.get_mut(&colors.sand) {
            mat.base_color = if season == Season::Winter {
                Color::srgb(0.70, 0.77, 0.81)
            } else {
                Color::srgb(0.72, 0.64, 0.43)
            };
        }
        colors.last = Some(season);
    }
    let night = config.is_night(clock.tick);
    let phase = (clock.tick % config.day_length.max(1)) as f32 / config.day_length.max(1) as f32;
    for (mut light, mut transform) in &mut sun {
        light.illuminance = if night { 700. } else { 10000. };
        light.color = if night {
            Color::srgb(0.51, 0.65, 1.)
        } else {
            Color::srgb(1., 0.96, 0.87)
        };
        *transform = Transform::from_xyz(phase.cos() * 16., 20., phase.sin() * 16.)
            .looking_at(Vec3::ZERO, Vec3::Y);
    }
    ambient.brightness = if night { 140. } else { 650. };
    sky.0 = if night {
        Color::srgb(0.055, 0.075, 0.14)
    } else {
        Color::srgb(0.56, 0.76, 0.93)
    };
    for mut t in &mut mill {
        t.rotation =
            Quat::from_rotation_z((clock.tick % 480) as f32 * std::f32::consts::TAU / 480.0);
    }
}
fn corpse_pose(
    people: Query<(&Vitality, &Villager)>,
    clock: Res<SimClock>,
    mut roots: Query<(&VisualOf, &mut Transform), With<crate::visuals::ActorVisual>>,
) {
    for (of, mut transform) in &mut roots {
        if people
            .get(of.0)
            .is_ok_and(|(v, p)| v.died_at.is_some() || p.resting_until > clock.tick)
        {
            transform.rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
            transform.translation.y = 0.18;
        }
    }
}
