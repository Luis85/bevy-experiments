//! Egui renderer of a `BattleViewModel`, painted back to front: ground and
//! weather, burning ground, construction grid, structures, bodies, hostiles,
//! the survivor rig, thrown ordnance, event effects and impact particles.
//!
//! Anything drawn per hostile, per body or per particle is batched into
//! layer meshes (`batch`), so the primitive count does not grow with the
//! crowd: settled bodies (one cached mesh, see `corpses`), collapsing
//! bodies, the hostiles' underlay, one shadow mesh (the rig's included), one
//! sprite mesh on the packed battle atlas, the overlay, callout text and one
//! impact-particle mesh.
mod batch;
mod corpses;
mod enemies;
mod fx;
mod ordnance;
mod structures;
pub mod terrain;
pub(crate) use enemies::HEALTH_BAR_LIFT;
pub(crate) use fx::fx_lifetime;
#[cfg(test)]
pub(crate) use fx::{OCCLUDER_GAP, boss_banner_rect, reinforcements_anchor};
#[cfg(test)]
mod tests;

use super::{ArenaProjection, RED, soft_shadow};
use crate::{
    animation::{self, SceneAnimation, SceneCache},
    battle_view::{BattleViewModel, SceneBlend},
    diagnostics::SceneDraw,
    visual_assets::{self, ArtTextures},
};
use bevy::log::warn;
use bevy_egui::egui::{self, Align2, FontId, Painter, Pos2, Rect, Stroke, pos2, vec2};
use horde_core::{content::Mode, sim::Attempt};

pub struct RenderOptions {
    pub construction: bool,
    pub outlines: bool,
    pub numbers: bool,
    pub reduced: bool,
    pub blood: bool,
    pub theme: usize,
    pub textures: ArtTextures,
    /// An opaque overlay painted over the field after it (the main battle's
    /// open construction panel; `None` for the observer and with the panel
    /// closed). Field callouts that are the visual twin of an audio warning
    /// (BOSS APPROACHING, REINFORCEMENTS) move left of it.
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
    terrain::paint(p, arena, options.theme, textures);
    fx::ambient(p, arena, &view.particles);
    ordnance::fire_zones(p, arena, view, reduced, textures);
    if options.construction {
        terrain::construction_grid(p, arena, view);
    }
    structures::paint(p, arena, view, textures);
    corpses::paint(p, arena, view, a, textures, options.blood);
    let rig_shadow = rig_shadow(arena, view);
    let hostiles = enemies::paint(
        p,
        arena,
        view,
        textures,
        options.outlines,
        reduced,
        rig_shadow,
    );
    let equipment = crate::gear_appearance::from_frozen_gear(&a.profile, &a.gear_snapshot);
    if !hostiles.rig_shadow {
        soft_shadow(p, rig_shadow.at, rig_shadow.size, rig_shadow.alpha);
    }
    let muzzle = defender(p, arena, view, a, aim, reduced, equipment.as_ref());
    ordnance::thrown(p, arena, view, a, reduced, textures, equipment.as_ref());
    fx::events(
        p,
        arena,
        view,
        muzzle,
        &fx::FxOptions {
            numbers: options.numbers,
            reduced,
            blood: options.blood,
            textures,
            occluder: options.occluder,
        },
    );
    fx::particles(p, arena, &view.particles);
    hostiles
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
    let appearance = if a.mode == Mode::Npc {
        horde_core::survivors::Appearance::Female
    } else {
        a.profile.appearance
    };
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
    paint_defender_feedback(p, origin, sy, &view.defender.pose, reduced);
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

fn paint_defender_feedback(
    p: &Painter,
    origin: Pos2,
    lane_height: f32,
    pose: &animation::DefenderPose,
    reduced: bool,
) {
    if pose.hit > 0. {
        p.circle_stroke(
            origin,
            lane_height * 0.52,
            Stroke::new(if reduced { 1. } else { 1.5 }, RED.gamma_multiply(pose.hit)),
        );
    }
}
