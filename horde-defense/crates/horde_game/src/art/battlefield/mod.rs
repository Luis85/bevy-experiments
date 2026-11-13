//! Egui renderer of a `BattleViewModel`, painted in passes (W5-01,
//! design-vfx-lighting §2):
//!
//! 1. World: ground and weather, burning ground, blood decals (`decals`,
//!    with the blood setting on), structures, bodies,
//!    hostiles, the survivor rig, thrown ordnance, event effects and impact
//!    particles.
//! 2. Grade: the stage's lighting over-rect across the whole canvas
//!    (`super::lighting`), the forest backdrop included.
//! 3. Lights: the additive light pools of explosions, burning ground and
//!    muzzle flashes (`light`), one mesh, none in reduced effects.
//! 4. Vignette: the canvas edge falloff.
//! 5. Construction grid, while construction is on.
//! 6. Cues: every warning, gauge and label on its INK backing (`cue`), so
//!    no grade or vignette dims a cue.
//!
//! Anything drawn per hostile, per body or per particle is batched into
//! layer meshes (`batch`), so the primitive count does not grow with the
//! crowd: settled bodies (one cached mesh, see `corpses`), collapsing
//! bodies, the hostiles' underlay, one shadow mesh (the rig's included), one
//! sprite mesh on the packed battle atlas, the overlay, callout text and one
//! impact-particle mesh.
mod batch;
mod corpses;
mod cue;
mod decals;
mod elite;
mod enemies;
mod explosion;
mod fx;
mod hits;
mod light;
mod ordnance;
mod structures;
pub(crate) use enemies::body_box as enemy_body_box;
pub(crate) use structures::face as structure_face;
pub mod terrain;
mod weapon_fx;
pub(crate) use enemies::HEALTH_BAR_LIFT;
#[cfg(test)]
pub(crate) use fx::reinforcements_anchor;
pub(crate) use fx::{OCCLUDER_GAP, fx_lifetime};
#[cfg(test)]
pub(crate) use terrain::{
    MIDPOINT_LABEL, MIDPOINT_LABEL_MIN_WRAP, MIDPOINT_LABEL_SIZE, midpoint_label_place,
};
pub use weapon_fx::{FxSource, shooter_class};
#[cfg(test)]
mod tests;

use super::{ArenaProjection, RED, SceneLook, SkyClock, backdrop, lighting, soft_shadow};
use crate::{
    animation::{self, SceneAnimation, SceneCache},
    battle_view::{BattleViewModel, SceneBlend},
    diagnostics::SceneDraw,
    visual_assets::{self, ArtTextures},
};
use bevy::log::warn;
use bevy_egui::egui::{self, Align2, FontId, Painter, Pos2, Rect, Vec2, pos2, vec2};
use horde_core::sim::Attempt;

pub struct RenderOptions {
    pub construction: bool,
    pub outlines: bool,
    pub numbers: bool,
    pub reduced: bool,
    pub blood: bool,
    /// Backdrop theme, lighting grade and storm tag of the stage
    /// ([`super::scene_look`]).
    pub scene: SceneLook,
    /// The lightning clock of this frame (wall time over unpaused frames
    /// and the observation speed; W5-02). Only a storm stage reads it.
    pub sky: SkyClock,
    /// The whole canvas this field sits on (the window, or an observer's
    /// box): the grade and the vignette cover it, clipped to it.
    pub canvas: Rect,
    pub textures: ArtTextures,
    /// An opaque overlay painted over the field after it (the main battle's
    /// open construction panel; `None` for the observer and with the panel
    /// closed). The field callout that is the visual twin of an audio
    /// warning (REINFORCEMENTS) moves left of it.
    pub occluder: Option<Rect>,
}

/// The cosmetic scene animation of `a` for a painter on `layer`, sampled from
/// the context's scene cache. The root background layer shares one unscoped
/// history; every other layer (an observer window) keeps its own.
pub fn sample_motion(
    ctx: &egui::Context,
    layer: egui::LayerId,
    a: &Attempt,
    reduced: bool,
) -> SceneAnimation {
    ctx.data_mut(|data| {
        let cache =
            data.get_temp_mut_or_default::<SceneCache>(egui::Id::new("lastlight_cosmetic_scenes"));
        if layer == egui::LayerId::background() {
            cache.sample(a, reduced)
        } else {
            cache.sample_scoped(a, reduced, layer.id.value())
        }
    })
}

/// Builds the field view of `a` for the painter's layer, blending actor
/// positions by `blend`, then paints it (see [`battlefield`]). Each call is
/// one `BattleViewModel` build: the operations observer, once per visible
/// worker. The main battle builds its view once per pass in `BattleFrame`.
pub fn battlefield_for_attempt(
    p: &Painter,
    r: Rect,
    a: &Attempt,
    aim: [f32; 2],
    blend: SceneBlend,
    options: RenderOptions,
) -> SceneDraw {
    let motion = sample_motion(p.ctx(), p.layer_id(), a, options.reduced);
    let view = BattleViewModel::build(a, motion, blend, options.reduced);
    battlefield(p, r, &view, a, aim, options)
}

/// Paints the field `view` of `a` and returns what it drew, for the perf
/// report's `scene_load`: the settled bodies drawn from the cached mesh and
/// the collapsing bodies sampled this pass.
pub fn battlefield(
    p: &Painter,
    r: Rect,
    view: &BattleViewModel,
    a: &Attempt,
    aim: [f32; 2],
    options: RenderOptions,
) -> SceneDraw {
    let arena = ArenaProjection::for_attempt(r, a);
    let hostiles = render(p, arena, view, a, aim, &options);
    SceneDraw {
        settled_corpses: view.settled.count,
        collapsing_corpses: view.corpses.len(),
        particles: view.particles.len(),
        fx_views: view.fx.len(),
        flashing_enemies: hostiles.flashing,
        flash_quads: hostiles.flash_quads,
    }
}

/// The flashes of one frame, as the motion manifest records them
/// (design-qa-captures 1.9, W5-10). All three are zero in reduced effects,
/// which draws no sky flash, no muzzle flash and no light pool.
#[cfg(feature = "review")]
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
pub struct FlashFacts {
    /// The largest alpha among the frame's flash shapes: the sky-flash
    /// over-rect, every muzzle flash polygon and the centre of every
    /// additive light pool (its strength after the grade's light gain).
    pub flash_alpha_max: f32,
    /// The alpha of the storm sky-flash over-rect (0 without a pulse).
    pub sky_flash: f32,
    /// The muzzle flash polygons drawn: one per volley inside its flash life.
    pub muzzle_flash_count: u32,
}

/// The [`FlashFacts`] of the frame the painter draws of `a` on the lightning
/// clock `sky`: the same rules [`render`] paints with (`lighting::sky_pulse`
/// and `scene_grade`, `weapon_fx::muzzle_flash_alpha`, `light::collect`),
/// on the attempt's own tick without frame blending. Alphas do not depend
/// on the window size, so the field is projected on a nominal rect.
#[cfg(feature = "review")]
pub fn flash_facts(a: &Attempt, sky: SkyClock, reduced: bool) -> FlashFacts {
    let view = BattleViewModel::build(
        a,
        SceneAnimation::sample(a, reduced),
        SceneBlend::NONE,
        reduced,
    );
    let arena = ArenaProjection::for_attempt(
        Rect::from_min_size(pos2(24.0, 124.0), vec2(1392.0, 692.0)),
        a,
    );
    let textures = ArtTextures::default();
    let muzzle = Some(arena.world(view.defender.position[0], view.defender.position[1]));
    let look = super::scene_look(a);
    let (grade, pulse) =
        lighting::scene_grade(look, reduced, lighting::sky_pulse(look, sky, a, reduced));
    let sky_flash = lighting::sky_alpha(pulse);
    let muzzle_flashes: Vec<f32> = view
        .fx
        .iter()
        .filter(|e| e.kind == horde_core::sim::EventKind::Shot)
        .filter_map(|e| {
            let from = fx::shot_origin(arena, &view, e, muzzle, textures)?;
            let alpha = weapon_fx::muzzle_flash_alpha(
                weapon_fx::weapon_fx(e.source?),
                fx::shot_age_ticks(e),
                e.volley.unwrap_or(crate::battle_view::Volley::SINGLE),
                reduced,
            )?;
            (arena.world(e.to[0], e.to[1]) != from).then_some(alpha)
        })
        .collect();
    let pools = light::collect(arena, &view, muzzle, textures, reduced);
    let lights = pools
        .iter()
        .map(|pool| (pool.intensity * grade.light_gain).clamp(0.0, 1.0));
    FlashFacts {
        flash_alpha_max: muzzle_flashes
            .iter()
            .copied()
            .chain(lights)
            .fold(sky_flash, f32::max),
        sky_flash,
        muzzle_flash_count: muzzle_flashes.len() as u32,
    }
}

/// What one frame draws of each budgeted effect class
/// (`perf_budget::FX_BUDGET`), counted by the collectors the painter itself
/// calls.
#[cfg(all(test, feature = "review"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FxCounts {
    pub lights: usize,
    pub blood_decals: usize,
    pub scorch: usize,
    pub explosions: usize,
    pub hit_events: usize,
    pub fire_zones: usize,
    pub flames_per_zone: usize,
    pub elite_tells: usize,
    pub particles: usize,
}
#[cfg(all(test, feature = "review"))]
impl FxCounts {
    /// Each count with its `FX_BUDGET` row.
    pub(crate) fn rows(&self) -> [(&'static str, usize); 9] {
        [
            ("lights", self.lights),
            ("decals", self.blood_decals),
            ("scorch", self.scorch),
            ("explosions_concurrent", self.explosions),
            ("hit_events", self.hit_events),
            ("fire_zones_drawn", self.fire_zones),
            ("flames_per_zone", self.flames_per_zone),
            ("elite_tells", self.elite_tells),
            ("particles", self.particles),
        ]
    }
}
/// The effect counts of the frame `battlefield` paints of `view` in `r`.
#[cfg(all(test, feature = "review"))]
pub(crate) fn fx_counts(
    r: Rect,
    view: &BattleViewModel,
    a: &Attempt,
    blood: bool,
    reduced: bool,
) -> FxCounts {
    let arena = ArenaProjection::for_attempt(r, a);
    let muzzle = arena.world(view.defender.position[0], view.defender.position[1]);
    FxCounts {
        lights: light::collect(arena, view, Some(muzzle), Default::default(), reduced).len(),
        blood_decals: decals::decals(a, blood, reduced).len(),
        scorch: view.scorch.len(),
        explosions: explosion::blasts(arena, view).len(),
        hit_events: hits::drawn(view).count(),
        fire_zones: ordnance::burning(view).len(),
        flames_per_zone: ordnance::flames_per_zone(reduced),
        elite_tells: elite::drawn(view)
            .into_iter()
            .filter(|drawn| *drawn)
            .count(),
        particles: view.particles.len(),
    }
}

/// `a` is read only where the survivor rig resolves poses from the attempt:
/// the defender's action sample and projectile release origins. Returns what
/// the hostiles' layers drew.
fn render(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    a: &Attempt,
    aim: [f32; 2],
    options: &RenderOptions,
) -> enemies::HostilesDrawn {
    let textures = options.textures;
    let reduced = options.reduced;
    let fx_options = fx::FxOptions {
        numbers: options.numbers,
        reduced,
        textures,
        occluder: options.occluder,
    };
    let mut batch = cue::CueBatch::new(view.enemies.len());
    // 1. World pass.
    terrain::paint(p, arena, options.scene.theme_index, textures);
    fx::ambient(p, arena, &view.particles);
    explosion::scorch(p, arena, view, reduced);
    ordnance::fire_zones(p, arena, view, reduced, textures);
    decals::paint(p, arena, a, options.blood, reduced);
    structures::paint(p, arena, view, textures);
    corpses::paint(p, arena, view, a, textures, options.blood);
    let rig_shadow = rig_shadow(arena, view);
    let hostiles = enemies::bodies(p, arena, view, textures, reduced, rig_shadow);
    let equipment = crate::gear_appearance::from_frozen_gear(&a.profile, &a.gear_snapshot);
    if !hostiles.rig_shadow {
        soft_shadow(p, rig_shadow.at, rig_shadow.size, rig_shadow.alpha);
    }
    let muzzle = defender(p, arena, view, a, aim, reduced, equipment.as_ref());
    ordnance::thrown(p, arena, view, a, reduced, textures, equipment.as_ref());
    explosion::paint(p, arena, view, reduced, textures);
    fx::events(p, arena, view, muzzle, &fx_options);
    fx::particles(p, arena, &view.particles);
    // 2. Grade, over the whole canvas. The sky pulse is the
    // wall-clock lightning of a storm stage: under the vignette and every
    // cue, and silent near a boss or attack cue.
    // (`with_clip_rect` would intersect with the arena clip `p` carries.)
    let mut canvas = p.clone();
    canvas.set_clip_rect(options.canvas);
    let pulse = lighting::sky_pulse(options.scene, options.sky, a, reduced);
    let (grade, sky) = lighting::scene_grade(options.scene, reduced, pulse);
    lighting::paint_grade(&canvas, options.canvas, grade, sky);
    // 3. Lights: additive pools on the field, over the grade so a dark
    // stage shows them strongest (the grade's light gain).
    let pools = light::collect(arena, view, muzzle, textures, reduced);
    light::paint_pools(p, &pools, grade.light_gain);
    // 4. Vignette.
    backdrop::edge_shade(&canvas, options.canvas);
    // 5. Construction grid.
    if options.construction {
        terrain::construction_grid(p, arena, view, options.occluder);
    }
    // 6. Cue pass.
    collect_cues(&mut batch, arena, view, options, &fx_options);
    batch.paint(p);
    hostiles
}

/// Every cue of the frame, in paint order: the hostiles' gauges and labels,
/// the event cues, then the survivor's hit ring.
fn collect_cues(
    batch: &mut cue::CueBatch,
    arena: ArenaProjection,
    view: &BattleViewModel,
    options: &RenderOptions,
    fx_options: &fx::FxOptions,
) {
    enemies::cues(batch, arena, view, options.outlines, options.reduced);
    fx::event_cues(batch, arena, view, fx_options);
    let origin = arena.world(view.defender.position[0], view.defender.position[1]);
    defender_feedback(
        batch,
        origin,
        arena.lane_height(),
        &view.defender.pose,
        options.reduced,
    );
}

/// The survivor rig's soft shadow, drawn in the hostiles' shadow layer.
fn rig_shadow(arena: ArenaProjection, view: &BattleViewModel) -> enemies::RigShadow {
    let sy = arena.lane_height();
    let origin = arena.world(view.defender.position[0], view.defender.position[1]);
    enemies::RigShadow {
        at: origin + vec2(1., sy * 0.87 * 0.48),
        size: vec2(sy * 0.30, sy * 0.11),
        alpha: 0.8,
    }
}

/// Draws the survivor rig and returns its muzzle socket when it resolved.
fn defender(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    a: &Attempt,
    aim: [f32; 2],
    reduced: bool,
    equipment: &horde_core::survivors::EquipmentVisual,
) -> Option<Pos2> {
    let sy = arena.lane_height();
    let origin = arena.world(view.defender.position[0], view.defender.position[1]);
    // The appearance stamped at deploy (ruling D17): Mara's Npc deploy stamps
    // Female, a recruit's seat stamps its identity-table appearance.
    let appearance = a.defender_appearance();
    let manual_target = arena.world(aim[0], aim[1]);
    let mut player_sample = crate::defender_presentation::sample(
        a,
        &view.defender.pose,
        [manual_target.x, manual_target.y],
        crate::defender_presentation::brace_at(a, a.tick),
        reduced,
    );
    let target = crate::defender_presentation::target_world(
        a,
        &view.defender.pose,
        player_sample.action,
        aim,
    );
    let target = arena.world(target[0], target[1]);
    player_sample.aim = [target.x, target.y];
    let player_pose = visual_assets::player_model_id(appearance).and_then(|model| {
        visual_assets::resolve_player_model(
            &model,
            equipment,
            player_sample,
            origin,
            vec2(sy * 0.82, sy * 0.87),
        )
    });
    let player_pose = match player_pose {
        Ok(pose) => match visual_assets::draw_player_model(p, &pose) {
            Ok(()) => Some(pose),
            Err(error) => {
                player_model_error(p, origin, &error);
                None
            }
        },
        Err(error) => {
            player_model_error(p, origin, &error);
            None
        }
    };
    player_pose.map(|pose| pos2(pose.muzzle[0], pose.muzzle[1]))
}

fn player_model_error(p: &Painter, origin: Pos2, error: &str) {
    let key = egui::Id::new(("lastlight.player_model.error", p.layer_id().id));
    let changed = p.ctx().data_mut(|data| {
        if data.get_temp::<String>(key).as_deref() == Some(error) {
            false
        } else {
            data.insert_temp(key, error.to_owned());
            true
        }
    });
    if changed {
        warn!("Survivor artwork unavailable: {error}");
    }
    p.text(
        origin,
        Align2::CENTER_CENTER,
        "Survivor art unavailable",
        FontId::proportional(10.),
        RED,
    );
}

/// The survivor's hit ring, the visual twin of the hurt sound.
fn defender_feedback(
    batch: &mut cue::CueBatch,
    origin: Pos2,
    lane_height: f32,
    pose: &animation::DefenderPose,
    reduced: bool,
) {
    if pose.hit > 0. {
        batch.ellipse(
            origin,
            Vec2::splat(lane_height * 0.52),
            if reduced { 1. } else { 1.5 },
            RED.gamma_multiply(pose.hit),
        );
    }
}
