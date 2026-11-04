//! Bodies and live hostiles: sprite, shadow, contact and windup gauges, impact
//! light, health and boss callouts.
use super::super::{AMBER, ArenaProjection, GREEN, RED, bar, figure, line, soft_shadow};
use crate::{
    animation::ActorPose,
    battle_view::{BattleViewModel, EnemyView, HealthView},
    visual_assets::{self, Actor, ArtTextures, HitFlash},
};
use bevy_egui::egui::{self, Align2, Color32 as C, FontId, Painter, Rect, Stroke, vec2};

fn tint(rgba: Option<[f32; 4]>) -> C {
    rgba.map_or(C::WHITE, |v| {
        C::from_rgba_unmultiplied(
            (v[0] * 255.) as u8,
            (v[1] * 255.) as u8,
            (v[2] * 255.) as u8,
            (v[3] * 255.) as u8,
        )
    })
}

pub(super) fn corpses(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    textures: ArtTextures,
) {
    let sy = arena.lane_height();
    let settled_batched = visual_assets::corpse_batch(
        p,
        textures,
        view.corpses
            .iter()
            .filter(|body| body.collapse > 0.9)
            .map(|body| {
                let height = body.height.pixels(sy);
                (
                    arena.world(body.at[0], body.at[1]) + vec2(0., height * 0.28),
                    vec2(height * 1.25, height * 0.45),
                    tint(body.tint).gamma_multiply(0.8),
                )
            }),
    );
    for corpse in &view.corpses {
        if settled_batched && corpse.collapse > 0.9 {
            continue;
        }
        let at = arena.world(corpse.at[0], corpse.at[1]);
        let height = corpse.height.pixels(sy);
        if corpse.has_figure {
            let Some(sprite) = corpse.sprite else {
                continue;
            };
            let pose = ActorPose {
                frame: corpse.frame,
                lean: corpse.lean,
                scale: [1. + corpse.collapse * 0.16, 1. - corpse.collapse * 0.72],
                ..Default::default()
            };
            visual_assets::actor_animated(
                p,
                textures,
                Actor::Enemy(sprite),
                at,
                vec2(height * 1.2, height),
                tint(corpse.tint).gamma_multiply(0.8),
                &pose,
            );
        } else {
            p.add(egui::Shape::ellipse_filled(
                at + vec2(0., sy * 0.25),
                vec2(sy * 0.3, sy * 0.10),
                C::from_black_alpha(90),
            ));
        }
    }
}

pub(super) fn paint(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    textures: ArtTextures,
    outlines: bool,
    reduced: bool,
) {
    for e in &view.enemies {
        enemy(p, arena, view, e, textures, outlines, reduced);
    }
}

fn enemy(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    e: &EnemyView,
    textures: ArtTextures,
    outlines: bool,
    reduced: bool,
) {
    let sy = arena.lane_height();
    let build_left = view.construction_bounds[0];
    let at = arena.world(e.position[0], e.position[1]);
    let pose = &e.pose;
    if let Some(contact) = e.contact {
        let rr = Rect::from_min_max(
            arena.world(1.5, e.lane as f32 + 0.05),
            arena.world(build_left, e.lane as f32 + 0.95),
        );
        p.rect_filled(rr, 0.0, RED.gamma_multiply(0.18));
        line(
            p,
            arena.world(1.52, e.lane as f32 + 0.95),
            arena.world(1.52, e.lane as f32 + 0.95 - contact.ready * 0.9),
            3.0,
            if contact.recovering { GREEN } else { RED },
        );
    }
    let actor = e.sprite.map(Actor::Enemy);
    let has_walk = e.sprite.is_some_and(|s| s.is_walk()) && textures.walkers.is_some();
    let painted_at = arena.world(
        e.position[0] + pose.offset[0],
        e.position[1] + if has_walk { 0. } else { pose.offset[1] },
    );
    let figure_height = e.height.pixels(sy);
    if outlines {
        p.circle_stroke(at, sy * 0.4, Stroke::new(1., AMBER));
    }
    if let Some(progress) = e.windup {
        p.add(egui::Shape::ellipse_stroke(
            at + vec2(0., sy * 0.3),
            vec2(sy * 0.36, sy * 0.13),
            Stroke::new(2., RED),
        ));
        bar(
            p,
            Rect::from_center_size(at + vec2(0., sy * 0.43), vec2(sy * 0.66, 3.)),
            progress,
            RED,
        );
    }
    soft_shadow(
        p,
        at + vec2(1., figure_height * 0.48),
        vec2(sy * 0.26 * (1. + pose.lunge * 0.12), sy * 0.095),
        0.7,
    );
    let drawn = actor.is_some_and(|actor| {
        visual_assets::actor_animated(
            p,
            textures,
            actor,
            painted_at,
            vec2(figure_height * 1.2, figure_height),
            tint(e.tint),
            pose,
        )
    });
    if !drawn {
        figure(
            p,
            painted_at,
            sy * e.fallback_scale,
            e.kind,
            e.boss,
            e.phase,
            false,
        );
    }
    if let (Some(strength), Some(actor)) = (e.hit_flash, actor) {
        visual_assets::actor_hit_flash(
            p,
            textures,
            actor,
            painted_at,
            vec2(figure_height * 1.2, figure_height),
            HitFlash { strength, reduced },
            pose,
        );
    }
    if e.stunned {
        p.circle_stroke(at + vec2(0., -sy * 0.3), sy * 0.16, Stroke::new(1.4, AMBER));
    }
    match e.health {
        HealthView::Critical => {
            p.text(
                at + vec2(0.0, -sy * 0.44),
                Align2::CENTER_BOTTOM,
                "CRIT · FIGHTING",
                FontId::proportional(8.0),
                AMBER,
            );
        }
        HealthView::Bar { fraction, boss } => {
            bar(
                p,
                Rect::from_center_size(
                    at + vec2(0.0, -sy * 0.41),
                    vec2(if boss { sy * 1.1 } else { sy * 0.65 }, 3.0),
                ),
                fraction,
                if boss { AMBER } else { RED },
            );
        }
        HealthView::Hidden => {}
    }
    if let Some(label) = e.label {
        p.text(
            at + vec2(0.0, -sy * 0.7),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(10.0),
            AMBER,
        );
    }
}
