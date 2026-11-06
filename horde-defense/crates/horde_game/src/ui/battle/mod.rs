//! Battle presentation. Game changes are queued typed intents. Every attempt
//! value laid out here comes from `battle_view::BattleScreen` (or the HUD view
//! model); the attempt itself is only handed to the battlefield painter and
//! the inspector tools.
use super::UiCtx;
use crate::{art, battle_view};
use bevy_egui::egui::{Rect, pos2, vec2};

mod attack;
mod construction;
mod details;
mod field;
mod input;
mod overlays;
#[cfg(test)]
mod tests;

pub(super) use field::battle;
pub(super) use input::{AttackSelection, controls, gather_combat_input};

fn projection(f: &UiCtx, r: Rect) -> art::ArenaProjection {
    let (lanes, width) = battle_view::arena_geometry(f.game);
    art::ArenaProjection::with_geometry(r, lanes, width).with_effects_clip(
        f.battle
            .effects_rect
            .filter(|clip| clip.contains_rect(r))
            .unwrap_or(r),
    )
}
/// A small opening is a genuine smaller clearing at the normal yard scale;
/// growth expands it up to the bounded viewport rather than stretching3rows.
pub(super) fn clearing_rect(maximum: Rect, lanes: u8, width: f32) -> Rect {
    let size = vec2(
        (maximum.width() * (width / 30.0).clamp(0.0, 1.0))
            .max(160.0)
            .min(maximum.width()),
        (maximum.height() * (f32::from(lanes) / 11.0).clamp(0.0, 1.0))
            .max(86.0)
            .min(maximum.height()),
    );
    if size == maximum.size() {
        maximum
    } else {
        Rect::from_center_size(maximum.center(), size).intersect(maximum)
    }
}
fn battle_arena(canvas: Rect, construction: bool) -> Rect {
    Rect::from_min_max(
        pos2(canvas.left() + 24., canvas.top() + 124.),
        pos2(
            canvas.right() - if construction { 276. } else { 24. },
            canvas.bottom() - 84.,
        ),
    )
}
