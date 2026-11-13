use super::{
    BarRows, BattleFrame, BattleLayout,
    attack::{attack_selector, field_actions, ordnance_cursor, personal_controls},
    boss_banner::boss_banner,
    construction::{construction_controls, placement_cursor},
    details::field_details,
    input::{preview_aim, reticle_ownership},
    layout::{BAR_BUTTON_PADDING_Y, BAR_ROW_GAP, PANEL_FILL_ALPHA, occluded_spawn_cues},
    low_hp::{low_hp, low_hp_edge},
    overlays::{corner_hud, field_hint_callout, incoming_cues, pause_banner},
    projection,
    retreat::retreat_button,
};
use crate::{
    art::{self, INK, MUTED, TEXT},
    diagnostics::{self, InterfaceSpan},
    runner::SaveIntent,
    ui::{Screen, UiCtx, armory, character, damage, loot, primary_action, reticle, toast},
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
        f.battle.boss_banner_rect = None;
        f.battle.pause_banner_rect = None;
        f.battle.low_hp = false;
        f.go(Screen::Results);
        return;
    };
    // The frame sampled the field's scene cache for this layer.
    debug_assert_eq!(root.layer_id(), egui::LayerId::background());
    let (view, hud_view) = (&views.screen, &views.hud);
    // Low HP has hysteresis (UX-12): the remembered half is presentation
    // state, and another attempt starts from "not low".
    let was_low = f.battle.low_hp && f.battle.low_hp_attempt == view.boss.attempt;
    f.battle.low_hp = low_hp(hud_view.hp, hud_view.max_hp, was_low);
    f.battle.low_hp_attempt = view.boss.attempt;
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
            f.battle.effects_keep_clear = layout.effects_keep_clear();
            let arena = projection(f, r);
            let response = ui.interact(
                r,
                egui::Id::new("pixel_battle_arena"),
                Sense::click_and_drag(),
            );
            f.battle.arena_rect = Some(r);
            // The construction overlay swallows the field under it. A review
            // capture has no pointer at all (CF-1-03): wherever the OS mouse
            // rests, the aim stays where the scene pinned it and nothing
            // hovers.
            let pointer = response
                .hover_pos()
                .filter(|_| !f.launch.review_capture())
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
            let field_painter = ui.painter_at(layout.arena);
            let drawn = art::battlefield(
                &field_painter,
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
                    scene: art::SceneLook::of(view.theme, view.field.setting.as_ref()),
                    canvas,
                    textures: f.textures,
                    // The construction panel is painted over the field below.
                    occluder: f.battle.construction.then_some(layout.construction_panel),
                },
            );
            drop(paint);
            if let Some(perf) = perf {
                perf.record_scene(drawn);
                perf.record_battlefield_clip(field_painter.clip_rect());
            }
            if f.battle.low_hp {
                // Over the field and the forest, under every HUD frame.
                low_hp_edge(
                    ui,
                    canvas,
                    !view.paused && !view.global_paused,
                    f.settings.reduced_effects,
                );
            }
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
            f.battle.pause_banner_rect = (view.paused && !f.battle.construction)
                .then(|| pause_banner(ui, &layout, view, &f.settings.bindings));
            // The callout sits in the reserved band over a zoomed field.
            let callout = f.settings.show_guidance && view.first_deployment && view.personal();
            if callout {
                field_hint_callout(ui, layout.hint_callout, f, view);
            }
            // The boss banner counts wall time while the battle runs (UX-11),
            // under the callout and left of the open construction panel.
            f.battle.boss_banner_rect = boss_banner(
                ui,
                layout.arena,
                layout.boss_banner(callout, f.battle.construction),
                &mut f.battle.boss_banner,
                &view.boss,
                !view.paused && !view.global_paused,
                f.settings.reduced_effects,
            );
            corner_hud(ui, &layout, f, hud_view);
            // The battle has no status line (C-10): a new status message is
            // a toast. The battle's one toast takes the slot over the loot
            // receipt.
            toast::status_toast(f);
            if !toast::battle_toast(ui, layout.receipt_slot, f)
                && let Some(receipt) = &view.last_loot
            {
                let area = layout.receipt_slot;
                ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                    ui.set_clip_rect(area.intersect(ui.clip_rect()));
                    ui.label(
                        RichText::new("Last loot · already banked")
                            .size(10.0)
                            .color(MUTED),
                    );
                    armory::loot_receipt(ui, receipt, f.settings);
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
                            // The frame fills its whole layout rect (margin
                            // and stroke are 11 pt a side): the cues, the
                            // callout occluder and the hover refusal all
                            // treat that rect as covered.
                            ui.set_width(panel.width() - 22.0);
                            ui.set_min_height(panel.height() - 22.0);
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .max_height(panel.height() - 24.0)
                                .show(ui, |ui| construction_controls(ui, f, view));
                        });
                });
                // Spawns the panel hides get a chevron left of it.
                let cues = occluded_spawn_cues(arena, panel, &views.field.enemies);
                incoming_cues(ui.painter(), &cues, arena.lane_height());
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
                f.battle.legend_rect = Some(ui.painter().text(
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
                ));
            }
            let bar = layout.bar;
            let observed = view.mode == Mode::Npc;
            // Every bar widget is counted into this frame's rows (C-10).
            let mut rows = BarRows::default();
            ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
                // Two rows end above the key legend (CF-1-08).
                ui.spacing_mut().item_spacing.y = BAR_ROW_GAP;
                ui.spacing_mut().button_padding.y = BAR_BUTTON_PADDING_Y;
                ui.horizontal_wrapped(|ui| {
                    let paused = view.paused;
                    let label = if !paused {
                        "Pause"
                    } else if view.tick == 0 {
                        "Begin defense"
                    } else {
                        "Resume defense"
                    };
                    // Amber only while paused: Begin or Resume is the
                    // battle's one primary; a running battle has none.
                    let response = rows.widget(if paused {
                        primary_action::primary_in_bar(ui, label)
                    } else {
                        ui.button(RichText::new(label).strong())
                    });
                    if response.clicked() {
                        f.dispatch(Command::SetPaused(!paused), "");
                        f.battle.set_construction(false);
                        f.battle.fire_latched = false;
                    }
                    if view.personal() && !f.battle.construction {
                        personal_controls(ui, f, view, &mut rows);
                    }
                    if view.any_blueprint
                        && rows
                            .widget(ui.button(if f.battle.construction {
                                "Close construction"
                            } else {
                                "Build defenses"
                            }))
                            .clicked()
                    {
                        f.battle.set_construction(!f.battle.construction);
                        f.battle.fire_latched = false;
                    }
                    if observed {
                        rows.widget(ui.label("Observe"));
                        for speed in [1, 2, 4] {
                            rows.widget(ui.selectable_value(
                                &mut f.battle.speed,
                                speed,
                                format!("{speed}×"),
                            ));
                        }
                        // Tactics edits whoever holds the defense: Mara,
                        // or the recruit taking over (ruling D16).
                        let defender = a.defender();
                        let title = game.defender_card(defender).map_or_else(
                            || "Tactics".to_owned(),
                            |c| format!("{} · tactics", c.name),
                        );
                        let response = rows.widget(ui.button("Tactics"));
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
                                    ui.label(RichText::new(&title).strong());
                                    crate::inspector::npc_decision(
                                        ui,
                                        a,
                                        f.launch.reviewing("npc-expanded"),
                                    );
                                    character::tactics(ui, f, defender);
                                });
                        });
                    }
                    // Withdraws only after a 0.8 s hold (UX-13).
                    retreat_button(ui, f, observed, &mut rows);
                    let management = ui.menu_button("Management", |ui| {
                        ui.set_max_width(320.);
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        if ui.button("Settings").clicked() {
                            f.open_settings();
                        }
                        if ui.button("Save").clicked() {
                            f.request_save(SaveIntent::Manual);
                        }
                        if ui.button("Save and return to main menu").clicked() {
                            f.dispatch(Command::SetPaused(true), "");
                            f.battle.fire_latched = false;
                            f.request_save(SaveIntent::Menu);
                        }
                        if view.personal() && !f.battle.construction {
                            field_actions(ui, f, &ctx, view);
                        }
                        ui.separator();
                        ui.checkbox(&mut f.battle.inspect_log, "Combat log");
                        egui::ScrollArea::vertical()
                            .max_height(280.)
                            .show(ui, |ui| field_details(ui, f, view, hud_view));
                    });
                    rows.add(management.response.rect);
                });
            });
            f.battle.bar_rows = rows.rows();
            f.battle.bar_bounds = rows.bounds();
        });
}
