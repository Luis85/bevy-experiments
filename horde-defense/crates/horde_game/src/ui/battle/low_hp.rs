//! The low-HP edge warning (UX-12, G2.7; design-ux-flow §2 row 21 and §4
//! "Low HP"): while the defender is low, an 8 pt RED band lies inside all
//! four canvas edges and the HP text ends in "· LOW".
//!
//! "Low" has hysteresis, so the warning never flickers around one threshold:
//! the defender becomes low at `LOW_HP_ENTER_PCT` of maximum HP or less and
//! stays low until above `LOW_HP_EXIT_PCT`. The remembered half is
//! `BattleUi::low_hp`, presentation state; nothing here touches the
//! simulation. It applies to the main battle (a personal or an observed
//! main-run defender), not to the worker observer window.
use crate::art::RED;
use bevy_egui::egui::{self, Rect, pos2};

/// The defender becomes low at this percentage of maximum HP or less.
pub(in crate::ui) const LOW_HP_ENTER_PCT: i64 = 30;
/// A low defender stays low up to this percentage and leaves above it.
pub(in crate::ui) const LOW_HP_EXIT_PCT: i64 = 35;
/// The band's width inside each canvas edge, in points.
pub(in crate::ui) const LOW_HP_EDGE: f32 = 8.0;
/// The band's strength where it does not pulse: reduced effects, and a
/// paused battle in either mode.
pub(in crate::ui) const STATIC_ALPHA: f32 = 0.40;
/// The pulse of normal effects: between these strengths, `PULSE_HZ` times a
/// second.
pub(in crate::ui) const PULSE_MIN_ALPHA: f32 = 0.25;
pub(in crate::ui) const PULSE_MAX_ALPHA: f32 = 0.50;
const PULSE_HZ: f64 = 1.0;

/// Whether a defender at `hp` of `max_hp` is low, given whether it was low
/// before (`was_low`): at 30% or less it is, above 35% it is not, and in
/// between it stays what it was. Exact integer arithmetic; a defender without
/// a maximum is never low. Pure, and stable when applied twice.
pub(in crate::ui) fn low_hp(hp: i32, max_hp: i32, was_low: bool) -> bool {
    if max_hp <= 0 {
        return false;
    }
    let (hp, max_hp) = (i64::from(hp) * 100, i64::from(max_hp));
    if hp <= LOW_HP_ENTER_PCT * max_hp {
        true
    } else if hp > LOW_HP_EXIT_PCT * max_hp {
        false
    } else {
        was_low
    }
}

/// The band's strength at wall-clock second `time`. While `pulsing` it runs
/// between 0.25 and 0.50 once a second, at its mean on every whole second;
/// otherwise it is the static 0.40. Pure.
pub(in crate::ui) fn edge_alpha(time: f64, pulsing: bool) -> f32 {
    if !pulsing {
        return STATIC_ALPHA;
    }
    let mean = (PULSE_MIN_ALPHA + PULSE_MAX_ALPHA) * 0.5;
    let swing = (PULSE_MAX_ALPHA - PULSE_MIN_ALPHA) * 0.5;
    // The phase is taken within one period, so a long session keeps its
    // precision.
    let phase = (time * PULSE_HZ).rem_euclid(1.0) as f32;
    mean + swing * (phase * std::f32::consts::TAU).sin()
}

/// The four bands inside `canvas`: top, bottom, left, right. The side bands
/// run between the top and bottom ones, so no corner is blended twice. Pure.
pub(in crate::ui) fn edge_rects(canvas: Rect) -> [Rect; 4] {
    let (min, max) = (canvas.min, canvas.max);
    let edge = LOW_HP_EDGE
        .min(canvas.width() * 0.5)
        .min(canvas.height() * 0.5);
    [
        Rect::from_min_max(min, pos2(max.x, min.y + edge)),
        Rect::from_min_max(pos2(min.x, max.y - edge), max),
        Rect::from_min_max(pos2(min.x, min.y + edge), pos2(min.x + edge, max.y - edge)),
        Rect::from_min_max(pos2(max.x - edge, min.y + edge), pos2(max.x, max.y - edge)),
    ]
}

/// The HP text of the defender HUD: "<name> · HP h / m", ending in "· LOW"
/// while the defender is low, so the warning never rests on colour alone.
pub(in crate::ui) fn hp_text(defender: &str, hp: i32, max_hp: i32, low: bool) -> String {
    let low = if low { " · LOW" } else { "" };
    format!("{defender} · HP {hp} / {max_hp}{low}")
}

/// Paints the band inside `canvas`. It pulses only in normal effects while
/// the battle runs (`running`); reduced effects and a paused battle hold it
/// static.
pub(super) fn low_hp_edge(ui: &egui::Ui, canvas: Rect, running: bool, reduced: bool) {
    let pulsing = running && !reduced;
    let alpha = edge_alpha(ui.input(|input| input.time), pulsing);
    if pulsing {
        // The pulse runs on the wall clock, not on the next input.
        ui.ctx().request_repaint();
    }
    let painter = ui.painter_at(canvas);
    let fill = RED.gamma_multiply(alpha);
    for band in edge_rects(canvas) {
        painter.rect_filled(band, 0.0, fill);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_hp_enters_at_30_percent_and_leaves_above_35() {
        // (hp, max, was_low, low)
        for (hp, max, was, low) in [
            (100, 100, false, false),
            (100, 100, true, false),
            (36, 100, true, false),
            (35, 100, true, true),
            (35, 100, false, false),
            (31, 100, true, true),
            (31, 100, false, false),
            (30, 100, false, true),
            (0, 100, false, true),
            (-4, 100, false, true),
            // Exact at maxima the percentages do not divide: 30% of 13 is
            // 3.9, 35% is 4.55.
            (3, 13, false, true),
            (4, 13, false, false),
            (4, 13, true, true),
            (5, 13, true, false),
            // No maximum: never low.
            (0, 0, true, false),
        ] {
            assert_eq!(low_hp(hp, max, was), low, "{hp}/{max}, was {was}");
            // Applying it again with its own answer changes nothing.
            assert_eq!(low_hp(hp, max, low), low, "{hp}/{max}: not stable");
        }
        assert_eq!((LOW_HP_ENTER_PCT, LOW_HP_EXIT_PCT), (30, 35));
    }

    #[test]
    fn edge_pulses_between_a_quarter_and_a_half_once_a_second() {
        for whole in [0.0, 7.0, 86_400.0] {
            for (offset, alpha) in [(0.0, 0.375), (0.25, 0.50), (0.5, 0.375), (0.75, 0.25)] {
                let got = edge_alpha(whole + offset, true);
                assert!((got - alpha).abs() < 1e-4, "{whole}+{offset}: {got}");
                assert_eq!(edge_alpha(whole + offset, false), STATIC_ALPHA);
            }
        }
        for step in 0..1000 {
            let alpha = edge_alpha(f64::from(step) * 0.0173, true);
            assert!(
                (PULSE_MIN_ALPHA..=PULSE_MAX_ALPHA).contains(&alpha),
                "{alpha}"
            );
        }
    }

    #[test]
    fn edge_bands_tile_the_canvas_border_without_overlap() {
        let canvas = Rect::from_min_max(pos2(10.0, 20.0), pos2(810.0, 520.0));
        let bands = edge_rects(canvas);
        for (i, a) in bands.iter().enumerate() {
            assert!(canvas.contains_rect(*a), "{a:?}");
            for b in &bands[i + 1..] {
                assert!(a.intersect(*b).area() == 0.0, "{a:?} overlaps {b:?}");
            }
        }
        let area: f32 = bands.iter().map(Rect::area).sum();
        let inner = canvas.shrink(LOW_HP_EDGE);
        assert!((area - (canvas.area() - inner.area())).abs() < 0.5);
    }
}
