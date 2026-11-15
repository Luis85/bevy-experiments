//! The loot and coin sounds (A6-14): the Results screen sounds a report
//! once, and no widget writes a sound for a drop or for a hover.
use super::*;
use crate::audio::{Cue, CueRequest};
use horde_audio::loot::ResultsAudioGuard;
use horde_core::{kind_map::KindMap, quality::Quality};

/// A frontend whose last run was withdrawn: it has a report to show.
fn after_a_run() -> IsolatedFrontend {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    fixture.frontend.game.apply(&Command::Withdraw).unwrap();
    assert!(fixture.frontend.game.last_result().is_some());
    fixture
}

#[test]
fn results_screen_reveals_the_loot_once_and_plays_one_coin_batch_for_fifty_sales() {
    let mut fixture = after_a_run();
    let f = &mut fixture.frontend;
    let report = f.game.last_result_mut().as_mut().unwrap();
    report.loot_by_quality = KindMap::from_fn(|quality| match quality {
        Quality::Common => 49,
        Quality::Epic => 1,
        _ => 0,
    });
    report.auto_sold.count = 50;
    let mut guard = ResultsAudioGuard::default();
    // Not on the Results screen: nothing.
    assert!(notices::results_cues(&mut guard, Screen::Camp, &f.game, 0).is_empty());
    let frames = |guard: &mut ResultsAudioGuard, from_ms: u64| -> Vec<CueRequest> {
        (0..60)
            .flat_map(|frame| {
                notices::results_cues(guard, Screen::Results, &f.game, from_ms + frame * 16)
            })
            .collect()
    };
    let heard = frames(&mut guard, 1000);
    let cues: Vec<Cue> = heard.iter().map(|request| request.cue).collect();
    assert_eq!(cues, vec![Cue::LootReveal, Cue::CoinBatch]);
    // The best drop tunes the reveal; both are centred interface sounds.
    assert_eq!(heard[0].voicing.unwrap().speed, 1.03);
    assert!(heard.iter().all(|r| r.place.is_none() && !r.player_weapon));
    // A re-render, and History and back: nothing more.
    assert!(frames(&mut guard, 3000).is_empty());
    assert!(notices::results_cues(&mut guard, Screen::History, &f.game, 5000).is_empty());
    assert!(frames(&mut guard, 5016).is_empty());
}

/// Attempt ids restart at 1 in a new journey: the new journey's first
/// Results still reveals its loot and plays its coin batch, as the guard the
/// Results system keeps is cleared with the other per-journey presentation.
#[test]
fn a_new_journey_results_on_the_same_attempt_id_sounds_again() {
    let mut fixture = after_a_run();
    let f = &mut fixture.frontend;
    let give_loot = |game: &mut Game| {
        let report = game.last_result_mut().as_mut().unwrap();
        report.loot_by_quality = KindMap::from_fn(|quality| match quality {
            Quality::Rare => 1,
            _ => 0,
        });
        report.auto_sold.count = 3;
        report.attempt_id
    };
    let heard = |f: &mut FrontendParts, from_ms: u64| -> Vec<Cue> {
        (0..60)
            .flat_map(|frame| {
                notices::results_cues(
                    &mut f.battle.results_audio,
                    Screen::Results,
                    &f.game,
                    from_ms + frame * 16,
                )
            })
            .map(|request| request.cue)
            .collect()
    };
    // The edited report would not save: the old journey keeps its own when
    // New game archives it.
    let saved = f.game.last_result().cloned();
    let old_attempt = give_loot(&mut f.game);
    f.screen = Screen::Results;
    assert_eq!(
        heard(f, 1000),
        vec![Cue::LootReveal, Cue::CoinBatch],
        "the old journey"
    );
    *f.game.last_result_mut() = saved;
    // New game, and the new journey's first defense ends.
    f.screen = Screen::Menu;
    f.act(|c| c.dispatch(AppCommand::NewJourney, ""));
    f.flush_for_test();
    assert!(!f.status.message_error, "{}", f.status.message);
    assert!(
        f.game.last_result().is_none(),
        "the new journey has no report"
    );
    assert!(notices::results_cues(&mut f.battle.results_audio, f.screen, &f.game, 3000).is_empty());
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.deploy_country_stage(0, Mode::Personal).unwrap();
    f.game.apply(&Command::Withdraw).unwrap();
    assert_eq!(give_loot(&mut f.game), old_attempt, "the same attempt id");
    f.screen = Screen::Results;
    assert_eq!(
        heard(f, 5000),
        vec![Cue::LootReveal, Cue::CoinBatch],
        "the new journey"
    );
    f.runtime.finish_storage_for_test();
}

/// The battle's loot fly-up draws a Legendary drop and plays nothing: the
/// runner's tracker owns the sound.
#[test]
fn battle_loot_widget_writes_no_sound_request() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let attempt = f.game.attempt_mut().unwrap();
    let tick = attempt.tick;
    attempt.loot_events.push(horde_core::quality::LootEvent {
        at: [6.0, 1.5],
        item_id: 1,
        definition: content().items[0].id.clone(),
        level: 1,
        quality: Quality::Legendary,
        name: "Test".into(),
        tick,
    });
    for _ in 0..3 {
        h.frame(f, View::Battle, vec![]);
    }
    assert_eq!(f.cues, vec![]);
}

/// The centre of every text of a frame.
fn text_centres(shape: &egui::epaint::Shape, out: &mut Vec<Pos2>) {
    match shape {
        egui::epaint::Shape::Text(text) => {
            out.push(text.pos + text.galley.rect.center().to_vec2());
        }
        egui::epaint::Shape::Vec(shapes) => {
            for shape in shapes {
                text_centres(shape, out);
            }
        }
        _ => {}
    }
}

/// A guard (user decision 17: no hover sound). The pointer visits every
/// text of the screen (every button, row, tab and node carries one) and a
/// 64 pt grid over the rest, on Results (the upgrade home), Camp home, the
/// skill graph, Equipment and Settings at both target sizes; no frame may
/// write a cue.
#[test]
fn no_widget_hover_writes_a_sound_request() {
    let scenes = [
        ("results", Screen::Results, None),
        ("camp home", Screen::Camp, Some(Tab::Home)),
        ("skills", Screen::Camp, Some(Tab::Skills)),
        ("equipment", Screen::Camp, Some(Tab::Armory)),
        ("settings", Screen::Settings, None),
    ];
    for target in Target::BOTH {
        let mut fixture = after_a_run();
        let h = Harness::at(target);
        let f = &mut fixture.frontend;
        f.game.campaign_mut().add_item(4).unwrap();
        for (name, screen, tab) in scenes {
            f.screen = screen;
            if let Some(tab) = tab {
                f.nav.tab = tab;
            }
            for _ in 0..2 {
                h.frame(f, View::Full, vec![]);
            }
            let output = h.frame(f, View::Full, vec![]);
            f.cues.clear();
            let mut points = Vec::new();
            for clipped in &output.shapes {
                text_centres(&clipped.shape, &mut points);
            }
            let screen_rect = h.ctx.content_rect();
            assert!(points.len() >= 5, "{name}: {} texts", points.len());
            let (mut x, mut y) = (screen_rect.left() + 8.0, screen_rect.top() + 8.0);
            while y < screen_rect.bottom() {
                while x < screen_rect.right() {
                    points.push(pos2(x, y));
                    x += 64.0;
                }
                x = screen_rect.left() + 8.0;
                y += 64.0;
            }
            points.retain(|point| screen_rect.contains(*point));
            // One visit per 8 pt cell is enough.
            points.sort_by_key(|point| ((point.y / 8.0) as i32, (point.x / 8.0) as i32));
            points.dedup_by_key(|point| ((point.y / 8.0) as i32, (point.x / 8.0) as i32));
            for point in points {
                h.frame(f, View::Full, vec![Event::PointerMoved(point)]);
                assert_eq!(
                    f.cues,
                    vec![],
                    "{name} at {target:?}: hovering {point:?} wrote a sound request"
                );
            }
        }
    }
}
