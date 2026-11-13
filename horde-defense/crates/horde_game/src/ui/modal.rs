//! Application modals (design-ux-flow §3 "ui/modal.rs"): one slot,
//! `StatusUi.modal`, drawn over the screen by `draw_screen`. Escape closes
//! the open modal through the router (ui/escape.rs); a backdrop click or
//! "Stay" closes it too. Later rows add their variants here.
use super::{UiCtx, primary_action::primary_in_modal};
use crate::runner::SaveIntent;
use bevy_egui::egui;

/// The open application modal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppModal {
    /// From the main menu: "Save and quit" or "Stay".
    Quit,
    /// From Camp home or Results: "Save and return to main menu" or "Stay".
    ReturnToMenu,
}

/// Which modal is open, without its data: what the Escape router reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModalKind {
    Quit,
    ReturnToMenu,
}

impl AppModal {
    pub(super) fn kind(self) -> ModalKind {
        match self {
            Self::Quit => ModalKind::Quit,
            Self::ReturnToMenu => ModalKind::ReturnToMenu,
        }
    }
    /// Title, body and the modal's primary action.
    pub(super) fn text(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Quit => (
                "Quit Last Light?",
                "Your journey is saved before the game closes.",
                "Save and quit",
            ),
            Self::ReturnToMenu => (
                "Return to the main menu?",
                "Your journey is saved and every field stays paused until you resume.",
                "Save and return to main menu",
            ),
        }
    }
}

impl From<ModalKind> for AppModal {
    fn from(kind: ModalKind) -> Self {
        match kind {
            ModalKind::Quit => Self::Quit,
            ModalKind::ReturnToMenu => Self::ReturnToMenu,
        }
    }
}

/// The label of the button that keeps the player where they are.
pub(super) const STAY: &str = "Stay";

/// Draws the open modal, if any, over the frame.
pub(super) fn show(ctx: &egui::Context, f: &mut UiCtx) {
    let Some(modal) = f.status.modal else {
        return;
    };
    let (title, body, confirm) = modal.text();
    let pending = f.runtime.durable_transition_pending() || f.status.save_pending;
    let shown = egui::Modal::new(egui::Id::new("lastlight_app_modal")).show(ctx, |ui| {
        ui.set_max_width(400.0);
        ui.heading(title);
        ui.label(body);
        ui.add_space(10.0);
        let mut choice = None;
        ui.add_enabled_ui(!pending, |ui| {
            ui.horizontal(|ui| {
                if primary_in_modal(ui, confirm).clicked() {
                    choice = Some(true);
                }
                if ui.button(STAY).clicked() {
                    choice = Some(false);
                }
            });
        });
        choice
    });
    match shown.inner {
        Some(true) => {
            f.status.modal = None;
            match modal {
                AppModal::Quit => f.request_save(SaveIntent::Quit),
                AppModal::ReturnToMenu => f.save_settings_to_menu(),
            }
        }
        Some(false) => f.status.modal = None,
        // A backdrop click stays, as "Stay" does.
        None if shown.should_close() => f.status.modal = None,
        None => {}
    }
}
