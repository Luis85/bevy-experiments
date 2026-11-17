use super::*;
use super::{
    input::{preview_aim, reticle_ownership, target_cell},
    overlays::arrival_groups,
};
use crate::art::TEXT;
use bevy_egui::egui::{self, FontId, pos2, vec2};
use horde_core::content::Mode;
use horde_core::sim::Game;
#[test]
fn all_attack_controls_and_the_single_line_helper_fit_above_the_minimum_arena() {
    for size in [vec2(1440.0, 900.0), vec2(1050.0 / 1.35, 700.0 / 1.35)] {
        let canvas = Rect::from_min_size(egui::Pos2::ZERO, size);
        // The selector anchors to the arena, so every geometry shares it.
        let layout = BattleLayout::new(canvas, 3, 12.0, true);
        let (arena, selector) = (layout.arena, layout.selector);
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
        let area = Rect::from_min_size(pos2(20.0, 120.0), vec2(729.0, 310.0));
        let arena =
            art::ArenaProjection::for_attempt(art::fit_field(area, area, lanes, width).rect, a);
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

/// Pin: a top-lane loot rise, icon and the quality label 29 pt above it, stays
/// inside the effects clip for its whole visible life (ages to 1.0) at the
/// progressed peak (17x30) and the 3x12 opening, at both sizes, and so does a
/// bottom-lane rise at 3x12; a 48-pt hit envelope at the top-lane feet fits
/// too, and the clip never reaches the control bar.
#[test]
fn lane_zero_drop_envelope_fits_the_effects_clip_at_progressed_peak() {
    for viewport in [vec2(1440.0, 900.0), vec2(1050.0 / 1.35, 700.0 / 1.35)] {
        let canvas = Rect::from_min_size(pos2(0.0, 0.0), viewport);
        for (lanes, width) in [(17, 30.0), (3, 12.0)] {
            let layout = BattleLayout::new(canvas, lanes, width, false);
            let clip = layout.effects_clip;
            let arena = art::ArenaProjection::with_geometry(layout.field.rect, lanes, width)
                .with_effects_clip(clip);
            assert_eq!(arena.rect, layout.field.rect);
            assert_eq!(arena.effects_clip, clip);
            assert!(clip.bottom() <= layout.bar.top());
            let mut rows = vec![0.5];
            if lanes == 3 {
                rows.push(f32::from(lanes) - 0.5);
            }
            for row in rows {
                for item_id in 0..3 {
                    for step in 0..=20 {
                        let age = step as f32 / 20.0;
                        let drop = crate::ui::loot::drop_icon_center(
                            arena,
                            [width * 0.5, row],
                            item_id,
                            age,
                        );
                        let icon = Rect::from_center_size(drop, vec2(25.0, 25.0));
                        assert!(
                            clip.contains_rect(icon) && drop.y - 29.0 >= clip.top() + 4.0,
                            "{lanes}x{width} row {row} age {age} in {viewport:?}: \
                             {icon:?} or its label leaves {clip:?}"
                        );
                    }
                }
            }
            let feet = arena.world(width * 0.5, 0.5);
            assert!(clip.contains_rect(Rect::from_center_size(feet, vec2(48.0, 48.0))));
        }
    }
}

/// CF-1-09: the effects headroom shares the top band with the HUD. On the
/// zoomed 7x16 and 9x18 fields at both sizes, with and without the hint
/// band, in normal and reduced effects, no damage number, MISS or loot
/// fly-up over a top-lane enemy meets the corner panels, the attack selector
/// or the loot receipt at any age, and the clip keeps its headroom. Where the
/// field fills the arena, none meets the compact READY / PAUSED tag under the
/// battle HUD either (CF-3-09; damage-4x showed a MISS through "to resume").
#[test]
fn headroom_popups_never_overlap_hud_rects() {
    use crate::{settings::Settings, ui::review::LootCaptureMode};
    use horde_core::{
        balance::BalanceSettings,
        content::{EnemyKind, ItemDefId},
        quality::{LootEvent, Quality},
        sim::{Event, EventKind},
    };
    let (mut checked, mut tags, mut in_tag_column) = (0, 0, 0);
    for (lanes, width) in [(7u8, 16u8), (9, 18)] {
        let mut game = Game::default();
        game.campaign_mut()
            .set_balance_settings(BalanceSettings {
                start_lanes: lanes,
                max_lanes: lanes,
                start_width: width,
                max_width: width,
                ..Default::default()
            })
            .unwrap();
        game.deploy(0, Mode::Personal).unwrap();
        let mut a = game.attempt().unwrap().clone();
        a.enemies.clear();
        a.events.clear();
        a.loot_events.clear();
        a.corpses.clear();
        let w = a.width_yards;
        let columns = [0.4, w * 0.2, w * 0.4, w * 0.5, w * 0.6, w * 0.8, w - 0.4];
        let mut seq = 0;
        for (i, &x) in columns.iter().enumerate() {
            for kind in [EventKind::Hit, EventKind::WeakPoint, EventKind::Miss] {
                game.spawn_enemy(&mut a, EnemyKind::Runner, 0, false);
                let enemy = a.enemies.last_mut().unwrap();
                enemy.x = x;
                let (id, feet) = (enemy.id, [enemy.x, enemy.y()]);
                seq += 1;
                a.events.push_back(Event {
                    seq,
                    kind,
                    from: a.defender_position(),
                    to: feet,
                    value: if kind == EventKind::Miss { 0 } else { 4632 },
                    tick: a.tick,
                    actor_id: None,
                    target_id: Some(id),
                    lane: Some(0),
                    duration_ticks: 0,
                });
            }
            a.loot_events.push(LootEvent {
                item_id: 900 + i as u64,
                definition: ItemDefId::at(4),
                level: 1,
                quality: if i % 2 == 0 {
                    Quality::Rare
                } else {
                    Quality::Uncommon
                },
                name: "Padded vest".into(),
                tick: a.tick,
                at: [x, 0.5],
            });
        }
        for (size, zoom) in [
            (vec2(1440.0, 900.0), 1.0),
            (vec2(1050.0, 700.0) / 1.35, 1.35),
        ] {
            let canvas = Rect::from_min_size(pos2(0.0, 0.0), size);
            for band in [false, true] {
                let layout = BattleLayout::new(canvas, a.lane_count, a.width_yards, band);
                assert_eq!(layout.effects_clip.top(), canvas.top() + 64.0);
                let keep_clear = layout.effects_keep_clear();
                // The compact tag between the battle HUD and the arena.
                let tag = layout.pause_banner_slot.bottom() <= layout.arena.top();
                tags += usize::from(tag);
                for reduced in [false, true] {
                    let settings = Settings {
                        reduced_effects: reduced,
                        ..Settings::default()
                    };
                    let ctx = egui::Context::default();
                    crate::settings::install_font(&ctx);
                    crate::settings::configure(&ctx, zoom);
                    let mut damage = crate::ui::damage::Feedback::default();
                    let mut loot = crate::ui::loot::Feedback::default();
                    let label = format!(
                        "{lanes}x{width} band {band} reduced {reduced} in {:?}",
                        canvas.size()
                    );
                    for step in 0..=22 {
                        let input = egui::RawInput {
                            time: Some(f64::from(step) * 0.07),
                            screen_rect: Some(canvas),
                            ..Default::default()
                        };
                        let mut output = ctx.run_ui(input, |ui| {
                            let projection =
                                art::ArenaProjection::for_attempt(layout.field.rect, &a)
                                    .with_effects_clip(layout.effects_clip);
                            crate::ui::damage::draw_for(
                                ui,
                                &a,
                                projection,
                                crate::battle_view::SceneBlend::NONE,
                                0,
                                &mut damage,
                                &settings,
                                LootCaptureMode::Disabled,
                                &keep_clear,
                            );
                            crate::ui::loot::draw_for(
                                ui,
                                projection,
                                &a,
                                0,
                                Default::default(),
                                &mut loot,
                                LootCaptureMode::Disabled,
                                &keep_clear,
                                settings.palette,
                                settings.numbers,
                                settings.reduced_effects,
                            );
                        });
                        output.textures_delta.clear();
                        for clipped in &output.shapes {
                            let drawn = clipped
                                .shape
                                .visual_bounding_rect()
                                .intersect(clipped.clip_rect);
                            if !drawn.is_positive() {
                                continue;
                            }
                            checked += 1;
                            for hud in keep_clear {
                                assert!(
                                    !drawn.intersects(hud),
                                    "{label} at {:.2} s: popup {drawn:?} meets HUD {hud:?}",
                                    f64::from(step) * 0.07
                                );
                            }
                            // CF-3-09: the READY / PAUSED tag of a field
                            // that fills the arena lies in the headroom too.
                            if tag {
                                assert!(
                                    !drawn.intersects(layout.pause_banner_slot),
                                    "{label} at {:.2} s: popup {drawn:?} meets the pause \
                                     tag slot {:?}",
                                    f64::from(step) * 0.07,
                                    layout.pause_banner_slot
                                );
                                let slot = layout.pause_banner_slot;
                                in_tag_column += usize::from(
                                    drawn.max.x > slot.min.x && drawn.min.x < slot.max.x,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(checked > 1000, "only {checked} shapes drawn");
    assert!(
        tags >= 4,
        "only {tags} geometries put the pause tag over the arena"
    );
    assert!(in_tag_column > 0, "no popup was drawn under the pause tag");
}

/// CF-3-05: a kill's banked-coin pop-up and a drop's quality label anchor
/// near the same kill point, and the normal-effects coin rose onto the label
/// row ("+18cEpic · Marksman rifle" in the quality-loot still at
/// 1050x700@1.35). For a kill that banks 18 coins and a drop of an Epic
/// rifle at and around that kill point, at coin ages over the coin's whole
/// life and drop ages from its first frame, at both sizes, in normal and
/// reduced effects, the coin the cue pass paints never meets the drop label
/// the loot pass paints on the same frame.
#[test]
fn kill_coin_popups_keep_clear_of_drop_labels() {
    use crate::{settings::Palette, ui::review::LootCaptureMode};
    use horde_core::{
        content::ItemDefId,
        quality::{LootEvent, Quality},
        sim::{Event, EventKind},
    };
    let mut game = Game::default();
    game.deploy(0, Mode::Personal).unwrap();
    let mut a = game.attempt().unwrap().clone();
    a.enemies.clear();
    a.events.clear();
    a.loot_events.clear();
    a.corpses.clear();
    a.paused = true;
    let kill = [a.width_yards * 0.5, f32::from(a.lane_count) * 0.5];
    let kill_tick = a.tick;
    a.events.push_back(Event {
        seq: 1,
        kind: EventKind::Kill,
        from: a.defender_position(),
        to: kill,
        value: 18,
        tick: kill_tick,
        actor_id: None,
        target_id: None,
        lane: None,
        duration_ticks: 0,
    });
    let (mut checked, mut both) = (0, 0);
    for (size, zoom) in [
        (vec2(1440.0, 900.0), 1.0),
        (vec2(1050.0, 700.0) / 1.35, 1.35),
    ] {
        let canvas = Rect::from_min_size(pos2(0.0, 0.0), size);
        let layout = BattleLayout::new(canvas, a.lane_count, a.width_yards, false);
        let keep_clear = layout.effects_keep_clear();
        for reduced in [false, true] {
            for dx in [-1.0, 0.0, 1.0_f32] {
                for step in -5..=10 {
                    let dy = step as f32 * 0.1;
                    for coin_ticks in [0, 9, 18, 26] {
                        let mut a = a.clone();
                        a.tick = kill_tick + coin_ticks;
                        a.loot_events.push(LootEvent {
                            item_id: 77,
                            definition: ItemDefId::at(4),
                            level: 3,
                            quality: Quality::Epic,
                            name: "Marksman rifle".into(),
                            tick: kill_tick,
                            at: [kill[0] + dx, kill[1] + dy],
                        });
                        let ctx = egui::Context::default();
                        crate::settings::install_font(&ctx);
                        crate::settings::configure(&ctx, zoom);
                        let mut loot = crate::ui::loot::Feedback::default();
                        for time in [0.0, 0.05, 0.3] {
                            let input = egui::RawInput {
                                time: Some(time),
                                screen_rect: Some(canvas),
                                ..Default::default()
                            };
                            let mut output = ctx.run_ui(input, |ui| {
                                art::battlefield_for_attempt(
                                    &ui.painter_at(layout.arena),
                                    layout.field.rect,
                                    &a,
                                    kill,
                                    crate::battle_view::SceneBlend::NONE,
                                    art::RenderOptions {
                                        construction: false,
                                        outlines: true,
                                        numbers: true,
                                        reduced,
                                        blood: false,
                                        scene: art::SceneLook::neutral(0),
                                        sky: art::SkyClock::default(),
                                        canvas,
                                        textures: Default::default(),
                                        occluder: None,
                                    },
                                );
                                let projection =
                                    art::ArenaProjection::for_attempt(layout.field.rect, &a)
                                        .with_effects_clip(layout.effects_clip);
                                crate::ui::loot::draw_for(
                                    ui,
                                    projection,
                                    &a,
                                    0,
                                    Default::default(),
                                    &mut loot,
                                    LootCaptureMode::Disabled,
                                    &keep_clear,
                                    Palette::default(),
                                    true,
                                    reduced,
                                );
                            });
                            output.textures_delta.clear();
                            let mut coin = Rect::NOTHING;
                            let mut label = None;
                            for clipped in &output.shapes {
                                if let egui::Shape::Text(text) = &clipped.shape {
                                    let rect = text.galley.rect.translate(text.pos.to_vec2());
                                    match text.galley.text() {
                                        "+18c" => coin = coin.union(rect),
                                        "Epic · Marksman rifle" => label = Some(rect),
                                        _ => {}
                                    }
                                }
                            }
                            checked += 1;
                            let Some(label) = label else { continue };
                            if !coin.is_positive() {
                                continue;
                            }
                            both += 1;
                            assert!(
                                !coin.intersects(label),
                                "{size:?} reduced {reduced}, drop at ({dx}, {dy:.1}), coin \
                                 {coin_ticks} ticks old, drop {time} s old: the coin {coin:?} \
                                 meets the drop label {label:?}"
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(
        both * 2 > checked,
        "only {both} of {checked} frames drew both"
    );
}

/// `BarRows` starts a row when a rect's centre lies below the row so far: a
/// shorter widget centred in a row, and a taller one that deepens it, join
/// the row; nothing counted is 0 rows.
#[test]
fn bar_rows_count_rows_by_vertical_centre() {
    let rect =
        |x: f32, top: f32, bottom: f32| Rect::from_min_max(pos2(x, top), pos2(x + 40.0, bottom));
    let mut rows = BarRows::default();
    assert_eq!(rows.rows(), 0);
    rows.add(rect(0.0, 100.0, 132.0));
    rows.add(rect(50.0, 101.0, 131.0));
    rows.add(rect(100.0, 96.0, 140.0));
    assert_eq!(rows.rows(), 1);
    // Its centre is still inside the deepened first row.
    rows.add(rect(150.0, 124.0, 150.0));
    assert_eq!(rows.rows(), 1);
    rows.add(rect(0.0, 157.0, 189.0));
    rows.add(rect(50.0, 157.0, 189.0));
    assert_eq!(rows.rows(), 2);
    rows.add(rect(0.0, 196.0, 218.0));
    assert_eq!(rows.rows(), 3);
}

/// Closing: a core action a rebind left without a key (Pause rebound to R
/// takes Reload's key away, to Mouse1 Fire's, to Mouse2 Contact melee's) is
/// named "<action> is unbound" with where to bind it, never as an empty key,
/// in the opening lesson and in its own lesson; the gun's hover, the
/// personal buttons' hovers and the Field actions shortcuts read "Unbound".
#[test]
fn hints_name_an_unbound_core_action_and_where_to_bind_it() {
    use crate::battle_view::FieldHint;
    use crate::settings::{Action, BindingId, Settings};
    use overlays::hint_text;
    for (key, action, lesson) in [
        ("R", Action::Reload, FieldHint::Reload),
        ("Mouse1", Action::Fire, FieldHint::Shoot),
        ("Mouse2", Action::Melee, FieldHint::Contact),
    ] {
        let mut settings = Settings::default();
        settings.set_binding(BindingId::Core(Action::Pause), key.into());
        assert_eq!(settings.binding(BindingId::Core(action)), "");
        let unbound = format!("{} is unbound", action.name());
        for (hint, paused) in [(FieldHint::Opening, true), (lesson, false)] {
            let text = hint_text(hint, paused, &settings.bindings);
            assert!(text.contains(&unbound), "{text}");
            assert!(text.contains("Settings › Controls"), "{text}");
            assert!(!text.contains("  ") && !text.contains(" ,"), "{text}");
        }
        assert_eq!(
            attack::key_text(&settings, action),
            crate::settings::UNBOUND
        );
    }
    // The bound lessons keep their wording.
    let settings = Settings::default();
    assert_eq!(
        hint_text(FieldHint::Opening, true, &settings.bindings),
        "Enemies walk in from the right. Hold Mouse1 to shoot, R reloads, Mouse2 strikes an enemy at your line. Press Begin defense or Escape to start."
    );
    assert_eq!(
        attack::gun_hint("Rifle", &settings),
        "Rifle: hold Mouse1 to fire."
    );
    assert_eq!(attack::key_text(&settings, Action::Heal), "1");
    let mut settings = Settings::default();
    settings.set_binding(BindingId::Core(Action::Pause), "Mouse1".into());
    assert_eq!(
        attack::gun_hint("Rifle", &settings),
        "Rifle: Fire is unbound, bind it in Settings › Controls."
    );
    let mut settings = Settings::default();
    settings.set_binding(BindingId::Core(Action::Pause), "1".into());
    assert_eq!(
        attack::key_text(&settings, Action::Heal),
        crate::settings::UNBOUND
    );
}
