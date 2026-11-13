//! Camp home (`Tab::Home`, design-ux-flow decision 5 and §4 "Camp/Home"):
//! the next defense, the defender roster with core status and power, alerts,
//! expeditions, offers and recent reports, and the footer that owns the
//! world clock. Every rule is a core read; this module only renders and
//! queues commands.
use super::{HomeFocus, Screen, Tab, UiCtx, chrome, muted, primary};
use crate::art::{AMBER, GREEN, RED};
use bevy_egui::egui::{self, RichText};
use horde_core::{
    campaign::{RecommendationAction, UpgradeFocus},
    command::Command,
    content::Mode,
    defenders::{DefenderId, DefenderOption, PowerBand},
    sim::{Game, Outcome},
    strategy::{OutbreakOutcome, RecruitStatus},
    world::world,
};

/// Content width from which Home lays out two columns.
const TWO_COLUMNS_FROM: f32 = 720.0;
/// Alerts listed on Home before "+N more in Operations".
const HOME_ALERTS: usize = 3;
/// Recruit offers listed on Home.
const HOME_OFFERS: usize = 2;
/// Recent run reports listed on Home.
const HOME_REPORTS: usize = 3;

/// The camp's one primary action between defenses (loop map, Camp/Home).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HomePrimary {
    /// A main defense is live: return to it.
    ReturnToDefense,
    /// The settled report liberated its country and the journey is still
    /// active: `ContinueJourney` closes it and opens the world map.
    ChooseNextCountry,
    /// A country is active: the briefing deploys.
    GoToBriefing,
    /// No country yet: the world map.
    ChooseCountry,
}
impl HomePrimary {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::ReturnToDefense => "Return to active defense",
            Self::ChooseNextCountry => "Choose next country",
            Self::GoToBriefing => "Go to briefing",
            Self::ChooseCountry => "Choose a country",
        }
    }
}

/// Whether the settled report liberated its country while the journey is
/// still active, so continuing closes the country (core `ContinueJourney`
/// answers `CountryLiberated`). Results' continue label reads the same rule.
pub(super) fn liberation_pending(game: &Game) -> bool {
    game.campaign().journey_active
        && game.attempt().is_none()
        && game.last_result().is_some_and(|report| {
            report.outcome == Outcome::Victory
                && report
                    .country_id
                    .as_deref()
                    .and_then(|id| world().country(id))
                    .is_some_and(|country| {
                        game.campaign().world.completed(&country.id) >= country.level_count()
                    })
        })
}

/// The Home primary, a pure read of the game. The camp footers of the other
/// Prepare pages use the same rule (`briefing::mission_actions`).
pub(super) fn home_primary(game: &Game) -> HomePrimary {
    if game.attempt().is_some() {
        HomePrimary::ReturnToDefense
    } else if liberation_pending(game) {
        HomePrimary::ChooseNextCountry
    } else if game.campaign().world.active_country().is_some() {
        HomePrimary::GoToBriefing
    } else {
        HomePrimary::ChooseCountry
    }
}

/// Runs `action` as the camp primary does.
pub(super) fn run_home_primary(f: &mut UiCtx, action: HomePrimary) {
    match action {
        HomePrimary::ReturnToDefense => {
            f.go(Screen::Battle);
            f.battle.fire_latched = false;
        }
        HomePrimary::ChooseNextCountry => {
            f.dispatch(Command::ContinueJourney, "Journey continued.");
        }
        HomePrimary::GoToBriefing => f.open_preparation(Tab::Briefing),
        HomePrimary::ChooseCountry => f.open_campaign(),
    }
}

/// Queues `SetNextDefender(id)`; core accepts or rejects it.
pub(super) fn defend_next(f: &mut UiCtx, id: DefenderId) {
    f.dispatch(Command::SetNextDefender(id), "");
}

/// A roster row's status: "Defending" for the live main defender, a
/// recruit's `RecruitStatus::label` (a guard names its country through the
/// core blocker), or the core reason a defender cannot be chosen.
pub(super) fn status_label(game: &Game, option: &DefenderOption) -> String {
    let id = option.card.id;
    if game.attempt().is_some_and(|a| a.defender() == id) {
        return RecruitStatus::Defending.label().into();
    }
    if let DefenderId::Recruit(recruit) = id
        && let Some(entry) = game.strategy().roster.iter().find(|r| r.id == recruit)
    {
        return match (&option.selectable, entry.status) {
            (Err(blocker), RecruitStatus::Guarding) => blocker.label(),
            (_, RecruitStatus::Ready) => match &option.selectable {
                Ok(()) => RecruitStatus::Ready.label().into(),
                Err(blocker) => blocker.label(),
            },
            (_, status) => status.label().into(),
        };
    }
    match &option.selectable {
        Ok(()) => RecruitStatus::Ready.label().into(),
        Err(blocker) => blocker.label(),
    }
}

/// "HP m · Lv L": the defender's deploy max HP at the next threat
/// (`Game::deployment_inputs`) and its level.
pub(super) fn readiness_line(game: &Game, option: &DefenderOption) -> String {
    let hp = game
        .deployment_inputs(option.card.id, option.power.threat)
        .map_or_else(
            |_| "HP —".to_owned(),
            |inputs| format!("HP {}", inputs.max_hp),
        );
    format!("{hp} · Lv {}", option.card.level)
}

/// "Power P · threat T · Band" from core `defender_power` (CD11).
pub(super) fn power_line(option: &DefenderOption) -> String {
    let power = option.power;
    format!(
        "Power {} · threat {} · {}",
        power.rating,
        power.threat,
        power.band.label()
    )
}

fn band_color(band: PowerBand) -> egui::Color32 {
    match band {
        PowerBand::Weak => RED,
        PowerBand::Even => AMBER,
        PowerBand::Strong => GREEN,
    }
}

/// The Home page body.
pub(super) fn home(ui: &mut egui::Ui, f: &mut UiCtx) {
    let wide = ui.available_width() >= TWO_COLUMNS_FROM;
    chrome::page_title(
        ui,
        "Camp home",
        if wide {
            "Your next defense, your defenders and what needs you now."
        } else {
            ""
        },
    );
    if wide {
        let gap = 18.0;
        let left = ((ui.available_width() - gap) * 0.58).floor();
        let right = ui.available_width() - gap - left;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(left);
                left_column(ui, f);
            });
            ui.add_space(gap);
            ui.vertical(|ui| {
                ui.set_width(right);
                right_column(ui, f);
            });
        });
    } else {
        left_column(ui, f);
        right_column(ui, f);
    }
}

fn left_column(ui: &mut egui::Ui, f: &mut UiCtx) {
    chrome::section(ui, "Next defense");
    next_defense(ui, f);
    chrome::section(ui, "Defenders");
    roster(ui, f);
}

fn next_defense(ui: &mut egui::Ui, f: &mut UiCtx) {
    let game = f.game;
    if let Some(attempt) = game.attempt() {
        let name = game.active_defender().map_or_else(String::new, |c| c.name);
        ui.label(format!(
            "Active defense · {} · Defense {} · {name}",
            attempt.country_name,
            attempt.stage + 1
        ));
        return;
    }
    let Some(country) = game.campaign().world.active_country() else {
        muted(ui, "Choose a country on the world map to begin.");
        return;
    };
    let stage = game
        .campaign()
        .world
        .next_stage()
        .min(country.level_count().saturating_sub(1));
    let defender = game
        .defender_card(game.next_defender())
        .map_or_else(String::new, |c| c.name);
    if liberation_pending(game) {
        ui.label(format!(
            "{} liberated · choose your next front",
            country.name
        ));
    } else {
        ui.label(format!(
            "{} · Defense {} · {defender}",
            country.name,
            stage + 1
        ));
    }
}

fn roster(ui: &mut egui::Ui, f: &mut UiCtx) {
    let options = f.game.available_defenders();
    let live = f.game.attempt().is_some();
    let companion = f
        .game
        .campaign()
        .has_feature(horde_core::skill_catalog::CampaignFeature::Companion);
    for option in &options {
        let id = option.card.id;
        // Mara is listed even while locked; Home shows her once she can be
        // trained or chosen.
        if id == DefenderId::Mara && !companion {
            continue;
        }
        let focused = matches!(
            (f.nav.home_focus, id),
            (Some(HomeFocus::Recruit(a)), DefenderId::Recruit(b)) if a == b
        );
        ui.push_id(("home_defender", id), |ui| {
            chrome::row(ui, |ui| {
                let name = RichText::new(&option.card.name).strong();
                ui.label(if focused { name.color(AMBER) } else { name });
                ui.label(status_label(f.game, option));
                muted(ui, readiness_line(f.game, option));
                ui.colored_label(band_color(option.power.band), power_line(option));
                if option.selected {
                    muted(ui, "Defends next");
                } else {
                    let reason = if live {
                        Some("Finish the active defense first.".to_owned())
                    } else {
                        option.selectable.as_ref().err().map(|b| b.label())
                    };
                    let response = ui
                        .add_enabled(reason.is_none(), egui::Button::new("Defend next"))
                        .on_hover_text("Choose who defends the next stage.");
                    let response = match &reason {
                        Some(reason) => response.on_disabled_hover_text(reason),
                        None => response,
                    };
                    if response.clicked() {
                        defend_next(f, id);
                    }
                }
                if id == DefenderId::Mara && ui.button("Training ›").clicked() {
                    f.open_preparation(Tab::Survivor);
                }
            });
        });
    }
}

fn right_column(ui: &mut egui::Ui, f: &mut UiCtx) {
    if f.operations_available() {
        alerts(ui, f);
        expeditions(ui, f);
    }
    offers(ui, f);
    recent_reports(ui, f);
}

fn alerts(ui: &mut egui::Ui, f: &mut UiCtx) {
    chrome::section(ui, "Alerts");
    let strategy = f.game.strategy();
    let active: Vec<(String, u32, f32)> = strategy
        .outbreaks
        .iter()
        .filter(|o| o.outcome == OutbreakOutcome::Active)
        .map(|o| {
            (
                o.country_id.clone(),
                o.strength,
                o.deadline_tick.saturating_sub(strategy.tick) as f32 / 60.0,
            )
        })
        .collect();
    if active.is_empty() {
        muted(ui, "No outbreaks need a response.");
        return;
    }
    for (country_id, strength, seconds) in active.iter().take(HOME_ALERTS) {
        let name = world()
            .country(country_id)
            .map_or(country_id.as_str(), |c| c.name.as_str());
        ui.push_id(("home_alert", country_id.as_str()), |ui| {
            chrome::row(ui, |ui| {
                ui.colored_label(
                    RED,
                    format!(
                        "{name} · {} · {seconds:.0}s left",
                        chrome::count_noun(*strength as usize, "group", "groups")
                    ),
                );
                let live = f.game.attempt().is_some();
                if ui
                    .add_enabled(!live, egui::Button::new("Plan response"))
                    .on_hover_text("Selects the country and opens its briefing. Nothing deploys until you begin.")
                    .on_disabled_hover_text("Finish the active defense first.")
                    .clicked()
                {
                    // Core selects the country; its briefing opens once the
                    // command applies (notices.rs). Nothing deploys here.
                    f.campaign.selected_country = country_id.clone();
                    f.dispatch(Command::SelectCountry(country_id.clone()), "");
                }
            });
        });
    }
    if active.len() > HOME_ALERTS {
        muted(
            ui,
            format!("+{} more in Operations", active.len() - HOME_ALERTS),
        );
    }
}

fn expeditions(ui: &mut egui::Ui, f: &mut UiCtx) {
    chrome::section(ui, "Expeditions");
    let live = f.game.battles().iter().filter(|b| !b.settled).count();
    let reports = f.game.battles().iter().filter(|b| b.settled).count();
    chrome::row(ui, |ui| {
        ui.label(format!(
            "{live} live · {}",
            chrome::count_noun(reports, "report", "reports")
        ));
        if ui.button("Open operations ›").clicked() {
            f.go_tab(Tab::Operations);
        }
    });
}

fn offers(ui: &mut egui::Ui, f: &mut UiCtx) {
    chrome::section(ui, "Offers");
    let mut any = false;
    let offers: Vec<_> = f
        .game
        .campaign()
        .world
        .cleared
        .iter()
        .filter_map(|id| f.game.recruit_offer(id).ok())
        .take(HOME_OFFERS)
        .collect();
    for offer in offers {
        any = true;
        ui.push_id(("home_offer", offer.country_id.as_str()), |ui| {
            chrome::row(ui, |ui| {
                ui.label(format!(
                    "{} · from {} · {} coins",
                    offer.name, offer.country_name, offer.price
                ));
                if ui.button("Recruit ›").clicked() {
                    f.campaign.selected_country = offer.country_id.clone();
                    f.go_tab(Tab::Operations);
                }
            });
        });
    }
    let upgrade = f
        .game
        .campaign()
        .recommendation_fallback(UpgradeFocus::Next(Mode::Personal))
        .filter(|r| {
            matches!(r.action, RecommendationAction::LearnNode(_)) && r.affordable && r.price > 0
        });
    if let Some(upgrade) = upgrade {
        any = true;
        if ui
            .button(format!(
                "Upgrade: {} · {} coins",
                upgrade.title, upgrade.price
            ))
            .clicked()
        {
            f.campaign.improvement_focus = Some(upgrade.action.clone());
            f.open_upgrades();
        }
    }
    if !any {
        muted(ui, "Nothing to buy right now.");
    }
}

fn recent_reports(ui: &mut egui::Ui, f: &mut UiCtx) {
    chrome::section(ui, "Recent reports");
    let history = &f.game.campaign().run_history;
    if history.is_empty() {
        muted(ui, "No defenses fought yet.");
        return;
    }
    for record in history.iter().rev().take(HOME_REPORTS) {
        let place = if record.country_name.is_empty() {
            String::new()
        } else {
            format!("{} · ", record.country_name)
        };
        muted(
            ui,
            format!(
                "{} · {place}Defense {} · {}",
                record.outcome.label(),
                record.stage + 1,
                record.defender_name
            ),
        );
    }
}

/// The Home footer: the Home primary beside what it acts on, then the world
/// clock, which only Home shows (decision 6).
pub(super) fn home_footer(ui: &mut egui::Ui, f: &mut UiCtx) {
    let action = home_primary(f.game);
    chrome::row(ui, |ui| {
        if primary(ui, action.label()).clicked() {
            run_home_primary(f, action);
        }
        if action == HomePrimary::GoToBriefing
            && let Some(country) = f.game.campaign().world.active_country()
        {
            let defender = f
                .game
                .defender_card(f.game.next_defender())
                .map_or_else(String::new, |c| c.name);
            muted(ui, format!("{} · {defender}", country.name));
        }
    });
}
