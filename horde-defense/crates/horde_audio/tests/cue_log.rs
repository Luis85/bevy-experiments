//! The review cue log (A6-18): what the executor did each frame, as JSON
//! lines, and the replay that renders a log back to a signal.
//!
//! The offline render writes the same log the game's review service writes
//! (`LASTLIGHT_REVIEW_CUE_LOG`), so a log of a real session can be metered
//! with `cargo run -p horde_audio --example replay_cue_log -- <cues.jsonl>`
//! and compared with the loudness gate's reference scene.

use horde_audio::{
    cue::Cue,
    cue_log::{self, CueLogFrame, LoggedStart},
    render::{
        self, FrameActions, Library, REFERENCE_WEIGHTS, Scene, Sliders, VoicePolicy, VoiceStart,
    },
};
use horde_core::{content::Mode, sim::Game};
use std::path::Path;

fn library() -> Library {
    Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
        .expect("the shipped audio loads")
}
/// The reference battle of the loudness gate (`tests/offline_mix.rs`).
fn reference_battle() -> Game {
    let mut game = Game::default();
    game.set_seed(1);
    game.campaign_mut().attempts = 1;
    game.deploy(0, Mode::Personal).unwrap();
    game.set_paused(false);
    game
}

/// One frame's `FrameActions` become one log line: every start with the clip
/// it plays, its speed, tone and the gains the executor gave it, and every
/// fade by voice id. The line parses back to the same frame.
#[test]
fn frame_actions_map_to_one_log_line() {
    let mut policy = VoicePolicy::default();
    let actions: FrameActions = policy.frame(vec![Cue::Boss, Cue::Rifle], 0, false, &[1.0; 5], &[]);
    assert_eq!(actions.starts.len(), 2);
    let actions = FrameActions {
        fades: vec![7, u64::MAX],
        ..actions
    };
    let gains = |start: &VoiceStart| [start.gain * 0.5, start.gain];
    let frame = CueLogFrame::from_actions(
        1_500_000,
        16_667,
        &actions,
        gains,
        [0.1, 0.2, 0.0, 0.0, 0.0, 0.0],
        0.8,
    );
    assert_eq!(frame.fades, [7, u64::MAX]);
    let boss = &actions.starts[0];
    assert_eq!(
        frame.starts[0],
        LoggedStart {
            voice: boss.voice,
            cue: "Boss".to_owned(),
            clip: boss.voicing.bank.variant_path(boss.variant),
            speed: boss.jitter.speed * boss.voicing.speed,
            lowpass_hz: boss.voicing.lowpass_hz,
            gains: gains(boss),
            compensated: boss.compensated,
        }
    );
    let line = frame.line();
    assert!(!line.contains('\n'), "{line}");
    let text = cue_log::write(std::slice::from_ref(&frame));
    assert_eq!(text.lines().count(), 2, "a version header and one frame");
    assert_eq!(cue_log::parse(&text).unwrap(), [frame]);
    // A log without the header, or of another version, is refused.
    assert!(cue_log::parse(&line).is_err());
    assert!(cue_log::parse("{\"cue_log\":99}\n").is_err());
}

/// The gate's cross-check: the reference scene's log survives the JSON-lines
/// round trip exactly, and its replay is the render (the target is within
/// 1 LU of `reference_mixed_60s`; the replay runs the render's own executor,
/// so it is the same signal).
#[test]
fn cue_log_round_trips_frame_actions() {
    let library = library();
    let frames = render::reference_mixed_60s(&mut reference_battle());
    let scene = Scene {
        weights: REFERENCE_WEIGHTS,
        sliders: Sliders::DEFAULT,
        reduced: false,
        mono: false,
    };
    let (rendered, log) = render::render_logged(&library, &frames, &scene);
    assert_eq!(rendered, render::render(&library, &frames, &scene));
    assert_eq!(log.len(), frames.len());
    assert!(log.iter().map(|frame| frame.starts.len()).sum::<usize>() > 100);

    let parsed = cue_log::parse(&cue_log::write(&log)).unwrap();
    assert_eq!(parsed, log, "the log round-trips through JSON lines");

    let replayed = cue_log::replay(&library, &parsed).unwrap();
    assert_eq!(replayed.left.len(), rendered.left.len());
    let (reference, replay) = (rendered.loudness(), replayed.loudness());
    println!(
        "reference_mixed_60s {:.2} LUFS TP {:.3}; replay {:.2} LUFS TP {:.3}",
        reference.integrated, reference.true_peak, replay.integrated, replay.true_peak
    );
    assert!(
        (reference.integrated - replay.integrated).abs() <= 1.0,
        "{} {}",
        reference.integrated,
        replay.integrated
    );
    assert_eq!(replayed, rendered, "the replay is the render");
}

/// A logged fade ends its voice as the executor does: silent
/// `voices::FADE_MS` after the frame that fades it, where the unfaded voice
/// still sounds.
#[test]
fn a_logged_fade_ends_its_voice_after_the_fade() {
    let library = library();
    let mut policy = VoicePolicy::default();
    let actions = policy.frame(vec![Cue::Victory], 0, false, &[1.0; 5], &[]);
    let voice = actions.starts[0].voice;
    let silent = [0.0; 6];
    let start = CueLogFrame::from_actions(0, 100_000, &actions, |_| [0.5, 0.5], silent, 1.0);
    let rest = |fades: Vec<u64>| CueLogFrame {
        us: 100_000,
        dt_us: 400_000,
        fades,
        starts: Vec::new(),
        ambience: silent,
        density: 1.0,
    };
    let faded = cue_log::replay(&library, &[start.clone(), rest(vec![voice])]).unwrap();
    let kept = cue_log::replay(&library, &[start, rest(Vec::new())]).unwrap();
    let rate = render::RATE as usize;
    assert_eq!(faded.left.len(), rate / 2);
    // The same signal up to the fade's frame.
    assert_eq!(faded.left[..rate / 10], kept.left[..rate / 10]);
    assert!(faded.left[..rate / 10].iter().any(|sample| *sample != 0.0));
    let after = rate / 10 + rate * (horde_audio::voices::FADE_MS as usize + 5) / 1000;
    assert!(faded.left[after..].iter().all(|sample| *sample == 0.0));
    assert!(kept.left[after..].iter().any(|sample| *sample != 0.0));
}

/// A log names clips by path; one the library does not hold is an error, not
/// a silent gap.
#[test]
fn replay_refuses_an_unknown_clip() {
    let frame = CueLogFrame {
        us: 0,
        dt_us: 100_000,
        fades: Vec::new(),
        starts: vec![LoggedStart {
            voice: 1,
            cue: "Pistol".to_owned(),
            clip: "audio/nope.wav".to_owned(),
            speed: 1.0,
            lowpass_hz: None,
            gains: [1.0, 1.0],
            compensated: false,
        }],
        ambience: [0.0; 6],
        density: 1.0,
    };
    let error = cue_log::replay(&library(), &[frame]).unwrap_err();
    assert!(error.contains("audio/nope.wav"), "{error}");
}
