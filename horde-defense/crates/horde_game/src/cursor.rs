//! Native cursor visibility is explicit: egui 0.43 maps CursorIcon::None to a
//! default system icon, so that hint alone cannot hide the OS cursor.
use bevy::{
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
pub(crate) fn visible(custom_requested: bool, focused: bool, inside_window: bool) -> bool {
    !(custom_requested && focused && inside_window)
}
pub fn sync_native_cursor(
    custom: Res<crate::ui::CustomCursor>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    for (window, mut options) in &mut windows {
        options.visible = visible(custom.0, window.focused, window.cursor_position().is_some());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_pointer_is_hidden_only_for_focused_arena_ownership_and_restored_everywhere_else() {
        assert!(!visible(true, true, true));
        for state in [
            (false, true, true),
            (true, false, true),
            (true, true, false),
            (false, false, false),
        ] {
            assert!(visible(state.0, state.1, state.2));
        }
    }
}
