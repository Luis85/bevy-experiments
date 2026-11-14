//! The debrief after every main round (UX-01, UX-02; design-ux-flow §4
//! "Results"). All rewards have already been settled. The screen shows the
//! outcome strip, then inline content (loot, the liberation block, the next
//! defender) and one action bar of exactly three buttons: [Upgrade]
//! [continue] [Leave journey]. The upgrade tree lives on `Screen::Upgrades`.
use super::{
    Screen, UiCtx, chrome, coins::grouped, muted, primary, primary_action::PRIMARY_MIN_SIZE, status,
};
use crate::art::{AMBER, GREEN, INK, MUTED, PANEL, RED, TEXT};
use bevy_egui::egui::{self, Align, FontId, RichText, TextFormat, text::LayoutJob, vec2};
use horde_core::{
    campaign::{NodeStatus, RecommendationAction, UpgradeFocus},
    command::Command,
    content::{Mode, content},
    defenders::{DefenderId, RecruitBlocker},
    progression::BossRank,
    sim::{Outcome, Report},
    skill_catalog::skills,
    stats::RunRecord,
    world::world,
};
mod loot_block;
/// Whether `report` is the victory that completed its country: the
/// continuation is then the choice of the next country.
pub(super) fn liberation_settled(f: &UiCtx, report: &Report) -> bool {
    report.outcome == Outcome::Victory
        && report
            .country_id
            .as_deref()
            .and_then(|id| world().country(id))
            .is_some_and(|country| {
                f.game.campaign().world.completed(&country.id) >= country.level_count()
            })
}
fn continue_label(f: &UiCtx) -> &'static str {
    if !f.game.campaign().journey_active {
        return "Choose a country";
    }
    match f.game.last_result() {
        Some(report) if liberation_settled(f, report) => "Choose next country",
        Some(report) if report.outcome == Outcome::Victory => "Next defense",
        Some(_) => "Try again",
        None => "Choose a country",
    }
}
/// Which of the action bar's first two buttons is the amber primary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ResultsAction {
    Upgrade,
    Continue,
}
/// The Results primary (design-ux-flow decision 3). After a defeat or a
/// withdrawal the loop's next step is the upgrade, but only when the
/// recommended node can be bought right now (`recommended` is its
/// `NodeStatus`); a win, a live defense and an unaffordable recommendation
/// all continue.
pub(super) fn results_primary(
    report: &Report,
    recommended: Option<&NodeStatus>,
    live: bool,
) -> ResultsAction {
    let lost = matches!(report.outcome, Outcome::Defeat | Outcome::Withdrawal);
    if !live && lost && matches!(recommended, Some(NodeStatus::Available(_))) {
        ResultsAction::Upgrade
    } else {
        ResultsAction::Continue
    }
}
/// The status of the node the tree would open on for `mode`
/// (`UpgradeFocus::Next`), or None when the recommendation is not a node.
fn recommended_status(f: &UiCtx, mode: Mode) -> Option<NodeStatus> {
    let campaign = f.game.campaign();
    match campaign
        .recommendation_fallback(UpgradeFocus::Next(mode))?
        .action
    {
        RecommendationAction::LearnNode(id) => Some(campaign.node_status(&id)),
        _ => None,
    }
}
/// The primary Results draws this frame.
pub(super) fn current_primary(f: &UiCtx) -> ResultsAction {
    f.game
        .last_result()
        .map_or(ResultsAction::Continue, |report| {
            results_primary(
                report,
                recommended_status(f, report.mode).as_ref(),
                f.game.attempt().is_some(),
            )
        })
}
/// How many tree nodes can be bought right now.
fn affordable_upgrades(f: &UiCtx) -> usize {
    let campaign = f.game.campaign();
    skills()
        .nodes
        .iter()
        .filter(|node| matches!(campaign.node_status(&node.id), NodeStatus::Available(_)))
        .count()
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
/// What a victory opened, from core queries: the fronts a liberation
/// unlocked, then the nodes whose milestone this round reached. A victory
/// over a Regional boss (`Report.boss_rank`) leads with the checkpoint.
fn victory_line(f: &UiCtx, report: &Report) -> Option<String> {
    let campaign = f.game.campaign();
    let mut parts = Vec::new();
    if report.boss_rank == BossRank::Regional {
        parts.push("Regional checkpoint secured.".to_owned());
    }
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
/// "Elite kills 3 · Capstone boss defeated in 25.5 s" from the report's run
/// record: the elites killed (when any) and the boss time-to-kill (when the
/// boss fell), a Regional or Capstone boss named by its rank. None when the
/// run has neither.
fn combat_line(run: &RunRecord) -> Option<String> {
    let mut parts = Vec::new();
    if run.elite_kills > 0 {
        parts.push(format!("Elite kills {}", run.elite_kills));
    }
    if let Some(ticks) = run.boss_ttk_ticks {
        let seconds = ticks as f32 / 60.0;
        parts.push(match run.boss_rank {
            BossRank::Sector => format!("Boss defeated in {seconds:.1} s"),
            rank => format!("{} boss defeated in {seconds:.1} s", rank.label()),
        });
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}
/// One outcome line in the page-title face (outcome · country · defense)
/// with the banked rewards beside it, and at most one muted hint under it.
/// The header shows the wallet; this strip only names what was banked.
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
        &format!(
            " · banked +{} coins · {} items",
            grouped(report.coins),
            report.loot
        ),
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
    if let Some(combat) = f.game.results_run().and_then(combat_line) {
        ui.add(egui::Label::new(RichText::new(combat).size(13.0).color(TEXT)).wrap());
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
    // Results is the debrief of a settled round: with none there is nothing
    // to show, so the screen hands over to Camp home.
    if f.game.last_result().is_none() {
        f.open_camp_home(None);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(INK))
            .show(root, |_| {});
        return;
    }
    // No camp row here: the bar's three actions are the only ways on.
    egui::Panel::bottom("results_actions")
        .frame(
            egui::Frame::new()
                .fill(PANEL)
                .inner_margin(egui::Margin::symmetric(18, 8)),
        )
        .show(root, |ui| {
            ui.horizontal(|ui| actions(ui, f));
        });
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK).inner_margin(14))
        .show(root, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| debrief(ui, f));
        });
}

/// The inline content above the action bar: the outcome strip, the run's
/// loot, the liberation block, the next defender and the upgrades hint.
/// Its only controls are the whitelisted inline ones ("Equip", "Recruit
/// from ...", the defender picker and the status line's own).
fn debrief(ui: &mut egui::Ui, f: &mut UiCtx) {
    // The title's mixed-size line box rounds a fraction above its row;
    // this keeps it inside the scroll viewport's clip.
    ui.add_space(2.0);
    // The global header carries the wallet, Menu, History and Settings.
    if let Some(report) = f.game.last_result() {
        outcome_strip(ui, f, report);
    }
    ui.add_space(4.0);
    loot_block::loot_block(ui, f);
    recruit_block(ui, f);
    // Who defends next, between defenses of a running journey.
    if f.game.campaign().journey_active {
        chrome::defender_picker(ui, f);
    }
    let affordable = affordable_upgrades(f);
    if affordable > 0 {
        muted(
            ui,
            chrome::count_noun(affordable, "upgrade affordable", "upgrades affordable"),
        );
    }
    if f.status.message_error || f.status.save_pending {
        status(ui, f);
    }
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
            label: format!("Recruit from {name} · {} coins", grouped(offer.price)),
        },
        Err(RecruitBlocker::NeedsNode {
            name: node, price, ..
        }) => RecruitBlock::Blocked(format!(
            "Learn {node} in Upgrades to recruit from {name} ({} coins)",
            grouped(price)
        )),
        Err(blocker) => RecruitBlock::Blocked(format!("Recruit from {name}: {}", blocker.label())),
    })
}

/// Inline content above the action bar, never a fourth action.
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

/// The continuation the action bar's second button names.
fn primary_label(f: &UiCtx) -> &'static str {
    if f.game.attempt().is_some() {
        "Return to active defense"
    } else {
        continue_label(f)
    }
}
pub(super) const UPGRADE: &str = "Upgrade";
pub(super) const LEAVE_JOURNEY: &str = "Leave journey";
/// The action bar's buttons in draw order: always exactly these three, in
/// fixed positions. Everything else on Results is inline content (the loot
/// block, the recruit block, the defender picker).
pub(super) fn action_bar_labels(f: &UiCtx) -> [&'static str; 3] {
    [UPGRADE, primary_label(f), LEAVE_JOURNEY]
}

/// One of the bar's first two buttons: the amber primary when `amber`,
/// otherwise a plain button of the primary's height.
fn bar_button(ui: &mut egui::Ui, label: &str, amber: bool) -> egui::Response {
    if amber {
        primary(ui, label)
    } else {
        ui.add(egui::Button::new(label).min_size(vec2(0.0, PRIMARY_MIN_SIZE.y)))
    }
}

/// [Upgrade] [continue] [Leave journey]. The amber primary is the
/// `results_primary` choice; the positions never change.
fn actions(ui: &mut egui::Ui, f: &mut UiCtx) {
    let live = f.game.attempt().is_some();
    let choice = current_primary(f);
    let [upgrade, continuation, leave] = action_bar_labels(f);
    if bar_button(ui, upgrade, choice == ResultsAction::Upgrade)
        .on_hover_text("Open the upgrade tree on the recommended upgrade.")
        .clicked()
    {
        f.focus_next_upgrade();
        f.open_upgrades();
    }
    if bar_button(ui, continuation, choice == ResultsAction::Continue).clicked() {
        if live {
            f.go(Screen::Battle);
            f.battle.fire_latched = false;
        } else if f.game.campaign().journey_active {
            f.dispatch(Command::ContinueJourney, "Journey continued.");
        } else {
            f.open_campaign();
        }
    }
    // A durable transition (a save or reset) would reject the leave and
    // strand "Saving…", so the exit waits for it; a live defense is
    // finished or retreated from first.
    if ui
        .add_enabled(
            !live && !f.runtime.durable_transition_pending(),
            egui::Button::new(RichText::new(leave).color(MUTED)).frame(false),
        )
        .on_hover_text(
            "End this journey and return to the main menu. Banked coins, items and upgrades stay.",
        )
        .on_disabled_hover_text(if live {
            "Finish or retreat from the active defense first."
        } else {
            "Saving…"
        })
        .clicked()
    {
        f.dispatch(Command::LeaveJourney, "");
        f.status.save_pending = true;
    }
}
