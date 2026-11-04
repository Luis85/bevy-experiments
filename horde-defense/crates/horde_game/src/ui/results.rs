//! Primary home after every main round. All rewards have already been settled.
use super::{Screen, Tab, UiCtx, muted, navigation, progression, status};
use crate::{
    art::{INK, MUTED, TEXT},
    runner::SaveIntent,
};
use bevy_egui::egui::{self, Color32, RichText, Stroke, vec2};
use horde_core::{command::Command, sim::Outcome, world::world};
fn continue_label(f: &UiCtx) -> &'static str {
    if !f.game.campaign().journey_active || f.game.last_result().is_none() {
        return "Choose a country";
    }
    match f.game.last_result() {
        Some(report) if report.outcome == Outcome::Victory => {
            if report
                .country_id
                .as_deref()
                .and_then(|id| world().country(id))
                .is_some_and(|country| {
                    f.game.campaign().world.completed(&country.id) >= country.level_count()
                })
            {
                "Choose next country"
            } else {
                "Next defense"
            }
        }
        Some(_) => "Try again",
        None => "Continue journey",
    }
}
fn retreat_hint(cause: &str) -> String {
    let lower = cause.to_ascii_lowercase();
    if lower.contains("rest") && lower.contains("train") {
        cause.to_owned()
    } else if cause.trim().is_empty() {
        "Rest and train before trying again.".into()
    } else {
        format!("{cause} Rest and train before trying again.")
    }
}
pub(super) fn results(root: &mut egui::Ui, f: &mut UiCtx) {
    // The same row as the camp; its selected first tab names this page.
    let with_row = f.game.campaign().world.start.is_some();
    navigation::camp_navigation(root, f);
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK).inner_margin(14))
        .show(root, |ui| {
            // The global header carries the wallet, Menu, History and Settings.
            if !with_row {
                ui.label(RichText::new("Journey upgrades").size(26.0).strong());
            }
            if let Some(report) = f.game.last_result() {
                let label = match report.outcome {
                    Outcome::Victory => "Victory",
                    Outcome::Defeat => "Retreated to camp",
                    Outcome::Withdrawal => "Retreated to camp",
                };
                ui.label(format!(
                    "{label} · {} · defense {} · Already banked: +{} coins · {} recovered items",
                    report.country_name,
                    report.stage + 1,
                    report.coins,
                    report.loot
                ));
            } else {
                muted(
                    ui,
                    "Spend retained coins, choose an upgrade, then continue your journey.",
                );
            }
            if let Some(report) = f
                .game
                .last_result()
                .filter(|r| r.outcome != Outcome::Victory)
            {
                ui.add(
                    egui::Label::new(
                        RichText::new(retreat_hint(&report.cause))
                            .size(12.0)
                            .color(MUTED),
                    )
                    .truncate(),
                )
                .on_hover_text(&report.cause);
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

/// Primary continuation first, then the analysis, the non-navigation extras
/// and the quiet exit. Destinations the camp row lists are not repeated here.
fn actions(ui: &mut egui::Ui, f: &mut UiCtx) {
    let live = f.game.attempt().is_some();
    let label = if live {
        "Return to active defense"
    } else {
        continue_label(f)
    };
    if ui
        .add(
            egui::Button::new(RichText::new(label).strong())
                .fill(Color32::BLACK)
                .stroke(Stroke::new(2.0, TEXT))
                .min_size(vec2(200.0, 40.0)),
        )
        .clicked()
    {
        if live {
            f.go(Screen::Battle);
            f.battle.fire_latched = false;
        } else if f.game.campaign().journey_active && f.game.last_result().is_some() {
            f.dispatch(Command::ContinueJourney, "Journey continued.");
        } else {
            f.open_campaign();
        }
    }
    if ui.button("Run analysis").clicked() {
        f.open_history();
    }
    ui.menu_button("More", |ui| {
        if !live
            && (f.game.campaign().legacy_features || f.game.campaign().victories > 0)
            && ui.button("Plan or replay a defense").clicked()
        {
            f.open_preparation(Tab::Briefing);
            ui.close();
        }
        if ui.button("Save journey").clicked() {
            f.request_save(SaveIntent::Manual);
            ui.close();
        }
    });
    if !live
        && ui
            .add(egui::Button::new(RichText::new("Leave journey").color(MUTED)).frame(false))
            .on_hover_text(
                "End this journey and return to the main menu. Banked coins, items and upgrades stay.",
            )
            .clicked()
    {
        f.dispatch(Command::LeaveJourney, "");
        f.status.save_pending = true;
    }
}
