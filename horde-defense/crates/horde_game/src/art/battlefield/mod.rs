//! Egui renderer of a `BattleViewModel`, painted back to front: ground and
//! weather, burning ground, construction grid, structures, bodies, hostiles,
//! the survivor rig, thrown ordnance, event effects and impact particles.
mod enemies;
mod fx;
mod ordnance;
mod structures;
pub mod terrain;

use super::{ArenaProjection, RED, soft_shadow};
use crate::{
    animation::{self, SceneCache},
    battle_view::{BattleViewModel, SceneBlend},
    visual_assets::{self, ArtTextures},
};
use bevy::log::warn;
use bevy_egui::egui::{self, Align2, FontId, Painter, Pos2, Rect, Stroke, pos2, vec2};
use horde_core::{content::Mode, sim::Attempt};

pub struct RenderOptions<'a> {
    pub construction: bool,
    pub outlines: bool,
    pub numbers: bool,
    pub reduced: bool,
    pub blood: bool,
    pub theme: usize,
    pub textures: ArtTextures,
    /// Actor position blend between the last two fixed ticks.
    pub blend: SceneBlend<'a>,
}

pub fn battlefield(p: &Painter, r: Rect, a: &Attempt, aim: [f32; 2], options: RenderOptions) {
    let arena = ArenaProjection::for_attempt(r, a);
    let motion = p.ctx().data_mut(|data| {
        let cache =
            data.get_temp_mut_or_default::<SceneCache>(egui::Id::new("lastlight_cosmetic_scenes"));
        if p.layer_id() == egui::LayerId::background() {
            cache.sample(a, options.reduced)
        } else {
            cache.sample_scoped(a, options.reduced, p.layer_id().id.value())
        }
    });
    let view = BattleViewModel::build(a, &motion, options.blend, options.reduced);
    render(p, arena, &view, a, aim, &options);
}

/// `a` is read only where the survivor rig resolves poses from the attempt:
/// the defender's action sample and projectile release origins.
fn render(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    a: &Attempt,
    aim: [f32; 2],
    options: &RenderOptions,
) {
    let textures = options.textures;
    let reduced = options.reduced;
    terrain::paint(p, arena, options.theme, textures);
    fx::ambient(p, arena, &view.particles);
    ordnance::fire_zones(p, arena, view, reduced, textures);
    if options.construction {
        terrain::construction_grid(p, arena, view);
    }
    structures::paint(p, arena, view, textures);
    enemies::corpses(p, arena, view, textures);
    enemies::paint(p, arena, view, textures, options.outlines, reduced);
    let equipment = crate::gear_appearance::from_frozen_gear(&a.profile, &a.gear_snapshot);
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
        },
    );
    fx::particles(p, arena, &view.particles);
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
    soft_shadow(
        p,
        origin + vec2(1., sy * 0.87 * 0.48),
        vec2(sy * 0.30, sy * 0.11),
        0.8,
    );
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
