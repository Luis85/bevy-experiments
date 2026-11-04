//! Primary home after every main round. All rewards have already been settled.
use super::{Tab, UiCtx, armory, muted, progression, status};
use crate::{
    art::{AMBER, INK, MUTED, TEXT},
    runner::SaveIntent,
};
use bevy_egui::egui::{self, Align, Color32, Layout, RichText, Stroke, vec2};
use horde_core::campaign::UpgradeFocus;
use horde_core::skill_catalog::CampaignFeature;
use horde_core::{command::Command, content::Mode, sim::Outcome, world::world};
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
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK).inner_margin(14))
        .show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Journey upgrades").size(26.0).strong());
                ui.colored_label(AMBER, format!("{} coins", f.game.campaign().coins));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Settings").clicked() {
                        f.open_settings();
                    }
                });
            });
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
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new(continue_label(f)).strong())
                            .fill(Color32::BLACK)
                            .stroke(Stroke::new(2.0, TEXT))
                            .min_size(vec2(200.0, 40.0)),
                    )
                    .clicked()
                {
                    if f.game.campaign().journey_active && f.game.last_result().is_some() {
                        f.dispatch(Command::ContinueJourney, "Journey continued.");
                    } else {
                        f.open_campaign();
                    }
                }
                if ui.button("Leave journey").clicked() {
                    f.dispatch(Command::LeaveJourney, "");
                    f.status.save_pending = true;
                }
                if ui.button("Equipment").clicked() {
                    f.campaign.mode = f.game.last_result().map_or(Mode::Personal, |r| r.mode);
                    f.armory.sort = armory::ArmorySort::Newest;
                    f.armory.selected_item = f.game.campaign().items.last().map(|i| i.id);
                    f.open_preparation(Tab::Armory);
                }
                if ui.button("Run analysis").clicked() {
                    f.open_history();
                }
                ui.menu_button("More", |ui| {
                    if ui.button("World map").clicked() {
                        f.open_campaign();
                        ui.close();
                    }
                    if (f.game.campaign().legacy_features || f.game.campaign().victories > 0)
                        && ui.button("Plan or replay a defense").clicked()
                    {
                        f.open_preparation(Tab::Briefing);
                        ui.close();
                    }
                    if ui.button("Character stats").clicked() {
                        f.open_preparation(Tab::Character);
                        ui.close();
                    }
                    for (feature, label, tab) in [
                        (
                            CampaignFeature::Construction,
                            "Fortifications",
                            Tab::Engineering,
                        ),
                        (CampaignFeature::Companion, "Mara", Tab::Survivor),
                        (CampaignFeature::Operations, "Operations", Tab::Operations),
                    ] {
                        if (if feature == CampaignFeature::Operations {
                            f.operations_available()
                        } else {
                            f.game.campaign().has_feature(feature)
                        }) && ui.button(label).clicked()
                        {
                            if feature == CampaignFeature::Construction {
                                f.campaign.improvement_focus = f
                                    .game
                                    .campaign()
                                    .recommendation_fallback(UpgradeFocus::Fortification)
                                    .map(|r| r.action);
                            }
                            f.open_preparation(tab);
                            ui.close();
                        }
                    }
                    if ui.button("Save journey").clicked() {
                        f.request_save(SaveIntent::Manual);
                        ui.close();
                    }
                });
            });
            if f.status.message_error || f.status.save_pending {
                status(ui, f);
            }
            let height = (ui.clip_rect().bottom() - ui.cursor().top())
                .min(ui.available_height())
                .max(150.0);
            progression::skill_tree(ui, f, height);
        });
}
