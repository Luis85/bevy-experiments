use super::{
    BattleFrame,
    attack::{attack_selector, attack_selector_rect, ordnance_cursor, personal_controls},
    battle_arena, clearing_rect,
    construction::{construction_controls, placement_cursor},
    details::field_details,
    input::{preview_aim, reticle_ownership},
    overlays::{corner_hud, field_hint_callout, pause_banner},
    projection,
};
use crate::{
    art::{self, INK, MUTED, TEXT},
    diagnostics::{self, InterfaceSpan},
    runner::SaveIntent,
    ui::{Screen, UiCtx, armory, damage, loot, policy, reticle, status},
};
use bevy_egui::egui::{self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};
use horde_core::{command::Command, content::Mode};

/// The battle screen of this pass's `views` (`None` outside a battle, which
/// leaves for the results).
pub(in crate::ui) fn battle(root: &mut egui::Ui, f: &mut UiCtx, views: Option<&BattleFrame>) {
    let game = f.game;
    let perf = f.perf;
    let (Some(views), Some(a)) = (views, game.attempt()) else {
        f.go(Screen::Results);
        return;
    };
    // The frame sampled the field's scene cache for this layer.
    debug_assert_eq!(root.layer_id(), egui::LayerId::background());
    let (view, hud_view) = (&views.screen, &views.hud);
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
            let drawn = art::battlefield(
                &ui.painter_at(maximum),
                r,
                &views.field,
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
                    ordnance_cursor(ui, r, f, view);
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
                pause_banner(ui, maximum, view, &f.settings.bindings);
            }
            if f.settings.show_guidance && view.first_deployment && view.personal() {
                field_hint_callout(ui, maximum, f, view);
            }
            corner_hud(ui, canvas, f, hud_view);
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
                    attack_selector(ui, f, view)
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
                                .show(ui, |ui| construction_controls(ui, f, view));
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
                        personal_controls(ui, f, &ctx, view);
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
                            .show(ui, |ui| field_details(ui, f, view, hud_view));
                    });
                });
                if !f.status.message.is_empty() {
                    status(ui, f);
                }
            });
        });
}
