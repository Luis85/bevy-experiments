//! Battle presentation. Game changes are queued typed intents. Every attempt
//! value laid out here comes from `battle_view::BattleScreen` (or the HUD view
//! model); the attempt itself is only handed to the battlefield painter and
//! the inspector tools.
use super::{
    Screen, UiCtx, armory, chrome, damage, feedback, hud, loot, muted, policy, progress, reticle,
    status,
};
use crate::{
    art::{self, AMBER, GREEN, INK, MUTED, RED, TEXT},
    battle_view::{self, BattleScreen, FieldHint},
    diagnostics::{self, InterfaceSpan},
    runner::SaveIntent,
    settings::Action,
};
use bevy_egui::egui::{self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};
use horde_core::{
    command::Command,
    content::{Mode, StructureKind, content},
    ordnance::{OrdnanceKind, ThrowRequest},
    sim::Input,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum AttackSelection {
    #[default]
    Gun,
    Grenade,
    Molotov,
}
impl AttackSelection {
    pub(super) fn ordnance(self) -> Option<OrdnanceKind> {
        match self {
            Self::Gun => None,
            Self::Grenade => Some(OrdnanceKind::Grenade),
            Self::Molotov => Some(OrdnanceKind::Molotov),
        }
    }
}
fn target_cell(aim: [f32; 2], lanes: u8, width: f32) -> Option<[u8; 2]> {
    (aim[0].is_finite()
        && aim[1].is_finite()
        && (0.0..width).contains(&aim[0])
        && (0.0..f32::from(lanes)).contains(&aim[1]))
    .then(|| [aim[0].floor() as u8, aim[1].floor() as u8])
    .filter(|cell| horde_core::ordnance::valid_cell_for_arena(*cell, lanes, width))
}

fn projection(f: &UiCtx, r: Rect) -> art::ArenaProjection {
    let (lanes, width) = battle_view::arena_geometry(f.game);
    art::ArenaProjection::with_geometry(r, lanes, width).with_effects_clip(
        f.battle
            .effects_rect
            .filter(|clip| clip.contains_rect(r))
            .unwrap_or(r),
    )
}

pub(super) fn controls(ctx: &egui::Context, f: &mut UiCtx) {
    let frame = ctx.cumulative_frame_nr();
    if f.battle.last_control_frame == frame {
        return;
    }
    f.battle.last_control_frame = frame;
    if feedback::overlay_owns_keyboard(ctx, f) {
        return;
    }
    if f.settings.pressed(ctx, Action::Pause, false) {
        let pause = !BattleScreen::build(f.game).is_some_and(|view| view.paused);
        f.dispatch(Command::SetPaused(pause), "");
        f.battle.construction = false;
        f.battle.fire_latched = false;
    }
    if f.settings.pressed(ctx, Action::Build, false) {
        if f.game.campaign().blueprints.iter().any(|owned| *owned) {
            f.battle.construction = !f.battle.construction;
        } else {
            f.battle.construction = false;
            f.status.message = format!(
                "{} Combat continues; use Pause when you need time.",
                f.game.campaign().construction_blocker().unwrap_or_default()
            );
            f.status.message_error = false;
            f.status.feedback_revision = f.status.feedback_revision.wrapping_add(1);
        }
        f.battle.fire_latched = false;
    }
    if f.battle.attack_selection == AttackSelection::Gun
        && f.settings.pressed(ctx, Action::Toggle, false)
    {
        f.toggle_fire(ctx);
    }
}
pub(super) fn gather_combat_input(ctx: &egui::Context, f: &mut UiCtx) -> Input {
    if f.battle.construction
        || ctx.text_edit_focused()
        || ctx.any_popup_open()
        || f.status.feedback.details_open
    {
        return Input {
            aim: f.battle.aim,
            auto_reload: f.settings.auto_reload,
            ..Input::default()
        };
    }
    let view = BattleScreen::build(f.game);
    let view = view.as_ref();
    let mut input = Input {
        aim: assisted_aim(f, view),
        auto_reload: f.settings.auto_reload,
        ..Input::default()
    };
    let fire_allowed =
        f.battle.arena_hover || !f.settings.bindings[Action::Fire as usize].starts_with("Mouse");
    if let Some(kind) = f.battle.attack_selection.ordnance() {
        // Throwing consumes a press edge, never a held button or firing latch.
        f.battle.fire_latched = false;
        let (lanes, width) = battle_view::arena_geometry(f.game);
        if let Some(cell) =
            raw_press_aim(ctx, f, Action::Fire).and_then(|aim| target_cell(aim, lanes, width))
        {
            f.battle
                .pending
                .throw
                .get_or_insert(ThrowRequest { kind, cell });
        }
    } else {
        let fire_press = press_aim(ctx, f, view, Action::Fire);
        if f.settings.toggle_fire && fire_press.is_some() {
            f.toggle_fire(ctx);
        }
        input.fire = f.battle.fire_latched
            || (!f.settings.toggle_fire
                && fire_allowed
                && f.settings.pressed(ctx, Action::Fire, true));
        if !f.settings.toggle_fire
            && let Some(aim) = fire_press
        {
            f.battle.pending.fire = true;
            f.battle.pending_attack_aims.fire.get_or_insert(aim);
        }
    }
    if let Some(aim) = press_aim(ctx, f, view, Action::Melee) {
        f.battle.pending.melee = true;
        f.battle.pending_attack_aims.melee.get_or_insert(aim);
    }
    input.melee |= f.settings.pressed(ctx, Action::Melee, true)
        && (f.battle.arena_hover
            || !f.settings.bindings[Action::Melee as usize].starts_with("Mouse"));
    input.reload |= f.settings.pressed(ctx, Action::Reload, false);
    input.heal |= f.settings.pressed(ctx, Action::Heal, false);
    input.brace |= f.settings.pressed(ctx, Action::Brace, false);
    input
}
fn assisted_aim(f: &UiCtx, view: Option<&BattleScreen>) -> [f32; 2] {
    assist_at(f, view, f.battle.aim)
}
fn press_aim(
    ctx: &egui::Context,
    f: &UiCtx,
    view: Option<&BattleScreen>,
    action: Action,
) -> Option<[f32; 2]> {
    raw_press_aim(ctx, f, action).map(|aim| assist_at(f, view, aim))
}
fn raw_press_aim(ctx: &egui::Context, f: &UiCtx, action: Action) -> Option<[f32; 2]> {
    let binding = &f.settings.bindings[action as usize];
    let button = match binding.as_str() {
        "Mouse1" => Some(egui::PointerButton::Primary),
        "Mouse2" => Some(egui::PointerButton::Secondary),
        "Mouse3" => Some(egui::PointerButton::Middle),
        _ => None,
    };
    if let Some(button) = button {
        let position = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: pressed,
                    pressed: true,
                    ..
                } if *pressed == button => Some(*pos),
                _ => None,
            })
        });
        if let Some(position) = position
            && let Some(arena) = f.battle.arena_rect
        {
            return arena
                .contains(position)
                .then(|| projection(f, arena).inverse(position));
        }
        return (f.battle.arena_hover && f.settings.pressed(ctx, action, false))
            .then_some(f.battle.aim);
    }
    f.settings
        .pressed(ctx, action, false)
        .then_some(f.battle.aim)
}
fn assist_at(f: &UiCtx, view: Option<&BattleScreen>, aim: [f32; 2]) -> [f32; 2] {
    view.map_or(aim, |view| view.assist(aim, f.settings.aim_assist))
}
fn reticle_ownership(preview: bool, hovered: bool, focused: bool, covered: bool) -> Option<bool> {
    if preview {
        Some(false)
    } else if hovered && focused && !covered {
        Some(true)
    } else {
        None
    }
}
fn preview_aim(arena: art::ArenaProjection) -> [f32; 2] {
    [(arena.width * 0.6).min(18.0), f32::from(arena.lanes) * 0.5]
}
pub(super) fn battle(root: &mut egui::Ui, f: &mut UiCtx) {
    let game = f.game;
    let perf = f.perf;
    let built = {
        let _span = diagnostics::interface_span(perf, InterfaceSpan::BattleFrameBuild);
        (
            BattleScreen::build(game),
            hud::view_model(game),
            game.attempt(),
        )
    };
    let (Some(view), Some(hud_view), Some(a)) = built else {
        f.go(Screen::Results);
        return;
    };
    let ctx = root.ctx().clone();
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK))
        .show(root, |ui| {
            let canvas = ui.max_rect();
            let maximum = battle_arena(canvas, f.battle.construction);
            let r = clearing_rect(maximum, view.lanes, view.width_yards);
            f.battle.effects_rect = Some(maximum);
            let arena = projection(f, r);
            let response = ui.interact(
                r,
                egui::Id::new("pixel_battle_arena"),
                Sense::click_and_drag(),
            );
            f.battle.arena_rect = Some(r);
            f.battle.arena_hover = response.hovered();
            let diagnostic_reticle = f.launch.reticle_capture_preview();
            if diagnostic_reticle {
                f.battle.aim = preview_aim(arena);
            } else if let Some(pointer) = response.hover_pos() {
                f.battle.aim = arena.inverse(pointer);
            }
            {
                let _span = diagnostics::interface_span(perf, InterfaceSpan::Backdrop);
                art::world_backdrop(ui.painter(), canvas, arena, view.theme, f.textures);
            }
            let paint = diagnostics::interface_span(perf, InterfaceSpan::BattlefieldPaint);
            // The painter builds its own `BattleViewModel` from the attempt.
            let drawn = art::battlefield(
                &ui.painter_at(maximum),
                r,
                a,
                f.battle.aim,
                art::RenderOptions {
                    construction: f.battle.construction,
                    outlines: f.settings.outlines,
                    numbers: f.settings.numbers,
                    reduced: f.settings.reduced_effects,
                    blood: f.settings.blood,
                    theme: view.theme,
                    textures: f.textures,
                    blend: f.battle.interpolation.scene(None, a),
                },
            );
            drop(paint);
            if let Some(perf) = perf {
                perf.record_scene(drawn);
            }
            art::world_atmosphere(ui.painter(), canvas);
            crate::inspector::hover(&response, a, f.battle.aim);
            if response.hovered() {
                if f.battle.construction {
                    placement_cursor(ui, r, f);
                } else if f.battle.attack_selection.ordnance().is_some() && view.personal() {
                    ordnance_cursor(ui, r, f, &view);
                }
            }
            if let Some(owns_cursor) = reticle_ownership(
                diagnostic_reticle,
                response.hovered(),
                ui.input(|i| i.raw.focused),
                ctx.any_popup_open() || f.status.feedback.details_open,
            ) {
                // The OS crosshair marks the aim with no presentation lag;
                // the painted readout follows it. Captures have no OS
                // cursor, so the diagnostic preview paints the cross.
                if owns_cursor {
                    ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                }
                reticle::paint(
                    ui,
                    &view.reticle(f.battle.attack_selection.ordnance()),
                    arena,
                    f.battle.aim,
                    reticle::Marks {
                        cross: !owns_cursor,
                        building: f.battle.construction,
                        reduced: f.settings.reduced_effects,
                    },
                );
            }
            damage::draw(ui, r, f);
            loot::draw(ui, r, f);
            // Both sit in the field margin around the clearing, so neither
            // covers the defender or the other at the minimum size.
            if view.paused && !f.battle.construction {
                pause_banner(ui, maximum, &view, &f.settings.bindings);
            }
            if f.settings.show_guidance && view.first_deployment && view.personal() {
                field_hint_callout(ui, maximum, f, &view);
            }
            corner_hud(ui, canvas, f, &hud_view);
            if let Some(receipt) = &view.last_loot {
                let area = Rect::from_min_max(
                    pos2(canvas.left() + 350.0, canvas.top() + 72.0),
                    pos2(canvas.right() - 222.0, r.top() - 8.0),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                    ui.set_clip_rect(area.intersect(ui.clip_rect()));
                    ui.label(
                        RichText::new("Last loot · already banked")
                            .size(10.0)
                            .color(MUTED),
                    );
                    armory::loot_receipt(ui, receipt);
                });
            }
            if view.personal() && !f.battle.construction {
                let selector = attack_selector_rect(canvas, r);
                ui.scope_builder(egui::UiBuilder::new().max_rect(selector), |ui| {
                    ui.set_clip_rect(selector.intersect(ui.clip_rect()));
                    attack_selector(ui, f, &view)
                });
            }
            if f.battle.construction {
                let panel = Rect::from_min_size(
                    pos2(canvas.right() - 296.0, r.top() + 8.0),
                    vec2(280.0, (r.height() - 16.0).max(180.0)),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(panel), |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_black_alpha(235))
                        .stroke(Stroke::new(1., TEXT))
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.set_width(258.0);
                            egui::ScrollArea::vertical()
                                .max_height(panel.height() - 24.0)
                                .show(ui, |ui| construction_controls(ui, f, &view));
                        });
                });
            }
            // The controls are always named; a new player has no other
            // place to learn the defaults.
            {
                ui.painter().text(
                    pos2(canvas.left() + 12., canvas.bottom() - 2.),
                    Align2::LEFT_BOTTOM,
                    format!(
                        "{} fire · {} melee · {} reload · {} build · {} pause",
                        f.settings.bindings[0],
                        f.settings.bindings[1],
                        f.settings.bindings[2],
                        f.settings.bindings[5],
                        f.settings.bindings[6]
                    ),
                    FontId::proportional(10.),
                    MUTED,
                );
            }
            let bar = Rect::from_min_max(
                pos2(canvas.left() + 12., canvas.bottom() - 76.),
                pos2(canvas.right() - 12., canvas.bottom() - 5.),
            );
            let observed = view.mode == Mode::Npc;
            ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
                ui.horizontal_wrapped(|ui| {
                    let paused = view.paused;
                    let label = if !paused {
                        "Pause"
                    } else if view.tick == 0 {
                        "Begin defense"
                    } else {
                        "Resume defense"
                    };
                    if ui.button(RichText::new(label).strong()).clicked() {
                        f.dispatch(Command::SetPaused(!paused), "");
                        f.battle.construction = false;
                        f.battle.fire_latched = false;
                    }
                    if view.personal() && !f.battle.construction {
                        personal_controls(ui, f, &ctx, &view);
                    }
                    if view.any_blueprint()
                        && ui
                            .button(if f.battle.construction {
                                "Close construction"
                            } else {
                                "Build defenses"
                            })
                            .clicked()
                    {
                        f.battle.construction = !f.battle.construction;
                        f.battle.fire_latched = false;
                    }
                    if observed {
                        ui.label("Observe");
                        for speed in [1, 2, 4] {
                            ui.selectable_value(&mut f.battle.speed, speed, format!("{speed}×"));
                        }
                        let response = ui.button("Survivor");
                        let mut popup = egui::Popup::menu(&response)
                            .width(300.0)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
                        if f.launch.reviewing("npc-expanded") {
                            popup = popup.open(true);
                        }
                        popup.show(|ui| {
                            ui.set_width(280.0);
                            ui.set_max_width(280.0);
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                            egui::ScrollArea::vertical()
                                .max_height((canvas.height() - 160.0).max(160.0))
                                .show(ui, |ui| {
                                    crate::inspector::npc_decision(
                                        ui,
                                        a,
                                        f.launch.reviewing("npc-expanded"),
                                    );
                                    policy(ui, f);
                                });
                        });
                    }
                    if ui
                        .button(if observed {
                            "Recall & train"
                        } else {
                            "Retreat & train"
                        })
                        .on_hover_text(
                            "Withdraw to Journey upgrades. Every earned coin and item is retained.",
                        )
                        .clicked()
                    {
                        f.battle.fire_latched = false;
                        f.dispatch(
                            Command::Withdraw,
                            "Rewards retained. Rest, train, and retry.",
                        );
                    }
                    ui.menu_button("Management", |ui| {
                        ui.set_max_width(320.);
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        if ui.button("Settings").clicked() {
                            f.open_settings();
                        }
                        if ui.button("Save deployment").clicked() {
                            f.request_save(SaveIntent::Manual);
                        }
                        if ui.button("Save and return to menu").clicked() {
                            f.dispatch(Command::SetPaused(true), "");
                            f.battle.fire_latched = false;
                            f.request_save(SaveIntent::Menu);
                        }
                        ui.separator();
                        ui.checkbox(&mut f.battle.inspect_log, "Combat log");
                        egui::ScrollArea::vertical()
                            .max_height(280.)
                            .show(ui, |ui| field_details(ui, f, &view, &hud_view));
                    });
                });
                if !f.status.message.is_empty() {
                    status(ui, f);
                }
            });
        });
}
/// A small opening is a genuine smaller clearing at the normal yard scale;
/// growth expands it up to the bounded viewport rather than stretching3rows.
pub(super) fn clearing_rect(maximum: Rect, lanes: u8, width: f32) -> Rect {
    let size = vec2(
        (maximum.width() * (width / 30.0).clamp(0.0, 1.0))
            .max(160.0)
            .min(maximum.width()),
        (maximum.height() * (f32::from(lanes) / 11.0).clamp(0.0, 1.0))
            .max(86.0)
            .min(maximum.height()),
    );
    if size == maximum.size() {
        maximum
    } else {
        Rect::from_center_size(maximum.center(), size).intersect(maximum)
    }
}
fn battle_arena(canvas: Rect, construction: bool) -> Rect {
    Rect::from_min_max(
        pos2(canvas.left() + 24., canvas.top() + 124.),
        pos2(
            canvas.right() - if construction { 276. } else { 24. },
            canvas.bottom() - 84.,
        ),
    )
}
fn corner_hud(ui: &mut egui::Ui, canvas: Rect, f: &mut UiCtx, view: &hud::HudView) {
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
fn arrival_groups(size: u32) -> String {
    match size {
        0 | 1 => "single arrivals".to_owned(),
        size => format!("groups of {size}"),
    }
}
/// The first-defense lesson for `hint`, naming the player's own keys.
/// `bindings` follows `settings::Action` order: fire, melee, reload, heal,
/// brace, build, pause.
fn hint_text(hint: FieldHint, paused: bool, bindings: &[String]) -> String {
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
fn field_hint_callout(ui: &mut egui::Ui, arena: Rect, f: &mut UiCtx, view: &BattleScreen) {
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
fn pause_banner(ui: &egui::Ui, arena: Rect, view: &BattleScreen, bindings: &[String]) {
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
fn attack_selector_rect(canvas: Rect, arena: Rect) -> Rect {
    Rect::from_min_max(
        pos2(canvas.left() + 350.0, canvas.top() + 12.0),
        pos2(canvas.right() - 222.0, arena.top() - 12.0),
    )
}
fn attack_selector(ui: &mut egui::Ui, f: &mut UiCtx, view: &BattleScreen) {
    let class = view.weapon_class;
    let spacing = 4.0;
    let button_width = ((ui.available_width() - 2.0 * spacing) / 3.0).max(1.0);
    ui.spacing_mut().item_spacing.x = spacing;
    ui.spacing_mut().button_padding = vec2(3.0, 4.0);
    ui.horizontal(|ui| {
        for (selection, label) in [
            (AttackSelection::Gun, "Gun"),
            (AttackSelection::Grenade, "Grenade"),
            (AttackSelection::Molotov, "Molotov"),
        ] {
            let slot = selection.ordnance().map(|kind| view.slot(kind));
            let unlocked = slot.is_none_or(|slot| slot.unlocked);
            let fire = &f.settings.bindings[Action::Fire as usize];
            let (hint, color) =
                throwable_state(label, slot, || format!("{class}: hold {fire} to fire."));
            let selected = f.battle.attack_selection == selection;
            let button = egui::Button::new(RichText::new(label).size(12.0).color(color))
                .selected(selected)
                .frame(true);
            let response = ui
                .add_enabled_ui(unlocked, |ui| {
                    ui.add_sized(vec2(button_width, 26.0), button)
                })
                .inner;
            // Cooldown progress underlines a recharging throwable.
            if let Some(slot) =
                slot.filter(|slot| slot.unlocked && slot.cooldown > 0 && slot.charges > 0)
            {
                let rect = response.rect.shrink2(vec2(4.0, 2.0));
                let ready = 1.0 - (slot.cooldown as f32 / slot.cooldown_total).clamp(0.0, 1.0);
                let y = rect.bottom();
                ui.painter().line_segment(
                    [
                        pos2(rect.left(), y),
                        pos2(rect.left() + rect.width() * ready, y),
                    ],
                    Stroke::new(2.0, AMBER),
                );
            }
            if response
                .on_hover_text(&hint)
                .on_disabled_hover_text(&hint)
                .clicked()
            {
                f.battle.attack_selection = selection;
                f.battle.fire_latched = false;
                f.battle.pending.fire = false;
                f.battle.pending.throw = None;
                f.battle.pending_attack_aims.fire = None;
                f.runtime.clear_input();
            }
        }
    });
    let helper = if let Some(kind) = f.battle.attack_selection.ordnance() {
        let slot = view.slot(kind);
        if slot.charges == 0 {
            "Empty · renewed next attempt".to_owned()
        } else if slot.cooldown > 0 {
            format!(
                "{} left · ready {:.1}s",
                slot.charges,
                slot.cooldown as f32 / 60.0
            )
        } else {
            format!("{} left · click ground cell", slot.charges)
        }
    } else {
        "Hold fire to shoot".to_owned()
    };
    ui.add(egui::Label::new(RichText::new(&helper).size(11.0).color(MUTED)).truncate())
        .on_hover_text(helper);
}
/// Hover reason and label colour of one attack selector. A locked throwable
/// names where it is learned; an unlocked one states its charges or cooldown.
fn throwable_state(
    label: &str,
    slot: Option<&battle_view::OrdnanceSlotView>,
    gun: impl FnOnce() -> String,
) -> (String, Color32) {
    match slot {
        None => (gun(), TEXT),
        Some(slot) if !slot.unlocked => (
            format!(
                "{label} locked. Learn it in the Ordnance branch of Journey upgrades after this attempt."
            ),
            MUTED,
        ),
        Some(slot) if slot.charges == 0 => (
            format!("{label}: no charges left. Renewed next attempt."),
            RED,
        ),
        Some(slot) if slot.cooldown > 0 => (
            format!(
                "{label}: {} charges · ready in {:.1}s. Select, then click a ground cell to throw.",
                slot.charges,
                slot.cooldown as f32 / 60.0
            ),
            AMBER,
        ),
        Some(slot) => (
            format!(
                "{label}: {} charges. Select, then click a ground cell to throw.",
                slot.charges
            ),
            TEXT,
        ),
    }
}
fn ordnance_cursor(ui: &mut egui::Ui, r: Rect, f: &UiCtx, view: &BattleScreen) {
    let arena = projection(f, r);
    let Some(kind) = f.battle.attack_selection.ordnance() else {
        return;
    };
    let Some(cell) = target_cell(f.battle.aim, arena.lanes, arena.width) else {
        return;
    };
    let slot = view.slot(kind);
    let color = if slot.ready { AMBER } else { RED };
    let painter = ui.painter_at(r);
    for column in 0..=arena.width.ceil() as u8 {
        painter.line_segment(
            [
                arena.world(column as f32, 0.0),
                arena.world(column as f32, f32::from(arena.lanes)),
            ],
            Stroke::new(0.5, MUTED.gamma_multiply(0.18)),
        );
    }
    for lane in 0..=arena.lanes {
        painter.line_segment(
            [
                arena.world(0.0, lane as f32),
                arena.world(arena.width, lane as f32),
            ],
            Stroke::new(0.5, MUTED.gamma_multiply(0.18)),
        );
    }
    let [x, y] = cell.map(f32::from);
    let rect = Rect::from_min_max(arena.world(x, y), arena.world(x + 1.0, y + 1.0));
    painter.rect_filled(rect, 0.0, color.gamma_multiply(0.25));
    painter.rect_stroke(rect, 0.0, Stroke::new(1.5, color), egui::StrokeKind::Inside);
    painter.add(egui::Shape::ellipse_stroke(
        rect.center(),
        arena.radius(slot.radius),
        Stroke::new(1.2, color),
    ));
    painter.text(
        rect.left_top() - vec2(0.0, 5.0),
        Align2::LEFT_BOTTOM,
        format!("{} · cell {},{}", kind.name(), cell[0] + 1, cell[1] + 1),
        FontId::proportional(12.0),
        TEXT,
    );
}

fn personal_controls(ui: &mut egui::Ui, f: &mut UiCtx, ctx: &egui::Context, view: &BattleScreen) {
    f.battle.pending.reload |= ui.button("Reload").clicked();
    if ui
        .button("Strike contact lane")
        .on_hover_text("Strike the selected contact lane. Melee cancels an unfinished reload.")
        .clicked()
    {
        let aim = assisted_aim(f, Some(view));
        f.battle.pending.melee = true;
        f.battle.pending.aim = aim;
        f.battle.pending_attack_aims.melee.get_or_insert(aim);
    }
    if f.battle.attack_selection == AttackSelection::Gun
        && ui
            .button(if f.battle.fire_latched {
                "Stop firing"
            } else {
                "Toggle firing"
            })
            .clicked()
    {
        f.toggle_fire(ctx);
    }
    if view.heal {
        f.battle.pending.heal |= ui
            .button("Emergency aid")
            .on_hover_text(&f.settings.bindings[Action::Heal as usize])
            .clicked();
    }
    if view.brace {
        f.battle.pending.brace |= ui
            .button("Brace the line")
            .on_hover_text(&f.settings.bindings[Action::Brace as usize])
            .clicked();
    }
}
fn field_details(ui: &mut egui::Ui, f: &mut UiCtx, view: &BattleScreen, hud_view: &hud::HudView) {
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
fn construction_controls(ui: &mut egui::Ui, f: &mut UiCtx, view: &BattleScreen) {
    ui.strong(format!("{} supply remaining", view.supply));
    muted(ui, "Cost in supply per defense");
    for kind in StructureKind::ALL {
        let def = content().structure(kind);
        ui.add_enabled_ui(view.blueprints[kind], |ui| {
            ui.selectable_value(
                &mut f.battle.build_kind,
                kind,
                format!("{} · {}", def.name, def.supply),
            );
        })
        .response
        .on_disabled_hover_text(
            "Learn this blueprint in the Fortification branch of Journey upgrades.",
        );
    }
    let def = content().structure(f.battle.build_kind);
    muted(
        ui,
        format!(
            "{} HP · {} damage · {:.1} yard range",
            def.health, def.damage, def.range
        ),
    );
    ui.add_space(8.0);
    if ui.button("Save country plan").clicked() {
        f.dispatch(
            Command::SaveStartingLayout,
            "Country plan saved. Next deployment pays its supply costs.",
        );
    }
    if ui.button("Clear country plan").clicked() {
        f.dispatch(
            Command::ClearLayout,
            "Country plan cleared; deployed defenses remain.",
        );
    }
    ui.add_space(10.0);
    muted(
        ui,
        "Before combat: full refund. During combat: salvage half the remaining-health value, rounded down.",
    );
    muted(
        ui,
        if view.global_paused {
            "Paused explicitly"
        } else {
            "Combat continues while building"
        },
    );
    if !view.any_blueprint() {
        muted(
            ui,
            "Unlock traps after 1 boss clear, walls after 2, turrets after 3. Purchase blueprints in camp.",
        );
    }
}
fn placement_cursor(ui: &mut egui::Ui, r: Rect, f: &mut UiCtx) {
    let arena = projection(f, r);
    let Some(preview) = battle_view::placement_preview(f.game, f.battle.build_kind, f.battle.aim)
    else {
        return;
    };
    let (lane, column, center) = (preview.lane, preview.column, preview.center);
    let color = if preview.valid.is_ok() { GREEN } else { RED };
    let cell = Rect::from_min_max(
        arena.world(center[0] - preview.half_width, f32::from(lane)),
        arena.world(center[0] + preview.half_width, f32::from(lane) + 1.0),
    );
    ui.painter()
        .rect_filled(cell, 0.0, color.gamma_multiply(0.22));
    ui.painter()
        .rect_stroke(cell, 0.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
    let def = content().structure(f.battle.build_kind);
    if def.range > 0.0 {
        ui.painter().add(egui::Shape::ellipse_stroke(
            cell.center(),
            arena.radius(def.range),
            Stroke::new(1.0, color.gamma_multiply(0.45)),
        ));
    }
    if let Err(reason) = &preview.valid {
        ui.painter().text(
            cell.left_top() + vec2(3.0, -5.0),
            Align2::LEFT_BOTTOM,
            reason,
            FontId::proportional(11.0),
            TEXT,
        );
    }
    if ui.input(|i| i.pointer.primary_clicked()) {
        f.dispatch(
            Command::Place {
                kind: f.battle.build_kind,
                lane,
                column,
            },
            "Defense placed.",
        );
    }
    if ui.input(|i| i.pointer.secondary_clicked())
        && let Some(id) = preview.occupant
    {
        f.dispatch(Command::Salvage(id), "Structure salvaged.");
    }
}

#[cfg(test)]
mod selector_layout_tests {
    use super::*;
    use horde_core::sim::Game;
    #[test]
    fn all_attack_controls_and_the_single_line_helper_fit_above_the_minimum_arena() {
        for size in [vec2(1440.0, 900.0), vec2(1050.0 / 1.35, 700.0 / 1.35)] {
            let canvas = Rect::from_min_size(egui::Pos2::ZERO, size);
            let arena = battle_arena(canvas, false);
            let selector = attack_selector_rect(canvas, arena);
            assert!(selector.bottom() <= arena.top() - 12.0);
            assert!(selector.width() >= 200.0);
            assert!(selector.height() >= 26.0 + 7.0 + 16.0);
            let width = (selector.width() - 8.0) / 3.0;
            let ctx = egui::Context::default();
            crate::settings::install_font(&ctx);
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                for label in ["Gun", "Grenade", "Molotov"] {
                    let galley =
                        ui.painter()
                            .layout_no_wrap(label.into(), FontId::proportional(12.0), TEXT);
                    assert!(
                        galley.size().x + 6.0 <= width,
                        "{label} must remain one line at minimum width"
                    );
                }
            });
            output.textures_delta.clear();
        }
    }
    #[test]
    fn arrival_groups_read_naturally() {
        assert_eq!(arrival_groups(1), "single arrivals");
        assert_eq!(arrival_groups(2), "groups of 2");
        assert_eq!(arrival_groups(5), "groups of 5");
    }
    #[test]
    fn shorter_opening_clearing_grows_at_stable_yard_scale_and_stays_in_its_canvas() {
        for size in [vec2(1392.0, 692.0), vec2(729.0, 310.0)] {
            let max = Rect::from_min_size(pos2(24.0, 124.0), size);
            let small = clearing_rect(max, 3, 12.0);
            assert!(max.contains_rect(small));
            assert!(small.width() < max.width() * 0.5 && small.height() < max.height() * 0.5);
            let opening = art::ArenaProjection::with_geometry(small, 3, 12.0);
            let old = art::ArenaProjection::with_geometry(clearing_rect(max, 11, 30.0), 11, 30.0);
            assert!((opening.yard_width() - old.yard_width()).abs() < 0.001);
            // The bounded minimum adds at most one logical pixel per opening row.
            assert!(opening.lane_height() >= old.lane_height() - 0.001);
            assert!(opening.lane_height() - old.lane_height() < 1.0);
            assert_eq!(clearing_rect(max, 11, 30.0), max);
            assert_eq!(clearing_rect(max, 17, 30.0), max);
        }
    }
    #[test]
    fn actual_short_and_full_geometry_round_trips_cells_building_and_aoe() {
        for (lanes, width) in [(3, 8.0), (3, 12.0), (7, 20.0), (11, 30.0), (17, 30.0)] {
            let mut game = Game::default();
            let settings = horde_core::balance::BalanceSettings {
                start_lanes: lanes,
                max_lanes: lanes,
                start_width: width as u8,
                max_width: width as u8,
                ..Default::default()
            };
            game.campaign_mut().set_balance_settings(settings).unwrap();
            game.deploy(0, Mode::Personal).unwrap();
            let a = game.attempt().unwrap();
            let arena = art::ArenaProjection::for_attempt(
                clearing_rect(
                    Rect::from_min_size(pos2(20.0, 120.0), vec2(729.0, 310.0)),
                    lanes,
                    width,
                ),
                a,
            );
            for lane in 0..lanes {
                for column in 0..5 {
                    let center = a.build_cell_center(lane, column).unwrap();
                    let actual = arena.inverse(arena.world(center[0], center[1]));
                    assert_eq!(a.build_cell_at(actual), Some([lane, column]));
                }
            }
            let last = [width - 0.01, f32::from(lanes) - 0.01];
            let restored = arena.inverse(arena.world(last[0], last[1]));
            assert_eq!(
                target_cell(restored, lanes, width),
                Some([width as u8 - 1, lanes - 1])
            );
            assert!(target_cell([width, 0.5], lanes, width).is_none());
            assert!(target_cell([0.5, f32::from(lanes)], lanes, width).is_none());
            assert!((arena.radius(1.0).x - arena.yard_width()).abs() < 0.001);
            assert!((arena.radius(1.0).y - arena.lane_height()).abs() < 0.001);
        }
    }
    #[test]
    fn strict_diagnostic_reticle_draws_once_without_claiming_os_cursor_despite_hover_or_focus() {
        for hovered in [false, true] {
            for focused in [false, true] {
                for covered in [false, true] {
                    assert_eq!(
                        reticle_ownership(true, hovered, focused, covered),
                        Some(false)
                    );
                    assert_eq!(
                        reticle_ownership(false, hovered, focused, covered),
                        if hovered && focused && !covered {
                            Some(true)
                        } else {
                            None
                        }
                    );
                }
            }
        }
        for (lanes, width) in [(3, 8.0), (3, 12.0), (17, 30.0)] {
            let arena = art::ArenaProjection::with_geometry(
                Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 140.0)),
                lanes,
                width,
            );
            let aim = preview_aim(arena);
            assert!(target_cell(aim, lanes, width).is_some());
            assert_eq!(aim[1], f32::from(lanes) * 0.5);
        }
    }

    #[test]
    fn short_arena_top_and_bottom_rows_share_forest_fx_space_without_touching_controls() {
        for viewport in [vec2(1440.0, 900.0), vec2(1050.0 / 1.35, 700.0 / 1.35)] {
            let canvas = Rect::from_min_size(pos2(0.0, 0.0), viewport);
            let effect = battle_arena(canvas, false);
            let rect = clearing_rect(effect, 3, 12.0);
            let arena =
                art::ArenaProjection::with_geometry(rect, 3, 12.0).with_effects_clip(effect);
            assert_eq!(arena.rect, rect);
            assert_eq!(arena.effects_clip, effect);
            assert!(effect.top() >= 124.0 && effect.bottom() <= canvas.bottom() - 84.0);
            for lane in [0, 2] {
                let feet = arena.world(6.0, lane as f32 + 0.5);
                for fraction in [0.0, 0.5, 0.8] {
                    let drop = feet + vec2(0.0, -12.0 - fraction * 78.0);
                    assert!(effect.contains_rect(Rect::from_center_size(drop, vec2(25.0, 25.0))));
                }
                assert!(effect.contains_rect(Rect::from_center_size(feet, vec2(48.0, 48.0))));
                assert!(!rect.contains(feet - vec2(0.0, 78.0)) || lane == 2);
            }
        }
    }
}
