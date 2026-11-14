//! The offline mix render (A6-04) and the loudness gate it feeds.
//!
//! The render runs the shipped assets through the runtime's own decisions
//! (`events::event_sound`, `select::select_sounds`, `select::CueGate`, the mix
//! table) and the voice policy (`render::VoicePolicy`), and meters the
//! result with `meter`. The gate tests run in the normal pass since the mix was
//! calibrated (A6-17); run
//! `cargo test -p horde_audio --test offline_mix -- --include-ignored --nocapture`
//! to print the numbers, the logged max-slider render included.

use horde_audio::{
    cue::Cue,
    meter::Loudness,
    mix::{self, Bus, CONTEXT_GAIN, mix},
    render::{
        self, Context, FRAME_SAMPLES, Frame, Library, RAIN_WEIGHTS, RATE, REFERENCE_WEIGHTS, Scene,
        Sliders, Stereo, VoicePolicy,
    },
    select::{Variety, select_sounds},
    voices::{LiveVoice, bus_cap},
    wav,
};
use horde_core::{content::Mode, sim::Game};
use std::path::{Path, PathBuf};

/// Every bus audible.
const AUDIBLE: [f32; 5] = [1.0; 5];

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}
fn library() -> Library {
    Library::load(&assets()).expect("the shipped audio loads")
}

/// The reference battle: seed 1, Personal, level 0, as `determinism.rs` runs
/// it.
fn reference_battle() -> Game {
    let mut game = Game::default();
    game.set_seed(1);
    game.campaign_mut().attempts = 1;
    game.deploy(0, Mode::Personal).unwrap();
    game.set_paused(false);
    game
}
fn dense_battle() -> Game {
    let mut game = horde_core::sim::maximum_progression_render_fixture().unwrap();
    game.set_paused(false);
    game
}

fn idle(context: Context, count: usize) -> Vec<Frame> {
    (0..count)
        .map(|_| Frame::centred(Vec::new(), context))
        .collect()
}
fn rms(samples: &[f32]) -> f64 {
    (samples
        .iter()
        .map(|sample| f64::from(*sample).powi(2))
        .sum::<f64>()
        / samples.len() as f64)
        .sqrt()
}
fn report(label: &str, loudness: &Loudness) -> String {
    format!(
        "{label}: I {:.1} LUFS, S-max {:.1}, M-max {:.1}, sample peak {:.3}, true peak {:.3}",
        loudness.integrated,
        loudness.short_term_max,
        loudness.momentary_max,
        loudness.sample_peak,
        loudness.true_peak
    )
}
fn default_scene(weights: [f32; 6]) -> Scene {
    Scene {
        weights,
        sliders: Sliders::DEFAULT,
        reduced: false,
        mono: false,
    }
}

// ----- WAV input -----

#[test]
fn pcm16_read_round_trips_write() {
    let samples = [0.0, 0.25, -0.5, 1.0, -1.0];
    let clip = wav::read_pcm16(&wav::pcm16_bytes(22_050, &samples)).unwrap();
    assert_eq!((clip.rate, clip.channels), (22_050, 1));
    assert_eq!(clip.samples.len(), samples.len());
    for (read, written) in clip.samples.iter().zip(samples) {
        assert!((read - written).abs() <= 1.0 / 32_767.0, "{read} {written}");
    }
    assert!(wav::read_pcm16(b"RIFF").is_err());
    assert!(wav::read_pcm16(&wav::pcm16_bytes(22_050, &samples)[..40]).is_err());
}

// ----- the v1 policy shim -----

#[test]
fn legacy_policy_starts_its_budget_per_frame_and_rotates_only_on_frames_with_requests() {
    let mut policy = VoicePolicy::default();
    let requests = vec![Cue::Boss, Cue::Damage, Cue::Rifle];
    let first = policy.frame(requests.clone(), 0, false, &AUDIBLE, &[]);
    assert!(first.fades.is_empty());
    assert_eq!(
        first.starts.iter().map(|s| s.cue).collect::<Vec<_>>(),
        select_sounds(requests, 0)
    );
    assert_eq!(first.starts.len(), 3);
    // Each start plays its bank's own round-robin pick (`tests/variety.rs`).
    let mut variety = Variety::default();
    for start in &first.starts {
        let pick = variety.pick_cue(start.cue);
        assert_eq!((start.variant, start.jitter), (pick.variant, pick.jitter));
    }
    assert_eq!(first.starts[0].gain, mix(Cue::Boss).level(false));
    assert_eq!(first.starts[0].priority, mix(Cue::Boss).priority);
    assert_ne!(first.starts[0].voice, first.starts[1].voice);
    // An empty frame does not advance the rotation; the next request does.
    assert!(
        policy
            .frame(Vec::new(), 17, false, &AUDIBLE, &[])
            .starts
            .is_empty()
    );
    let second = policy.frame(
        vec![Cue::Kill, Cue::Smg, Cue::Pistol],
        34,
        false,
        &AUDIBLE,
        &[],
    );
    assert_eq!(
        second.starts.iter().map(|s| s.cue).collect::<Vec<_>>(),
        select_sounds(vec![Cue::Kill, Cue::Smg, Cue::Pistol], 1)
    );
    // The repeat gate holds a cue back inside its cooldown.
    let gated = policy.frame(vec![Cue::Boss], 1_000, false, &AUDIBLE, &[]);
    assert!(gated.starts.is_empty(), "{gated:?}");
    assert_eq!(
        policy
            .frame(vec![Cue::Boss], 3_000, false, &AUDIBLE, &[])
            .starts
            .len(),
        1
    );
    // Reduced effects plays the reduced column.
    let reduced = policy.frame(vec![Cue::Explosion], 9_000, true, &AUDIBLE, &[]);
    assert_eq!(reduced.starts[0].gain, mix(Cue::Explosion).level(true));
}

#[test]
fn policy_caps_at_eight_effects_and_fades_the_least_important() {
    assert_eq!(bus_cap(Bus::Effects), 8);
    let mut live: Vec<LiveVoice> = (100..108)
        .map(|id| LiveVoice {
            id,
            cue: Cue::Trap,
            started_ms: 0,
        })
        .collect();
    let mut policy = VoicePolicy::default();
    // A cue less important than every live voice is dropped.
    let dropped = policy.frame(vec![Cue::Fire], 0, false, &AUDIBLE, &live);
    assert!(
        dropped.starts.is_empty() && dropped.fades.is_empty(),
        "{dropped:?}"
    );
    // A more important one fades the least important voice out.
    live[2].cue = Cue::Fire;
    let stolen = policy.frame(vec![Cue::Boss], 20, false, &AUDIBLE, &live);
    assert_eq!(stolen.fades, vec![102]);
    assert_eq!(stolen.starts.len(), 1);
    // With a free voice nothing fades.
    let free = policy.frame(vec![Cue::Explosion], 40, false, &AUDIBLE, &live[..7]);
    assert!(free.fades.is_empty());
    assert_eq!(free.starts.len(), 1);
    // A muted bus starts nothing and leaves its banks where they were.
    let mut muted = VoicePolicy::default();
    assert!(
        muted
            .frame(vec![Cue::Boss], 0, false, &[0.0; 5], &[])
            .starts
            .is_empty()
    );
    assert_eq!(
        muted
            .frame(vec![Cue::Boss], 20, false, &AUDIBLE, &[])
            .starts[0]
            .variant,
        Variety::default().pick_cue(Cue::Boss).variant
    );
}

// ----- the render -----

#[test]
fn single_cue_renders_dual_mono_at_bus_times_row_gain() {
    let library = library();
    let mut frames = idle(Context::Battle, 120);
    frames[0].cues.push(Cue::Explosion);
    let scene = Scene {
        weights: REFERENCE_WEIGHTS,
        sliders: Sliders {
            ambience: 0.0,
            ..Sliders::DEFAULT
        },
        reduced: false,
        mono: false,
    };
    let out = render::render(&library, &frames, &scene);
    assert_eq!(out.left.len(), 120 * FRAME_SAMPLES);
    assert_eq!(out.left, out.right);
    // The explosion bank's first pick: its variant, at the pick's speed and
    // gain jitter.
    let pick = Variety::default().pick_cue(Cue::Explosion);
    assert_ne!((pick.jitter.speed, pick.jitter.gain), (1.0, 1.0));
    let clip = render::pitched(
        library.clip(Cue::Explosion, pick.variant),
        pick.jitter.speed,
    );
    let clip = clip.as_slice();
    let clip_peak = clip.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
    let expected = mix::DEFAULT_MASTER
        * mix::DEFAULT_EFFECTS
        * mix::calibration_gain()
        * mix(Cue::Explosion).gain
        * pick.jitter.gain
        * clip_peak;
    let peak = out.loudness().sample_peak;
    assert!((peak - expected).abs() < 1e-6, "{peak} is not {expected}");
    // A mastered clip peaks at 0.4 to 0.8 (the v1 library peaked at 0.18), so one
    // voice at the default sliders (0.8 x 0.8) is 0.15 to 0.4 of full scale.
    assert!((0.15..0.40).contains(&peak), "{peak}");
    // The cue starts on its frame's first sample.
    let first = |samples: &[f32]| samples.iter().position(|s| *s != 0.0);
    assert_eq!(first(&out.left), first(clip));
    // A muted effects bus renders silence.
    let muted = Scene {
        sliders: Sliders {
            effects: 0.0,
            ambience: 0.0,
            ..Sliders::DEFAULT
        },
        ..scene
    };
    assert!(
        render::render(&library, &frames, &muted)
            .left
            .iter()
            .all(|s| *s == 0.0)
    );
}

#[test]
fn ambience_is_dual_mono_and_follows_the_slider_and_the_context() {
    let library = library();
    let battle = idle(Context::Battle, 240);
    let quiet = render::render(&library, &battle, &default_scene(REFERENCE_WEIGHTS));
    assert_eq!(quiet.left, quiet.right);
    assert!(quiet.left.iter().all(|s| s.is_finite()));
    // The loops fade in over the 1.2 s crossfade from silence.
    assert_eq!(quiet.left[0], 0.0);
    let tail = 180 * FRAME_SAMPLES;
    assert!(rms(&quiet.left[tail..]) > 0.0);
    // Half the slider is half the signal.
    let soft = render::render(
        &library,
        &battle,
        &Scene {
            sliders: Sliders {
                ambience: 0.5 * mix::DEFAULT_AMBIENCE,
                ..Sliders::DEFAULT
            },
            ..default_scene(REFERENCE_WEIGHTS)
        },
    );
    let ratio = rms(&soft.left[tail..]) / rms(&quiet.left[tail..]);
    assert!((ratio - 0.5).abs() < 1e-3, "{ratio}");
    // A paused screen holds the loops at the paused share.
    let paused = render::render(
        &library,
        &idle(Context::Paused, 240),
        &default_scene(REFERENCE_WEIGHTS),
    );
    let share = rms(&paused.left[tail..]) / rms(&quiet.left[tail..]);
    let expected = f64::from(CONTEXT_GAIN.paused / CONTEXT_GAIN.battle);
    assert!((share - expected).abs() < 1e-3, "{share} is not {expected}");
    // Another theme is another bed.
    let rain = render::render(&library, &battle, &default_scene(RAIN_WEIGHTS));
    assert_ne!(rain.left, quiet.left);
}

#[test]
fn battle_frames_carry_the_cues_of_the_reference_battle() {
    let mut game = reference_battle();
    let frames = render::battle_frames(&mut game, 600);
    assert_eq!(frames.len(), 600);
    assert!(frames.iter().all(|frame| frame.context == Context::Battle));
    let heard: Vec<Cue> = frames.iter().flat_map(|f| f.cues.iter().copied()).collect();
    assert!(heard.contains(&Cue::Pistol), "no shot in 10 s: {heard:?}");
    assert!(heard.iter().all(|cue| !cue.retired()));
    // The same game gives the same frames.
    let again = render::battle_frames(&mut reference_battle(), 600);
    assert_eq!(frames, again);
}

#[test]
fn reference_scene_is_camp_battle_results() {
    let frames = render::reference_mixed_60s(&mut reference_battle());
    assert_eq!(frames.len(), 3_600);
    assert!(frames[..480].iter().all(|f| f.context == Context::Camp));
    let confirms = frames[..480]
        .iter()
        .filter(|f| f.cues == [Cue::Confirm])
        .count();
    assert_eq!(confirms, 2);
    assert!(frames[3_120..].iter().all(|f| f.context == Context::Paused));
    let stingers = frames
        .iter()
        .flat_map(|f| &f.cues)
        .filter(|cue| matches!(cue, Cue::Victory | Cue::Defeat | Cue::Retreat))
        .count();
    assert_eq!(stingers, 1, "one stinger ends the scene's battle");
}

/// Runs in the normal test pass: ten seconds of the reference scene (camp into
/// battle) render to a finite, audible, unclipped signal.
#[test]
fn render_10s_is_finite_and_audible() {
    let frames = render::reference_mixed_60s(&mut reference_battle());
    let out = render::render(
        &library(),
        &frames[..600],
        &default_scene(REFERENCE_WEIGHTS),
    );
    assert_eq!(out.left.len(), 10 * RATE as usize);
    assert!(out.left.iter().chain(&out.right).all(|s| s.is_finite()));
    let loudness = out.loudness();
    assert!(loudness.integrated.is_finite(), "{loudness:?}");
    assert!(loudness.integrated > -70.0, "{loudness:?}");
    assert!(loudness.sample_peak > 0.01, "{loudness:?}");
    assert!(loudness.true_peak <= 0.95, "{loudness:?}");
}

// ----- the gate (60 s renders; calibrated by A6-17) -----

fn reference_render(weights: [f32; 6], sliders: Sliders) -> Stereo {
    let frames = render::reference_mixed_60s(&mut reference_battle());
    render::render(
        &library(),
        &frames,
        &Scene {
            weights,
            sliders,
            reduced: false,
            mono: false,
        },
    )
}

/// The user's loudness gate: the 60 s reference mix at the default sliders
/// reads -23 +/-2 LUFS integrated with a true peak of at most 0.95 FS, in the
/// stage-1 theme and in rain.
#[test]
fn reference_mixed_60s_meets_loudness_gate() {
    let mut failures = Vec::new();
    for (theme, weights) in [("stage-1", REFERENCE_WEIGHTS), ("rain", RAIN_WEIGHTS)] {
        let loudness = reference_render(weights, Sliders::DEFAULT).loudness();
        let line = report(&format!("reference_mixed_60s [{theme}]"), &loudness);
        println!("{line}");
        if !(-25.0..=-21.0).contains(&loudness.integrated) || loudness.true_peak > 0.95 {
            failures.push(line);
        }
    }
    assert!(
        failures.is_empty(),
        "outside -25 <= I <= -21 LUFS, TP <= 0.95:\n{}",
        failures.join("\n")
    );
}

/// The dense battle holds its true peak under 0.95 FS and its short-term
/// loudness under -14 LUFS. The integrated value is logged.
#[test]
fn dense_battle_60s_true_peak_at_most_0_95() {
    let frames = render::battle_frames(&mut dense_battle(), 3_600);
    let loudness =
        render::render(&library(), &frames, &default_scene(REFERENCE_WEIGHTS)).loudness();
    println!("{}", report("dense_battle_60s", &loudness));
    assert!(loudness.true_peak <= 0.95, "{loudness:?}");
    assert!(loudness.short_term_max <= -14.0, "{loudness:?}");
}

/// Logged only: the reference mix with every slider at 1.0.
#[test]
#[ignore = "logged only"]
fn max_sliders_60s_is_logged() {
    let loudness = reference_render(REFERENCE_WEIGHTS, Sliders::MAX).loudness();
    println!("{}", report("max_sliders_60s", &loudness));
    assert!(
        loudness.sample_peak <= 1.0,
        "the max-slider mix clips: file a follow-up ({loudness:?})"
    );
}

// ----- ducking and density (A6-11) -----

/// A boss arrival pulls the rendered ambience 6 dB down for its hold and
/// lets it back: the ambience under the cue, over the ambience alone.
#[test]
fn render_ducks_the_ambience_under_a_boss_arrival() {
    let library = library();
    let quiet = idle(Context::Battle, 600);
    let mut arrival = quiet.clone();
    arrival[300].cues.push(Cue::Boss);
    let scene = default_scene(REFERENCE_WEIGHTS);
    let silent_bed = Scene {
        sliders: Sliders {
            ambience: 0.0,
            ..Sliders::DEFAULT
        },
        ..scene
    };
    let bed = render::render(&library, &quiet, &scene).left;
    let both = render::render(&library, &arrival, &scene).left;
    let cue = render::render(&library, &arrival, &silent_bed).left;
    assert!(cue.iter().any(|sample| *sample != 0.0));
    let ducked: Vec<f32> = both.iter().zip(&cue).map(|(a, b)| a - b).collect();
    let share = |from: usize, to: usize| {
        let window = from * FRAME_SAMPLES..to * FRAME_SAMPLES;
        rms(&ducked[window.clone()]) / rms(&bed[window])
    };
    // Before the cue: untouched.
    assert!((share(240, 300) - 1.0).abs() < 1e-3, "{}", share(240, 300));
    // 100 ms to 1100 ms after it: inside the 1200 ms hold, 6 dB down.
    let held = share(306, 366);
    let expected = f64::from(mix::db_to_linear(-6.0));
    assert!((held - expected).abs() < 0.01, "{held} is not {expected}");
    // 30 + 1200 + 800 ms later it is back.
    assert!((share(430, 500) - 1.0).abs() < 1e-3, "{}", share(430, 500));
}

/// Six effect voices at once play below their sum: the density gain of six
/// voices, sqrt(4/6), is on every one of them. Four play at their sum.
#[test]
fn render_levels_a_crowd_of_effect_voices() {
    let library = library();
    let scene = Scene {
        sliders: Sliders {
            ambience: 0.0,
            ..Sliders::DEFAULT
        },
        ..default_scene(REFERENCE_WEIGHTS)
    };
    let crowd = [
        (0, Cue::Explosion),
        (0, Cue::Structure),
        (0, Cue::Damage),
        (1, Cue::Molotov),
        (1, Cue::Melee),
        (1, Cue::Trap),
    ];
    let share = |cues: &[(usize, Cue)]| {
        let mut frames = idle(Context::Battle, 30);
        let mut sum = vec![0.0_f32; 30 * FRAME_SAMPLES];
        for (frame, cue) in cues {
            frames[*frame].cues.push(*cue);
            let mut alone = idle(Context::Battle, 30);
            alone[*frame].cues.push(*cue);
            let single = render::render(&library, &alone, &scene).left;
            for (total, sample) in sum.iter_mut().zip(single) {
                *total += sample;
            }
        }
        let together = render::render(&library, &frames, &scene).left;
        // 67 ms to 133 ms: the compensation has come in, the clips still play.
        let window = 4 * FRAME_SAMPLES..8 * FRAME_SAMPLES;
        assert!(rms(&sum[window.clone()]) > 0.0);
        rms(&together[window.clone()]) / rms(&sum[window])
    };
    let four = share(&crowd[..4]);
    assert!((four - 1.0).abs() < 1e-4, "{four}");
    let six = share(&crowd);
    let expected = f64::from(mix::density_gain(6));
    assert!((six - expected).abs() < 0.03, "{six} is not {expected}");
    // The policy marks who takes the gain: not a warning.
    let mut policy = VoicePolicy::default();
    let starts = policy
        .frame(vec![Cue::Boss, Cue::Impact], 0, false, &AUDIBLE, &[])
        .starts;
    let marks: Vec<(Cue, bool)> = starts
        .iter()
        .map(|start| (start.cue, start.compensated))
        .collect();
    assert_eq!(marks, [(Cue::Boss, false), (Cue::Impact, true)]);
}
