//! Campaign selection and the persistent preparation shell.
use super::{
    Screen, Tab, UiCtx, armory, briefing, character, chrome, engineering, feedback, muted,
    navigation, operations, primary, progress, skills, status, survivor,
};
use crate::{
    art::{AMBER, GREEN, INK, PANEL, RED},
    runner::SaveIntent,
    settings::Action,
};
use bevy_egui::egui::{self, Align, Layout, RichText, vec2};
use horde_core::{command::Command, content::Mode, world::world};

pub(super) fn camp(root: &mut egui::Ui, f: &mut UiCtx) {
    camp_controls(root.ctx(), f);
    if matches!(f.tab, Tab::Skills | Tab::Engineering) {
        upgrade_surface(root, f);
        return;
    }
    navigation::camp_navigation(root, f);
    egui::Panel::bottom("camp_status")
        .min_size(if f.game.campaign.world.start.is_none() {
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
            if f.tab == Tab::World {
                country_action(ui, f);
            } else if f.tab != Tab::Operations {
                briefing::mission_actions(ui, f);
            }
            ui.add_space(5.0);
            if f.game.campaign.world.start.is_some() {
                ui.horizontal_wrapped(|ui| {
                    if ui.small_button("Save campaign").clicked() {
                        f.request_save(SaveIntent::Manual);
                    }
                    if f.operations_available() {
                        time_controls(ui, f);
                    }
                });
            } else if ui.small_button("Back to survivor").clicked() {
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
                .show(ui, |ui| match f.tab {
                    Tab::World => world_ui(ui, f),
                    Tab::Briefing => briefing::briefing(ui, f),
                    Tab::Character => character(ui, f),
                    Tab::Survivor => survivor(ui, f),
                    Tab::Armory => armory(ui, f),
                    Tab::Skills => skills(ui, f),
                    Tab::Engineering => engineering(ui, f),
                    Tab::Operations => operations(ui, f),
                });
        });
}
fn upgrade_surface(root: &mut egui::Ui, f: &mut UiCtx) {
    egui::Panel::top("upgrade_navigation")
        .frame(
            egui::Frame::new()
                .fill(INK)
                .inner_margin(egui::Margin::symmetric(12, 6)),
        )
        .show(root, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("Camp", |ui| {
                    if ui.button("Back to briefing").clicked() {
                        f.open_preparation(Tab::Briefing);
                    }
                    if ui.button("Campaign map").clicked() {
                        f.open_campaign();
                    }
                    if ui.button("Equipment").clicked() {
                        f.open_preparation(Tab::Armory);
                    }
                    if ui.button("Save campaign").clicked() {
                        f.request_save(SaveIntent::Manual);
                    }
                    time_controls(ui, f);
                });
                if ui
                    .selectable_label(f.tab == Tab::Skills, "Skills")
                    .clicked()
                {
                    f.open_preparation(Tab::Skills);
                }
                if ui
                    .selectable_label(f.tab == Tab::Engineering, "Fortifications")
                    .clicked()
                {
                    f.open_preparation(Tab::Engineering);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Settings").clicked() {
                        f.open_settings();
                    }
                    muted(
                        ui,
                        if f.game.global_paused {
                            "World paused"
                        } else {
                            "World running"
                        },
                    );
                });
            });
        });
    egui::Panel::bottom("upgrade_mission")
        .min_size(76.0)
        .frame(egui::Frame::new().fill(INK).inner_margin(10))
        .show(root, |ui| {
            briefing::mission_actions(ui, f);
            if f.status.message_error || f.status.save_pending || f.status.practice_profile {
                status(ui, f);
            }
        });
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK).inner_margin(14))
        .show(root, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("upgrade_page", f.tab as u8))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if f.tab == Tab::Skills {
                        skills(ui, f);
                    } else {
                        engineering(ui, f);
                    }
                });
        });
}
fn world_ui(ui: &mut egui::Ui, f: &mut UiCtx) {
    let compact = ui.available_width() < 900.0;
    if compact {
        ui.label(
            RichText::new(if f.game.campaign.world.start.is_none() {
                "Choose your starting country"
            } else {
                "Campaign map"
            })
            .size(22.0)
            .strong(),
        );
    } else {
        chrome::page_title(
            ui,
            if f.game.campaign.world.start.is_none() {
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
                        && f.game.campaign.world.start.is_some()
                        && !f.game.campaign.world.can_enter(&country.id)
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
    let cleared = f.game.campaign.world.completed(&country.id);
    ui.label(format!(
        "{} defenses · {:.2}× difficulty",
        levels, country.difficulty
    ));
    if f.game.campaign.world.start.is_some() {
        progress(
            ui,
            cleared as f32 / levels as f32,
            format!("{cleared} / {levels} secured"),
            GREEN,
        );
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
fn country_action(ui: &mut egui::Ui, f: &mut UiCtx) {
    let Some(country) = world().country(&f.campaign.selected_country) else {
        return;
    };
    if f.game.attempt.is_some() {
        if primary(ui, "Return to active defense") {
            f.go(Screen::Battle);
            f.battle.fire_latched = false;
        }
        return;
    }
    let choosing = f.game.campaign.world.start.is_none();
    if choosing && f.game.campaign.selected_survivor.is_none() {
        if primary(ui, "Choose survivor first") {
            f.open_survivor_selection();
        }
        return;
    }
    ui.horizontal_wrapped(|ui| {
        let allowed = choosing || f.game.campaign.world.can_enter(&country.id);
        ui.add_enabled_ui(allowed, |ui| {
            if primary(
                ui,
                if !choosing
                    && f.game.campaign.world.completed(&country.id) >= country.level_count()
                {
                    "Manage country"
                } else {
                    "Start journey"
                },
            ) {
                if choosing {
                    f.dispatch(
                        Command::ChooseStart(country.id.clone()),
                        "First defense underway. Aim, earn coins, then retreat to train.",
                    );
                    f.campaign.stage = 0;
                } else {
                    f.dispatch(
                        Command::EnterCountry(country.id.clone()),
                        "Journey started in the selected country.",
                    );
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
    if feedback::overlay_owns_keyboard(ctx, f) || f.prefs.rebind.is_some() {
        return;
    }
    if f.settings.pressed(ctx, Action::Pause, false) {
        toggle_time(f);
    }
}
fn toggle_time(f: &mut UiCtx) {
    let paused = f.game.global_paused
        || f.game
            .attempt
            .as_ref()
            .is_some_and(|attempt| attempt.paused);
    if paused && f.game.attempt.is_some() {
        f.go(Screen::Battle);
        f.battle.construction = false;
    }
    f.dispatch(Command::SetPaused(!paused), "");
    f.battle.fire_latched = false;
    f.runtime.clear_input();
}
fn time_controls(ui: &mut egui::Ui, f: &mut UiCtx) {
    let paused = f.game.global_paused
        || f.game
            .attempt
            .as_ref()
            .is_some_and(|attempt| attempt.paused);
    let label = if paused && f.game.attempt.is_some() {
        "Return and resume defense"
    } else if paused {
        "Resume world clock"
    } else {
        "Pause world clock"
    };
    if ui
        .button(label)
        .on_hover_text(format!(
            "{} pauses every battle and strategic deadline. Personal combat runs at 1×.",
            f.settings.bindings[Action::Pause as usize]
        ))
        .clicked()
    {
        toggle_time(f);
    }
    let fields = f
        .game
        .battles
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
    if fields > 0 || f.game.strategy.active_outbreaks() > 0 {
        muted(
            ui,
            format!(
                "{fields} expeditions · {} alerts",
                f.game.strategy.active_outbreaks()
            ),
        );
    }
    if fields > 0 || f.game.strategy.grand_strategy_unlocked(&f.game.campaign) {
        if f.game
            .attempt
            .as_ref()
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
