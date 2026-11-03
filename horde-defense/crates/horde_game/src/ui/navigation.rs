//! Task navigation and active-field wayfinding; authority stays in commands.
use super::*;

pub(super) fn header(root: &mut egui::Ui, f: &mut Frontend, accent: f32) {
    let header = egui::Panel::top("header")
        .exact_size(44.0)
        .frame(
            egui::Frame::new()
                .fill(PANEL)
                .inner_margin(egui::Margin::symmetric(18, 6)),
        )
        .show(root, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("LASTLIGHT")
                        .font(FontId::new(24.0, egui::FontFamily::Name("display".into())))
                        .color(AMBER),
                );
                if ui.available_width() > 670.0 {
                    muted(
                        ui,
                        match f.screen {
                            Screen::Battle => "Hold the line",
                            Screen::Results => "Journey upgrades",
                            Screen::Settings => "Settings",
                            Screen::History => "Run analysis",
                            _ => "Field command",
                        },
                    );
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if f.screen != Screen::Settings && ui.button("Settings").clicked() {
                        f.open_settings();
                    }
                    if f.screen == Screen::Camp && ui.small_button("History").clicked() {
                        f.open_history();
                    }
                    if f.screen == Screen::Camp && ui.small_button("Menu").clicked() {
                        f.dispatch(GameCommand::SetPaused(true), "");
                        f.request_save(SaveIntent::Menu);
                    }
                    ui.colored_label(AMBER, format!("{} coins", f.game.campaign.coins));
                    if ui.available_width() > 230.0 && !f.game.campaign.world.cleared.is_empty() {
                        muted(ui, format!("{} freed", f.game.campaign.world.cleared.len()));
                    }
                    let alerts = f.game.strategy.active_outbreaks();
                    if alerts > 0
                        && ui
                            .add(egui::Button::new(
                                RichText::new(format!("{alerts} plague alerts")).color(RED),
                            ))
                            .clicked()
                    {
                        f.dispatch(GameCommand::SetPaused(true), "");
                        f.fire_latched = false;
                        f.screen = Screen::Camp;
                        f.tab = Tab::Operations;
                    }
                });
            });
        });
    let r = header.response.rect;
    root.painter().line_segment(
        [r.left_bottom(), pos2(r.left() + 52.0 * accent, r.bottom())],
        Stroke::new(1.0, AMBER.gamma_multiply(accent)),
    );
}

pub(super) fn camp_navigation(root: &mut egui::Ui, f: &mut Frontend) {
    if f.game.campaign.world.start.is_none() {
        return;
    }
    egui::Panel::top("camp_navigation")
        .frame(
            egui::Frame::new()
                .fill(PANEL)
                .inner_margin(egui::Margin::symmetric(18, 7)),
        )
        .show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                if f.game.attempt.is_none() && ui.button("Back to journey upgrades").clicked() {
                    f.open_upgrades();
                }
                let response = ui.selectable_label(f.tab == Tab::World, "Campaign");
                if response.clicked() {
                    f.tab = Tab::World;
                }
                feedback::tab_marker(
                    ui,
                    &response,
                    "Campaign",
                    f.tab == Tab::World,
                    f.settings.reduced_effects,
                );
                let prep = !matches!(f.tab, Tab::World | Tab::Operations);
                let response = ui.selectable_label(prep, "Prepare");
                if response.clicked() {
                    f.open_preparation(f.last_prepare_tab);
                }
                feedback::tab_marker(
                    ui,
                    &response,
                    "Prepare",
                    !matches!(f.tab, Tab::World | Tab::Operations),
                    f.settings.reduced_effects,
                );
                if f.operations_available() {
                    let response = ui.selectable_label(f.tab == Tab::Operations, "Operations");
                    if response.clicked() {
                        f.tab = Tab::Operations;
                    }
                    feedback::tab_marker(
                        ui,
                        &response,
                        "Operations",
                        f.tab == Tab::Operations,
                        f.settings.reduced_effects,
                    );
                }
            });
            if !matches!(f.tab, Tab::World | Tab::Operations) {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for (tab, label) in [
                        (Tab::Briefing, "Briefing"),
                        (Tab::Armory, "Equipment"),
                        (Tab::Character, "Your character"),
                        (Tab::Survivor, "Mara"),
                        (Tab::Skills, "Skills"),
                        (Tab::Engineering, "Fortifications"),
                    ] {
                        if tab == Tab::Survivor
                            && !f
                                .game
                                .campaign
                                .has_feature(horde_core::skill_catalog::CampaignFeature::Companion)
                        {
                            continue;
                        }
                        if tab == Tab::Engineering
                            && !f.game.campaign.has_feature(
                                horde_core::skill_catalog::CampaignFeature::Construction,
                            )
                        {
                            continue;
                        }
                        if ui.selectable_label(f.tab == tab, label).clicked() {
                            f.tab = tab;
                            f.last_prepare_tab = tab;
                        }
                    }
                });
            }
        });
}

pub(super) fn field_status(root: &mut egui::Ui, f: &mut Frontend) {
    if !matches!(f.screen, Screen::Camp | Screen::Results) {
        return;
    }
    let fields = f.game.battles.iter().filter(|b| !b.settled).count();
    let alerts = f.game.strategy.active_outbreaks();
    if f.game.attempt.is_none() && fields == 0 && alerts == 0 {
        return;
    }
    egui::Panel::top("active_fields")
        .frame(
            egui::Frame::new()
                .fill(Color32::from_rgb(28, 37, 35))
                .inner_margin(egui::Margin::symmetric(18, 7)),
        )
        .show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                if let Some(a) = &f.game.attempt {
                    ui.strong(format!(
                        "Active defense · {}",
                        if a.mode == Mode::Personal {
                            "You"
                        } else {
                            "Mara"
                        }
                    ));
                    if ui.button("Return to active defense").clicked() {
                        f.screen = Screen::Battle;
                        f.fire_latched = false;
                    }
                }
                if fields > 0 {
                    ui.label(format!("{fields} live expeditions"));
                    if ui.button("View live fields").clicked() {
                        if f.screen != Screen::Camp {
                            f.dispatch(GameCommand::SetPaused(true), "");
                        }
                        f.screen = Screen::Camp;
                        f.tab = Tab::Operations;
                        if f.observed.is_none() {
                            f.observed = f.game.battles.iter().find(|b| !b.settled).map(|b| b.id);
                        }
                    }
                }
                if alerts > 0 {
                    ui.colored_label(RED, format!("{alerts} outbreaks need a response"));
                }
                muted(
                    ui,
                    if f.screen == Screen::Results {
                        "Fields paused during upgrades"
                    } else if f.game.global_paused {
                        "All fields paused"
                    } else {
                        "World clock running"
                    },
                );
            });
        });
}
