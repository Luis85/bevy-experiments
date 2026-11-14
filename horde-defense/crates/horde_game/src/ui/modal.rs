//! Application modals (design-ux-flow §3 "ui/modal.rs"): one slot,
//! `StatusUi.modal`, drawn over the screen by `draw_screen`. Escape closes
//! the open modal through the router (ui/escape.rs); a backdrop click or
//! "Stay" closes it too. Later rows add their variants here.
//!
//! A storage error (UX-08, §4 "Storage modal") takes the slot from whatever
//! modal is open: the preempted modal is dropped, not queued.
//!
//! The draft warning (UX-09, §4 "Settings") opens when a Settings leave
//! route meets an unapplied difficulty draft: "Apply and return", "Discard
//! and return" or "Keep editing" (which Escape and a backdrop click also do).
//!
//! The Recruit modal (UX-18, §4 "Recruit modal") has two phases. Choose
//! shows the core offer (`Game::recruit_offer`: name, origin trait, price and
//! two archetypes) and dispatches `Command::RecruitAs`; once the command
//! answers with the recruit's id the notice system moves it to Joined, whose
//! primary is "Back to camp".
//!
//! The stash apply (W3-21, design-loot-economy §9) confirms the Equipment
//! Auto-sell panel's "Apply rule to stash now…": it carries the quote it
//! shows and dispatches `Command::ApplyAutoSellToStash` for exactly that
//! quote. The notice system closes it on the sale and re-quotes it when core
//! refuses a stale quote.
use super::{
    HomeFocus, StatusUi, UiCtx, chrome, muted, preferences::DraftState,
    primary_action::primary_in_modal,
};
use crate::{
    art::{AMBER, MUTED, RED},
    runner::{AppCommand, SaveIntent},
};
use bevy_egui::egui::{self, RichText};
use horde_core::{
    command::Command, defenders::RecruitOffer, quality::Quality, sim::AutoSellPreview,
};

/// The open application modal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AppModal {
    /// From the main menu: "Save and quit" or "Stay".
    Quit,
    /// From Camp home or Results: "Save and return to main menu" or "Stay".
    ReturnToMenu,
    /// A save, load or settings read failed.
    Storage(StorageError),
    /// Leaving Settings by `then` would drop an unapplied difficulty draft.
    DraftWarning { then: LeaveSettings },
    /// Recruiting a survivor from liberated `country`.
    Recruit {
        country: String,
        phase: RecruitPhase,
    },
    /// "Apply rule to stash now…": sell `quote` from the stash, or keep it.
    ApplyAutoSell {
        /// What the modal shows and its primary confirms.
        quote: AutoSellPreview,
        /// The confirmation is with the runner; the primary waits for it.
        pending: bool,
        /// Core refused a stale quote and this is the new one.
        requoted: bool,
    },
}

/// Where the Recruit modal stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecruitPhase {
    /// The offer's two archetypes wait for a choice.
    Choose,
    /// `Command::RecruitAs` answered with this recruit's id.
    Joined { recruit_id: u64 },
}

/// The Settings leave route a draft warning interrupted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeaveSettings {
    /// Save and return to the screen Settings was opened from (the amber
    /// primary and Escape).
    Back,
    /// "Save and return to main menu".
    Menu,
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
    DraftWarning,
    Recruit,
    ApplyAutoSell,
}

impl AppModal {
    pub(super) fn kind(&self) -> ModalKind {
        match self {
            Self::Quit => ModalKind::Quit,
            Self::ReturnToMenu => ModalKind::ReturnToMenu,
            Self::Storage(_) => ModalKind::Storage,
            Self::DraftWarning { .. } => ModalKind::DraftWarning,
            Self::Recruit { .. } => ModalKind::Recruit,
            Self::ApplyAutoSell { .. } => ModalKind::ApplyAutoSell,
        }
    }
    /// The stash-apply confirmation on a fresh `quote`.
    pub(super) fn auto_sell(quote: AutoSellPreview) -> Self {
        Self::ApplyAutoSell {
            quote,
            pending: false,
            requoted: false,
        }
    }
    /// The confirmation an Escape press opens. A storage modal is opened by
    /// its error, the Recruit modal and the stash apply by their buttons,
    /// never by a key.
    /// Escape leaves Settings by its Back route.
    pub(super) fn confirmation(kind: ModalKind) -> Option<Self> {
        match kind {
            ModalKind::Quit => Some(Self::Quit),
            ModalKind::ReturnToMenu => Some(Self::ReturnToMenu),
            ModalKind::Storage | ModalKind::Recruit | ModalKind::ApplyAutoSell => None,
            ModalKind::DraftWarning => Some(Self::DraftWarning {
                then: LeaveSettings::Back,
            }),
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

/// The draft warning (design-ux-flow §4 "Settings").
pub(super) const DRAFT_WARNING_TITLE: &str = "Unapplied difficulty changes";
pub(super) const APPLY_AND_RETURN: &str = "Apply and return";
pub(super) const DISCARD_AND_RETURN: &str = "Discard and return";
pub(super) const KEEP_EDITING: &str = "Keep editing";
/// Why "Apply and return" is disabled; the Settings Apply button says the
/// same.
pub(super) const FIX_DRAFT_FIRST: &str = "Fix the highlighted value first.";

/// The Recruit modal (design-ux-flow §4 "Recruit modal").
pub(super) const NOT_NOW: &str = "Not now";
pub(super) const BACK_TO_CAMP: &str = "Back to camp";
pub(super) const STAY_HERE: &str = "Stay here";
pub(super) const CLOSE: &str = "Close";

/// The stash apply (design-loot-economy §9 "Modal").
const AUTO_SELL_KEPT: &str =
    "Locked, favourite, equipped, preset, recruit and shop items are kept.";
const AUTO_SELL_REQUOTED: &str = "The stash changed. This is the new quote.";
pub(super) const KEEP_ITEMS: &str = "Keep items";

/// "Sell N items for C coins?": the stash apply's question.
fn auto_sell_title(quote: &AutoSellPreview) -> String {
    format!(
        "Sell {} for {} coins?",
        chrome::count_noun(quote.count as usize, "item", "items"),
        quote.coins
    )
}
/// "Sell N · +C": the stash apply's amber primary.
fn auto_sell_confirm(quote: &AutoSellPreview) -> String {
    format!("Sell {} · +{}", quote.count, quote.coins)
}
/// "Sold N · +C": the toast of a finished stash apply.
pub(super) fn auto_sold_toast(count: u32, coins: u64) -> String {
    format!("Sold {count} · +{coins}")
}

/// "Recruit as <Archetype> · N coins": a Choose-phase button, at the offer's
/// core price.
fn recruit_as_label(offer: &RecruitOffer, index: usize) -> String {
    format!(
        "Recruit as {} · {} coins",
        offer.archetypes[index].archetype.label(),
        offer.price
    )
}

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
    /// Opens the Recruit modal on the offer of liberated `country`. Results,
    /// Home's offers and Operations all recruit through it.
    pub(super) fn open_recruit(&mut self, country: &str) {
        self.modal = Some(AppModal::Recruit {
            country: country.into(),
            phase: RecruitPhase::Choose,
        });
    }
    /// `Command::RecruitAs` for `country` answered with `recruit_id`: the
    /// Recruit modal still choosing for that country shows who joined.
    pub(super) fn recruit_joined(&mut self, country: &str, recruit_id: u64) {
        if let Some(AppModal::Recruit {
            country: open,
            phase,
        }) = &mut self.modal
            && open == country
            && *phase == RecruitPhase::Choose
        {
            *phase = RecruitPhase::Joined { recruit_id };
        }
    }
    /// `Command::ApplyAutoSellToStash` sold its batch: the stash apply is
    /// done.
    pub(super) fn auto_sell_sold(&mut self) {
        if matches!(self.modal, Some(AppModal::ApplyAutoSell { .. })) {
            self.modal = None;
        }
    }
    /// Core refused the stash apply (a stale quote, a full wallet, a defense
    /// that began meanwhile): the open modal shows `fresh`, the quote core
    /// would accept now, and says so when it differs; with nothing left to
    /// sell it closes. The status line carries core's reason either way.
    pub(super) fn auto_sell_refused(&mut self, fresh: AutoSellPreview) {
        let Some(AppModal::ApplyAutoSell { quote, .. }) = &self.modal else {
            return;
        };
        let requoted = (fresh.count, fresh.coins) != (quote.count, quote.coins);
        self.modal = (fresh.count > 0).then_some(AppModal::ApplyAutoSell {
            quote: fresh,
            pending: false,
            requoted,
        });
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
        Some(AppModal::DraftWarning { then }) => show_draft_warning(ctx, f, then),
        Some(AppModal::Recruit { country, phase }) => show_recruit(ctx, f, &country, phase),
        Some(AppModal::ApplyAutoSell {
            quote,
            pending,
            requoted,
        }) => show_auto_sell(ctx, f, &quote, pending, requoted),
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

/// What a click in the Recruit modal asks for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RecruitChoice {
    /// Recruit as the offer's archetype at this index.
    As(usize),
    BackToCamp(u64),
    Close,
}

/// The Recruit modal. Choose reads the core offer every frame, so its price
/// and archetypes are always what `RecruitAs` will charge and accept; an
/// offer core withdraws meanwhile reads its blocker instead.
fn show_recruit(ctx: &egui::Context, f: &mut UiCtx, country: &str, phase: RecruitPhase) {
    let offer = f.game.recruit_offer(country);
    let joined = match phase {
        RecruitPhase::Choose => None,
        RecruitPhase::Joined { recruit_id } => Some((
            recruit_id,
            f.game
                .strategy()
                .roster
                .iter()
                .find(|r| r.id == recruit_id)
                .map_or_else(|| "A survivor".to_owned(), |r| r.name.clone()),
        )),
    };
    let shown = egui::Modal::new(egui::Id::new("lastlight_app_modal")).show(ctx, |ui| {
        ui.set_max_width(440.0);
        let mut choice = None;
        if let Some((recruit_id, name)) = &joined {
            ui.heading(format!("{name} joined your camp."));
            ui.label("Their own equipment, training and history start now.");
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if primary_in_modal(ui, BACK_TO_CAMP).clicked() {
                    choice = Some(RecruitChoice::BackToCamp(*recruit_id));
                }
                if ui.button(STAY_HERE).clicked() {
                    choice = Some(RecruitChoice::Close);
                }
            });
            return choice;
        }
        match &offer {
            Ok(offer) => {
                ui.heading(format!("Recruit from {}", offer.country_name));
                ui.label(
                    RichText::new(format!("{} · {}", offer.name, offer.origin_trait.label()))
                        .strong(),
                );
                muted(ui, offer.origin_trait.blurb());
                for (index, card) in offer.archetypes.iter().enumerate() {
                    ui.add_space(8.0);
                    ui.push_id(("recruit_archetype", index), |ui| {
                        muted(
                            ui,
                            format!(
                                "{} tactics · ST {} · DX {} · HT {} · Guns {} · Melee {}",
                                card.policy.label(),
                                card.st,
                                card.dx,
                                card.ht,
                                card.guns,
                                card.melee
                            ),
                        );
                        let label = recruit_as_label(offer, index);
                        // One amber primary: the offer's first archetype.
                        let clicked = if index == 0 {
                            primary_in_modal(ui, &label).clicked()
                        } else {
                            ui.button(label).clicked()
                        };
                        if clicked {
                            choice = Some(RecruitChoice::As(index));
                        }
                    });
                }
                ui.add_space(10.0);
                if ui.button(NOT_NOW).clicked() {
                    choice = Some(RecruitChoice::Close);
                }
            }
            Err(blocker) => {
                ui.heading("Recruit a survivor");
                ui.label(blocker.label());
                ui.add_space(10.0);
                if ui.button(CLOSE).clicked() {
                    choice = Some(RecruitChoice::Close);
                }
            }
        }
        choice
    });
    match shown.inner {
        Some(RecruitChoice::As(index)) => {
            if let Ok(offer) = &offer {
                // The notice system moves the modal to Joined on the id.
                f.dispatch(
                    Command::RecruitAs {
                        country: country.to_owned(),
                        archetype: offer.archetypes[index].archetype.id().to_owned(),
                    },
                    "",
                );
            }
        }
        Some(RecruitChoice::BackToCamp(recruit_id)) => {
            f.status.modal = None;
            f.open_camp_home(Some(HomeFocus::Recruit(recruit_id)));
        }
        Some(RecruitChoice::Close) => f.status.close_modal(),
        // A backdrop click closes, as Escape does.
        None if shown.should_close() => f.status.close_modal(),
        None => {}
    }
}

/// The stash apply: what the stored rule would sell from the stash, by
/// quality, and the two ways on. Nothing is sold until the amber primary
/// dispatches `Command::ApplyAutoSellToStash` with the quote shown; the
/// modal then waits for the runner's answer (`notices.rs`).
fn show_auto_sell(
    ctx: &egui::Context,
    f: &mut UiCtx,
    quote: &AutoSellPreview,
    pending: bool,
    requoted: bool,
) {
    let confirm = auto_sell_confirm(quote);
    let shown = egui::Modal::new(egui::Id::new("lastlight_app_modal")).show(ctx, |ui| {
        ui.set_max_width(440.0);
        ui.heading(auto_sell_title(quote));
        if requoted {
            ui.label(RichText::new(AUTO_SELL_REQUOTED).color(AMBER));
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for quality in Quality::ALL.into_iter().rev() {
                let count = quote.by_quality[quality];
                if count == 0 {
                    continue;
                }
                chrome::quality_shape_icon(ui, quality, f.settings);
                ui.colored_label(
                    chrome::quality_colour(quality, f.settings.palette),
                    format!("{count} {}", quality.name()),
                );
            }
        });
        muted(ui, AUTO_SELL_KEPT);
        ui.add_space(10.0);
        let mut choice = None;
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(!pending, |ui| {
                if primary_in_modal(ui, &confirm).clicked() {
                    choice = Some(true);
                }
            });
            if ui.button(KEEP_ITEMS).clicked() {
                choice = Some(false);
            }
        });
        choice
    });
    match shown.inner {
        Some(true) => {
            if let Some(AppModal::ApplyAutoSell { pending, .. }) = &mut f.status.modal {
                *pending = true;
            }
            f.dispatch(
                Command::ApplyAutoSellToStash {
                    expect_count: quote.count,
                    expect_coins: quote.coins,
                },
                "",
            );
        }
        Some(false) => f.status.close_modal(),
        // A backdrop click keeps the items, as Escape does.
        None if shown.should_close() => f.status.close_modal(),
        None => {}
    }
}

/// What a click in the draft warning asks for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DraftChoice {
    Apply,
    Discard,
    Keep,
}

/// The draft warning: the difficulty draft differs from what the next run
/// uses, and leaving Settings would drop it.
fn show_draft_warning(ctx: &egui::Context, f: &mut UiCtx, then: LeaveSettings) {
    let pending = f.runtime.durable_transition_pending() || f.status.save_pending;
    let draft = f.balance_draft_state();
    let shown = egui::Modal::new(egui::Id::new("lastlight_app_modal")).show(ctx, |ui| {
        ui.set_max_width(440.0);
        ui.heading(DRAFT_WARNING_TITLE);
        ui.label(
            "The difficulty draft differs from what the next run uses.              Leaving Settings without applying it drops the draft.",
        );
        let invalid = matches!(draft, DraftState::Invalid);
        if invalid {
            // Readable without hovering the disabled button.
            ui.label(RichText::new(FIX_DRAFT_FIRST).color(RED));
        }
        ui.add_space(10.0);
        let mut choice = None;
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(!pending, |ui| {
                ui.add_enabled_ui(!invalid, |ui| {
                    if primary_in_modal(ui, APPLY_AND_RETURN)
                        .on_disabled_hover_text(FIX_DRAFT_FIRST)
                        .clicked()
                    {
                        choice = Some(DraftChoice::Apply);
                    }
                });
                if ui.button(DISCARD_AND_RETURN).clicked() {
                    choice = Some(DraftChoice::Discard);
                }
            });
            if ui.button(KEEP_EDITING).clicked() {
                choice = Some(DraftChoice::Keep);
            }
        });
        choice
    });
    match shown.inner {
        Some(DraftChoice::Apply) => {
            f.status.modal = None;
            // The steps are queued ahead of the save, so the save holds them.
            f.apply_balance_draft(draft);
            f.finish_leaving_settings(then);
        }
        Some(DraftChoice::Discard) => {
            f.status.modal = None;
            f.discard_balance_draft();
            f.finish_leaving_settings(then);
        }
        Some(DraftChoice::Keep) => f.status.close_modal(),
        // A backdrop click keeps editing, as Escape does.
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
