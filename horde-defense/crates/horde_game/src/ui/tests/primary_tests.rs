//! Egui harness tests: the amber primary and the per-frame primary registry
//! (UX-02, W3-01; design-ux-flow §3 and decision 18).
use super::*;
use crate::art::{AMBER, INK};
use crate::ui::primary_action::{PRIMARY_HOVER, PRIMARY_MIN_SIZE, primary, primary_in_modal};

/// Every filled rect painted under `shape`, with its fill.
fn filled_rects(shape: &egui::epaint::Shape, out: &mut Vec<(Rect, Color32)>) {
    match shape {
        egui::epaint::Shape::Rect(rect) if rect.fill.a() > 0 => out.push((rect.rect, rect.fill)),
        egui::epaint::Shape::Vec(shapes) => {
            for shape in shapes {
                filled_rects(shape, out);
            }
        }
        _ => {}
    }
}

/// The smallest filled rect painted under the centre of the text run reading
/// exactly `label`: the button behind it.
fn button_under(output: &FullOutput, label: &str) -> Option<(Rect, Color32)> {
    let text = output
        .shapes
        .iter()
        .find_map(|clipped| labeled_bounds(&clipped.shape, label))?;
    let mut rects = Vec::new();
    for clipped in &output.shapes {
        filled_rects(&clipped.shape, &mut rects);
    }
    rects
        .into_iter()
        .filter(|(rect, _)| rect.contains(text.center()))
        .min_by(|a, b| a.0.area().total_cmp(&b.0.area()))
}

fn painted_color(output: &FullOutput, label: &str) -> Option<Color32> {
    output
        .shapes
        .iter()
        .find_map(|clipped| text_color(&clipped.shape, label))
}

/// One frame of three bare primaries: an enabled and a disabled screen
/// primary, then a modal one.
fn bare_primaries(h: &Harness, events: Vec<Event>) -> FullOutput {
    let mut output = h.ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, h.target.points())),
            events,
            ..RawInput::default()
        },
        |ui| {
            primary_action::clear_frame_primaries(ui.ctx());
            egui::CentralPanel::default().show(ui, |ui| {
                primary(ui, "Hold the line");
                ui.add_enabled_ui(false, |ui| {
                    primary(ui, "Not yet");
                });
                primary_in_modal(ui, "Confirm");
            });
        },
    );
    output.textures_delta.clear();
    output
}

#[test]
fn primary_is_amber_and_registered() {
    let h = Harness::large();
    let output = bare_primaries(&h, vec![]);
    let (rect, fill) = button_under(&output, "Hold the line").expect("primary painted");
    assert_eq!(fill, AMBER, "the primary is filled amber");
    assert_eq!(painted_color(&output, "Hold the line"), Some(INK));
    assert!(
        rect.width() >= PRIMARY_MIN_SIZE.x - 0.5 && rect.height() >= PRIMARY_MIN_SIZE.y - 0.5,
        "at least 190x40: {rect:?}"
    );
    assert_eq!(
        h.primaries(),
        [
            PrimaryRecord {
                label: "Hold the line".into(),
                surface: Surface::Screen,
            },
            PrimaryRecord {
                label: "Not yet".into(),
                surface: Surface::Screen,
            },
            PrimaryRecord {
                label: "Confirm".into(),
                surface: Surface::Modal,
            },
        ]
    );
    // Disabled: a faded amber with MUTED text.
    let (_, disabled) = button_under(&output, "Not yet").expect("disabled primary painted");
    assert!(
        disabled != AMBER && disabled.a() <= AMBER.gamma_multiply(0.4).a(),
        "disabled fill {disabled:?}"
    );
    assert_eq!(painted_color(&output, "Not yet"), Some(MUTED));
    // Hovered: the lighter hover fill.
    let over = rect.center();
    bare_primaries(&h, vec![Event::PointerMoved(over)]);
    let hovered = bare_primaries(&h, vec![Event::PointerMoved(over)]);
    assert_eq!(
        button_under(&hovered, "Hold the line").map(|(_, fill)| fill),
        Some(PRIMARY_HOVER)
    );

    // A real screen: the Results continuation is the registered amber
    // primary, painted once, at both target sizes.
    for target in Target::BOTH {
        let mut fixture = IsolatedFrontend::new();
        fixture.deploy();
        let h = Harness::at(target);
        let f = &mut fixture.frontend;
        f.game.apply(&Command::Withdraw).unwrap();
        f.screen = Screen::Results;
        h.frame(f, View::Full, vec![]);
        let output = h.frame(f, View::Full, vec![]);
        let primaries = h.screen_primaries();
        assert_eq!(primaries.len(), 1, "{target:?}: {primaries:?}");
        let label = primaries[0].as_str();
        assert_eq!(text_count(&output, label), 1, "{target:?}: {label}");
        assert_eq!(
            button_under(&output, label).map(|(_, fill)| fill),
            Some(AMBER),
            "{target:?}: {label} is filled amber"
        );
        assert_eq!(painted_color(&output, label), Some(INK), "{target:?}");
    }
}

/// What a matrix row expects of its screen primaries.
#[derive(Clone, Copy, Debug)]
enum Expect {
    /// No screen primary.
    Nothing,
    /// Exactly one, whose label the screen computes.
    One,
    /// Exactly this one.
    Label(&'static str),
}

/// Starts the journey in Germany (no attempt).
fn between_runs(fixture: &mut IsolatedFrontend) {
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().journey_active = true;
}

/// A matrix row: its name, how it prepares the fixture, the screen it lands
/// on and what that screen's primaries must be.
type MatrixRow = (&'static str, fn(&mut IsolatedFrontend), Screen, Expect);

const MATRIX: &[MatrixRow] = &[
    (
        "menu",
        |x| x.frontend.screen = Screen::Menu,
        Screen::Menu,
        Expect::Label("Resume journey"),
    ),
    (
        "survivor-selection",
        |x| x.frontend.screen = Screen::SurvivorSelection,
        Screen::SurvivorSelection,
        Expect::Label("Continue as Rook"),
    ),
    (
        "camp-world-before-start",
        |_| {},
        Screen::Camp,
        Expect::Label("Start journey"),
    ),
    (
        "camp-home",
        |x| {
            between_runs(x);
            x.frontend.nav.tab = Tab::Home;
        },
        Screen::Camp,
        Expect::Label("Go to briefing"),
    ),
    (
        "camp-briefing",
        |x| {
            between_runs(x);
            x.frontend.nav.tab = Tab::Briefing;
        },
        Screen::Camp,
        Expect::Label("Deploy personally"),
    ),
    (
        "camp-armory",
        |x| {
            between_runs(x);
            x.frontend.nav.tab = Tab::Armory;
        },
        Screen::Camp,
        Expect::Label("Go to briefing"),
    ),
    (
        "camp-operations",
        |x| {
            between_runs(x);
            x.frontend.nav.tab = Tab::Operations;
        },
        Screen::Camp,
        // Closing: the Operations drill-down has the Prepare footer primary
        // between defenses too (design-ux-flow §1, JOURNEYS loop map).
        Expect::Label("Go to briefing"),
    ),
    (
        "camp-armory-live",
        |x| {
            x.deploy();
            x.frontend.game.set_paused(true);
            x.frontend.screen = Screen::Camp;
            x.frontend.nav.tab = Tab::Armory;
        },
        Screen::Camp,
        Expect::Label("Return to active defense"),
    ),
    (
        "battle-ready",
        |x| {
            x.deploy();
            x.frontend.game.set_paused(true);
        },
        Screen::Battle,
        Expect::Label("Begin defense"),
    ),
    (
        "battle-paused",
        |x| {
            x.deploy();
            x.frontend.game.tick(Input::default());
            x.frontend.game.set_paused(true);
        },
        Screen::Battle,
        Expect::Label("Resume defense"),
    ),
    (
        "battle-running",
        |x| x.deploy(),
        Screen::Battle,
        Expect::Nothing,
    ),
    (
        "results",
        |x| {
            x.deploy();
            x.frontend.game.apply(&Command::Withdraw).unwrap();
            x.frontend.screen = Screen::Results;
        },
        Screen::Results,
        Expect::One,
    ),
    // W3-03 rewrite: the screen opened over a live defense is Upgrades
    // (Results with no settled report hands over to Camp home).
    (
        "upgrades-live",
        |x| {
            x.deploy();
            x.frontend.game.set_paused(true);
            x.frontend.screen = Screen::Upgrades;
        },
        Screen::Upgrades,
        Expect::Label("Return to active defense"),
    ),
    (
        "upgrades",
        |x| {
            x.deploy();
            x.frontend.game.apply(&Command::Withdraw).unwrap();
            x.frontend.screen = Screen::Upgrades;
        },
        Screen::Upgrades,
        Expect::Label("Back to battle"),
    ),
    (
        "upgrades-no-report",
        |x| {
            between_runs(x);
            x.frontend.screen = Screen::Upgrades;
        },
        Screen::Upgrades,
        Expect::Label("Back to camp"),
    ),
    (
        "history",
        |x| {
            between_runs(x);
            x.frontend.screen = Screen::History;
        },
        Screen::History,
        Expect::Nothing,
    ),
    (
        "settings",
        |x| {
            between_runs(x);
            x.frontend.screen = Screen::Settings;
        },
        Screen::Settings,
        Expect::One,
    ),
];

#[test]
fn at_most_one_screen_primary_matrix() {
    for target in Target::BOTH {
        let h = Harness::at(target);
        for &(name, prepare, screen, expect) in MATRIX {
            let mut fixture = IsolatedFrontend::new();
            prepare(&mut fixture);
            let f = &mut fixture.frontend;
            for _ in 0..3 {
                h.frame(f, View::Full, vec![]);
            }
            assert!(f.screen == screen, "{name} {target:?}: on {:?}", f.screen);
            let primaries = h.screen_primaries();
            assert!(
                primaries.len() <= 1,
                "{name} {target:?}: more than one screen primary {primaries:?}"
            );
            match expect {
                Expect::Nothing => {
                    assert!(primaries.is_empty(), "{name} {target:?}: {primaries:?}")
                }
                Expect::One => assert_eq!(primaries.len(), 1, "{name} {target:?}"),
                Expect::Label(label) => assert_eq!(primaries, [label], "{name} {target:?}"),
            }
        }
    }
}

#[test]
fn registry_empty_on_running_battle_frame() {
    for h in [Harness::large(), Harness::small()] {
        let target = h.target;
        let mut fixture = IsolatedFrontend::new();
        fixture.deploy();
        let f = &mut fixture.frontend;
        f.game.set_paused(true);
        h.frame(f, View::Full, vec![]);
        assert_eq!(h.screen_primaries(), ["Begin defense"], "{target:?}");
        // The next frame runs: its registry holds nothing, not the paused
        // frame's stale primary.
        f.game.set_paused(false);
        let output = h.frame(f, View::Full, vec![]);
        assert_eq!(text_count(&output, "Pause"), 1, "{target:?}");
        assert_eq!(h.primaries(), [], "{target:?}");
    }
}
