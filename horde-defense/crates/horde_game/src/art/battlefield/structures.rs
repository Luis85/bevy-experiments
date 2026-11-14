//! Traps, walls and turrets with their damage, health and charge readouts.
use super::super::{ArenaProjection, GREEN, TEXT, bar, line, soft_shadow, structure_fallback};
use crate::{
    battle_view::BattleViewModel,
    visual_assets::{self, ArtTextures, Prop, PropMotion},
};
use bevy_egui::egui::{Align2, Color32 as C, FontId, Painter, Rect, Vec2, vec2};
use horde_core::content::StructureKind;

fn prop(kind: StructureKind) -> Prop {
    match kind {
        StructureKind::Trap => Prop::Trap,
        StructureKind::Wall => Prop::Wall,
        StructureKind::Turret => Prop::Turret,
    }
}
/// The box a structure's prop is fitted into, centred on its position.
fn prop_size(arena: ArenaProjection, footprint: f32) -> Vec2 {
    vec2(arena.yard_width() * footprint, arena.lane_height() * 0.88)
}
/// The screen rect `paint` draws the prop of a `kind` structure in: at
/// `position` (yards, lanes) with a `footprint` yards wide. The damage popups
/// keep off it (CF-1-07).
pub(crate) fn face(
    arena: ArenaProjection,
    kind: StructureKind,
    position: [f32; 2],
    footprint: f32,
) -> Rect {
    visual_assets::prop_rect(
        prop(kind),
        arena.world(position[0], position[1]),
        prop_size(arena, footprint),
    )
}

pub(super) fn paint(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    textures: ArtTextures,
) {
    let sy = arena.lane_height();
    // Three passes (every shadow, every prop, every readout) keep the
    // battlefield's primitive count independent of how many structures stand
    // or are struck: shapes sharing a texture merge in tessellation,
    // alternating ones do not.
    let prop_size = |footprint: f32| prop_size(arena, footprint);
    for s in &view.structures {
        let at = arena.world(s.position[0], s.position[1]);
        soft_shadow(p, at + vec2(2., sy * 0.29), vec2(sy * 0.48, sy * 0.14), 0.7);
    }
    for s in &view.structures {
        let at = arena.world(s.position[0], s.position[1]);
        let pose = &s.pose;
        let direction = arena.world(pose.target[0], pose.target[1]) - at;
        let tint = if s.kind == StructureKind::Trap && s.charges == 0 {
            C::from_rgb(132, 126, 109)
        } else {
            C::WHITE
        };
        if !visual_assets::prop_animated(
            p,
            textures,
            prop(s.kind),
            at,
            prop_size(s.footprint_width),
            tint,
            PropMotion {
                angle: direction.y.atan2(direction.x),
                target: Some(arena.world(pose.target[0], pose.target[1])),
                recoil: pose.recoil,
                trigger: pose.trigger,
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
    }
    for s in &view.structures {
        let at = arena.world(s.position[0], s.position[1]);
        visual_assets::prop_hit_outline(
            p,
            textures,
            prop(s.kind),
            at,
            prop_size(s.footprint_width),
            s.pose.hit,
        );
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
