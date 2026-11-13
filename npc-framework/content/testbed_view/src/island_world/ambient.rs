//! The island's ambient motion: one wind and one view clock that every
//! moving thing that is not a person reads, so they agree. Clouds drift and
//! their shadows slide over the land and the sea, trees sway, smoke bends,
//! rain slants, the sea swells, the boat rocks, lanterns and hearths
//! flicker, gulls circle the harbour and small birds cross the trees by day.
//!
//! Pure atmosphere runs on real time: it keeps moving while the game is
//! paused and is not sped up by 3x or 10x. Anything that stands for sim
//! activity (a turning mill, work smoke, people) follows the sim clock
//! instead and stops with it. [`AmbientMotion::scale`] slows all of it to a
//! standstill at 0 (a future "Reduce motion" setting): the view clock then
//! stops, so nothing that reads it moves. View only: nothing here is sim
//! state or is read by the sim.
use std::f32::consts::TAU;

use bevy::prelude::*;
use npc_framework::prelude::{SimClock, SimConfig};
use testbed_content::village::Season;
use testbed_content::village::frontier::life_spec::Weather;

use crate::camera::RtsCamera;
use crate::island_atmosphere::daylight::{ViewDayOverride, view_daylight};
use crate::island_atmosphere::sky::sun_strength;

/// How much of the ambient motion plays: 1 all of it, 0 none (every
/// transform it drives stays as it is). Public for a "Reduce motion"
/// setting: `testbed_view::island_world::ambient::AmbientMotion` (crate
/// path `crate::island_world::ambient::AmbientMotion`).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct AmbientMotion {
    pub scale: f32,
}

impl Default for AmbientMotion {
    fn default() -> Self {
        Self { scale: 1.0 }
    }
}

/// The one wind of the island this frame, and the view clock that drives
/// all ambient motion.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Wind {
    /// Unit direction the wind blows towards (world x, z).
    pub direction: Vec2,
    /// 0 calm .. about 3 in a storm, gusts included; 1 a fresh breeze.
    pub strength: f32,
    /// Seconds of ambient motion: real time times [`AmbientMotion::scale`].
    /// It does not stop with the sim and is not sped up with it.
    pub time: f32,
}

impl Default for Wind {
    fn default() -> Self {
        Self {
            direction: Vec2::X,
            strength: 1.0,
            time: 0.0,
        }
    }
}

impl Wind {
    /// The turn about the vertical that points +X along the wind.
    pub fn yaw(&self) -> f32 {
        // A yaw turns +X to (cos, -sin) in (x, z).
        (-self.direction.y).atan2(self.direction.x)
    }
}

/// The wind's mean strength under a weather.
pub(crate) fn base_strength(kind: Weather) -> f32 {
    match kind {
        Weather::Clear => 0.7,
        Weather::Rain => 1.3,
        Weather::Snow => 1.0,
        Weather::Storm => 2.6,
    }
}

/// The wind at view time `time`, on day fraction `day` (0..1), under a
/// weather eased in by `amount` from clear: its direction swings slowly
/// over the day, its strength gusts over seconds.
pub(crate) fn sample(kind: Weather, amount: f32, day: f32, time: f32) -> (Vec2, f32) {
    let angle = 0.6 + 0.9 * (TAU * day).sin() + 0.12 * (time * 0.013).sin();
    let base = base_strength(Weather::Clear)
        + (base_strength(kind) - base_strength(Weather::Clear)) * amount.clamp(0.0, 1.0);
    let gust = 0.18 * (time * 0.61).sin() + 0.1 * (time * 1.73 + 1.3).sin();
    (
        Vec2::new(angle.cos(), angle.sin()),
        (base * (1.0 + gust)).max(0.0),
    )
}

/// The view clock's next value: real seconds times the motion scale.
pub(crate) fn advance(time: f32, real_delta: f32, scale: f32) -> f32 {
    time + real_delta.max(0.0) * scale.clamp(0.0, 1.0)
}

/// Update the wind and the view clock, once a frame before anything reads
/// them.
pub(crate) fn update_wind(
    real: Res<Time<Real>>,
    motion: Option<Res<AmbientMotion>>,
    clock: Option<Res<SimClock>>,
    config: Option<Res<SimConfig>>,
    look: Option<Res<super::weather::WeatherLook>>,
    mut wind: ResMut<Wind>,
) {
    let scale = motion.map_or(1.0, |motion| motion.scale);
    // No motion: the wind holds as it is, direction and all.
    if scale <= 0.0 {
        return;
    }
    let time = advance(wind.time, real.delta_secs(), scale);
    let day = clock.zip(config).map_or(0.3, |(clock, config)| {
        (clock.tick % config.day_length.max(1)) as f32 / config.day_length.max(1) as f32
    });
    let (kind, amount) = look.map_or((Weather::Clear, 0.0), |look| (look.kind, look.amount));
    let (direction, strength) = sample(kind, amount, day, time);
    let next = Wind {
        direction,
        strength,
        time,
    };
    if *wind != next {
        *wind = next;
    }
}

/// Where a cloud is at view time `time`: carried from its `anchor` along
/// the wind, faster the higher it is, and wrapped round the island (a
/// square `span` metres across) so it never pops.
pub(crate) fn cloud_at(anchor: Vec3, wind: &Wind, span: f32) -> Vec3 {
    let speed = (0.5 + (anchor.y - 80.0).max(0.0) / 40.0) * 1.6 * wind.strength.max(0.3);
    // Distance travelled is the integral of speed; with gusts small, speed
    // times time keeps the drift smooth.
    let travelled = speed * wind.time;
    let along = Vec2::new(anchor.x, anchor.z).dot(wind.direction) + travelled;
    let wrapped = (along + span * 0.5).rem_euclid(span) - span * 0.5;
    let shift = wind.direction * (wrapped - Vec2::new(anchor.x, anchor.z).dot(wind.direction));
    Vec3::new(anchor.x + shift.x, anchor.y, anchor.z + shift.y)
}

/// How much of the sky is cloud under a weather: thin on a clear day,
/// built up under rain, snow or a storm.
pub(crate) fn cloud_cover(kind: Weather, amount: f32) -> f32 {
    let full = match kind {
        Weather::Clear => 0.45,
        Weather::Rain | Weather::Snow => 0.9,
        Weather::Storm => 1.0,
    };
    0.45 + (full - 0.45) * amount.clamp(0.0, 1.0)
}

/// A light's flicker at view time `time`: a gentle wobble round 1, its own
/// rhythm per `seed`.
pub(crate) fn flicker(time: f32, seed: f32) -> f32 {
    1.0 + 0.07 * (time * 7.3 + seed * 1.7).sin() + 0.04 * (time * 13.1 + seed * 3.1).sin()
}

// ----- Cloud shadows -----

/// Camera height over the focus from which the cloud shadows show, and the
/// metres over which they fade in. From the island overview nobody sees the
/// sky: the shadows are the clouds, so they are the main effect there.
const SHADOW_FROM_M: f32 = 16.0;
const SHADOW_OVER_M: f32 = 22.0;
/// Metres of ground one tile of the shadow texture covers.
const SHADOW_TILE_M: f32 = 240.0;
/// How dark a shadow's core is: about a fifth darker on grass, a clear
/// grey-blue on snow.
const SHADOW_OPACITY: f32 = 0.5;
/// The shadows' height over the sea: over the roofs and most hills, so the
/// patches fall on everything below.
const SHADOW_LAYER_Y: f32 = 14.0;
/// Metres a second the patches ride at a fresh breeze: one crosses a house
/// in about five seconds.
const SHADOW_SPEED_M_PER_S: f32 = 1.6;
const SHADOW_TEXELS: usize = 256;

/// Alpha of the shadow texture at `(u, v)` in 0..1, tileable: soft-edged
/// patches over about two fifths of the ground.
pub(crate) fn shadow_alpha(u: f32, v: f32) -> f32 {
    // Blobs on the torus: centre, radius (both in tile units).
    const BLOBS: [(f32, f32, f32); 9] = [
        (0.12, 0.18, 0.13),
        (0.42, 0.10, 0.10),
        (0.70, 0.30, 0.15),
        (0.25, 0.55, 0.11),
        (0.55, 0.62, 0.14),
        (0.88, 0.70, 0.09),
        (0.10, 0.86, 0.10),
        (0.62, 0.92, 0.08),
        (0.86, 0.06, 0.07),
    ];
    let mut field = 0.0_f32;
    for (x, y, r) in BLOBS {
        let dx = ((u - x + 0.5).rem_euclid(1.0)) - 0.5;
        let dy = ((v - y + 0.5).rem_euclid(1.0)) - 0.5;
        let d2 = (dx * dx + dy * dy) / (r * r);
        field += (-d2 * 1.6).exp();
    }
    // Ragged edges: a little wobble on the field.
    let wobble = 0.08 * ((u * 31.0).sin() * (v * 27.0).cos() + (u * 17.0 + v * 23.0).sin());
    let t = ((field + wobble - 0.12) / 0.3).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn shadow_image() -> Image {
    let mut data = Vec::with_capacity(SHADOW_TEXELS * SHADOW_TEXELS * 4);
    for row in 0..SHADOW_TEXELS {
        for column in 0..SHADOW_TEXELS {
            let at = |texel: usize| (texel as f32 + 0.5) / SHADOW_TEXELS as f32;
            let alpha = (shadow_alpha(at(column), at(row)) * 255.0).round() as u8;
            data.extend([255, 255, 255, alpha]);
        }
    }
    let mut image = Image::new(
        bevy::render::render_resource::Extent3d {
            width: SHADOW_TEXELS as u32,
            height: SHADOW_TEXELS as u32,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        data,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        address_mode_u: bevy::image::ImageAddressMode::Repeat,
        address_mode_v: bevy::image::ImageAddressMode::Repeat,
        mag_filter: bevy::image::ImageFilterMode::Linear,
        min_filter: bevy::image::ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// How strong the cloud shadows are under a weather: full in sun, gone
/// under a full overcast or a storm (nothing casts a shadow then).
pub(crate) fn shadow_strength(kind: Weather, amount: f32) -> f32 {
    if kind == Weather::Clear {
        1.0
    } else {
        1.0 - amount.clamp(0.0, 1.0)
    }
}

/// The shadow layer: one square over the focus.
#[derive(Resource, Default)]
pub(crate) struct CloudShadows {
    entity: Option<Entity>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn cloud_shadows(
    mut commands: Commands,
    mut state: ResMut<CloudShadows>,
    wind: Res<Wind>,
    look: Option<Res<super::weather::WeatherLook>>,
    clock: Option<Res<SimClock>>,
    config: Option<Res<SimConfig>>,
    pinned: Option<Res<ViewDayOverride>>,
    cameras: Query<(&RtsCamera, &GlobalTransform)>,
    mut layers: Query<(
        &mut Transform,
        &mut Visibility,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some((camera, eye)) = cameras.iter().next() else {
        return;
    };
    let Some(entity) = state.entity else {
        let texture = images.add(shadow_image());
        let material = materials.add(StandardMaterial {
            base_color: Color::linear_rgba(0.03, 0.05, 0.11, 0.0),
            base_color_texture: Some(texture),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            double_sided: true,
            fog_enabled: false,
            ..default()
        });
        let mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(0.5)).mesh());
        state.entity = Some(
            commands
                .spawn((
                    Name::new("cloud shadows"),
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    Transform::default(),
                    Visibility::Hidden,
                    bevy::light::NotShadowCaster,
                    bevy::light::NotShadowReceiver,
                ))
                .id(),
        );
        return;
    };
    let Ok((mut transform, mut visibility, material)) = layers.get_mut(entity) else {
        return;
    };
    let above = eye.translation().y - SHADOW_LAYER_Y.max(camera.focus.y + 4.0);
    let t = ((above - SHADOW_FROM_M) / SHADOW_OVER_M).clamp(0.0, 1.0);
    let share = t * t * (3.0 - 2.0 * t);
    // The sun makes the shadow: none at night.
    let sun = clock.zip(config).map_or(1.0, |(clock, config)| {
        sun_strength(view_daylight(clock.tick, &config, pinned.as_deref()))
    });
    let (kind, amount) = look.map_or((Weather::Clear, 0.0), |look| (look.kind, look.amount));
    // A low sun still casts them: they fade only towards dusk.
    let opacity = SHADOW_OPACITY * share * sun.sqrt() * shadow_strength(kind, amount);
    if opacity <= 0.002 {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    visibility.set_if_neq(Visibility::Inherited);
    let size = (above * 8.0).max(500.0);
    transform.translation = Vec3::new(
        camera.focus.x,
        SHADOW_LAYER_Y.max(camera.focus.y + 4.0),
        camera.focus.z,
    );
    transform.scale = Vec3::new(size, 1.0, size);
    let tiles = size / SHADOW_TILE_M;
    // The pattern rides with the wind, anchored to the ground.
    let drift =
        wind.direction * SHADOW_SPEED_M_PER_S * (0.5 + 0.5 * wind.strength.max(0.4)) * wind.time
            / SHADOW_TILE_M;
    let offset =
        Vec2::new(camera.focus.x, camera.focus.z) / SHADOW_TILE_M - Vec2::splat(tiles * 0.5);
    if let Some(mut material) = materials.get_mut(&material.0) {
        material.base_color = Color::linear_rgba(0.03, 0.05, 0.11, opacity);
        material.uv_transform = bevy::math::Affine2::from_scale_angle_translation(
            Vec2::splat(tiles),
            0.0,
            offset - drift,
        );
    }
}

// ----- Flags -----

/// A flag on a pole: it turns to stream downwind (its cloth along the
/// model's +Z) and flaps.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct WindFlag {
    pub(crate) seed: f32,
}

/// A flag's turn in the world: +Z downwind, flapping with the gusts.
pub(crate) fn flag_turn(wind: &Wind, seed: f32) -> Quat {
    let stream = wind.direction.x.atan2(wind.direction.y);
    let flap = ((wind.time * 5.1 + seed * 3.0).sin() * 0.6 + (wind.time * 8.3 + seed).sin() * 0.4)
        * 0.14
        * wind.strength.clamp(0.3, 3.0);
    Quat::from_rotation_y(stream + flap)
}

pub(crate) fn flags(
    wind: Res<Wind>,
    parents: Query<&GlobalTransform>,
    mut flags: Query<(&WindFlag, &ChildOf, &mut Transform)>,
) {
    for (flag, parent, mut transform) in &mut flags {
        let parent_turn = parents
            .get(parent.parent())
            .map_or(Quat::IDENTITY, |parent| parent.compute_transform().rotation);
        let next = parent_turn.inverse() * flag_turn(&wind, flag.seed);
        if transform.rotation != next {
            transform.rotation = next;
        }
    }
}

// ----- Crops -----

/// A crop's rows: they lean and spring back as gusts run over the field.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct Ripple;

/// A crop's lean at world point `at`: a wave running downwind over the
/// field, stronger in a blow.
pub(crate) fn ripple_lean(wind: &Wind, at: Vec2) -> f32 {
    let along = at.dot(wind.direction);
    let wave = (wind.time * 1.7 - along * 0.35).sin() * 0.5 + 0.5;
    (0.03 + 0.09 * wave * wave) * wind.strength.clamp(0.2, 2.6)
}

pub(crate) fn ripple(
    wind: Res<Wind>,
    mut rows: Query<(&GlobalTransform, &mut Transform), With<Ripple>>,
) {
    for (at, mut transform) in &mut rows {
        let p = at.translation();
        let lean = ripple_lean(&wind, Vec2::new(p.x, p.z));
        let next = Quat::from_rotation_x(lean);
        if transform.rotation != next {
            transform.rotation = next;
        }
    }
}

// ----- Birds -----

/// A gull circling the harbour or following the ship, or a small bird of
/// a flock crossing over the trees.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct Bird {
    /// 0 a gull at the harbour, 1 a gull after the ship, 2 a small bird.
    kind: u8,
    seed: f32,
}

/// One wing of a bird: it beats about the body's long axis.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct Wing {
    side: f32,
}

#[derive(Resource, Default)]
pub(crate) struct Birds {
    spawned: bool,
}

/// Gulls at the harbour, gulls after the ship, small birds (in flocks).
const GULLS: usize = 8;
const SHIP_GULLS: usize = 3;
const FLOCKS: usize = 2;
const FLOCK_BIRDS: usize = 5;

/// Where a gull is at view time `time`: on a circle `radius` round
/// `centre`, `height` up, its own phase and pace, banking into the turn.
pub(crate) fn gull_at(centre: Vec3, seed: f32, time: f32) -> (Vec3, f32) {
    let radius = 8.0 + 7.0 * (seed * 1.3).fract();
    let pace = 0.18 + 0.08 * (seed * 2.1).fract();
    let angle = time * pace + seed * TAU;
    let height = 8.0 + 4.0 * (seed * 3.7).fract() + 1.5 * (time * 0.3 + seed).sin();
    let at = centre + Vec3::new(angle.cos() * radius, height, angle.sin() * radius);
    // Heading along the circle.
    (at, -angle)
}

/// Where a small bird is: its flock crosses over `focus` from tree to tree
/// on a slow arc, one crossing every 11 s, out of sight between; each bird
/// keeps its own place in the flock.
pub(crate) fn songbird_at(focus: Vec3, seed: f32, time: f32) -> Option<(Vec3, f32)> {
    let flock = seed.floor();
    let member = seed.fract();
    let cycle = 11.0;
    let t = (time / cycle + flock * 0.43).fract();
    if t > 0.6 {
        return None;
    }
    let along = t / 0.6;
    let heading = flock * 2.4 + 0.7;
    let dir = Vec3::new(heading.cos(), 0.0, heading.sin());
    let side = Vec3::new(-dir.z, 0.0, dir.x);
    let place = Vec3::new(
        (member * 7.3).fract() * 3.0 - 1.5,
        (member * 3.1).fract() * 1.2,
        (member * 5.7).fract() * 2.5 - 1.25,
    );
    let at = focus
        + dir * (along * 46.0 - 23.0 + place.z)
        + side * (place.x + 4.0 * (flock - 0.5))
        + Vec3::Y * (5.5 + place.y + 2.5 * (along * std::f32::consts::PI).sin())
        // A flock bobs as it flies.
        + Vec3::Y * 0.3 * (time * 6.0 + member * 9.0).sin();
    // Heading: +Z of the bird along `dir`.
    Some((at, dir.x.atan2(dir.z)))
}

/// A gull's wings at view time `time`: mostly a glide, held a little up;
/// a few beats every six to nine seconds.
pub(crate) fn gull_wings(time: f32, seed: f32) -> f32 {
    let cycle = 6.0 + 3.0 * (seed * 3.7).fract();
    let t = (time / cycle + seed).fract() * cycle;
    if t < 1.4 {
        (time * 7.5 + seed * 7.0).sin() * 0.55 + 0.1
    } else {
        0.12 + 0.03 * (time * 0.8 + seed).sin()
    }
}

/// A bird's meshes at `length` metres beak to tail: the body, then the
/// right wing's inner and outer panels and the left's (mirrored). The
/// wings are hinged at the body's middle and reach out along +X (right)
/// or -X (left).
pub(crate) fn bird_meshes(length: f32) -> [Mesh; 5] {
    let l = length;
    // A tapered body: a beak at +Z, a tail at -Z, widest a little ahead of
    // the middle.
    let nose = Vec3::new(0.0, 0.0, 0.5 * l);
    let tail = Vec3::new(0.0, 0.02 * l, -0.48 * l);
    let left = Vec3::new(-0.12 * l, 0.0, 0.08 * l);
    let right = Vec3::new(0.12 * l, 0.0, 0.08 * l);
    let up = Vec3::new(0.0, 0.1 * l, 0.08 * l);
    let down = Vec3::new(0.0, -0.08 * l, 0.08 * l);
    let fan_l = Vec3::new(-0.1 * l, 0.02 * l, -0.56 * l);
    let fan_r = Vec3::new(0.1 * l, 0.02 * l, -0.56 * l);
    let body = triangles(&[
        [nose, up, left],
        [nose, right, up],
        [nose, down, right],
        [nose, left, down],
        [tail, left, up],
        [tail, up, right],
        [tail, right, down],
        [tail, down, left],
        [tail, fan_r, fan_l],
    ]);
    // The right wing: an inner panel, then an outer one swept back and bent
    // down to a point.
    let span = 1.3 * l;
    let root_front = Vec3::new(0.05 * l, 0.0, 0.16 * l);
    let root_back = Vec3::new(0.05 * l, 0.0, -0.12 * l);
    let elbow_front = Vec3::new(0.48 * span, 0.02 * l, 0.1 * l);
    let elbow_back = Vec3::new(0.48 * span, 0.02 * l, -0.12 * l);
    let tip = Vec3::new(span, -0.08 * l, -0.26 * l);
    let tip_back = Vec3::new(0.94 * span, -0.07 * l, -0.32 * l);
    let inner = [
        [root_front, elbow_front, elbow_back],
        [root_front, elbow_back, root_back],
    ];
    let outer = [
        [elbow_front, tip, tip_back],
        [elbow_front, tip_back, elbow_back],
    ];
    let mirror = |tris: &[[Vec3; 3]]| -> Vec<[Vec3; 3]> {
        tris.iter()
            .map(|[a, b, c]| {
                let m = |v: &Vec3| Vec3::new(-v.x, v.y, v.z);
                [m(a), m(c), m(b)]
            })
            .collect()
    };
    [
        body,
        triangles(&inner),
        triangles(&outer),
        triangles(&mirror(&inner)),
        triangles(&mirror(&outer)),
    ]
}

/// A flat-shaded mesh of triangles.
fn triangles(tris: &[[Vec3; 3]]) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    for [a, b, c] in tris {
        let normal = (*b - *a).cross(*c - *a).normalize_or(Vec3::Y);
        for v in [a, b, c] {
            positions.push(v.to_array());
            normals.push(normal.to_array());
        }
    }
    Mesh::new(
        bevy::mesh::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
}

/// A wing's beat at view time `time`: radians up (+) or down.
pub(crate) fn wing_beat(time: f32, rate: f32, seed: f32) -> f32 {
    (time * rate + seed * 7.0).sin() * 0.55 + 0.15
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn birds(
    mut commands: Commands,
    mut state: ResMut<Birds>,
    wind: Res<Wind>,
    harbour: Option<Res<super::harbour::Harbour>>,
    boat: Option<Res<super::boat::BoatScene>>,
    (clock, config, pinned): (
        Option<Res<SimClock>>,
        Option<Res<SimConfig>>,
        Option<Res<ViewDayOverride>>,
    ),
    cameras: Query<&RtsCamera>,
    places: Query<(&GlobalTransform, &Visibility), Without<Bird>>,
    mut flock: Query<(&Bird, &Children, &mut Transform, &mut Visibility), Without<Wing>>,
    mut wings: Query<(&Wing, &mut Transform), Without<Bird>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !state.spawned {
        state.spawned = true;
        // A bird: a tapered body and two wings in two panels each, the
        // inner one swept back a little and the outer one more and bent
        // down; a gull's outer panels are grey. One shared mesh per part
        // and kind, so they batch.
        let gull = bird_meshes(0.5);
        let small = bird_meshes(0.18);
        let parts = |set: [Mesh; 5], meshes: &mut Assets<Mesh>| set.map(|mesh| meshes.add(mesh));
        let gull = parts(gull, &mut meshes);
        let small = parts(small, &mut meshes);
        let mut matte = |r: f32, g: f32, b: f32| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(r, g, b),
                perceptual_roughness: 0.9,
                cull_mode: None,
                double_sided: true,
                ..default()
            })
        };
        let (white, grey, dark) = (
            matte(0.95, 0.95, 0.93),
            matte(0.5, 0.53, 0.58),
            matte(0.2, 0.17, 0.15),
        );
        let mut seeds = Vec::new();
        for i in 0..GULLS {
            seeds.push((0_u8, i as f32 * 0.137 + 0.05));
        }
        for i in 0..SHIP_GULLS {
            seeds.push((1_u8, i as f32 * 0.29 + 0.11));
        }
        for flock in 0..FLOCKS {
            for member in 0..FLOCK_BIRDS {
                seeds.push((2_u8, flock as f32 + member as f32 * 0.173 + 0.02));
            }
        }
        for (kind, seed) in seeds {
            let (set, coat, tips) = if kind == 2 {
                (&small, &dark, &dark)
            } else {
                (&gull, &white, &grey)
            };
            commands
                .spawn((
                    Name::new(if kind == 2 { "songbird" } else { "gull" }),
                    Bird { kind, seed },
                    Mesh3d(set[0].clone()),
                    MeshMaterial3d(coat.clone()),
                    Transform::default(),
                    Visibility::Hidden,
                    bevy::light::NotShadowCaster,
                ))
                .with_children(|bird| {
                    for (side, inner, outer) in [(1.0_f32, 1, 2), (-1.0, 3, 4)] {
                        bird.spawn((Wing { side }, Transform::default(), Visibility::Inherited))
                            .with_children(|wing| {
                                wing.spawn((
                                    Mesh3d(set[inner].clone()),
                                    MeshMaterial3d(coat.clone()),
                                    bevy::light::NotShadowCaster,
                                ));
                                wing.spawn((
                                    Mesh3d(set[outer].clone()),
                                    MeshMaterial3d(tips.clone()),
                                    bevy::light::NotShadowCaster,
                                ));
                            });
                    }
                });
        }
        return;
    }
    let camera = cameras.iter().next().copied();
    let near = camera.is_some_and(|camera| camera.distance < 80.0);
    let day = clock
        .as_deref()
        .zip(config.as_deref())
        .map_or(1.0, |(clock, config)| {
            sun_strength(view_daylight(clock.tick, config, pinned.as_deref()))
        });
    let harbour_at = harbour
        .as_deref()
        .and_then(|harbour| harbour.landing.as_ref())
        .map(|landing| Vec3::new(landing.head.x, 0.0, landing.head.y));
    // The ship, when it is drawn.
    let ship_at = boat
        .as_deref()
        .and_then(super::boat::BoatScene::root)
        .and_then(|root| places.get(root).ok())
        .filter(|(_, visibility)| **visibility != Visibility::Hidden)
        .map(|(at, _)| at.translation());
    for (bird, children, mut transform, mut visibility) in &mut flock {
        let placed = match bird.kind {
            0 => harbour_at
                .filter(|_| day > 0.2)
                .map(|at| gull_at(at, bird.seed, wind.time)),
            1 => ship_at
                .filter(|_| day > 0.2)
                .map(|at| gull_at(at, bird.seed, wind.time * 1.3)),
            _ => camera
                .filter(|_| near && day > 0.3)
                .and_then(|camera| songbird_at(camera.focus, bird.seed, wind.time)),
        };
        let Some((at, heading)) = placed else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        transform.translation = at;
        transform.rotation = Quat::from_rotation_y(heading)
            * Quat::from_rotation_z(if bird.kind == 2 { 0.0 } else { 0.3 });
        // Gulls glide with their wings held a little up and flap now and
        // then; small birds whirr.
        let beat = if bird.kind == 2 {
            wing_beat(wind.time, 22.0, bird.seed)
        } else {
            gull_wings(wind.time, bird.seed)
        };
        for child in children.iter() {
            if let Ok((wing, mut transform)) = wings.get_mut(child) {
                // Up about the body's long axis on both sides.
                transform.rotation = Quat::from_rotation_z(beat * wing.side);
            }
        }
    }
}

// ----- Small life: butterflies, fireflies, falling leaves -----

/// A mote of small life round the camera's focus.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct Mote {
    kind: MoteKind,
    seed: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MoteKind {
    Butterfly,
    Firefly,
    Leaf,
}

#[derive(Resource, Default)]
pub(crate) struct Motes {
    spawned: bool,
}

const BUTTERFLIES: usize = 10;
const FIREFLIES: usize = 36;
const LEAVES: usize = 30;

/// The season the land and the woods are drawn in (the calendar's, or the
/// view's pinned one): written by the land's own sync.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawnSeason(pub Season);

/// `ISLAND_VIEW_SEASON=spring|summer|autumn|winter`: the season the view
/// draws the land, the woods and the small life in, for reviews and
/// captures; the simulation's own calendar is untouched.
pub(crate) fn view_season() -> Option<Season> {
    static SEASON: std::sync::OnceLock<Option<Season>> = std::sync::OnceLock::new();
    *SEASON.get_or_init(|| {
        match std::env::var("ISLAND_VIEW_SEASON")
            .ok()?
            .to_ascii_lowercase()
            .as_str()
        {
            "spring" => Some(Season::Spring),
            "summer" => Some(Season::Summer),
            "autumn" => Some(Season::Autumn),
            "winter" => Some(Season::Winter),
            _ => None,
        }
    })
}

/// When each kind shows: butterflies on summer days, fireflies on summer
/// nights from dusk, leaves through autumn; all only with the camera near.
pub(crate) fn mote_shows(kind: MoteKind, season: Season, daylight: f32) -> bool {
    match kind {
        MoteKind::Butterfly => season == Season::Summer && daylight > 0.45,
        MoteKind::Firefly => season == Season::Summer && daylight < 0.3,
        MoteKind::Leaf => season == Season::Autumn,
    }
}

/// Where a mote is at view time `time` round `focus`: butterflies wander
/// low over the grass, fireflies drift and blink, leaves fall downwind and
/// start again. Returns the place and a scale (0 not shown).
pub(crate) fn mote_at(kind: MoteKind, focus: Vec3, wind: &Wind, seed: f32) -> (Vec3, f32) {
    let time = wind.time;
    let home = Vec3::new(
        ((seed * 12.9898).sin() * 43_758.547).fract() * 28.0 - 14.0,
        0.0,
        ((seed * 78.233).sin() * 12_345.679).fract() * 28.0 - 14.0,
    );
    match kind {
        MoteKind::Butterfly => {
            let wander = Vec3::new(
                (time * 0.37 + seed * 9.0).sin() * 2.5 + (time * 0.91 + seed).sin(),
                0.6 + 0.4 * (time * 1.3 + seed * 4.0).sin().abs(),
                (time * 0.29 + seed * 5.0).cos() * 2.5,
            );
            (focus + home + wander, 1.0)
        }
        MoteKind::Firefly => {
            let drift = Vec3::new(
                (time * 0.21 + seed * 3.0).sin() * 1.5,
                0.5 + 0.8 * (time * 0.17 + seed * 2.0).sin().abs(),
                (time * 0.19 + seed * 7.0).cos() * 1.5,
            );
            let blink = ((time * 0.9 + seed * 13.0).sin() * 0.5 + 0.5).powi(4);
            (focus + home + drift, blink)
        }
        MoteKind::Leaf => {
            // A fall of 7 s from 6 m, carried downwind and fluttering.
            let life = 7.0;
            let t = (time / life + seed * 3.1).fract();
            let carry = Vec3::new(wind.direction.x, 0.0, wind.direction.y)
                * (t * life * 0.8 * wind.strength.max(0.4));
            let flutter = Vec3::new((time * 2.3 + seed * 6.0).sin() * 0.4, 0.0, 0.0);
            let at = focus + home + carry + flutter + Vec3::Y * (6.0 * (1.0 - t) + 0.05);
            (at, if t > 0.97 { 0.0 } else { 1.0 })
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn motes(
    mut commands: Commands,
    mut state: ResMut<Motes>,
    wind: Res<Wind>,
    (clock, config, pinned): (
        Option<Res<SimClock>>,
        Option<Res<SimConfig>>,
        Option<Res<ViewDayOverride>>,
    ),
    cameras: Query<&RtsCamera>,
    mut motes: Query<(&Mote, &mut Transform, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !state.spawned {
        state.spawned = true;
        let quad = meshes.add(super::boxes_mesh(&[(
            Vec3::ZERO,
            Vec3::new(0.24, 0.01, 0.17),
        )]));
        let spark = meshes.add(Sphere::new(0.09).mesh().uv(6, 4));
        let leaf = meshes.add(super::boxes_mesh(&[(
            Vec3::ZERO,
            Vec3::new(0.12, 0.01, 0.08),
        )]));
        let mut colour = |r: f32, g: f32, b: f32, glow: f32| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(r, g, b),
                emissive: LinearRgba::rgb(r * glow, g * glow, b * glow),
                unlit: glow > 0.0,
                cull_mode: None,
                double_sided: true,
                ..default()
            })
        };
        let wings = [
            colour(0.98, 0.84, 0.25, 0.0),
            colour(0.95, 0.95, 0.92, 0.0),
            colour(0.85, 0.45, 0.2, 0.0),
        ];
        let glow = colour(0.85, 1.0, 0.45, 6.0);
        let browns = [
            colour(0.78, 0.42, 0.12, 0.0),
            colour(0.86, 0.62, 0.18, 0.0),
            colour(0.62, 0.26, 0.1, 0.0),
        ];
        let mut spawn = |kind: MoteKind,
                         seed: f32,
                         mesh: &Handle<Mesh>,
                         material: &Handle<StandardMaterial>| {
            commands.spawn((
                Name::new("small life"),
                Mote { kind, seed },
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::default(),
                Visibility::Hidden,
                bevy::light::NotShadowCaster,
                bevy::light::NotShadowReceiver,
            ));
        };
        for i in 0..BUTTERFLIES {
            spawn(
                MoteKind::Butterfly,
                i as f32 * 0.311 + 0.07,
                &quad,
                &wings[i % 3],
            );
        }
        for i in 0..FIREFLIES {
            spawn(MoteKind::Firefly, i as f32 * 0.173 + 0.03, &spark, &glow);
        }
        for i in 0..LEAVES {
            spawn(
                MoteKind::Leaf,
                i as f32 * 0.229 + 0.05,
                &leaf,
                &browns[i % 3],
            );
        }
        return;
    }
    let (Some(clock), Some(config)) = (clock, config) else {
        return;
    };
    let season =
        view_season().unwrap_or(testbed_content::village::calendar_at(clock.tick, &config, true).1);
    let daylight = view_daylight(clock.tick, &config, pinned.as_deref());
    let camera = cameras.iter().next().copied();
    // Close cameras only: at island zoom a mote is too small to see.
    let focus = camera
        .filter(|camera| camera.distance < 45.0)
        .map(|camera| camera.focus);
    for (mote, mut transform, mut visibility) in &mut motes {
        let shown = focus.filter(|_| mote_shows(mote.kind, season, daylight));
        let Some(focus) = shown else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let (at, scale) = mote_at(mote.kind, focus, &wind, mote.seed);
        visibility.set_if_neq(Visibility::Inherited);
        transform.translation = at;
        transform.rotation = match mote.kind {
            // Wings fold up and down; leaves tumble.
            MoteKind::Butterfly => {
                Quat::from_rotation_y(wind.time * 0.7 + mote.seed * 9.0)
                    * Quat::from_rotation_z((wind.time * 14.0 + mote.seed * 5.0).sin() * 0.9)
            }
            MoteKind::Leaf => Quat::from_euler(
                EulerRot::XYZ,
                wind.time * 2.1 + mote.seed,
                wind.time * 1.3,
                wind.time * 1.7 + mote.seed * 2.0,
            ),
            MoteKind::Firefly => Quat::IDENTITY,
        };
        transform.scale = Vec3::splat(scale.max(0.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_clock_runs_on_real_time_and_stops_at_zero_motion() {
        assert_eq!(advance(3.0, 0.5, 1.0), 3.5);
        assert_eq!(advance(3.0, 0.5, 0.5), 3.25);
        // Reduce motion to nothing: the clock stands, so does everything
        // that reads it.
        assert_eq!(advance(3.0, 0.5, 0.0), 3.0);
        let wind = Wind {
            time: 3.0,
            ..default()
        };
        let still = Wind {
            time: advance(3.0, 0.5, 0.0),
            ..wind
        };
        let anchor = Vec3::new(10.0, 90.0, -20.0);
        assert_eq!(
            cloud_at(anchor, &wind, 300.0),
            cloud_at(anchor, &still, 300.0)
        );
        assert_eq!(
            gull_at(Vec3::ZERO, 0.3, wind.time),
            gull_at(Vec3::ZERO, 0.3, still.time)
        );
        // So does everything else the view clock drives: flags, crops,
        // small life, wings.
        assert_eq!(flag_turn(&wind, 0.4), flag_turn(&still, 0.4));
        assert_eq!(
            ripple_lean(&wind, Vec2::ONE),
            ripple_lean(&still, Vec2::ONE)
        );
        assert_eq!(
            mote_at(MoteKind::Leaf, Vec3::ZERO, &wind, 0.2),
            mote_at(MoteKind::Leaf, Vec3::ZERO, &still, 0.2)
        );
        assert_eq!(
            wing_beat(wind.time, 4.2, 0.1),
            wing_beat(still.time, 4.2, 0.1)
        );
    }

    #[test]
    fn clouds_drift_downwind_wrap_round_and_the_high_ones_go_faster() {
        let (span, anchor) = (300.0, Vec3::new(0.0, 90.0, 0.0));
        let at = |time: f32| {
            cloud_at(
                anchor,
                &Wind {
                    direction: Vec2::X,
                    strength: 1.0,
                    time,
                },
                span,
            )
        };
        // Along the wind (+X), nothing across it.
        let (a, b) = (at(10.0), at(20.0));
        assert!(b.x > a.x && (b.z - a.z).abs() < 1e-4 && b.y == a.y);
        // Never outside the span: wrapped round, not gone.
        for step in 0..400 {
            let p = at(step as f32 * 7.0);
            assert!(p.x.abs() <= span * 0.5 + 1e-3, "{p}");
        }
        let high = cloud_at(
            Vec3::new(0.0, 110.0, 0.0),
            &Wind {
                direction: Vec2::X,
                strength: 1.0,
                time: 10.0,
            },
            span,
        );
        assert!(high.x > a.x, "higher clouds move faster");
    }

    #[test]
    fn one_wind_calm_on_clear_days_strong_in_storms_turning_over_the_day() {
        let (clear_dir, clear) = sample(Weather::Clear, 1.0, 0.3, 50.0);
        let (_, storm) = sample(Weather::Storm, 1.0, 0.3, 50.0);
        assert!(storm > clear * 2.5, "{clear} {storm}");
        assert!((clear_dir.length() - 1.0).abs() < 1e-4);
        // The direction swings over the day, not in seconds.
        let (morning, _) = sample(Weather::Clear, 1.0, 0.1, 50.0);
        let (evening, _) = sample(Weather::Clear, 1.0, 0.6, 50.0);
        assert!(morning.angle_to(evening).abs() > 0.5);
        let (later, _) = sample(Weather::Clear, 1.0, 0.1, 52.0);
        assert!(morning.angle_to(later).abs() < 0.01);
        // Gusts: the strength moves over seconds.
        let (_, a) = sample(Weather::Rain, 1.0, 0.3, 10.0);
        let (_, b) = sample(Weather::Rain, 1.0, 0.3, 11.5);
        assert!((a - b).abs() > 0.01);
        // The wind's yaw points +X along it.
        let wind = Wind {
            direction: Vec2::new(0.0, 1.0),
            ..default()
        };
        let along = Quat::from_rotation_y(wind.yaw()) * Vec3::X;
        assert!((Vec2::new(along.x, along.z) - wind.direction).length() < 1e-5);
    }

    #[test]
    fn lights_flicker_gently_and_birds_keep_moving_while_the_sim_is_paused() {
        for step in 0..100 {
            let f = flicker(step as f32 * 0.1, 2.0);
            assert!((0.85..=1.15).contains(&f));
        }
        // The birds follow the view clock, not the sim's: two view times,
        // two places, whatever the sim does.
        let (a, _) = gull_at(Vec3::ZERO, 0.3, 1.0);
        let (b, _) = gull_at(Vec3::ZERO, 0.3, 3.0);
        assert!(a.distance(b) > 0.5);
        assert!(songbird_at(Vec3::ZERO, 0.0, 0.0).is_some());
        assert!(cloud_cover(Weather::Storm, 1.0) > cloud_cover(Weather::Clear, 1.0));
    }
    #[test]
    fn cloud_shadows_are_soft_patches_that_go_under_an_overcast() {
        let samples = 64;
        let mut covered = 0;
        let mut soft = 0;
        for row in 0..samples {
            for column in 0..samples {
                let a = shadow_alpha(column as f32 / samples as f32, row as f32 / samples as f32);
                if a > 0.5 {
                    covered += 1;
                }
                if a > 0.05 && a < 0.95 {
                    soft += 1;
                }
            }
        }
        let share = covered as f32 / (samples * samples) as f32;
        assert!((0.2..0.6).contains(&share), "{share}");
        assert!(soft > samples * 4, "soft edges: {soft}");
        // Tileable: the edges meet.
        assert!((shadow_alpha(0.0, 0.3) - shadow_alpha(1.0, 0.3)).abs() < 1e-3);
        assert_eq!(shadow_strength(Weather::Clear, 1.0), 1.0);
        assert_eq!(shadow_strength(Weather::Storm, 1.0), 0.0);
        assert!(shadow_strength(Weather::Rain, 0.5) > 0.4);
    }

    #[test]
    fn flags_stream_downwind_crops_ripple_and_small_life_keeps_its_season() {
        let wind = Wind {
            direction: Vec2::new(0.0, 1.0),
            strength: 1.0,
            time: 0.0,
        };
        let cloth = flag_turn(&wind, 0.0) * Vec3::Z;
        assert!(cloth.z > 0.9, "{cloth}");
        let later = Wind { time: 1.0, ..wind };
        assert_ne!(
            ripple_lean(&wind, Vec2::ZERO),
            ripple_lean(&later, Vec2::ZERO)
        );
        assert!(mote_shows(MoteKind::Butterfly, Season::Summer, 0.9));
        assert!(!mote_shows(MoteKind::Butterfly, Season::Winter, 0.9));
        assert!(mote_shows(MoteKind::Firefly, Season::Summer, 0.1));
        assert!(!mote_shows(MoteKind::Firefly, Season::Summer, 0.9));
        assert!(mote_shows(MoteKind::Leaf, Season::Autumn, 0.5));
        // A leaf falls and is carried downwind.
        let (a, _) = mote_at(MoteKind::Leaf, Vec3::ZERO, &Wind { time: 0.5, ..wind }, 0.3);
        let (b, _) = mote_at(MoteKind::Leaf, Vec3::ZERO, &Wind { time: 1.5, ..wind }, 0.3);
        assert!(b.y < a.y && b.z > a.z, "{a} {b}");
        // A small bird's wings beat; a gull mostly glides and now and then
        // flaps.
        assert_ne!(wing_beat(0.0, 22.0, 0.1), wing_beat(0.05, 22.0, 0.1));
        let samples: Vec<f32> = (0..900).map(|i| gull_wings(i as f32 * 0.01, 0.3)).collect();
        let gliding = samples
            .iter()
            .filter(|beat| (0.08..0.16).contains(*beat))
            .count();
        assert!(gliding > 600 && gliding < 900, "{gliding}");
        // A bird has a body and four wing panels, the wings swept back.
        let [body, inner, outer, ..] = bird_meshes(0.5);
        assert!(body.count_vertices() >= 27 && inner.count_vertices() == 6);
        let reach = |mesh: &Mesh| match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) => p
                .iter()
                .fold(Vec3::ZERO, |m, v| m.max(Vec3::from_array(*v))),
            _ => Vec3::ZERO,
        };
        assert!(reach(&outer).x > 0.6, "a gull spans more than a metre");
    }
}
