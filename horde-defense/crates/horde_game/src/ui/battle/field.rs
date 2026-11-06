use super::{
    BattleFrame, BattleLayout,
    attack::{attack_selector, ordnance_cursor, personal_controls},
    construction::{construction_controls, placement_cursor},
    details::field_details,
    input::{preview_aim, reticle_ownership},
    layout::{PANEL_FILL_ALPHA, occluded_spawn_cues},
    overlays::{corner_hud, field_hint_callout, incoming_cues, pause_banner},
    projection,
};
use crate::{
    art::{self, INK, MUTED, TEXT},
    diagnostics::{self, InterfaceSpan},
    runner::SaveIntent,
    ui::{Screen, UiCtx, armory, damage, loot, policy, reticle, status},
};
use bevy_egui::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke};
use horde_core::{command::Command, content::Mode};

/// The battle screen of this pass's `views` (`None` outside a battle, which
/// leaves for the results).
pub(in crate::ui) fn battle(root: &mut egui::Ui, f: &mut UiCtx, views: Option<&BattleFrame>) {
    let game = f.game;
    let perf = f.perf;
    let (Some(views), Some(a)) = (views, game.attempt()) else {
        f.battle.occluded_lanes.clear();
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
            // The hint band belongs to the whole first personal attempt, not
            // to the hint toggle, so "Hide hints" never moves the field.
            let layout = BattleLayout::new(
                canvas,
                view.lanes,
                view.width_yards,
                view.first_deployment && view.personal(),
            );
            let r = layout.field.rect;
            f.battle.effects_rect = Some(layout.effects_clip);
            let arena = projection(f, r);
            let response = ui.interact(
                r,
                egui::Id::new("pixel_battle_arena"),
                Sense::click_and_drag(),
            );
            f.battle.arena_rect = Some(r);
            // The construction overlay swallows the field under it.
            let pointer = response
                .hover_pos()
                .filter(|p| layout.addresses_field(*p, f.battle.construction));
            let hovered = response.hovered() && pointer.is_some();
            f.battle.arena_hover = hovered;
            let diagnostic_reticle = f.launch.reticle_capture_preview();
            if diagnostic_reticle {
                f.battle.aim = preview_aim(arena);
            } else if let Some(pointer) = pointer {
                f.battle.aim = arena.inverse(pointer);
            }
            {
                let _span = diagnostics::interface_span(perf, InterfaceSpan::Backdrop);
                art::world_backdrop(
                    ui.painter(),
                    canvas,
                    arena,
                    view.theme,
                    f.textures,
                    layout.dressing_unit_cap(),
                );
            }
            let paint = diagnostics::interface_span(perf, InterfaceSpan::BattlefieldPaint);
            let drawn = art::battlefield(
                &ui.painter_at(layout.arena),
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
            if hovered {
                crate::inspector::hover(&response, a, f.battle.aim);
                if f.battle.construction {
                    placement_cursor(ui, r, f);
                } else if f.battle.attack_selection.ordnance().is_some() && view.personal() {
                    ordnance_cursor(ui, r, f, view);
                }
            }
            if let Some(owns_cursor) = reticle_ownership(
                diagnostic_reticle,
                hovered,
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
            if view.paused && !f.battle.construction {
                pause_banner(ui, &layout, view, &f.settings.bindings);
            }
            // The callout sits in the reserved band over a zoomed field.
            if f.settings.show_guidance && view.first_deployment && view.personal() {
                field_hint_callout(ui, layout.hint_callout, f, view);
            }
            corner_hud(ui, &layout, f, hud_view);
            if let Some(receipt) = &view.last_loot {
                let area = layout.receipt_slot;
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
                let selector = layout.selector;
                ui.scope_builder(egui::UiBuilder::new().max_rect(selector), |ui| {
                    ui.set_clip_rect(selector.intersect(ui.clip_rect()));
                    attack_selector(ui, f, view)
                });
            }
            if f.battle.construction {
                // An overlay over the field: opening it never rescales the
                // arena, and it never covers a build cell.
                let panel = layout.construction_panel;
                ui.scope_builder(egui::UiBuilder::new().max_rect(panel), |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_black_alpha(PANEL_FILL_ALPHA))
                        .stroke(Stroke::new(1., TEXT))
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.set_width(panel.width() - 22.0);
                            egui::ScrollArea::vertical()
                                .max_height(panel.height() - 24.0)
                                .show(ui, |ui| construction_controls(ui, f, view));
                        });
                });
                // Spawns the panel hides get a chevron left of it.
                let cues = occluded_spawn_cues(arena, panel, &views.field.enemies);
                incoming_cues(ui.painter(), &cues);
                f.battle.occluded_lanes.clear();
                f.battle
                    .occluded_lanes
                    .extend(cues.iter().map(|&(lane, _)| lane));
            } else {
                f.battle.occluded_lanes.clear();
            }
            // The controls are always named; a new player has no other
            // place to learn the defaults.
            {
                ui.painter().text(
                    layout.legend_anchor,
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
            let bar = layout.bar;
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
                    if view.any_blueprint
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
