//! Task navigation and active-field wayfinding; authority stays in commands.
use super::{Screen, Tab, UiCtx, feedback, muted};
use crate::{
    art::{AMBER, PANEL, RED},
    runner::SaveIntent,
};
use bevy_egui::egui::{self, Align, Color32, FontId, Layout, RichText, Stroke, pos2};
use horde_core::{command::Command, content::Mode, skill_catalog::CampaignFeature};

pub(super) fn header(root: &mut egui::Ui, f: &mut UiCtx, accent: f32) {
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
                    // Camp and the upgrade home share one header grammar.
                    let between_runs = matches!(f.screen, Screen::Camp | Screen::Results);
                    if between_runs && ui.small_button("History").clicked() {
                        f.open_history();
                    }
                    if between_runs && ui.small_button("Menu").clicked() {
                        f.dispatch(Command::SetPaused(true), "");
                        f.request_save(SaveIntent::Menu);
                    }
                    ui.colored_label(AMBER, format!("{} coins", f.game.campaign().coins));
                    if ui.available_width() > 230.0 && !f.game.campaign().world.cleared.is_empty() {
                        muted(
                            ui,
                            format!("{} freed", f.game.campaign().world.cleared.len()),
                        );
                    }
                    let alerts = f.game.strategy().active_outbreaks();
                    if alerts > 0
                        && ui
                            .add(egui::Button::new(
                                RichText::new(format!("{alerts} plague alerts")).color(RED),
                            ))
                            .clicked()
                    {
                        f.dispatch(Command::SetPaused(true), "");
                        f.battle.fire_latched = false;
                        f.go(Screen::Camp);
                        f.go_tab(Tab::Operations);
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

/// One destination in the camp navigation row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NavEntry {
    /// The Journey upgrades home (`Screen::Results`).
    Upgrades,
    Tab(Tab),
}
impl NavEntry {
    pub(super) fn label(self) -> &'static str {
        match self {
            NavEntry::Upgrades => "Journey upgrades",
            NavEntry::Tab(Tab::Briefing) => "Briefing",
            NavEntry::Tab(Tab::Armory) => "Equipment",
            NavEntry::Tab(Tab::Character) => "Character",
            NavEntry::Tab(Tab::Survivor) => "Mara",
            NavEntry::Tab(Tab::World) => "World map",
            NavEntry::Tab(Tab::Operations) => "Operations",
            // Redirected to the upgrade home; never listed.
            NavEntry::Tab(Tab::Skills | Tab::Engineering) => "Journey upgrades",
        }
    }
}

/// The single camp navigation row, in order. The upgrade home is listed while
/// no main attempt is live (and on the home itself); Mara and Operations
/// follow their unlocks.
/// The complete upgrade tree has exactly one route: the home.
pub(super) fn camp_entries(f: &UiCtx) -> Vec<NavEntry> {
    let mut entries = Vec::with_capacity(7);
    // On the home itself the tab stays listed so the row shows where you are.
    if f.game.attempt().is_none() || f.screen == Screen::Results {
        entries.push(NavEntry::Upgrades);
    }
    entries.extend([Tab::Briefing, Tab::Armory, Tab::Character].map(NavEntry::Tab));
    if f.game.campaign().has_feature(CampaignFeature::Companion) {
        entries.push(NavEntry::Tab(Tab::Survivor));
    }
    entries.push(NavEntry::Tab(Tab::World));
    if f.operations_available() {
        entries.push(NavEntry::Tab(Tab::Operations));
    }
    entries
}

pub(super) fn camp_navigation(root: &mut egui::Ui, f: &mut UiCtx) {
    if f.game.campaign().world.start.is_none() {
        return;
    }
    egui::Panel::top("camp_navigation")
        .frame(
            egui::Frame::new()
                .fill(PANEL)
                .inner_margin(egui::Margin::symmetric(18, 6)),
        )
        .show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                for entry in camp_entries(f) {
                    let selected = match entry {
                        NavEntry::Upgrades => f.screen == Screen::Results,
                        NavEntry::Tab(tab) => f.screen == Screen::Camp && f.tab == tab,
                    };
                    let label = entry.label();
                    let response = ui.selectable_label(selected, label);
                    if response.clicked() && !selected {
                        match entry {
                            NavEntry::Upgrades => f.open_upgrades(),
                            // After a round, Equipment opens on the newest find.
                            NavEntry::Tab(Tab::Armory) if f.screen == Screen::Results => {
                                f.campaign.mode =
                                    f.game.last_result().map_or(Mode::Personal, |r| r.mode);
                                f.armory.sort = super::armory::ArmorySort::Newest;
                                f.armory.selected_item =
                                    f.game.campaign().items.last().map(|i| i.id);
                                f.open_preparation(Tab::Armory);
                            }
                            NavEntry::Tab(Tab::World) => f.open_campaign(),
                            NavEntry::Tab(Tab::Operations) => f.go_tab(Tab::Operations),
                            NavEntry::Tab(tab) => f.open_preparation(tab),
                        }
                    }
                    feedback::tab_marker(
                        ui,
                        &response,
                        label,
                        selected,
                        f.settings.reduced_effects,
                    );
                }
            });
        });
}

pub(super) fn field_status(root: &mut egui::Ui, f: &mut UiCtx) {
    if !matches!(f.screen, Screen::Camp | Screen::Results) {
        return;
    }
    let fields = f.game.battles().iter().filter(|b| !b.settled).count();
    let alerts = f.game.strategy().active_outbreaks();
    if f.game.attempt().is_none() && fields == 0 && alerts == 0 {
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
                if let Some(a) = f.game.attempt() {
                    ui.strong(format!(
                        "Active defense · {}",
                        if a.mode == Mode::Personal {
                            "You"
                        } else {
                            "Mara"
                        }
                    ));
                }
                if fields > 0 {
                    ui.label(format!("{fields} live expeditions"));
                    if ui.button("View live fields").clicked() {
                        if f.screen != Screen::Camp {
                            f.dispatch(Command::SetPaused(true), "");
                        }
                        f.go(Screen::Camp);
                        f.go_tab(Tab::Operations);
                        if f.campaign.observed.is_none() {
                            f.campaign.observed =
                                f.game.battles().iter().find(|b| !b.settled).map(|b| b.id);
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
                    } else if f.game.global_paused() {
                        "All fields paused"
                    } else {
                        "World clock running"
                    },
                );
            });
        });
}
