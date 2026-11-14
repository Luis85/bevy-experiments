//! Ground, the contact line and the construction grid.
use super::super::{ArenaProjection, GREEN, MUTED, line, paint_ground};
use super::fx::OCCLUDER_GAP;
use crate::{battle_view::BattleViewModel, visual_assets::ArtTextures};
use bevy_egui::egui::{Align2, Color32 as C, FontId, Painter, Pos2, Rect};

/// The construction grid's label right of the last build column.
pub(crate) const MIDPOINT_LABEL: &str = "NO BUILDING BEYOND MIDPOINT";
/// Its type size.
pub(crate) const MIDPOINT_LABEL_SIZE: f32 = 10.0;
/// The narrowest column the label wraps into: its longest word fits, so no
/// word is broken.
pub(crate) const MIDPOINT_LABEL_MIN_WRAP: f32 = 64.0;

/// The midpoint label's left-top anchor and wrap width: 0.2 yd right of the
/// build zone's right edge `build_right` and 0.2 lanes under the field top,
/// on one line. Under an `occluder` (the open construction panel) it wraps
/// into the room left of it, ending [`OCCLUDER_GAP`] short (CF-1-02); where
/// that room is narrower than [`MIDPOINT_LABEL_MIN_WRAP`] the anchor moves
/// left to make it.
pub(crate) fn midpoint_label_place(
    arena: ArenaProjection,
    build_right: f32,
    occluder: Option<Rect>,
) -> (Pos2, f32) {
    let mut at = arena.world(build_right + 0.2, 0.2);
    let Some(occluder) = occluder else {
        return (at, f32::INFINITY);
    };
    let limit = occluder.left() - OCCLUDER_GAP;
    let room = limit - at.x;
    if room >= MIDPOINT_LABEL_MIN_WRAP {
        return (at, room);
    }
    at.x = limit - MIDPOINT_LABEL_MIN_WRAP;
    (at, MIDPOINT_LABEL_MIN_WRAP)
}

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

pub(super) fn construction_grid(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    occluder: Option<Rect>,
) {
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
    let (at, wrap) = midpoint_label_place(arena, build_right, occluder);
    let galley = p.layout(
        MIDPOINT_LABEL.to_owned(),
        FontId::proportional(MIDPOINT_LABEL_SIZE),
        MUTED,
        wrap,
    );
    p.galley(at, galley, MUTED);
}
