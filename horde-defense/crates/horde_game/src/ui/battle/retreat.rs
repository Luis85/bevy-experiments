//! Hold-to-retreat (UX-13, G2.8; design-ux-flow §4 "Retreat hold"). One
//! click on "Retreat & train" used to end the defense. Now the button (and
//! the observed bar's "Recall & train") dispatches `Command::Withdraw` only
//! after it has been held for [`RETREAT_HOLD_SECS`] of wall time: with the
//! pointer, or focused with Enter or Space down. An AMBER fill grows left to
//! right while it is held (in quarter steps under reduced effects); letting
//! go early cancels and shows a hint; Escape cancels through the router.
//!
//! The hold's clock is the egui input clock, never the battle tick, so a
//! paused or a 4x battle holds exactly as long, and nothing here touches the
//! simulation until the one `Withdraw`.
use super::BarRows;
use crate::{
    art::{AMBER, INK},
    ui::{UiCtx, toast::ToastKind},
};
use bevy_egui::egui::{
    self, Key, Rect, Sense, StrokeKind, TextStyle, WidgetInfo, WidgetType, pos2, vec2,
};
use horde_core::command::Command;

/// How long the button is held before the defense is withdrawn, in seconds
/// of wall time.
pub(in crate::ui) const RETREAT_HOLD_SECS: f64 = 0.8;
/// How long the hint stays after an early release, in seconds.
pub(in crate::ui) const RETREAT_HINT_SECS: f64 = 1.5;
/// The most one frame adds to the hold, so a stalled frame never completes
/// it: 0.8 s takes at least eight frames with the button down.
const MAX_FRAME_SECS: f64 = 0.1;
/// The fill's steps under reduced effects: 0/25/50/75/100%.
const REDUCED_STEPS: f64 = 4.0;

/// The button's stable id: the hold survives the label changing under it.
pub(in crate::ui) fn retreat_hold_id() -> egui::Id {
    egui::Id::new("lastlight_retreat_hold")
}

/// The share of the button the fill covers after `held` seconds: linear in
/// normal effects, quarter steps (rounded down) in reduced effects.
pub(in crate::ui) fn retreat_fill(held: f64, reduced: bool) -> f32 {
    let fraction = (held / RETREAT_HOLD_SECS).clamp(0.0, 1.0);
    if reduced {
        ((fraction * REDUCED_STEPS).floor() / REDUCED_STEPS) as f32
    } else {
        fraction as f32
    }
}

/// What one frame does to the hold.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) enum HoldStep {
    /// Not held; nothing was held before.
    Idle,
    /// Held for this many seconds so far.
    Holding(f64),
    /// Let go before the hold completed.
    ReleasedEarly,
    /// Held long enough: withdraw.
    Complete,
}

/// One frame of the hold. `held` is the hold before this frame, `down`
/// whether the button is down now, `spent` whether this press already
/// completed or was cancelled, and `dt` the wall seconds since the last
/// frame. Pure.
pub(in crate::ui) fn hold_step(held: Option<f64>, down: bool, spent: bool, dt: f64) -> HoldStep {
    match (held, down && !spent) {
        (None, false) => HoldStep::Idle,
        (Some(_), false) => HoldStep::ReleasedEarly,
        (None, true) => HoldStep::Holding(0.0),
        (Some(held), true) => {
            let held = held + dt.clamp(0.0, MAX_FRAME_SECS);
            if held >= RETREAT_HOLD_SECS {
                HoldStep::Complete
            } else {
                HoldStep::Holding(held)
            }
        }
    }
}

/// The bar's retreat button: "Retreat & train", or "Recall & train" on an
/// observed defense. Counted into `rows` like every bar widget.
pub(super) fn retreat_button(ui: &mut egui::Ui, f: &mut UiCtx, observed: bool, rows: &mut BarRows) {
    let (idle, holding, hint) = if observed {
        ("Recall & train", "Hold to recall…", "Hold to recall")
    } else {
        ("Retreat & train", "Hold to retreat…", "Hold to retreat")
    };
    // Both labels are laid out and the wider one sizes the button, so the
    // bar does not move when the hold starts.
    let font = TextStyle::Button.resolve(ui.style());
    let width = [idle, holding]
        .map(|text| {
            ui.painter()
                .layout_no_wrap(text.to_owned(), font.clone(), INK)
                .size()
        })
        .into_iter()
        .fold(egui::Vec2::ZERO, egui::Vec2::max);
    let padding = ui.spacing().button_padding;
    let size = (width + 2.0 * padding).max(vec2(0.0, ui.spacing().interact_size.y));
    let (_, rect) = ui.allocate_space(size);
    // Click for focus, drag for the press: egui gives up a click after its
    // own 0.8 s limit or once the pointer wanders, but keeps a drag for as
    // long as the button stays down.
    let response = rows.widget(ui.interact(rect, retreat_hold_id(), Sense::click_and_drag()));
    let enabled = ui.is_enabled();

    // Held with the primary button, or focused with Enter or Space down.
    let (pointer_down, key_down, dt) = ui.input(|input| {
        (
            input.pointer.primary_down(),
            input.key_down(Key::Enter) || input.key_down(Key::Space),
            f64::from(input.unstable_dt),
        )
    });
    let down = enabled
        && ((response.is_pointer_button_down_on() && pointer_down)
            || (response.has_focus() && key_down));
    let frame = ui.ctx().cumulative_frame_nr();
    // A hold left behind by a battle that ended under it is not this one.
    let held = if f.battle.retreat_holding(frame) {
        f.battle.retreat_hold
    } else {
        None
    };
    // A discarded pass reruns the frame: it adds no time.
    let dt = if frame == f.battle.retreat_hold_frame {
        0.0
    } else {
        dt
    };
    if !down {
        f.battle.retreat_hold_spent = false;
    }
    let step = hold_step(held, down, f.battle.retreat_hold_spent, dt);
    f.battle.retreat_hold_frame = frame;
    f.battle.retreat_hold = match step {
        HoldStep::Holding(held) => Some(held),
        HoldStep::Idle | HoldStep::ReleasedEarly | HoldStep::Complete => None,
    };
    match step {
        HoldStep::Idle => {}
        HoldStep::Holding(_) => ui.ctx().request_repaint(),
        HoldStep::ReleasedEarly => {
            let now = ui.input(|input| input.time);
            f.status
                .toasts
                .push_at(hint, ToastKind::Info, RETREAT_HINT_SECS, now);
        }
        HoldStep::Complete => {
            // The button is still down: this press is used up.
            f.battle.retreat_hold_spent = true;
            f.battle.fire_latched = false;
            f.dispatch(
                Command::Withdraw,
                "Rewards retained. Rest, train, and retry.",
            );
        }
    }

    let label = if f.battle.retreat_hold.is_some() {
        holding
    } else {
        idle
    };
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, label));
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let radius = visuals.corner_radius;
        let painter = ui.painter();
        painter.rect(
            rect.expand(visuals.expansion),
            radius,
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            StrokeKind::Inside,
        );
        let galley = painter.layout_no_wrap(label.to_owned(), font.clone(), visuals.text_color());
        let at = rect.center() - galley.size() * 0.5;
        painter.galley(at, galley, visuals.text_color());
        let fill = f
            .battle
            .retreat_hold
            .map_or(0.0, |held| retreat_fill(held, f.settings.reduced_effects));
        if fill > 0.0 {
            let filled = Rect::from_min_max(
                rect.min,
                pos2(rect.left() + rect.width() * fill, rect.bottom()),
            );
            painter.rect_filled(filled, radius, AMBER);
            // The label over the fill is INK, as on an amber primary.
            let galley = painter.layout_no_wrap(label.to_owned(), font, INK);
            painter.with_clip_rect(filled).galley(at, galley, INK);
        }
    }
    response.on_hover_text(
        "Hold 0.8 s to end this defense and open Results. Earned coins and items stay banked.",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retreat_fill_is_linear_and_steps_in_quarters_when_reduced() {
        assert_eq!(retreat_fill(0.0, false), 0.0);
        assert!((retreat_fill(0.2, false) - 0.25).abs() < 1e-6);
        assert!((retreat_fill(0.5, false) - 0.625).abs() < 1e-6);
        assert_eq!(retreat_fill(0.8, false), 1.0);
        assert_eq!(retreat_fill(9.0, false), 1.0);
        for (held, step) in [
            (0.0, 0.0),
            (0.19, 0.0),
            (0.21, 0.25),
            (0.39, 0.25),
            (0.41, 0.5),
            (0.59, 0.5),
            (0.61, 0.75),
            (0.79, 0.75),
            (0.8, 1.0),
            (9.0, 1.0),
        ] {
            assert_eq!(retreat_fill(held, true), step, "{held}");
        }
        // Reduced effects never shows more than normal effects.
        for n in 0..=100 {
            let held = f64::from(n) * 0.01;
            assert!(retreat_fill(held, true) <= retreat_fill(held, false) + 1e-6);
        }
    }

    #[test]
    fn hold_step_needs_the_full_hold_and_a_fresh_press() {
        assert_eq!(hold_step(None, false, false, 0.016), HoldStep::Idle);
        // The press frame starts at zero whatever the frame took.
        assert_eq!(hold_step(None, true, false, 5.0), HoldStep::Holding(0.0));
        assert_eq!(
            hold_step(Some(0.25), true, false, 0.125),
            HoldStep::Holding(0.25 + 0.1),
            "one frame adds at most 0.1 s"
        );
        assert_eq!(
            hold_step(Some(0.25), true, false, 0.0625),
            HoldStep::Holding(0.3125)
        );
        assert_eq!(
            hold_step(Some(0.3), false, false, 0.05),
            HoldStep::ReleasedEarly
        );
        assert_eq!(
            hold_step(Some(0.69), true, false, 5.0),
            HoldStep::Holding(0.69 + 0.1)
        );
        assert_eq!(hold_step(Some(0.75), true, false, 0.05), HoldStep::Complete);
        // A spent press (completed, or cancelled by Escape) holds nothing.
        assert_eq!(hold_step(None, true, true, 0.05), HoldStep::Idle);
        // From the press frame, 0.8 s takes at least eight more frames.
        let mut held = Some(0.0);
        let mut frames = 1;
        while let HoldStep::Holding(next) = hold_step(held, true, false, 60.0) {
            held = Some(next);
            frames += 1;
        }
        assert!(frames >= 8, "{frames}");
        assert_eq!(RETREAT_HOLD_SECS, 0.8);
        assert_eq!(RETREAT_HINT_SECS, 1.5);
    }
}
