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
    actor: bool,
    fallback_size: Vec2,
}
#[derive(Resource, Default)]
struct Labels(BTreeMap<SimId, (Entity, bool)>);
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
fn label(commands: &mut Commands, sim: SimId, visual: Entity, text: &str, lift: f32, actor: bool) {
    let width = (text.chars().count() as f32 * 6.7 + 10.0).clamp(40.0, 210.0);
    let fallback_size = Vec2::new(
        width,
        (text.chars().count() as f32 * 6.7 / (width - 10.0))
            .ceil()
            .max(1.0)
            * 14.0
            + 4.0,
    );
    commands
        .spawn((
            Name::new("village world label"),
            Label {
                sim,
                visual,
                lift,
                actor,
                fallback_size,
            },
            Pickable::IGNORE,
            GlobalZIndex(3),
            Node {
                position_type: PositionType::Absolute,
                padding: UiRect::axes(px(5), px(2)),
                width: px(width),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.055, 0.07, 0.86)),
        ))
        .with_children(|p| {
            p.spawn((
                native_text(text, 11.0),
                TextLayout::default().with_linebreak(bevy::text::LineBreak::WordOrCharacter),
            ));
        });
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
        label(&mut commands, *id, root, &place.label, 4.7, false);
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
            false,
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
    people: Query<(Entity, &SimId, &Villager, &Vitality)>,
    index: Res<VisualIndex>,
    mut seen: ResMut<Labels>,
    labels: Query<(Entity, &Label)>,
) {
    for (e, id, person, vitality) in &people {
        if let Some(&visual) = index.map.get(&e) {
            let alive = vitality.died_at.is_none();
            if seen.0.get(id) == Some(&(visual, alive)) {
                continue;
            }
            for (entity, label) in &labels {
                if label.actor && label.sim == *id {
                    commands.entity(entity).despawn();
                }
            }
            label(
                &mut commands,
                *id,
                visual,
                &if alive {
                    format!("{} | {}", person.name, person.role.name())
                } else {
                    format!("{} (dead)", person.name)
                },
                2.1,
                true,
            );
            seen.0.insert(*id, (visual, alive));
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
/// Prefer a nearby, fully visible caption; suppress excess captions rather than piling them up.
fn caption_rect(anchor: Vec2, size: Vec2, viewport: Rect, occupied: &[Rect]) -> Option<Rect> {
    let bounds = viewport.inflate(-4.0);
    if size.x > bounds.width() || size.y > bounds.height() {
        return None;
    }
    let preferred = anchor - Vec2::new(size.x * 0.5, 0.0);
    for offset in [
        Vec2::ZERO,
        Vec2::new(0.0, -size.y - 4.0),
        Vec2::new(0.0, size.y + 4.0),
        Vec2::new(-size.x * 0.5 - 4.0, 0.0),
        Vec2::new(size.x * 0.5 + 4.0, 0.0),
        Vec2::new(0.0, -2.0 * (size.y + 4.0)),
        Vec2::new(0.0, 2.0 * (size.y + 4.0)),
    ] {
        let position = (preferred + offset).clamp(bounds.min, bounds.max - size);
        let rect = Rect::from_corners(position, position + size);
        if position.distance(preferred) <= 90.0
            && occupied
                .iter()
                .all(|other| rect.inflate(2.0).intersect(*other).is_empty())
        {
            return Some(rect);
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn project(
    mut commands: Commands,
    index: Res<npc_framework::prelude::SimIndex>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    positions: Query<&SimPos>,
    visible: Query<&Visibility>,
    state: Res<VillageState>,
    selected: Res<npc_framework::view_helpers::Selected>,
    vitality: Query<&Vitality>,
    mut labels: Query<(Entity, &Label, &ComputedNode, &mut Node)>,
    panels: Query<
        (&Node, &ComputedNode, &UiGlobalTransform),
        (
            Or<(With<super::village::Hud>, With<super::village::BookPanel>)>,
            Without<Label>,
        ),
    >,
) {
    let Some((camera, transform)) = camera.iter().next() else {
        return;
    };
    let Some(viewport) = camera.logical_viewport_rect() else {
        return;
    };
    let mut occupied: Vec<_> = panels
        .iter()
        .filter(|(node, computed, _)| {
            node.display != Display::None && computed.size().min_element() > 0.0
        })
        .map(|(_, computed, transform)| {
            Rect::from_center_size(
                transform.translation * computed.inverse_scale_factor(),
                computed.size() * computed.inverse_scale_factor(),
            )
        })
        .collect();
    let mut requests = Vec::new();
    for (entity, label, computed, mut node) in &mut labels {
        if visible.get(label.visual).is_err() {
            commands.entity(entity).despawn();
            continue;
        }
        let mut shown = visible
            .get(label.visual)
            .is_ok_and(|v| *v != Visibility::Hidden);
        if label.actor
            && label.sim != state.played
            && selected.0 != Some(label.sim)
            && index
                .get(label.sim)
                .and_then(|e| vitality.get(e).ok())
                .is_some_and(|v| v.died_at.is_some())
        {
            shown = false;
        }
        let pos = index
            .get(label.sim)
            .and_then(|e| positions.get(e).ok())
            .map(|p| sim_to_world(*p) + Vec3::Y * label.lift);
        if shown
            && let Some(pos) = pos
            && let Ok(point) = camera.world_to_viewport(transform, pos)
            && viewport.contains(point)
        {
            let measured = computed.size() * computed.inverse_scale_factor();
            let size = if measured.min_element() > 0.0 {
                measured
            } else {
                label.fallback_size
            };
            let priority = if label.sim == state.played {
                0
            } else if selected.0 == Some(label.sim) {
                1
            } else if label.actor {
                2
            } else {
                3
            };
            requests.push((priority, label.sim, entity, point, size));
            continue;
        }
        if node.display != Display::None {
            node.display = Display::None;
        }
    }
    requests.sort_by_key(|(priority, id, ..)| (*priority, *id));
    for (_, _, entity, anchor, size) in requests {
        let placement = caption_rect(anchor, size, viewport, &occupied);
        if let Some(rect) = placement {
            occupied.push(rect);
        }
        if let Ok((_, _, _, mut node)) = labels.get_mut(entity) {
            if let Some(rect) = placement {
                if node.left != px(rect.min.x) {
                    node.left = px(rect.min.x);
                }
                if node.top != px(rect.min.y) {
                    node.top = px(rect.min.y);
                }
                if node.display != Display::Flex {
                    node.display = Display::Flex;
                }
            } else if node.display != Display::None {
                node.display = Display::None;
            }
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

#[cfg(test)]
mod label_tests {
    use super::*;
    #[test]
    fn crowded_captions_stay_in_the_viewport_and_clear_of_each_other_and_the_hud() {
        let viewport = Rect::new(0.0, 0.0, 480.0, 720.0);
        let hud = Rect::new(98.0, 12.0, 468.0, 260.0);
        let mut occupied = vec![hud];
        let mut shown = 0;
        for anchor in [
            Vec2::new(240.0, 400.0),
            Vec2::new(245.0, 401.0),
            Vec2::new(249.0, 404.0),
            Vec2::new(4.0, 705.0),
            Vec2::new(475.0, 400.0),
            Vec2::new(240.0, 120.0),
        ] {
            if let Some(rect) = caption_rect(anchor, Vec2::new(140.0, 22.0), viewport, &occupied) {
                assert!(viewport.contains(rect.min) && viewport.contains(rect.max));
                assert!(
                    occupied
                        .iter()
                        .all(|other| rect.intersect(*other).is_empty())
                );
                occupied.push(rect);
                shown += 1;
            }
        }
        assert!(
            shown >= 3,
            "crowding should preserve useful labels instead of hiding everything"
        );
        assert!(
            caption_rect(
                Vec2::new(100.0, 100.0),
                Vec2::new(600.0, 22.0),
                viewport,
                &[]
            )
            .is_none()
        );
    }
}
