//! Chimney smoke for homes with a fire going and embers over a working forge.
//! A puff is a camera-facing quad with a soft-edged alpha texture made in
//! code; a column is a handful of them that overlap, grow, drift and thin
//! out as they rise. Puffs are children of the building's own visual root,
//! so they inherit its terrain height and fog visibility. State is read from
//! the sim and never written.
use std::collections::{BTreeMap, BTreeSet};

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use island_content::village::{
    self, Role, Season, VillagePlace, colony::Household, homestead::Cottage,
};
use npc_framework::crafting::Crafting;
use npc_framework::prelude::{SimClock, SimConfig, SimId, SimIndex, SimPos};
use npc_framework::view_helpers::VisualIndex;

use super::daylight::{ViewDayOverride, view_daylight};
use super::night::AT_HOME_MM2;
use crate::RtsCamera;
use crate::fog::{Fogged, Look};
use crate::island_homes::{Cutaways, HomeShell, Shells};

pub(crate) const PUFFS_PER_CHIMNEY: usize = 9;
/// A forge's own smoke: fewer and smaller puffs than a hearth's.
pub(crate) const PUFFS_PER_FORGE: usize = 5;
const FORGE_PUFF_SIZE: f32 = 0.7;
pub(crate) const EMBERS_PER_FORGE: usize = 7;
/// Shared materials a puff fades through. Eight steps of at most 7 % opacity
/// each are not seen as steps under a soft texture.
const FADE_STEPS: usize = 8;
/// Real seconds a puff lives.
const PUFF_LIFE: f32 = 7.5;
/// Real seconds of an ember's hop; each ember's own is 0.8 to 1.4 of it.
const EMBER_LIFE: f32 = 1.1;
/// The part of a puff's quad that reads as smoke. The texture is densest in
/// the middle and gone at the rim, so a quad looks about this much of its
/// width across.
#[cfg(test)]
pub(crate) const PUFF_BODY: f32 = 0.62;
/// Radius of an ember's mesh, metres at scale 1: a spark 13 cm
/// across, a few pixels from settlement zoom, which bloom then widens.
const EMBER_RADIUS: f32 = 0.065;
/// Side of the smoke texture, texels.
const PUFF_TEXELS: usize = 64;

/// Chimney mouth of a homestead cottage, relative to its visual root. A
/// settlement home's is the mouth of its fitted model (`island_homes`).
pub(crate) const COTTAGE_CHIMNEY: Vec3 = Vec3::new(0.0, 3.0, 0.2);
/// Forge chimney of the blacksmith mesh at the old workplace scale and
/// offset: for a smithy without a fitted model (a host that draws none).
const FORGE_CHIMNEY: Vec3 = Vec3::new(0.50, 2.38, 3.16);

/// A repeatable number in 0..1 for a puff or an ember, from its seed.
pub(crate) fn variation(seed: f32, salt: f32) -> f32 {
    ((seed * 12.989_8 + salt * 78.233).sin() * 43_758.547)
        .fract()
        .abs()
}

/// A puff at `age` 0..1 of its life.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PuffState {
    /// From the chimney mouth, metres, before the wind frame is applied.
    pub offset: Vec3,
    /// Side of the quad, metres. About six tenths of it reads as smoke.
    pub size: f32,
    /// Turn of the quad about the line of sight, so no two look alike.
    pub roll: f32,
    /// 0 transparent, 1 as dense as smoke gets.
    pub fade: f32,
}

/// A puff leaves the mouth small and dense, slowly at first so the column
/// has a body there, rises about five metres, leans
/// downwind more the higher it is, wanders a little off the column's line,
/// and grows and thins all the way up. Each puff's size, wander and turn are
/// its own.
pub(crate) fn puff_state(age: f32, seed: f32) -> PuffState {
    let age = age.clamp(0.0, 1.0);
    let lean = age.powf(1.7);
    let sway = (age * 5.0 + seed * 17.0).sin() * 0.16 * age;
    let wander = Vec2::new(variation(seed, 1.0) - 0.5, variation(seed, 2.0) - 0.5) * 0.7 * age;
    let offset = Vec3::new(
        lean * 1.9 + sway + wander.x,
        age.powf(1.3) * 5.2,
        lean * 0.6 + sway * 0.5 + wander.y,
    );
    let size = (0.55 + 2.3 * age.powf(0.8)) * (0.82 + 0.36 * variation(seed, 3.0));
    let turn = (variation(seed, 4.0) - 0.5) * 2.2;
    let roll = variation(seed, 5.0) * std::f32::consts::TAU + age * turn;
    // In over the first twelfth of the life, then thinning to nothing.
    let rise = (age / 0.08).min(1.0);
    let fade = rise * rise * (3.0 - 2.0 * rise) * (1.0 - age).powf(1.4);
    PuffState {
        offset,
        size,
        roll,
        fade,
    }
}

/// Alpha of the smoke texture at `(u, v)` in 0..1: densest in the middle,
/// falling smoothly to exactly nothing before the edge of the quad, with a
/// lobed outline and a mottled body so a puff is a wisp and not a disc.
pub(crate) fn puff_alpha(u: f32, v: f32) -> f32 {
    let (x, y) = (u * 2.0 - 1.0, v * 2.0 - 1.0);
    let angle = y.atan2(x);
    // Three lobes of different width push the outline in and out.
    let outline = 0.80
        + 0.10 * (angle * 3.0 + 0.6).sin()
        + 0.06 * (angle * 5.0 - 1.9).sin()
        + 0.04 * (angle * 8.0 + 2.7).sin();
    let radius = (x * x + y * y).sqrt() / outline;
    if radius >= 1.0 {
        return 0.0;
    }
    let body = (1.0 - radius * radius).powf(1.5);
    let mottle = 0.86
        + 0.14 * ((x * 5.3 + 1.1).sin() * (y * 4.7 - 0.4).cos() + (x * 9.1 - y * 7.3).sin() * 0.5)
            / 1.5;
    (body * mottle).clamp(0.0, 1.0)
}

/// The smoke texture: white, with [`puff_alpha`] as its alpha. Generated in
/// code; no asset file.
pub(crate) fn puff_image() -> Image {
    let mut data = Vec::with_capacity(PUFF_TEXELS * PUFF_TEXELS * 4);
    for row in 0..PUFF_TEXELS {
        for column in 0..PUFF_TEXELS {
            let at = |texel: usize| (texel as f32 + 0.5) / PUFF_TEXELS as f32;
            let alpha = (puff_alpha(at(column), at(row)) * 255.0).round() as u8;
            data.extend([255, 255, 255, alpha]);
        }
    }
    Image::new(
        Extent3d {
            width: PUFF_TEXELS as u32,
            height: PUFF_TEXELS as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Which of the shared fading materials a puff uses.
pub(crate) fn fade_step(fade: f32) -> usize {
    ((fade.clamp(0.0, 1.0) * FADE_STEPS as f32) as usize).min(FADE_STEPS - 1)
}

/// Position and scale of ember `seed` at `seconds`. Every ember has its own
/// rhythm: it sits out part of each cycle, hops out of the chimney mouth on
/// its own arc (0.7 to 1.6 m up, up to 1.2 m to the side), burns down as it
/// goes and flickers on the way. A scale of zero is an ember not showing.
pub(crate) fn ember_state(seconds: f32, seed: f32) -> (Vec3, f32) {
    let life = EMBER_LIFE * (0.8 + 0.6 * variation(seed, 6.0));
    // The hop takes 55 to 85 % of the cycle; the rest is the wait for the
    // next one, so only some of a forge's embers are in the air at once.
    let airborne = 0.55 + 0.30 * variation(seed, 7.0);
    let cycle = seconds / (life / airborne) + variation(seed, 8.0);
    let hop = cycle.floor();
    let age = cycle.fract() / airborne;
    if age >= 1.0 {
        return (Vec3::ZERO, 0.0);
    }
    // A new direction and height every hop, so no ember retraces its path.
    let angle = variation(seed + hop * 0.37, 9.0) * std::f32::consts::TAU;
    let reach = 0.25 + 0.95 * variation(seed + hop * 0.53, 10.0);
    let lift = 0.7 + 0.9 * variation(seed + hop * 0.71, 11.0);
    let offset = Vec3::new(
        angle.cos() * reach * age,
        // Up fast, hanging at the top of the arc, then falling a little.
        lift * (2.4 * age - 1.6 * age * age) / 0.9,
        angle.sin() * reach * age,
    );
    // Two beats that drift against each other: an uneven, restless glint.
    let flicker =
        0.62 + 0.38 * (seconds * 23.0 + seed * 61.0).sin() * (seconds * 37.0 + seed * 17.0).cos();
    let burn = (1.0 - age).powf(0.7);
    (offset, (burn * flicker).max(0.0))
}

/// Colour of the smoke for a value of the day curve, linear. Smoke is not
/// lit by the scene's lights: a billboard's normal follows the camera, so
/// real lighting would brighten and darken it as the view orbits. It takes
/// its light from the same sky model instead. By day it is a pale grey a
/// little short of white, through the whole working day (it follows the
/// sun's strength, not the raw curve); at night it is a blue-grey a third as
/// bright, lighter than the dark ground behind it so the column still reads,
/// and far below the lamps and windows. A third of that again, the first
/// night value, could not be seen at all.
pub(crate) fn smoke_shade(daylight: f32) -> [f32; 3] {
    let strength = super::sky::sun_strength(daylight);
    let shade = 0.24 + 0.50 * strength;
    // Moonlight is blue; the day's light is neutral.
    let night = 1.0 - strength;
    [
        shade * (1.0 - 0.14 * night),
        shade * (1.0 - 0.04 * night),
        shade * (1.0 + 0.16 * night),
    ]
}

#[derive(Resource)]
pub(crate) struct SmokeAssets {
    puff: Handle<Mesh>,
    fades: [Handle<StandardMaterial>; FADE_STEPS],
    ember_mesh: Handle<Mesh>,
    ember: Handle<StandardMaterial>,
}

/// Whether a home's chimney smokes.
///
/// A heated home always does: that is the simulation's own hearth
/// (`Household::heated_until`). The simulation lights a hearth only in the
/// cold (at night, or in winter) when a resident brings a log to the door,
/// and the fire lasts half a day. A new island starts at sunrise with every
/// hearth cold, and a night's fire is out by early afternoon, so on heat
/// alone no chimney smokes on the first day and none at any dusk.
/// A settlement home with someone in therefore also shows its cooking fire.
/// That second rule is a view interpretation and is switched off where smoke
/// must stay a truthful sign of warmth: in winter, and for a homestead
/// cottage, whose fuel the player manages.
pub(crate) fn hearth_smokes(
    heated: bool,
    occupied: bool,
    winter: bool,
    cottage: bool,
    live: bool,
) -> bool {
    live && (heated || (occupied && !winter && !cottage))
}

/// Real seconds for a chimney to come up to full smoke or die down.
const HEARTH_FADE_SECS: f32 = 3.0;

#[derive(Component)]
pub(crate) struct Chimney {
    home: SimId,
    root: Entity,
    cottage: bool,
    /// 0 cold, 1 drawing fully.
    level: f32,
    wind: Quat,
}

impl Chimney {
    /// How fully the hearth under it draws: 0 cold, 1 a full fire.
    pub(crate) fn level(&self) -> f32 {
        self.level
    }
}
#[derive(Component)]
pub(crate) struct Puff {
    seed: f32,
    step: usize,
    /// Scale against a hearth's puff.
    size: f32,
}

impl Puff {
    /// Puff `index` of `count` over building `id`: evenly spread through the
    /// life of a puff, each column out of step with its neighbours'.
    fn new(index: usize, count: usize, id: SimId, size: f32) -> Self {
        Self {
            seed: index as f32 / count as f32 + (id.0 % 7) as f32 * 0.031,
            step: 0,
            size,
        }
    }
}
#[derive(Component)]
pub(crate) struct Forge {
    station: SimId,
    root: Entity,
    wind: Quat,
}
#[derive(Component)]
pub(crate) struct Ember {
    seed: f32,
}
/// The fire seen in the forge chimney's mouth while it works.
#[derive(Component)]
pub(crate) struct ForgeGlow;
#[derive(Component)]
pub(crate) struct ForgeLight;

#[derive(Resource, Default)]
pub(crate) struct Dressed(BTreeMap<SimId, Entity>);

/// Opacity of the densest shared smoke material, at the middle of a puff
/// just out of the chimney. The texture thins it towards the rim, and where
/// puffs overlap near the mouth they add up to a body. At 0.56 the column
/// was a faint wisp against pale ground by day.
const PUFF_OPACITY: f32 = 0.8;

/// A puff fainter than half the faintest material is not drawn at all.
const HIDDEN_BELOW: f32 = 0.5 / FADE_STEPS as f32;

/// Opacity of shared smoke material `step`, from the faintest to the densest.
/// The plume's shade and density against the ground: against snow a
/// heated home's smoke is drawn darker and denser, so it reads from the
/// overview; otherwise as it is.
pub(crate) fn plume(snowy: bool) -> (f32, f32) {
    if snowy { (0.7, 1.45) } else { (1.0, 1.0) }
}

pub(crate) fn fade_alpha(step: usize) -> f32 {
    PUFF_OPACITY * (step.min(FADE_STEPS - 1) + 1) as f32 / FADE_STEPS as f32
}

pub(crate) fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let [r, g, b] = smoke_shade(1.0);
    // Without an image store (a headless test) the quads are plain.
    let texture = images.map(|mut images| images.add(puff_image()));
    let fades = std::array::from_fn(|step| {
        materials.add(StandardMaterial {
            base_color: Color::linear_rgba(r, g, b, fade_alpha(step)),
            base_color_texture: texture.clone(),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            // A billboard is seen from one side; never cull it by mistake.
            cull_mode: None,
            double_sided: true,
            ..default()
        })
    });
    commands.insert_resource(SmokeAssets {
        // A unit quad facing +Z; each puff scales it and turns it to the
        // camera every frame.
        puff: meshes.add(Rectangle::new(1.0, 1.0)),
        fades,
        ember_mesh: meshes.add(
            Sphere::new(EMBER_RADIUS)
                .mesh()
                .ico(1)
                .expect("ember sphere"),
        ),
        // Unlit and a few times brighter than white, so it stays orange in
        // full sun and blooms. Brighter still, the tonemapper bleaches it to
        // cream; the first, lit emissive version washed out against the roof.
        ember: materials.add(StandardMaterial {
            base_color: Color::linear_rgb(3.6, 0.8, 0.06),
            unlit: true,
            ..default()
        }),
    });
}

/// Sim ticks a second at normal speed, for the work clock of forges.
const WORK_TICKS_PER_SEC: f32 = 20.0;

/// How the island's wind bends a column: turned to blow its way, leaning
/// further the stronger it is. The default is the old fixed breeze.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bend {
    turn: Quat,
    reach: f32,
}

impl Default for Bend {
    fn default() -> Self {
        Self {
            turn: Quat::IDENTITY,
            reach: 1.0,
        }
    }
}

impl Bend {
    pub(crate) fn of(wind: &crate::island_world::ambient::Wind) -> Self {
        // The column's own lean is towards +x (and a little +z): turned so
        // +x is downwind.
        Self {
            turn: Quat::from_rotation_y(wind.yaw()),
            reach: (0.45 + 0.55 * wind.strength).clamp(0.3, 2.2),
        }
    }

    /// A puff's offset from the mouth, bent: the rise stays, the drift
    /// across turns downwind and stretches with the strength.
    pub(crate) fn apply(&self, offset: Vec3) -> Vec3 {
        let across = Vec3::new(offset.x, 0.0, offset.z) * self.reach;
        self.turn * across + Vec3::Y * offset.y
    }
}

/// The turn that undoes a building's own, applied to its puffs' drift so
/// every column on the island leans with one wind instead of with the way
/// its house happens to face.
fn wind_frame(root: Option<&Transform>) -> Quat {
    root.map_or(Quat::IDENTITY, |root| root.rotation.inverse())
}

/// Attach chimneys and forges to building visuals as they appear.
#[allow(clippy::too_many_arguments)]
pub(crate) fn dress(
    mut commands: Commands,
    assets: Option<Res<SmokeAssets>>,
    homes: Query<(Entity, &SimId, Has<Cottage>), With<Household>>,
    places: Query<(Entity, &SimId, &VillagePlace)>,
    visuals: Res<VisualIndex>,
    bodies: Option<Res<Shells>>,
    roots: Query<(&Transform, Option<&HomeShell>)>,
    fitted: Query<&crate::island_world::workplaces::WorkplaceFit>,
    mut dressed: ResMut<Dressed>,
) {
    let Some(assets) = assets else { return };
    dressed.0.retain(|_, entity| roots.contains(*entity));
    for (entity, id, cottage) in &homes {
        if dressed.0.contains_key(id) {
            continue;
        }
        let Some((root, (turn, shell))) = visuals
            .map
            .get(&entity)
            .and_then(|root| Some((*root, roots.get(*root).ok()?)))
        else {
            continue;
        };
        let mouth = match shell {
            // The mouth of the fitted model, under the root.
            Some(shell) => shell.local.transform_point(shell.fit.chimney()),
            // Its body is on its way.
            None if bodies.is_some() => continue,
            None if cottage => COTTAGE_CHIMNEY,
            None => Vec3::new(0.0, 3.75, -2.1),
        };
        let chimney = commands
            .spawn((
                Name::new("hearth smoke"),
                ChildOf(root),
                Chimney {
                    home: *id,
                    root,
                    cottage,
                    level: 0.0,
                    wind: wind_frame(Some(turn)),
                },
                Transform::from_translation(mouth),
                Visibility::Hidden,
            ))
            .id();
        for i in 0..PUFFS_PER_CHIMNEY {
            commands.spawn((
                ChildOf(chimney),
                Puff::new(i, PUFFS_PER_CHIMNEY, *id, 1.0),
                Mesh3d(assets.puff.clone()),
                MeshMaterial3d(assets.fades[0].clone()),
                Transform::default(),
                bevy::light::NotShadowCaster,
                bevy::light::NotShadowReceiver,
            ));
        }
        dressed.0.insert(*id, chimney);
    }
    for (entity, id, place) in &places {
        if place.role != Role::Smith || dressed.0.contains_key(id) {
            continue;
        }
        let Some(&root) = visuals
            .map
            .get(&entity)
            .filter(|root| roots.contains(**root))
        else {
            continue;
        };
        // The mouth of the fitted smithy's chimney and its fire below it.
        let (chimney, fire) =
            fitted
                .get(root)
                .map_or((FORGE_CHIMNEY, Vec3::new(0.0, -1.2, 0.9)), |fit| {
                    use crate::island_world::workplaces::{FORGE_CHIMNEY_UNITS, FORGE_FIRE_UNITS};
                    let chimney = fit.0.transform_point(FORGE_CHIMNEY_UNITS);
                    (chimney, fit.0.transform_point(FORGE_FIRE_UNITS) - chimney)
                });
        let forge = commands
            .spawn((
                Name::new("forge embers"),
                ChildOf(root),
                Forge {
                    station: *id,
                    root,
                    wind: wind_frame(roots.get(root).ok().map(|(turn, _)| turn)),
                },
                Transform::from_translation(chimney),
                Visibility::Hidden,
            ))
            .id();
        for i in 0..EMBERS_PER_FORGE {
            commands.spawn((
                ChildOf(forge),
                Ember {
                    // Unevenly spread, so the embers never march in step.
                    seed: i as f32 * 0.618_034 + 0.013 * (id.0 % 11) as f32,
                },
                Mesh3d(assets.ember_mesh.clone()),
                MeshMaterial3d(assets.ember.clone()),
                Transform::default(),
                bevy::light::NotShadowCaster,
            ));
        }
        for i in 0..PUFFS_PER_FORGE {
            commands.spawn((
                ChildOf(forge),
                Puff::new(i, PUFFS_PER_FORGE, *id, FORGE_PUFF_SIZE),
                Mesh3d(assets.puff.clone()),
                MeshMaterial3d(assets.fades[0].clone()),
                Transform::default(),
                bevy::light::NotShadowCaster,
                bevy::light::NotShadowReceiver,
            ));
        }
        commands.spawn((
            ChildOf(forge),
            ForgeGlow,
            Mesh3d(assets.ember_mesh.clone()),
            MeshMaterial3d(assets.ember.clone()),
            Transform::from_xyz(0.0, 0.05, 0.0),
            bevy::light::NotShadowCaster,
        ));
        commands.spawn((
            ChildOf(forge),
            ForgeLight,
            PointLight {
                color: Color::srgb(1.0, 0.48, 0.16),
                intensity: 0.0,
                range: 9.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(fire),
        ));
        dressed.0.insert(*id, forge);
    }
}

fn live(fog: &Query<&Fogged>, root: Entity) -> bool {
    fog.get(root).map_or(true, |fog| fog.0 == Look::Live)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn animate(
    (clock, config, pinned, cut_away, unblocked): (
        Res<SimClock>,
        Res<SimConfig>,
        Option<Res<ViewDayOverride>>,
        Option<Res<Cutaways>>,
        Option<Res<crate::island_homes::Unblocked>>,
    ),
    (time, wind, sim_time, rate, control): (
        Res<Time>,
        Option<Res<crate::island_world::ambient::Wind>>,
        Option<Res<npc_framework::prelude::SimTime>>,
        Option<Res<npc_framework::prelude::SimRate>>,
        Option<Res<npc_framework::prelude::SimControl>>,
    ),
    index: Res<SimIndex>,
    assets: Option<Res<SmokeAssets>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    homes: Query<&Household>,
    positions: Query<&SimPos>,
    crafts: Query<&Crafting>,
    fog: Query<&Fogged>,
    mut chimneys: Query<(&mut Chimney, &Children, &mut Visibility), Without<Forge>>,
    mut puffs: Query<(
        &mut Puff,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
    mut forges: Query<(&Forge, &Children, &mut Visibility), Without<Chimney>>,
    mut embers: Query<
        (Option<&Ember>, &mut Transform),
        (Without<Puff>, Or<(With<Ember>, With<ForgeGlow>)>),
    >,
    mut forge_lights: Query<&mut PointLight, With<ForgeLight>>,
    cameras: Query<&GlobalTransform, With<RtsCamera>>,
    mut last_light: Local<Option<f32>>,
) {
    let Some(assets) = assets else { return };
    // Hearth smoke is weather: it drifts on the view clock with the island's
    // one wind, also while paused. A forge's smoke and sparks are work: they
    // follow the sim clock and stop with it.
    let (seconds, bend) = wind
        .as_deref()
        .map_or((time.elapsed_secs(), Bend::default()), |wind| {
            (wind.time, Bend::of(wind))
        });
    let work_seconds = if wind.is_some() {
        let alpha = npc_framework::view_helpers::interp::interp_alpha(
            sim_time.as_deref(),
            rate.as_deref(),
            control.as_deref(),
        );
        (clock.tick as f32 - 1.0 + alpha) / WORK_TICKS_PER_SEC
    } else {
        seconds
    };
    // Every puff faces the island camera.
    let facing = cameras
        .iter()
        .next()
        .map_or(Quat::IDENTITY, |camera| camera.rotation());
    // Unlit smoke would glow at night: follow the sky's brightness instead.
    let light = view_daylight(clock.tick, &config, pinned.as_deref());
    let winter = village::calendar_at(clock.tick, &config, true).1 == Season::Winter;
    // Against snow the smoke is drawn darker and denser, so a heated home's
    // column reads from the overview; on the island only (with the wind).
    let snowy = winter && wind.is_some();
    let key = light + if snowy { 10.0 } else { 0.0 };
    // Always on the first frame: the materials are created in the noon
    // colour, and a night that never changes (a pinned phase) must not keep it.
    if last_light.is_none_or(|last| (key - last).abs() > 0.01) {
        *last_light = Some(key);
        let [r, g, b] = smoke_shade(light);
        let (shade, dense) = plume(snowy);
        for (step, handle) in assets.fades.iter().enumerate() {
            if let Some(mut material) = materials.get_mut(handle) {
                material.base_color = Color::linear_rgba(
                    r * shade,
                    g * shade,
                    b * shade * 1.04,
                    (fade_alpha(step) * dense).min(0.95),
                );
            }
        }
    }
    let step = (time.delta_secs() / HEARTH_FADE_SECS).min(1.0);
    let gone = |root: Entity| unblocked.as_deref().is_some_and(|cut| cut.hides(root));
    for (mut chimney, children, mut visibility) in &mut chimneys {
        let home = index
            .get(chimney.home)
            .and_then(|home| homes.get(home).ok());
        let heated = home.is_some_and(|home| home.heated_until > clock.tick);
        let occupied = home.is_some_and(|home| {
            home.residents.iter().any(|resident| {
                index
                    .get(*resident)
                    .and_then(|resident| positions.get(resident).ok())
                    .is_some_and(|at| at.dist2(home.door) <= AT_HOME_MM2)
            })
        });
        // A cut-away home has lost its roof and its chimney with it, and so
        // has one taken out of the camera's way.
        let roofed = !cut_away
            .as_deref()
            .is_some_and(|cut| cut.is_open(chimney.home))
            && !gone(chimney.root);
        let smoking = hearth_smokes(
            heated,
            occupied,
            winter,
            chimney.cottage,
            roofed && live(&fog, chimney.root),
        );
        let level = if smoking {
            (chimney.level + step).min(1.0)
        } else {
            (chimney.level - step).max(0.0)
        };
        if chimney.level != level {
            chimney.level = level;
        }
        let want = if level > 0.0 && roofed {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != want {
            *visibility = want;
        }
        if level <= 0.0 {
            continue;
        }
        for child in children.iter() {
            let Ok((mut puff, mut transform, mut material)) = puffs.get_mut(child) else {
                continue;
            };
            drift(
                &mut puff,
                &mut transform,
                &mut material,
                &assets,
                seconds,
                level,
                chimney.wind,
                bend,
                facing,
            );
        }
    }
    let working: BTreeSet<SimId> = crafts
        .iter()
        .filter_map(|craft| craft.workstation)
        .collect();
    for (forge, children, mut visibility) in &mut forges {
        let active =
            working.contains(&forge.station) && live(&fog, forge.root) && !gone(forge.root);
        let want = if active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != want {
            *visibility = want;
        }
        for child in children.iter() {
            if let Ok(mut light) = forge_lights.get_mut(child) {
                let intensity = if active {
                    9_000.0 * (0.82 + 0.18 * (work_seconds * 11.0 + forge.station.0 as f32).sin())
                } else {
                    0.0
                };
                if light.intensity != intensity {
                    light.intensity = intensity;
                }
            }
            if !active {
                continue;
            }
            if let Ok((mut puff, mut transform, mut material)) = puffs.get_mut(child) {
                drift(
                    &mut puff,
                    &mut transform,
                    &mut material,
                    &assets,
                    work_seconds,
                    1.0,
                    forge.wind,
                    bend,
                    facing,
                );
            }
            match embers.get_mut(child) {
                Ok((Some(ember), mut transform)) => {
                    let (offset, scale) = ember_state(work_seconds, ember.seed);
                    transform.translation = offset;
                    transform.scale = Vec3::splat(scale);
                }
                // The fire in the chimney mouth is a low bed of coals that
                // breathes with the bellows and flickers, not a ball.
                Ok((None, mut transform)) => {
                    let phase = forge.station.0 as f32;
                    let breath = (work_seconds * 7.0 + phase).sin();
                    let flicker =
                        (work_seconds * 29.0 + phase * 3.0).sin() * (work_seconds * 17.0).cos();
                    transform.scale =
                        Vec3::new(3.4, 1.1, 3.4) * (1.0 + 0.10 * breath + 0.08 * flicker);
                }
                Err(_) => {}
            }
        }
    }
}

/// The local turn that makes a quad face the camera: `frame` undoes the
/// building's own rotation, `facing` is the camera's, and `roll` turns the
/// quad about the line of sight.
pub(crate) fn billboard(frame: Quat, facing: Quat, roll: f32) -> Quat {
    frame * facing * Quat::from_rotation_z(roll)
}

/// Moves one puff along its column, turns it to the camera and picks its
/// shared material.
#[allow(clippy::too_many_arguments)]
fn drift(
    puff: &mut Puff,
    transform: &mut Transform,
    material: &mut MeshMaterial3d<StandardMaterial>,
    assets: &SmokeAssets,
    seconds: f32,
    level: f32,
    wind: Quat,
    bend: Bend,
    facing: Quat,
) {
    let age = (seconds / PUFF_LIFE + puff.seed).fract();
    let state = puff_state(age, puff.seed);
    transform.translation = wind * bend.apply(state.offset * puff.size.sqrt());
    transform.rotation = billboard(wind, facing, state.roll);
    let fade = state.fade * level;
    // Too thin for the faintest material: not drawn, so a puff is gone
    // before it wraps back to the chimney and never pops in.
    transform.scale = if fade < HIDDEN_BELOW {
        Vec3::ZERO
    } else {
        Vec3::splat(state.size * puff.size)
    };
    let step = fade_step(fade);
    if puff.step != step || material.0 != assets.fades[step] {
        puff.step = step;
        material.0 = assets.fades[step].clone();
    }
}

/// Puffs and embers of a burning building: a dense column with sparks.
const PUFFS_PER_BLAZE: usize = 9;
const EMBERS_PER_BLAZE: usize = 9;
/// Scale of a blaze's puff against a hearth's.
const BLAZE_PUFF_SIZE: f32 = 1.6;
/// Where a blaze rises from, over the building's visual root.
const BLAZE_MOUTH: Vec3 = Vec3::new(0.0, 3.0, 0.0);

/// The smoke and sparks of a building on fire (`civic::upkeep`): spawned
/// under its visual root while the simulation lists it as burning, and gone
/// when the fire is out. Read from the sim, never written.
#[derive(Component)]
pub(crate) struct Blaze {
    building: SimId,
    wind: Quat,
}

/// Keeps one blaze over every burning building and moves its puffs and
/// embers. The sim's list of burning buildings is short and read whole; no
/// building is scanned.
#[allow(clippy::too_many_arguments)]
pub(crate) fn blaze(
    mut commands: Commands,
    assets: Option<Res<SmokeAssets>>,
    upkeep: Option<Res<village::civic::upkeep::UpkeepState>>,
    index: Res<SimIndex>,
    visuals: Res<VisualIndex>,
    roots: Query<&Transform, (Without<Puff>, Without<Ember>)>,
    (time, wind): (Res<Time>, Option<Res<crate::island_world::ambient::Wind>>),
    blazes: Query<(Entity, &Blaze, &Children)>,
    mut puffs: Query<(
        &mut Puff,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
    mut embers: Query<(&Ember, &mut Transform), Without<Puff>>,
    cameras: Query<&GlobalTransform, With<RtsCamera>>,
) {
    let Some(assets) = assets else { return };
    let burning: &[SimId] = upkeep
        .as_deref()
        .map_or(&[], |state| state.burning.as_slice());
    if burning.is_empty() && blazes.is_empty() {
        return;
    }
    let mut shown = BTreeSet::new();
    for (entity, blaze, _) in &blazes {
        if burning.contains(&blaze.building) {
            shown.insert(blaze.building);
        } else {
            commands.entity(entity).despawn();
        }
    }
    for id in burning {
        if shown.contains(id) {
            continue;
        }
        let Some((root, turn)) = index
            .get(*id)
            .and_then(|entity| visuals.map.get(&entity))
            .and_then(|root| Some((*root, roots.get(*root).ok()?)))
        else {
            continue;
        };
        let column = commands
            .spawn((
                Name::new("fire"),
                ChildOf(root),
                Blaze {
                    building: *id,
                    wind: wind_frame(Some(turn)),
                },
                Transform::from_translation(BLAZE_MOUTH),
                Visibility::Inherited,
            ))
            .id();
        for i in 0..PUFFS_PER_BLAZE {
            commands.spawn((
                ChildOf(column),
                Puff::new(i, PUFFS_PER_BLAZE, *id, BLAZE_PUFF_SIZE),
                Mesh3d(assets.puff.clone()),
                MeshMaterial3d(assets.fades[0].clone()),
                Transform::default(),
                bevy::light::NotShadowCaster,
                bevy::light::NotShadowReceiver,
            ));
        }
        for i in 0..EMBERS_PER_BLAZE {
            commands.spawn((
                ChildOf(column),
                Ember {
                    seed: i as f32 * 0.618_034 + 0.017 * (id.0 % 13) as f32,
                },
                Mesh3d(assets.ember_mesh.clone()),
                MeshMaterial3d(assets.ember.clone()),
                Transform::default(),
                bevy::light::NotShadowCaster,
            ));
        }
    }
    let (seconds, bend) = wind
        .as_deref()
        .map_or((time.elapsed_secs(), Bend::default()), |wind| {
            (wind.time, Bend::of(wind))
        });
    let facing = cameras
        .iter()
        .next()
        .map_or(Quat::IDENTITY, |camera| camera.rotation());
    for (_, blaze, children) in &blazes {
        for child in children.iter() {
            if let Ok((mut puff, mut transform, mut material)) = puffs.get_mut(child) {
                drift(
                    &mut puff,
                    &mut transform,
                    &mut material,
                    &assets,
                    seconds * 1.6,
                    1.0,
                    blaze.wind,
                    bend,
                    facing,
                );
            } else if let Ok((ember, mut transform)) = embers.get_mut(child) {
                let (offset, scale) = ember_state(seconds, ember.seed);
                transform.translation = offset * 1.5;
                transform.scale = Vec3::splat(scale * 1.6);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn puffs_start_small_grow_and_are_gone_before_they_wrap() {
        let start = puff_state(0.0, 0.3);
        let mid = puff_state(0.4, 0.3);
        let end = puff_state(0.999, 0.3);
        assert_eq!(start.offset, Vec3::ZERO);
        assert_eq!(start.fade, 0.0);
        assert!(mid.offset.y > 1.2 && end.offset.y > 4.5 && end.offset.y < 5.6);
        assert!(mid.fade > 0.4, "{}", mid.fade);
        assert!(end.fade < 0.01, "a wrapping puff must already be invisible");
        for i in 0..PUFFS_PER_CHIMNEY {
            let seed = Puff::new(i, PUFFS_PER_CHIMNEY, SimId(3), 1.0).seed;
            // What reads as smoke is a third of a metre across at the mouth
            // (the old sphere was half a metre) and well over a metre where
            // it thins out.
            let mouth = puff_state(0.0, seed).size * PUFF_BODY;
            let top = puff_state(1.0, seed).size * PUFF_BODY;
            assert!(mouth > 0.26 && mouth < 0.42, "{mouth}");
            assert!(top > 1.4 && top < 2.2, "{top}");
            let mut previous = puff_state(0.0, seed);
            for step in 1..=40 {
                let now = puff_state(step as f32 / 40.0, seed);
                assert!(now.size > previous.size, "it only grows");
                assert!(now.offset.y > previous.offset.y, "it only rises");
                assert!(now.offset.xz().length() <= 3.2, "the wind is light");
                previous = now;
            }
        }
        assert_eq!(fade_step(0.0), 0);
        assert_eq!(fade_step(1.0), FADE_STEPS - 1);
        assert!(fade_alpha(FADE_STEPS - 1) > 0.7 && fade_alpha(FADE_STEPS - 1) < 0.9);
        assert!(fade_alpha(0) < 0.11, "the faintest step is nearly nothing");
        // The budget is unchanged: nine puffs a chimney, five a forge.
        const { assert!(PUFFS_PER_CHIMNEY == 9 && PUFFS_PER_FORGE == 5) };
    }

    #[test]
    fn a_column_is_overlapping_puffs_not_a_chain_of_separate_ones() {
        for home in [SimId(1), SimId(4), SimId(9)] {
            let seeds: Vec<f32> = (0..PUFFS_PER_CHIMNEY)
                .map(|i| Puff::new(i, PUFFS_PER_CHIMNEY, home, 1.0).seed)
                .collect();
            for frame in 0..60 {
                let seconds = frame as f32 * 0.37;
                let mut column: Vec<PuffState> = seeds
                    .iter()
                    .map(|seed| puff_state((seconds / PUFF_LIFE + seed).fract(), *seed))
                    .collect();
                column.sort_by(|a, b| a.offset.y.total_cmp(&b.offset.y));
                for pair in column.windows(2) {
                    let gap = pair[0].offset.distance(pair[1].offset);
                    let bodies = (pair[0].size + pair[1].size) * 0.5 * PUFF_BODY;
                    assert!(
                        gap < bodies,
                        "puffs {gap} m apart with {bodies} m of smoke between their centres"
                    );
                }
            }
        }
    }

    #[test]
    fn no_two_puffs_of_a_column_look_alike() {
        let puffs: Vec<PuffState> = (0..PUFFS_PER_CHIMNEY)
            .map(|i| puff_state(0.5, Puff::new(i, PUFFS_PER_CHIMNEY, SimId(2), 1.0).seed))
            .collect();
        for (i, a) in puffs.iter().enumerate() {
            for b in &puffs[i + 1..] {
                assert!((a.size - b.size).abs() > 1e-3 || (a.roll - b.roll).abs() > 1e-2);
                assert!(a.offset.xz().distance(b.offset.xz()) > 1e-3);
            }
        }
        let sizes: Vec<f32> = puffs.iter().map(|puff| puff.size).collect();
        let (low, high) = sizes
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), size| {
                (low.min(*size), high.max(*size))
            });
        assert!(high / low > 1.1 && high / low < 1.5, "{low}..{high}");
        for seed in [0.0, 0.37, 1.9] {
            let value = variation(seed, 3.0);
            assert!((0.0..1.0).contains(&value));
            assert_eq!(value, variation(seed, 3.0), "repeatable");
        }
    }

    #[test]
    fn the_smoke_texture_is_soft_to_its_edge() {
        assert!(puff_alpha(0.5, 0.5) > 0.8, "dense in the middle");
        // Nothing at the rim of the quad, all the way round: no hard edge
        // and no square corner.
        for step in 0..=64 {
            let t = step as f32 / 64.0;
            for (u, v) in [(t, 0.0), (t, 1.0), (0.0, t), (1.0, t)] {
                assert_eq!(puff_alpha(u, v), 0.0, "at {u}, {v}");
            }
        }
        // No step anywhere: neighbouring texels differ by a few percent.
        let at = |texel: usize| (texel as f32 + 0.5) / PUFF_TEXELS as f32;
        let mut steepest = 0.0_f32;
        let mut covered = 0.0;
        for row in 0..PUFF_TEXELS {
            for column in 0..PUFF_TEXELS - 1 {
                let here = puff_alpha(at(column), at(row));
                let across = puff_alpha(at(column + 1), at(row));
                let down = puff_alpha(at(row), at(column)) - puff_alpha(at(row), at(column + 1));
                steepest = steepest.max((here - across).abs()).max(down.abs());
                covered += here;
            }
        }
        assert!(steepest < 0.1, "{steepest}");
        let mean = covered / (PUFF_TEXELS * (PUFF_TEXELS - 1)) as f32;
        assert!(
            mean > 0.12 && mean < 0.3,
            "a wisp, not a filled disc: {mean}"
        );
        // Not round: the outline reaches further out in some directions.
        let reach = |angle: f32| {
            (1..=100)
                .map(|step| step as f32 / 100.0)
                .take_while(|r| {
                    puff_alpha(0.5 + 0.5 * r * angle.cos(), 0.5 + 0.5 * r * angle.sin()) > 0.02
                })
                .count()
        };
        let reaches: Vec<usize> = (0..12)
            .map(|i| reach(i as f32 * std::f32::consts::FRAC_PI_6))
            .collect();
        assert!(reaches.iter().max().unwrap() - reaches.iter().min().unwrap() >= 8);

        let image = puff_image();
        assert_eq!(image.width() as usize, PUFF_TEXELS);
        let data = image.data.as_ref().expect("texels");
        assert_eq!(data.len(), PUFF_TEXELS * PUFF_TEXELS * 4);
        assert_eq!(&data[..4], &[255, 255, 255, 0], "a clear corner");
        let centre = (PUFF_TEXELS / 2 * PUFF_TEXELS + PUFF_TEXELS / 2) * 4;
        assert!(data[centre + 3] > 200);
    }

    #[test]
    fn a_puff_faces_the_camera_whichever_way_its_building_and_the_view_turn() {
        for turn in [0.0, 1.2, std::f32::consts::PI, -2.0] {
            let house = Transform::from_rotation(Quat::from_rotation_y(turn));
            for camera in [
                Quat::IDENTITY,
                Quat::from_euler(EulerRot::YXZ, 0.7, -0.66, 0.0),
                Quat::from_euler(EulerRot::YXZ, std::f32::consts::PI, -1.0, 0.0),
            ] {
                for roll in [0.0, 1.3] {
                    let world = house.rotation * billboard(wind_frame(Some(&house)), camera, roll);
                    // The quad's normal is the camera's own backward axis.
                    assert!(
                        (world * Vec3::Z).distance(camera * Vec3::Z) < 1e-4,
                        "turned {turn}, roll {roll}"
                    );
                }
            }
        }
    }

    #[test]
    fn embers_are_small_sparse_and_flicker() {
        const { assert!(EMBERS_PER_FORGE <= 8) };
        const { assert!(EMBER_RADIUS * 2.0 <= 0.14 && EMBER_RADIUS * 2.0 >= 0.08) };
        let seeds: Vec<f32> = (0..EMBERS_PER_FORGE)
            .map(|i| i as f32 * 0.618_034)
            .collect();
        let (mut showing, mut frames, mut together) = (0, 0, 0);
        for seed in &seeds {
            let mut scales = Vec::new();
            for frame in 0..600 {
                let seconds = frame as f32 / 60.0;
                let (at, scale) = ember_state(seconds, *seed);
                assert!((0.0..=1.0).contains(&scale), "{scale}");
                assert!(at.y >= 0.0 && at.y <= 1.61, "{}", at.y);
                assert!(at.xz().length() <= 1.21, "{}", at.xz().length());
                scales.push(scale);
            }
            // It sits out part of every cycle, and glints while it flies.
            let dark = scales.iter().filter(|scale| **scale == 0.0).count();
            assert!(dark > 60 && dark < 330, "{dark} of 600 frames dark");
            let jumps = scales
                .windows(2)
                .filter(|pair| pair[0] > 0.0 && (pair[1] - pair[0]).abs() > 0.08)
                .count();
            assert!(jumps > 40, "{jumps} flickers in ten seconds");
        }
        // Never a full, evenly spaced column.
        for frame in 0..600 {
            let seconds = frame as f32 / 60.0;
            let up: Vec<f32> = seeds
                .iter()
                .map(|seed| ember_state(seconds, *seed))
                .filter(|(_, scale)| *scale > 0.15)
                .map(|(at, _)| at.y)
                .collect();
            showing += up.len();
            frames += 1;
            together += usize::from(up.len() == EMBERS_PER_FORGE);
        }
        let mean = showing as f32 / frames as f32;
        assert!(mean > 2.0 && mean < 5.0, "{mean} embers showing on average");
        assert!(together < 12, "all at once in {together} of 600 frames");
    }

    #[test]
    fn smoke_is_pale_by_day_and_a_dim_blue_grey_at_night() {
        let [noon, noon_green, noon_blue] = smoke_shade(1.0);
        let [morning, ..] = smoke_shade(0.5);
        let [sunrise, ..] = smoke_shade(0.25);
        let [midnight, green, blue] = smoke_shade(0.0);
        assert!(noon > 0.7 && noon < 0.8, "a pale grey, short of white");
        assert!((noon - noon_blue).abs() < 0.01 && (noon - noon_green).abs() < 0.01);
        assert!(morning > 0.6, "the working day is not a dusk: {morning}");
        assert!(sunrise > 0.42 && sunrise < morning);
        // Seen against dark ground, and a third of its daytime brightness:
        // nowhere near a lamp (3.3 linear) or a lit pane (4.6).
        assert!(green > 0.2 && green < 0.28, "{green}");
        assert!(green < noon_green * 0.34);
        assert!(blue > midnight * 1.25, "moonlit, not grey");
    }

    #[test]
    fn a_cooking_fire_shows_only_where_smoke_does_not_claim_warmth() {
        // Heat is the simulation's and always shows, wherever the fog allows.
        for (winter, cottage) in [(false, false), (true, false), (false, true), (true, true)] {
            assert!(hearth_smokes(true, false, winter, cottage, true));
            assert!(
                !hearth_smokes(true, true, winter, cottage, false),
                "not through fog"
            );
        }
        // A settlement home with someone in, in a mild season.
        assert!(hearth_smokes(false, true, false, false, true));
        assert!(
            !hearth_smokes(false, false, false, false, true),
            "nobody home"
        );
        // In winter, and at the player's own cottage, smoke means heat only.
        assert!(!hearth_smokes(false, true, true, false, true));
        assert!(!hearth_smokes(false, true, false, true, true));
    }

    #[test]
    fn smoke_leans_with_one_wind_whichever_way_the_house_faces() {
        let downwind = puff_state(0.8, 0.0).offset;

        for turn in [0.0, 1.2, std::f32::consts::PI, -2.0] {
            let house = Transform::from_rotation(Quat::from_rotation_y(turn));
            let world = house.rotation * (wind_frame(Some(&house)) * downwind);
            assert!(world.distance(downwind) < 1e-4, "turned {turn}: {world}");
        }
        assert_eq!(wind_frame(None), Quat::IDENTITY);
    }

    #[test]
    fn each_house_shape_has_its_own_chimney() {
        use crate::island_homes::model::{BodyFrame, HouseModel};
        let frame = BodyFrame::of_cells((0, 0), (3, 4), 2, SimPos::new(2_000, 6_000));
        let cottage = HouseModel::Cottage.fit(&frame).chimney();
        let hall = HouseModel::Hall.fit(&frame).chimney();
        assert_ne!(cottage, hall);
        // On the back wall, away from the door, and above the roof.
        assert!(
            cottage.z > frame.depth * 0.5 - 1.2 && cottage.y > 5.0,
            "{cottage}"
        );
        assert!(hall.y > cottage.y, "the hall is the taller house");
        assert!(COTTAGE_CHIMNEY.y < cottage.y);
    }

    #[test]
    fn a_heated_home_in_winter_smokes_with_a_darker_denser_plume() {
        // Heated in winter: it smokes, and the plume is the snow's.
        assert!(hearth_smokes(true, false, true, false, true));
        assert!(hearth_smokes(true, false, true, true, true));
        let (shade, dense) = plume(true);
        let (summer_shade, summer_dense) = plume(false);
        assert!(shade < summer_shade && dense > summer_dense);
        // Every fade step stays visible and denser than the summer one.
        for step in 0..FADE_STEPS {
            let winter = (fade_alpha(step) * dense).min(0.95);
            assert!(winter >= fade_alpha(step) * summer_dense);
        }
        assert!((fade_alpha(0) * dense).min(0.95) > fade_alpha(0));
        // Unheated in winter, smoke would claim a warmth there is not.
        assert!(!hearth_smokes(false, true, true, false, true));
    }

    #[test]
    fn smoke_follows_heat_and_embers_follow_an_active_craft_without_sim_writes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<SimIndex>()
            .init_resource::<VisualIndex>()
            .init_resource::<Dressed>()
            .insert_resource(SimConfig::default())
            .insert_resource(SimClock { tick: 100 })
            // A quarter of a second a frame (the most virtual time allows in
            // one step): a chimney comes up or dies down in twelve frames.
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(250),
            ))
            .add_systems(Startup, setup)
            .add_systems(Update, (dress, animate).chain());
        let home = Household {
            label: "Home".into(),
            capacity: 2,
            residents: vec![],
            door: npc_framework::prelude::SimPos::new(0, 0),
            heated_until: 500,
            logs_burned: 0,
        };
        let link = |app: &mut App, id: SimId, sim: Entity| {
            let visual = app
                .world_mut()
                .spawn((
                    Transform::default(),
                    Visibility::default(),
                    Fogged(Look::Live),
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
        };
        let home_sim = app.world_mut().spawn((SimId(1), home.clone())).id();
        link(&mut app, SimId(1), home_sim);
        let smithy = app
            .world_mut()
            .spawn((
                SimId(2),
                VillagePlace {
                    role: Role::Smith,
                    label: "Smithy".into(),
                },
            ))
            .id();
        link(&mut app, SimId(2), smithy);
        app.update();
        app.update();
        let visible = |app: &mut App| {
            let world = app.world_mut();
            let chimney = world
                .query::<(&Chimney, &Visibility)>()
                .single(world)
                .map(|(_, v)| *v)
                .unwrap();
            let forge = world
                .query::<(&Forge, &Visibility)>()
                .single(world)
                .map(|(_, v)| *v)
                .unwrap();
            (chimney, forge)
        };
        assert_eq!(
            visible(&mut app),
            (Visibility::Inherited, Visibility::Hidden)
        );
        let level = |app: &mut App| {
            let world = app.world_mut();
            world.query::<&Chimney>().single(world).unwrap().level
        };
        for _ in 0..16 {
            app.update();
        }
        assert_eq!(level(&mut app), 1.0);
        // The fire burns out with nobody home: the column dies down, then goes.
        app.world_mut().resource_mut::<SimClock>().tick = 600;
        app.update();
        assert!(
            level(&mut app) > 0.0 && level(&mut app) < 1.0,
            "it does not snap off"
        );
        assert_eq!(visible(&mut app).0, Visibility::Inherited);
        for _ in 0..16 {
            app.update();
        }
        assert_eq!(level(&mut app), 0.0);
        assert_eq!(visible(&mut app), (Visibility::Hidden, Visibility::Hidden));
        // A resident at the door has a cooking fire going, without any heat.
        let resident = app
            .world_mut()
            .spawn((SimId(3), npc_framework::prelude::SimPos::new(1_000, 0)))
            .id();
        app.world_mut()
            .resource_mut::<SimIndex>()
            .map
            .insert(SimId(3), resident);
        app.world_mut()
            .get_mut::<Household>(home_sim)
            .unwrap()
            .residents
            .push(SimId(3));
        let home = app.world().get::<Household>(home_sim).unwrap().clone();
        app.update();
        assert!(
            level(&mut app) > 0.0 && level(&mut app) < 1.0,
            "it eases in"
        );
        assert_eq!(visible(&mut app).0, Visibility::Inherited);
        for _ in 0..16 {
            app.update();
        }
        assert_eq!(level(&mut app), 1.0);
        assert_eq!(
            app.world_mut().query::<&Puff>().iter(app.world()).count(),
            PUFFS_PER_CHIMNEY + PUFFS_PER_FORGE
        );
        assert_eq!(
            app.world_mut().query::<&Ember>().iter(app.world()).count(),
            EMBERS_PER_FORGE
        );
        assert_eq!(app.world().get::<Household>(home_sim), Some(&home));
        assert!(app.world().get::<Children>(home_sim).is_none());
        assert!(app.world().get::<Children>(smithy).is_none());
    }
}
