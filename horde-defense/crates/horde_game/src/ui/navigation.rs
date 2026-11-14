//! Task navigation and active-field wayfinding; authority stays in commands.
use super::{Screen, Tab, UiCtx, coins::grouped, feedback, muted};
use crate::{
    art::{AMBER, PANEL, RED},
    runner::SaveIntent,
};
use bevy_egui::egui::{self, Align, Color32, FontId, Layout, RichText, Stroke, pos2};
use horde_core::{
    command::Command, content::Mode, defenders::DefenderId, sim::Game,
    skill_catalog::CampaignFeature,
};

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
                // Each screen is named once. Upgrades is named by its
                // selected tab, and Settings and run history by their page
                // titles, so the header adds no subtitle there.
                let subtitle = match f.screen {
                    Screen::Battle => Some("Hold the line"),
                    Screen::Results | Screen::Upgrades | Screen::Settings | Screen::History => None,
                    _ => Some("Field command"),
                };
                if let Some(subtitle) = subtitle
                    && ui.available_width() > 670.0
                {
                    muted(ui, subtitle);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if f.screen != Screen::Settings && ui.button("Settings").clicked() {
                        f.open_settings();
                    }
                    // Camp, Results and Upgrades share one header grammar.
                    let between_runs =
                        matches!(f.screen, Screen::Camp | Screen::Results | Screen::Upgrades);
                    if between_runs && ui.button("History").clicked() {
                        f.open_history();
                    }
                    // The one manual save (W3-05): "Saved" answers as a toast.
                    if between_runs
                        && f.game.campaign().world.start.is_some()
                        && ui
                            .add_enabled(!f.status.save_pending, egui::Button::new("Save"))
                            .on_hover_text("Save the journey now")
                            .clicked()
                    {
                        f.request_save(SaveIntent::Manual);
                    }
                    if between_runs && ui.button("Menu").clicked() {
                        f.dispatch(Command::SetPaused(true), "");
                        f.request_save(SaveIntent::Menu);
                    }
                    ui.colored_label(AMBER, format!("{} coins", grouped(f.game.campaign().coins)));
                    if ui.available_width() > 230.0 && !f.game.campaign().world.cleared.is_empty() {
                        muted(
                            ui,
                            format!("{} liberated", f.game.campaign().world.cleared.len()),
                        );
                    }
                    let alerts = f.game.strategy().active_outbreaks();
                    if alerts > 0
                        && ui
                            .add(egui::Button::new(
                                RichText::new(super::chrome::count_noun(
                                    alerts,
                                    "plague alert",
                                    "plague alerts",
                                ))
                                .color(RED),
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
    /// The upgrade tree (`Screen::Upgrades`).
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
            NavEntry::Tab(Tab::Home) => "Camp",
            NavEntry::Tab(Tab::Survivor) => CampaignFeature::Companion.label(),
            NavEntry::Tab(Tab::World) => "World map",
            NavEntry::Tab(Tab::Operations) => "Operations",
            // Redirected to Upgrades; never listed.
            NavEntry::Tab(Tab::Skills | Tab::Engineering) => "Journey upgrades",
        }
    }
}

/// The single camp navigation row, in order: Camp home, Upgrades,
/// then the Prepare pages and the world map (design-ux-flow decision 5). The
/// Upgrades entry is listed while no main attempt is live (and on Upgrades
/// itself). Mara and Operations are drill-downs under Camp home, not entries.
/// The complete upgrade tree has exactly one route: the home.
pub(super) fn camp_entries(f: &UiCtx) -> Vec<NavEntry> {
    let mut entries = Vec::with_capacity(6);
    entries.push(NavEntry::Tab(Tab::Home));
    // On Upgrades itself the entry stays listed so the row shows where you are.
    if f.game.attempt().is_none() || f.screen == Screen::Upgrades {
        entries.push(NavEntry::Upgrades);
    }
    entries.extend([Tab::Briefing, Tab::Armory, Tab::Character, Tab::World].map(NavEntry::Tab));
    entries
}

/// Whether `entry` is the selected row entry. Camp stays selected on its
/// drill-downs (Mara, Operations).
fn entry_selected(f: &UiCtx, entry: NavEntry) -> bool {
    match entry {
        NavEntry::Upgrades => f.screen == Screen::Upgrades,
        NavEntry::Tab(Tab::Home) => {
            f.screen == Screen::Camp && (f.tab == Tab::Home || f.tab.is_home_drill_down())
        }
        NavEntry::Tab(tab) => f.screen == Screen::Camp && f.tab == tab,
    }
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
            super::chrome::row(ui, |ui| {
                for entry in camp_entries(f) {
                    let selected = entry_selected(f, entry);
                    let label = entry.label();
                    let response = ui.selectable_label(selected, label);
                    // Camp also leads back to Home from its drill-downs.
                    let home_from_drill_down =
                        entry == NavEntry::Tab(Tab::Home) && f.tab != Tab::Home;
                    if response.clicked() && (!selected || home_from_drill_down) {
                        match entry {
                            NavEntry::Upgrades => f.open_upgrades(),
                            NavEntry::Tab(Tab::Home) => f.open_camp_home(None),
                            // After a round, Equipment opens on the newest find.
                            NavEntry::Tab(Tab::Armory)
                                if f.screen == Screen::Upgrades
                                    && f.game.last_result().is_some() =>
                            {
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
    // The one unlock callout due, under the row (UX-16).
    super::onboarding::callout_row(root, f);
}

/// "Active defense · <who>" while a main defense is on the field: "You" for
/// the player, otherwise the defender's name from its core card. The mode
/// does not name anyone: Mara and every recruit defend under `Mode::Npc`.
pub(super) fn active_defense_label(game: &Game) -> Option<String> {
    let attempt = game.attempt()?;
    let who = match attempt.defender() {
        DefenderId::Player => "You".to_owned(),
        defender => game
            .defender_card(defender)
            .map_or_else(|| "Survivor".to_owned(), |card| card.name),
    };
    Some(format!("Active defense · {who}"))
}

pub(super) fn field_status(root: &mut egui::Ui, f: &mut UiCtx) {
    if !matches!(f.screen, Screen::Camp | Screen::Results | Screen::Upgrades) {
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
            super::chrome::row(ui, |ui| {
                if let Some(label) = active_defense_label(f.game) {
                    ui.strong(label);
                }
                if fields > 0 {
                    ui.label(super::chrome::count_noun(
                        fields,
                        "live expedition",
                        "live expeditions",
                    ));
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
                    ui.colored_label(
                        RED,
                        if alerts == 1 {
                            "1 outbreak needs a response".to_owned()
                        } else {
                            format!("{alerts} outbreaks need a response")
                        },
                    );
                }
                muted(
                    ui,
                    if f.screen == Screen::Upgrades {
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
