use super::{recording::*, replay::*, *};
use crate::{
    profile::ProfilePaths,
    runner::{ControlFrame, RuntimeCtx, flush, step},
    settings::Settings,
};
use horde_core::{
    command::Command,
    content::{Mode, StructureKind},
    sim::Input,
};

/// A scratch folder removed on drop (the runner may autosave into it).
struct Scratch(PathBuf);
impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("lastlight-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn camp_game() -> Game {
    horde_core::catalog::init_catalogs().unwrap();
    let mut game = Game::default();
    game.campaign_mut().legacy_features = true;
    game.campaign_mut()
        .set_balance_settings(horde_core::balance::BalanceSettings::legacy())
        .unwrap();
    game.campaign_mut()
        .choose_survivor(horde_core::survivors::SurvivorId::Rook)
        .unwrap();
    game.campaign_mut().choose_start_country("DEU").unwrap();
    game
}

/// Drives the real runner passes for a scripted session and returns the final game.
fn scripted_session(game: &mut Game, runtime: &mut Runtime, scratch: &Scratch) -> Vec<u64> {
    let settings = Settings::default();
    let paths = ProfilePaths::in_directory(scratch.0.clone());
    let (mut notices, mut cues) = (Vec::new(), Vec::new());
    let mut pass = |game: &mut Game, runtime: &mut Runtime, frame: u32| {
        runtime.begin_frame(frame);
        let mut ctx = RuntimeCtx {
            game,
            runtime,
            settings: &settings,
            paths: &paths,
            notices: &mut notices,
            cues: &mut cues,
            game_changed: false,
        };
        flush(&mut ctx);
        step(&mut ctx);
    };
    let mut frame = 0;
    let mut next = |game: &mut Game, runtime: &mut Runtime| {
        frame += 1;
        pass(game, runtime, frame);
    };
    runtime.enqueue(
        Command::Deploy {
            stage: 0,
            mode: Mode::Personal,
        },
        "",
    );
    runtime.publish(ControlFrame {
        enabled: true,
        ..ControlFrame::default()
    });
    let mut ticks_after = Vec::new();
    for tick in 0..240u32 {
        match tick {
            10 => {
                runtime.enqueue(
                    Command::Place {
                        kind: StructureKind::Wall,
                        lane: 1,
                        column: 4,
                    },
                    "",
                );
            }
            // Pause, then single-step three ticks through the runner.
            60 => {
                runtime.enqueue(Command::SetPaused(true), "");
            }
            62..=64 => runtime.request_step(),
            70 => {
                runtime.enqueue(Command::SetPaused(false), "");
            }
            _ => {}
        }
        // Hold fire from tick 20 and sweep the aim, so input lines vary.
        let held = Input {
            fire: tick >= 20,
            aim: [15.0 + (tick / 30) as f32, 5.5],
            ..Input::default()
        };
        runtime.publish(ControlFrame {
            enabled: true,
            held,
            ..ControlFrame::default()
        });
        next(game, runtime);
        ticks_after.push(runtime.debug_snapshot().simulated_ticks);
    }
    ticks_after
}

#[test]
fn a_session_recorded_through_the_runner_replays_to_the_same_digest() {
    let scratch = Scratch::new("record-replay");
    let path = scratch.0.join("session.jsonl");
    let mut game = camp_game();
    let mut runtime = Runtime::default();
    runtime.start_recording(Recorder::create(&path, &game, 0).unwrap());

    let ticks_after = scripted_session(&mut game, &mut runtime, &scratch);
    let added = |frame: usize| ticks_after[frame] - ticks_after[frame - 1];
    assert_eq!(added(61), 0, "paused: no tick");
    for frame in 62..=64 {
        assert_eq!(
            added(frame),
            1,
            "F5 step at frame {frame} adds exactly one tick"
        );
    }
    assert_eq!(added(65), 0, "re-paused after the step");
    assert!(
        game.attempt().is_some_and(|a| a.tick > 100),
        "the scripted session really simulated ticks"
    );
    assert!(runtime.stop_recording(&game, "test"));
    let expected = game.state_digest();
    runtime.finish_storage_for_test();

    let lines = read_lines(&path).unwrap();
    assert!(matches!(lines.first(), Some(Line::Header(_))));
    let commands = lines
        .iter()
        .filter(|l| matches!(l, Line::Command { .. }))
        .count();
    let inputs = lines
        .iter()
        .filter(|l| matches!(l, Line::Input { .. }))
        .count();
    assert!(
        commands >= 6,
        "deploy, place, pauses and step toggles: {commands}"
    );
    assert!(
        (3..120).contains(&inputs),
        "inputs are written only when they change, not every tick: {inputs}"
    );

    let report = replay_file(&path).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.digest, expected);
    assert!(report.matches_recording());
    assert!(report.ticks > 100);
}

#[test]
fn replay_detects_a_tampered_recording() {
    let scratch = Scratch::new("record-tamper");
    let path = scratch.0.join("session.jsonl");
    let mut game = camp_game();
    let mut runtime = Runtime::default();
    runtime.start_recording(Recorder::create(&path, &game, 0).unwrap());
    scripted_session(&mut game, &mut runtime, &scratch);
    runtime.stop_recording(&game, "test");
    runtime.finish_storage_for_test();

    let mut lines = read_lines(&path).unwrap();
    lines.retain(|l| {
        !matches!(
            l,
            Line::Command {
                command: Command::Deploy { .. },
                ..
            }
        )
    });
    let report = replay(lines).unwrap();
    assert!(
        !report.matches_recording(),
        "dropping the deploy command changes the outcome"
    );
}

#[test]
fn header_mismatches_warn_and_garbage_is_an_error() {
    horde_core::catalog::init_catalogs().unwrap();
    let game = Game::default();
    let scratch = Scratch::new("record-header");
    let path = scratch.0.join("empty.jsonl");
    let recorder = Recorder::create(&path, &game, 0).unwrap();
    recorder.finish(0, &game, "test");
    let mut lines = read_lines(&path).unwrap();
    let Some(Line::Header(header)) = lines.first_mut() else {
        panic!("header first");
    };
    header.content_fingerprint = hex(1);
    header.ruleset += 1;
    let report = replay(lines).unwrap();
    assert_eq!(report.warnings.len(), 2, "{:?}", report.warnings);
    assert!(report.matches_recording());

    assert!(replay(vec![]).is_err());
    std::fs::write(&path, "not json\n").unwrap();
    assert!(read_lines(&path).is_err());
}

#[test]
fn digest_is_stable_and_state_sensitive() {
    horde_core::catalog::init_catalogs().unwrap();
    let mut game = Game::default();
    let first = game.state_digest();
    assert_eq!(first, game.clone().state_digest());
    game.set_seed(game.seed() ^ 1);
    assert_ne!(first, game.state_digest());
}

#[test]
fn an_unusable_step_request_is_dropped_and_the_ordinary_tick_still_runs() {
    let scratch = Scratch::new("record-step");
    let mut game = camp_game();
    game.deploy_country_stage(0, Mode::Personal).unwrap();
    game.set_paused(false);
    let mut runtime = Runtime::default();
    runtime.publish(ControlFrame {
        enabled: true,
        ..ControlFrame::default()
    });
    runtime.request_step();
    let settings = Settings::default();
    let paths = ProfilePaths::in_directory(scratch.0.clone());
    let (mut notices, mut cues) = (Vec::new(), Vec::new());
    let mut ctx = RuntimeCtx {
        game: &mut game,
        runtime: &mut runtime,
        settings: &settings,
        paths: &paths,
        notices: &mut notices,
        cues: &mut cues,
        game_changed: false,
    };
    step(&mut ctx);
    assert_eq!(
        runtime.debug_snapshot().simulated_ticks,
        1,
        "ordinary tick ran"
    );
    assert_eq!(
        runtime.debug_snapshot().step_requests,
        0,
        "request consumed"
    );
    assert_eq!(game.attempt().unwrap().tick, 1);
}

#[test]
fn panel_reports_the_live_attempt() {
    let mut game = camp_game();
    game.deploy_country_stage(0, Mode::Personal).unwrap();
    let text = sim_panel_lines(&game, &Runtime::default(), false).join("\n");
    for needle in [
        "seed",
        "attempt",
        "enemies",
        "corpses",
        "event seq",
        "recording",
    ] {
        assert!(text.contains(needle), "{needle} in {text}");
    }
}

#[test]
fn dump_writes_the_save_payload() {
    let scratch = Scratch::new("record-dump");
    horde_core::catalog::init_catalogs().unwrap();
    let game = Game::default();
    let path = dump_game(&game, &scratch.0).unwrap();
    assert!(path.starts_with(scratch.0.join("dumps")));
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&game).unwrap());
}
