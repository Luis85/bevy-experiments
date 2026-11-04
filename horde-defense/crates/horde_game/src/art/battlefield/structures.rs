//! Traps, walls and turrets with their damage, health and charge readouts.
use super::super::{ArenaProjection, GREEN, TEXT, bar, line, soft_shadow, structure_fallback};
use crate::{
    battle_view::BattleViewModel,
    visual_assets::{self, ArtTextures, Prop, PropMotion},
};
use bevy_egui::egui::{Align2, Color32 as C, FontId, Painter, Rect, vec2};
use horde_core::content::StructureKind;

pub(super) fn paint(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    textures: ArtTextures,
) {
    let sy = arena.lane_height();
    for s in &view.structures {
        let at = arena.world(s.position[0], s.position[1]);
        let pose = &s.pose;
        let direction = arena.world(pose.target[0], pose.target[1]) - at;
        let prop = match s.kind {
            StructureKind::Trap => Prop::Trap,
            StructureKind::Wall => Prop::Wall,
            StructureKind::Turret => Prop::Turret,
        };
        soft_shadow(p, at + vec2(2., sy * 0.29), vec2(sy * 0.48, sy * 0.14), 0.7);
        let tint = if s.kind == StructureKind::Trap && s.charges == 0 {
            C::from_rgb(132, 126, 109)
        } else {
            C::WHITE
        };
        if !visual_assets::prop_animated(
            p,
            textures,
            prop,
            at,
            vec2(arena.yard_width() * s.footprint_width, sy * 0.88),
            tint,
            PropMotion {
                angle: direction.y.atan2(direction.x),
                target: Some(arena.world(pose.target[0], pose.target[1])),
                recoil: pose.recoil,
                trigger: pose.trigger,
                hit: pose.hit,
            },
        ) {
            structure_fallback(
                p,
                at,
                vec2(arena.yard_width() * s.footprint_width, sy * 0.65),
                s.kind,
                direction,
                pose.recoil,
            );
        }
        if s.hp < s.max_hp {
            let damage = 1. - s.hp.max(0) as f32 / s.max_hp as f32;
            for n in 0..3 {
                let start = at + vec2((n as f32 - 1.) * sy * 0.12, -sy * 0.08);
                line(
                    p,
                    start,
                    start + vec2(sy * 0.06, sy * 0.17),
                    1.,
                    C::from_black_alpha((damage * 150.) as u8),
                );
            }
        }
        bar(
            p,
            Rect::from_center_size(at + vec2(0.0, sy * 0.42), vec2(sy * 0.6, 3.0)),
            s.hp as f32 / s.max_hp as f32,
            GREEN,
        );
        if s.kind == StructureKind::Trap {
            p.text(
                at + vec2(0.0, -sy * 0.42),
                Align2::CENTER_CENTER,
                format!("{}", s.charges),
                FontId::monospace(10.0),
                TEXT,
            );
        }
    }
}
