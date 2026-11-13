//! A small role-coloured dot above each resident at settlement zoom, where a
//! figure is only a few pixels tall. It fades out up close, where the model
//! and its caption are readable, and at the overview, where forty dots would
//! be clutter. Each dot has a thin dark rim, so a pale dot still reads over
//! pale ground; `legend` names the colours. Markers are view entities of
//! their own; residents are read.
//!
//! The rim is part of the dot's own mesh: a larger, near-black sphere whose
//! triangles come first, then the dot's. Both are blended without writing
//! depth, so within the one draw the dot always lands on the rim. As two
//! entities told apart by a depth bias, the pair's order was the renderer's
//! transparent sort to decide, and in a capture some rims were drawn over
//! their dots: black discs from one side of the island, correct from the
//! other.
use std::collections::BTreeMap;

use bevy::prelude::*;
use npc_framework::view_helpers::{ViewHidden, VisualOf};
use testbed_content::village::{Role, Villager, Vitality};

use super::night::nearest_within_cap;
use crate::RtsCamera;
use crate::visuals::ActorVisual;

/// Dots drawn at once, nearest the camera focus first.
pub(crate) const MAX_MARKERS: usize = 96;
/// Camera distances (m) of the fade in and the fade out.
const FADE_IN: (f32, f32) = (24.0, 38.0);
const FADE_OUT: (f32, f32) = (190.0, 280.0);
/// Marker diameter as a share of the camera distance: about nine pixels of a
/// 720-pixel-high view at any zoom.
const SIZE_PER_METRE: f32 = 0.0105;
/// Above the head of a standing resident.
const HEAD_CLEARANCE: f32 = 2.15;
/// The rim: a dark shell this much larger than the dot, drawn before it.
/// 1.3 leaves about 15 % of the dot's diameter showing all round.
pub(crate) const OUTLINE_SCALE: f32 = 1.3;
pub(crate) const OUTLINE: Color = Color::srgb(0.05, 0.06, 0.08);

#[derive(Component)]
pub(crate) struct ResidentMarker {
    root: Entity,
}

#[derive(Resource)]
pub(crate) struct MarkerAssets {
    mesh: Handle<Mesh>,
    materials: BTreeMap<u8, Handle<StandardMaterial>>,
}

/// A sphere of `radius` whose every vertex carries `shade`, which the
/// material's colour is multiplied by.
fn shaded_sphere(radius: f32, shade: [f32; 4]) -> Mesh {
    let sphere = Sphere::new(radius).mesh().ico(2).expect("marker sphere");
    let vertices = sphere.count_vertices();
    sphere.with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![shade; vertices])
}

/// The marker's one mesh: the rim's triangles, then the dot's. The dot takes
/// the material's role colour as it is; the rim takes it darkened to the
/// outline's near-black.
pub(crate) fn marker_mesh() -> Mesh {
    let rim = OUTLINE.to_linear();
    let mut mesh = shaded_sphere(0.5 * OUTLINE_SCALE, [rim.red, rim.green, rim.blue, 1.0]);
    mesh.merge(&shaded_sphere(0.5, [1.0; 4]))
        .expect("two spheres with the same attributes");
    mesh
}

#[derive(Resource, Default)]
pub(crate) struct Markers {
    by_root: BTreeMap<Entity, Entity>,
    opacity: f32,
}

/// Opacity of the markers for a camera distance: none up close, full at
/// settlement zoom, none again at the overview.
pub(crate) fn marker_fade(distance: f32) -> f32 {
    let ramp = |(from, to): (f32, f32)| {
        let t = ((distance - from) / (to - from)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    ramp(FADE_IN) * (1.0 - ramp(FADE_OUT))
}

/// World diameter that keeps a marker the same size on screen.
pub(crate) fn marker_size(distance: f32) -> f32 {
    (distance * SIZE_PER_METRE).clamp(0.3, 3.6)
}

/// One clear hue per trade (sRGB), kept apart from the grass and roof tones.
pub(crate) fn role_color(role: Role) -> [f32; 3] {
    match role {
        Role::Visitor => [0.96, 0.96, 0.96],
        Role::Farmer => [0.98, 0.84, 0.20],
        Role::Miller => [0.90, 0.80, 0.55],
        Role::Baker => [0.96, 0.60, 0.22],
        Role::WaterKeeper => [0.30, 0.74, 0.98],
        Role::Woodcutter => [0.36, 0.86, 0.36],
        Role::Carpenter => [0.80, 0.56, 0.30],
        Role::Miner => [0.66, 0.70, 0.80],
        Role::Smith => [0.96, 0.32, 0.26],
        Role::Innkeeper => [0.80, 0.46, 0.92],
        Role::Hunter => [0.20, 0.62, 0.44],
        Role::Tanner => [0.72, 0.36, 0.22],
        Role::Tailor => [0.98, 0.52, 0.74],
        Role::Fisher => [0.26, 0.48, 0.92],
        // A trade only the world's data defines: a hue of its own, stable in
        // its key.
        Role::Trade(key) => trade_color(key),
    }
}

/// Hues for trades only the world's data defines, kept apart from the
/// shipped trades' colours, the grass and the roofs.
pub(crate) const TRADE_PALETTE: [[f32; 3]; 6] = [
    [0.18, 0.80, 0.74],
    [0.62, 0.66, 0.16],
    [0.44, 0.40, 0.88],
    [0.56, 0.22, 0.44],
    [0.54, 0.86, 0.94],
    [0.74, 0.58, 0.08],
];

/// The legend colour of a data-only trade: its key hashed (FNV-1a, so the
/// same in every run) onto [`TRADE_PALETTE`].
pub(crate) fn trade_color(key: &str) -> [f32; 3] {
    let hash = key.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    TRADE_PALETTE[hash as usize % TRADE_PALETTE.len()]
}

fn material_color(role: Role, opacity: f32) -> Color {
    let [r, g, b] = role_color(role);
    Color::srgba(r, g, b, opacity)
}

pub(crate) const ROLES: [Role; 14] = [
    Role::Visitor,
    Role::Farmer,
    Role::Miller,
    Role::Baker,
    Role::WaterKeeper,
    Role::Woodcutter,
    Role::Carpenter,
    Role::Miner,
    Role::Smith,
    Role::Innkeeper,
    Role::Hunter,
    Role::Tanner,
    Role::Tailor,
    Role::Fisher,
];

pub(crate) fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(MarkerAssets {
        mesh: meshes.add(marker_mesh()),
        materials: ROLES
            .into_iter()
            .map(|role| {
                (
                    role.tag(),
                    materials.add(StandardMaterial {
                        base_color: material_color(role, 0.0),
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        fog_enabled: false,
                        ..default()
                    }),
                )
            })
            .collect(),
    });
}

/// One marker per living resident that has a visual; gone with its visual.
pub(crate) fn sync(
    mut commands: Commands,
    assets: Option<Res<MarkerAssets>>,
    mut markers: ResMut<Markers>,
    roots: Query<(Entity, &VisualOf), With<ActorVisual>>,
    residents: Query<(&Villager, Option<&Vitality>)>,
) {
    let Some(assets) = assets else { return };
    let mut wanted = BTreeMap::new();
    for (root, of) in &roots {
        if let Ok((villager, vitality)) = residents.get(of.0)
            && vitality.is_none_or(|vitality| vitality.died_at.is_none())
        {
            wanted.insert(root, villager.role);
        }
    }
    markers.by_root.retain(|root, marker| {
        let keep = wanted.contains_key(root);
        if !keep {
            commands.entity(*marker).try_despawn();
        }
        keep
    });
    for (root, role) in wanted {
        if markers.by_root.contains_key(&root) {
            continue;
        }
        let Some(material) = assets.materials.get(&role.tag()) else {
            continue;
        };
        let marker = commands
            .spawn((
                Name::new("resident marker"),
                ResidentMarker { root },
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::default(),
                Visibility::Hidden,
                bevy::light::NotShadowCaster,
                bevy::light::NotShadowReceiver,
            ))
            .id();
        markers.by_root.insert(root, marker);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn present(
    cameras: Query<&RtsCamera>,
    assets: Option<Res<MarkerAssets>>,
    mut state: ResMut<Markers>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    hidden: Option<Res<ViewHidden>>,
    roots: Query<(&Transform, &Visibility, &VisualOf), Without<ResidentMarker>>,
    mut markers: Query<(Entity, &ResidentMarker, &mut Transform, &mut Visibility)>,
) {
    let (Some(assets), Some(camera)) = (assets, cameras.iter().next()) else {
        return;
    };
    let opacity = marker_fade(camera.distance);
    if (opacity - state.opacity).abs() > 0.004 || (opacity == 0.0) != (state.opacity == 0.0) {
        state.opacity = opacity;
        for role in ROLES {
            if let Some(mut material) = assets
                .materials
                .get(&role.tag())
                .and_then(|handle| materials.get_mut(handle))
            {
                material.base_color = material_color(role, opacity);
            }
        }
    }
    let shown = |root: Entity| {
        roots.get(root).ok().filter(|(_, visibility, of)| {
            **visibility != Visibility::Hidden
                && !hidden.as_deref().is_some_and(|hidden| hidden.hides(of.0))
        })
    };
    let lit = if opacity > 0.0 {
        nearest_within_cap(
            markers
                .iter()
                .filter_map(|(entity, marker, ..)| {
                    let (at, ..) = shown(marker.root)?;
                    Some((at.translation.distance_squared(camera.focus), entity))
                })
                .collect(),
            MAX_MARKERS,
        )
    } else {
        Vec::new()
    };
    let size = marker_size(camera.distance);
    for (entity, marker, mut transform, mut visibility) in &mut markers {
        let on = lit.binary_search(&entity).is_ok();
        let want = if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != want {
            *visibility = want;
        }
        if !on {
            continue;
        }
        if let Some((at, ..)) = shown(marker.root) {
            let want = Transform::from_translation(
                at.translation + Vec3::Y * (HEAD_CLEARANCE + size * 0.5),
            )
            .with_scale(Vec3::splat(size));
            if *transform != want {
                *transform = want;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_data_only_trade_has_its_own_stable_colour_apart_from_the_shipped_ones() {
        let shipped: Vec<[f32; 3]> = [
            Role::Visitor,
            Role::Farmer,
            Role::Miller,
            Role::Baker,
            Role::WaterKeeper,
            Role::Woodcutter,
            Role::Carpenter,
            Role::Miner,
            Role::Smith,
            Role::Innkeeper,
            Role::Hunter,
            Role::Tanner,
            Role::Tailor,
            Role::Fisher,
        ]
        .map(role_color)
        .to_vec();
        let distance = |a: [f32; 3], b: [f32; 3]| {
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
        };
        for hue in TRADE_PALETTE {
            for known in &shipped {
                assert!(
                    distance(hue, *known) > 0.15,
                    "{hue:?} is too near {known:?}"
                );
            }
        }
        let weaver = Role::from_key("weaver");
        assert_eq!(role_color(weaver), trade_color("weaver"));
        assert_eq!(role_color(weaver), role_color(Role::from_key("weaver")));
        assert!(TRADE_PALETTE.contains(&role_color(Role::from_key("glassblower"))));
        let keys = ["weaver", "potter", "brewer", "cooper", "dyer", "chandler"];
        let hues: std::collections::BTreeSet<String> = keys
            .iter()
            .map(|key| format!("{:?}", trade_color(key)))
            .collect();
        assert!(hues.len() >= 3, "keys spread over the palette: {hues:?}");
    }
    use npc_framework::prelude::{SimId, SimPos};

    #[test]
    fn markers_show_only_at_settlement_zoom() {
        // Up close the model and caption speak for themselves.
        for distance in [5.0_f32, 14.0, 24.0] {
            assert_eq!(marker_fade(distance), 0.0, "{distance} m");
        }
        // Medium zoom: fully shown.
        for distance in [38.0_f32, 62.0, 120.0, 190.0] {
            assert_eq!(marker_fade(distance), 1.0, "{distance} m");
        }
        // The overview is left clean.
        for distance in [280.0_f32, 330.0, 600.0] {
            assert_eq!(marker_fade(distance), 0.0, "{distance} m");
        }
        // Both ends are ramps: no wheel notch pops a marker in or out.
        let mut distance = 5.0_f32;
        let mut previous = marker_fade(distance);
        let mut partial = 0;
        while distance < 600.0 {
            distance /= 0.9;
            let fade = marker_fade(distance);
            assert!((fade - previous).abs() < 0.45, "pop at {distance} m");
            if fade > 0.0 && fade < 1.0 {
                partial += 1;
            }
            previous = fade;
        }
        assert!(partial >= 4);
    }

    #[test]
    fn markers_keep_their_screen_size_and_every_trade_its_own_colour() {
        let pixels = |distance: f32| marker_size(distance) / distance;
        assert!((pixels(40.0) - pixels(200.0)).abs() < 1e-6);
        assert!(marker_size(1.0) >= 0.3 && marker_size(5000.0) <= 3.6);
        for (i, a) in ROLES.into_iter().enumerate() {
            for b in ROLES.into_iter().skip(i + 1) {
                let (a, b) = (role_color(a), role_color(b));
                let apart = (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs();
                assert!(apart > 0.25, "{a:?} and {b:?} are too alike");
            }
        }
        let tags: std::collections::BTreeSet<u8> = ROLES.into_iter().map(Role::tag).collect();
        assert_eq!(tags.len(), ROLES.len());
    }

    fn villager(role: Role) -> Villager {
        Villager {
            role,
            name: "Resident".into(),
            home: SimPos::new(0, 0),
            workplace: SimId(1),
            base_speed: 60,
            next_decision: 0,
            resting_until: 0,
            rest_rate: 0,
            life: 1,
        }
    }

    fn app(distance: f32) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Markers>()
            .add_systems(Startup, setup)
            .add_systems(Update, (sync, present).chain());
        let camera = app
            .world_mut()
            .spawn(RtsCamera {
                focus: Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.8,
                distance,
            })
            .id();
        (app, camera)
    }

    fn resident(app: &mut App, role: Role, at: Vec3) -> (Entity, Entity) {
        let sim = app.world_mut().spawn(villager(role)).id();
        let root = app
            .world_mut()
            .spawn((
                VisualOf(sim),
                ActorVisual {
                    kind: crate::assets::ModelKind::Knight,
                    motion: default(),
                    hand_r: None,
                    hand_l: None,
                },
                Transform::from_translation(at),
                Visibility::Inherited,
            ))
            .id();
        (sim, root)
    }

    fn marker_of(app: &mut App, root: Entity) -> Option<(Transform, Visibility)> {
        let world = app.world_mut();
        world
            .query::<(&ResidentMarker, &Transform, &Visibility)>()
            .iter(world)
            .find(|(marker, ..)| marker.root == root)
            .map(|(_, transform, visibility)| (*transform, *visibility))
    }

    #[test]
    fn a_marker_follows_its_resident_and_never_touches_the_simulation() {
        let (mut app, camera) = app(62.0);
        let (sim, root) = resident(&mut app, Role::Smith, Vec3::new(3.0, 4.7, -2.0));
        let before = app.world().get::<Villager>(sim).unwrap().clone();
        app.update();
        app.update();
        let (transform, visibility) = marker_of(&mut app, root).expect("a marker");
        assert_eq!(visibility, Visibility::Inherited);
        assert_eq!(transform.translation.x, 3.0);
        assert_eq!(transform.translation.z, -2.0);
        assert!(transform.translation.y > 4.7 + 2.0);
        assert_eq!(transform.scale, Vec3::splat(marker_size(62.0)));
        // The dot carries its dark rim in its own mesh: one entity, one
        // draw, and no second entity whose order against it could change.
        let marker = {
            let world = app.world_mut();
            world
                .query_filtered::<Entity, With<ResidentMarker>>()
                .single(world)
                .unwrap()
        };
        assert!(app.world().get::<Children>(marker).is_none());
        let mesh = marker_mesh();
        let positions: Vec<Vec3> = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|values| values.as_float3())
            .expect("positions")
            .iter()
            .map(|at| Vec3::from(*at))
            .collect();
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(shades)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("the marker mesh shades its own vertices");
        };
        let indices: Vec<usize> = mesh.indices().expect("indexed").iter().collect();
        // The rim's triangles come first and are the larger, dark sphere; the
        // dot's follow and keep the role colour, so the dot is drawn over it.
        let half = indices.len() / 2;
        for (order, index) in indices.iter().enumerate() {
            let (radius, shade) = (positions[*index].length(), shades[*index]);
            if order < half {
                assert!((radius - 0.5 * OUTLINE_SCALE).abs() < 1e-4, "{radius}");
                assert!(shade[0] < 0.01 && shade[1] < 0.01 && shade[2] < 0.01);
            } else {
                assert!((radius - 0.5).abs() < 1e-4, "{radius}");
                assert_eq!(shade, [1.0; 4]);
            }
            assert_eq!(shade[3], 1.0, "the rim fades with the dot's material");
        }
        let assets = app.world().resource::<MarkerAssets>();
        let materials = app.world().resource::<Assets<StandardMaterial>>();
        let dot = materials
            .get(&assets.materials[&Role::Smith.tag()])
            .unwrap();
        assert_eq!(dot.base_color.alpha(), 1.0);
        assert_eq!(
            dot.alpha_mode,
            AlphaMode::Blend,
            "blended, so no depth is written"
        );
        // A thin rim.
        const { assert!(OUTLINE_SCALE > 1.15 && OUTLINE_SCALE < 1.5) };

        // The sim entity gained nothing and its data is untouched.
        assert_eq!(app.world().get::<Villager>(sim), Some(&before));
        assert!(app.world().get::<Transform>(sim).is_none());
        assert!(app.world().get::<Mesh3d>(sim).is_none());
        assert!(app.world().get::<Visibility>(sim).is_none());
        assert!(app.world().get::<ResidentMarker>(sim).is_none());

        // Zoomed right in, the marker steps aside.
        app.world_mut()
            .get_mut::<RtsCamera>(camera)
            .unwrap()
            .distance = 12.0;
        app.update();
        assert_eq!(marker_of(&mut app, root).unwrap().1, Visibility::Hidden);
        // A resident the fog hides has no marker either.
        app.world_mut()
            .get_mut::<RtsCamera>(camera)
            .unwrap()
            .distance = 62.0;
        *app.world_mut().get_mut::<Visibility>(root).unwrap() = Visibility::Hidden;
        app.update();
        assert_eq!(marker_of(&mut app, root).unwrap().1, Visibility::Hidden);
        // When the visual goes, so does the marker.
        app.world_mut().despawn(root);
        app.update();
        app.update();
        assert!(marker_of(&mut app, root).is_none());
        assert_eq!(
            app.world_mut()
                .query::<&ResidentMarker>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn the_dead_have_no_marker_and_the_count_is_capped() {
        let (mut app, _) = app(62.0);
        let (sim, root) = resident(&mut app, Role::Farmer, Vec3::ZERO);
        app.world_mut().entity_mut(sim).insert(Vitality {
            health: 0,
            starved_ticks: 0,
            dehydrated_ticks: 0,
            died_at: Some(5),
            exhaustion_ticks: 0,
        });
        for i in 0..MAX_MARKERS + 20 {
            resident(&mut app, Role::Baker, Vec3::new(1.0 + i as f32, 0.0, 0.0));
        }
        app.update();
        app.update();
        assert!(marker_of(&mut app, root).is_none());
        let world = app.world_mut();
        let shown = world
            .query::<(&ResidentMarker, &Visibility)>()
            .iter(world)
            .filter(|(_, visibility)| **visibility != Visibility::Hidden)
            .count();
        assert_eq!(shown, MAX_MARKERS);
    }
}
