//! Ground, the contact line and the construction grid.
use super::super::{ArenaProjection, GREEN, MUTED, line, paint_ground};
use crate::{battle_view::BattleViewModel, visual_assets::ArtTextures};
use bevy_egui::egui::{Align2, Color32 as C, FontId, Painter, Rect};

pub fn paint(p: &Painter, arena: ArenaProjection, theme: usize, textures: ArtTextures) {
    let r = arena.rect;
    paint_ground(p, r, None, theme, textures);
    // A faded chalk line joins all eleven contact positions. Lane beds and
    // construction cells appear only when the player needs those coordinates.
    line(
        p,
        arena.world(1.5, 0.),
        arena.world(1.5, f32::from(arena.lanes)),
        1.,
        C::from_rgba_unmultiplied(205, 184, 119, 100),
    );
}

pub(super) fn construction_grid(p: &Painter, arena: ArenaProjection, view: &BattleViewModel) {
    let [build_left, build_right] = view.construction_bounds;
    let cell_width = view.construction_cell_width;
    p.rect_filled(
        Rect::from_min_max(
            arena.world(build_left, 0.0),
            arena.world(build_right, f32::from(arena.lanes)),
        ),
        0.0,
        C::from_rgba_unmultiplied(146, 195, 145, 12),
    );
    for column in 0..=5 {
        let x = build_left + column as f32 * cell_width;
        line(
            p,
            arena.world(x, 0.0),
            arena.world(x, f32::from(arena.lanes)),
            1.0,
            GREEN.gamma_multiply(0.45),
        );
    }
    for lane in 0..=arena.lanes {
        line(
            p,
            arena.world(build_left, lane as f32),
            arena.world(build_right, lane as f32),
            1.0,
            GREEN.gamma_multiply(0.35),
        );
    }
    for lane in 0..arena.lanes {
        p.text(
            arena.world(build_left - 0.2, lane as f32 + 0.5),
            Align2::RIGHT_CENTER,
            format!("{:02}", lane + 1),
            FontId::monospace(10.),
            MUTED,
        );
    }
    p.text(
        arena.world(build_right + 0.2, 0.2),
        Align2::LEFT_TOP,
        "NO BUILDING BEYOND MIDPOINT",
        FontId::proportional(10.0),
        MUTED,
    );
}
