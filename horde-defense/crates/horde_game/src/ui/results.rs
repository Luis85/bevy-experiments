//! Primary home after every main round. All rewards have already been settled.
use super::{Screen, Tab, UiCtx, muted, navigation, primary, progression, status};
use crate::{
    art::{AMBER, GREEN, INK, MUTED, RED, TEXT},
    runner::SaveIntent,
};
use bevy_egui::egui::{self, Align, FontId, RichText, TextFormat, text::LayoutJob};
use horde_core::{
    command::Command,
    sim::{Outcome, Report},
    world::world,
};
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
/// Why the round ended and what to try next, from the core report.
fn reason_line(report: &Report) -> String {
    let cause = report.cause.trim().trim_end_matches('.');
    match (cause.is_empty(), report.tip.trim().is_empty()) {
        (true, true) => "Rest and train before trying again.".into(),
        (true, false) => report.tip.trim().to_owned(),
        (false, true) => format!("{cause}."),
        (false, false) => format!("{cause}. {}", report.tip.trim()),
    }
}
/// One outcome line in the page-title face (outcome · country · defense)
/// with the banked rewards beside it, and at most one muted hint under it.
/// The header shows the wallet; this strip only names what was banked.
/// What a victory opened, from core queries: the fronts a liberation
/// unlocked, then the nodes whose milestone this round reached.
fn victory_line(f: &UiCtx, report: &Report) -> Option<String> {
    let campaign = f.game.campaign();
    let mut parts = Vec::new();
    if let Some(id) = report.country_id.as_deref()
        && campaign.world.cleared.iter().any(|c| c == id)
        && report.country_freed
    {
        let fronts: Vec<&str> = campaign
            .world
            .fronts_opened_by(id)
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        parts.push(if fronts.is_empty() {
            format!("{} liberated.", report.country_name)
        } else {
            format!(
                "{} liberated. New fronts: {}.",
                report.country_name,
                fronts.join(", ")
            )
        });
    }
    let opened: Vec<&str> = campaign
        .milestone_opened_nodes()
        .iter()
        .map(|n| n.name.as_str())
        .collect();
    if !opened.is_empty() {
        let shown = opened.len().min(4);
        let more = opened.len() - shown;
        parts.push(format!(
            "Now open in the tree: {}{}.",
            opened[..shown].join(", "),
            if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            }
        ));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}
fn outcome_strip(ui: &mut egui::Ui, f: &UiCtx, report: &Report) {
    let label = report.outcome.label();
    let color = match report.outcome {
        Outcome::Victory => GREEN,
        Outcome::Defeat => RED,
        Outcome::Withdrawal => AMBER,
    };
    let title = |color| TextFormat {
        font_id: FontId::new(24.0, egui::FontFamily::Name("display".into())),
        color,
        valign: Align::Center,
        ..TextFormat::default()
    };
    let mut line = LayoutJob::default();
    line.append(label, 0.0, title(color));
    line.append(
        &format!(" · {} · Defense {}", report.country_name, report.stage + 1),
        0.0,
        title(TEXT),
    );
    line.append(
        &format!(" · banked +{} coins · {} items", report.coins, report.loot),
        0.0,
        TextFormat {
            font_id: FontId::proportional(16.0),
            color: MUTED,
            valign: Align::Center,
            ..TextFormat::default()
        },
    );
    // One line at every width: a long country name truncates (full text on
    // hover) instead of pushing the actions down.
    let full = line.text.clone();
    ui.add(egui::Label::new(line).truncate())
        .on_hover_text(full);
    // The reason is the point of this screen after a loss; after a win it
    // names what opened. Wrapped, never cut.
    let next = if report.outcome == Outcome::Victory {
        victory_line(f, report)
    } else {
        Some(reason_line(report))
    };
    if let Some(next) = next {
        ui.add(egui::Label::new(RichText::new(next).size(13.0).color(TEXT)).wrap());
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
                outcome_strip(ui, f, report);
            } else {
                muted(
                    ui,
                    "Spend retained coins, choose an upgrade, then continue your journey.",
                );
            }
            ui.add_space(4.0);
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
    if primary(ui, label).clicked() {
        if live {
            f.go(Screen::Battle);
            f.battle.fire_latched = false;
        } else if f.game.campaign().journey_active && f.game.last_result().is_some() {
            f.dispatch(Command::ContinueJourney, "Journey continued.");
        } else {
            f.open_campaign();
        }
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
    // A durable transition (a save or reset) would reject the leave and
    // strand "Saving…", so the exit waits for it.
    if !live
        && ui
            .add_enabled(
                !f.runtime.durable_transition_pending(),
                egui::Button::new(RichText::new("Leave journey").color(MUTED)).frame(false),
            )
            .on_hover_text(
                "End this journey and return to the main menu. Banked coins, items and upgrades stay.",
            )
            .clicked()
    {
        f.dispatch(Command::LeaveJourney, "");
        f.status.save_pending = true;
    }
}
