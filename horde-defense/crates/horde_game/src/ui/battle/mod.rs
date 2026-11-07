//! Battle presentation. Game changes are queued typed intents. Every attempt
//! value laid out here comes from `battle_view::BattleScreen` (or the HUD view
//! model), built once per egui pass in a `BattleFrame`; the attempt itself is
//! only handed to the battlefield painter and the inspector tools.
use super::UiCtx;
use crate::{art, battle_view};
use bevy_egui::egui::Rect;

mod attack;
mod construction;
mod details;
mod field;
mod frame;
mod input;
mod layout;
mod overlays;
#[cfg(test)]
mod tests;

pub(super) use field::battle;
pub(super) use frame::BattleFrame;
pub(super) use input::{AttackSelection, controls, gather_combat_input};
pub(in crate::ui) use layout::BattleLayout;
#[cfg(test)]
pub(in crate::ui) use layout::PANEL_FILL_ALPHA;

fn projection(f: &UiCtx, r: Rect) -> art::ArenaProjection {
    let (lanes, width) = battle_view::arena_geometry(f.game);
    art::ArenaProjection::with_geometry(r, lanes, width).with_effects_clip(
        f.battle
            .effects_rect
            .filter(|clip| clip.contains_rect(r))
            .unwrap_or(r),
    )
}
