//! Primary home after every main round. All rewards have already been settled.
use super::{Screen, Tab, UiCtx, chrome, muted, navigation, primary, progression, status};
use crate::art::{AMBER, GREEN, INK, MUTED, RED, TEXT};
use bevy_egui::egui::{self, Align, FontId, RichText, TextFormat, text::LayoutJob};
use horde_core::{
    command::Command,
    content::content,
    defenders::{DefenderId, RecruitBlocker},
    sim::{Outcome, Report},
    world::world,
};
mod loot_block;
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
/// " · <name>" when someone other than the player ran the defense, from
/// the defender's core card; empty for the player or a defender no longer
/// in camp.
fn defender_suffix(f: &UiCtx, report: &Report) -> String {
    match report.defender() {
        DefenderId::Player => String::new(),
        defender => f
            .game
            .defender_card(defender)
            .map_or_else(String::new, |card| format!(" · {}", card.name)),
    }
}
/// 1077 as "1,077": coin amounts of the progression lines.
fn grouped(amount: u64) -> String {
    let digits = amount.to_string();
    let mut text = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            text.push(',');
        }
        text.push(digit);
    }
    text
}
/// "Rank 10 milestone · +1,000 coins · <item name>" for each rank milestone
/// the report's settlement receipt lists, in its (ascending) order. The item
/// is named from the catalogue; a milestone that stored none names its rank
/// and coins only.
fn milestone_headlines(report: &Report) -> Vec<String> {
    report
        .progression_rewards
        .milestones
        .iter()
        .map(|milestone| {
            let item = milestone
                .item_definition
                .as_ref()
                .and_then(|id| content().item(id))
                .map_or_else(String::new, |definition| format!(" · {}", definition.name));
            format!(
                "Rank {} milestone · +{} coins{item}",
                milestone.rank,
                grouped(milestone.coins)
            )
        })
        .collect()
}
/// "Level 6 reached · +77 coins": the level-up grant the receipt paid, or
/// None when the report paid none.
fn level_grant_line(report: &Report) -> Option<String> {
    let rewards = &report.progression_rewards;
    (rewards.level_coins > 0).then(|| {
        format!(
            "Level {} reached · +{} coins",
            rewards.level_to,
            grouped(rewards.level_coins)
        )
    })
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
        &format!(
            " · {} · Defense {}{}",
            report.country_name,
            report.stage + 1,
            defender_suffix(f, report)
        ),
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
    // What progression settlement paid with this report, above "Now open".
    for headline in milestone_headlines(report) {
        ui.add(egui::Label::new(RichText::new(headline).size(14.0).strong().color(AMBER)).wrap());
    }
    if let Some(grant) = level_grant_line(report) {
        ui.add(egui::Label::new(RichText::new(grant).size(13.0).color(TEXT)).wrap());
    }
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
            // Inline content under the actions: the run's loot, then the
            // liberation block.
            loot_block::loot_block(ui, f);
            recruit_block(ui, f);
            // Who defends next, between defenses of a running journey.
            if f.game.campaign().journey_active && f.game.last_result().is_some() {
                chrome::defender_picker(ui, f);
            }
            if f.status.message_error || f.status.save_pending {
                status(ui, f);
            }
            let height = (ui.clip_rect().bottom() - ui.cursor().top())
                .min(ui.available_height())
                .max(150.0);
            progression::skill_tree(ui, f, height);
        });
}

/// What the liberation block shows, read from core's recruit offer.
#[derive(Clone, Debug, PartialEq, Eq)]
enum RecruitBlock {
    /// "Recruit from <Country> · N coins": opens the Recruit modal.
    Offer { country: String, label: String },
    /// A muted line: the missing node, or the core blocker's own words.
    Blocked(String),
}

/// The liberation block of the settled report: only a victory that freed
/// its country offers a recruit there. The price, the node's name and cost
/// and every blocker text come from `Game::recruit_offer`.
fn recruit_block_model(f: &UiCtx) -> Option<RecruitBlock> {
    let report = f.game.last_result()?;
    let country = report.country_id.as_deref()?;
    if report.outcome != Outcome::Victory
        || !report.country_freed
        || f.game.attempt().is_some()
        || !f.game.campaign().world.cleared.iter().any(|c| c == country)
    {
        return None;
    }
    let name = &report.country_name;
    Some(match f.game.recruit_offer(country) {
        Ok(offer) => RecruitBlock::Offer {
            country: offer.country_id,
            label: format!("Recruit from {name} · {} coins", offer.price),
        },
        Err(RecruitBlocker::NeedsNode {
            name: node, price, ..
        }) => RecruitBlock::Blocked(format!(
            "Learn {node} in Upgrades to recruit from {name} ({price} coins)"
        )),
        Err(blocker) => RecruitBlock::Blocked(format!("Recruit from {name}: {}", blocker.label())),
    })
}

/// Inline content under the action bar, never a fourth action.
fn recruit_block(ui: &mut egui::Ui, f: &mut UiCtx) {
    match recruit_block_model(f) {
        None => {}
        Some(RecruitBlock::Offer { country, label }) => {
            if ui
                .button(label)
                .on_hover_text("Choose one of two archetypes; the survivor joins your camp.")
                .clicked()
            {
                f.status.open_recruit(&country);
            }
        }
        Some(RecruitBlock::Blocked(line)) => muted(ui, line),
    }
}

/// The continuation the action bar's primary names.
fn primary_label(f: &UiCtx) -> &'static str {
    if f.game.attempt().is_some() {
        "Return to active defense"
    } else {
        continue_label(f)
    }
}
/// Whether the bar shows "More" (the replay route).
fn more_shown(f: &UiCtx) -> bool {
    f.game.attempt().is_none()
        && (f.game.campaign().legacy_features || f.game.campaign().victories > 0)
}
/// The action bar's buttons in draw order: at most three. Everything else
/// on Results is inline content (the recruit block, the defender picker).
#[cfg(test)]
pub(super) fn action_bar_labels(f: &UiCtx) -> Vec<&'static str> {
    let mut labels = vec![primary_label(f)];
    if more_shown(f) {
        labels.push("More");
    }
    if f.game.attempt().is_none() {
        labels.push(LEAVE_JOURNEY);
    }
    labels
}
const LEAVE_JOURNEY: &str = "Leave journey";

/// Primary continuation first, then the analysis, the non-navigation extras
/// and the quiet exit. Destinations the camp row lists are not repeated here.
fn actions(ui: &mut egui::Ui, f: &mut UiCtx) {
    let live = f.game.attempt().is_some();
    let label = primary_label(f);
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
    // The manual save is the header's "Save" (W3-05), so "More" holds only
    // the replay route and is hidden when that is unavailable.
    if more_shown(f) {
        ui.menu_button("More", |ui| {
            if ui.button("Plan or replay a defense").clicked() {
                f.open_preparation(Tab::Briefing);
                ui.close();
            }
        });
    }
    // A durable transition (a save or reset) would reject the leave and
    // strand "Saving…", so the exit waits for it.
    if !live
        && ui
            .add_enabled(
                !f.runtime.durable_transition_pending(),
                egui::Button::new(RichText::new(LEAVE_JOURNEY).color(MUTED)).frame(false),
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
