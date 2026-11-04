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
            .campaign()
            .selected_survivor
            .map_or("You", SurvivorId::name)
    } else {
        &f.game.campaign().npc.name
    }
}
pub(super) fn mode_picker(ui: &mut egui::Ui, f: &mut UiCtx) {
    if !f
        .game
        .campaign()
        .has_feature(horde_core::skill_catalog::CampaignFeature::Companion)
    {
        f.campaign.mode = Mode::Personal;
        return;
    }
    let previous = f.campaign.mode;
    ui.horizontal_wrapped(|ui| {
        ui.label("Defender");
        ui.selectable_value(&mut f.campaign.mode, Mode::Personal, "You");
        let available = f.game.campaign().npc_available();
        ui.add_enabled_ui(available, |ui| {
            ui.selectable_value(
                &mut f.campaign.mode,
                Mode::Npc,
                f.game.campaign().npc.name.clone(),
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
/// Height of a padded button or combo box. Wrapped filter rows set their
/// interact height to this before adding widgets: egui centres each widget in
/// the row height known when it is placed, so a shorter start leaves the first
/// widget above its taller siblings.
pub(super) fn filter_row_height(ui: &egui::Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.y
}
/// Starts a wrapped filter row whose widgets share one centre line.
pub(super) fn begin_filter_row(ui: &mut egui::Ui) -> f32 {
    let row = filter_row_height(ui);
    ui.spacing_mut().interact_size.y = row;
    ui.set_min_height(row);
    row
}
/// Sentence-case label for an authored snake_case or kebab-case content id,
/// such as a stage theme's weather (`ground_fog` → `Ground fog`) or lighting.
/// Theme ids have no separate authored display name, so presentation derives one.
pub(super) fn display_label(id: &str) -> String {
    let words: Vec<&str> = id
        .split(['_', '-', ' '])
        .filter(|word| !word.is_empty())
        .collect();
    let mut label = words.join(" ").to_lowercase();
    if let Some(first) = label.chars().next() {
        label.replace_range(..first.len_utf8(), &first.to_uppercase().to_string());
    }
    label
}

#[cfg(test)]
mod tests {
    use super::display_label;

    #[test]
    fn display_label_humanises_authored_theme_ids() {
        assert_eq!(display_label("ground_fog"), "Ground fog");
        assert_eq!(display_label("light_rain"), "Light rain");
        assert_eq!(display_label("dawn"), "Dawn");
        assert_eq!(display_label("sea-mist"), "Sea mist");
        assert_eq!(display_label("_double__under_"), "Double under");
        assert_eq!(display_label("Overcast"), "Overcast");
        assert_eq!(display_label(""), "");
    }

    #[test]
    fn every_authored_weather_and_lighting_id_has_a_readable_label() {
        let themes = &horde_core::world::world().stage_themes;
        assert!(!themes.is_empty());
        for theme in themes {
            for id in [&theme.weather, &theme.lighting] {
                let label = display_label(id);
                assert!(!label.is_empty() && !label.contains('_'), "{id} -> {label}");
            }
        }
    }
}
