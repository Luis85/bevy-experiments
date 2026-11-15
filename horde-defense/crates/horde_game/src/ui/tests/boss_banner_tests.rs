//! W3-14 (UX-11, G2.7): the boss banner runs on the wall clock, 2.0 s for
//! "BOSS APPROACHING" and 1.5 s for "<BOSS> HAS ARRIVED", whatever the
//! observation speed, at the BattleLayout position (design-ux-flow §4 "Boss
//! banner", ruling C-11).
use super::*;
use crate::art::AMBER;

/// The input-clock second the runs below begin at.
const T0: f64 = 100.0;
const APPROACHING: &str = "BOSS APPROACHING";
/// One frame of a 60 Hz display, in seconds.
const FRAME: f64 = 1.0 / 60.0;

/// A frame counter on the input clock: every frame is 1/60 s after the last.
struct Clock {
    frames: u32,
}
impl Clock {
    fn new() -> Self {
        Self { frames: 0 }
    }
    /// One frame of `view` after `speed` whole ticks, as a display frame at
    /// that observation speed.
    fn frame(&mut self, h: &Harness, f: &mut FrontendParts, view: View, speed: u32) -> FullOutput {
        for _ in 0..speed {
            if !f.game.global_paused() {
                f.game.tick(Input::default());
            }
        }
        self.frames += 1;
        h.frame_at(f, view, vec![], T0 + FRAME * f64::from(self.frames))
    }
    /// `frames` frames; the last one's output.
    fn run(
        &mut self,
        h: &Harness,
        f: &mut FrontendParts,
        view: View,
        speed: u32,
        frames: u32,
    ) -> FullOutput {
        assert!(frames > 0);
        let mut output = None;
        for _ in 0..frames {
            output = Some(self.frame(h, f, view, speed));
        }
        output.unwrap()
    }
}
fn painted(output: &FullOutput, label: &str) -> bool {
    output
        .shapes
        .iter()
        .any(|c| shape_has_text(&c.shape, label))
}
fn color_of(output: &FullOutput, label: &str) -> Option<Color32> {
    output
        .shapes
        .iter()
        .find_map(|c| text_color(&c.shape, label))
}
/// The painted bounds of the first text run containing `part`.
fn labeled_suffix(shape: &egui::epaint::Shape, part: &str) -> Option<Rect> {
    match shape {
        egui::epaint::Shape::Text(t) if t.galley.text().contains(part) => {
            Some(t.galley.rect.translate(t.pos.to_vec2()))
        }
        egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| labeled_suffix(s, part)),
        _ => None,
    }
}
/// Equal to a thousandth of a point (f32 sums differ in the last bit).
fn same_rect(a: Rect, b: Rect) -> bool {
    (a.min - b.min).length() < 1e-3 && (a.max - b.max).length() < 1e-3
}
/// The boss's arrival tick and its warning's lead, in ticks.
fn boss_ticks(f: &FrontendParts) -> (u64, u64) {
    let a = f.game.attempt().unwrap();
    let warning = content().levels[a.level].boss_timing.warning;
    (
        (a.boss_time() * 60.0).ceil() as u64,
        (warning * 60.0).ceil() as u64,
    )
}
/// The text the arrival banner reads for the live attempt's boss.
fn arrived_text(f: &FrontendParts) -> String {
    let a = f.game.attempt().unwrap();
    format!(
        "{} HAS ARRIVED",
        content().levels[a.level].boss_name.to_uppercase()
    )
}
/// A personal defense one tick before its boss warning.
fn personal_before_warning() -> IsolatedFrontend {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let (boss, warning) = boss_ticks(&fixture.frontend);
    fixture.frontend.game.attempt_mut().unwrap().tick = boss - warning - 1;
    fixture
}
/// An observed (Npc) defense one tick before its boss warning.
fn observed_before_warning() -> IsolatedFrontend {
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().attempts = 1;
    f.game.deploy_country_stage(0, Mode::Npc).unwrap();
    f.game.set_paused(false);
    f.screen = Screen::Battle;
    let (boss, warning) = boss_ticks(f);
    f.game.attempt_mut().unwrap().tick = boss - warning - 1;
    fixture
}

/// UX-11: at 4x the old banner lasted 0.25 s of wall time. The banner counts
/// wall time, so it reads the same at every speed.
#[test]
fn boss_banner_visible_at_1_2s_and_gone_after_2s_wall_at_speed_4() {
    // 4x, observed: the warning leads the boss by 5 s of battle time, 1.25 s
    // of wall time.
    let mut fixture = observed_before_warning();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let (_, warning) = boss_ticks(f);
    assert_eq!(warning, 300, "the test's arithmetic assumes a 5 s warning");
    let arrived = arrived_text(f);
    let before = h.frame_at(f, View::Battle, vec![], T0);
    assert!(
        !painted(&before, APPROACHING),
        "no banner before the warning"
    );
    let mut clock = Clock::new();
    // The frame the warning is first seen in starts the banner at 0 s.
    clock.frame(&h, f, View::Battle, 4);
    // 0.25 s of wall time: the old banner's whole life at 4x.
    let output = clock.run(&h, f, View::Battle, 4, 15);
    assert!(painted(&output, APPROACHING), "0.25 s");
    // 1.2 s of wall time: 4.8 s of battle time, the boss is still out.
    let output = clock.run(&h, f, View::Battle, 4, 57);
    assert!(
        f.game.attempt().unwrap().enemies.iter().all(|e| !e.boss),
        "the boss is not due yet"
    );
    assert!(painted(&output, APPROACHING), "1.2 s of wall time at 4x");
    // The boss arrives at 1.25 s: the banner swaps to the arrival for 1.5 s.
    let output = clock.run(&h, f, View::Battle, 4, 54);
    assert!(f.game.attempt().unwrap().enemies.iter().any(|e| e.boss));
    assert!(!painted(&output, APPROACHING), "2.1 s: still approaching");
    assert!(painted(&output, &arrived), "2.1 s: no arrival banner");
    let output = clock.run(&h, f, View::Battle, 4, 48);
    assert!(
        !painted(&output, &arrived) && !painted(&output, APPROACHING),
        "2.9 s: a banner outstays its time"
    );

    // 1x, personal: the whole 2.0 s, then nothing until the boss.
    let mut fixture = personal_before_warning();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let mut clock = Clock::new();
    clock.frame(&h, f, View::Battle, 1);
    let output = clock.run(&h, f, View::Battle, 1, 72);
    assert!(painted(&output, APPROACHING), "1.2 s at 1x");
    let output = clock.run(&h, f, View::Battle, 1, 42);
    assert!(painted(&output, APPROACHING), "1.9 s at 1x");
    let output = clock.run(&h, f, View::Battle, 1, 12);
    assert!(!painted(&output, APPROACHING), "2.1 s at 1x");
    // The HUD countdown stays.
    assert!(painted(&output, "BOSS IN 00:0"), "the HUD countdown");
}

/// Attempt ids restart at 1 in a new journey, and the first defense warns on
/// the same tick: the new journey's warning still gets its banner, so the
/// wave alert never sounds without its visual twin.
#[test]
fn a_new_journey_announces_a_warning_on_the_same_attempt_and_tick() {
    let mut fixture = personal_before_warning();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let mut clock = Clock::new();
    clock.frame(&h, f, View::Battle, 1);
    let output = clock.run(&h, f, View::Battle, 1, 30);
    assert!(painted(&output, APPROACHING), "the old journey's warning");
    let announced = f.battle.boss_banner.expect("a banner").key;
    // 2.1 s: the banner is over, the boss is still 2.9 s out.
    let output = clock.run(&h, f, View::Battle, 1, 96);
    assert!(!painted(&output, APPROACHING), "the old banner is over");
    assert!(f.game.attempt().unwrap().enemies.iter().all(|e| !e.boss));
    // Leave before the boss arrives and start a new journey.
    f.game.set_paused(true);
    f.screen = Screen::Menu;
    f.act(|c| c.dispatch(AppCommand::NewJourney, ""));
    f.flush_for_test();
    assert!(!f.status.message_error, "{}", f.status.message);
    assert!(f.game.attempt().is_none(), "the new journey has no attempt");
    // Its first defense, one tick before the same warning.
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.deploy_country_stage(0, Mode::Personal).unwrap();
    f.game.set_paused(false);
    f.screen = Screen::Battle;
    let (boss, warning) = boss_ticks(f);
    f.game.attempt_mut().unwrap().tick = boss - warning - 1;
    clock.frame(&h, f, View::Battle, 1);
    let output = clock.run(&h, f, View::Battle, 1, 30);
    assert_eq!(
        f.battle.boss_banner.map(|banner| banner.key),
        Some(announced),
        "the same attempt id and warning tick"
    );
    assert!(painted(&output, APPROACHING), "the new journey's warning");
    f.runtime.finish_storage_for_test();
}

/// The arrival banner lasts 1.5 s of wall time, and only running time
/// counts: a paused battle holds the banner where it is.
#[test]
fn boss_arrived_banner_lasts_1_5s() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let (boss, _) = boss_ticks(f);
    // The director warns on this tick and spawns the boss on the next.
    f.game.attempt_mut().unwrap().tick = boss - 1;
    let arrived = arrived_text(f);
    let mut clock = Clock::new();
    clock.run(&h, f, View::Battle, 1, 2);
    assert!(f.game.attempt().unwrap().enemies.iter().any(|e| e.boss));
    // The arrival is first seen in the last frame: 0 s.
    let output = clock.run(&h, f, View::Battle, 1, 30);
    assert!(painted(&output, &arrived), "0.5 s");
    assert!(
        !painted(&output, APPROACHING),
        "the arrival replaces the warning"
    );
    // The old 0.45 s spawn tag is gone.
    assert_eq!(text_count(&output, "BOSS"), 0, "the old spawn tag");
    // Three seconds paused: the banner waits.
    f.game.set_paused(true);
    let output = clock.run(&h, f, View::Battle, 1, 180);
    assert!(
        painted(&output, &arrived),
        "a paused battle keeps the banner"
    );
    f.game.set_paused(false);
    let output = clock.run(&h, f, View::Battle, 1, 54);
    assert!(painted(&output, &arrived), "1.4 s of running time");
    let output = clock.run(&h, f, View::Battle, 1, 12);
    assert!(!painted(&output, &arrived), "1.6 s of running time");
}

/// CF-3-10: a warning first seen on a paused frame (a battle paused or
/// loaded inside the fade-in) is held at full strength in normal effects as
/// in reduced effects, so the boss cue's visual twin shows in both modes
/// while the battle waits, and keeps full strength once it resumes.
#[test]
fn a_banner_started_on_a_paused_frame_shows_in_both_modes() {
    for reduced in [false, true] {
        let mut fixture = personal_before_warning();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.settings.reduced_effects = reduced;
        // The warning tick runs; the battle pauses before a frame shows it.
        f.game.tick(Input::default());
        f.game.set_paused(true);
        let mut clock = Clock::new();
        for frames in [1, 120] {
            let output = clock.run(&h, f, View::Battle, 1, frames);
            assert_eq!(
                color_of(&output, APPROACHING),
                Some(AMBER),
                "reduced {reduced}: paused, {frames} frames in"
            );
            assert!(
                f.battle.boss_banner_rect.is_some(),
                "reduced {reduced}: paused, no banner rect"
            );
        }
        // Resumed: full strength until the fade-out.
        f.game.set_paused(false);
        let output = clock.run(&h, f, View::Battle, 1, 30);
        assert_eq!(
            color_of(&output, APPROACHING),
            Some(AMBER),
            "reduced {reduced}: 0.5 s after resuming"
        );
    }
}

/// Ruling C-11: the banner is centred on the arena, `min(380, 0.6 x arena
/// width)` by 34, 4 pt under the first-defense hint callout while that shows
/// and 6 pt under the arena top otherwise.
#[test]
fn boss_banner_never_overlaps_the_hint_callout() {
    // Both target sizes in points, as the battle screen's own layout tests
    // draw them: 1440x900, and 1050x700 at 1.35.
    for points in Target::BOTH.map(|target| target.pixels / target.scale) {
        let target = Target {
            pixels: points,
            scale: 1.0,
        };
        for hints in [true, false] {
            let mut fixture = personal_before_warning();
            let h = Harness::at(target);
            let f = &mut fixture.frontend;
            f.settings.show_guidance = hints;
            // The canvas of a drawn frame at this target.
            h.frame(f, View::Battle, vec![]);
            let canvas = h.ctx.content_rect();
            assert!(
                (canvas.size() - points).length() < 0.5,
                "{target:?}: {canvas:?}"
            );
            let a = f.game.attempt().unwrap();
            let layout = battle::BattleLayout::new(canvas, a.lane_count, a.width_yards, true);
            let width = (layout.arena.width() * 0.6).min(380.0);
            let top = if hints {
                layout.hint_callout.bottom() + 4.0
            } else {
                layout.arena.top() + 6.0
            };
            let expected = Rect::from_min_size(
                pos2(layout.arena.center().x - width * 0.5, top),
                vec2(width, 34.0),
            );
            let name = format!("{target:?} hints {hints}");
            let banner = layout.boss_banner(hints, false);
            assert!(same_rect(banner, expected), "{name}: {banner:?}");
            let mut clock = Clock::new();
            clock.frame(&h, f, View::Battle, 1);
            let output = clock.run(&h, f, View::Battle, 1, 30);
            // The test's canvas is the drawn one: the callout is where the
            // layout says.
            if hints {
                let (hide, _) = clipped_bounds(&output, "Hide hints").expect("the hint callout");
                assert!(layout.hint_callout.contains_rect(hide), "{name}: {hide:?}");
            } else {
                assert!(!painted(&output, "Hide hints"), "{name}");
            }
            let (title, clip) =
                clipped_bounds(&output, APPROACHING).unwrap_or_else(|| panic!("{name}: no banner"));
            assert!(
                banner.contains_rect(title) && clip.contains_rect(title),
                "{name}: the title {title:?} is outside the banner {expected:?}"
            );
            assert!(
                (title.center().x - layout.arena.center().x).abs() < 0.5,
                "{name}: {title:?} is not centred on the arena"
            );
            // The whole panel, as the battle recorded it.
            assert_eq!(f.battle.boss_banner_rect, Some(banner), "{name}");
            if hints {
                assert!(
                    !title.intersects(layout.hint_callout)
                        && !banner.intersects(layout.hint_callout),
                    "{name}: {expected:?} over the callout {:?}",
                    layout.hint_callout
                );
            }
            assert!(layout.arena.contains_rect(banner), "{name}");
            let subline = output
                .shapes
                .iter()
                .find_map(|c| labeled_suffix(&c.shape, " · arrives in "))
                .unwrap_or_else(|| panic!("{name}: no subline"));
            assert!(
                banner.contains_rect(subline),
                "{name}: the subline {subline:?} is outside the banner {expected:?}"
            );
        }
    }
}

/// Reduced effects: the banner appears and goes at full strength, with no
/// fade. Normal effects fade it in over 0.15 s and out over 0.30 s.
#[test]
fn boss_banner_static_in_reduced_effects() {
    for reduced in [true, false] {
        let mut fixture = personal_before_warning();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.settings.reduced_effects = reduced;
        let mut clock = Clock::new();
        clock.frame(&h, f, View::Battle, 1);
        // 1/60 s, 0.5 s and 1.9 s into the banner.
        let early = color_of(&clock.run(&h, f, View::Battle, 1, 1), APPROACHING);
        let middle = color_of(&clock.run(&h, f, View::Battle, 1, 29), APPROACHING);
        let late = color_of(&clock.run(&h, f, View::Battle, 1, 84), APPROACHING);
        assert_eq!(middle, Some(AMBER), "reduced {reduced}: 0.5 s");
        if reduced {
            assert_eq!(early, Some(AMBER), "reduced: 1/60 s");
            assert_eq!(late, Some(AMBER), "reduced: 1.9 s");
        } else {
            for (at, color) in [("1/60 s", early), ("1.9 s", late)] {
                let color = color.unwrap_or_else(|| panic!("normal: no banner at {at}"));
                assert!(
                    color.a() > 0 && color.a() < 255,
                    "normal: {at} is not mid-fade: {color:?}"
                );
            }
        }
        let gone = clock.run(&h, f, View::Battle, 1, 12);
        assert!(!painted(&gone, APPROACHING), "reduced {reduced}: 2.1 s");
    }
}
