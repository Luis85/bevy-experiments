//! Egui harness tests: the battle views are built once per egui pass, as the
//! App draws a frame (`draw_screen`), never once per widget that reads them.
use super::*;
use crate::battle_view::builds::{self, ViewBuilds};

/// Runs one App frame of `f` (as many egui passes as egui asks for) with
/// `events`, the first pass optionally requesting a discard. Returns the
/// passes run and the view builds they made.
fn app_frame(
    h: &Harness,
    f: &mut FrontendParts,
    events: Vec<Event>,
    discard: bool,
) -> (u32, ViewBuilds) {
    f.runtime.begin_frame(h.ctx.cumulative_frame_nr() as u32);
    builds::take();
    let mut passes = 0;
    let mut output = h.ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events,
            ..RawInput::default()
        },
        |ui| {
            passes += 1;
            if discard && passes == 1 {
                ui.ctx().request_discard("test: a discarded first pass");
            }
            draw_screen(ui.ctx(), &mut f.ctx(), 0.0);
        },
    );
    output.textures_delta.clear();
    let built = builds::take();
    f.flush_for_test();
    (passes, built)
}

const ONCE: ViewBuilds = ViewBuilds {
    screen: 1,
    field: 1,
    hud: 1,
};

#[test]
fn battle_views_build_once_per_egui_pass() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    // A settling frame, then the costliest one: an active, unpaused battle
    // whose Pause press reads the screen in the controls as well.
    app_frame(&h, f, vec![], false);
    assert!(!f.game.attempt().unwrap().paused);
    let (passes, built) = app_frame(&h, f, vec![key_event(Key::Escape, true)], false);
    assert!(
        f.game.attempt().unwrap().paused,
        "the Pause press was handled"
    );
    assert_eq!(passes, 1);
    assert_eq!(built, ONCE);
}

#[test]
fn npc_observation_builds_battle_screen_once_per_pass() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().attempts = 1;
    f.game.deploy_country_stage(0, Mode::Npc).unwrap();
    f.game.set_paused(false);
    f.screen = Screen::Battle;
    app_frame(&h, f, vec![], false);
    let (passes, built) = app_frame(&h, f, vec![], false);
    assert_eq!(passes, 1);
    assert_eq!(built, ONCE);
}

#[test]
fn discarded_pass_builds_once_per_pass() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    app_frame(&h, f, vec![], false);
    let (passes, built) = app_frame(&h, f, vec![], true);
    assert_eq!(passes, 2, "egui reran the discarded pass");
    assert_eq!(
        built,
        ViewBuilds {
            screen: passes,
            field: passes,
            hud: passes,
        }
    );
}

#[test]
fn observer_builds_one_field_view_per_visible_worker() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game = horde_core::sim::stress_authored_fixture().unwrap();
    f.screen = Screen::Camp;
    f.nav.tab = Tab::Operations;
    let workers: Vec<_> = f
        .game
        .battles()
        .iter()
        .filter(|b| !b.settled && b.game().attempt().is_some())
        .map(|b| b.id)
        .collect();
    assert_eq!(workers.len(), 3);
    for id in workers {
        f.campaign.observed = Some(id);
        let (passes, built) = app_frame(&h, f, vec![], false);
        assert_eq!(passes, 1);
        // The observer paints its worker's field only; the main battle's
        // screen and HUD are not drawn on the operations tab.
        assert_eq!(
            built,
            ViewBuilds {
                screen: 0,
                field: 1,
                hud: 0,
            },
            "observing worker #{id}"
        );
    }
}

/// The harness size convention (closing): a frame at a target lays out
/// `pixels / scale` points, as the App's window does. `RawInput::screen_rect`
/// is in points and egui's zoom does not divide it, so the small target
/// (1050x700 at 1.35) is a 777.8x518.5 pt canvas, for a partial view and for
/// the whole App frame, through `frame`, `frame_at` and `click`'s frames.
#[test]
fn harness_frames_lay_out_the_target_in_points() {
    for target in Target::BOTH {
        let points = target.points();
        assert!(
            (points - target.pixels / target.scale).length() < 1e-3,
            "{target:?}"
        );
        for view in [View::Camp, View::Full] {
            let mut fixture = IsolatedFrontend::new();
            let h = Harness::at(target);
            let f = &mut fixture.frontend;
            h.frame(f, view, vec![]);
            let canvas = h.ctx.content_rect();
            assert!(
                (canvas.size() - points).length() < 0.5,
                "{target:?}: frame lays out {canvas:?}, not {points:?}"
            );
            h.frame_at(f, view, vec![], 10.0);
            let canvas = h.ctx.content_rect();
            assert!(
                (canvas.size() - points).length() < 0.5,
                "{target:?}: frame_at lays out {canvas:?}, not {points:?}"
            );
        }
    }
}
