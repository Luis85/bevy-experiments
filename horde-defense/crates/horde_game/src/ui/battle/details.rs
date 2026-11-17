use super::overlays::{arrival_groups, hint_text};
use crate::{
    art::MUTED,
    battle_view::BattleScreen,
    ui::{UiCtx, chrome, hud, muted},
};
use bevy_egui::egui::{self, RichText};

pub(super) fn field_details(
    ui: &mut egui::Ui,
    f: &mut UiCtx,
    view: &BattleScreen,
    hud_view: &hud::HudView,
) {
    let field = &view.field;
    muted(
        ui,
        format!(
            "{} lanes · threat {} · {} hostiles · ordinary cap {} (+ boss) · {:.2} enemies/s",
            view.lanes, field.threat_rank, field.hostiles, field.enemy_cap, field.arrival_rate
        ),
    );
    muted(
        ui,
        format!(
            "{:.0} yd path · {} difficulty · {} · new gear Lv{}",
            view.width_yards,
            field.difficulty,
            arrival_groups(field.batch_size),
            field.gear_level
        ),
    );
    hud::dps(ui, &hud_view.dps);
    muted(ui, field.level_name);
    if view.personal() {
        muted(
            ui,
            format!(
                "Selected lane {} · melee {:.1}s",
                view.selected_lane(f.battle.aim),
                view.melee_cooldown as f32 / 60.0
            ),
        );
    }
    if f.settings.show_guidance && view.first_deployment {
        chrome::section(ui, "Your first defense");
        let hint = hint_text(field.hint, view.paused, &f.settings.bindings);
        ui.label(hint);
        if ui.small_button("Hide field hints").clicked() {
            f.settings.show_guidance = false;
        }
    }
    egui::CollapsingHeader::new("Shared line rule")
        .default_open(f.settings.show_guidance && view.first_deployment)
        .show(ui, |ui| {
            ui.label("Enemies at any part of the line can attack you. Aim at a contact lane to strike back.");
        });
    if let Some(theme) = &field.setting {
        egui::CollapsingHeader::new("Local setting").show(ui, |ui| {
            ui.strong(&theme.title);
            ui.label(&theme.setting);
            muted(
                ui,
                format!(
                    "{} · {}",
                    chrome::display_label(&theme.weather),
                    chrome::display_label(&theme.lighting)
                ),
            );
            muted(ui, format!("Field props: {}", theme.props.join(", ")));
        });
    }
    if f.battle.inspect_log {
        chrome::section(ui, "Resolution log");
        for line in &view.log_tail {
            ui.label(RichText::new(line).size(11.0).color(MUTED));
        }
    }
}
