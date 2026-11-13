//! The bus split (A6-06): Master, Effects, UI, Music and Atmosphere. A cue's
//! mix row names its bus, and a voice plays at its own bus's gain: interface
//! sounds follow the Interface slider, the attempt stingers follow "Music &
//! stingers", and no slider but Master and Combat effects reaches a combat
//! cue, warnings included.

use horde_audio::{
    cue::Cue,
    mix::{self, Bus, Sliders, mix},
    render::{self, Context, Frame, LegacyPolicy, Library, REFERENCE_WEIGHTS, Scene},
};
use std::path::Path;

const UI_CUES: [Cue; 1] = [Cue::Confirm];
const STINGERS: [Cue; 3] = [Cue::Victory, Cue::Defeat, Cue::Retreat];
/// Cues that announce danger. They are combat cues, so they ride Effects.
const WARNINGS: [Cue; 5] = [
    Cue::Boss,
    Cue::Wave,
    Cue::Incoming,
    Cue::Structure,
    Cue::Damage,
];

fn library() -> Library {
    Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
        .expect("the shipped audio loads")
}
/// Two seconds of battle with `cue` requested in the first frame, rendered
/// without ambience. Returns the sample peak.
fn peak_of(library: &Library, cue: Cue, sliders: Sliders) -> f32 {
    let mut frames: Vec<Frame> = (0..120)
        .map(|_| Frame::centred(Vec::new(), Context::Battle))
        .collect();
    frames[0].cues.push(cue);
    let scene = Scene {
        weights: REFERENCE_WEIGHTS,
        sliders: Sliders {
            ambience: 0.0,
            ..sliders
        },
        reduced: false,
        mono: false,
    };
    render::render(library, &frames, &scene)
        .left
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
}

#[test]
fn bus_gains_multiply_master_slider_and_calibration() {
    let sliders = Sliders {
        master: 0.5,
        effects: 0.8,
        ui: 0.6,
        music: 0.4,
        ambience: 0.2,
    };
    let calibration = mix::calibration_gain();
    assert_eq!(sliders.bus_gain(Bus::Master), 0.5);
    assert_eq!(sliders.bus_gain(Bus::Effects), 0.5 * 0.8 * calibration);
    assert_eq!(sliders.bus_gain(Bus::Ui), 0.5 * 0.6 * calibration);
    assert_eq!(sliders.bus_gain(Bus::Music), 0.5 * 0.4 * calibration);
    assert_eq!(
        sliders.bus_gain(Bus::Atmosphere),
        0.5 * 0.2 * mix::ambience_calibration_gain()
    );
    // The table form is indexed by the bus.
    let gains = sliders.bus_gains();
    assert_eq!(Bus::ALL.len(), gains.len());
    for bus in Bus::ALL {
        assert_eq!(gains[bus as usize], sliders.bus_gain(bus), "{bus:?}");
    }
    // Sliders are clamped to 0..=1 and a non-finite one is silence.
    let wild = Sliders {
        master: 9.0,
        effects: f32::NAN,
        ui: -1.0,
        music: 9.0,
        ambience: f32::INFINITY,
    };
    assert_eq!(wild.bus_gain(Bus::Master), 1.0);
    assert_eq!(wild.bus_gain(Bus::Effects), 0.0);
    assert_eq!(wild.bus_gain(Bus::Ui), 0.0);
    assert_eq!(wild.bus_gain(Bus::Music), calibration);
    assert_eq!(wild.bus_gain(Bus::Atmosphere), 0.0);
    // The new sliders default to the level the cues had on the effects bus,
    // so the split changes nothing at the defaults.
    assert_eq!(mix::DEFAULT_UI, mix::DEFAULT_EFFECTS);
    assert_eq!(mix::DEFAULT_MUSIC, mix::DEFAULT_EFFECTS);
    assert_eq!(
        Sliders::DEFAULT,
        Sliders {
            master: mix::DEFAULT_MASTER,
            effects: mix::DEFAULT_EFFECTS,
            ui: mix::DEFAULT_UI,
            music: mix::DEFAULT_MUSIC,
            ambience: mix::DEFAULT_AMBIENCE,
        }
    );
}

#[test]
fn every_cue_rides_exactly_one_voice_bus() {
    for cue in Cue::ALL {
        let expected = if UI_CUES.contains(&cue) {
            Bus::Ui
        } else if STINGERS.contains(&cue) {
            Bus::Music
        } else {
            Bus::Effects
        };
        assert_eq!(mix(cue).bus, expected, "{cue:?}");
    }
}

#[test]
fn ui_cues_follow_ui_volume_not_effects() {
    let library = library();
    for cue in UI_CUES {
        assert_eq!(mix(cue).bus, Bus::Ui, "{cue:?}");
        let at = |sliders| peak_of(&library, cue, sliders);
        let full = at(Sliders::MAX);
        assert!(full > 0.0, "{cue:?}");
        // The effects and music sliders do not reach it.
        let other_buses_muted = at(Sliders {
            effects: 0.0,
            music: 0.0,
            ..Sliders::MAX
        });
        assert_eq!(other_buses_muted, full, "{cue:?}");
        // The interface slider scales it and mutes it.
        let half = at(Sliders {
            ui: 0.5,
            ..Sliders::MAX
        });
        assert!((half - 0.5 * full).abs() < 1e-6, "{cue:?} {half} {full}");
        let muted = at(Sliders {
            ui: 0.0,
            ..Sliders::MAX
        });
        assert_eq!(muted, 0.0, "{cue:?}");
        // Master still reaches it.
        let no_master = at(Sliders {
            master: 0.0,
            ..Sliders::MAX
        });
        assert_eq!(no_master, 0.0, "{cue:?}");
    }
}

#[test]
fn stingers_follow_music_bus_and_warnings_do_not() {
    let library = library();
    let music_muted = Sliders {
        music: 0.0,
        ui: 0.0,
        ..Sliders::MAX
    };
    let effects_muted = Sliders {
        effects: 0.0,
        ui: 0.0,
        ..Sliders::MAX
    };
    for cue in STINGERS {
        assert_eq!(mix(cue).bus, Bus::Music, "{cue:?}");
        let full = peak_of(&library, cue, Sliders::MAX);
        assert!(full > 0.0, "{cue:?}");
        assert_eq!(peak_of(&library, cue, music_muted), 0.0, "{cue:?}");
        assert_eq!(peak_of(&library, cue, effects_muted), full, "{cue:?}");
        let half = peak_of(
            &library,
            cue,
            Sliders {
                music: 0.5,
                ..Sliders::MAX
            },
        );
        assert!((half - 0.5 * full).abs() < 1e-6, "{cue:?} {half} {full}");
    }
    // A danger cue is a combat cue: only Master and Combat effects mute it.
    let warnings = Cue::ALL
        .into_iter()
        .filter(|cue| WARNINGS.contains(cue) || mix(*cue).warning.is_some());
    for cue in warnings {
        assert_eq!(mix(cue).bus, Bus::Effects, "{cue:?}");
        let full = peak_of(&library, cue, Sliders::MAX);
        assert!(full > 0.0, "{cue:?}");
        assert_eq!(peak_of(&library, cue, music_muted), full, "{cue:?}");
        assert_eq!(peak_of(&library, cue, effects_muted), 0.0, "{cue:?}");
    }
}

/// A cue on a muted bus is dropped before selection: it takes no start slot
/// from an audible cue, starts no voice and leaves its cooldown untouched.
#[test]
fn a_muted_bus_drops_its_cues_before_selection() {
    let ui_muted = Sliders {
        ui: 0.0,
        ..Sliders::MAX
    }
    .bus_gains();
    let mut policy = LegacyPolicy::default();
    // Confirm (priority 3) and Damage (3) would take both slots from Kill.
    let requests = vec![Cue::Confirm, Cue::Damage, Cue::Kill];
    let actions = policy.frame(requests, 0, false, &ui_muted, &[]);
    assert_eq!(
        actions.starts.iter().map(|s| s.cue).collect::<Vec<_>>(),
        vec![Cue::Damage, Cue::Kill]
    );
    // The muted request did not start the cue's cooldown.
    let audible = Sliders::MAX.bus_gains();
    let next = policy.frame(vec![Cue::Confirm], 17, false, &audible, &[]);
    assert_eq!(
        next.starts.iter().map(|s| s.cue).collect::<Vec<_>>(),
        vec![Cue::Confirm]
    );
    // Every bus muted: nothing starts, as before the split.
    let silent = Sliders {
        master: 0.0,
        ..Sliders::MAX
    }
    .bus_gains();
    let none = LegacyPolicy::default().frame(vec![Cue::Boss, Cue::Confirm], 0, false, &silent, &[]);
    assert!(none.starts.is_empty() && none.stops.is_empty());
}
