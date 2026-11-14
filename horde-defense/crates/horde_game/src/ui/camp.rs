//! Campaign selection and the persistent preparation shell.
use super::{
    Screen, Tab, UiCtx, armory, briefing, character, chrome, escape, home, muted, navigation,
    operations, primary, progress, status, survivor,
};
use crate::{
    art::{AMBER, GREEN, INK, PANEL, RED},
    settings::{self, ExtraAction},
};
use bevy_egui::egui::{self, Align, Layout, RichText, vec2};
use horde_core::{
    command::Command,
    content::Mode,
    sim::Game,
    world::{Country, world},
};

pub(super) fn camp(root: &mut egui::Ui, f: &mut UiCtx) {
    if f.tab == Tab::Home && f.game.campaign().world.start.is_none() {
        // There is no camp home before the first start: the world map is
        // where the journey begins. This pass already draws it.
        f.go_tab(Tab::World);
        f.tab = Tab::World;
    }
    camp_controls(root.ctx(), f);
    if matches!(f.tab, Tab::Skills | Tab::Engineering) {
        // The complete tree has one home. Old tab routes (remembered tabs,
        // saved navigation) land there instead of rendering a second copy.
        f.open_preparation(f.tab);
        return;
    }
    navigation::camp_navigation(root, f);
    egui::Panel::bottom("camp_status")
        .min_size(if f.game.campaign().world.start.is_none() {
            98.0
        } else {
            118.0
        })
        .frame(
            egui::Frame::new()
                .fill(PANEL)
                .inner_margin(egui::Margin::symmetric(18, 10)),
        )
        .show(root, |ui| {
            match f.tab {
                Tab::World => country_action(ui, f),
                Tab::Home => home::home_footer(ui, f),
                Tab::Operations if f.game.attempt().is_none() => {}
                _ => briefing::mission_actions(ui, f),
            }
            ui.add_space(5.0);
            // The manual save is the header's "Save" (W3-05).
            if f.game.campaign().world.start.is_some() {
                // Camp home owns the world clock (decision 6).
                if f.tab == Tab::Home && f.operations_available() {
                    chrome::row(ui, |ui| time_controls(ui, f));
                }
            } else if ui.button("Back to survivor").clicked() {
                f.open_survivor_selection();
            }
            if !f.status.message.is_empty()
                || f.status.message_error
                || f.status.save_pending
                || f.status.practice_profile
            {
                status(ui, f);
            }
        });
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK).inner_margin(18))
        .show(root, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("camp_page", f.tab as u8))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if f.tab.is_home_drill_down() {
                        breadcrumb(ui, f);
                    }
                    match f.tab {
                        Tab::Home => home::home(ui, f),
                        Tab::World => world_ui(ui, f),
                        Tab::Briefing => briefing::briefing(ui, f),
                        Tab::Character => character(ui, f),
                        Tab::Survivor => survivor(ui, f),
                        Tab::Armory => armory(ui, f),
                        // Redirected to Upgrades above.
                        Tab::Skills | Tab::Engineering => {}
                        Tab::Operations => operations(ui, f),
                    }
                });
        });
}
/// "‹ Camp home · Mara" above a drill-down page; the link opens Home.
fn breadcrumb(ui: &mut egui::Ui, f: &mut UiCtx) {
    chrome::row(ui, |ui| {
        if ui
            .add(egui::Button::new("‹ Camp home").frame(false))
            .on_hover_text("Back to Camp home")
            .clicked()
        {
            f.open_camp_home(None);
        }
        if let Some(name) = f.tab.drill_down_name() {
            muted(ui, format!("· {name}"));
        }
    });
}
fn world_ui(ui: &mut egui::Ui, f: &mut UiCtx) {
    let compact = ui.available_width() < 900.0;
    if compact {
        // The same display title as every page, without its description.
        chrome::page_title(
            ui,
            if f.game.campaign().world.start.is_none() {
                "Choose your starting country"
            } else {
                "Campaign map"
            },
            "",
        );
    } else {
        chrome::page_title(
            ui,
            if f.game.campaign().world.start.is_none() {
                "Choose your starting country"
            } else {
                "Campaign map"
            },
            "Select a front. Secure its defenses to open neighboring countries.",
        );
    }
    ui.horizontal(|ui| {
        ui.label("Find country");
        ui.add(
            egui::TextEdit::singleline(&mut f.campaign.search)
                .id(egui::Id::new("country_search"))
                .hint_text("Name or country code")
                .desired_width(ui.available_width().min(320.0)),
        );
    });
    if !f.campaign.search.trim().is_empty() {
        country_results_with_height(ui, f, if compact { 28.0 } else { 112.0 });
    }
    ui.add_space(8.0);
    if ui.available_width() >= 900.0 {
        let width = ui.available_width() - 316.0;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(width);
                f.campaign
                    .map
                    .show(ui, f.game, &mut f.campaign.selected_country);
            });
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.set_width(286.0);
                country_summary(ui, f);
            });
        });
    } else {
        // Reserve the map's220px projection plus its own controls before any
        // dossier content. This viewport is bounded even inside outer scrolling.
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), 260.0),
            Layout::top_down(Align::Min),
            |ui| {
                f.campaign
                    .map
                    .show(ui, f.game, &mut f.campaign.selected_country);
            },
        );
        if let Some(country) = world().country(&f.campaign.selected_country) {
            egui::CollapsingHeader::new(format!(
                "{} · {} defenses · country details",
                country.name,
                country.level_count()
            ))
            .show(ui, |ui| country_summary(ui, f));
        }
    }
    if f.campaign.search.trim().is_empty() {
        egui::CollapsingHeader::new("Browse available countries")
            .show(ui, |ui| country_results(ui, f));
    }
}
fn country_results(ui: &mut egui::Ui, f: &mut UiCtx) {
    country_results_with_height(ui, f, 112.0);
}
fn country_results_with_height(ui: &mut egui::Ui, f: &mut UiCtx, height: f32) {
    egui::ScrollArea::vertical()
        .id_salt("countries")
        .max_height(height)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let query = f.campaign.search.trim().to_lowercase();
                for country in &world().countries {
                    if !query.is_empty()
                        && !country.name.to_lowercase().contains(&query)
                        && !country.id.to_lowercase().contains(&query)
                    {
                        continue;
                    }
                    if query.is_empty()
                        && country.id != f.campaign.selected_country
                        && f.game.campaign().world.start.is_some()
                        && !f.game.campaign().world.can_enter(&country.id)
                    {
                        continue;
                    }
                    if ui
                        .selectable_label(f.campaign.selected_country == country.id, &country.name)
                        .clicked()
                    {
                        f.campaign.selected_country = country.id.clone();
                    }
                }
            });
        });
}
fn country_summary(ui: &mut egui::Ui, f: &UiCtx) {
    let Some(country) = world().country(&f.campaign.selected_country) else {
        return;
    };
    ui.label(RichText::new(&country.name).size(23.0).strong());
    let levels = country.level_count();
    let cleared = f.game.campaign().world.completed(&country.id);
    ui.label(defenses_and_difficulty(f.game, country));
    if f.game.campaign().world.start.is_some() {
        progress(
            ui,
            cleared as f32 / levels as f32,
            format!("{cleared} / {levels} secured"),
            GREEN,
        );
    }
    if let Some(line) = mara_timing(f.game, country) {
        ui.label(line);
    }
    muted(ui, format!("{} · {}", country.biome, country.culture));
    egui::CollapsingHeader::new("Country conditions and population").show(ui, |ui| {
        ui.label(format!(
            "Population: {:.1} million",
            country.population as f64 / 1_000_000.0
        ));
        for buff in &country.boosts {
            ui.colored_label(AMBER, buff);
        }
        for buff in &country.buffs {
            ui.colored_label(GREEN, buff);
        }
        for debuff in &country.debuffs {
            ui.colored_label(RED, debuff);
        }
    });
}
/// "N defenses · D× difficulty" for `country`, D the difficulty it deploys
/// at (`Game::effective_country_difficulty`, the start clamp included).
fn defenses_and_difficulty(game: &Game, country: &Country) -> String {
    format!(
        "{} defenses · {:.2}× difficulty",
        country.level_count(),
        game.effective_country_difficulty(&country.id)
            .unwrap_or(country.difficulty)
    )
}
/// When liberating `country` is what lets the player meet Mara, the
/// dossier says so before the country is chosen. The rule is core's
/// (`Game::country_choice_preview`).
fn mara_timing(game: &Game, country: &Country) -> Option<String> {
    game.country_choice_preview(&country.id)
        .filter(|preview| preview.companion_after_liberation)
        .map(|_| {
            format!(
                "Liberate {} to unlock Mara in Journey upgrades.",
                country.name
            )
        })
}
fn country_action(ui: &mut egui::Ui, f: &mut UiCtx) {
    let Some(country) = world().country(&f.campaign.selected_country) else {
        return;
    };
    if f.game.attempt().is_some() {
        if primary(ui, "Return to active defense").clicked() {
            f.go(Screen::Battle);
            f.battle.fire_latched = false;
        }
        return;
    }
    let choosing = f.game.campaign().world.start.is_none();
    if choosing && f.game.campaign().selected_survivor.is_none() {
        if primary(ui, "Choose survivor first").clicked() {
            f.open_survivor_selection();
        }
        return;
    }
    ui.horizontal_wrapped(|ui| {
        let allowed = choosing || f.game.campaign().world.can_enter(&country.id);
        ui.add_enabled_ui(allowed, |ui| {
            if primary(ui, country_action_label(f.game, country)).clicked() {
                if choosing {
                    f.dispatch(
                        Command::ChooseStart(country.id.clone()),
                        "First defense underway. Aim, earn coins, then retreat to train.",
                    );
                    f.campaign.stage = 0;
                } else {
                    // Choosing a later country never deploys (CD6): core
                    // selects it, then its Briefing opens (notices.rs).
                    f.dispatch(Command::SelectCountry(country.id.clone()), "");
                }
            }
        });
        if allowed {
            ui.label(format!(
                "{} · {} local defenses",
                country.name,
                country.level_count()
            ));
        } else {
            muted(ui, "Liberate a neighboring country to open this front.");
        }
    });
}
/// The World footer's label for `country`: "Start journey" deploys the first
/// country (`ChooseStart`); later countries are only chosen (`SelectCountry`),
/// as "Manage country" once liberated and "Plan defense" before.
fn country_action_label(game: &Game, country: &Country) -> &'static str {
    let world = &game.campaign().world;
    if world.start.is_none() {
        "Start journey"
    } else if world.completed(&country.id) >= country.level_count() {
        "Manage country"
    } else {
        "Plan defense"
    }
}
#[cfg(test)]
pub(super) fn country_dossier(ui: &mut egui::Ui, f: &mut UiCtx) {
    country_summary(ui, f);
    country_action(ui, f);
}

fn camp_controls(ctx: &egui::Context, f: &mut UiCtx) {
    let frame = ctx.cumulative_frame_nr();
    if f.battle.last_control_frame == frame {
        return;
    }
    f.battle.last_control_frame = frame;
    if escape::keyboard_owner(ctx, f) != escape::KeyboardOwner::Free {
        return;
    }
    // The world-clock key (its own binding, never the battle's Pause) drives
    // the clock only where the clock is shown (Camp home, decision 6), and
    // never resumes a waiting main defense from here: that has its own
    // "Return to active defense".
    if f.tab == Tab::Home
        && f.operations_available()
        && f.game.attempt().is_none()
        && f.settings
            .pressed_extra(ctx, ExtraAction::WorldClock, false)
    {
        toggle_time(f);
    }
}
fn toggle_time(f: &mut UiCtx) {
    let paused = f.game.global_paused() || f.game.attempt().is_some_and(|attempt| attempt.paused);
    if paused && f.game.attempt().is_some() {
        f.go(Screen::Battle);
        f.battle.set_construction(false);
    }
    f.dispatch(Command::SetPaused(!paused), "");
    f.battle.fire_latched = false;
    f.runtime.clear_input();
}
fn time_controls(ui: &mut egui::Ui, f: &mut UiCtx) {
    let paused = f.game.global_paused() || f.game.attempt().is_some_and(|attempt| attempt.paused);
    // One clock control with one name. The footer's primary "Return to
    // active defense" stays the one visible return route on the page; the
    // hover says that resuming also brings a live main defense back.
    let label = if paused {
        "Resume world clock"
    } else {
        "Pause world clock"
    };
    let returns = if f.game.attempt().is_some() {
        " Resuming returns you to your active defense."
    } else {
        ""
    };
    // With a main defense waiting the key is off (`camp_controls`), so the
    // hover names it only when it works.
    let key = f.settings.extra_bindings.get(ExtraAction::WorldClock);
    let key = if key.is_empty() || f.game.attempt().is_some() {
        String::new()
    } else {
        format!(" Key: {}.", settings::binding_text(key))
    };
    if ui
        .button(label)
        .on_hover_text(format!(
            "Pausing the world clock stops every battle and strategic deadline. Personal combat runs at 1×.{key}{returns}"
        ))
        .clicked()
    {
        toggle_time(f);
    }
    let fields = f
        .game
        .battles()
        .iter()
        .filter(|battle| !battle.settled)
        .count();
    ui.colored_label(
        if paused { AMBER } else { GREEN },
        if paused {
            "All fields paused"
        } else {
            "World running"
        },
    );
    if fields > 0 || f.game.strategy().active_outbreaks() > 0 {
        muted(
            ui,
            format!(
                "{} · {}",
                chrome::count_noun(fields, "expedition", "expeditions"),
                chrome::count_noun(f.game.strategy().active_outbreaks(), "alert", "alerts")
            ),
        );
    }
    if fields > 0 || f.game.strategy().grand_strategy_unlocked(f.game.campaign()) {
        if f.game
            .attempt()
            .is_some_and(|attempt| attempt.mode == Mode::Personal)
        {
            muted(ui, "Personal defense · 1×");
        } else {
            ui.label("Observe");
            for speed in [1, 2, 4] {
                ui.selectable_value(&mut f.battle.speed, speed, format!("{speed}×"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The world summary shows the difficulty a country deploys at
    /// (`Game::effective_country_difficulty`): before a start is chosen every
    /// country is a candidate start, so Bangladesh's raw 1.60 reads as its
    /// clamped 1.05, and after another start it reads raw (W2 review).
    #[test]
    fn country_summary_difficulty_reads_the_effective_value() {
        let bgd = world().country("BGD").unwrap();
        assert_eq!(bgd.difficulty, 1.60);
        let levels = bgd.level_count();
        let mut game = Game::default();
        assert_eq!(
            defenses_and_difficulty(&game, bgd),
            format!("{levels} defenses · 1.05× difficulty")
        );
        game.campaign_mut().choose_start_country("AUT").unwrap();
        assert_eq!(
            defenses_and_difficulty(&game, bgd),
            format!("{levels} defenses · 1.60× difficulty")
        );
        let aut = world().country("AUT").unwrap();
        assert_eq!(
            defenses_and_difficulty(&game, aut),
            format!("{} defenses · 0.95× difficulty", aut.level_count())
        );
    }
}
