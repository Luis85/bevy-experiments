//! Night life: path lanterns that fade along the dusk curve and a warm glow
//! at homes that are heated or occupied. Lights are shadowless and capped by
//! distance from the camera focus. Everything is a view entity; home state is
//! only read, and only for homes the fog currently shows live.
use std::collections::BTreeMap;

use bevy::prelude::*;
use npc_framework::prelude::{SimClock, SimConfig, SimId, SimIndex, SimPos};
use npc_framework::view_helpers::VisualIndex;
use testbed_content::village::{
    colony::Household,
    frontier::{terrain::TerrainState, world_spec::LivingWorldSpec},
    homestead::Cottage,
};

use super::daylight::{ViewDayOverride, lamp_level, view_daylight};
use crate::RtsCamera;
use crate::fog::{FogArea, FogDim, Fogged, Look};
use crate::island_homes::{Cutaways, HomeShell, Shells};

/// Shadowless point lights allowed at once, nearest the camera focus first.
pub(crate) const MAX_LANTERN_LIGHTS: usize = 24;
pub(crate) const MAX_WINDOW_LIGHTS: usize = 16;
const LANTERN_LUMENS: f32 = 21_000.0;
const LANTERN_RANGE: f32 = 10.0;
/// The warm light at a lit home's door. At 9,000 lumens it barely showed on
/// the ground at settlement zoom; this lays a pool of light on the doorstep
/// and the facade without reaching the neighbours.
const WINDOW_LUMENS: f32 = 16_000.0;
const WINDOW_RANGE: f32 = 9.0;
/// A resident this close to the door counts as being at home.
pub(crate) const AT_HOME_MM2: i64 = 5_000 * 5_000;
/// Real seconds for a window to come up or die down.
const WINDOW_FADE_RATE: f32 = 1.6;

#[derive(Component)]
pub(crate) struct Lantern;

/// The lantern head material; its emissive follows the lamp level.
#[derive(Resource)]
pub(crate) struct LanternGlass(pub(crate) Handle<StandardMaterial>);

#[derive(Component)]
pub(crate) struct HomeGlow {
    home: SimId,
    root: Entity,
    panes: Vec<Entity>,
    level: f32,
}

/// Emissive of a lit pane, linear. Deep amber: the first version, at
/// (4.2, 2.1, 0.55), came out of bloom and the tonemapper as a pale cream
/// that read as a white square, not as firelight.
pub(crate) const PANE_EMISSIVE: [f32; 3] = [4.6, 1.55, 0.16];

#[derive(Resource, Default)]
pub(crate) struct GlowHomes(BTreeMap<SimId, Entity>);

#[derive(Resource)]
pub(crate) struct GlowAssets {
    pane_mesh: Handle<Mesh>,
    pane: Handle<StandardMaterial>,
}

/// The nearest `cap` candidates; ties resolve by key so the set is stable.
pub(crate) fn nearest_within_cap<K: Copy + Ord>(
    mut candidates: Vec<(f32, K)>,
    cap: usize,
) -> Vec<K> {
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    candidates.truncate(cap);
    let mut keys: Vec<K> = candidates.into_iter().map(|(_, key)| key).collect();
    keys.sort();
    keys
}

/// A home shows light when it is dark enough and someone keeps it warm or is in.
pub(crate) fn home_glow_target(lamp_level: f32, heated: bool, occupied: bool, live: bool) -> f32 {
    if live && (heated || occupied) {
        lamp_level
    } else {
        0.0
    }
}

fn glass_emissive(level: f32) -> LinearRgba {
    LinearRgba::new(
        0.10 + 3.3 * level,
        0.07 + 1.75 * level,
        0.03 + 0.38 * level,
        1.0,
    )
}

pub(crate) fn spawn_lanterns(
    mut commands: Commands,
    spec: Res<LivingWorldSpec>,
    terrain: Res<TerrainState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cube = meshes.add(Cuboid::default());
    let wood = materials.add(Color::srgb(0.28, 0.18, 0.11));
    let glass = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.75, 0.34),
        emissive: glass_emissive(0.0),
        ..default()
    });
    commands.insert_resource(LanternGlass(glass.clone()));
    commands.insert_resource(GlowAssets {
        pane_mesh: cube.clone(),
        pane: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.62, 0.24),
            emissive: LinearRgba::new(PANE_EMISSIVE[0], PANE_EMISSIVE[1], PANE_EMISSIVE[2], 1.0),
            ..default()
        }),
    });
    for settlement in &spec.settlements {
        for site in &settlement.sites {
            let p = site.position;
            let y = terrain
                .cell(p.x.div_euclid(1000), p.z.div_euclid(1000))
                .map_or(0.0, |c| c.height_mm as f32 / 1000.0);
            let at = Vec3::new(p.x as f32 / 1000.0 - 2.5, y, p.z as f32 / 1000.0 - 1.0);
            let root = commands
                .spawn((
                    Name::new("village path lantern"),
                    Transform::from_translation(at),
                    Visibility::default(),
                    FogArea::around(at, Vec3::splat(0.5)),
                    FogDim::default(),
                ))
                .id();
            for (position, scale, material) in [
                (
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::new(0.09, 2.0, 0.09),
                    wood.clone(),
                ),
                (
                    Vec3::new(0.0, 1.8, 0.0),
                    Vec3::new(0.23, 0.32, 0.23),
                    glass.clone(),
                ),
            ] {
                commands.spawn((
                    ChildOf(root),
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(material),
                    Transform::from_translation(position).with_scale(scale),
                ));
            }
            commands.spawn((
                ChildOf(root),
                Lantern,
                PointLight {
                    color: Color::srgb(1.0, 0.69, 0.31),
                    intensity: 0.0,
                    range: LANTERN_RANGE,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(0.0, 1.9, 0.0),
                Visibility::Hidden,
            ));
        }
    }
}

/// Glow fittings ride the home's own visual: terrain height and fog
/// visibility are inherited. A settlement home's hang from the frame of its
/// body (`island_homes`), where its model's windows are; homes whose visual
/// or body is not there yet are retried.
pub(crate) fn dress_homes(
    mut commands: Commands,
    assets: Option<Res<GlowAssets>>,
    homes: Query<(Entity, &SimId, Has<Cottage>), With<Household>>,
    visuals: Res<VisualIndex>,
    bodies: Option<Res<Shells>>,
    roots: Query<Option<&HomeShell>, With<Transform>>,
    mut dressed: ResMut<GlowHomes>,
) {
    let Some(assets) = assets else { return };
    dressed.0.retain(|_, glow| roots.contains(*glow));
    for (entity, id, cottage) in &homes {
        if dressed.0.contains_key(id) {
            continue;
        }
        let Some((root, shell)) = visuals
            .map
            .get(&entity)
            .and_then(|root| Some((*root, roots.get(*root).ok()?)))
        else {
            continue;
        };
        // The player's cottage only receives the light; its fuel and its
        // windows are the player's own business.
        let (parent, panes, light_at) = match shell {
            Some(shell) => (
                shell.frame_entity,
                if cottage {
                    Vec::new()
                } else {
                    shell.fit.panes()
                },
                // Outside the door, so the doorstep and the facade are lit.
                Vec3::new(
                    shell.fit.door_x() + 0.3,
                    1.9_f32.min(shell.fit.door_height() + 0.2),
                    -shell.frame.depth * 0.5 - 0.6,
                ),
            ),
            // Its body is on its way.
            None if bodies.is_some() => continue,
            _ => (root, Vec::new(), Vec3::new(0.0, 1.6, 1.4)),
        };
        let panes = panes
            .into_iter()
            .map(|(at, size)| {
                commands
                    .spawn((
                        Name::new("home window glow"),
                        ChildOf(parent),
                        Mesh3d(assets.pane_mesh.clone()),
                        MeshMaterial3d(assets.pane.clone()),
                        Transform::from_translation(at).with_scale(size),
                        Visibility::Hidden,
                        bevy::light::NotShadowCaster,
                    ))
                    .id()
            })
            .collect();
        let glow = commands
            .spawn((
                Name::new("home hearth light"),
                ChildOf(parent),
                HomeGlow {
                    home: *id,
                    root,
                    panes,
                    level: 0.0,
                },
                PointLight {
                    color: Color::srgb(1.0, 0.62, 0.28),
                    intensity: 0.0,
                    range: WINDOW_RANGE,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_translation(light_at),
                Visibility::Hidden,
            ))
            .id();
        dressed.0.insert(*id, glow);
    }
}

fn show(visibility: &mut Mut<Visibility>, on: bool) {
    let want = if on {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if **visibility != want {
        **visibility = want;
    }
}

/// A flame's gentle flicker on the view clock, its own rhythm per light;
/// `depth` 1 for an open lantern, less for a hearth seen through a window.
/// Steady (1) without the island's wind.
fn flame(wind: Option<&crate::island_world::ambient::Wind>, light: Entity, depth: f32) -> f32 {
    wind.map_or(1.0, |wind| {
        let seed = (light.to_bits() % 97) as f32;
        1.0 + (crate::island_world::ambient::flicker(wind.time, seed) - 1.0) * depth
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn night_lights(
    clock: Res<SimClock>,
    config: Res<SimConfig>,
    pinned: Option<Res<ViewDayOverride>>,
    (time, wind): (Res<Time>, Option<Res<crate::island_world::ambient::Wind>>),
    index: Res<SimIndex>,
    cameras: Query<&RtsCamera>,
    glass: Option<Res<LanternGlass>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    homes: Query<(&SimPos, &Household)>,
    positions: Query<&SimPos>,
    fog: Query<&Fogged>,
    mut lanterns: Query<
        (Entity, &GlobalTransform, &mut PointLight, &mut Visibility),
        (With<Lantern>, Without<HomeGlow>),
    >,
    mut glows: Query<(
        Entity,
        &mut HomeGlow,
        &GlobalTransform,
        &mut PointLight,
        &mut Visibility,
    )>,
    mut panes: Query<&mut Visibility, (Without<HomeGlow>, Without<Lantern>)>,
    cut_away: Option<Res<Cutaways>>,
    mut last_level: Local<f32>,
) {
    let level = lamp_level(view_daylight(clock.tick, &config, pinned.as_deref()));
    let focus = cameras
        .iter()
        .next()
        .map_or(Vec3::ZERO, |camera| camera.focus);
    if (level - *last_level).abs() > 0.004 || (level == 0.0) != (*last_level == 0.0) {
        *last_level = level;
        if let Some(mut material) = glass.as_ref().and_then(|glass| materials.get_mut(&glass.0)) {
            material.emissive = glass_emissive(level);
        }
    }
    let lit = if level > 0.0 {
        nearest_within_cap(
            lanterns
                .iter()
                .map(|(entity, at, ..)| (at.translation().distance_squared(focus), entity))
                .collect(),
            MAX_LANTERN_LIGHTS,
        )
    } else {
        Vec::new()
    };
    for (entity, _, mut light, mut visibility) in &mut lanterns {
        let on = lit.binary_search(&entity).is_ok();
        show(&mut visibility, on);
        let intensity = if on {
            LANTERN_LUMENS * level * flame(wind.as_deref(), entity, 1.0)
        } else {
            0.0
        };
        if light.intensity != intensity {
            light.intensity = intensity;
        }
    }

    let step = 1.0 - (-time.delta_secs() * WINDOW_FADE_RATE).exp();
    let mut candidates = Vec::new();
    for (entity, mut glow, at, ..) in &mut glows {
        let state = index
            .get(glow.home)
            .and_then(|home| homes.get(home).ok())
            .map(|(door, home)| {
                let occupied = home.residents.iter().any(|resident| {
                    index
                        .get(*resident)
                        .and_then(|resident| positions.get(resident).ok())
                        .is_some_and(|at| at.dist2(*door) <= AT_HOME_MM2)
                });
                (home.heated_until > clock.tick, occupied)
            });
        let live = fog.get(glow.root).map_or(true, |fog| fog.0 == Look::Live);
        let target = state.map_or(0.0, |(heated, occupied)| {
            home_glow_target(level, heated, occupied, live)
        });
        let next = glow.level + (target - glow.level) * step;
        let next = if (next - target).abs() < 0.003 {
            target
        } else {
            next
        };
        if glow.level != next {
            glow.level = next;
        }
        if next > 0.01 {
            candidates.push((at.translation().distance_squared(focus), entity));
        }
    }
    let lit = nearest_within_cap(candidates, MAX_WINDOW_LIGHTS);
    for (entity, glow, _, mut light, mut visibility) in &mut glows {
        let on = lit.binary_search(&entity).is_ok();
        show(&mut visibility, on);
        let intensity = if on {
            WINDOW_LUMENS * glow.level * flame(wind.as_deref(), entity, 0.6)
        } else {
            0.0
        };
        if light.intensity != intensity {
            light.intensity = intensity;
        }
        // A cut-away home has no walls for its windows to be in.
        let walls = !cut_away
            .as_deref()
            .is_some_and(|cut| cut.is_open(glow.home));
        for pane in &glow.panes {
            if let Ok(mut pane) = panes.get_mut(*pane) {
                show(&mut pane, walls && glow.level > 0.25);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_caps_keep_the_nearest_and_are_stable_under_ties() {
        let candidates: Vec<(f32, u32)> = (0..100).map(|i| ((i % 10) as f32, i)).collect();
        let lit = nearest_within_cap(candidates.clone(), MAX_WINDOW_LIGHTS);
        assert_eq!(lit.len(), MAX_WINDOW_LIGHTS);
        // Distance 0 has ten members, then the six lowest keys at distance 1.
        assert_eq!(
            lit,
            vec![0, 1, 10, 11, 20, 21, 30, 31, 40, 41, 50, 51, 60, 70, 80, 90]
        );
        let mut reversed = candidates;
        reversed.reverse();
        assert_eq!(nearest_within_cap(reversed, MAX_WINDOW_LIGHTS), lit);
        assert_eq!(nearest_within_cap(vec![(3.0, 7_u32)], 24), vec![7]);
        assert!(nearest_within_cap(Vec::<(f32, u32)>::new(), 24).is_empty());
    }

    #[test]
    fn a_lit_home_shows_a_warm_pane_on_every_side() {
        use crate::island_homes::model::{BodyFrame, DOOR_HEIGHT_M, HouseModel, WALL_HEIGHT_M};
        // A founding home and a family house, in their own frames.
        for (lo, hi) in [((0, 0), (3, 4)), ((0, 0), (4, 5))] {
            let frame = BodyFrame::of_cells(lo, hi, 0, SimPos::new(2_000, -1_000));
            let panes = HouseModel::Cottage.fit(&frame).panes();
            let (half_x, half_z) = (frame.width * 0.5, frame.depth * 0.5);
            // A window pair on each side wall, the gable window and a
            // lantern on the door's side.
            for side in [-1.0, 1.0] {
                let on_wall = panes
                    .iter()
                    .filter(|(at, _)| (at.x * side - half_x).abs() < 0.2 && at.z.abs() < half_z)
                    .count();
                assert_eq!(on_wall, 2, "side {side}");
            }
            assert!(
                panes
                    .iter()
                    .any(|(at, _)| at.z < -half_z && at.y > WALL_HEIGHT_M),
                "the gable window, over the door"
            );
            assert!(
                panes
                    .iter()
                    .any(|(at, _)| at.z < -half_z && at.y < DOOR_HEIGHT_M && at.x.abs() > 0.7),
                "a lantern beside the door"
            );
            for (at, size) in panes {
                assert!(at.y > 0.9 && at.y < 4.2, "at window height: {at}");
                // Large enough to read from settlement zoom.
                let mut sides = [size.x, size.y, size.z];
                sides.sort_by(f32::total_cmp);
                assert!(sides[0] <= 0.2 && sides[1] >= 0.2, "{size}");
            }
        }
        // The inn's hall has no household glow.
        let inn = BodyFrame::of_founding_workplace(SimPos::new(0, 0));
        assert!(HouseModel::Hall.fit(&inn).panes().is_empty());
        // Firelight: mostly red, some green, almost no blue.
        let [red, green, blue] = PANE_EMISSIVE;
        assert!(red > 2.5 * green && green > 6.0 * blue, "amber, not cream");
        const { assert!(WINDOW_LUMENS > 12_000.0 && WINDOW_LUMENS < LANTERN_LUMENS) };
    }

    #[test]
    fn homes_glow_only_when_dark_and_kept_and_currently_seen() {
        assert_eq!(home_glow_target(0.0, true, true, true), 0.0);
        assert_eq!(home_glow_target(1.0, false, false, true), 0.0);
        assert_eq!(home_glow_target(0.6, true, false, true), 0.6);
        assert_eq!(home_glow_target(0.6, false, true, true), 0.6);
        // A home the fog only remembers must not reveal who is inside.
        assert_eq!(home_glow_target(1.0, true, true, false), 0.0);
    }

    fn household(residents: Vec<SimId>, heated_until: u64) -> Household {
        Household {
            label: "Home".into(),
            capacity: 2,
            residents,
            door: SimPos::new(0, 0),
            heated_until,
            logs_burned: 0,
        }
    }

    #[test]
    fn the_scene_never_lights_more_than_the_cap_and_never_writes_the_sim() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<SimIndex>()
            .init_resource::<VisualIndex>()
            .init_resource::<GlowHomes>()
            .insert_resource(SimConfig::default())
            .insert_resource(SimClock {
                tick: SimConfig::default().day_length - 200,
            })
            .add_systems(Update, (dress_homes, night_lights).chain());
        let pane_mesh = Handle::default();
        let pane = Handle::default();
        app.insert_resource(GlowAssets { pane_mesh, pane });
        let mut sims = Vec::new();
        for i in 0..40_u32 {
            let id = SimId(i + 1);
            let home = household(vec![], if i == 39 { 0 } else { u64::MAX });
            let sim = app
                .world_mut()
                .spawn((id, SimPos::new(i as i32 * 9000, 0), home.clone()))
                .id();
            let visual = app
                .world_mut()
                .spawn((
                    Transform::from_xyz(i as f32 * 9.0, 0.0, 0.0),
                    Visibility::default(),
                    Fogged(if i == 0 { Look::Dim } else { Look::Live }),
                ))
                .id();
            app.world_mut()
                .resource_mut::<SimIndex>()
                .map
                .insert(id, sim);
            app.world_mut()
                .resource_mut::<VisualIndex>()
                .map
                .insert(sim, visual);
            sims.push((sim, home));
        }
        for _ in 0..600 {
            app.update();
        }
        let world = app.world_mut();
        let mut lights = world.query::<(&HomeGlow, &PointLight, &Visibility)>();
        let all: Vec<_> = lights.iter(world).collect();
        assert_eq!(all.len(), 40);
        let lit: Vec<_> = all
            .iter()
            .filter(|(_, light, visibility)| {
                light.intensity > 0.0 && **visibility != Visibility::Hidden
            })
            .collect();
        assert_eq!(lit.len(), MAX_WINDOW_LIGHTS);
        // The dimmed (remembered) home and the cold empty home stay dark.
        for (glow, light, _) in &all {
            if glow.home == SimId(1) || glow.home == SimId(40) {
                assert_eq!(light.intensity, 0.0);
                assert_eq!(glow.level, 0.0);
            }
        }
        for (sim, home) in sims {
            assert_eq!(app.world().get::<Household>(sim), Some(&home));
            assert!(app.world().get::<Transform>(sim).is_none());
            assert!(app.world().get::<Children>(sim).is_none());
        }
    }
}
