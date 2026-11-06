use crate::{
    art::{AMBER, RED, TEXT},
    battle_view::{BattleScreen, FieldHint},
    ui::{UiCtx, hud, progress},
};
use bevy_egui::egui::{self, Color32, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};

pub(super) fn corner_hud(ui: &mut egui::Ui, canvas: Rect, f: &mut UiCtx, view: &hud::HudView) {
    let left = Rect::from_min_size(canvas.min + vec2(14., 12.), vec2(322., 108.));
    ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| {
        egui::Frame::new()
            .fill(Color32::from_black_alpha(220))
            .stroke(Stroke::new(1., TEXT))
            .inner_margin(5)
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let (portrait, _) = ui.allocate_exact_size(vec2(40., 40.), Sense::hover());
                    crate::visual_assets::portrait(
                        ui.painter(),
                        f.textures,
                        view.female_portrait,
                        portrait.center(),
                        portrait.size(),
                    );
                    ui.vertical(|ui| {
                        ui.set_width(250.);
                        progress(
                            ui,
                            view.hp as f32 / view.max_hp as f32,
                            format!("{} · HP {} / {}", view.defender, view.hp, view.max_hp),
                            RED,
                        );
                        ui.horizontal(|ui| {
                            ui.strong(format!("{} / {} ammo", view.ammo, view.magazine));
                            ui.label(
                                RichText::new(format!(
                                    "{} {:.0}/{:.0}",
                                    view.stamina_label, view.stamina, view.max_stamina
                                ))
                                .size(12.0),
                            );
                        });
                        hud::action(ui, &view.action);
                        ui.horizontal(|ui| {
                            hud::danger(ui, &view.danger);
                            hud::dps(ui, &view.dps);
                        });
                    });
                });
            });
    });
    let right = Rect::from_min_size(
        pos2(canvas.right() - 206., canvas.top() + 12.),
        vec2(192., 86.),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| {
        egui::Frame::new()
            .fill(Color32::from_black_alpha(220))
            .stroke(Stroke::new(1., TEXT))
            .inner_margin(6)
            .show(ui, |ui| {
                ui.set_width(176.);
                match view.boss {
                    hud::BossReadout::Present {
                        health,
                        critical,
                        hp,
                    } => progress(
                        ui,
                        health,
                        if critical {
                            format!("CRITICAL · {hp} HP")
                        } else {
                            format!("BOSS · {hp} HP")
                        },
                        RED,
                    ),
                    hud::BossReadout::Incoming { seconds } => {
                        ui.strong(format!("BOSS IN {:02}:{:02}", seconds / 60, seconds % 60));
                    }
                }
                ui.label(
                    RichText::new(format!(
                        "{:.0}s · {} kills · +{} coins",
                        view.elapsed, view.kills, view.coins
                    ))
                    .size(12.),
                );
                ui.label(
                    RichText::new(format!(
                        "{} lanes · {:.0} yd · threat {}",
                        view.lanes, view.width_yards, view.threat_rank
                    ))
                    .size(10.0),
                )
                .on_hover_text(format!(
                    "{} difficulty, frozen at deployment",
                    view.difficulty
                ));
                ui.label(
                    RichText::new(format!(
                        "{} · gear Lv{}",
                        arrival_groups(view.batch_size),
                        view.gear_level
                    ))
                    .size(10.0),
                );
            });
    });
}
/// How enemies arrive, in words: "single arrivals" or "groups of 3".
pub(super) fn arrival_groups(size: u32) -> String {
    match size {
        0 | 1 => "single arrivals".to_owned(),
        size => format!("groups of {size}"),
    }
}
/// The first-defense lesson for `hint`, naming the player's own keys.
/// `bindings` follows `settings::Action` order: fire, melee, reload, heal,
/// brace, build, pause.
pub(super) fn hint_text(hint: FieldHint, paused: bool, bindings: &[String]) -> String {
    let key = |index: usize| bindings.get(index).map_or("?", String::as_str);
    match hint {
        FieldHint::Opening if paused => format!(
            "Enemies walk in from the right. Hold {} to shoot, {} reloads, {} strikes an enemy at your line. Press Begin defense or {} to start.",
            key(0),
            key(2),
            key(1),
            key(6)
        ),
        FieldHint::Opening | FieldHint::Shoot => format!(
            "Aim at the nearest enemy and hold {} to shoot.",
            key(0)
        ),
        FieldHint::Reload => format!(
            "Press {} in a quiet moment: reloading is free, an empty gun is not.",
            key(2)
        ),
        FieldHint::Contact => format!(
            "An enemy at your line hurts you. Aim at its lane and press {} to strike it.",
            key(1)
        ),
        FieldHint::Banked => "Coins and items are already banked. Kill the boss to win, or Retreat & train to spend them.".to_owned(),
    }
}
/// The current first-defense lesson as one line over the top of the field.
pub(super) fn field_hint_callout(
    ui: &mut egui::Ui,
    arena: Rect,
    f: &mut UiCtx,
    view: &BattleScreen,
) {
    let text = hint_text(view.field.hint, view.paused, &f.settings.bindings);
    let width = (arena.width() - 24.0).clamp(160.0, 620.0);
    let area = Rect::from_min_size(
        pos2(arena.center().x - width * 0.5, arena.top() + 4.0),
        vec2(width, 84.0),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
        egui::Frame::new()
            .fill(Color32::from_black_alpha(215))
            .stroke(Stroke::new(1.0, AMBER))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                // Text above, the opt-out under it: one column fits every width.
                ui.set_width(width - 22.0);
                ui.add(egui::Label::new(RichText::new(text).size(13.0).color(TEXT)).wrap());
                if ui.small_button("Hide hints").clicked() {
                    f.settings.show_guidance = false;
                }
            });
    });
}
/// Names the paused state on the field, so a frozen field never reads as a
/// hang.
pub(super) fn pause_banner(ui: &egui::Ui, arena: Rect, view: &BattleScreen, bindings: &[String]) {
    let pause = bindings.get(6).map_or("Escape", String::as_str);
    let text = if view.tick == 0 {
        format!("READY · Begin defense or {pause}")
    } else {
        format!("PAUSED · {pause} to resume")
    };
    let painter = ui.painter_at(arena);
    let galley = painter.layout_no_wrap(text, FontId::proportional(18.0), AMBER);
    let at = pos2(arena.center().x, arena.bottom() - 18.0) - galley.size() * 0.5;
    painter.rect_filled(
        Rect::from_min_size(at - vec2(10.0, 5.0), galley.size() + vec2(20.0, 10.0)),
        0.0,
        Color32::from_black_alpha(200),
    );
    painter.galley(at, galley, AMBER);
}
