//! Today's weather, from `view::weather`: rain, storm and snow as falling
//! sheets of streaks or flakes round the camera, and a dimmer, hazier sky
//! for them and for a shock (drought, blight, a harsh winter).
//!
//! A sheet is one mesh of a few hundred crossed quads in a box of air. Two
//! copies stacked on top of each other slide down and wrap, so the fall is
//! endless and costs two entities, not one per drop. Nothing is spawned and
//! nothing runs while the weather is clear.
//!
//! Streaks and flakes are sized in world metres and never scaled with the
//! zoom. Full streaks fall only [`FAR_CLEAR_M`] or more from the camera's
//! upright; nearer to it a second, sparser sheet of short streaks
//! ([`NEAR_LENGTH_M`]) falls, so no streak is drawn longer than about 6 % of
//! the picture's height at any camera height. From a high camera (above [`FADE_FROM_M`]) they fade out over
//! [`FADE_OVER_M`] and a soft drifting haze, like cloud shadow, fades in
//! instead; the light, the sky and the ground tint carry the weather there.
//!
//! Capture overrides, view only: `ISLAND_VIEW_FLASH=hold` holds a lightning
//! flash at its peak (a number from 0 to 1 at that strength) while a storm is
//! drawn; `ISLAND_VIEW_SHOCK=drought|blight|harsh_winter` draws that shock's
//! tints whatever the simulation's shock is.
use std::f32::consts::TAU;
use std::sync::OnceLock;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use island_content::village::frontier::life_spec::{Shock, Weather};
use island_content::village::view;
use npc_framework::prelude::{SimClock, SimConfig};

use crate::camera::RtsCamera;
use crate::island_atmosphere::daylight::{ViewDayOverride, view_daylight};
use crate::island_atmosphere::sky::sun_strength;

/// Seconds between two reads of the weather, and to ease from one to the next.
const READ_SECS: f32 = 0.5;
const EASE_SECS: f32 = 2.5;
/// The box of air one sheet fills, metres. It is centred under the camera,
/// so it reaches well past the focus; it never grows with the zoom.
const SHEET: Vec3 = Vec3::new(150.0, 26.0, 150.0);
const STREAKS: usize = 14_000;
/// No streak falls within this many metres of the upright through the
/// camera: none is drawn against the lens.
pub(crate) const CLEAR_ROUND_EYE_M: f32 = 4.5;
/// Full streaks keep this far from the camera's upright: a storm's 0.75 m
/// streak 15 m away is 6 % of the picture's height (45 degree view).
pub(crate) const FAR_CLEAR_M: f32 = 15.0;
/// Between [`CLEAR_ROUND_EYE_M`] and [`FAR_CLEAR_M`] streaks are this long
/// at most, and this many fill the ring.
pub(crate) const NEAR_LENGTH_M: f32 = 0.2;
const NEAR_STREAKS: usize = 900;
/// The camera's vertical field of view (Bevy's default).
#[cfg(test)]
const VIEW_FOV: f32 = std::f32::consts::FRAC_PI_4;
/// Camera height over its focus from which the streaks fade out, and the
/// metres they take to go; the haze fades in over the same span.
pub(crate) const FADE_FROM_M: f32 = 60.0;
pub(crate) const FADE_OVER_M: f32 = 30.0;
/// The haze: metres of ground one tile of its texture covers, how high over
/// the focus it drifts, how fast, and its texels.
const HAZE_TILE_M: f32 = 240.0;
const HAZE_ABOVE_M: f32 = 25.0;
const HAZE_DRIFT_M_PER_S: Vec2 = Vec2::new(3.2, 1.4);
const HAZE_TEXELS: usize = 256;
/// A second haze layer, turned and on a tile of another size, so the two
/// never line up into a repeat over open sea; each carries this share of
/// the haze.
const HAZE_SECOND_TILE_M: f32 = 370.0;
const HAZE_SECOND_TURN: f32 = 0.71;
const HAZE_LAYER_SHARE: f32 = 0.62;
/// Real seconds for the ground to get wet and to dry; for snow to settle
/// and to melt.
const WET_SECS: f32 = 15.0;
const DRY_SECS: f32 = 60.0;
const SETTLE_SECS: f32 = 40.0;
const MELT_SECS: f32 = 90.0;

/// What the view shows of the weather. `amount` eases between 0 and 1 as
/// the weather comes and goes.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub(crate) struct WeatherLook {
    pub(crate) kind: Weather,
    pub(crate) shock: Option<Shock>,
    pub(crate) amount: f32,
    /// 0 to 1: a flash of lightning, in a storm.
    pub(crate) flash: f32,
    /// 0 to 1: how wet the ground is; how deep the snow lies on it.
    pub(crate) wet: f32,
    pub(crate) snow: f32,
    since: f32,
    sheets: Option<Sheets>,
}

impl Default for WeatherLook {
    fn default() -> Self {
        Self {
            kind: Weather::Clear,
            shock: None,
            amount: 0.0,
            flash: 0.0,
            wet: 0.0,
            snow: 0.0,
            since: READ_SECS,
            sheets: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Sheets {
    kind: Weather,
    entities: [Entity; 2],
    material: Entity,
    /// The short streaks near the camera, two copies like the full ones.
    near: [Entity; 2],
    /// The two haze layers shown from high cameras; their materials are on
    /// them.
    haze: [Entity; 2],
}

/// The haze's colour and greatest opacity under a weather.
pub(crate) fn haze_tone(kind: Weather) -> Option<([f32; 3], f32)> {
    match kind {
        Weather::Clear => None,
        Weather::Rain => Some(([0.42, 0.45, 0.5], 0.34)),
        Weather::Storm => Some(([0.2, 0.22, 0.27], 0.46)),
        Weather::Snow => Some(([0.93, 0.95, 0.98], 0.4)),
    }
}

/// How much of the streaks shows from a camera `height` metres over its
/// focus: all of it up to [`FADE_FROM_M`], none [`FADE_OVER_M`] higher.
/// The haze shows the rest.
pub(crate) fn streak_share(height: f32) -> f32 {
    let t = ((height - FADE_FROM_M) / FADE_OVER_M).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// The haze texture's opacity at (u, v) in 0..1: soft patches made of
/// whole waves across the tile, so it repeats without a seam.
pub(crate) fn haze_alpha(u: f32, v: f32) -> f32 {
    let wave = |a: f32, b: f32, phase: f32| (TAU * (a * u + b * v) + phase).sin();
    let sum = wave(2.0, 3.0, 0.3)
        + wave(-3.0, 2.0, 1.7)
        + 0.7 * wave(5.0, 1.0, 4.1)
        + 0.7 * wave(1.0, -5.0, 2.2)
        + 0.45 * wave(7.0, 4.0, 0.9)
        + 0.45 * wave(-4.0, 7.0, 5.3)
        + 0.3 * wave(9.0, -6.0, 3.1);
    // Patches where the waves meet, clear between; a slow swell across
    // the tile thins some and thickens others, so they do not repeat.
    let t = ((sum + 0.6) / 3.0).clamp(0.0, 1.0);
    let swell =
        (0.5 + 0.5 * (0.6 * wave(1.0, 1.0, 0.8) + 0.4 * wave(-1.0, 2.0, 2.9))).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t) * (0.35 + 0.65 * swell)
}

pub(crate) fn haze_image() -> Image {
    let mut data = Vec::with_capacity(HAZE_TEXELS * HAZE_TEXELS * 4);
    for row in 0..HAZE_TEXELS {
        for column in 0..HAZE_TEXELS {
            let at = |texel: usize| texel as f32 / HAZE_TEXELS as f32;
            let alpha = (haze_alpha(at(column), at(row)) * 255.0).round() as u8;
            data.extend([255, 255, 255, alpha]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: HAZE_TEXELS as u32,
            height: HAZE_TEXELS as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

/// `ISLAND_VIEW_FLASH`: a lightning flash held at this strength.
fn forced_flash() -> Option<f32> {
    static FLASH: OnceLock<Option<f32>> = OnceLock::new();
    *FLASH.get_or_init(|| {
        let value = std::env::var("ISLAND_VIEW_FLASH").ok()?;
        match value.trim() {
            "hold" => Some(1.0),
            number => number.parse::<f32>().ok().map(|v| v.clamp(0.0, 1.0)),
        }
    })
}

/// `ISLAND_VIEW_SHOCK`: the shock drawn whatever the simulation's is.
pub(crate) fn parse_shock(value: &str) -> Option<Option<Shock>> {
    match value.trim() {
        "drought" => Some(Some(Shock::Drought)),
        "blight" => Some(Some(Shock::Blight)),
        "harsh_winter" | "harsh-winter" => Some(Some(Shock::HarshWinter)),
        "none" => Some(None),
        _ => None,
    }
}

fn forced_shock() -> Option<Option<Shock>> {
    static SHOCK: OnceLock<Option<Option<Shock>>> = OnceLock::new();
    *SHOCK.get_or_init(|| {
        std::env::var("ISLAND_VIEW_SHOCK")
            .ok()
            .and_then(|value| parse_shock(&value))
    })
}

/// What the weather does to the sky: the sun and the sky light are
/// multiplied, the haze comes nearer and everything is mixed towards a tint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SkyChange {
    pub(crate) sun: f32,
    pub(crate) ambient: f32,
    /// The distance haze is fully thick at, against a clear day's.
    pub(crate) visibility: f32,
    pub(crate) tint: [f32; 3],
    pub(crate) tint_amount: f32,
}

impl SkyChange {
    pub(crate) const CLEAR: SkyChange = SkyChange {
        sun: 1.0,
        ambient: 1.0,
        visibility: 1.0,
        tint: [1.0, 1.0, 1.0],
        tint_amount: 0.0,
    };
    fn towards(self, amount: f32) -> SkyChange {
        let mix = |value: f32| 1.0 + (value - 1.0) * amount;
        SkyChange {
            sun: mix(self.sun),
            ambient: mix(self.ambient),
            visibility: mix(self.visibility),
            tint: self.tint,
            tint_amount: self.tint_amount * amount,
        }
    }
    fn and(self, other: SkyChange) -> SkyChange {
        let weight = self.tint_amount + other.tint_amount;
        SkyChange {
            sun: self.sun * other.sun,
            ambient: self.ambient * other.ambient,
            visibility: self.visibility * other.visibility,
            tint: if weight > 0.0 {
                std::array::from_fn(|channel| {
                    (self.tint[channel] * self.tint_amount
                        + other.tint[channel] * other.tint_amount)
                        / weight
                })
            } else {
                [1.0; 3]
            },
            tint_amount: weight.min(0.6),
        }
    }
}

/// The sky under a weather and a shock, `amount` of the way in. Rain, a
/// storm and snow are overcast: the sun is mostly taken, so shadows all but
/// go, and the sky light carries the day. Rain greys, a storm darkens, snow
/// pales the haze and brings it close; a drought is dusty and warm, a harsh
/// winter cold, a blight sallow.
pub(crate) fn sky_change(kind: Weather, shock: Option<Shock>, amount: f32) -> SkyChange {
    let weather = match kind {
        Weather::Clear => SkyChange::CLEAR,
        Weather::Rain => SkyChange {
            sun: 0.14,
            ambient: 1.3,
            visibility: 0.42,
            tint: [0.56, 0.61, 0.66],
            tint_amount: 0.5,
        },
        Weather::Storm => SkyChange {
            sun: 0.06,
            ambient: 0.95,
            visibility: 0.28,
            tint: [0.34, 0.37, 0.44],
            tint_amount: 0.58,
        },
        Weather::Snow => SkyChange {
            sun: 0.25,
            ambient: 1.4,
            visibility: 0.32,
            tint: [0.86, 0.89, 0.93],
            tint_amount: 0.5,
        },
    }
    .towards(amount.clamp(0.0, 1.0));
    let shock = match shock {
        None => SkyChange::CLEAR,
        Some(Shock::Drought) => SkyChange {
            sun: 1.04,
            ambient: 1.0,
            visibility: 0.7,
            tint: [0.93, 0.80, 0.56],
            tint_amount: 0.22,
        },
        Some(Shock::HarshWinter) => SkyChange {
            sun: 0.9,
            ambient: 0.96,
            visibility: 0.8,
            tint: [0.78, 0.86, 1.0],
            tint_amount: 0.2,
        },
        Some(Shock::Blight) => SkyChange {
            sun: 0.95,
            ambient: 0.97,
            visibility: 0.9,
            tint: [0.72, 0.74, 0.52],
            tint_amount: 0.12,
        },
    };
    weather.and(shock)
}

/// How a sheet falls: metres a second, how far it leans, the size of one
/// streak or flake (width, length), its colour and how opaque it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Fall {
    pub(crate) speed: f32,
    pub(crate) lean: f32,
    pub(crate) size: Vec2,
    pub(crate) colour: [f32; 3],
    pub(crate) alpha: f32,
    /// Metres a flake wanders sideways.
    pub(crate) sway: f32,
}

pub(crate) fn fall(kind: Weather) -> Option<Fall> {
    match kind {
        Weather::Clear => None,
        Weather::Rain => Some(Fall {
            speed: 17.0,
            lean: 0.07,
            size: Vec2::new(0.022, 0.55),
            colour: [0.8, 0.86, 0.95],
            alpha: 0.62,
            sway: 0.0,
        }),
        Weather::Storm => Some(Fall {
            speed: 25.0,
            lean: 0.32,
            size: Vec2::new(0.026, 0.75),
            colour: [0.74, 0.8, 0.92],
            alpha: 0.6,
            sway: 0.0,
        }),
        Weather::Snow => Some(Fall {
            speed: 1.9,
            lean: 0.0,
            size: Vec2::new(0.1, 0.1),
            colour: [0.96, 0.97, 1.0],
            alpha: 0.9,
            sway: 1.1,
        }),
    }
}

fn scatter(index: usize, salt: u32) -> f32 {
    let mut n = (index as u32).wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA6B);
    n ^= n >> 15;
    n = n.wrapping_mul(0x2C1B_3C6D);
    n ^= n >> 12;
    (n & 0xFFFF) as f32 / 65_535.0
}

/// One sheet: `STREAKS` pairs of crossed upright quads scattered through
/// the box, each `size` wide and long, none within [`FAR_CLEAR_M`] of its
/// upright middle, where the camera is. Its bottom is at y 0.
pub(crate) fn sheet_mesh(size: Vec2) -> Mesh {
    let points = (0..STREAKS).map(|streak| {
        let mut at = Vec3::new(
            (scatter(streak, 1) - 0.5) * SHEET.x,
            scatter(streak, 2) * (SHEET.y - size.y),
            (scatter(streak, 3) - 0.5) * SHEET.z,
        );
        // Pushed out of the clear round the camera, not dropped: the
        // density elsewhere stays even.
        let flat = Vec2::new(at.x, at.z);
        if flat.length() < FAR_CLEAR_M {
            let out = flat.normalize_or(Vec2::X) * (FAR_CLEAR_M + flat.length());
            at.x = out.x;
            at.z = out.y;
        }
        at
    });
    quads(points, size)
}

/// The near sheet: `NEAR_STREAKS` short streaks in the ring between
/// [`CLEAR_ROUND_EYE_M`] and [`FAR_CLEAR_M`] round the upright, spread evenly
/// over its area and the box's height.
pub(crate) fn near_mesh(size: Vec2) -> Mesh {
    let size = Vec2::new(size.x, size.y.min(NEAR_LENGTH_M));
    let points = (0..NEAR_STREAKS).map(|streak| {
        let (inner, outer) = (
            CLEAR_ROUND_EYE_M * CLEAR_ROUND_EYE_M,
            FAR_CLEAR_M * FAR_CLEAR_M,
        );
        let radius = (inner + (outer - inner) * scatter(streak, 5)).sqrt() - size.x;
        let angle = scatter(streak, 6) * TAU;
        Vec3::new(
            angle.cos() * radius,
            scatter(streak, 7) * (SHEET.y - size.y),
            angle.sin() * radius,
        )
    });
    quads(points, size)
}

/// Crossed upright quads `size` wide and long standing on `points`.
fn quads(points: impl Iterator<Item = Vec3>, size: Vec2) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for at in points {
        for across in [Vec3::X, Vec3::Z] {
            let base = positions.len() as u32;
            let half = across * size.x * 0.5;
            for corner in [
                at - half,
                at + half,
                at + half + Vec3::Y * size.y,
                at - half + Vec3::Y * size.y,
            ] {
                positions.push(corner.to_array());
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_indices(Indices::U32(indices))
}

/// The longest a streak `length` metres long, `distance` metres from the
/// camera, is drawn: a share of the picture's height.
#[cfg(test)]
pub(crate) fn screen_share(length: f32, distance: f32) -> f32 {
    length / (2.0 * distance.max(1e-3) * (VIEW_FOV * 0.5).tan())
}

/// Where the two copies of a sheet stand `seconds` in: how far each has
/// slid down its own height, so that one always covers the box.
pub(crate) fn slide(seconds: f32, speed: f32, height: f32) -> [f32; 2] {
    let down = (seconds * speed).rem_euclid(height);
    [-down, height - down]
}

/// How wet the ground is heading for, and how deep the snow, under a
/// weather and a shock.
pub(crate) fn ground_targets(kind: Weather, shock: Option<Shock>) -> (f32, f32) {
    let wet = match kind {
        Weather::Rain | Weather::Storm => 1.0,
        _ => 0.0,
    };
    let snow = match (kind, shock) {
        (_, Some(Shock::HarshWinter)) => 1.0,
        (Weather::Snow, _) => 0.8,
        _ => 0.0,
    };
    (wet, snow)
}

/// How rain-wet buildings look at `wet` (0 dry .. 1 soaked): their colour
/// darkened and their surface glossier (roughness scaled), as the ground's.
pub(crate) fn wet_surface(wet: f32) -> (f32, f32) {
    let wet = wet.clamp(0.0, 1.0);
    (1.0 - 0.26 * wet, 1.0 - 0.5 * wet)
}

/// Wetness is applied in steps of this much, so the building materials
/// change a score of times over a shower, not every frame.
const WET_STEP: f32 = 0.05;

/// The buildings' materials as they were dry, by material, and the wetness
/// last drawn.
#[derive(Resource, Default)]
pub(crate) struct WetBuildings {
    level: f32,
    dry: std::collections::BTreeMap<AssetId<StandardMaterial>, (LinearRgba, f32)>,
}

/// Rain darkens and glosses the buildings' models (walls, roofs, timber)
/// with the ground. Runs its walk over the buildings only when the
/// wetness crosses a step, and a building raised in the rain takes the
/// look at the next step.
pub(crate) fn wet_buildings(
    look: Res<WeatherLook>,
    mut state: ResMut<WetBuildings>,
    buildings: Query<Entity, With<crate::colony_scene::living_scene::BuildingModel>>,
    children: Query<&Children>,
    meshes: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let level = (look.wet / WET_STEP).round() * WET_STEP;
    if (level - state.level).abs() < WET_STEP * 0.5 {
        return;
    }
    state.level = level;
    let (shade, gloss) = wet_surface(level);
    let mut seen = std::collections::BTreeSet::new();
    for building in &buildings {
        for part in children.iter_descendants(building) {
            let Ok(material) = meshes.get(part) else {
                continue;
            };
            if !seen.insert(material.0.id()) {
                continue;
            }
            let Some(mut drawn) = materials.get_mut(&material.0) else {
                continue;
            };
            let (base, rough) = *state
                .dry
                .entry(material.0.id())
                .or_insert((drawn.base_color.to_linear(), drawn.perceptual_roughness));
            let wet = LinearRgba::new(
                base.red * shade,
                base.green * shade,
                base.blue * shade,
                base.alpha,
            );
            drawn.base_color = Color::LinearRgba(wet);
            drawn.perceptual_roughness = (rough * gloss).max(0.08);
        }
    }
}

/// One step of `value` towards `target`: `up` seconds for the whole way up,
/// `down` for the whole way down.
pub(crate) fn ease_towards(value: f32, target: f32, delta: f32, up: f32, down: f32) -> f32 {
    if value < target {
        (value + delta / up).min(target)
    } else {
        (value - delta / down).max(target)
    }
}

/// What the weather does to the bare ground: a factor on its colour, a
/// glow of lying snow (a share of white the sky light lifts), and its
/// roughness. Wet ground is darker, cooler and has a sheen; snow pales it;
/// a drought parches it, a blight sours it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GroundLook {
    pub(crate) factor: [f32; 3],
    pub(crate) snow_glow: f32,
    pub(crate) roughness: f32,
}

pub(crate) const DRY_ROUGHNESS: f32 = 0.96;

pub(crate) fn ground_look(wet: f32, snow: f32, shock: Option<Shock>) -> GroundLook {
    let mix = |a: [f32; 3], b: [f32; 3], t: f32| -> [f32; 3] {
        std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t.clamp(0.0, 1.0))
    };
    let mut factor = mix([1.0; 3], [0.66, 0.69, 0.74], wet);
    factor = mix(factor, [1.25, 1.3, 1.4], snow);
    let shocked = match shock {
        Some(Shock::Drought) => [1.14, 0.97, 0.66],
        Some(Shock::Blight) => [0.97, 0.93, 0.78],
        _ => [1.0; 3],
    };
    factor = std::array::from_fn(|i| factor[i] * shocked[i]);
    GroundLook {
        factor,
        snow_glow: snow.clamp(0.0, 1.0) * 0.55,
        roughness: DRY_ROUGHNESS - (DRY_ROUGHNESS - 0.5) * wet.clamp(0.0, 1.0) * (1.0 - snow),
    }
}

/// The land's own material, as the landscape made it: its handle and its
/// base colour, which the weather multiplies.
#[derive(Resource, Clone)]
pub(crate) struct LandMaterial {
    pub(crate) handle: Handle<StandardMaterial>,
    pub(crate) base: LinearRgba,
}

/// Put the weather on the ground: wet, snowed on, parched or soured. Only
/// writes when what it would draw changes, so a clear dry day costs a
/// comparison.
pub(crate) fn ground(
    look: Res<WeatherLook>,
    land: Option<Res<LandMaterial>>,
    clock: Option<Res<SimClock>>,
    config: Option<Res<SimConfig>>,
    pinned: Option<Res<ViewDayOverride>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut last: Local<Option<(GroundLook, i32)>>,
) {
    let Some(land) = land else { return };
    let ground = ground_look(look.wet, look.snow, look.shock);
    // Lying snow glows with the sky light: dim at night. In twentieths, so
    // the dusk does not rewrite the material every frame.
    let steps = if ground.snow_glow > 0.0 {
        let hour = clock.zip(config).map_or(1.0, |(clock, config)| {
            sun_strength(view_daylight(clock.tick, &config, pinned.as_deref()))
        });
        (hour * 20.0).round() as i32
    } else {
        0
    };
    if *last == Some((ground, steps)) {
        return;
    }
    *last = Some((ground, steps));
    let Some(material) = materials.get_mut(&land.handle) else {
        return;
    };
    let mut material = material;
    let [r, g, b] = ground.factor;
    material.base_color = Color::LinearRgba(LinearRgba::new(
        land.base.red * r,
        land.base.green * g,
        land.base.blue * b,
        land.base.alpha,
    ));
    let glow = ground.snow_glow * (0.12 + 0.88 * steps as f32 / 20.0) * 0.45;
    material.emissive = LinearRgba::rgb(glow * 0.92, glow * 0.95, glow);
    material.perceptual_roughness = ground.roughness;
}

/// How hard the island's plants sway in the wind: rain stirs them, a storm
/// bends them.
pub(crate) fn wind_strength(look: &WeatherLook) -> f32 {
    match look.kind {
        Weather::Storm => 1.0 + 3.5 * look.amount,
        Weather::Rain => 1.0 + 0.8 * look.amount,
        _ => 1.0,
    }
}

/// Lightning: a short double flash every few seconds of a storm.
pub(crate) fn lightning(seconds: f32) -> f32 {
    let beat = seconds.rem_euclid(7.3);
    let strike = |from: f32| {
        let t = beat - from;
        if (0.0..0.16).contains(&t) {
            1.0 - t / 0.16
        } else {
            0.0
        }
    };
    strike(5.1).max(strike(5.42) * 0.7)
}

/// Read today's weather and shock twice a second, ease towards it, and put
/// up or take down the sheets when the kind changes.
pub(crate) fn read(world: &mut World) {
    let delta = world
        .get_resource::<Time>()
        .map_or(0.0, |time| time.delta_secs());
    let mut look = *world.resource::<WeatherLook>();
    look.since += delta;
    let falling = fall(look.kind).is_some();
    let target = if falling { 1.0 } else { 0.0 };
    if look.amount != target {
        let step = delta / EASE_SECS;
        look.amount = if look.amount < target {
            (look.amount + step).min(target)
        } else {
            (look.amount - step).max(target)
        };
    }
    if look.since >= READ_SECS {
        look.since = 0.0;
        let today = view::weather(world);
        look.kind = today.today.weather;
        look.shock = forced_shock().unwrap_or(today.shock);
    }
    // The ground gets wet under rain and dries after; snow settles and
    // melts. A harsh winter keeps deep snow down.
    let (wet, snow) = ground_targets(look.kind, look.shock);
    look.wet = ease_towards(look.wet, wet, delta, WET_SECS, DRY_SECS);
    look.snow = ease_towards(look.snow, snow, delta, SETTLE_SECS, MELT_SECS);
    // The sheets follow the kind: none while clear.
    if look.sheets.is_some_and(|sheets| sheets.kind != look.kind) {
        if let Some(sheets) = look.sheets.take() {
            for entity in sheets
                .entities
                .into_iter()
                .chain(sheets.near)
                .chain(sheets.haze)
                .chain([sheets.material])
            {
                if world.get_entity(entity).is_ok() {
                    world.entity_mut(entity).despawn();
                }
            }
        }
        look.amount = 0.0;
    }
    if look.sheets.is_none()
        && let Some(fall) = fall(look.kind)
        && world.contains_resource::<Assets<Mesh>>()
        && world.contains_resource::<Assets<StandardMaterial>>()
        && world.contains_resource::<Assets<Image>>()
    {
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(sheet_mesh(fall.size));
        let [r, g, b] = fall.colour;
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: Color::linear_rgba(r, g, b, 0.0),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                cull_mode: None,
                double_sided: true,
                ..default()
            });
        let near_mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(near_mesh(fall.size));
        let mut sheet = |mesh: &Handle<Mesh>, name: &'static str| {
            world
                .spawn((
                    Name::new(name),
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::default(),
                    bevy::light::NotShadowCaster,
                    bevy::light::NotShadowReceiver,
                ))
                .id()
        };
        let entities = [0, 1].map(|_| sheet(&mesh, "weather sheet"));
        let near = [0, 1].map(|_| sheet(&near_mesh, "weather sheet near"));
        // The material is kept by an entity of its own, to find it again.
        let holder = world
            .spawn((Name::new("weather material"), MeshMaterial3d(material)))
            .id();
        // The haze: one wide square, hidden until a high camera fades it in.
        let texture = world.resource_mut::<Assets<Image>>().add(haze_image());
        let haze_mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Plane3d::new(Vec3::Y, Vec2::splat(0.5)).mesh());
        let haze = [0, 1].map(|_| {
            let haze_material =
                world
                    .resource_mut::<Assets<StandardMaterial>>()
                    .add(StandardMaterial {
                        base_color: Color::linear_rgba(1.0, 1.0, 1.0, 0.0),
                        base_color_texture: Some(texture.clone()),
                        alpha_mode: AlphaMode::Blend,
                        unlit: true,
                        cull_mode: None,
                        double_sided: true,
                        fog_enabled: true,
                        ..default()
                    });
            world
                .spawn((
                    Name::new("weather haze"),
                    Mesh3d(haze_mesh.clone()),
                    MeshMaterial3d(haze_material),
                    Transform::default(),
                    Visibility::Hidden,
                    bevy::light::NotShadowCaster,
                    bevy::light::NotShadowReceiver,
                ))
                .id()
        });
        look.sheets = Some(Sheets {
            kind: look.kind,
            entities,
            material: holder,
            near,
            haze,
        });
    }
    *world.resource_mut::<WeatherLook>() = look;
}

/// Slide the sheets down round the camera's focus. Returns at once in
/// clear weather.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fall_sheets(
    mut look: ResMut<WeatherLook>,
    (time, wind): (Res<Time>, Option<Res<super::ambient::Wind>>),
    clock: Option<Res<SimClock>>,
    config: Option<Res<SimConfig>>,
    pinned: Option<Res<ViewDayOverride>>,
    cameras: Query<(&RtsCamera, &GlobalTransform)>,
    mut sheets: Query<(&mut Transform, &mut Visibility)>,
    holders: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Weather runs on the view clock: it keeps falling while paused.
    let seconds = wind
        .as_deref()
        .map_or(time.elapsed_secs(), |wind| wind.time);
    let flash = if look.kind == Weather::Storm {
        forced_flash().unwrap_or_else(|| lightning(seconds)) * look.amount
    } else {
        0.0
    };
    if look.flash != flash {
        look.flash = flash;
    }
    let Some(drawn) = look.sheets else { return };
    let Some(fall) = fall(drawn.kind) else { return };
    let (camera, eye) = cameras
        .iter()
        .next()
        .map_or((RtsCamera::default(), None), |(camera, eye)| {
            (*camera, Some(eye.translation()))
        });
    // Its middle is under the camera, where no streak falls, so nothing is
    // drawn against the lens; it reaches past the focus all round. Streaks
    // keep their size in metres at every zoom.
    let middle = eye.map_or(camera.focus, |eye| Vec3::new(eye.x, camera.focus.y, eye.z));
    let above = eye.map_or(camera.distance, |eye| eye.y - camera.focus.y);
    let share = streak_share(above);
    // The streaks slant with the island's one wind: their tops upwind,
    // more in a gust.
    let lean = wind
        .as_deref()
        .map_or(Quat::from_rotation_z(fall.lean), |wind| {
            let gust = (wind.strength / super::ambient::base_strength(drawn.kind)).clamp(0.6, 1.5);
            let yaw = Quat::from_rotation_y(wind.yaw());
            yaw * Quat::from_rotation_z(fall.lean * gust) * yaw.inverse()
        });
    let sway = Vec3::new(
        (seconds * 0.6).sin() * fall.sway,
        0.0,
        (seconds * 0.43).cos() * fall.sway,
    );
    let streaks_on = if share > 0.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    // The sheets lean about their foot: the foot is moved across so the
    // leaning upright, round which no full streak falls, passes through the
    // camera.
    let foot = middle.y - 2.0;
    let eye_y = eye.map_or(foot + SHEET.y, |eye| eye.y);
    let top = lean * Vec3::Y;
    let base = Vec3::new(
        middle.x - top.x / top.y * (eye_y - foot),
        foot,
        middle.z - top.z / top.y * (eye_y - foot),
    );
    let slid = slide(seconds, fall.speed, SHEET.y);
    for (entity, down) in drawn
        .entities
        .into_iter()
        .zip(slid)
        .chain(drawn.near.into_iter().zip(slid))
    {
        if let Ok((mut transform, mut visibility)) = sheets.get_mut(entity) {
            visibility.set_if_neq(streaks_on);
            if share > 0.0 {
                transform.translation = base + lean * Vec3::new(0.0, down, 0.0) + sway;
                transform.rotation = lean;
                transform.scale = Vec3::ONE;
            }
        }
    }
    // Unlit streaks would glow at night: they follow the sky's brightness.
    let light = clock.zip(config).map_or(1.0, |(clock, config)| {
        0.3 + 0.7 * sun_strength(view_daylight(clock.tick, &config, pinned.as_deref()))
    });
    let [r, g, b] = fall.colour.map(|channel| channel * light);
    let wanted = Color::linear_rgba(r, g, b, fall.alpha * look.amount * share);
    if let Ok(holder) = holders.get(drawn.material)
        && materials
            .get(&holder.0)
            .is_some_and(|material| material.base_color != wanted)
        && let Some(mut material) = materials.get_mut(&holder.0)
    {
        material.base_color = wanted;
    }
    // The haze, over the focus, wide enough to fill a high camera's view;
    // its patches keep their size in metres and drift with the wind. Two
    // layers on tiles of different sizes, one turned, so no repeat shows.
    let haze_share = (1.0 - share) * look.amount;
    let size = (above * 7.0).max(400.0);
    let tone = haze_tone(drawn.kind);
    for (layer, entity) in drawn.haze.into_iter().enumerate() {
        let Ok((mut transform, mut visibility)) = sheets.get_mut(entity) else {
            continue;
        };
        if haze_share <= 0.0 {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        visibility.set_if_neq(Visibility::Inherited);
        // The second layer a little higher, so the two never fight.
        transform.translation = camera.focus + Vec3::Y * (HAZE_ABOVE_M + layer as f32 * 1.5);
        transform.scale = Vec3::new(size, 1.0, size);
        let Some((colour, opacity)) = tone else {
            continue;
        };
        let (tile, turn) = if layer == 0 {
            (HAZE_TILE_M, 0.0)
        } else {
            (HAZE_SECOND_TILE_M, HAZE_SECOND_TURN)
        };
        let uv = haze_uv(size, tile, turn, camera.focus, seconds);
        let [r, g, b] = colour.map(|channel| channel * light);
        let colour = Color::linear_rgba(r, g, b, opacity * HAZE_LAYER_SHARE * haze_share);
        if let Ok(holder) = holders.get(entity)
            && let Some(mut material) = materials.get_mut(&holder.0)
        {
            material.base_color = colour;
            material.uv_transform = uv;
        }
    }
}

/// The haze texture's placement on a square `size` metres wide over
/// `focus`: one tile per `tile` metres, turned by `turn`, anchored to the
/// ground (the pattern stays put as the square follows the camera) and
/// drifting with the wind.
pub(crate) fn haze_uv(size: f32, tile: f32, turn: f32, focus: Vec3, seconds: f32) -> Affine2 {
    let tiles = size / tile;
    let drift = HAZE_DRIFT_M_PER_S * seconds / tile;
    let offset = Vec2::new(focus.x, focus.z) / tile - Vec2::splat(tiles * 0.5);
    Affine2::from_angle(turn)
        * Affine2::from_scale_angle_translation(Vec2::splat(tiles), 0.0, offset + drift)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rain_darkens_and_glosses_the_buildings_and_dry_weather_leaves_them() {
        assert_eq!(wet_surface(0.0), (1.0, 1.0));
        let (shade, gloss) = wet_surface(1.0);
        assert!(shade < 0.8 && shade > 0.6, "{shade}");
        assert!(gloss < 0.6 && gloss > 0.3, "{gloss}");
        let (half, _) = wet_surface(0.5);
        assert!(half < 1.0 && half > shade);
        assert_eq!(wet_surface(3.0), wet_surface(1.0));
    }

    #[test]
    fn clear_weather_draws_nothing_and_changes_nothing() {
        assert_eq!(fall(Weather::Clear), None);
        assert_eq!(sky_change(Weather::Clear, None, 1.0), SkyChange::CLEAR);
        // Weather that has not come in yet leaves the sky as it is.
        assert_eq!(sky_change(Weather::Storm, None, 0.0), SkyChange::CLEAR);
        assert_eq!(WeatherLook::default().kind, Weather::Clear);
        assert!(WeatherLook::default().sheets.is_none());
    }

    #[test]
    fn rain_storm_and_snow_each_read_differently() {
        let (rain, storm, snow) = (
            sky_change(Weather::Rain, None, 1.0),
            sky_change(Weather::Storm, None, 1.0),
            sky_change(Weather::Snow, None, 1.0),
        );
        // Dimmer than clear, a storm darkest; haze nearer in all three.
        assert!(storm.sun < rain.sun && rain.sun < snow.sun && snow.sun < 1.0);
        for sky in [rain, storm, snow] {
            assert!(sky.visibility < 0.5 && sky.tint_amount >= 0.4);
            // Overcast: the sun is mostly taken, so shadows all but go, and
            // the sky light carries the day (a night or a season's colours
            // still read through it).
            assert!(sky.sun > 0.0 && sky.sun <= 0.25, "{}", sky.sun);
            assert!(sky.ambient > 0.9);
        }
        // Under rain the sky light outweighs the sun several times over.
        assert!(rain.ambient / rain.sun > 5.0);
        // Snow pales, a storm darkens.
        assert!(snow.tint[0] > 0.8 && storm.tint[0] < 0.4);
        // Half way in is half the change.
        let half = sky_change(Weather::Rain, None, 0.5);
        assert!((half.sun - (1.0 + rain.sun) * 0.5).abs() < 1e-5);
        // Rain is fast thin streaks, a storm faster and leaning, snow slow
        // square flakes that wander.
        let (rain, storm, snow) = (
            fall(Weather::Rain).expect("rain"),
            fall(Weather::Storm).expect("storm"),
            fall(Weather::Snow).expect("snow"),
        );
        assert!(storm.speed > rain.speed && rain.speed > snow.speed * 5.0);
        assert!(storm.lean > rain.lean * 3.0 && snow.lean == 0.0);
        assert!(rain.size.y > rain.size.x * 10.0 && snow.size.x == snow.size.y);
        // Thin and short: many fine streaks, not a few thick bars.
        assert!(rain.size.x <= 0.025 && rain.size.y <= 0.6 && storm.size.y <= 0.8);
        assert!(snow.sway > 0.5 && rain.sway == 0.0);
        assert!(snow.alpha > rain.alpha);
    }

    #[test]
    fn a_shock_tints_the_sky_with_or_without_weather() {
        let drought = sky_change(Weather::Clear, Some(Shock::Drought), 0.0);
        assert!(drought.tint[0] > drought.tint[2] + 0.3, "dusty and warm");
        assert!(drought.visibility < 1.0 && drought.tint_amount > 0.1);
        let cold = sky_change(Weather::Clear, Some(Shock::HarshWinter), 0.0);
        assert!(cold.tint[2] > cold.tint[0] + 0.2 && cold.sun < 1.0);
        let blight = sky_change(Weather::Clear, Some(Shock::Blight), 0.0);
        assert!(blight.tint_amount > 0.0 && blight.tint_amount < drought.tint_amount);
        // With snow on top the two add up, within bounds.
        let both = sky_change(Weather::Snow, Some(Shock::HarshWinter), 1.0);
        assert!(both.sun < sky_change(Weather::Snow, None, 1.0).sun);
        assert!(both.tint_amount <= 0.6 && both.tint.iter().all(|c| (0.0..=1.0).contains(c)));
    }

    #[test]
    fn two_copies_of_a_sheet_always_cover_the_box() {
        let height = 26.0;
        let mut last = slide(0.0, 19.0, height);
        assert_eq!(last, [0.0, height]);
        for step in 1..2_000 {
            let now = slide(step as f32 / 60.0, 19.0, height);
            // One on top of the other, the lower one's top at the box's top
            // or below, the upper one's bottom inside the box.
            assert!((now[1] - now[0] - height).abs() < 1e-3);
            assert!(now[0] <= 0.0 && now[0] > -height && now[1] > 0.0);
            // Falling, except where it wraps.
            assert!(now[0] < last[0] || now[0] > last[0] + height * 0.5);
            last = now;
        }
    }

    #[test]
    fn a_sheet_is_scattered_through_its_box() {
        let size = Vec2::new(0.045, 1.0);
        let mesh = sheet_mesh(size);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|values| values.as_float3())
            .expect("positions");
        assert_eq!(positions.len(), STREAKS * 8);
        let (mut low, mut high) = (Vec3::MAX, Vec3::MIN);
        for at in positions {
            let at = Vec3::from_array(*at);
            low = low.min(at);
            high = high.max(at);
        }
        assert!(low.y >= 0.0 && high.y <= SHEET.y + 1e-3);
        // Nothing falls within reach of the camera's upright.
        let nearest = positions
            .iter()
            .map(|at| Vec2::new(at[0], at[2]).length())
            .fold(f32::MAX, f32::min);
        assert!(nearest >= FAR_CLEAR_M - size.x, "{nearest}");
        assert!(low.x < -SHEET.x * 0.45 && high.x > SHEET.x * 0.45);
        assert!(low.z < -SHEET.z * 0.45 && high.z > SHEET.z * 0.45);
        // Spread over the height, not banded: every fifth of it has streaks.
        for band in 0..5 {
            let (from, to) = (
                band as f32 * SHEET.y / 5.0,
                (band + 1) as f32 * SHEET.y / 5.0,
            );
            let count = positions
                .iter()
                .filter(|at| at[1] >= from && at[1] < to)
                .count();
            assert!(count > STREAKS, "band {band}: {count}");
        }
    }

    #[test]
    fn no_streak_is_drawn_longer_than_six_hundredths_of_the_picture() {
        for kind in [Weather::Rain, Weather::Storm, Weather::Snow] {
            let fall = fall(kind).expect("falling");
            // Full streaks at the nearest they fall; short ones in the ring.
            assert!(screen_share(fall.size.y, FAR_CLEAR_M) <= 0.061, "{kind:?}");
            let near = fall.size.y.min(NEAR_LENGTH_M);
            assert!(screen_share(near, CLEAR_ROUND_EYE_M) <= 0.061, "{kind:?}");
        }
        let mesh = near_mesh(Vec2::new(0.02, 0.75));
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|values| values.as_float3())
            .expect("positions");
        assert_eq!(positions.len(), NEAR_STREAKS * 8);
        for quad in positions.chunks(4) {
            // Short, and in the ring round the upright.
            let (foot, top) = (Vec3::from_array(quad[0]), Vec3::from_array(quad[3]));
            assert!((top.y - foot.y - NEAR_LENGTH_M).abs() < 1e-4);
            let flat = Vec2::new(foot.x, foot.z).length();
            assert!(
                (CLEAR_ROUND_EYE_M - 0.05..FAR_CLEAR_M).contains(&flat),
                "{flat}"
            );
        }
        // The two haze layers are anchored to the ground: moving the square
        // with the focus moves its texture coordinates with it.
        let (size, focus) = (600.0, Vec3::new(40.0, 3.0, -20.0));
        for (tile, turn) in [(HAZE_TILE_M, 0.0), (HAZE_SECOND_TILE_M, HAZE_SECOND_TURN)] {
            let at_world = |focus: Vec3, world: Vec2| {
                let mesh_uv = Vec2::splat(0.5) + (world - Vec2::new(focus.x, focus.z)) / size;
                haze_uv(size, tile, turn, focus, 0.0).transform_point2(mesh_uv)
            };
            let spot = Vec2::new(55.0, -7.0);
            let moved = focus + Vec3::new(13.0, 0.0, 9.0);
            assert!((at_world(focus, spot) - at_world(moved, spot)).length() < 1e-3);
        }
    }

    #[test]
    fn the_ground_wets_and_dries_and_snow_settles_and_melts() {
        // Dry and clear: the ground as it is.
        let dry = ground_look(0.0, 0.0, None);
        assert_eq!(dry.factor, [1.0; 3]);
        assert_eq!((dry.snow_glow, dry.roughness), (0.0, DRY_ROUGHNESS));
        // Wet: darker, cooler and with a sheen.
        let wet = ground_look(1.0, 0.0, None);
        assert!(wet.factor.iter().all(|c| *c < 0.8) && wet.factor[2] > wet.factor[0]);
        assert!(wet.roughness < 0.6);
        // Snow: paler, and glowing with the sky light.
        let snow = ground_look(0.0, 1.0, None);
        assert!(snow.factor.iter().all(|c| *c > 1.1) && snow.snow_glow > 0.4);
        // A drought parches, a blight sours.
        let parched = ground_look(0.0, 0.0, Some(Shock::Drought));
        assert!(parched.factor[0] > 1.0 && parched.factor[2] < 0.75);
        let soured = ground_look(0.0, 0.0, Some(Shock::Blight));
        assert!(soured.factor[2] < soured.factor[0]);
        // Rain wets, a harsh winter lays deep snow without falling snow.
        assert_eq!(ground_targets(Weather::Rain, None), (1.0, 0.0));
        assert_eq!(
            ground_targets(Weather::Clear, Some(Shock::HarshWinter)).1,
            1.0
        );
        assert_eq!(ground_targets(Weather::Clear, None), (0.0, 0.0));
        // It builds and fades over seconds, not at once.
        let mut cover = 0.0;
        for _ in 0..60 {
            cover = ease_towards(cover, 1.0, 0.1, SETTLE_SECS, MELT_SECS);
        }
        assert!(cover > 0.1 && cover < 0.2, "{cover}");
        let melting = ease_towards(1.0, 0.0, 6.0, SETTLE_SECS, MELT_SECS);
        assert!(melting > 0.9 && melting < 1.0);
        // A storm bends the plants; a clear day does not.
        let mut look = WeatherLook::default();
        assert_eq!(wind_strength(&look), 1.0);
        look.kind = Weather::Storm;
        look.amount = 1.0;
        assert!(wind_strength(&look) > 3.0);
    }

    #[test]
    fn streaks_give_way_to_haze_from_high_cameras() {
        assert_eq!(streak_share(0.0), 1.0);
        assert_eq!(streak_share(FADE_FROM_M), 1.0);
        assert_eq!(streak_share(FADE_FROM_M + FADE_OVER_M), 0.0);
        assert_eq!(streak_share(400.0), 0.0);
        let mid = streak_share(FADE_FROM_M + FADE_OVER_M * 0.5);
        assert!((mid - 0.5).abs() < 1e-4, "{mid}");
        // Every falling weather has a haze, clear none.
        assert!(haze_tone(Weather::Clear).is_none());
        for kind in [Weather::Rain, Weather::Storm, Weather::Snow] {
            let (_, opacity) = haze_tone(kind).expect("a haze");
            assert!(opacity > 0.2 && opacity < 0.7);
        }
        // The haze texture repeats without a seam and has both patches and
        // gaps.
        for step in 0..32 {
            let t = step as f32 / 32.0;
            assert!((haze_alpha(0.0, t) - haze_alpha(1.0, t)).abs() < 1e-3);
            assert!((haze_alpha(t, 0.0) - haze_alpha(t, 1.0)).abs() < 1e-3);
        }
        let samples: Vec<f32> = (0..64 * 64)
            .map(|i| haze_alpha((i % 64) as f32 / 64.0, (i / 64) as f32 / 64.0))
            .collect();
        let clear = samples.iter().filter(|a| **a < 0.05).count();
        let thick = samples.iter().filter(|a| **a > 0.6).count();
        assert!(clear > 300 && thick > 300, "{clear} clear, {thick} thick");
        // Capture overrides are parsed, unknown words ignored.
        assert_eq!(parse_shock("drought"), Some(Some(Shock::Drought)));
        assert_eq!(parse_shock("blight"), Some(Some(Shock::Blight)));
        assert_eq!(parse_shock("none"), Some(None));
        assert_eq!(parse_shock("fog"), None);
    }

    #[test]
    fn lightning_is_a_brief_double_flash() {
        let lit = (0..730)
            .filter(|step| lightning(*step as f32 / 100.0) > 0.0)
            .count();
        assert!(lit > 20 && lit < 45, "{lit} hundredths of 7.3 s");
        assert_eq!(lightning(1.0), 0.0);
        assert!(lightning(5.1) > 0.99);
        assert!(lightning(5.43) > 0.5 && lightning(5.43) < 0.75);
    }
}
