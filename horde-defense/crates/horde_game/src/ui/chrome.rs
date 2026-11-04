//! Shared native presentation grammar. No simulation or persistence operations.
use super::UiCtx;
use crate::art::MUTED;
use bevy_egui::egui::{self, FontId, RichText};
use horde_core::{content::Mode, survivors::SurvivorId};

pub(super) fn page_title(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.label(
        RichText::new(title).font(FontId::new(28.0, egui::FontFamily::Name("display".into()))),
    );
    if !description.is_empty() {
        ui.label(RichText::new(description).color(MUTED));
    }
    ui.add_space(12.0);
}
pub(super) fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(12.0);
    ui.label(RichText::new(title).size(18.0).strong());
    ui.add_space(4.0);
}
pub(super) fn defender_name<'a>(f: &UiCtx<'a>) -> &'a str {
    if f.campaign.mode == Mode::Personal {
        f.game
            .campaign
            .selected_survivor
            .map_or("You", SurvivorId::name)
    } else {
        &f.game.campaign.npc.name
    }
}
pub(super) fn mode_picker(ui: &mut egui::Ui, f: &mut UiCtx) {
    if !f
        .game
        .campaign
        .has_feature(horde_core::skill_catalog::CampaignFeature::Companion)
    {
        f.campaign.mode = Mode::Personal;
        return;
    }
    let previous = f.campaign.mode;
    ui.horizontal_wrapped(|ui| {
        ui.label("Defender");
        ui.selectable_value(&mut f.campaign.mode, Mode::Personal, "You");
        let available = f.game.campaign.npc_available();
        ui.add_enabled_ui(available, |ui| {
            ui.selectable_value(
                &mut f.campaign.mode,
                Mode::Npc,
                f.game.campaign.npc.name.clone(),
            );
        })
        .response
        .on_disabled_hover_text(
            "Unlock Mara on the journey upgrade tree after the required country milestone.",
        );
    });
    if previous != f.campaign.mode {
        f.campaign.improvement_focus = None;
    }
}
