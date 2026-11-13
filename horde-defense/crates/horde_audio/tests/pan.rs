//! Lane pan (A6-07): a positional cue pans along the lane, relative to the
//! listener at the defender, through a peak-safe balance law. No channel ever
//! exceeds the centred level, the mono fold-down stays within 1.5 dB, and the
//! "Mono audio" setting centres every voice.

use horde_audio::{
    cue::Cue,
    events::{event_sound, event_sound_at, event_x},
    mix::{Sliders, calibration_gain, mix},
    pan::{
        self, CENTRE, DEFENDER_X, PAN_DEPTH, PAN_DEPTH_REDUCED, Places, depth, gains,
        lane_position, mono_fold_db, positional, voice_gains,
    },
    render::{self, Context, Frame, Library, REFERENCE_WEIGHTS, Scene},
};
use horde_core::{
    content::Mode,
    sim::{Game, SimEvent},
};
use std::path::Path;

fn library() -> Library {
    Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
        .expect("the shipped audio loads")
}
fn peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
}

/// Field widths the game uses or could: the legacy 30 yards and the range
/// around it.
const WIDTHS: [f32; 5] = [12.0, 20.0, 30.0, 45.0, 60.0];

/// Every point of every field: lane counts 3 to 17, every lane centre, x
/// from behind the defender to past the far end.
fn every_point() -> impl Iterator<Item = (u8, [f32; 2], f32)> {
    (3..=17_u8).flat_map(|lanes| {
        WIDTHS.into_iter().flat_map(move |width| {
            (0..lanes).flat_map(move |lane| {
                (-4..=124).map(move |step| {
                    let x = width * step as f32 / 120.0;
                    (lanes, [x, f32::from(lane) + 0.5], width)
                })
            })
        })
    })
}

/// The user's requirement: a mix that pans stays mono-compatible. At every
/// point of every field, in normal and reduced effects, no channel is louder
/// than the centred voice (pan never raises a peak) and the mono fold-down
/// loses at most 1.5 dB.
#[test]
fn pan_is_mono_compatible_for_every_lane_count_3_to_17() {
    let mut deepest = 0.0_f32;
    let mut count = 0;
    for (lanes, at, width) in every_point() {
        for reduced in [false, true] {
            let [left, right] = gains(lane_position(at[0], width), depth(reduced, false));
            assert!(
                left <= 1.0 && right <= 1.0 && left > 0.0 && right > 0.0,
                "{lanes} lanes, {at:?} of {width}: {left} / {right}"
            );
            let fold = mono_fold_db([left, right]);
            assert!(
                (-1.5..=0.0).contains(&fold),
                "{lanes} lanes, {at:?} of {width}, reduced {reduced}: the mono fold-down moves {fold} dB"
            );
            deepest = deepest.min(fold);
            count += 1;
        }
    }
    assert!(count > 10_000, "{count}");
    // The law's numbers at the far end: 3.1 dB between the channels, 1.4 dB
    // lost in mono, and half of the depth in reduced effects.
    let far = gains(1.0, depth(false, false));
    assert_eq!(far, [1.0 - PAN_DEPTH, 1.0]);
    assert!((20.0 * (far[1] / far[0]).log10() - 3.1).abs() < 0.05);
    assert!((deepest + 1.41).abs() < 0.01, "{deepest}");
    assert_eq!(
        gains(1.0, depth(true, false)),
        [1.0 - PAN_DEPTH_REDUCED, 1.0]
    );
    // The listener's own position is centred: today's dual-mono level.
    assert_eq!(gains(0.0, PAN_DEPTH), CENTRE);
    // The pan is audible: the far end differs from the centre.
    assert!(far[0] < 0.75);
    // Garbage in, centre out.
    assert_eq!(gains(f32::NAN, PAN_DEPTH), CENTRE);
    assert_eq!(gains(1.0, f32::NAN), CENTRE);
    assert_eq!(gains(7.0, PAN_DEPTH), far);
}

/// The "Mono audio" setting: depth 0, so every voice of every cue plays
/// centred wherever it happens, in the law and in the render.
#[test]
fn mono_audio_forces_centre() {
    assert_eq!(depth(false, true), 0.0);
    assert_eq!(depth(true, true), 0.0);
    for (_, at, width) in every_point() {
        let p = lane_position(at[0], width);
        for reduced in [false, true] {
            assert_eq!(gains(p, depth(reduced, true)), CENTRE);
        }
    }
    let mut places = Places::default();
    for cue in Cue::ALL {
        places.place(cue, 1.0);
    }
    for cue in Cue::ALL {
        for reduced in [false, true] {
            assert_eq!(voice_gains(cue, &places, reduced, true), CENTRE, "{cue:?}");
        }
    }
    // Rendered: an explosion at the far end is quieter on the left; with mono
    // audio the two channels are the same signal at the centred level.
    let library = library();
    let mut frames: Vec<Frame> = (0..120)
        .map(|_| Frame::centred(Vec::new(), Context::Battle))
        .collect();
    frames[0].cues.push(Cue::Explosion);
    let centred = frames.clone();
    frames[0].places.place(Cue::Explosion, 1.0);
    let scene = |mono| Scene {
        weights: REFERENCE_WEIGHTS,
        sliders: Sliders {
            ambience: 0.0,
            ..Sliders::DEFAULT
        },
        reduced: false,
        mono,
    };
    let reference = render::render(&library, &centred, &scene(false));
    assert_eq!(reference.left, reference.right);
    let panned = render::render(&library, &frames, &scene(false));
    assert_eq!(panned.right, reference.right);
    let ratio = peak(&panned.left) / peak(&panned.right);
    assert!((ratio - (1.0 - PAN_DEPTH)).abs() < 1e-4, "{ratio}");
    let mono = render::render(&library, &frames, &scene(true));
    assert_eq!(mono.left, mono.right);
    assert_eq!(mono, reference);
}

/// The position reads only x and the field width: never the lane (y), the
/// lane count or anything else. It is 0 at the defender and 1 at the far end.
#[test]
fn pan_depends_only_on_x_and_width() {
    for (lanes, at, width) in every_point() {
        let p = lane_position(at[0], width);
        let expected = ((at[0] - DEFENDER_X) / (width - DEFENDER_X)).clamp(0.0, 1.0);
        assert_eq!(p, expected, "{lanes} lanes, {at:?} of {width}");
    }
    assert_eq!(lane_position(DEFENDER_X, 30.0), 0.0);
    assert_eq!(lane_position(0.0, 30.0), 0.0);
    assert_eq!(lane_position(30.0, 30.0), 1.0);
    assert_eq!(lane_position(15.25, 30.0), 0.5);
    for (x, width) in [(f32::NAN, 30.0), (5.0, f32::NAN), (5.0, 0.5), (5.0, 0.0)] {
        assert_eq!(lane_position(x, width), 0.0, "{x} of {width}");
    }
    // The listener is where the core puts the defender.
    let mut game = Game::default();
    game.deploy(0, Mode::Personal).unwrap();
    let attempt = game.attempt().unwrap();
    assert_eq!(attempt.defender_position()[0], DEFENDER_X);
    // An event's place is its x through that mapping, whatever its lane.
    for lane in [0.5, 3.5, 16.5] {
        let event = SimEvent::Kill {
            enemy_id: 1,
            lane: None,
            at: [20.0, lane],
            coins: 0,
        };
        let (mut cues, mut places) = (Vec::new(), Places::default());
        event_sound_at(&mut cues, &mut places, &event, Some(30.0));
        assert_eq!(cues, vec![Cue::Kill]);
        assert_eq!(places.of(Cue::Kill), Some(lane_position(20.0, 30.0)));
        assert_eq!(event_x(Cue::Kill, &event), Some(20.0));
        // The cues are those of the unplaced mapping.
        let mut plain = Vec::new();
        event_sound(&mut plain, &event);
        assert_eq!(plain, cues);
        // Without a field (no attempt) the cue is centred.
        let (mut cues, mut places) = (Vec::new(), Places::default());
        event_sound_at(&mut cues, &mut places, &event, None);
        assert_eq!((cues.len(), places), (1, Places::default()));
    }
    // The first place of a cue in a frame stands, as its first request does.
    let mut places = Places::default();
    places.place(Cue::Kill, 0.25);
    places.place(Cue::Kill, 0.75);
    assert_eq!(places.of(Cue::Kill), Some(0.25));
}

/// The defender's own gun, the reload, the throw, injuries, melee, interface
/// sounds, stingers and the wave and boss warnings never pan, wherever a
/// place is claimed for them.
#[test]
fn defender_ui_stinger_ambience_centred() {
    let positional_cues = [
        Cue::TurretShot,
        Cue::Impact,
        Cue::Armor,
        Cue::Explosion,
        Cue::Molotov,
        Cue::Fire,
        Cue::Structure,
        Cue::Trap,
        Cue::Incoming,
        Cue::Kill,
    ];
    let mut places = Places::default();
    for cue in Cue::ALL {
        places.place(cue, 1.0);
    }
    for cue in Cue::ALL {
        let expected = positional_cues.contains(&cue);
        assert_eq!(positional(cue), expected, "{cue:?}");
        assert_eq!(places.of(cue).is_some(), expected, "{cue:?}");
        let gains = voice_gains(cue, &places, false, false);
        if expected {
            assert_eq!(gains, [1.0 - PAN_DEPTH, 1.0], "{cue:?}");
        } else {
            assert_eq!(gains, CENTRE, "{cue:?}");
        }
        // A claimed place cannot pan a centred cue either.
        assert_eq!(
            pan::request_gains(cue, Some(1.0), false, false) == CENTRE,
            !expected,
            "{cue:?}"
        );
    }
    for cue in [
        Cue::Pistol,
        Cue::Smg,
        Cue::Shotgun,
        Cue::Rifle,
        Cue::Marksman,
        Cue::MachineGun,
        Cue::Reload,
        Cue::Throw,
        Cue::Damage,
        Cue::Melee,
        Cue::Ready,
        Cue::Wave,
        Cue::Boss,
        Cue::Victory,
        Cue::Defeat,
        Cue::Retreat,
        Cue::Confirm,
    ] {
        assert!(!positional(cue), "{cue:?}");
    }
    // The ambience loops are dual mono: a render of ambience alone has two
    // equal channels.
    let frames: Vec<Frame> = (0..180)
        .map(|_| Frame::centred(Vec::new(), Context::Battle))
        .collect();
    let scene = Scene {
        weights: REFERENCE_WEIGHTS,
        sliders: Sliders::MAX,
        reduced: false,
        mono: false,
    };
    let out = render::render(&library(), &frames, &scene);
    assert!(peak(&out.left) > 0.0);
    assert_eq!(out.left, out.right);
}

/// Headroom of one voice: at the maximum sliders, the loudest variant of the
/// loudest row, on its louder (near) channel, peaks at or below 0.75.
#[test]
fn single_voice_peak_at_max_sliders_le_0_75() {
    let library = library();
    let bus = calibration_gain();
    let mut worst = (0.0_f32, None);
    for cue in Cue::ALL.into_iter().filter(|cue| !cue.retired()) {
        assert_eq!(Sliders::MAX.bus_gain(mix(cue).bus), bus, "{cue:?}");
        for variant in 0..cue.bank().variants() {
            for p in [0.0, 0.5, 1.0] {
                let near = pan::request_gains(cue, Some(p), false, false)
                    .into_iter()
                    .fold(0.0_f32, f32::max);
                assert_eq!(near, 1.0, "{cue:?}");
                let level = peak(library.clip(cue, u64::from(variant)))
                    * mix(cue).level(false)
                    * bus
                    * near;
                if level > worst.0 {
                    worst = (level, Some((cue, variant)));
                }
            }
        }
    }
    assert!(worst.0 <= 0.75, "{worst:?}");
    assert!(worst.0 > 0.3, "{worst:?}");
}

/// Headroom of the ambience: at the maximum sliders, in battle, the loops of
/// either reference theme peak at or below 0.57 once the crossfade settles.
#[test]
fn atmosphere_peak_at_max_sliders_le_0_57() {
    let library = library();
    let frames: Vec<Frame> = (0..20 * 60)
        .map(|_| Frame::centred(Vec::new(), Context::Battle))
        .collect();
    for weights in [REFERENCE_WEIGHTS, render::RAIN_WEIGHTS] {
        let scene = Scene {
            weights,
            sliders: Sliders::MAX,
            reduced: false,
            mono: false,
        };
        let out = render::render(&library, &frames, &scene);
        let level = peak(&out.left).max(peak(&out.right));
        assert!(level <= 0.57, "{weights:?}: {level}");
        assert!(level > 0.01, "{weights:?}: {level}");
    }
}
