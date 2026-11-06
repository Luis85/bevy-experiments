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

/// Pin: the first half of a top-lane loot rise (and a 48-pt hit envelope at
/// the feet) stays inside the effects clip at the progressed peak (17x30) and
/// the 3x12 opening, at both sizes; the clip never reaches the control bar.
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
            let feet = arena.world(width * 0.5, 0.5);
            // ui::loot draws the icon 12 + age * 78 pt above the feet.
            for age in [0.0, 0.25, 0.5] {
                let drop = feet + vec2(0.0, -12.0 - age * 78.0);
                let icon = Rect::from_center_size(drop, vec2(25.0, 25.0));
                assert!(
                    clip.contains_rect(icon),
                    "{lanes}x{width} age {age} in {viewport:?}: {icon:?} leaves {clip:?}"
                );
            }
            assert!(clip.contains_rect(Rect::from_center_size(feet, vec2(48.0, 48.0))));
        }
    }
}
