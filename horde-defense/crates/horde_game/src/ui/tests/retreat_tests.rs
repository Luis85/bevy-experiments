//! W3-13 (UX-13, G2.8): "Retreat & train" and "Recall & train" withdraw only
//! after a 0.8 s wall-clock hold (design-ux-flow §4 "Retreat hold").
use super::*;
use crate::art::AMBER;

/// The input-clock second the holds below begin at.
const T0: f64 = 100.0;
const RETREAT: &str = "Retreat & train";
const RECALL: &str = "Recall & train";
const HOLDING: &str = "Hold to retreat…";
/// The early-release hint of each button.
const RETREAT_HINT: &str = "Hold to retreat";
const RECALL_HINT: &str = "Hold to recall";

/// Where the text run reading `label` is drawn in `view`.
fn label_position(h: &Harness, f: &mut FrontendParts, view: View, label: &str) -> Pos2 {
    let output = h.frame(f, view, vec![]);
    output
        .shapes
        .iter()
        .find_map(|clipped| text_position(&clipped.shape, label))
        .unwrap_or_else(|| panic!("Rendered action not found: {label}"))
}
/// The pointer resting at `position` with its button state unchanged.
fn resting(position: Pos2) -> Vec<Event> {
    vec![Event::PointerMoved(position)]
}
fn painted(output: &FullOutput, label: &str) -> bool {
    output
        .shapes
        .iter()
        .any(|c| shape_has_text(&c.shape, label))
}
/// The widest AMBER-filled rect painted inside `bar`: the hold's fill (a
/// running battle has no amber primary).
fn amber_fill_width(output: &FullOutput, bar: Rect) -> f32 {
    fn widest(shape: &egui::epaint::Shape, bar: Rect) -> f32 {
        match shape {
            egui::epaint::Shape::Rect(rect)
                if rect.fill == AMBER && bar.contains_rect(rect.rect) =>
            {
                rect.rect.width()
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().map(|s| widest(s, bar)).fold(0.0, f32::max)
            }
            _ => 0.0,
        }
    }
    output
        .shapes
        .iter()
        .map(|c| widest(&c.shape, bar))
        .fold(0.0, f32::max)
}
/// An observed (Npc) defense on the battle screen.
fn observed_battle() -> IsolatedFrontend {
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().attempts = 1;
    f.game.deploy_country_stage(0, Mode::Npc).unwrap();
    f.game.set_paused(false);
    f.screen = Screen::Battle;
    fixture
}

#[test]
fn single_click_retreat_does_not_withdraw() {
    for (label, hint, observed) in [(RETREAT, RETREAT_HINT, false), (RECALL, RECALL_HINT, true)] {
        let mut fixture = if observed {
            observed_battle()
        } else {
            let mut fixture = IsolatedFrontend::new();
            fixture.deploy();
            fixture
        };
        let h = Harness::new();
        let f = &mut fixture.frontend;
        let runs = f.game.campaign().run_history.len();
        let position = label_position(&h, f, View::Full, label);
        h.frame_at(f, View::Full, pointer_at(position, true), T0);
        h.frame_at(f, View::Full, pointer_at(position, false), T0 + 0.05);
        assert!(f.screen == Screen::Battle, "{label}: one click withdrew");
        assert!(f.game.attempt().is_some(), "{label}");
        assert_eq!(f.game.campaign().run_history.len(), runs, "{label}");
        assert!(!f.game.global_paused(), "{label}: a click does not pause");
        // The early release says what the button wants, for 1.5 s.
        let output = h.frame_at(f, View::Full, resting(position), T0 + 1.0);
        assert!(painted(&output, hint), "{label}: no hold hint");
        assert_eq!(text_count(&output, label), 1, "{label}: the idle label");
        let output = h.frame_at(f, View::Full, resting(position), T0 + 1.7);
        assert!(!painted(&output, hint), "{label}: the hint outstays 1.5 s");
        // Clicking again and again never adds up to a hold.
        for click in 0..12 {
            let at = T0 + 2.0 + 0.2 * f64::from(click);
            h.frame_at(f, View::Full, pointer_at(position, true), at);
            h.frame_at(f, View::Full, pointer_at(position, false), at + 0.1);
        }
        assert!(f.screen == Screen::Battle, "{label}: repeated clicks");
        assert!(f.game.attempt().is_some(), "{label}");
    }
}

#[test]
fn held_retreat_withdraws_after_0_8s_and_opens_results() {
    for (label, observed) in [(RETREAT, false), (RECALL, true)] {
        let mut fixture = if observed {
            observed_battle()
        } else {
            let mut fixture = IsolatedFrontend::new();
            fixture.deploy();
            fixture
        };
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.game.campaign_mut().reward(25, 0, Mode::Personal);
        let coins = f.game.campaign().coins;
        let position = label_position(&h, f, View::Battle, label);
        h.frame_at(f, View::Battle, pointer_at(position, true), T0);
        // 0.75 s held: still the battle.
        for frame in 1..=15 {
            let output = h.frame_at(
                f,
                View::Battle,
                resting(position),
                T0 + 0.05 * f64::from(frame),
            );
            assert!(
                f.screen == Screen::Battle && f.game.attempt().is_some(),
                "{label}: withdrew after {} ms",
                frame * 50
            );
            assert_eq!(text_count(&output, label), 0, "{label}: frame {frame}");
        }
        // 0.85 s held: withdrawn, and the results are open.
        for frame in 16..=17 {
            h.frame_at(
                f,
                View::Battle,
                resting(position),
                T0 + 0.05 * f64::from(frame),
            );
        }
        assert!(
            f.screen == Screen::Results,
            "{label}: no results after 0.85 s"
        );
        assert!(f.game.attempt().is_none(), "{label}");
        assert_eq!(
            f.game.last_result().unwrap().outcome,
            Outcome::Withdrawal,
            "{label}"
        );
        assert_eq!(f.game.campaign().coins, coins, "{label}: coins stay banked");
        // The button still held down on the results does nothing more.
        let runs = f.game.campaign().run_history.len();
        for frame in 18..=40 {
            h.frame_at(
                f,
                View::Results,
                resting(position),
                T0 + 0.05 * f64::from(frame),
            );
        }
        assert_eq!(f.game.campaign().run_history.len(), runs, "{label}");
    }
}

/// The hold is wall time, capped per frame: one long frame (a stall, a
/// dragged window) never completes it.
#[test]
fn a_stalled_frame_does_not_complete_the_retreat_hold() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let position = label_position(&h, f, View::Battle, RETREAT);
    h.frame_at(f, View::Battle, pointer_at(position, true), T0);
    h.frame_at(f, View::Battle, resting(position), T0 + 5.0);
    assert!(f.screen == Screen::Battle, "a 5 s frame withdrew");
    assert!(f.game.attempt().is_some());
    // It counted as 0.1 s: 0.65 s more is not enough, 0.75 s more is.
    for frame in 1..=13 {
        h.frame_at(
            f,
            View::Battle,
            resting(position),
            T0 + 5.0 + 0.05 * f64::from(frame),
        );
    }
    assert!(f.screen == Screen::Battle);
    for frame in 14..=15 {
        h.frame_at(
            f,
            View::Battle,
            resting(position),
            T0 + 5.0 + 0.05 * f64::from(frame),
        );
    }
    assert!(f.screen == Screen::Results);
}

/// Frames 0.028 s apart: the hold stands at 3.5% per frame, so no frame sits
/// on a quarter. Reduced effects shows 0/25/50/75% and nothing between;
/// normal effects grows every frame.
#[test]
fn retreat_hold_stepped_in_reduced_effects() {
    for reduced in [false, true] {
        let mut fixture = IsolatedFrontend::new();
        fixture.deploy();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.settings.reduced_effects = reduced;
        let position = label_position(&h, f, View::Battle, RETREAT);
        let bar = f.battle.bar_bounds.unwrap();
        let idle = h.frame(f, View::Battle, vec![]);
        assert_eq!(amber_fill_width(&idle, bar), 0.0, "an idle bar has no fill");
        h.frame_at(f, View::Battle, pointer_at(position, true), T0);
        let mut widths = vec![0.0f32];
        for frame in 1..=26 {
            let output = h.frame_at(
                f,
                View::Battle,
                resting(position),
                T0 + 0.028 * f64::from(frame),
            );
            assert!(f.screen == Screen::Battle, "frame {frame}");
            assert!(painted(&output, HOLDING), "frame {frame}: label");
            widths.push(amber_fill_width(&output, f.battle.bar_bounds.unwrap()));
        }
        if reduced {
            let quarter = widths[8];
            assert!(quarter > 4.0, "{widths:?}");
            for (frame, width) in widths.iter().enumerate().skip(1) {
                let step = match frame {
                    1..=7 => 0.0,
                    8..=14 => 1.0,
                    15..=21 => 2.0,
                    _ => 3.0,
                };
                assert!(
                    (width - step * quarter).abs() < 0.01,
                    "reduced frame {frame}: {width} is not {step} quarters of {quarter}"
                );
            }
        } else {
            for frame in 1..widths.len() {
                assert!(
                    widths[frame] > widths[frame - 1],
                    "normal frame {frame} did not grow: {widths:?}"
                );
            }
        }
    }
}

#[test]
fn escape_cancels_a_retreat_hold_without_pausing() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    assert_eq!(f.settings.bindings[Action::Pause as usize], "Escape");
    let position = label_position(&h, f, View::Full, RETREAT);
    h.frame_at(f, View::Full, pointer_at(position, true), T0);
    for frame in 1..=8 {
        h.frame_at(
            f,
            View::Full,
            resting(position),
            T0 + 0.05 * f64::from(frame),
        );
    }
    let mut events = resting(position);
    events.push(key_event(Key::Escape, true));
    h.frame_at(f, View::Full, events, T0 + 0.45);
    let mut events = resting(position);
    events.push(key_event(Key::Escape, false));
    let output = h.frame_at(f, View::Full, events, T0 + 0.5);
    assert!(
        !f.game.global_paused(),
        "Escape cancelled the hold, not paused"
    );
    assert_eq!(text_count(&output, RETREAT), 1, "the idle label is back");
    // The button is still down: the cancelled hold does not start again.
    for frame in 11..=40 {
        h.frame_at(
            f,
            View::Full,
            resting(position),
            T0 + 0.05 * f64::from(frame),
        );
    }
    assert!(f.screen == Screen::Battle, "a cancelled hold withdrew");
    assert!(f.game.attempt().is_some());
    // With no hold, Escape is the Pause binding's again.
    h.frame_at(f, View::Full, pointer_at(position, false), T0 + 2.1);
    h.key(f, View::Full, Key::Escape);
    assert!(f.game.global_paused(), "Escape reaches Pause again");
    h.key(f, View::Full, Key::Escape);
    assert!(!f.game.global_paused());
    // A fresh press holds again.
    h.hold(f, View::Full, RETREAT, 0.9);
    assert!(f.screen == Screen::Results);
}

/// Keyboard twin of the pointer hold: the focused button with Enter or Space
/// held down. One key press alone never withdraws.
#[test]
fn focused_retreat_holds_on_enter_or_space() {
    for key in [Key::Enter, Key::Space] {
        let mut fixture = IsolatedFrontend::new();
        fixture.deploy();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        h.frame(f, View::Battle, vec![]);
        let id = egui::Id::new("lastlight_retreat_hold");
        h.ctx.memory_mut(|memory| memory.request_focus(id));
        h.frame_at(f, View::Battle, vec![], T0);
        // A tap: pressed and released within 0.1 s.
        h.frame_at(f, View::Battle, vec![key_event(key, true)], T0 + 0.05);
        h.frame_at(f, View::Battle, vec![key_event(key, false)], T0 + 0.1);
        for frame in 3..=30 {
            h.frame_at(f, View::Battle, vec![], T0 + 0.05 * f64::from(frame));
        }
        assert!(f.screen == Screen::Battle, "{key:?}: a tap withdrew");
        assert!(f.game.attempt().is_some(), "{key:?}");
        // Held down for 0.9 s.
        h.frame_at(f, View::Battle, vec![key_event(key, true)], T0 + 2.0);
        for frame in 1..=18 {
            h.frame_at(f, View::Battle, vec![], T0 + 2.0 + 0.05 * f64::from(frame));
        }
        assert!(f.screen == Screen::Results, "{key:?}: the held key");
        assert_eq!(
            f.game.last_result().unwrap().outcome,
            Outcome::Withdrawal,
            "{key:?}"
        );
    }
}
