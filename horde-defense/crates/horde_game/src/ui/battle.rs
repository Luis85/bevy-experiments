//! Battle presentation. Game changes are queued typed intents.
use super::*;

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

fn active_lanes(f: &Frontend) -> u8 {
    f.game
        .attempt
        .as_ref()
        .map_or_else(|| f.game.campaign.lane_count(), |a| a.lane_count)
}
fn projection(f: &Frontend, r: Rect) -> art::ArenaProjection {
    let width = f
        .game
        .attempt
        .as_ref()
        .map_or_else(|| f.game.campaign.arena_geometry().1, |a| a.width_yards);
    art::ArenaProjection::with_geometry(r, active_lanes(f), width).with_effects_clip(
        f.effects_rect
            .filter(|clip| clip.contains_rect(r))
            .unwrap_or(r),
    )
}

pub(super) fn controls(ctx: &egui::Context, f: &mut Frontend) {
    let frame = ctx.cumulative_frame_nr();
    if f.last_control_frame == frame {
        return;
    }
    f.last_control_frame = frame;
    if feedback::overlay_owns_keyboard(ctx, f) {
        return;
    }
    if f.settings.pressed(ctx, Action::Pause, false) {
        let pause = !f.game.attempt.as_ref().is_some_and(|a| a.paused);
        f.dispatch(Command::SetPaused(pause), "");
        f.construction = false;
        f.fire_latched = false;
    }
    if f.settings.pressed(ctx, Action::Build, false) {
        if f.game.campaign.blueprints.iter().any(|owned| *owned) {
            f.construction = !f.construction;
        } else {
            f.construction = false;
            f.message="Your first boss clear opens trap blueprints. Combat continues; use Pause when you need time.".into();
            f.message_error = false;
            f.feedback_revision = f.feedback_revision.wrapping_add(1);
        }
        f.fire_latched = false;
    }
    if f.attack_selection == AttackSelection::Gun && f.settings.pressed(ctx, Action::Toggle, false)
    {
        f.toggle_fire(ctx);
    }
}
pub(super) fn gather_combat_input(ctx: &egui::Context, f: &mut Frontend) -> Input {
    if f.construction || ctx.text_edit_focused() || ctx.any_popup_open() || f.feedback.details_open
    {
        return Input {
            aim: f.aim,
            auto_reload: f.settings.auto_reload,
            ..Input::default()
        };
    }
    let mut input = Input {
        aim: assisted_aim(f),
        auto_reload: f.settings.auto_reload,
        ..Input::default()
    };
    let fire_allowed =
        f.arena_hover || !f.settings.bindings[Action::Fire as usize].starts_with("Mouse");
    if let Some(kind) = f.attack_selection.ordnance() {
        // Throwing consumes a press edge, never a held button or firing latch.
        f.fire_latched = false;
        if let Some(cell) = raw_press_aim(ctx, f, Action::Fire).and_then(|aim| {
            target_cell(
                aim,
                active_lanes(f),
                f.game
                    .attempt
                    .as_ref()
                    .map_or_else(|| f.game.campaign.arena_geometry().1, |a| a.width_yards),
            )
        }) {
            f.pending.throw.get_or_insert(ThrowRequest { kind, cell });
        }
    } else {
        let fire_press = press_aim(ctx, f, Action::Fire);
        if f.settings.toggle_fire && fire_press.is_some() {
            f.toggle_fire(ctx);
        }
        input.fire = f.fire_latched
            || (!f.settings.toggle_fire
                && fire_allowed
                && f.settings.pressed(ctx, Action::Fire, true));
        if !f.settings.toggle_fire
            && let Some(aim) = fire_press
        {
            f.pending.fire = true;
            f.pending_attack_aims.fire.get_or_insert(aim);
        }
    }
    if let Some(aim) = press_aim(ctx, f, Action::Melee) {
        f.pending.melee = true;
        f.pending_attack_aims.melee.get_or_insert(aim);
    }
    input.melee |= f.settings.pressed(ctx, Action::Melee, true)
        && (f.arena_hover || !f.settings.bindings[Action::Melee as usize].starts_with("Mouse"));
    input.reload |= f.settings.pressed(ctx, Action::Reload, false);
    input.heal |= f.settings.pressed(ctx, Action::Heal, false);
    input.brace |= f.settings.pressed(ctx, Action::Brace, false);
    input
}
fn assisted_aim(f: &Frontend) -> [f32; 2] {
    assist_at(f, f.aim)
}
fn press_aim(ctx: &egui::Context, f: &Frontend, action: Action) -> Option<[f32; 2]> {
    raw_press_aim(ctx, f, action).map(|aim| assist_at(f, aim))
}
fn raw_press_aim(ctx: &egui::Context, f: &Frontend, action: Action) -> Option<[f32; 2]> {
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
            && let Some(arena) = f.arena_rect
        {
            return arena
                .contains(position)
                .then(|| projection(f, arena).inverse(position));
        }
        return (f.arena_hover && f.settings.pressed(ctx, action, false)).then_some(f.aim);
    }
    f.settings.pressed(ctx, action, false).then_some(f.aim)
}
fn assist_at(f: &Frontend, aim: [f32; 2]) -> [f32; 2] {
    if f.settings.aim_assist <= 0.0 {
        return aim;
    }
    if let Some(a) = &f.game.attempt
        && let Some(e) = a
            .enemies
            .iter()
            .filter(|e| {
                ((e.x - aim[0]).powi(2) + (e.y() - aim[1]).powi(2)).sqrt() < f.settings.aim_assist
            })
            .min_by(|a, b| {
                ((a.x - aim[0]).powi(2) + (a.y() - aim[1]).powi(2))
                    .total_cmp(&((b.x - aim[0]).powi(2) + (b.y() - aim[1]).powi(2)))
            })
    {
        return [e.x, e.y()];
    }
    aim
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
pub(super) fn battle(root: &mut egui::Ui, f: &mut Frontend) {
    if f.game.attempt.is_none() {
        f.screen = Screen::Results;
        return;
    }
    let ctx = root.ctx().clone();
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK))
        .show(root, |ui| {
            let canvas = ui.max_rect();
            let maximum = battle_arena(canvas, f.construction);
            let a = f.game.attempt.as_ref().unwrap();
            let r = clearing_rect(maximum, a.lane_count, a.width_yards);
            f.effects_rect = Some(maximum);
            let arena = projection(f, r);
            let response = ui.interact(
                r,
                egui::Id::new("pixel_battle_arena"),
                Sense::click_and_drag(),
            );
            f.arena_rect = Some(r);
            f.arena_hover = response.hovered();
            let diagnostic_reticle = review::reticle_capture_preview();
            if diagnostic_reticle {
                f.aim = preview_aim(arena);
            } else if let Some(pointer) = response.hover_pos() {
                f.aim = arena.inverse(pointer);
            }
            let a = f.game.attempt.as_ref().unwrap();
            let theme = world()
                .country(a.country_id.as_deref().unwrap_or(""))
                .map(|country| match country.biome.to_lowercase().as_str() {
                    "desert" | "arid" => 2,
                    "tundra" | "polar" | "boreal" => 3,
                    "tropical" | "rainforest" => 4,
                    "mediterranean" => 1,
                    _ => 0,
                })
                .unwrap_or(a.level);
            art::world_backdrop(ui.painter(), canvas, arena, theme, f.textures);
            art::battlefield(
                &ui.painter_at(maximum),
                r,
                a,
                f.aim,
                art::RenderOptions {
                    construction: f.construction,
                    outlines: f.settings.outlines,
                    numbers: f.settings.numbers,
                    reduced: f.settings.reduced_effects,
                    blood: f.settings.blood,
                    theme,
                    textures: f.textures,
                },
            );
            art::world_atmosphere(ui.painter(), canvas);
            crate::inspector::hover(&response, a, f.aim);
            if response.hovered() {
                if f.construction {
                    placement_cursor(ui, r, f);
                } else if f.attack_selection.ordnance().is_some() && a.mode == Mode::Personal {
                    ordnance_cursor(ui, r, f);
                }
            }
            if let Some(owns_cursor) = reticle_ownership(
                diagnostic_reticle,
                response.hovered(),
                ui.input(|i| i.raw.focused),
                ctx.any_popup_open() || f.feedback.details_open,
            ) {
                if owns_cursor {
                    f.custom_cursor_requested = true;
                    ctx.set_cursor_icon(egui::CursorIcon::None);
                }
                reticle::paint(
                    ui,
                    f.game.attempt.as_ref().unwrap(),
                    arena,
                    f.aim,
                    f.attack_selection,
                    f.construction,
                    f.settings.reduced_effects,
                );
            }
            damage::draw(ui, r, f);
            loot::draw(ui, r, f);
            corner_hud(ui, canvas, f);
            if let Some(receipt) = f.game.attempt.as_ref().and_then(|a| a.loot_events.last()) {
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
            if f.game
                .attempt
                .as_ref()
                .is_some_and(|a| a.mode == Mode::Personal)
                && !f.construction
            {
                let selector = attack_selector_rect(canvas, r);
                ui.scope_builder(egui::UiBuilder::new().max_rect(selector), |ui| {
                    ui.set_clip_rect(selector.intersect(ui.clip_rect()));
                    attack_selector(ui, f)
                });
            }
            if f.construction {
                let panel = Rect::from_min_size(
                    pos2(canvas.right() - 252.0, r.top() + 8.0),
                    vec2(236.0, (r.height() - 16.0).max(180.0)),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(panel), |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_black_alpha(235))
                        .stroke(Stroke::new(1., TEXT))
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.set_width(214.0);
                            egui::ScrollArea::vertical()
                                .max_height(panel.height() - 24.0)
                                .show(ui, |ui| construction_controls(ui, f));
                        });
                });
            }
            if f.settings
                .bindings
                .iter()
                .zip(["Mouse1", "Mouse2", "R", "1", "2", "B", "Escape", "F"])
                .any(|(a, b)| a != b)
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
            ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
                ui.horizontal_wrapped(|ui| {
                    let a = f.game.attempt.as_ref().unwrap();
                    let paused = a.paused;
                    let label = if !paused {
                        "Pause"
                    } else if a.tick == 0 {
                        "Begin defense"
                    } else {
                        "Resume defense"
                    };
                    if ui.button(RichText::new(label).strong()).clicked() {
                        f.dispatch(Command::SetPaused(!paused), "");
                        f.construction = false;
                        f.fire_latched = false;
                    }
                    if f.game.attempt.as_ref().unwrap().mode == Mode::Personal && !f.construction {
                        personal_controls(ui, f, &ctx);
                    }
                    if f.game.campaign.blueprints.iter().any(|b| *b)
                        && ui
                            .button(if f.construction {
                                "Close construction"
                            } else {
                                "Build defenses"
                            })
                            .clicked()
                    {
                        f.construction = !f.construction;
                        f.fire_latched = false;
                    }
                    if f.game.attempt.as_ref().unwrap().mode == Mode::Npc {
                        ui.label("Observe");
                        for speed in [1, 2, 4] {
                            ui.selectable_value(&mut f.speed, speed, format!("{speed}×"));
                        }
                        let response = ui.button("Survivor");
                        let mut popup = egui::Popup::menu(&response)
                            .width(300.0)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
                        if crate::environment::string("LASTLIGHT_REVIEW_SCREEN").as_deref()
                            == Some("npc-expanded")
                            && !crate::environment::flag("LASTLIGHT_PRACTICE")
                        {
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
                                        f.game.attempt.as_ref().unwrap(),
                                    );
                                    policy(ui, f);
                                });
                        });
                    }
                    if ui
                        .button(
                            if f.game.attempt.as_ref().is_some_and(|a| a.mode == Mode::Npc) {
                                "Recall & train"
                            } else {
                                "Retreat & train"
                            },
                        )
                        .on_hover_text("Return to skills with every earned coin and item retained.")
                        .clicked()
                    {
                        f.fire_latched = false;
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
                            f.fire_latched = false;
                            f.request_save(SaveIntent::Menu);
                        }
                        if ui
                            .button(if f.game.attempt.as_ref().unwrap().mode == Mode::Npc {
                                "Recall survivor"
                            } else {
                                "Withdraw"
                            })
                            .clicked()
                        {
                            f.fire_latched = false;
                            f.dispatch(
                                Command::Withdraw,
                                "Deployment recalled; earned rewards retained.",
                            );
                        }
                        ui.separator();
                        ui.checkbox(&mut f.inspect_log, "Combat log");
                        egui::ScrollArea::vertical()
                            .max_height(280.)
                            .show(ui, |ui| field_details(ui, f));
                    });
                });
                if !f.message.is_empty() {
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
fn corner_hud(ui: &mut egui::Ui, canvas: Rect, f: &mut Frontend) {
    let a = f.game.attempt.as_ref().unwrap();
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
                        a.mode == Mode::Npc || a.profile.appearance == Appearance::Female,
                        portrait.center(),
                        portrait.size(),
                    );
                    ui.vertical(|ui| {
                        ui.set_width(250.);
                        progress(
                            ui,
                            a.hp.max(0) as f32 / a.max_hp as f32,
                            format!(
                                "{} · HP {} / {}",
                                if a.mode == Mode::Personal {
                                    f.game
                                        .campaign
                                        .selected_survivor
                                        .map_or("YOU", SurvivorId::name)
                                } else {
                                    "MARA"
                                },
                                a.hp.max(0),
                                a.max_hp
                            ),
                            RED,
                        );
                        ui.horizontal(|ui| {
                            ui.strong(format!("{} / {} ammo", a.ammo, a.magazine));
                            ui.label(
                                RichText::new(format!(
                                    "{} {:.0}/{:.0}",
                                    if a.mode == Mode::Npc { "FP" } else { "Stamina" },
                                    a.stamina,
                                    a.max_stamina
                                ))
                                .size(12.0),
                            );
                        });
                        hud::action(ui, a);
                        ui.horizontal(|ui| {
                            hud::danger(ui, a);
                            hud::dps(ui, a);
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
                if let Some(boss) = a.enemies.iter().find(|e| e.boss) {
                    progress(
                        ui,
                        boss.hp.max(0) as f32 / boss.max_hp as f32,
                        if boss.critically_wounded(a.mode) {
                            format!("CRITICAL · {} HP", boss.injury.hp)
                        } else {
                            format!("BOSS · {} HP", boss.hp.max(0))
                        },
                        RED,
                    );
                } else {
                    ui.strong(format!(
                        "BOSS IN {:02}:{:02}",
                        a.boss_countdown() as u32 / 60,
                        a.boss_countdown() as u32 % 60
                    ));
                }
                ui.label(
                    RichText::new(format!(
                        "{:.0}s · {} kills · +{} coins",
                        a.elapsed(),
                        a.kills,
                        a.coins
                    ))
                    .size(12.),
                );
                ui.label(
                    RichText::new(format!(
                        "{} lanes · {:.0} yd · threat {}",
                        a.lane_count, a.width_yards, a.progression.rank
                    ))
                    .size(10.0),
                )
                .on_hover_text(format!(
                    "{} difficulty, frozen at deployment",
                    a.balance.preset.name()
                ));
                ui.label(
                    RichText::new(format!(
                        "{}-enemy groups · gear Lv{}",
                        a.progression.batch_size, a.progression.item_level
                    ))
                    .size(10.0),
                );
            });
    });
}
fn attack_selector_rect(canvas: Rect, arena: Rect) -> Rect {
    Rect::from_min_max(
        pos2(canvas.left() + 350.0, canvas.top() + 12.0),
        pos2(canvas.right() - 222.0, arena.top() - 12.0),
    )
}
fn attack_selector(ui: &mut egui::Ui, f: &mut Frontend) {
    let a = f.game.attempt.as_ref().expect("active battle");
    let class = a.profile.weapon.class.name();
    let grenade = a.profile.modifiers.grenade_unlocked;
    let molotov = a.profile.modifiers.molotov_unlocked;
    let charges = a.ordnance.charges;
    let cooldowns = a.ordnance.cooldowns;
    let spacing = 4.0;
    let button_width = ((ui.available_width() - 2.0 * spacing) / 3.0).max(1.0);
    ui.spacing_mut().item_spacing.x = spacing;
    ui.spacing_mut().button_padding = vec2(3.0, 4.0);
    ui.horizontal(|ui| {
        for (selection, label, unlocked) in [
            (AttackSelection::Gun, "Gun", true),
            (AttackSelection::Grenade, "Grenade", grenade),
            (AttackSelection::Molotov, "Molotov", molotov),
        ] {
            let hint = if selection == AttackSelection::Gun {
                format!("{class}: hold {} to fire.", f.settings.bindings[Action::Fire as usize])
            } else if unlocked {
                format!("{label}: {} charges. Select, then click a ground cell to throw.", charges[selection.ordnance().expect("throwable").index()])
            } else {
                format!("Learn {label} handling in the Ordnance branch of Survival skills after this attempt.")
            };
            if ui.add_enabled_ui(unlocked, |ui| ui.add_sized(vec2(button_width, 26.0), egui::Button::selectable(f.attack_selection == selection, RichText::new(label).size(12.0))))
                .inner.on_hover_text(&hint).on_disabled_hover_text(&hint).clicked() {
                f.attack_selection = selection;
                f.fire_latched = false;
                f.pending.fire = false;
                f.pending.throw = None;
                f.pending_attack_aims.fire = None;
                f.runtime.clear_input();
            }
        }
    });
    let helper = if let Some(kind) = f.attack_selection.ordnance() {
        let index = kind.index();
        if charges[index] == 0 {
            "Empty · renewed next attempt".to_owned()
        } else if cooldowns[index] > 0 {
            format!(
                "{} left · ready {:.1}s",
                charges[index],
                cooldowns[index] as f32 / 60.0
            )
        } else {
            format!("{} left · click ground cell", charges[index])
        }
    } else {
        "Hold fire to shoot".to_owned()
    };
    ui.add(egui::Label::new(RichText::new(&helper).size(11.0).color(MUTED)).truncate())
        .on_hover_text(helper);
}
fn ordnance_cursor(ui: &mut egui::Ui, r: Rect, f: &Frontend) {
    let arena = projection(f, r);
    let Some(kind) = f.attack_selection.ordnance() else {
        return;
    };
    let Some(cell) = target_cell(f.aim, arena.lanes, arena.width) else {
        return;
    };
    let a = f.game.attempt.as_ref().expect("active battle");
    let ready = a.ordnance.charges[kind.index()] > 0 && a.ordnance.cooldowns[kind.index()] == 0;
    let color = if ready { AMBER } else { RED };
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
    let radius = horde_core::ordnance::catalog().definition(kind).radius
        * a.profile.modifiers.ordnance_radius_multiplier;
    painter.add(egui::Shape::ellipse_stroke(
        rect.center(),
        arena.radius(radius),
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

fn personal_controls(ui: &mut egui::Ui, f: &mut Frontend, ctx: &egui::Context) {
    f.pending.reload |= ui.button("Reload").clicked();
    if ui
        .button("Strike contact lane")
        .on_hover_text("Strike the selected contact lane. Melee cancels an unfinished reload.")
        .clicked()
    {
        let aim = assisted_aim(f);
        f.pending.melee = true;
        f.pending.aim = aim;
        f.pending_attack_aims.melee.get_or_insert(aim);
    }
    if f.attack_selection == AttackSelection::Gun
        && ui
            .button(if f.fire_latched {
                "Stop firing"
            } else {
                "Toggle firing"
            })
            .clicked()
    {
        f.toggle_fire(ctx);
    }
    if f.game.attempt.as_ref().unwrap().profile.heal {
        f.pending.heal |= ui
            .button("Emergency aid")
            .on_hover_text(&f.settings.bindings[Action::Heal as usize])
            .clicked();
    }
    if f.game.attempt.as_ref().unwrap().profile.brace {
        f.pending.brace |= ui
            .button("Brace the line")
            .on_hover_text(&f.settings.bindings[Action::Brace as usize])
            .clicked();
    }
}
fn field_details(ui: &mut egui::Ui, f: &mut Frontend) {
    let a = f.game.attempt.as_ref().unwrap();
    muted(
        ui,
        format!(
            "{} lanes · threat {} · {} hostiles · ordinary cap {} (+ boss) · {:.2} enemies/s",
            a.lane_count,
            a.progression.rank,
            a.enemies.len(),
            a.enemy_cap(),
            a.arrival_rate()
        ),
    );
    muted(
        ui,
        format!(
            "{:.0} yd path · {} difficulty · groups {} · new gear Lv{}",
            a.width_yards,
            a.balance.preset.name(),
            a.progression.batch_size,
            a.progression.item_level
        ),
    );
    hud::dps(ui, a);
    muted(ui, &content().levels[a.level].name);
    if a.mode == Mode::Personal {
        muted(
            ui,
            format!(
                "Selected lane {} · melee {:.1}s",
                f.aim[1]
                    .floor()
                    .clamp(0.0, f32::from(a.lane_count.saturating_sub(1))) as u8
                    + 1,
                a.melee_cooldown as f32 / 60.0
            ),
        );
    }
    if f.settings.show_guidance && f.game.campaign.attempts == 0 {
        chrome::section(ui, "Your first defense");
        let hint = if a.tick == 0 {
            "You stay at the center of the defense. Combat is live: aim across every lane. Retreat banks your rewards for training."
        } else if a.shots == 0 {
            "Aim at a visible enemy and hold fire. Reload during a safe gap."
        } else if a.reload_ticks == 0 && a.ammo < a.magazine {
            "Reload renews the magazine without spending coins. Melee cancels an unfinished reload."
        } else if a.first_contact.is_none() {
            "An enemy reaching any part of the left line can hurt you. Aim at that lane and strike back."
        } else {
            "Coins and equipment are already banked. Defeat the boss, or return to camp to improve."
        };
        ui.label(hint);
        if ui.small_button("Hide field hints").clicked() {
            f.settings.show_guidance = false;
        }
    }
    egui::CollapsingHeader::new("Shared line rule")
        .default_open(f.settings.show_guidance && f.game.campaign.attempts == 0)
        .show(ui, |ui| {
            ui.label("Enemies at any part of the line can attack you. Aim at a contact lane to strike back.");
        });
    if let Some(country) = f
        .game
        .attempt
        .as_ref()
        .and_then(|a| world().country(a.country_id.as_deref().unwrap_or("")))
    {
        let a = f.game.attempt.as_ref().unwrap();
        let theme = country.stage_theme(a.stage);
        egui::CollapsingHeader::new("Local setting").show(ui, |ui| {
            ui.strong(&theme.title);
            ui.label(&theme.setting);
            muted(ui, format!("{} · {}", theme.weather, theme.lighting));
            muted(ui, format!("Field props: {}", theme.props.join(", ")));
        });
    }
    if f.inspect_log {
        chrome::section(ui, "Resolution log");
        for line in f.game.attempt.as_ref().unwrap().log.iter().rev().take(18) {
            ui.label(RichText::new(line).size(11.0).color(MUTED));
        }
    }
}
fn construction_controls(ui: &mut egui::Ui, f: &mut Frontend) {
    ui.strong(format!(
        "{} supply remaining",
        f.game.attempt.as_ref().unwrap().supply
    ));
    for kind in StructureKind::ALL {
        let def = content().structure(kind);
        ui.add_enabled_ui(f.game.campaign.blueprints[kind.index()], |ui| {
            ui.selectable_value(
                &mut f.build_kind,
                kind,
                format!("{} · {} supply", def.name, def.supply),
            );
        });
    }
    let def = content().structure(f.build_kind);
    muted(
        ui,
        format!(
            "{} HP · {} damage · {:.1} yard range",
            def.health, def.damage, def.range
        ),
    );
    ui.add_space(14.0);
    muted(
        ui,
        "Before combat: full refund. During combat: salvage half the remaining-health value, rounded down.",
    );
    muted(
        ui,
        if f.game.global_paused {
            "Paused explicitly"
        } else {
            "Combat continues while building"
        },
    );
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
    if f.game.campaign.blueprints.iter().all(|b| !*b) {
        muted(
            ui,
            "Unlock traps after 1 boss clear, walls after 2, turrets after 3. Purchase blueprints in camp.",
        );
    }
}
fn placement_cursor(ui: &mut egui::Ui, r: Rect, f: &mut Frontend) {
    let arena = projection(f, r);
    let attempt = f.game.attempt.as_ref().unwrap();
    let Some([lane, column]) = attempt.build_cell_at(f.aim) else {
        return;
    };
    let center = attempt
        .build_cell_center(lane, column)
        .expect("validated cell");
    let half_width = attempt.construction_cell_width() * 0.5;
    let valid = f.game.preview_placement(f.build_kind, lane, column);
    let color = if valid.is_ok() { GREEN } else { RED };
    let cell = Rect::from_min_max(
        arena.world(center[0] - half_width, f32::from(lane)),
        arena.world(center[0] + half_width, f32::from(lane) + 1.0),
    );
    ui.painter()
        .rect_filled(cell, 0.0, color.gamma_multiply(0.22));
    ui.painter()
        .rect_stroke(cell, 0.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
    let def = content().structure(f.build_kind);
    if def.range > 0.0 {
        ui.painter().add(egui::Shape::ellipse_stroke(
            cell.center(),
            arena.radius(def.range),
            Stroke::new(1.0, color.gamma_multiply(0.45)),
        ));
    }
    if let Err(reason) = &valid {
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
                kind: f.build_kind,
                lane,
                column,
            },
            "Defense placed.",
        );
    }
    if ui.input(|i| i.pointer.secondary_clicked()) {
        let id = f
            .game
            .attempt
            .as_ref()
            .unwrap()
            .structures
            .iter()
            .find(|s| s.lane == lane && s.column == column)
            .map(|s| s.id);
        if let Some(id) = id {
            f.dispatch(Command::Salvage(id), "Structure salvaged.");
        }
    }
}

#[cfg(test)]
mod selector_layout_tests {
    use super::*;
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
            game.campaign.set_balance_settings(settings).unwrap();
            game.deploy(0, Mode::Personal).unwrap();
            let a = game.attempt.as_ref().unwrap();
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
