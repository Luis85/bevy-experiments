//! Island dressing and native world labels, using the game's existing KayKit assets.
#[path = "village_homestead.rs"]
mod homestead;
use crate::{
    assets::ViewAssets,
    camera::{CameraStart, RtsCamera},
    fog::{FogArea, FogDim, Fogged},
};
use bevy::{picking::Pickable, prelude::*, world_serialization::WorldAsset};
use npc_framework::agent_io::stats::ActionNotices;
use npc_framework::kernel::journal::{Journal, JournalKind};
use npc_framework::prelude::{SimClock, SimConfig, SimId, SimPos};
use npc_framework::view_helpers::{ViewSystems, VisualIndex, VisualOf, sim_to_world};
use std::collections::{BTreeMap, VecDeque};
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
/// Public ordering seam; projection implementation details remain private.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct VillageProjectionSet;
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
#[derive(Component)]
struct ActorCaption(SimId);
#[derive(Component)]
struct WorldLabelText;
/// Immutable scenery anchor, never part of the authoritative world.
#[derive(Component)]
struct SceneryAnchor(Vec3);
#[derive(Resource, Default)]
struct CaptionRefresh(f32);
#[derive(Resource)]
struct SpeechFont(Handle<Font>);
fn load_speech_font(mut commands: Commands, fonts: Option<ResMut<Assets<Font>>>) {
    if let Some(mut fonts) = fonts {
        // Embedded from this workspace's licensed asset, so first speech does not race an
        // asynchronous load or depend on a cross-workspace path at runtime.
        let font = Font::from_bytes(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/fonts/Barlow-Regular.ttf"
            ))
            .to_vec(),
        );
        commands.insert_resource(SpeechFont(fonts.add(font)));
    }
}
/// Spectator camera intent only. Never registered, saved or read by simulation systems.
#[derive(Resource, Default)]
pub(crate) struct FollowHero {
    pub enabled: bool,
    expected_focus: Option<Vec3>,
    actor: Option<SimId>,
    input_serial: u64,
}
impl FollowHero {
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
        self.expected_focus = None;
        self.actor = None;
    }
}
const THOUGHT_TICKS: u64 = 160;
const SPEECH_TICKS: u64 = 360;
const MAX_BUBBLES: usize = 32;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum BubbleKind {
    Thought,
    Speech,
}
#[derive(Clone, Debug)]
struct BubbleMessage {
    text: String,
    expires: u64,
    actions: VecDeque<(u128, String)>,
}
#[derive(Resource, Default)]
struct Bubbles {
    epoch: Option<u64>,
    tick: u64,
    journal_cursor: Option<(u64, u32)>,
    notice_cursor: u64,
    messages: BTreeMap<(SimId, BubbleKind), BubbleMessage>,
}
#[derive(Component)]
struct BubbleView {
    kind: BubbleKind,
    text: Entity,
}

pub(crate) fn plugin(app: &mut App) {
    if !app.world().contains_resource::<VillageState>()
        && !app
            .world()
            .get_resource::<testbed_content::world_runtime::ActiveWorld>()
            .is_some_and(|w| w.0.scene == testbed_content::worlds::WorldScene::IslandVillage)
    {
        return;
    }
    let default_camera = !app.world().contains_resource::<CameraStart>();
    if !app.world().contains_resource::<FollowHero>() {
        app.insert_resource(FollowHero {
            enabled: default_camera && !testbed_content::village::colony::enabled(app.world()),
            ..default()
        });
    }
    if default_camera {
        if testbed_content::village::colony::enabled(app.world()) {
            app.insert_resource(crate::island_camera::AutoFrame);
        }
        app.insert_resource(CameraStart(RtsCamera {
            focus: Vec3::ZERO,
            distance: if testbed_content::village::geometry::is_expanded(app.world()) {
                68.0
            } else {
                54.0
            },
            pitch: 58_f32.to_radians(),
            yaw: std::f32::consts::PI,
        }));
    }
    if testbed_content::village::geometry::is_expanded(app.world()) {
        homestead::plugin(app);
    }
    app.init_resource::<Labels>()
        .init_resource::<CaptionRefresh>()
        .init_resource::<Bubbles>()
        .add_systems(PreStartup, load_speech_font)
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            relink_static
                .in_set(ViewSystems::Sync)
                .before(crate::visuals::despawn_visuals),
        )
        .add_systems(
            Update,
            wildlife_visuals
                .in_set(ViewSystems::Sync)
                .after(relink_static)
                .before(crate::visuals::despawn_visuals),
        )
        .add_systems(
            Update,
            (
                follow_hero,
                actor_labels,
                actor_captions,
                label_typography,
                bubbles,
                project.in_set(VillageProjectionSet),
                weather,
                corpse_pose,
            )
                .chain()
                .in_set(ViewSystems::Present),
        );
}
#[allow(clippy::too_many_arguments)]
fn follow_hero(
    time: Res<Time<Real>>,
    state: Res<VillageState>,
    index: Res<npc_framework::prelude::SimIndex>,
    positions: Query<&SimPos>,
    mut follow: ResMut<FollowHero>,
    mut cameras: Query<(Option<&Camera>, &mut RtsCamera, &mut Transform)>,
    panels: Query<
        (
            &Node,
            &ComputedNode,
            &UiGlobalTransform,
            Option<&Visibility>,
            Has<super::homestead_panel::FarmRoot>,
            Has<super::player_hud::WorkspaceRoot>,
            Has<super::player_hud::PlayerRoot>,
        ),
        Or<(
            With<super::village::Hud>,
            With<super::homestead_panel::FarmRoot>,
            With<super::player_hud::PlayerRoot>,
        )>,
    >,
    manual: Option<Res<crate::camera::CameraManualIntent>>,
) {
    let manual_pan = manual.as_deref().is_some_and(|manual| {
        let fresh = manual.serial != follow.input_serial;
        follow.input_serial = manual.serial;
        fresh && manual.active
    });
    if !follow.enabled {
        return;
    }
    if manual_pan {
        follow.enabled = false;
        follow.expected_focus = None;
        return;
    }
    let Some(target) = index
        .get(state.played)
        .and_then(|entity| positions.get(entity).ok())
        .map(|pos| sim_to_world(*pos))
    else {
        follow.expected_focus = None;
        return;
    };
    let changed_actor = follow.actor != Some(state.played);
    let panel_rects: Vec<_> = panels
        .iter()
        .filter(|(node, size, _, visibility, _, _, _)| {
            node.display != Display::None
                && size.size().min_element() > 0.0
                && visibility.is_none_or(|visibility| *visibility != Visibility::Hidden)
        })
        .map(|(_, size, transform, _, farm, workspace, player)| {
            (
                farm || workspace,
                player,
                Rect::from_center_size(
                    transform.translation * size.inverse_scale_factor(),
                    size.size() * size.inverse_scale_factor(),
                ),
            )
        })
        .collect();
    let hud_rect = panel_rects
        .iter()
        .find_map(|(farm, player, rect)| (!farm && !player).then_some(*rect));
    let farm_rect = panel_rects
        .iter()
        .find_map(|(farm, player, rect)| (*farm && !player).then_some(*rect));
    let player_rects: Vec<_> = panel_rects
        .iter()
        .filter_map(|(_, player, rect)| player.then_some(*rect))
        .collect();
    for (projection, mut camera, mut transform) in &mut cameras {
        // The ordinary camera has already handled input. Preserve any real manual pan/focus;
        // zoom and orbit do not change focus, so they keep tracking the hero.
        let mut smoothing_origin = camera.focus;
        if !changed_actor && let Some(expected) = follow.expected_focus {
            let input_clamped = expected.clamp(
                Vec3::splat(-crate::camera::PAN_LIMIT),
                Vec3::splat(crate::camera::PAN_LIMIT),
            );
            if camera.focus.distance_squared(expected) > 0.000_001 {
                if camera.focus.distance_squared(input_clamped) <= 0.000_001 {
                    // Keep our previous tracking pose as the interpolation origin. Starting
                    // again at the input clamp every frame would prevent edge convergence.
                    smoothing_origin = expected;
                } else {
                    follow.enabled = false;
                    follow.expected_focus = None;
                    return;
                }
            }
        }
        let first = changed_actor || follow.expected_focus.is_none();
        let target = projection
            .and_then(|projection| {
                projection.logical_viewport_rect().and_then(|viewport| {
                    framed_focus(
                        projection,
                        &camera,
                        target,
                        if player_rects.is_empty() {
                            hero_screen_anchor(viewport, hud_rect, farm_rect)
                        } else {
                            player_screen_anchor(viewport, &player_rects)
                        },
                    )
                })
            })
            .unwrap_or(target);
        let alpha = 1.0 - (-8.0 * time.delta_secs()).exp();
        camera.focus = if first {
            target
        } else {
            smoothing_origin.lerp(target, alpha)
        };
        camera.focus.y = 0.0;
        *transform = camera.transform();
        follow.expected_focus = Some(camera.focus);
        follow.actor = Some(state.played);
    }
}
/// Center a player or preview in the largest rectangle left by every visible
/// HUD panel, including wrapped header/toolbar and contextual feedback.
pub(crate) fn player_screen_anchor(viewport: Rect, panels: &[Rect]) -> Vec2 {
    let mut regions = vec![viewport.inflate(-12.0)];
    for panel in panels {
        let mut clear = Vec::new();
        for region in regions {
            let blocked = region.intersect(panel.inflate(12.0));
            if blocked.is_empty() {
                clear.push(region);
                continue;
            }
            clear.extend(
                [
                    Rect::new(region.min.x, region.min.y, blocked.min.x, region.max.y),
                    Rect::new(blocked.max.x, region.min.y, region.max.x, region.max.y),
                    Rect::new(region.min.x, region.min.y, region.max.x, blocked.min.y),
                    Rect::new(region.min.x, blocked.max.y, region.max.x, region.max.y),
                ]
                .into_iter()
                .filter(|rect| rect.width() > 1.0 && rect.height() > 1.0),
            );
        }
        regions = clear;
    }
    regions
        .into_iter()
        .max_by(|a, b| (a.width() * a.height()).total_cmp(&(b.width() * b.height())))
        .map_or(viewport.center(), |area| area.center())
}
/// Frame the actor in the usable map, rather than behind a HUD occupying the viewport center.
fn hero_play_area(viewport: Rect, hud: Option<Rect>) -> Rect {
    let mut bounds = viewport.inflate(-12.0);
    bounds.max.y = (viewport.max.y - 44.0).max(bounds.min.y + 1.0);
    let Some(hud) = hud
        .map(|hud| hud.intersect(bounds))
        .filter(|hud| !hud.is_empty())
    else {
        return bounds;
    };
    let left = Rect::from_corners(
        bounds.min,
        Vec2::new((hud.min.x - 12.0).max(bounds.min.x), bounds.max.y),
    );
    let below = Rect::from_corners(
        Vec2::new(bounds.min.x, (hud.max.y + 12.0).min(bounds.max.y)),
        bounds.max,
    );
    if left.width() >= 320.0 {
        left
    } else if below.height() >= 96.0 {
        below
    } else if left.width() * left.height() > below.width() * below.height() {
        left
    } else {
        below
    }
}
pub(crate) fn hero_screen_anchor(viewport: Rect, hud: Option<Rect>, farm: Option<Rect>) -> Vec2 {
    let area = if let Some(farm) = farm {
        // Keep the original spectator framing when the farm is closed. With
        // native controls open, choose a real clear region beside both panels.
        let mut regions = vec![hero_play_area(viewport, None)];
        for panel in [Some(farm), hud].into_iter().flatten() {
            let mut clear = Vec::new();
            for region in regions {
                let blocked = region.intersect(panel.inflate(12.0));
                if blocked.is_empty() {
                    clear.push(region);
                    continue;
                }
                clear.extend(
                    [
                        Rect::new(region.min.x, region.min.y, blocked.min.x, region.max.y),
                        Rect::new(blocked.max.x, region.min.y, region.max.x, region.max.y),
                        Rect::new(region.min.x, region.min.y, region.max.x, blocked.min.y),
                        Rect::new(region.min.x, blocked.max.y, region.max.x, region.max.y),
                    ]
                    .into_iter()
                    .filter(|rect| rect.width() > 1.0 && rect.height() > 1.0),
                );
            }
            regions = clear;
        }
        regions
            .into_iter()
            .max_by(|a, b| (a.width() * a.height()).total_cmp(&(b.width() * b.height())))
            .unwrap_or_else(|| hero_play_area(viewport, None))
    } else {
        hero_play_area(viewport, hud)
    };
    if area.min.y > viewport.min.y + 12.5 {
        // A narrow map needs dialogue headroom above the actor, while feet and name remain clear
        // of the footer. Name/feet clearance is in logical pixels, never a world-space offset.
        Vec2::new(area.center().x, (area.max.y - 44.0).max(area.center().y))
    } else {
        area.center()
    }
}
/// Translation equivariance of a ground-plane ray gives a pixel-correct framing offset at any
/// operator zoom/orbit. No world-space constant is introduced and no authoritative position moves.
pub(crate) fn framed_focus(
    projection: &Camera,
    camera: &RtsCamera,
    actor: Vec3,
    screen: Vec2,
) -> Option<Vec3> {
    let mut centered = *camera;
    centered.focus = actor;
    let ray = projection
        .viewport_to_world(&GlobalTransform::from(centered.transform()), screen)
        .ok()?;
    if ray.direction.y.abs() < 0.000_001 {
        return None;
    }
    let distance = -ray.origin.y / ray.direction.y;
    if distance < 0.0 {
        return None;
    }
    let ground = ray.get_point(distance);
    Some(actor + (actor - ground))
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
fn label(
    commands: &mut Commands,
    sim: SimId,
    visual: Entity,
    text: &str,
    lift: f32,
    actor: bool,
) -> Entity {
    // Actor captions include a live activity line. Reserve its width so a change of work does not
    // repeatedly move neighboring labels; placement still suppresses crowded/hidden captions.
    let width = if actor {
        210.0
    } else {
        (text.chars().count() as f32 * 6.7 + 10.0).clamp(40.0, 210.0)
    };
    let fallback_size = Vec2::new(
        width,
        if actor {
            46.0
        } else {
            (text.chars().count() as f32 * 6.7 / (width - 10.0))
                .ceil()
                .max(1.0)
                * 14.0
                + 4.0
        },
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
            let mut caption = p.spawn((
                WorldLabelText,
                native_text(text, 11.0),
                TextLayout::default().with_linebreak(bevy::text::LineBreak::WordOrCharacter),
            ));
            if actor {
                caption.insert(ActorCaption(sim));
            }
        })
        .id()
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
    geography: Option<Res<testbed_content::village::geometry::Geography>>,
    terrain: Option<Res<testbed_content::village::terrain::TerrainState>>,
) {
    let geography = geography
        .as_deref()
        .unwrap_or(&testbed_content::village::geometry::FIXED);
    let radius = geography.radii_mm.0 as f32 / 1000.;
    let zradius = geography.radii_mm.1 as f32 / 1000.;
    let water = materials.add(StandardMaterial {
        base_color: if terrain.is_some() {
            Color::srgb(0.055, 0.22, 0.29)
        } else {
            Color::srgb(0.075, 0.32, 0.48)
        },
        perceptual_roughness: if terrain.is_some() { 0.92 } else { 0.24 },
        reflectance: if terrain.is_some() { 0.06 } else { 0.65 },
        ..default()
    });
    commands.spawn((
        Name::new("island surrounding water"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(
            if terrain.is_some() { 600. } else { 160. },
            if terrain.is_some() { 600. } else { 160. },
        ))),
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
        (
            radius + 0.2,
            zradius + 0.2,
            -0.06,
            0.10,
            sand.clone(),
            "island shoreline",
        ),
        (
            radius - 0.6,
            zradius - 0.6,
            -0.04,
            0.08,
            land.clone(),
            "island grassland",
        ),
    ] {
        if terrain.is_none() {
            commands.spawn((
                Name::new(name),
                Mesh3d(meshes.add(Cylinder::new(radius, height))),
                MeshMaterial3d(mat),
                Transform::from_xyz(0., y, 0.).with_scale(Vec3::new(1., 1., zradius / radius)),
            ));
        }
    }
    let road = materials.add(StandardMaterial {
        base_color: Color::srgb(0.41, 0.32, 0.22),
        perceptual_roughness: 1.,
        ..default()
    });
    for path in geography.roads {
        if terrain.is_some() {
            continue;
        }
        let (x, z, w, d) = (
            path.x as f32 / 1000.,
            path.z as f32 / 1000.,
            path.width_mm as f32 / 1000.,
            path.depth_mm as f32 / 1000.,
        );
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
    let stake = meshes.add(Cuboid::new(0.10, 0.55, 0.10));
    let clearing = materials.add(if terrain.is_some() {
        Color::srgb(0.45, 0.39, 0.26)
    } else {
        Color::srgb(0.52, 0.43, 0.29)
    });
    for plot in geography.expansion_plots {
        let center = Vec3::new(plot.x as f32 / 1000., 0., plot.z as f32 / 1000.);
        let width = plot.width_mm as f32 / 1000.;
        let depth = plot.depth_mm as f32 / 1000.;
        commands
            .spawn((
                Name::new(format!("{} clearing", plot.label)),
                Transform::from_translation(center),
                Visibility::default(),
                FogDim::default(),
                FogArea::around(center, Vec3::new(width * 0.5, 0., depth * 0.5)),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Mesh3d(meshes.add(Cuboid::new(width - 0.3, 0.015, depth - 0.3))),
                    MeshMaterial3d(clearing.clone()),
                    Transform::from_xyz(0., 0.003, 0.),
                ));
                for x in [-width * 0.5 + 0.2, width * 0.5 - 0.2] {
                    for z in [-depth * 0.5 + 0.2, depth * 0.5 - 0.2] {
                        parent.spawn((
                            Mesh3d(stake.clone()),
                            MeshMaterial3d(wood.clone()),
                            Transform::from_xyz(x, 0.275, z),
                        ));
                    }
                }
                if terrain.is_some() {
                    // Low survey rails distinguish an empty plot from a built foundation.
                    for x in [-width * 0.5 + 0.2, width * 0.5 - 0.2] {
                        parent.spawn((
                            Mesh3d(meshes.add(Cuboid::new(0.035, 0.035, depth - 0.4))),
                            MeshMaterial3d(wood.clone()),
                            Transform::from_xyz(x, 0.32, 0.0),
                        ));
                    }
                    parent.spawn((
                        Mesh3d(meshes.add(Cuboid::new(width - 0.4, 0.035, 0.035))),
                        MeshMaterial3d(wood.clone()),
                        Transform::from_xyz(0.0, 0.32, depth * 0.5 - 0.2),
                    ));
                }
            });
    }
    let sign_board = meshes.add(Cuboid::new(1.2, 0.4, 0.10));
    for district in geography.districts {
        let center = Vec3::new(district.x as f32 / 1000., 0., district.z as f32 / 1000.);
        let root = commands
            .spawn((
                Name::new(district.label),
                Transform::from_translation(center),
                Visibility::default(),
                FogDim::default(),
                FogArea::around(center, Vec3::splat(0.6)),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Mesh3d(stake.clone()),
                    MeshMaterial3d(wood.clone()),
                    Transform::from_xyz(0., 0.275, 0.),
                ));
                parent.spawn((
                    Mesh3d(sign_board.clone()),
                    MeshMaterial3d(wood.clone()),
                    Transform::from_xyz(0., 0.7, 0.),
                ));
            })
            .id();
        let caption = label(&mut commands, SimId(0), root, district.label, 1.1, false);
        commands
            .entity(caption)
            .insert(SceneryAnchor(center + Vec3::Y * 1.1));
    }
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
            testbed_content::village::catalog::FIBER => (grain.clone(), 1.3),
            testbed_content::village::catalog::BERRIES => (tree.clone(), 1.2),
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
                testbed_content::village::catalog::FIBER => "Fiber plants",
                testbed_content::village::catalog::BERRIES => "Berry bushes",
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
fn wildlife_visuals(
    mut commands: Commands,
    animals: Query<(Entity, &SimId, &SimPos), With<testbed_content::village::hunting::Wildlife>>,
    mut index: ResMut<VisualIndex>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let unseen: Vec<_> = animals
        .iter()
        .filter(|(entity, _, _)| !index.map.contains_key(entity))
        .collect();
    if unseen.is_empty() {
        return;
    }
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let hide = materials.add(Color::srgb(0.56, 0.36, 0.19));
    let dark = materials.add(Color::srgb(0.19, 0.16, 0.12));
    let light = materials.add(Color::srgb(0.88, 0.79, 0.62));
    for (entity, id, pos) in unseen {
        let visual = commands
            .spawn((
                Name::new("wild deer"),
                VisualOf(entity),
                StaticId(*id),
                Transform::from_translation(sim_to_world(*pos)),
                Visibility::default(),
                Fogged::default(),
                FogDim::default(),
            ))
            .id();
        index.map.insert(entity, visual);
        commands.entity(visual).with_children(|body| {
            for (position, scale, material) in [
                (
                    Vec3::new(0., 0.86, 0.),
                    Vec3::new(0.48, 0.42, 0.95),
                    hide.clone(),
                ),
                (
                    Vec3::new(0., 1.12, -0.43),
                    Vec3::new(0.22, 0.48, 0.24),
                    hide.clone(),
                ),
                (
                    Vec3::new(0., 1.38, -0.57),
                    Vec3::new(0.28, 0.22, 0.37),
                    hide.clone(),
                ),
                (
                    Vec3::new(-0.18, 1.56, -0.51),
                    Vec3::new(0.14, 0.26, 0.08),
                    hide.clone(),
                ),
                (
                    Vec3::new(0.18, 1.56, -0.51),
                    Vec3::new(0.14, 0.26, 0.08),
                    hide.clone(),
                ),
                (
                    Vec3::new(0., 0.94, 0.51),
                    Vec3::new(0.20, 0.15, 0.20),
                    light.clone(),
                ),
                (
                    Vec3::new(0., 1.37, -0.78),
                    Vec3::new(0.18, 0.10, 0.07),
                    dark.clone(),
                ),
            ] {
                body.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(material),
                    Transform::from_translation(position).with_scale(scale),
                ));
            }
            for x in [-0.17, 0.17] {
                for z in [-0.31, 0.31] {
                    body.spawn((
                        Mesh3d(cube.clone()),
                        MeshMaterial3d(dark.clone()),
                        Transform::from_xyz(x, 0.34, z).with_scale(Vec3::new(0.08, 0.68, 0.09)),
                    ));
                }
            }
        });
        label(&mut commands, *id, visual, "Deer", 2.1, false);
    }
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
                    format!(
                        "{} | {}",
                        caption_short(&person.name, 24),
                        person.role.name()
                    )
                } else {
                    format!("{} (dead)", caption_short(&person.name, 24))
                },
                2.1,
                true,
            );
            seen.0.insert(*id, (visual, alive));
        }
    }
}
fn label_typography(
    colony: Option<Res<testbed_content::village::colony::ColonyState>>,
    font: Option<Res<SpeechFont>>,
    mut labels: Query<(&mut TextFont, Has<ActorCaption>), With<WorldLabelText>>,
) {
    if colony.is_none() {
        return;
    }
    let Some(font) = font else { return };
    for (mut text, actor) in &mut labels {
        text.font = font.0.clone().into();
        text.font_size = FontSize::Px(if actor { 13.0 } else { 12.0 });
    }
}
fn caption_short(value: &str, limit: usize) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.chars().count() <= limit {
        value
    } else {
        format!(
            "{}…",
            value
                .chars()
                .take(limit.saturating_sub(1))
                .collect::<String>()
        )
    }
}
fn actor_caption(world: &World, entity: Entity) -> Option<String> {
    let person = world.get::<Villager>(entity)?;
    let (activity, _, _) = testbed_content::village::activity(world, entity);
    if crate::player_hud::enabled(world)
        && world
            .get_resource::<VillageState>()
            .is_some_and(|state| world.get::<SimId>(entity) == Some(&state.played))
        && !world
            .get_resource::<crate::OverlayState>()
            .is_some_and(|state| state.visible)
    {
        let name = format!("{} · You", caption_short(&person.name, 24));
        return Some(match activity {
            "idle" => name,
            "building" => format!("{name}\nBuilding your cottage"),
            "walking" => format!("{name}\nWalking"),
            "resting" => format!("{name}\nResting"),
            "gathering" => format!("{name}\nGathering supplies"),
            "crafting" => format!("{name}\nMaking something"),
            "dead" => format!("{} · Remembered", caption_short(&person.name, 24)),
            _ => name,
        });
    }
    let detail = match activity {
        "gathering" => world
            .get::<testbed_content::village::Gathering>(entity)
            .and_then(|g| {
                world
                    .resource::<npc_framework::prelude::SimIndex>()
                    .get(g.patch)
            })
            .and_then(|patch| world.get::<ResourcePatch>(patch))
            .map(|patch| format!("gathering {}", patch.kind.trim_start_matches("village_"))),
        "crafting" => world
            .get::<npc_framework::crafting::Crafting>(entity)
            .map(|craft| format!("crafting {}", craft.recipe.trim_start_matches("village_"))),
        _ => testbed_content::village::production::job_summary(world, entity).or_else(|| {
            testbed_content::village::behavior::decision(world, entity).map(str::to_owned)
        }),
    };
    Some(format!(
        "{} | {}\n{}",
        caption_short(&person.name, 24),
        person.role.name(),
        caption_short(detail.as_deref().unwrap_or(activity), 32),
    ))
}
/// Presentation only: visible NPCs explain their work without adding a second world-truth feed.
/// The existing root caption visibility/placement is still the sole fog and decluttering authority.
fn actor_captions(world: &mut World) {
    let delta = world.resource::<Time<Real>>().delta_secs();
    let mut refresh = world.resource_mut::<CaptionRefresh>();
    refresh.0 += delta;
    if refresh.0 < 0.5 {
        return;
    }
    refresh.0 = 0.0;
    let labels: Vec<_> = world
        .query::<(Entity, &ActorCaption)>()
        .iter(world)
        .map(|(entity, caption)| (entity, caption.0))
        .collect();
    for (label, id) in labels {
        let caption = world
            .resource::<npc_framework::prelude::SimIndex>()
            .get(id)
            .and_then(|entity| actor_caption(world, entity));
        if let Some(caption) = caption
            && let Some(mut text) = world.get_mut::<Text>(label)
            && text.0 != caption
        {
            text.0 = caption;
        }
    }
}
impl Bubbles {
    fn thought(&mut self, actor: SimId, key: u128, tick: u64, verb: &str) {
        let message = self
            .messages
            .entry((actor, BubbleKind::Thought))
            .or_insert_with(|| BubbleMessage {
                text: String::new(),
                expires: tick.saturating_add(THOUGHT_TICKS),
                actions: VecDeque::new(),
            });
        // Queued/started records for one handle describe the same public action, not a new thought.
        if message.actions.iter().any(|(seen, _)| *seen == key) {
            message.expires = message.expires.max(tick.saturating_add(THOUGHT_TICKS));
            return;
        }
        let label = caption_short(verb, 40);
        if let Some((last_key, last_label)) = message.actions.back_mut()
            && *last_label == label
            && (*last_key >> 126) != (key >> 126)
        {
            *last_key = key;
            message.expires = tick.saturating_add(THOUGHT_TICKS);
            return;
        }
        message.actions.push_back((key, label));
        while message.actions.len() > 4 {
            message.actions.pop_front();
        }
        message.text = format!(
            "Action: {}",
            message
                .actions
                .iter()
                .map(|(_, verb)| verb.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        );
        message.expires = tick.saturating_add(THOUGHT_TICKS);
    }
    fn collect(&mut self, world: &World) {
        let epoch = npc_framework::agent_io::api::epoch(world);
        let now = world.resource::<SimClock>().tick;
        let latest = world
            .get_resource::<Journal>()
            .and_then(|journal| journal.entries.back())
            .map(|e| (e.tick, e.seq));
        if self.epoch != Some(epoch) || now < self.tick || latest < self.journal_cursor {
            *self = Self {
                epoch: Some(epoch),
                ..default()
            };
        }
        self.tick = now;
        self.messages.retain(|_, message| message.expires > now);
        let index = world.resource::<npc_framework::prelude::SimIndex>();
        let is_actor = |id: SimId| {
            index
                .get(id)
                .is_some_and(|entity| world.get::<Villager>(entity).is_some())
        };
        let notices = world.get_resource::<ActionNotices>();
        let live_notices =
            notices.is_some_and(|notices| notices.entries.iter().any(|n| n.epoch == epoch));
        if let Some(notices) = notices {
            for notice in &notices.entries {
                if notice.seq > self.notice_cursor
                    && notice.epoch == epoch
                    && notice.tick.saturating_add(THOUGHT_TICKS) > now
                    && is_actor(SimId(notice.unit))
                {
                    self.thought(
                        SimId(notice.unit),
                        (1_u128 << 126) | u128::from(notice.seq),
                        notice.tick,
                        public_verb(world, &notice.verb).unwrap_or("action"),
                    );
                }
                self.notice_cursor = self.notice_cursor.max(notice.seq);
            }
        }
        if let Some(journal) = world.get_resource::<Journal>() {
            let cursor = self.journal_cursor;
            for entry in journal
                .entries
                .iter()
                .filter(|entry| cursor.is_none_or(|cursor| (entry.tick, entry.seq) > cursor))
            {
                let Some(actor) = entry.unit.filter(|id| is_actor(*id)) else {
                    continue;
                };
                match &entry.kind {
                    JournalKind::Effect {
                        effect,
                        applied: true,
                    } if effect.get("type").and_then(|v| v.as_str()) == Some("VillageSpeech") => {
                        if let Some(text) = effect.get("text").and_then(|v| v.as_str())
                            && !text.is_empty()
                            && text.chars().count() <= 240
                            && text.len() <= 1024
                            && !text.chars().any(char::is_control)
                            && !text.contains(['\u{2028}', '\u{2029}'])
                            && entry.tick.saturating_add(SPEECH_TICKS) > now
                        {
                            self.messages.insert(
                                (actor, BubbleKind::Speech),
                                BubbleMessage {
                                    text: format!(
                                        "{} says: “{text}”",
                                        index
                                            .get(actor)
                                            .and_then(|entity| world.get::<Villager>(entity))
                                            .map(|person| caption_short(&person.name, 24))
                                            .unwrap_or_else(|| "Resident".into())
                                    ),
                                    expires: entry.tick.saturating_add(SPEECH_TICKS),
                                    actions: VecDeque::new(),
                                },
                            );
                        }
                    }
                    JournalKind::ExternalAction {
                        handle,
                        verb,
                        status,
                        ..
                    } if (status == "started"
                        || (status == "queued"
                            && !notices.is_some_and(|notices| {
                                notices.entries.iter().any(|notice| {
                                    notice.epoch == epoch
                                        && notice.unit == actor.0
                                        && notice.verb == *verb
                                        && notice.tick <= entry.tick
                                        && entry.tick.saturating_sub(notice.tick) < THOUGHT_TICKS
                                })
                            })))
                        && entry.tick.saturating_add(THOUGHT_TICKS) > now =>
                    {
                        self.thought(
                            actor,
                            (2_u128 << 126) | u128::from(*handle),
                            entry.tick,
                            public_verb(world, verb).unwrap_or("recorded action"),
                        );
                    }
                    JournalKind::ActionRejected {
                        controller, verb, ..
                    } if entry.tick.saturating_add(THOUGHT_TICKS) > now
                        && index
                            .get(actor)
                            .and_then(|entity| {
                                world.get::<npc_framework::agent_io::ExternalController>(entity)
                            })
                            .is_some_and(|owner| owner.id == *controller) =>
                    {
                        let label = public_verb(world, verb)
                            .map(|verb| format!("{verb} (rejected)"))
                            .unwrap_or_else(|| "action rejected".into());
                        self.thought(
                            actor,
                            (3_u128 << 126)
                                | (u128::from(entry.tick) << 32)
                                | u128::from(entry.seq),
                            entry.tick,
                            &label,
                        );
                    }
                    // Replays have recorded commands, but no live host notices. Display only the
                    // public verb/type, never command arguments or notebook/speech bodies.
                    JournalKind::Command {
                        cmd: npc_framework::prelude::SimCommand::Content { kind, payload },
                    } if !live_notices
                        && kind != "AgentAct"
                        && kind != "AttachController"
                        && kind != "DetachController"
                        && kind != "VillagePublishDiary"
                        && entry.tick.saturating_add(THOUGHT_TICKS) > now
                        && index.get(actor).is_some_and(|e| {
                            world
                                .get::<npc_framework::agent_io::ExternalController>(e)
                                .is_some()
                        }) =>
                    {
                        let verb = payload
                            .get("verb")
                            .and_then(|v| v.as_str())
                            .and_then(|verb| public_verb(world, verb))
                            .or_else(|| public_verb(world, kind))
                            .unwrap_or("recorded action");
                        self.thought(
                            actor,
                            (3_u128 << 126)
                                | (u128::from(entry.tick) << 32)
                                | u128::from(entry.seq),
                            entry.tick,
                            verb,
                        );
                    }
                    _ => {}
                }
            }
        }
        self.journal_cursor = latest;
        self.messages.retain(|(id, _), _| is_actor(*id));
        while self.messages.len() > MAX_BUBBLES {
            let oldest = self
                .messages
                .iter()
                .min_by_key(|(key, message)| (message.expires, **key))
                .map(|(key, _)| *key)
                .unwrap();
            self.messages.remove(&oldest);
        }
    }
}
pub(super) fn public_verb(world: &World, candidate: &str) -> Option<&'static str> {
    testbed_content::village::public_feedback::public_verb(world, candidate)
}
fn spawn_bubble(world: &mut World, actor: SimId, kind: BubbleKind, visual: Entity, message: &str) {
    let speech = kind == BubbleKind::Speech;
    let mut font = TextFont {
        font_size: FontSize::Px(12.0),
        ..default()
    };
    if speech && let Some(speech_font) = world.get_resource::<SpeechFont>() {
        font = font.with_font(speech_font.0.clone());
    }
    let background = if speech {
        Color::srgb(0.94, 0.91, 0.82)
    } else {
        Color::srgb(0.11, 0.19, 0.27)
    };
    let foreground = if speech {
        Color::srgb(0.07, 0.10, 0.13)
    } else {
        Color::srgb(0.91, 0.96, 1.0)
    };
    let root = world
        .spawn((
            Name::new(if speech {
                "village speech bubble"
            } else {
                "village action bubble"
            }),
            Label {
                sim: actor,
                visual,
                lift: if speech { 4.8 } else { 3.4 },
                actor: false,
                fallback_size: Vec2::new(236.0, 150.0),
            },
            Pickable::IGNORE,
            GlobalZIndex(4),
            Node {
                position_type: PositionType::Absolute,
                width: px(236),
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            },
        ))
        .id();
    let body = world
        .spawn((
            ChildOf(root),
            Pickable::IGNORE,
            Node {
                padding: UiRect::all(px(9)),
                border_radius: BorderRadius::all(px(if speech { 10 } else { 18 })),
                ..default()
            },
            BackgroundColor(background),
        ))
        .id();
    let text = world
        .spawn((
            ChildOf(body),
            Pickable::IGNORE,
            Text::new(message),
            font,
            TextColor(foreground),
            TextLayout::default().with_linebreak(bevy::text::LineBreak::WordOrCharacter),
        ))
        .id();
    if speech {
        world.spawn((
            ChildOf(root),
            Pickable::IGNORE,
            Node {
                width: px(11),
                height: px(11),
                margin: UiRect {
                    left: px(22),
                    top: px(-5),
                    bottom: px(5),
                    ..default()
                },
                ..default()
            },
            UiTransform::from_rotation(Rot2::degrees(45.0)),
            BackgroundColor(background),
        ));
    } else {
        let dots = world
            .spawn((
                ChildOf(root),
                Pickable::IGNORE,
                Node {
                    height: px(15),
                    column_gap: px(4),
                    padding: UiRect::left(px(20)),
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
            .id();
        for size in [7, 4] {
            world.spawn((
                ChildOf(dots),
                Pickable::IGNORE,
                Node {
                    width: px(size),
                    height: px(size),
                    border_radius: BorderRadius::all(percent(50)),
                    ..default()
                },
                BackgroundColor(background),
            ));
        }
    }
    world.entity_mut(root).insert(BubbleView { kind, text });
}
fn bubbles(world: &mut World) {
    world.resource_scope(|world, mut state: Mut<Bubbles>| {
        state.collect(world);
        let existing: Vec<_> = world
            .query::<(Entity, &Label, &BubbleView)>()
            .iter(world)
            .map(|(entity, label, bubble)| {
                (entity, label.sim, label.visual, bubble.kind, bubble.text)
            })
            .collect();
        let mut present = Vec::new();
        for (entity, actor, visual, kind, text) in existing {
            let expected_visual = world
                .resource::<npc_framework::prelude::SimIndex>()
                .get(actor)
                .and_then(|sim| world.resource::<VisualIndex>().map.get(&sim).copied());
            if let Some(message) = state.messages.get(&(actor, kind))
                && expected_visual == Some(visual)
            {
                if let Some(mut shown) = world.get_mut::<Text>(text)
                    && shown.0 != message.text
                {
                    shown.0.clone_from(&message.text);
                }
                present.push((actor, kind));
            } else {
                world.despawn(entity);
            }
        }
        for (&(actor, kind), message) in &state.messages {
            if present.contains(&(actor, kind)) {
                continue;
            }
            if let Some(visual) = world
                .resource::<npc_framework::prelude::SimIndex>()
                .get(actor)
                .and_then(|sim| world.resource::<VisualIndex>().map.get(&sim).copied())
            {
                spawn_bubble(world, actor, kind, visual, &message.text);
            }
        }
    });
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
    nearby_rect(anchor, size, viewport, occupied, 90.0, false)
}
fn bubble_rect(anchor: Vec2, size: Vec2, viewport: Rect, occupied: &[Rect]) -> Option<Rect> {
    nearby_rect(anchor, size, viewport, occupied, 220.0, true)
}
fn nearby_rect(
    anchor: Vec2,
    size: Vec2,
    viewport: Rect,
    occupied: &[Rect],
    limit: f32,
    edge_candidates: bool,
) -> Option<Rect> {
    let bounds = viewport.inflate(-4.0);
    if size.x > bounds.width() || size.y > bounds.height() {
        return None;
    }
    let preferred = anchor - Vec2::new(size.x * 0.5, 0.0);
    let mut offsets = vec![
        Vec2::ZERO,
        Vec2::new(0.0, -size.y - 4.0),
        Vec2::new(0.0, size.y + 4.0),
        Vec2::new(-size.x * 0.5 - 4.0, 0.0),
        Vec2::new(size.x * 0.5 + 4.0, 0.0),
        Vec2::new(0.0, -2.0 * (size.y + 4.0)),
        Vec2::new(0.0, 2.0 * (size.y + 4.0)),
    ];
    if edge_candidates {
        for other in occupied {
            offsets.extend([
                Vec2::new(other.min.x - size.x - 4.0 - preferred.x, 0.0),
                Vec2::new(other.max.x + 4.0 - preferred.x, 0.0),
                Vec2::new(0.0, other.min.y - size.y - 4.0 - preferred.y),
                Vec2::new(0.0, other.max.y + 4.0 - preferred.y),
            ]);
        }
        offsets.sort_by(|a, b| a.length_squared().total_cmp(&b.length_squared()));
    }
    for offset in offsets {
        let position = (preferred + offset).clamp(bounds.min, bounds.max - size);
        let rect = Rect::from_corners(position, position + size);
        if position.distance(preferred) <= limit
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
    camera: Query<
        (
            &Camera,
            &GlobalTransform,
            &Transform,
            Has<RtsCamera>,
            Has<ChildOf>,
        ),
        With<Camera3d>,
    >,
    positions: Query<&SimPos>,
    visible: Query<&Visibility>,
    state: Res<VillageState>,
    selected: Res<npc_framework::view_helpers::Selected>,
    colony: Option<Res<testbed_content::village::colony::ColonyState>>,
    mode: Option<Res<crate::IslandMode>>,
    diagnostics: Option<Res<crate::OverlayState>>,
    vitality: Query<&Vitality>,
    mut labels: Query<(
        Entity,
        &Label,
        &ComputedNode,
        &mut Node,
        Option<&BubbleView>,
        Option<&SceneryAnchor>,
    )>,
    panels: Query<
        (
            &Node,
            &ComputedNode,
            &UiGlobalTransform,
            Option<&Visibility>,
        ),
        (
            Or<(
                With<super::village::Hud>,
                With<super::village::BookPanel>,
                With<super::homestead_panel::FarmRoot>,
                With<super::homestead_panel::FarmLauncher>,
                With<super::colony_panel::LedgerRoot>,
                With<super::player_hud::WorkspaceRoot>,
                With<super::player_hud::JourneyRoot>,
            )>,
            Without<Label>,
        ),
    >,
) {
    let Some((camera, global, local, rts, parented)) = camera.iter().next() else {
        return;
    };
    // The root RTS camera was updated during Input/Present this frame. GlobalTransform only
    // propagates in PostUpdate; using it here would lag labels/bubbles during follow or orbit.
    let current_pose = if rts && !parented {
        GlobalTransform::from(*local)
    } else {
        *global
    };
    let transform = &current_pose;
    let Some(viewport) = camera.logical_viewport_rect() else {
        return;
    };
    let mut occupied: Vec<_> = panels
        .iter()
        .filter(|(node, computed, _, visibility)| {
            node.display != Display::None
                && computed.size().min_element() > 0.0
                && visibility.is_none_or(|visibility| *visibility != Visibility::Hidden)
        })
        .map(|(_, computed, transform, _)| {
            Rect::from_center_size(
                transform.translation * computed.inverse_scale_factor(),
                computed.size() * computed.inverse_scale_factor(),
            )
        })
        .collect();
    occupied.push(Rect::new(
        viewport.min.x,
        (viewport.max.y - 44.0).max(viewport.min.y),
        viewport.max.x,
        viewport.max.y,
    ));
    let mut requests = Vec::new();
    for (entity, label, computed, mut node, bubble, scenery) in &mut labels {
        if visible.get(label.visual).is_err() {
            commands.entity(entity).despawn();
            continue;
        }
        let mut shown = visible
            .get(label.visual)
            .is_ok_and(|v| *v != Visibility::Hidden);
        if colony.is_some() && !diagnostics.as_ref().is_some_and(|overlay| overlay.visible) {
            let focused = selected.0 == Some(label.sim)
                || (mode.as_deref() == Some(&crate::IslandMode::Character)
                    && label.sim == state.played);
            shown &= if let Some(bubble) = bubble {
                bubble.kind == BubbleKind::Speech
            } else {
                focused || scenery.is_some()
            };
        }
        if (label.actor || bubble.is_some())
            && label.sim != state.played
            && selected.0 != Some(label.sim)
            && index
                .get(label.sim)
                .and_then(|e| vitality.get(e).ok())
                .is_some_and(|v| v.died_at.is_some())
        {
            shown = false;
        }
        let pos = scenery.map(|anchor| anchor.0).or_else(|| {
            index
                .get(label.sim)
                .and_then(|e| positions.get(e).ok())
                .map(|p| sim_to_world(*p) + Vec3::Y * label.lift)
        });
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
            } else if label.actor || bubble.is_some() {
                2
            } else {
                3
            };
            let surface_priority = match bubble.map(|bubble| bubble.kind) {
                Some(BubbleKind::Speech) => 0,
                Some(BubbleKind::Thought) => 1,
                None => 2,
            };
            let anchor = if bubble.is_some() {
                point - Vec2::new(0.0, size.y)
            } else {
                point
            };
            requests.push((
                priority,
                surface_priority,
                label.sim,
                entity,
                anchor,
                size,
                bubble.is_some(),
            ));
            continue;
        }
        if node.display != Display::None {
            node.display = Display::None;
        }
    }
    // Complete the visible dialogue before placing action history/name captions. Within each
    // surface, the played and selected actors retain priority over other residents.
    requests.sort_by_key(|(priority, surface, id, ..)| (*surface, *priority, *id));
    for (_, _, _, entity, anchor, size, is_bubble) in requests {
        let placement = if is_bubble {
            bubble_rect(anchor, size, viewport, &occupied)
        } else {
            caption_rect(anchor, size, viewport, &occupied)
        };
        if let Some(rect) = placement {
            occupied.push(rect);
        }
        if let Ok((_, _, _, mut node, _, _)) = labels.get_mut(entity) {
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
    market: Option<Res<testbed_content::village::market::MarketState>>,
    mut sun: Query<(&mut DirectionalLight, &mut Transform), Without<Windmill>>,
    mut mill: Query<&mut Transform, With<Windmill>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut sky: ResMut<ClearColor>,
    mut colors: ResMut<IslandMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Option<Res<ViewAssets>>,
    colony: Option<Res<testbed_content::village::colony::ColonyState>>,
) {
    let (_, season, _) =
        testbed_content::village::calendar_at(clock.tick, &config, market.is_some());
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
        light.illuminance = if night {
            if colony.is_some() { 2300.0 } else { 700.0 }
        } else {
            10000.
        };
        light.color = if night {
            if colony.is_some() {
                Color::srgb(0.64, 0.76, 1.0)
            } else {
                Color::srgb(0.51, 0.65, 1.)
            }
        } else {
            Color::srgb(1., 0.96, 0.87)
        };
        *transform = Transform::from_xyz(phase.cos() * 16., 20., phase.sin() * 16.)
            .looking_at(Vec3::ZERO, Vec3::Y);
    }
    ambient.brightness = if night {
        if colony.is_some() { 420.0 } else { 140.0 }
    } else {
        650.
    };
    if colony.is_some() {
        ambient.color = if night {
            Color::srgb(0.67, 0.77, 1.0)
        } else {
            Color::srgb(0.9, 0.93, 1.0)
        };
    }
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
pub(crate) fn corpse_pose(
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
    fn projection_camera(width: u32, height: u32) -> Camera {
        use bevy::camera::{CameraProjection, ComputedCameraValues, RenderTargetInfo};
        let projection = PerspectiveProjection {
            aspect_ratio: width as f32 / height as f32,
            ..default()
        };
        Camera {
            computed: ComputedCameraValues {
                clip_from_view: projection.get_clip_from_view(),
                target_info: Some(RenderTargetInfo {
                    physical_size: UVec2::new(width, height),
                    scale_factor: 1.0,
                }),
                ..default()
            },
            ..default()
        }
    }
    #[test]
    fn native_follow_anchor_clears_both_farm_and_status_panels() {
        for (width, height) in [(1440.0, 900.0), (1024.0, 768.0), (640.0, 600.0)] {
            let viewport = Rect::new(0.0, 0.0, width, height);
            let farm = Rect::new(12.0, 12.0, 412.0, height * 0.88);
            let hud = (width >= 900.0).then(|| Rect::new(width - 382.0, 12.0, width - 12.0, 560.0));
            let anchor = hero_screen_anchor(viewport, hud, Some(farm));
            assert!(viewport.contains(anchor));
            assert!(
                !farm.inflate(8.0).contains(anchor),
                "the native hero must stay outside the left farm controls at {width}x{height}: {anchor:?}"
            );
            assert!(hud.is_none_or(|hud| !hud.inflate(8.0).contains(anchor)));
        }
    }
    #[test]
    fn follow_framing_projects_the_field_hero_into_the_play_area_and_keeps_both_speakers_clear() {
        let hero = Vec3::new(-15.5, 0.0, 7.5);
        for (width, height, hud) in [
            (1280, 720, Rect::new(898.0, 12.0, 1268.0, 315.0)),
            (480, 720, Rect::new(98.0, 12.0, 468.0, 315.0)),
        ] {
            let projection = projection_camera(width, height);
            let viewport = projection.logical_viewport_rect().unwrap();
            let area = hero_play_area(viewport, Some(hud));
            assert!(area.intersect(hud).is_empty());
            assert!(area.max.y <= viewport.max.y - 44.0);
            let mut camera = RtsCamera {
                distance: 54.0,
                yaw: std::f32::consts::PI,
                ..default()
            };
            let desired = hero_screen_anchor(viewport, Some(hud), None);
            assert!(area.contains(desired));
            camera.focus = framed_focus(&projection, &camera, hero, desired).unwrap();
            let transform = GlobalTransform::from(camera.transform());
            let framed = projection.world_to_viewport(&transform, hero).unwrap();
            assert!(
                framed.distance(desired) < 0.1,
                "actual projection must hit the measured usable area"
            );
            let mut occupied = vec![
                hud,
                Rect::new(0.0, height as f32 - 44.0, width as f32, height as f32),
            ];
            for (actor, size) in [
                (hero, Vec2::new(236.0, 160.0)),
                (hero + Vec3::new(1.5, 0.0, 0.0), Vec2::new(236.0, 110.0)),
            ] {
                let anchor = projection
                    .world_to_viewport(&transform, actor + Vec3::Y * 4.8)
                    .unwrap()
                    - Vec2::new(0.0, size.y);
                let rect=bubble_rect(anchor,size,viewport,&occupied).expect("hero speech and nearby resident reply must both fit clear of narrow HUD/footer");
                assert!(viewport.contains(rect.min) && viewport.contains(rect.max));
                assert!(
                    occupied
                        .iter()
                        .all(|other| rect.inflate(2.0).intersect(*other).is_empty())
                );
                occupied.push(rect);
            }
        }
    }
    #[test]
    fn pan_intent_releases_follow_even_when_the_pan_is_absorbed_by_the_camera_limit() {
        let mut app = testbed_content::new_sim_app(1, "ai_village");
        app.init_resource::<Time>()
            .init_resource::<Time<Real>>()
            .init_resource::<ButtonInput<KeyCode>>();
        crate::camera::plugin(&mut app);
        app.configure_sets(Update, (ViewSystems::Input, ViewSystems::Present).chain())
            .add_systems(Update, follow_hero.in_set(ViewSystems::Present));
        let hero = app.world().resource::<VillageState>().played;
        let outside = Vec3::new(-35.0, 0.0, 7.5);
        app.insert_resource(FollowHero {
            enabled: true,
            expected_focus: Some(outside),
            actor: Some(hero),
            ..default()
        });
        let camera = app
            .world_mut()
            .spawn((
                RtsCamera {
                    focus: outside,
                    distance: 54.0,
                    yaw: 0.0,
                    ..default()
                },
                Transform::default(),
            ))
            .id();
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(100));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyA);
        let hash = npc_framework::prelude::state_hash(app.world_mut());
        app.world_mut().run_schedule(Update);
        assert_eq!(
            app.world().get::<RtsCamera>(camera).unwrap().focus.x,
            -crate::camera::PAN_LIMIT,
            "the actual pan and the automatic clamp end at the same focus"
        );
        assert!(
            !app.world().resource::<FollowHero>().enabled,
            "real pan intent must release follow even at the limit"
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        // Re-enabling while the input set is suppressed must not replay the previous key press.
        app.world_mut().resource_mut::<FollowHero>().toggle();
        use bevy::ecs::system::RunSystemOnce;
        app.world_mut().run_system_once(follow_hero).unwrap();
        assert!(app.world().resource::<FollowHero>().enabled);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyQ);
        app.world_mut().run_schedule(Update);
        assert!(
            app.world().resource::<FollowHero>().enabled,
            "orbit remains compatible with follow"
        );
        assert_eq!(npc_framework::prelude::state_hash(app.world_mut()), hash);
    }
    #[test]
    fn stationary_follow_converges_across_repeated_manual_camera_clamps() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = testbed_content::new_sim_app(1, "ai_village");
        let hero = app.world().resource::<VillageState>().played;
        let hero_entity = app
            .world()
            .resource::<npc_framework::prelude::SimIndex>()
            .get(hero)
            .unwrap();
        // A synthetic stationary target exercises the same out-of-limit focus that HUD framing
        // produces at an island edge. No simulation tick or movement system runs in this fixture.
        *app.world_mut().get_mut::<SimPos>(hero_entity).unwrap() = SimPos::new(-35000, 7500);
        let target = Vec3::new(-35.0, 0.0, 7.5);
        let initial = Vec3::new(-31.0, 0.0, 7.5);
        app.init_resource::<Time<Real>>()
            .insert_resource(FollowHero {
                enabled: true,
                expected_focus: Some(initial),
                actor: Some(hero),
                ..default()
            });
        let camera = app
            .world_mut()
            .spawn((
                RtsCamera {
                    focus: initial,
                    distance: 54.0,
                    ..default()
                },
                Transform::default(),
            ))
            .id();
        let hash = npc_framework::prelude::state_hash(app.world_mut());
        let mut previous_error = initial.distance(target);
        for _ in 0..20 {
            app.world_mut()
                .resource_mut::<Time<Real>>()
                .advance_by(std::time::Duration::from_millis(100));
            let mut view = app.world_mut().get_mut::<RtsCamera>(camera).unwrap();
            view.focus = view.focus.clamp(
                Vec3::splat(-crate::camera::PAN_LIMIT),
                Vec3::splat(crate::camera::PAN_LIMIT),
            );
            app.world_mut().run_system_once(follow_hero).unwrap();
            let error = app
                .world()
                .get::<RtsCamera>(camera)
                .unwrap()
                .focus
                .distance(target);
            assert!(
                error < previous_error || error < 0.0001,
                "tracking must converge instead of restarting from the same clamp: {error} vs {previous_error}"
            );
            previous_error = error;
            assert!(app.world().resource::<FollowHero>().enabled);
        }
        assert!(previous_error < 0.001);
        assert_eq!(npc_framework::prelude::state_hash(app.world_mut()), hash);
    }
    #[test]
    fn root_rts_caption_projection_uses_this_frames_transform_even_before_global_propagation() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = testbed_content::new_sim_app(1, "ai_village");
        app.init_resource::<npc_framework::view_helpers::Selected>();
        let hero = app.world().resource::<VillageState>().played;
        let hero_entity = app
            .world()
            .resource::<npc_framework::prelude::SimIndex>()
            .get(hero)
            .unwrap();
        *app.world_mut().get_mut::<SimPos>(hero_entity).unwrap() = SimPos::new(-15500, 7500);
        let position = Vec3::new(-15.5, 0.0, 7.5);
        let rts = RtsCamera {
            focus: position,
            distance: 54.0,
            yaw: std::f32::consts::PI,
            ..default()
        };
        let local = rts.transform();
        let stale = GlobalTransform::from(
            RtsCamera {
                focus: Vec3::ZERO,
                ..rts
            }
            .transform(),
        );
        let projection = projection_camera(1280, 720);
        let expected = projection
            .world_to_viewport(&GlobalTransform::from(local), position)
            .unwrap();
        assert!(
            projection
                .world_to_viewport(&stale, position)
                .unwrap()
                .distance(expected)
                > 20.0,
            "fixture must contain a materially stale global pose"
        );
        let camera = app
            .world_mut()
            .spawn((projection, Camera3d::default(), rts, local, stale))
            .id();
        let visual = app.world_mut().spawn(Visibility::Visible).id();
        let caption = app
            .world_mut()
            .spawn((
                Label {
                    sim: hero,
                    visual,
                    lift: 0.0,
                    actor: true,
                    fallback_size: Vec2::new(120.0, 22.0),
                },
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ComputedNode::default(),
            ))
            .id();
        let hash = npc_framework::prelude::state_hash(app.world_mut());
        app.world_mut().run_system_once(project).unwrap();
        let node = app.world().get::<Node>(caption).unwrap();
        assert_eq!(node.display, Display::Flex);
        assert_eq!(node.left, px(expected.x - 60.0));
        assert_eq!(node.top, px(expected.y));
        assert_eq!(
            *app.world().get::<GlobalTransform>(camera).unwrap(),
            stale,
            "projection does not mutate transform propagation"
        );
        assert_eq!(npc_framework::prelude::state_hash(app.world_mut()), hash);
    }
    #[test]
    fn island_follow_default_preserves_an_explicit_operator_camera() {
        let mut ordinary = testbed_content::new_sim_app(1, "ai_village");
        plugin(&mut ordinary);
        assert!(ordinary.world().resource::<FollowHero>().enabled);
        assert_eq!(ordinary.world().resource::<CameraStart>().0.distance, 54.0);
        let mut explicit = testbed_content::new_sim_app(1, "ai_village");
        let camera = RtsCamera {
            focus: Vec3::new(5.0, 0.0, 9.0),
            distance: 22.0,
            ..default()
        };
        explicit.insert_resource(CameraStart(camera));
        plugin(&mut explicit);
        assert!(!explicit.world().resource::<FollowHero>().enabled);
        assert_eq!(explicit.world().resource::<CameraStart>().0, camera);
    }
    #[test]
    fn following_tracks_the_current_life_but_manual_focus_releases_it_without_simulation_changes() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = testbed_content::new_sim_app(1, "ai_village");
        app.init_resource::<Time<Real>>()
            .insert_resource(FollowHero {
                enabled: true,
                ..default()
            });
        let hero = app.world().resource::<VillageState>().played;
        let hero_entity = app
            .world()
            .resource::<npc_framework::prelude::SimIndex>()
            .get(hero)
            .unwrap();
        let camera = app
            .world_mut()
            .spawn((
                RtsCamera {
                    distance: 54.0,
                    ..default()
                },
                Transform::default(),
            ))
            .id();
        let before = npc_framework::prelude::state_hash(app.world_mut());
        app.world_mut().run_system_once(follow_hero).unwrap();
        assert_eq!(
            app.world().get::<RtsCamera>(camera).unwrap().focus,
            sim_to_world(*app.world().get::<SimPos>(hero_entity).unwrap())
        );
        assert_eq!(npc_framework::prelude::state_hash(app.world_mut()), before);
        // Zoom/orbit preserve follow, while an actual camera focus edit is a manual override.
        app.world_mut()
            .get_mut::<RtsCamera>(camera)
            .unwrap()
            .distance = 40.0;
        app.world_mut().get_mut::<RtsCamera>(camera).unwrap().yaw = 0.5;
        app.world_mut().run_system_once(follow_hero).unwrap();
        assert!(app.world().resource::<FollowHero>().enabled);
        assert_eq!(app.world().get::<RtsCamera>(camera).unwrap().distance, 40.0);
        // Framing a valid island-edge actor can move the camera's focus beyond its manual pan
        // limit. The ordinary input clamp must not be mistaken for a spectator's intervention.
        let framed_outside = Vec3::new(-crate::camera::PAN_LIMIT - 5.0, 0.0, 7.5);
        app.world_mut().resource_mut::<FollowHero>().expected_focus = Some(framed_outside);
        app.world_mut().get_mut::<RtsCamera>(camera).unwrap().focus = framed_outside.clamp(
            Vec3::splat(-crate::camera::PAN_LIMIT),
            Vec3::splat(crate::camera::PAN_LIMIT),
        );
        app.world_mut().run_system_once(follow_hero).unwrap();
        assert!(
            app.world().resource::<FollowHero>().enabled,
            "the automatic manual-camera clamp is not a manual pan"
        );
        let manual = Vec3::new(18.0, 0.0, 12.0);
        app.world_mut().get_mut::<RtsCamera>(camera).unwrap().focus = manual;
        app.world_mut().run_system_once(follow_hero).unwrap();
        assert!(!app.world().resource::<FollowHero>().enabled);
        assert_eq!(app.world().get::<RtsCamera>(camera).unwrap().focus, manual);
        app.world_mut().resource_mut::<FollowHero>().toggle();
        let successor = app
            .world_mut()
            .query::<(&SimId, &Villager)>()
            .iter(app.world())
            .find(|(id, _)| **id != hero)
            .map(|(id, _)| *id)
            .unwrap();
        app.world_mut().resource_mut::<VillageState>().played = successor;
        let before = npc_framework::prelude::state_hash(app.world_mut());
        app.world_mut().run_system_once(follow_hero).unwrap();
        let successor_entity = app
            .world()
            .resource::<npc_framework::prelude::SimIndex>()
            .get(successor)
            .unwrap();
        assert_eq!(
            app.world().get::<RtsCamera>(camera).unwrap().focus,
            sim_to_world(*app.world().get::<SimPos>(successor_entity).unwrap())
        );
        assert_eq!(npc_framework::prelude::state_hash(app.world_mut()), before);
        app.world_mut().resource_mut::<VillageState>().played = SimId(u32::MAX);
        app.world_mut().run_system_once(follow_hero).unwrap();
        assert!(
            app.world().resource::<FollowHero>().enabled,
            "a missing target waits safely for the next life"
        );
    }
    fn bubble_app() -> App {
        let mut app = testbed_content::try_new_sim_app(1, "ai_village", 1).unwrap();
        app.init_resource::<Bubbles>()
            .init_resource::<VisualIndex>();
        let people: Vec<_> = app
            .world_mut()
            .query_filtered::<Entity, With<Villager>>()
            .iter(app.world())
            .collect();
        for person in people {
            let visual = app.world_mut().spawn(Visibility::Hidden).id();
            app.world_mut()
                .resource_mut::<VisualIndex>()
                .map
                .insert(person, visual);
        }
        app
    }
    #[test]
    fn public_action_notices_and_speech_create_attributed_view_only_bubbles() {
        use bevy::ecs::system::RunSystemOnce;
        use npc_framework::agent_io::stats::{ActionNotice, ActionNotices};
        let mut app = bubble_app();
        app.init_resource::<Assets<Font>>();
        app.world_mut().run_system_once(load_speech_font).unwrap();
        let speech_font = app.world().resource::<SpeechFont>().0.id();
        let actor = app.world().resource::<VillageState>().played;
        let actor_entity = app
            .world()
            .resource::<npc_framework::prelude::SimIndex>()
            .get(actor)
            .unwrap();
        app.world_mut()
            .get_mut::<Villager>(actor_entity)
            .unwrap()
            .name = "Herp".into();
        let other = app
            .world_mut()
            .query::<(&SimId, &Villager)>()
            .iter(app.world())
            .find(|(id, _)| **id != actor)
            .map(|(id, _)| *id)
            .unwrap();
        let epoch = npc_framework::agent_io::api::epoch(app.world());
        let mut notices = ActionNotices::default();
        notices.entries.push_back(ActionNotice {
            seq: 1,
            epoch,
            tick: 0,
            unit: actor.0,
            verb: "buy".into(),
        });
        notices.entries.push_back(ActionNotice {
            seq: 2,
            epoch,
            tick: 0,
            unit: other.0,
            verb: "rest".into(),
        });
        app.insert_resource(notices);
        let mut journal = app.world_mut().resource_mut::<Journal>();
        journal.enabled = true;
        journal.push(
            0,
            Some(actor),
            JournalKind::effect(
                "VillageSpeech",
                &serde_json::json!({"text":"Hello, Ada! A loaf for the road?","target":other}),
            ),
        );
        journal.push(0,Some(other),JournalKind::effect("VillageSpeechHeard",&serde_json::json!({"text":"recipient copy must not become a second bubble","speaker":actor})));
        journal.push(
            0,
            Some(actor),
            JournalKind::effect(
                "VillageDiaryRead",
                &serde_json::json!({"entry":{"body":"private notebook body"}}),
            ),
        );
        let before = npc_framework::prelude::state_hash(app.world_mut());
        bubbles(app.world_mut());
        let state = app.world().resource::<Bubbles>();
        assert_eq!(
            state.messages[&(actor, BubbleKind::Thought)].text,
            "Action: buy"
        );
        assert_eq!(
            state.messages[&(other, BubbleKind::Thought)].text,
            "Action: rest"
        );
        assert_eq!(
            state.messages[&(actor, BubbleKind::Speech)].text,
            "Herp says: “Hello, Ada! A loaf for the road?”"
        );
        assert!(!state.messages.contains_key(&(other, BubbleKind::Speech)));
        assert!(
            state
                .messages
                .values()
                .all(|message| !message.text.contains("private notebook"))
        );
        let roots: Vec<_> = app
            .world_mut()
            .query::<(&Label, &BubbleView, &Node)>()
            .iter(app.world())
            .map(|(label, bubble, node)| (label.sim, bubble.kind, node.display, label.visual))
            .collect();
        assert_eq!(roots.len(), 3);
        let text_styles: Vec<_> = app
            .world_mut()
            .query::<&BubbleView>()
            .iter(app.world())
            .map(|bubble| (bubble.kind, bubble.text))
            .collect();
        for (kind, text) in text_styles {
            let font = &app.world().get::<TextFont>(text).unwrap().font;
            let uses_speech_font =
                matches!(font,bevy::text::FontSource::Handle(handle) if handle.id()==speech_font);
            assert_eq!(
                uses_speech_font,
                kind == BubbleKind::Speech,
                "only speech uses the licensed accent/quotation-capable font"
            );
        }
        for (_, _, display, visual) in roots {
            assert_eq!(
                display,
                Display::None,
                "placement/fog must authorize visibility first"
            );
            assert_eq!(
                *app.world().get::<Visibility>(visual).unwrap(),
                Visibility::Hidden
            );
        }
        assert_eq!(npc_framework::prelude::state_hash(app.world_mut()), before);
    }
    #[test]
    fn unknown_rejected_verbs_affordances_and_replay_payloads_never_publish_private_canaries() {
        let mut app = bubble_app();
        let actor = app.world().resource::<VillageState>().played;
        let entity = app
            .world()
            .resource::<npc_framework::prelude::SimIndex>()
            .get(actor)
            .unwrap();
        let controller = app
            .world()
            .get::<npc_framework::agent_io::ExternalController>(entity)
            .unwrap()
            .id
            .clone();
        let before = npc_framework::prelude::state_hash(app.world_mut());
        const CANARY: &str = "private-notebook-credential-reasoning-canary";
        for field in ["verb", "affordance"] {
            let mut request = serde_json::json!({"unit":actor.0,"controller":controller});
            request[field] = serde_json::json!(CANARY);
            let _ = npc_framework::agent_io::api::call(app.world_mut(), "agent/act", Some(request));
        }
        let mut malformed =
            serde_json::json!({"unit":actor.0,"controller":controller,"verb":"eat","args":{}});
        malformed["args"][CANARY] = serde_json::json!("private argument value");
        let rejection =
            npc_framework::agent_io::api::call(app.world_mut(), "agent/act", Some(malformed))
                .unwrap();
        assert_eq!(rejection["accepted"], false);
        assert!(
            rejection["message"]
                .as_str()
                .is_some_and(|message| message.contains(CANARY))
        );
        let replay = testbed_content::village::VillageAct::new(actor, CANARY);
        app.world_mut().resource_mut::<Journal>().push(
            0,
            Some(actor),
            JournalKind::Command {
                cmd: npc_framework::prelude::SimCommand::content(&replay),
            },
        );
        bubbles(app.world_mut());
        let message =
            &app.world().resource::<Bubbles>().messages[&(actor, BubbleKind::Thought)].text;
        assert!(message.contains("action rejected") && message.contains("recorded action"));
        assert!(!message.contains(CANARY));
        assert!(
            app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .all(|text| !text.0.contains(CANARY))
        );
        assert_eq!(npc_framework::prelude::state_hash(app.world_mut()), before);
    }
    #[test]
    fn delayed_queued_actions_show_at_execution_after_old_acceptance_notices_expire() {
        use npc_framework::agent_io::stats::{ActionNotice, ActionNotices};
        let mut app = bubble_app();
        let actor = app.world().resource::<VillageState>().played;
        let actor_entity = app
            .world()
            .resource::<npc_framework::prelude::SimIndex>()
            .get(actor)
            .unwrap();
        *app.world_mut().get_mut::<SimPos>(actor_entity).unwrap() = SimPos::new(-15500, 7500);
        let epoch = npc_framework::agent_io::api::epoch(app.world());
        let mut notices = ActionNotices::default();
        notices.entries.push_back(ActionNotice {
            seq: 1,
            epoch,
            tick: 0,
            unit: actor.0,
            verb: "gather".into(),
        });
        app.insert_resource(notices);
        bubbles(app.world_mut());
        app.world_mut().resource_mut::<SimClock>().tick = 1;
        app.world_mut().resource_mut::<Journal>().push(
            1,
            Some(actor),
            JournalKind::ExternalAction {
                handle: 1,
                verb: "gather".into(),
                status: "started".into(),
                latency_ticks: None,
            },
        );
        bubbles(app.world_mut());
        let initial = &app.world().resource::<Bubbles>().messages[&(actor, BubbleKind::Thought)];
        assert_eq!(
            initial.actions.len(),
            1,
            "accepted and started views of the same adjacent verb refresh rather than duplicate"
        );
        assert_eq!(
            initial.actions[0].0 >> 126,
            2,
            "live notice and action handle have separate key domains"
        );
        app.world_mut().resource_mut::<SimClock>().tick = THOUGHT_TICKS + 2;
        bubbles(app.world_mut());
        assert!(app.world().resource::<Bubbles>().messages.is_empty());
        let delayed = THOUGHT_TICKS + 100;
        app.world_mut().resource_mut::<SimClock>().tick = delayed;
        app.world_mut().resource_mut::<Journal>().push(
            delayed,
            Some(actor),
            JournalKind::ExternalAction {
                handle: 2,
                verb: "rest".into(),
                status: "started".into(),
                latency_ticks: None,
            },
        );
        let before = npc_framework::prelude::state_hash(app.world_mut());
        bubbles(app.world_mut());
        assert_eq!(
            app.world().resource::<Bubbles>().messages[&(actor, BubbleKind::Thought)].text,
            "Action: rest"
        );
        assert_eq!(
            app.world().resource::<ActionNotices>().entries.len(),
            1,
            "old notice remains present throughout the regression"
        );
        assert_eq!(npc_framework::prelude::state_hash(app.world_mut()), before);
    }
    #[test]
    fn replay_actions_deduplicate_and_bubbles_expire_and_clear_when_time_is_reset() {
        let mut app = bubble_app();
        let actor = app.world().resource::<VillageState>().played;
        let mut journal = app.world_mut().resource_mut::<Journal>();
        journal.enabled = true;
        for status in ["queued", "started"] {
            journal.push(
                0,
                Some(actor),
                JournalKind::ExternalAction {
                    handle: 17,
                    verb: "gather".into(),
                    status: status.into(),
                    latency_ticks: None,
                },
            );
        }
        bubbles(app.world_mut());
        assert_eq!(
            app.world().resource::<Bubbles>().messages[&(actor, BubbleKind::Thought)]
                .actions
                .len(),
            1
        );
        app.world_mut().resource_mut::<SimClock>().tick = THOUGHT_TICKS;
        bubbles(app.world_mut());
        assert!(app.world().resource::<Bubbles>().messages.is_empty());
        app.world_mut().resource_mut::<Journal>().push(
            THOUGHT_TICKS,
            Some(actor),
            JournalKind::effect(
                "VillageSpeech",
                &serde_json::json!({"text":"Good morning.","target":null}),
            ),
        );
        bubbles(app.world_mut());
        assert!(
            app.world()
                .resource::<Bubbles>()
                .messages
                .contains_key(&(actor, BubbleKind::Speech))
        );
        app.world_mut().resource_mut::<SimClock>().tick = THOUGHT_TICKS + SPEECH_TICKS;
        bubbles(app.world_mut());
        assert!(app.world().resource::<Bubbles>().messages.is_empty());
        app.world_mut().resource_mut::<Journal>().clear();
        app.world_mut().resource_mut::<SimClock>().tick = 0;
        bubbles(app.world_mut());
        assert!(app.world().resource::<Bubbles>().messages.is_empty());
        assert_eq!(
            app.world_mut()
                .query::<&BubbleView>()
                .iter(app.world())
                .count(),
            0
        );
    }
    #[test]
    fn action_bursts_and_long_speech_are_bounded_and_share_narrow_caption_placement() {
        use npc_framework::agent_io::stats::{ActionNotice, ActionNotices};
        let mut app = bubble_app();
        let actor = app.world().resource::<VillageState>().played;
        let epoch = npc_framework::agent_io::api::epoch(app.world());
        let mut notices = ActionNotices::default();
        const LONG_PUBLIC_VERB: &str =
            "a_very_long_but_registered_public_action_name_for_the_observer_bubble";
        app.world_mut()
            .resource_mut::<npc_framework::agent_io::registry::AgentRegistry>()
            .handlers
            .insert(
                LONG_PUBLIC_VERB,
                npc_framework::agent_io::builtin::wait_handler,
            );
        for seq in 1..=30 {
            notices.entries.push_back(ActionNotice {
                seq,
                epoch,
                tick: 0,
                unit: actor.0,
                verb: if seq % 2 == 0 {
                    LONG_PUBLIC_VERB
                } else {
                    "buy"
                }
                .into(),
            });
        }
        app.insert_resource(notices);
        let speech = "長".repeat(240);
        let mut journal = app.world_mut().resource_mut::<Journal>();
        journal.enabled = true;
        journal.push(
            0,
            Some(actor),
            JournalKind::effect(
                "VillageSpeech",
                &serde_json::json!({"text":speech,"target":null}),
            ),
        );
        bubbles(app.world_mut());
        let state = app.world().resource::<Bubbles>();
        let thought = &state.messages[&(actor, BubbleKind::Thought)];
        assert_eq!(thought.actions.len(), 4);
        assert!(thought.text.chars().count() <= 174);
        assert_eq!(
            thought
                .actions
                .iter()
                .map(|(seq, _)| *seq as u64)
                .collect::<Vec<_>>(),
            [27, 28, 29, 30]
        );
        assert!(thought.text.contains('…'));
        let attributed_speech = &state.messages[&(actor, BubbleKind::Speech)].text;
        let (speaker, quote) = attributed_speech.split_once(" says: “").unwrap();
        assert!(speaker.chars().count() <= 24);
        assert_eq!(
            quote.strip_suffix('”').unwrap(),
            "長".repeat(240),
            "attribution must preserve the original bounded public speech"
        );
        assert!(attributed_speech.chars().count() <= 273);
        let viewport = Rect::new(0.0, 0.0, 480.0, 720.0);
        let mut occupied = vec![Rect::new(98.0, 12.0, 468.0, 260.0)];
        for size in [Vec2::new(236.0, 150.0), Vec2::new(236.0, 70.0)] {
            let placed = caption_rect(Vec2::new(240.0, 400.0), size, viewport, &occupied)
                .expect("speech and action have space below the narrow HUD");
            assert!(viewport.contains(placed.min) && viewport.contains(placed.max));
            assert!(
                occupied
                    .iter()
                    .all(|other| placed.intersect(*other).is_empty())
            );
            occupied.push(placed);
        }
    }
    #[test]
    fn live_actor_captions_explain_work_and_preserve_simulation_and_hidden_roots() {
        let mut app = testbed_content::try_new_sim_app(1, "ai_village", 1).unwrap();
        app.init_resource::<Time<Real>>()
            .insert_resource(CaptionRefresh(1.0));
        let actor = app.world().resource::<VillageState>().played;
        let entity = app
            .world()
            .resource::<npc_framework::prelude::SimIndex>()
            .get(actor)
            .unwrap();
        let patch = app
            .world_mut()
            .query::<(&SimId, &ResourcePatch)>()
            .iter(app.world())
            .find(|(_, patch)| patch.kind == testbed_content::village::catalog::GRAIN)
            .map(|(id, _)| *id)
            .unwrap();
        app.world_mut()
            .entity_mut(entity)
            .insert(testbed_content::village::Gathering { patch, ends: 100 });
        let root = app
            .world_mut()
            .spawn(Node {
                display: Display::None,
                ..default()
            })
            .id();
        let caption = app
            .world_mut()
            .spawn((ChildOf(root), ActorCaption(actor), Text::new("old caption")))
            .id();
        let before = npc_framework::prelude::state_hash(app.world_mut());
        actor_captions(app.world_mut());
        assert!(
            app.world()
                .get::<Text>(caption)
                .unwrap()
                .0
                .ends_with("\ngathering grain")
        );
        assert_eq!(
            app.world().get::<Node>(root).unwrap().display,
            Display::None
        );
        assert_eq!(npc_framework::prelude::state_hash(app.world_mut()), before);

        app.world_mut()
            .entity_mut(entity)
            .remove::<testbed_content::village::Gathering>();
        app.world_mut()
            .get_mut::<Villager>(entity)
            .unwrap()
            .resting_until = 100;
        // Text remains stable until the next presentation refresh.
        actor_captions(app.world_mut());
        assert!(
            app.world()
                .get::<Text>(caption)
                .unwrap()
                .0
                .ends_with("\ngathering grain")
        );
        app.world_mut().resource_mut::<CaptionRefresh>().0 = 1.0;
        actor_captions(app.world_mut());
        assert!(
            app.world()
                .get::<Text>(caption)
                .unwrap()
                .0
                .ends_with("\nresting")
        );
        app.world_mut().get_mut::<Vitality>(entity).unwrap().died_at = Some(0);
        app.world_mut().resource_mut::<CaptionRefresh>().0 = 1.0;
        actor_captions(app.world_mut());
        assert!(
            app.world()
                .get::<Text>(caption)
                .unwrap()
                .0
                .ends_with("\ndead")
        );
    }
    #[test]
    fn actor_caption_names_are_bounded_and_keep_one_activity_line() {
        let mut app = testbed_content::try_new_sim_app(1, "ai_village", 1).unwrap();
        let actor = app.world().resource::<VillageState>().played;
        let entity = app
            .world()
            .resource::<npc_framework::prelude::SimIndex>()
            .get(actor)
            .unwrap();
        app.world_mut().get_mut::<Villager>(entity).unwrap().name = "長\n".repeat(300);
        let caption = actor_caption(app.world(), entity).unwrap();
        assert_eq!(caption.lines().count(), 2);
        assert!(caption.lines().next().unwrap().chars().count() <= 24 + 3 + "visitor".len());
        assert!(caption.contains('…'));
    }
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
