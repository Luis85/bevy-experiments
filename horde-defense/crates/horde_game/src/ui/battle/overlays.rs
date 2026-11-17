use super::{
    BattleLayout,
    layout::{CHEVRON_BACKING, PAUSE_BANNER_FONT, cue_chevron},
    low_hp::hp_text,
};
use crate::{
    art::{AMBER, INK, RED, TEXT},
    battle_view::{BattleScreen, FieldHint},
    settings::{Action, BindingId, ExtraAction, Settings, binding_text},
    ui::{UiCtx, coins::grouped, hud, progress},
};
use bevy_egui::egui::{self, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, pos2, vec2};
use horde_core::content::Mode;

pub(super) fn corner_hud(
    ui: &mut egui::Ui,
    layout: &BattleLayout,
    f: &mut UiCtx,
    view: &hud::HudView,
) {
    let left = layout.hud_left;
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
                            hp_text(&view.defender, view.hp, view.max_hp, f.battle.low_hp),
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
    let right = layout.hud_right;
    ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| {
        egui::Frame::new()
            .fill(Color32::from_black_alpha(220))
            .stroke(Stroke::new(1., TEXT))
            .inner_margin(6)
            .show(ui, |ui| {
                ui.set_width(176.);
                // Four lines inside `hud_right`'s 86 pt: at the shared 7 pt
                // gap the card grew about 8 pt past it, onto the PAUSED tag
                // slot under it (CF-3).
                ui.spacing_mut().item_spacing.y = 3.0;
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
                        view.elapsed,
                        view.kills,
                        grouped(view.coins)
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
/// The key legend under the bar, each key named through
/// `settings::binding_text`, so one a rebind left without a key reads
/// "Unbound". A personal defense names fire, melee, reload, build, pause and
/// cycle attack; an observed one only build and pause, the keys that act
/// while observing (CF-3-31): the core runs the manual actions only in a
/// personal defense, and the observed bar has no attack selector to cycle.
pub(super) fn key_legend(settings: &Settings, mode: Mode) -> String {
    const PERSONAL: [(BindingId, &str); 6] = [
        (BindingId::Core(Action::Fire), "fire"),
        (BindingId::Core(Action::Melee), "melee"),
        (BindingId::Core(Action::Reload), "reload"),
        (BindingId::Core(Action::Build), "build"),
        (BindingId::Core(Action::Pause), "pause"),
        (BindingId::Extra(ExtraAction::CycleAttack), "cycle attack"),
    ];
    const OBSERVED: [(BindingId, &str); 2] = [
        (BindingId::Core(Action::Build), "build"),
        (BindingId::Core(Action::Pause), "pause"),
    ];
    let keys: &[(BindingId, &str)] = match mode {
        Mode::Personal => &PERSONAL,
        Mode::Npc => &OBSERVED,
    };
    keys.iter()
        .map(|&(id, action)| format!("{} {action}", binding_text(settings.binding(id))))
        .collect::<Vec<_>>()
        .join(" · ")
}
pub(super) use crate::settings::CONTROLS;
/// The bound key of the core `action`; None when it is unbound.
fn core_key(bindings: &[String], action: Action) -> Option<&str> {
    bindings
        .get(action as usize)
        .map(String::as_str)
        .filter(|key| !key.is_empty())
}
/// "<action> is unbound", naming the action as Settings › Controls does.
pub(super) fn unbound(action: Action) -> String {
    crate::settings::unbound(BindingId::Core(action))
}
/// The bound Pause key; None when it is unbound.
fn pause_key(bindings: &[String]) -> Option<&str> {
    core_key(bindings, Action::Pause)
}
/// " or <Pause key>" after "Begin defense", or nothing without a Pause key.
fn or_pause_key(bindings: &[String]) -> String {
    pause_key(bindings).map_or_else(String::new, |key| format!(" or {key}"))
}
/// The first-defense lesson for `hint`, naming the player's own keys.
/// `bindings` follows `settings::Action` order: fire, melee, reload, heal,
/// brace, build, pause. A key a rebind left unbound is never shown empty:
/// the lesson says "<action> is unbound" and where to bind it.
pub(super) fn hint_text(hint: FieldHint, paused: bool, bindings: &[String]) -> String {
    let key = |action: Action| core_key(bindings, action);
    match hint {
        FieldHint::Opening if paused => {
            let shoot = key(Action::Fire)
                .map_or_else(|| unbound(Action::Fire), |k| format!("Hold {k} to shoot"));
            let reload = key(Action::Reload)
                .map_or_else(|| unbound(Action::Reload), |k| format!("{k} reloads"));
            let strike = key(Action::Melee).map_or_else(
                || unbound(Action::Melee),
                |k| format!("{k} strikes an enemy at your line"),
            );
            let bind = match [Action::Fire, Action::Reload, Action::Melee]
                .into_iter()
                .filter(|&action| key(action).is_none())
                .count()
            {
                0 => String::new(),
                1 => format!(" Bind it in {CONTROLS}."),
                _ => format!(" Bind them in {CONTROLS}."),
            };
            format!(
                "Enemies walk in from the right. {shoot}, {reload}, {strike}.{bind} Press Begin defense{} to start.",
                or_pause_key(bindings)
            )
        }
        FieldHint::Opening | FieldHint::Shoot => match key(Action::Fire) {
            Some(k) => format!("Aim at the nearest enemy and hold {k} to shoot."),
            None => format!(
                "{}: bind it in {CONTROLS}, then aim at the nearest enemy and hold it to shoot.",
                unbound(Action::Fire)
            ),
        },
        FieldHint::Reload => match key(Action::Reload) {
            Some(k) => format!(
                "Press {k} in a quiet moment: reloading is free, an empty gun is not."
            ),
            None => format!(
                "{}: bind it in {CONTROLS}. Reloading in a quiet moment is free, an empty gun is not.",
                unbound(Action::Reload)
            ),
        },
        FieldHint::Contact => match key(Action::Melee) {
            Some(k) => format!(
                "An enemy at your line hurts you. Aim at its lane and press {k} to strike it."
            ),
            None => format!(
                "An enemy at your line hurts you. {}: bind it in {CONTROLS} to strike it.",
                unbound(Action::Melee)
            ),
        },
        FieldHint::Banked => "Coins and items are already banked. Kill the boss to win, or Retreat & train to spend them.".to_owned(),
    }
}
/// The current first-defense lesson in `area`, the layout's callout rect at
/// the top of the arena (inside the hint band over a zoomed field).
pub(super) fn field_hint_callout(
    ui: &mut egui::Ui,
    area: Rect,
    f: &mut UiCtx,
    view: &BattleScreen,
) {
    let text = hint_text(view.field.hint, view.paused, &f.settings.bindings);
    let width = area.width();
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
/// Names the paused state beside the field, so a frozen field never reads as
/// a hang: centred in the layout's banner slot, which never meets the field
/// (CF-1-01), at the size that slot holds. Returns the painted backing.
pub(super) fn pause_banner(
    ui: &egui::Ui,
    layout: &BattleLayout,
    view: &BattleScreen,
    bindings: &[String],
) -> Rect {
    let pause = pause_key(bindings);
    let text = match (view.tick == 0, pause) {
        (true, _) => format!("READY · Begin defense{}", or_pause_key(bindings)),
        (false, Some(pause)) => format!("PAUSED · {pause} to resume"),
        // Without a Pause key the bar's button is the only way back in.
        (false, None) => "PAUSED · Resume defense".to_owned(),
    };
    let slot = layout.pause_banner_slot;
    let painter = ui.painter_at(slot);
    let full = FontId::proportional(PAUSE_BANNER_FONT);
    let mut galley = painter.layout_no_wrap(text.clone(), full, AMBER);
    let (font, padding) = layout.pause_banner_fit(galley.size());
    if font < PAUSE_BANNER_FONT {
        galley = painter.layout_no_wrap(text, FontId::proportional(font), AMBER);
    }
    let backing = Rect::from_center_size(slot.center(), galley.size() + 2.0 * padding);
    painter.rect_filled(backing, 0.0, Color32::from_black_alpha(200));
    painter.galley(backing.min + padding, galley, AMBER);
    backing
}

/// The INCOMING cues over the construction overlay's left edge: a static
/// AMBER right-pointing chevron on an INK backing per cue (no motion in any
/// mode), and the word on the first cue only. Lanes are `lane_height` pt apart.
pub(super) fn incoming_cues(painter: &egui::Painter, cues: &[(u8, Pos2)], lane_height: f32) {
    for (n, &(_, at)) in cues.iter().enumerate() {
        let chevron = cue_chevron(at, lane_height);
        let backing = chevron.expand(CHEVRON_BACKING);
        painter.rect_filled(backing, 2.0, INK);
        painter.add(egui::Shape::line(
            vec![
                chevron.left_top(),
                pos2(chevron.right(), at.y),
                chevron.left_bottom(),
            ],
            Stroke::new(2.5, AMBER),
        ));
        if n == 0 {
            let (label, galley) = incoming_word(painter, backing, at);
            painter.rect_filled(label.expand2(INCOMING_WORD_PLATE), 2.0, INK);
            painter.galley(label.min, galley, AMBER);
        }
    }
}

/// How far the INK plate under the INCOMING word reaches past it.
const INCOMING_WORD_PLATE: egui::Vec2 = vec2(3.0, 1.0);

/// The INCOMING word left of the chevron `backing` of the cue at `at`: its
/// rect and galley.
fn incoming_word(
    painter: &egui::Painter,
    backing: Rect,
    at: Pos2,
) -> (Rect, std::sync::Arc<egui::Galley>) {
    let galley = painter.layout_no_wrap("INCOMING".into(), FontId::proportional(10.0), AMBER);
    let label = Rect::from_min_size(
        pos2(
            backing.left() - 4.0 - galley.size().x,
            at.y - galley.size().y * 0.5,
        ),
        galley.size(),
    );
    (label, galley)
}

/// The rects [`incoming_cues`] paints for `cues`, backings and plate
/// included: damage numbers keep clear of them (CF-7-02).
pub(super) fn incoming_cue_rects(
    painter: &egui::Painter,
    cues: &[(u8, Pos2)],
    lane_height: f32,
) -> Vec<Rect> {
    cues.iter()
        .enumerate()
        .flat_map(|(n, &(_, at))| {
            let backing = cue_chevron(at, lane_height).expand(CHEVRON_BACKING);
            let word = (n == 0).then(|| {
                incoming_word(painter, backing, at)
                    .0
                    .expand2(INCOMING_WORD_PLATE)
            });
            std::iter::once(backing).chain(word)
        })
        .collect()
}
