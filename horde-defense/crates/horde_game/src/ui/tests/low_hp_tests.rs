//! W3-15 (UX-12, G2.7): the low-HP edge warning. The defender is "low" from
//! 30% of maximum HP down and stays low until above 35% (design-ux-flow §2
//! row 21 and §4 "Low HP"): an 8 pt RED band inside all four canvas edges and
//! "· LOW" on the HP text. Presentation only.
use super::*;
use crate::art::RED;

/// The input-clock second the runs below begin at (a whole second, so the
/// 1 Hz pulse is at its mean there).
const T0: f64 = 100.0;
/// The band's width, in points.
const EDGE: f32 = 8.0;

/// The four edge bands of `canvas`: top, bottom, left, right. The side bands
/// run between the top and bottom ones, so no corner is painted twice.
fn bands(canvas: Rect) -> [Rect; 4] {
    let (min, max) = (canvas.min, canvas.max);
    [
        Rect::from_min_max(min, pos2(max.x, min.y + EDGE)),
        Rect::from_min_max(pos2(min.x, max.y - EDGE), max),
        Rect::from_min_max(pos2(min.x, min.y + EDGE), pos2(min.x + EDGE, max.y - EDGE)),
        Rect::from_min_max(pos2(max.x - EDGE, min.y + EDGE), pos2(max.x, max.y - EDGE)),
    ]
}
fn same_rect(a: Rect, b: Rect) -> bool {
    (a.min - b.min).length() < 1e-3 && (a.max - b.max).length() < 1e-3
}
/// Every filled rect of the frame, in paint order.
fn filled_rects(output: &FullOutput) -> Vec<(Rect, Color32)> {
    fn collect(shape: &egui::epaint::Shape, out: &mut Vec<(Rect, Color32)>) {
        match shape {
            egui::epaint::Shape::Rect(rect) if rect.fill.a() > 0 => {
                out.push((rect.rect, rect.fill));
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut out);
    }
    out
}
/// The fill of each of the four edge bands of `canvas`, `None` where the
/// frame painted none.
fn edge_fills(output: &FullOutput, canvas: Rect) -> [Option<Color32>; 4] {
    let rects = filled_rects(output);
    bands(canvas).map(|band| {
        rects
            .iter()
            .find(|(rect, _)| same_rect(*rect, band))
            .map(|(_, fill)| *fill)
    })
}
/// The one fill all four bands share, `None` when no band is painted. A
/// frame with some bands, or bands of different fills, fails here.
fn edge(output: &FullOutput, canvas: Rect) -> Option<Color32> {
    let fills = edge_fills(output, canvas);
    assert!(
        fills.iter().all(|fill| *fill == fills[0]),
        "the four bands differ: {fills:?}"
    );
    fills[0]
}
fn painted(output: &FullOutput, label: &str) -> bool {
    output
        .shapes
        .iter()
        .any(|c| shape_has_text(&c.shape, label))
}
/// Sets the defender's HP to `percent` of the maximum, which the fixtures
/// below make 100.
fn set_hp(f: &mut FrontendParts, percent: i32) {
    let a = f.game.attempt_mut().unwrap();
    a.max_hp = 100;
    a.hp = percent;
}
/// The HP text the HUD reads at `percent`.
fn hp_text(percent: i32, low: bool) -> String {
    format!(
        "Rook · HP {percent} / 100{}",
        if low { " · LOW" } else { "" }
    )
}
/// A running personal defense in reduced effects (a static band).
fn personal() -> IsolatedFrontend {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    fixture.frontend.settings.reduced_effects = true;
    fixture
}
/// A running observed (main-run Npc) defense in reduced effects.
fn observed() -> IsolatedFrontend {
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().attempts = 1;
    f.game.deploy_country_stage(0, Mode::Npc).unwrap();
    f.game.set_paused(false);
    f.screen = Screen::Battle;
    f.settings.reduced_effects = true;
    fixture
}

/// UX-12: nothing warned of low HP. From 30% down the canvas gets an 8 pt RED
/// band inside all four edges and the HP text says LOW; at 31% neither.
#[test]
fn low_hp_edge_below_30_percent() {
    for target in Target::BOTH {
        let mut fixture = personal();
        let h = Harness::at(target);
        let f = &mut fixture.frontend;
        set_hp(f, 31);
        let output = h.frame(f, View::Battle, vec![]);
        // The canvas is the target in points (pixels / scale), as the
        // context rounds it.
        let canvas = h.ctx.content_rect();
        assert!(
            canvas.min == Pos2::ZERO && (canvas.size() - target.points()).length() < 0.5,
            "{target:?}: {canvas:?}"
        );
        assert_eq!(edge(&output, canvas), None, "{target:?}: 31% is not low");
        assert!(painted(&output, &hp_text(31, false)), "{target:?}: 31%");
        assert!(!painted(&output, "LOW"), "{target:?}: 31%");

        set_hp(f, 30);
        let output = h.frame(f, View::Battle, vec![]);
        // Reduced effects: a static 0.40 of RED.
        assert_eq!(
            edge(&output, canvas),
            Some(RED.gamma_multiply(0.40)),
            "{target:?}: 30% is low"
        );
        assert!(painted(&output, &hp_text(30, true)), "{target:?}: 30%");
        // The band lies outside the two corner HUD frames and the arena.
        let a = f.game.attempt().unwrap();
        let layout = battle::BattleLayout::new(canvas, a.lane_count, a.width_yards, true);
        for band in bands(canvas) {
            for (name, rect) in [
                ("hud_left", layout.hud_left),
                ("hud_right", layout.hud_right),
                ("arena", layout.arena),
            ] {
                assert!(!band.intersects(rect), "{target:?}: {band:?} over {name}");
            }
        }

        set_hp(f, 5);
        let output = h.frame(f, View::Battle, vec![]);
        assert!(edge(&output, canvas).is_some(), "{target:?}: 5%");
        assert!(painted(&output, &hp_text(5, true)), "{target:?}: 5%");
    }

    // The main-run observed defender gets the same warning.
    let mut fixture = observed();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let canvas = Rect::from_min_size(Pos2::ZERO, Target::LARGE.pixels);
    set_hp(f, 80);
    let output = h.frame(f, View::Battle, vec![]);
    assert_eq!(edge(&output, canvas), None, "observed: 80%");
    set_hp(f, 25);
    let output = h.frame(f, View::Battle, vec![]);
    assert!(edge(&output, canvas).is_some(), "observed: 25%");
    assert!(painted(&output, "HP 25 / 100 · LOW"), "observed: 25%");
}

/// The warning does not flicker around one threshold: once low, the defender
/// stays low up to 35% and leaves only above it; coming back down, the
/// warning returns only at 30%.
#[test]
fn low_hp_hysteresis_exits_above_35() {
    let mut fixture = personal();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let canvas = Rect::from_min_size(Pos2::ZERO, Target::LARGE.pixels);
    // (percent, low) in order: each step starts from the one before.
    for (percent, low) in [
        (100, false),
        (35, false),
        (31, false),
        (30, true),
        (33, true),
        (35, true),
        (36, false),
        (33, false),
        (31, false),
        (30, true),
        (31, true),
        (90, false),
    ] {
        set_hp(f, percent);
        let output = h.frame(f, View::Battle, vec![]);
        assert_eq!(
            edge(&output, canvas).is_some(),
            low,
            "{percent}%: the edge band"
        );
        assert!(
            painted(&output, &hp_text(percent, low)),
            "{percent}%: the HP text"
        );
        assert_eq!(painted(&output, "LOW"), low, "{percent}%");
        assert_eq!(f.battle.low_hp, low, "{percent}%: BattleUi.low_hp");
    }

    // A frame drawn twice changes nothing: the state is the hysteresis's own.
    set_hp(f, 30);
    h.frame(f, View::Battle, vec![]);
    set_hp(f, 34);
    for _ in 0..3 {
        let output = h.frame(f, View::Battle, vec![]);
        assert!(edge(&output, canvas).is_some(), "34% after 30%");
    }
    // The memory belongs to the attempt: another attempt at 34% is not low.
    f.game.attempt_mut().unwrap().id += 1;
    let output = h.frame(f, View::Battle, vec![]);
    assert_eq!(edge(&output, canvas), None, "another attempt at 34%");
    assert!(!f.battle.low_hp);
}

/// ISO-10: reduced effects holds the band at a static 0.40. Normal effects
/// pulse it between 0.25 and 0.50 once a second while the battle runs; a
/// paused battle holds the static 0.40 in both modes.
#[test]
fn low_hp_edge_static_in_reduced_effects() {
    let canvas = Rect::from_min_size(Pos2::ZERO, Target::LARGE.pixels);
    // Quarter seconds of one pulse: mean, peak, mean, trough.
    let times = [T0, T0 + 0.25, T0 + 0.5, T0 + 0.75];
    for reduced in [true, false] {
        let mut fixture = personal();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.settings.reduced_effects = reduced;
        set_hp(f, 20);
        let fills = times.map(|time| {
            let output = h.frame_at(f, View::Battle, vec![], time);
            edge(&output, canvas).unwrap_or_else(|| panic!("reduced {reduced}: no band at {time}"))
        });
        if reduced {
            assert_eq!(fills, [RED.gamma_multiply(0.40); 4], "reduced: static");
        } else {
            assert_eq!(
                fills,
                [0.375, 0.50, 0.375, 0.25].map(|alpha| RED.gamma_multiply(alpha)),
                "normal: a 1 Hz pulse between 0.25 and 0.50"
            );
        }
        // Paused: static in both modes, at any time.
        f.game.set_paused(true);
        for time in [T0 + 1.25, T0 + 1.75] {
            let output = h.frame_at(f, View::Battle, vec![], time);
            assert_eq!(
                edge(&output, canvas),
                Some(RED.gamma_multiply(0.40)),
                "reduced {reduced}: paused at {time}"
            );
        }
    }
}

/// Pin (design-ux-flow §8): drawing never changes the game. 100 paused
/// frames of the whole App frame, with the low-HP warning, a boss warning
/// and every wall-clock overlay running, leave `state_digest` where it was.
#[test]
fn ui_frames_do_not_change_state_digest() {
    for (name, mut fixture) in [("personal", personal()), ("observed", observed())] {
        for reduced in [false, true] {
            let h = Harness::new();
            let f = &mut fixture.frontend;
            f.settings.reduced_effects = reduced;
            set_hp(f, 20);
            f.game.set_paused(true);
            let before = f.game.state_digest();
            let canvas = Rect::from_min_size(Pos2::ZERO, Target::LARGE.pixels);
            for frame in 0..100 {
                let output = h.frame_at(f, View::Full, vec![], T0 + f64::from(frame) / 60.0);
                assert_eq!(f.screen, Screen::Battle, "{name}: frame {frame}");
                assert!(
                    edge(&output, canvas).is_some(),
                    "{name} reduced {reduced}: frame {frame} has no low-HP band"
                );
                assert_eq!(
                    f.game.state_digest(),
                    before,
                    "{name} reduced {reduced}: frame {frame} changed the game"
                );
            }
        }
    }
}

/// Every filled rect painted in the band's colour: RED at one of the
/// strengths the band takes in these frames (the static 0.40, or the pulse at
/// a quarter second: its mean, peak and trough).
fn band_coloured(output: &FullOutput) -> Vec<Rect> {
    let fills = [0.40, 0.375, 0.50, 0.25].map(|alpha| RED.gamma_multiply(alpha));
    filled_rects(output)
        .into_iter()
        .filter(|(_, fill)| fills.contains(fill))
        .map(|(rect, _)| rect)
        .collect()
}

/// The rule's other half (battle/low_hp.rs): the warning belongs to the main
/// battle, never to the Operations worker observer window. A worker at 10% HP,
/// observed from the Operations tab, gets no band inside the canvas or its
/// observer field, nothing of the band's colour anywhere, no "LOW", and
/// `BattleUi::low_hp` stays clear, in both effects modes, running or paused.
#[test]
fn worker_observer_never_paints_the_low_hp_edge() {
    for reduced in [true, false] {
        for paused in [false, true] {
            let mut fixture = IsolatedFrontend::new();
            let h = Harness::new();
            let f = &mut fixture.frontend;
            f.game = horde_core::sim::stress_authored_fixture().unwrap();
            f.game.set_paused(paused);
            f.settings.reduced_effects = reduced;
            f.screen = Screen::Camp;
            f.nav.tab = Tab::Operations;
            let worker = f
                .game
                .battles_mut()
                .iter_mut()
                .find(|b| !b.settled && b.game().attempt().is_some())
                .expect("the fixture keeps a live worker");
            let id = worker.id;
            let a = worker.game_mut().attempt_mut().unwrap();
            (a.max_hp, a.hp) = (100, 10);
            f.campaign.observed = Some(id);
            let case = format!("reduced {reduced}, paused {paused}");
            // Each frame is a quarter second on, through one pulse. The window
            // lays itself out on its first pass and paints from the second.
            for frame in 0..5 {
                let output = h.frame_at(f, View::Full, vec![], T0 + f64::from(frame) * 0.25);
                assert_eq!(
                    band_coloured(&output),
                    Vec::<Rect>::new(),
                    "{case}: frame {frame}: the band's colour"
                );
                assert!(!painted(&output, "LOW"), "{case}: frame {frame}");
                assert!(!f.battle.low_hp, "{case}: frame {frame}: BattleUi.low_hp");
                if frame == 0 {
                    continue;
                }
                assert!(
                    painted(&output, "Save country plan"),
                    "{case}: frame {frame}: the observer window is open"
                );
                assert!(
                    painted(&output, "HP 10/100"),
                    "{case}: frame {frame}: the observer reads the worker at 10%"
                );
                let canvas = h.ctx.content_rect();
                let field = f
                    .battle
                    .observer_arena_rect
                    .unwrap_or_else(|| panic!("{case}: frame {frame}: no observer field"));
                for (name, rect) in [("canvas", canvas), ("observer field", field)] {
                    assert_eq!(
                        edge_fills(&output, rect),
                        [None; 4],
                        "{case}: frame {frame}: a low-HP band inside the {name}"
                    );
                }
            }
        }
    }
}
