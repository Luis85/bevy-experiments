//! The music director (A6-15): which music state a screen and a battle ask
//! for, the hysteresis between the two battle intensities, the track slots
//! (every one deferred: no music files ship), the crossfade envelope a track
//! file would play on, and the stingers, which play existing banks on the
//! Music bus.

use horde_audio::{
    bank::BankId,
    cue::Cue,
    mix::{Bus, Sliders, mix},
    music::{
        BattleInput, CROSSFADE_MS, DEFERRED_REASON, HIGH_HOLD_MS, LOW_HOLD_MS, MusicDirector,
        MusicInput, MusicScreen, MusicState, PAUSED_GAIN, STINGERS, TRACKS, Track, intensity,
        track,
    },
    render::{self, Context, Frame, Library, REFERENCE_WEIGHTS, Scene},
};
use std::path::Path;

fn assets() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}
fn calm() -> BattleInput {
    BattleInput {
        enemies_alive: 0,
        nearest_x_frac: 1.0,
        hp_frac: 1.0,
        boss_alive: false,
        paused: false,
    }
}
/// Intensity 0.45 + 0.35 + 0.1 = 0.9.
fn fierce() -> BattleInput {
    BattleInput {
        enemies_alive: 40,
        nearest_x_frac: 0.0,
        hp_frac: 0.5,
        ..calm()
    }
}
/// Intensity 0.45 * 0.5 + 0.35 * 0.5 + 0.2 * 0.5 = 0.5: between the two
/// thresholds.
fn middling() -> BattleInput {
    BattleInput {
        enemies_alive: 20,
        nearest_x_frac: 0.5,
        hp_frac: 0.5,
        ..calm()
    }
}
fn on(screen: MusicScreen) -> MusicInput {
    MusicInput {
        screen,
        battle: None,
    }
}
fn battle(input: BattleInput) -> MusicInput {
    MusicInput {
        screen: MusicScreen::Battle,
        battle: Some(input),
    }
}
/// Runs `input` for `ms` in 50 ms frames and returns the state.
fn run(director: &mut MusicDirector, input: MusicInput, ms: u32) -> MusicState {
    for _ in 0..ms / 50 {
        director.update(&input, 50);
    }
    director.state()
}

#[test]
fn music_director_matrix() {
    // The screens: Briefing is Camp, Results and Upgrades are Results.
    for (screen, state) in [
        (MusicScreen::Menu, MusicState::Menu),
        (MusicScreen::Camp, MusicState::Camp),
        (MusicScreen::Briefing, MusicState::Camp),
        (MusicScreen::Results, MusicState::Results),
        (MusicScreen::Upgrades, MusicState::Results),
    ] {
        let mut director = MusicDirector::default();
        assert_eq!(director.state(), MusicState::Silent);
        assert_eq!(director.update(&on(screen), 16), state, "{screen:?}");
    }
    // An overlay (Settings, History) keeps what was playing.
    let mut director = MusicDirector::default();
    director.update(&on(MusicScreen::Camp), 16);
    assert_eq!(
        director.update(&on(MusicScreen::Overlay), 16),
        MusicState::Camp
    );
    // The battle screen with no battle has nothing to score.
    assert_eq!(
        director.update(&on(MusicScreen::Battle), 16),
        MusicState::Silent
    );

    // The intensity formula.
    assert_eq!(intensity(&calm()), 0.0);
    assert!((intensity(&fierce()) - 0.9).abs() < 1e-6);
    assert!((intensity(&middling()) - 0.5).abs() < 1e-6);
    let crowd = BattleInput {
        enemies_alive: 400,
        ..calm()
    };
    assert!((intensity(&crowd) - 0.45).abs() < 1e-6, "capped at 40");
    let wild = BattleInput {
        nearest_x_frac: f32::NAN,
        hp_frac: -3.0,
        ..calm()
    };
    assert!((0.0..=1.0).contains(&intensity(&wild)));

    // A battle starts low whatever its intensity, and goes high only after
    // the intensity held at 0.60 or more for 2 s.
    let mut director = MusicDirector::default();
    assert_eq!(
        director.update(&battle(fierce()), 16),
        MusicState::BattleLow
    );
    assert_eq!(
        run(&mut director, battle(fierce()), HIGH_HOLD_MS - 100),
        MusicState::BattleLow
    );
    // A dip resets the hold.
    director.update(&battle(middling()), 50);
    assert_eq!(
        run(&mut director, battle(fierce()), HIGH_HOLD_MS - 100),
        MusicState::BattleLow
    );
    assert_eq!(
        run(&mut director, battle(fierce()), 200),
        MusicState::BattleHigh
    );
    // Between the thresholds it stays high for good.
    assert_eq!(
        run(&mut director, battle(middling()), 3 * LOW_HOLD_MS),
        MusicState::BattleHigh
    );
    // At 0.35 or less it returns only after 4 s, and a surge resets that.
    assert_eq!(
        run(&mut director, battle(calm()), LOW_HOLD_MS - 100),
        MusicState::BattleHigh
    );
    director.update(&battle(middling()), 50);
    assert_eq!(
        run(&mut director, battle(calm()), LOW_HOLD_MS - 100),
        MusicState::BattleHigh
    );
    assert_eq!(
        run(&mut director, battle(calm()), 200),
        MusicState::BattleLow
    );
    // Between the thresholds it stays low for good.
    assert_eq!(
        run(&mut director, battle(middling()), 3 * HIGH_HOLD_MS),
        MusicState::BattleLow
    );

    // A living boss is Boss at once, from either intensity; its death
    // returns to the low state at once.
    let boss = BattleInput {
        boss_alive: true,
        ..calm()
    };
    assert_eq!(director.update(&battle(boss), 16), MusicState::Boss);
    assert_eq!(
        director.update(&battle(fierce()), 16),
        MusicState::BattleLow
    );
    run(&mut director, battle(fierce()), HIGH_HOLD_MS + 100);
    assert_eq!(director.state(), MusicState::BattleHigh);
    assert_eq!(director.update(&battle(boss), 16), MusicState::Boss);

    // A pause keeps the state, stops the hold clocks and lowers the gain.
    let mut director = MusicDirector::default();
    director.update(&battle(fierce()), 16);
    assert_eq!(director.context_gain(), 1.0);
    let paused = BattleInput {
        paused: true,
        ..fierce()
    };
    assert_eq!(
        run(&mut director, battle(paused), 3 * HIGH_HOLD_MS),
        MusicState::BattleLow
    );
    assert_eq!(director.context_gain(), PAUSED_GAIN);
    assert_eq!(PAUSED_GAIN, 0.35);
    // Leaving the battle is immediate, and a new battle starts low again.
    run(&mut director, battle(fierce()), HIGH_HOLD_MS + 100);
    assert_eq!(director.state(), MusicState::BattleHigh);
    assert_eq!(
        director.update(&on(MusicScreen::Results), 16),
        MusicState::Results
    );
    assert_eq!(director.context_gain(), 1.0);
    assert_eq!(
        director.update(&battle(fierce()), 16),
        MusicState::BattleLow
    );
}

#[test]
fn every_music_state_resolves_to_file_or_deferred() {
    assert_eq!(TRACKS.len(), MusicState::ALL.len());
    for state in MusicState::ALL {
        let slots = TRACKS.iter().filter(|(slot, _)| *slot == state).count();
        assert_eq!(slots, 1, "{state:?}");
        match track(state) {
            // No music files ship (decision 9): every slot says why.
            Track::Deferred(reason) => assert_eq!(reason, DEFERRED_REASON, "{state:?}"),
            Track::File(path) => panic!("{state:?} names a music file: {path}"),
        }
    }
    assert!(DEFERRED_REASON.contains("deferred") && DEFERRED_REASON.contains("decision 9"));
    // With every slot deferred the director plays no track in any state, on
    // any transition.
    let mut director = MusicDirector::default();
    for screen in [
        MusicScreen::Menu,
        MusicScreen::Camp,
        MusicScreen::Results,
        MusicScreen::Menu,
    ] {
        for _ in 0..10 {
            director.update(&on(screen), 300);
            assert!(director.voices(&TRACKS).is_empty(), "{screen:?}");
        }
    }
    // No music ships under assets/audio either.
    for entry in std::fs::read_dir(assets().join("audio")).expect("assets/audio") {
        let name = entry.expect("entry").file_name();
        let name = name.to_string_lossy().to_lowercase();
        assert!(!name.contains("music") && !name.contains("track"), "{name}");
    }
}

/// `Track::File` playback: a state change crossfades the two files over 2 s
/// on a smoothstep whose gains always sum to 1.
#[test]
fn track_files_crossfade_on_a_two_second_smoothstep() {
    let mut files = TRACKS;
    for (state, slot) in &mut files {
        *slot = match state {
            MusicState::Menu => Track::File("audio/test_menu.wav"),
            MusicState::Camp => Track::File("audio/test_camp.wav"),
            _ => *slot,
        };
    }
    assert_eq!(CROSSFADE_MS, 2000);
    let mut director = MusicDirector::default();
    // From silence the first track fades in alone.
    director.update(&on(MusicScreen::Menu), 0);
    let voices = director.voices(&files);
    assert_eq!(voices.len(), 1);
    assert_eq!(
        (voices[0].path, voices[0].gain),
        ("audio/test_menu.wav", 0.0)
    );
    director.update(&on(MusicScreen::Menu), CROSSFADE_MS / 2);
    assert!((director.voices(&files)[0].gain - 0.5).abs() < 1e-6);
    director.update(&on(MusicScreen::Menu), CROSSFADE_MS);
    let voices = director.voices(&files);
    assert_eq!(voices.len(), 1);
    assert_eq!(voices[0].gain, 1.0);

    // Menu to Camp: both play, and the gains sum to 1 all the way.
    director.update(&on(MusicScreen::Camp), 0);
    let mut last_in = -1.0;
    for step in 0..=20 {
        let voices = director.voices(&files);
        let gain = |path: &str| {
            voices
                .iter()
                .find(|voice| voice.path == path)
                .map_or(0.0, |voice| voice.gain)
        };
        let (out, into) = (gain("audio/test_menu.wav"), gain("audio/test_camp.wav"));
        assert!((out + into - 1.0).abs() < 1e-6, "step {step}: {voices:?}");
        assert!(into >= last_in, "step {step}: the fade is monotonic");
        last_in = into;
        if step == 5 {
            // A quarter of the way: smoothstep(0.25) = 0.15625.
            assert!((into - 0.156_25).abs() < 1e-5, "{into}");
        }
        director.update(&on(MusicScreen::Camp), CROSSFADE_MS / 20);
    }
    let voices = director.voices(&files);
    assert_eq!(voices.len(), 1, "the old track ends: {voices:?}");
    assert_eq!(
        (voices[0].path, voices[0].gain),
        ("audio/test_camp.wav", 1.0)
    );

    // To a deferred slot: the file fades out and nothing fades in.
    director.update(&on(MusicScreen::Results), CROSSFADE_MS / 2);
    let voices = director.voices(&files);
    assert_eq!(voices.len(), 1);
    assert!((voices[0].gain - 0.5).abs() < 1e-6);
    director.update(&on(MusicScreen::Results), CROSSFADE_MS);
    assert!(director.voices(&files).is_empty());

    // A paused battle plays its track at the paused context gain.
    for (state, slot) in &mut files {
        if *state == MusicState::BattleLow {
            *slot = Track::File("audio/test_battle.wav");
        }
    }
    let paused = BattleInput {
        enemies_alive: 0,
        nearest_x_frac: 1.0,
        hp_frac: 1.0,
        boss_alive: false,
        paused: true,
    };
    let mut director = MusicDirector::default();
    director.update(&battle(paused), 0);
    director.update(&battle(paused), CROSSFADE_MS);
    assert_eq!(director.voices(&files)[0].gain, PAUSED_GAIN);
}

#[test]
fn every_stinger_resolves_to_existing_bank() {
    let shipped = BankId::shipped();
    let assets = assets();
    for cue in STINGERS {
        assert!(!cue.retired(), "{cue:?}");
        assert_eq!(mix(cue).bus, Bus::Music, "{cue:?}");
        let bank = cue.bank();
        assert!(shipped.contains(&bank), "{cue:?} plays an unshipped bank");
        assert!(!bank.retired(), "{cue:?}");
        // An existing bank: one of the v1 survival families, not a bank made
        // for music.
        assert_eq!(bank, cue.legacy_family().bank(), "{cue:?}");
        assert!(bank.variants() > 0, "{cue:?}");
        for variant in 0..bank.variants() {
            let path = bank.variant_path(variant);
            assert!(path.starts_with("audio/survival_"), "{path}");
            assert!(assets.join(&path).is_file(), "{cue:?}: missing {path}");
        }
    }
    // The stingers are the whole Music bus: no other cue rides it.
    for cue in Cue::ALL {
        assert_eq!(
            mix(cue).bus == Bus::Music,
            STINGERS.contains(&cue),
            "{cue:?}"
        );
    }
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
fn music_volume_scales_stingers_only() {
    let library = Library::load(&assets()).expect("the shipped audio loads");
    let half = Sliders {
        music: 0.5,
        ..Sliders::MAX
    };
    // The stingers follow the slider.
    for cue in STINGERS {
        let full = peak_of(&library, cue, Sliders::MAX);
        assert!(full > 0.0, "{cue:?}");
        let halved = peak_of(&library, cue, half);
        assert!((halved - 0.5 * full).abs() < 1e-6, "{cue:?}");
    }
    // Nothing else does: no other bus's gain moves, so no other cue and no
    // ambience loop changes level.
    for bus in Bus::ALL {
        let moved = half.bus_gain(bus) != Sliders::MAX.bus_gain(bus);
        assert_eq!(moved, bus == Bus::Music, "{bus:?}");
    }
    for cue in [
        Cue::Rifle,
        Cue::Boss,
        Cue::Wave,
        Cue::Confirm,
        Cue::CoinBatch,
    ] {
        assert!(!STINGERS.contains(&cue));
        let full = peak_of(&library, cue, Sliders::MAX);
        assert!(full > 0.0, "{cue:?}");
        assert_eq!(peak_of(&library, cue, half), full, "{cue:?}");
    }
    // And there is no track for it to scale: every slot is deferred.
    assert!(
        TRACKS
            .iter()
            .all(|(_, slot)| matches!(slot, Track::Deferred(_)))
    );
}
