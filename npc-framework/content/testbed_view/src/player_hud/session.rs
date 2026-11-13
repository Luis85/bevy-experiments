//! What the island session bar and its companions need from the character
//! view: hand the screen to another mode, reopen the life choices, and keep
//! clear of the toolbar. Presentation only; no mode or actor is changed here.
use super::*;

/// Another mode takes the screen: close the workspace and acknowledge an ended
/// life without requesting a mode. The control coordinator stays the authority.
pub(crate) fn leave_character_view(world: &mut World) {
    if world.contains_resource::<PlayerState>() {
        life::leave(world);
    }
}

/// Reopen the successor / keep-watching choices for an ended life.
pub(crate) fn show_life_choices(world: &mut World) {
    if world.contains_resource::<PlayerState>() {
        life::show_choices(world);
        world.resource_mut::<PlayerState>().dirty = true;
    }
}

/// Distance from the window bottom that clears the hotbar.
pub(crate) fn toolbar_clearance(world: &mut World) -> f32 {
    let toolbar = world
        .query_filtered::<(&Node, &ComputedNode), With<ToolbarRoot>>()
        .iter(world)
        .filter(|(node, _)| node.display != Display::None)
        .map(|(_, size)| size.size().y * size.inverse_scale_factor())
        .find(|height| *height > 0.0)
        .unwrap_or(crate::game_ui::tokens::DOCK);
    2.0 * crate::game_ui::tokens::SCREEN_MARGIN + toolbar
}
