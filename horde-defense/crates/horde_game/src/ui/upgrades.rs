//! The Upgrades screen (UX-01; design-ux-flow §4 "Upgrades"): the camp row,
//! one action strip ([continue] and "Back to camp") and the upgrade tree at
//! full height. It is the one route to the complete tree; Results only
//! links here.
use super::{Screen, UiCtx, navigation, primary, progression, results, status};
use crate::art::INK;
use bevy_egui::egui::{self, RichText};
use horde_core::command::Command;

/// Where the strip's continue action leads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UpgradesContinue {
    /// A main defense is live: back onto its field.
    ReturnToDefense,
    /// The settled round liberated its country: `ContinueJourney` opens the
    /// choice of the next one.
    ChooseNextCountry,
    /// The settled round of a running journey: `ContinueJourney` deploys the
    /// stage to play next (the same stage after a loss).
    BackToBattle,
}
impl UpgradesContinue {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::ReturnToDefense => "Return to active defense",
            Self::ChooseNextCountry => "Choose next country",
            Self::BackToBattle => "Back to battle",
        }
    }
}
pub(super) const BACK_TO_CAMP: &str = "Back to camp";

/// The strip's continue action, or None when there is nothing to continue
/// (no settled report, or the journey was left): then the action is hidden
/// and "Back to camp" is the primary.
pub(super) fn upgrades_continue(f: &UiCtx) -> Option<UpgradesContinue> {
    if f.game.attempt().is_some() {
        return Some(UpgradesContinue::ReturnToDefense);
    }
    if !f.game.campaign().journey_active {
        return None;
    }
    let report = f.game.last_result()?;
    Some(if results::liberation_settled(f.game, report) {
        UpgradesContinue::ChooseNextCountry
    } else {
        UpgradesContinue::BackToBattle
    })
}
/// The continue action's label (design-ux-flow §3 `upgrades_continue_label`).
#[cfg(test)]
pub(super) fn upgrades_continue_label(f: &UiCtx) -> Option<&'static str> {
    upgrades_continue(f).map(UpgradesContinue::label)
}

pub(super) fn upgrades(root: &mut egui::Ui, f: &mut UiCtx) {
    // The same row as the camp; its selected entry names this page.
    let with_row = f.game.campaign().world.start.is_some();
    navigation::camp_navigation(root, f);
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK).inner_margin(14))
        .show(root, |ui| {
            // The global header carries the wallet, Menu, History and Settings.
            if !with_row {
                ui.label(RichText::new("Journey upgrades").size(26.0).strong());
            }
            ui.horizontal_wrapped(|ui| actions(ui, f));
            if f.status.message_error || f.status.save_pending {
                status(ui, f);
            }
            let height = (ui.clip_rect().bottom() - ui.cursor().top())
                .min(ui.available_height())
                .max(150.0);
            progression::skill_tree(ui, f, height);
        });
}

/// [continue] and "Back to camp". The continue action is the amber primary;
/// with nothing to continue, "Back to camp" is.
fn actions(ui: &mut egui::Ui, f: &mut UiCtx) {
    let next = upgrades_continue(f);
    if let Some(next) = next
        && primary(ui, next.label()).clicked()
    {
        match next {
            UpgradesContinue::ReturnToDefense => {
                f.go(Screen::Battle);
                f.battle.fire_latched = false;
            }
            UpgradesContinue::ChooseNextCountry | UpgradesContinue::BackToBattle => {
                f.dispatch(Command::ContinueJourney, "Journey continued.");
            }
        }
    }
    let back = if next.is_some() {
        ui.button(BACK_TO_CAMP)
    } else {
        primary(ui, BACK_TO_CAMP)
    };
    if back.clicked() {
        f.open_camp_home(None);
    }
}
