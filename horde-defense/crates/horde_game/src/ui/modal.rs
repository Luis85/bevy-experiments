//! Application modals (design-ux-flow §3 "ui/modal.rs"): one slot,
//! `StatusUi.modal`, drawn over the screen by `draw_screen`. Escape closes
//! the open modal through the router (ui/escape.rs); a backdrop click or
//! "Stay" closes it too. Later rows add their variants here.
//!
//! A storage error (UX-08, §4 "Storage modal") takes the slot from whatever
//! modal is open: the preempted modal is dropped, not queued.
use super::{StatusUi, UiCtx, primary_action::primary_in_modal};
use crate::{
    art::{MUTED, RED},
    runner::{AppCommand, SaveIntent},
};
use bevy_egui::egui::{self, RichText};

/// The open application modal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AppModal {
    /// From the main menu: "Save and quit" or "Stay".
    Quit,
    /// From Camp home or Results: "Save and return to main menu" or "Stay".
    ReturnToMenu,
    /// A save, load or settings read failed.
    Storage(StorageError),
}

/// What the storage modal reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StorageError {
    /// A save of `intent` failed (any intent, Autosave included).
    Save { intent: SaveIntent, error: String },
    /// The startup load could not read the save; a fresh journey is in
    /// memory. `settings` carries the settings warning of the same startup,
    /// which shares the one slot.
    Load {
        error: String,
        settings: Option<String>,
    },
    /// The startup settings read fell back to defaults
    /// (`SettingsLoad::warning`, which names the kept copy).
    Settings { warning: String },
}

/// Which modal is open, without its data: what the Escape router reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModalKind {
    Quit,
    ReturnToMenu,
    Storage,
}

impl AppModal {
    pub(super) fn kind(&self) -> ModalKind {
        match self {
            Self::Quit => ModalKind::Quit,
            Self::ReturnToMenu => ModalKind::ReturnToMenu,
            Self::Storage(_) => ModalKind::Storage,
        }
    }
    /// The confirmation an Escape press opens. A storage modal is opened by
    /// its error, never by a key.
    pub(super) fn confirmation(kind: ModalKind) -> Option<Self> {
        match kind {
            ModalKind::Quit => Some(Self::Quit),
            ModalKind::ReturnToMenu => Some(Self::ReturnToMenu),
            ModalKind::Storage => None,
        }
    }
}

/// Title, body and primary action of the two confirmations.
fn confirmation_text(quit: bool) -> (&'static str, &'static str, &'static str) {
    if quit {
        (
            "Quit Last Light?",
            "Your journey is saved before the game closes.",
            "Save and quit",
        )
    } else {
        (
            "Return to the main menu?",
            "Your journey is saved and every field stays paused until you resume.",
            "Save and return to main menu",
        )
    }
}

/// The label of the button that keeps the player where they are.
pub(super) const STAY: &str = "Stay";

/// Storage modal titles.
pub(super) const SAVE_FAILED_TITLE: &str = "Save failed";
pub(super) const LOAD_FAILED_TITLE: &str = "Save could not be read";
pub(super) const SETTINGS_FAILED_TITLE: &str = "Settings could not be read";
/// Storage modal actions (design-ux-flow §4 "Storage modal").
pub(super) const RETRY: &str = "Retry";
pub(super) const RETRY_LOAD: &str = "Retry load";
pub(super) const OPEN_FOLDER: &str = "Open folder";
pub(super) const COPY_PATH: &str = "Copy path";
pub(super) const DISMISS: &str = "Dismiss";
pub(super) const CONTINUE_FRESH: &str = "Continue with a fresh journey";
pub(super) const USE_DEFAULTS: &str = "Use defaults";
/// Why "Retry load" is disabled.
const RETRY_LOAD_GONE: &str =
    "The journey changed since the game started, so the save is not read again.";
/// The note after "Copy path".
const PATH_COPIED: &str = "Path copied.";

impl StatusUi {
    /// Opens the storage modal for `error`, replacing whatever modal is open
    /// (the single slot drops the preempted one). The same error arriving
    /// again keeps the modal and its note as they are.
    pub(super) fn open_storage(&mut self, error: StorageError) {
        let modal = AppModal::Storage(error);
        if self.modal.as_ref() != Some(&modal) {
            self.modal = Some(modal);
            self.storage_note.clear();
        }
    }
    /// Closes the open modal: "Stay", "Dismiss", Escape or a backdrop click.
    /// A dismissed save failure is remembered, so the same autosave error
    /// does not reopen the modal every retry until a save succeeds.
    pub(super) fn close_modal(&mut self) {
        match self.modal.take() {
            Some(AppModal::Storage(StorageError::Save { error, .. })) => {
                self.dismissed_save_error = Some(error);
            }
            // The settings warning of the same startup follows the load
            // failure it shared the slot with.
            Some(AppModal::Storage(StorageError::Load {
                settings: Some(warning),
                ..
            })) => self.modal = Some(AppModal::Storage(StorageError::Settings { warning })),
            _ => {}
        }
        self.storage_note.clear();
    }
}

/// The modal a startup storage failure opens: the unreadable save first (its
/// modal carries the settings warning of the same startup), else the settings
/// warning alone.
pub(super) fn startup_modal(
    load_error: Option<String>,
    settings_warning: Option<String>,
) -> Option<AppModal> {
    let error = match (load_error, settings_warning) {
        (Some(error), settings) => StorageError::Load { error, settings },
        (None, Some(warning)) => StorageError::Settings { warning },
        (None, None) => return None,
    };
    Some(AppModal::Storage(error))
}

/// What a click in the storage modal asks for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StorageChoice {
    Retry,
    RetryLoad,
    OpenFolder,
    CopyPath,
    Close,
}

/// Draws the open modal, if any, over the frame.
pub(super) fn show(ctx: &egui::Context, f: &mut UiCtx) {
    match f.status.modal.clone() {
        None => {}
        Some(AppModal::Storage(error)) => show_storage(ctx, f, &error),
        Some(modal) => show_confirmation(ctx, f, modal == AppModal::Quit),
    }
}

fn show_confirmation(ctx: &egui::Context, f: &mut UiCtx, quit: bool) {
    let (title, body, confirm) = confirmation_text(quit);
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
            if quit {
                f.request_save(SaveIntent::Quit);
            } else {
                f.save_settings_to_menu();
            }
        }
        Some(false) => f.status.close_modal(),
        // A backdrop click stays, as "Stay" does.
        None if shown.should_close() => f.status.close_modal(),
        None => {}
    }
}

/// The storage modal: what failed, the save folder (always shown, so it can
/// be copied by hand), and the ways on.
fn show_storage(ctx: &egui::Context, f: &mut UiCtx, error: &StorageError) {
    let directory = f.paths.directory().display().to_string();
    let saving = f.runtime.durable_transition_pending() || f.status.save_pending;
    let reload_allowed = f.runtime.reload_allowed();
    let note = f.status.storage_note.clone();
    let shown = egui::Modal::new(egui::Id::new("lastlight_app_modal")).show(ctx, |ui| {
        ui.set_max_width(440.0);
        let mut choice = None;
        match error {
            StorageError::Save { error, .. } => {
                ui.heading(SAVE_FAILED_TITLE);
                ui.label(format!(
                    "The journey could not be saved: {}.",
                    error.trim_end_matches('.')
                ));
                ui.label("Your progress is still in memory. Retry before closing the game.");
            }
            StorageError::Load { error, settings } => {
                ui.heading(LOAD_FAILED_TITLE);
                ui.label(format!("{}.", error.trim_end_matches('.')));
                ui.label(
                    "A fresh journey is open in memory. The original files are unchanged; \
                     unreadable ones are kept as recovery copies before a new save replaces them.",
                );
                if let Some(warning) = settings {
                    ui.label(warning.as_str());
                }
            }
            StorageError::Settings { warning } => {
                ui.heading(SETTINGS_FAILED_TITLE);
                ui.label(warning.as_str());
            }
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Save folder").color(MUTED).small());
        ui.label(RichText::new(directory.as_str()).monospace());
        if !note.is_empty() {
            let color = if note == PATH_COPIED { MUTED } else { RED };
            ui.label(RichText::new(note.as_str()).color(color));
        }
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| match error {
            StorageError::Save { .. } => {
                ui.add_enabled_ui(!saving, |ui| {
                    if primary_in_modal(ui, RETRY).clicked() {
                        choice = Some(StorageChoice::Retry);
                    }
                });
                folder_buttons(ui, &mut choice);
                if ui.button(DISMISS).clicked() {
                    choice = Some(StorageChoice::Close);
                }
            }
            StorageError::Load { .. } => {
                ui.add_enabled_ui(reload_allowed, |ui| {
                    if primary_in_modal(ui, RETRY_LOAD)
                        .on_disabled_hover_text(RETRY_LOAD_GONE)
                        .clicked()
                    {
                        choice = Some(StorageChoice::RetryLoad);
                    }
                });
                folder_buttons(ui, &mut choice);
                if ui.button(CONTINUE_FRESH).clicked() {
                    choice = Some(StorageChoice::Close);
                }
            }
            StorageError::Settings { .. } => {
                if ui.button(OPEN_FOLDER).clicked() {
                    choice = Some(StorageChoice::OpenFolder);
                }
                if primary_in_modal(ui, USE_DEFAULTS).clicked() {
                    choice = Some(StorageChoice::Close);
                }
            }
        });
        choice
    });
    match shown.inner {
        Some(StorageChoice::Retry) => {
            let StorageError::Save { intent, .. } = error else {
                return;
            };
            f.status.modal = None;
            f.status.storage_note.clear();
            // A failed autosave is retried as the player's own save, so its
            // result is reported like one.
            f.request_save(match intent {
                SaveIntent::Autosave => SaveIntent::Manual,
                intent => *intent,
            });
        }
        Some(StorageChoice::RetryLoad) => f.dispatch(AppCommand::ReloadProfile, ""),
        Some(StorageChoice::OpenFolder) => f.dispatch(AppCommand::OpenSaveFolder, ""),
        Some(StorageChoice::CopyPath) => {
            ctx.copy_text(directory);
            f.status.storage_note = PATH_COPIED.into();
        }
        Some(StorageChoice::Close) => f.status.close_modal(),
        // A backdrop click dismisses, as Escape does.
        None if shown.should_close() => f.status.close_modal(),
        None => {}
    }
}

/// "Open folder" and "Copy path".
fn folder_buttons(ui: &mut egui::Ui, choice: &mut Option<StorageChoice>) {
    if ui.button(OPEN_FOLDER).clicked() {
        *choice = Some(StorageChoice::OpenFolder);
    }
    if ui.button(COPY_PATH).clicked() {
        *choice = Some(StorageChoice::CopyPath);
    }
}
