//! W3-25 (UX-17): expedition outcome toasts (design-ux-flow §4 "Expedition
//! notices"). Each unacknowledged outcome becomes exactly one toast and a
//! Home "Recent reports" line, `AcknowledgeOutcomes` leaves in the same
//! frame, the folded count becomes one summary toast, and nothing is
//! delivered while a live defense runs unpaused.
use super::defender_tests::camp_with_recruits;
use super::*;

/// Camp home with `count` recruits whose expeditions to Poland were recalled:
/// `count` settled, unacknowledged outcomes. Returns each outcome's line.
/// Expeditions need two liberated countries, so at least two recruits join.
fn recalled_expeditions(f: &mut FrontendParts, count: usize) -> Vec<String> {
    let origins = ["FRA", "POL", "NLD"];
    let recruits = camp_with_recruits(f, &origins[..count.max(2)]);
    for recruit in recruits.into_iter().take(count) {
        let battle = f.game.deploy_survivor(recruit, "POL", 0).unwrap();
        f.game.recall_survivor(battle).unwrap();
    }
    f.game.set_paused(true);
    f.screen = Screen::Camp;
    f.nav.tab = Tab::Home;
    let outcomes = f.game.unacknowledged_outcomes();
    assert_eq!(outcomes.notices.len(), count);
    outcomes
        .notices
        .iter()
        .map(|notice| {
            assert_eq!(notice.summary.coins, 0, "a recall at tick 0 earns nothing");
            format!("{} retreated from Poland", notice.recruit_name)
        })
        .collect()
}

/// One whole App frame at input-clock second `time`, without the runner's
/// Apply pass: what the widget pass queued is still in the runtime.
fn draw_only(h: &Harness, f: &mut FrontendParts, time: Option<f64>) -> FullOutput {
    f.runtime.begin_frame(h.ctx.cumulative_frame_nr() as u32);
    f.settings.scale = h.target.scale;
    let mut output = h.ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, h.target.pixels)),
            time,
            ..RawInput::default()
        },
        |ui| draw_screen(ui.ctx(), &mut f.ctx(), 0.0),
    );
    output.textures_delta.clear();
    output
}

/// The queued toasts' texts, newest first.
fn toast_texts(f: &FrontendParts) -> Vec<String> {
    f.status
        .toasts
        .newest(16)
        .map(|toast| toast.text.clone())
        .collect()
}

#[test]
fn one_toast_per_unacknowledged_outcome_and_ack_dispatched_same_frame() {
    for target in Target::BOTH {
        let mut fixture = IsolatedFrontend::new();
        let f = &mut fixture.frontend;
        let lines = recalled_expeditions(f, 2);
        assert_ne!(lines[0], lines[1], "two recruits, two lines");
        let h = Harness::at(target);
        let base = h.ctx.input(|input| input.time) + 1.0;

        // The first frame toasts both outcomes and queues the acknowledgement.
        let output = draw_only(&h, f, Some(base));
        for line in &lines {
            assert_eq!(
                toast_texts(f).iter().filter(|text| *text == line).count(),
                1,
                "{target:?}: one toast reads {line:?}: {:?}",
                toast_texts(f)
            );
            assert!(
                text_count(&output, line) >= 1,
                "{target:?}: {line:?} is painted in the frame that read it"
            );
        }
        assert_eq!(
            f.runtime.pending_commands(),
            1,
            "{target:?}: AcknowledgeOutcomes is queued in the same frame"
        );
        // A repeated pass before the runner applies it toasts nothing twice.
        draw_only(&h, f, Some(base + 0.01));
        assert_eq!(toast_texts(f).len(), 2, "{target:?}: {:?}", toast_texts(f));
        assert_eq!(f.runtime.pending_commands(), 1);

        // The runner applies it: core holds no unacknowledged outcome.
        f.flush_for_test();
        assert_eq!(f.game.unacknowledged_outcomes().notices, Vec::new());
        assert!(!f.status.message_error, "{}", f.status.message);

        // Later frames add no second toast, and a reload announces nothing.
        for step in 1..=3 {
            h.frame_at(f, View::Full, vec![], base + 0.1 * f64::from(step));
        }
        assert_eq!(toast_texts(f).len(), 2, "{target:?}: {:?}", toast_texts(f));
        assert_eq!(f.runtime.pending_commands(), 0);

        // Once the toasts are gone the outcome stays in Home "Recent reports",
        // newest first. The small window scrolls to that section, so only
        // the large one paints it without scrolling.
        let later = h.frame_at(f, View::Full, vec![], base + 30.0);
        assert!(toast_texts(f).is_empty(), "{:?}", toast_texts(f));
        assert_eq!(
            expedition_notices::recent_lines(&f.game, 2),
            [lines[1].clone(), lines[0].clone()]
        );
        if target == Target::LARGE {
            for line in &lines {
                assert_eq!(
                    text_count(&later, line),
                    1,
                    "{target:?}: {line:?} is a Recent reports line"
                );
            }
        }
    }
}

#[test]
fn folded_overflow_shows_single_summary_toast() {
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    let lines = recalled_expeditions(f, 1);
    f.game.strategy_mut().folded_outcomes = 3;
    let h = Harness::new();
    let base = h.ctx.input(|input| input.time) + 1.0;
    let summary = "3 more expedition outcomes · Operations";

    let output = draw_only(&h, f, Some(base));
    assert_eq!(
        toast_texts(f),
        [summary.to_owned(), lines[0].clone()],
        "the outcome's own toast, then one summary toast"
    );
    assert_eq!(text_count(&output, summary), 1);
    assert_eq!(f.runtime.pending_commands(), 1, "one acknowledgement");
    f.flush_for_test();
    assert_eq!(f.game.strategy().folded_outcomes, 0);
    assert_eq!(f.game.unacknowledged_outcomes().notices, Vec::new());
    for step in 1..=3 {
        h.frame_at(f, View::Full, vec![], base + 0.1 * f64::from(step));
    }
    assert_eq!(toast_texts(f).len(), 2, "{:?}", toast_texts(f));

    // One folded outcome reads in the singular.
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    recalled_expeditions(f, 1);
    f.game
        .apply(&Command::AcknowledgeOutcomes {
            battles: f
                .game
                .unacknowledged_outcomes()
                .notices
                .iter()
                .map(|notice| notice.battle)
                .collect(),
            folded: 0,
        })
        .unwrap();
    f.game.strategy_mut().folded_outcomes = 1;
    let h = Harness::new();
    h.frame(f, View::Full, vec![]);
    assert_eq!(
        toast_texts(f),
        ["1 more expedition outcome · Operations".to_owned()]
    );
    assert_eq!(f.game.strategy().folded_outcomes, 0);
}

#[test]
fn expedition_toasts_deferred_during_live_unpaused_battle() {
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    let lines = recalled_expeditions(f, 2);
    f.game.strategy_mut().folded_outcomes = 2;
    f.game.deploy_country_stage(0, Mode::Personal).unwrap();
    f.game.set_paused(false);
    f.screen = Screen::Battle;
    let h = Harness::new();

    // A live, unpaused defense: nothing is toasted and nothing acknowledged.
    for _ in 0..3 {
        draw_only(&h, f, None);
    }
    assert!(toast_texts(f).is_empty(), "{:?}", toast_texts(f));
    assert_eq!(f.runtime.pending_commands(), 0);
    assert_eq!(f.game.unacknowledged_outcomes().notices.len(), 2);
    assert_eq!(f.game.strategy().folded_outcomes, 2);

    // Paused, the battle's one toast slot takes the oldest outcome only.
    f.game.set_paused(true);
    h.frame(f, View::Full, vec![]);
    assert_eq!(toast_texts(f), [lines[0].clone()]);
    assert_eq!(f.game.unacknowledged_outcomes().notices.len(), 1);
    h.frame(f, View::Full, vec![]);
    assert_eq!(toast_texts(f).len(), 1, "the slot is still taken");
    assert_eq!(f.game.strategy().folded_outcomes, 2);

    // Back at camp the rest arrive: the second outcome and the folded count.
    f.screen = Screen::Camp;
    f.nav.tab = Tab::Home;
    h.frame(f, View::Full, vec![]);
    assert_eq!(
        toast_texts(f),
        [
            "2 more expedition outcomes · Operations".to_owned(),
            lines[1].clone(),
            lines[0].clone()
        ]
    );
    assert_eq!(f.game.unacknowledged_outcomes().notices, Vec::new());
    assert_eq!(f.game.strategy().folded_outcomes, 0);
}
