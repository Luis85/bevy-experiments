//! Ducking and density compensation (A6-11): which cue pulls the ambience and
//! the music tracks down, by how much and for how long, and how a crowd of
//! effect voices is levelled. The runtime and the offline renderer both run
//! [`Ducker`].

use horde_audio::{
    cue::Cue,
    duck::{DuckGains, Ducker, compensated},
    mix::{self, Bus, DENSITY_FULL_VOICES, DUCK_FLOOR_DB, Duck, DuckHold, db_to_linear, duck, mix},
    select::Start,
    voices::LiveVoice,
};

fn db(gain: f32) -> f32 {
    20.0 * gain.log10()
}
fn atmosphere_db(gains: DuckGains) -> f32 {
    db(gains.duck[Bus::Atmosphere as usize])
}
fn music_db(gains: DuckGains) -> f32 {
    db(gains.duck[Bus::Music as usize])
}
fn live(cues: &[Cue]) -> Vec<LiveVoice> {
    cues.iter()
        .enumerate()
        .map(|(id, cue)| LiveVoice {
            id: id as u64,
            cue: *cue,
            started_ms: 0,
        })
        .collect()
}
/// Advances `ducker` by `ms` in 1 ms steps and returns the last gains.
fn run(ducker: &mut Ducker, ms: u32, voices: &[LiveVoice]) -> DuckGains {
    let mut gains = DuckGains::NONE;
    for _ in 0..ms {
        gains = ducker.advance(0.001, voices);
    }
    gains
}
fn close(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 0.05
}

/// The design table (design-audio.md section 6), for the cues that exist.
#[test]
fn duck_rows_match_the_design_table() {
    let stinger = Some(Duck {
        atmosphere_db: -6.0,
        music_db: -9.0,
        attack_ms: 40,
        hold: DuckHold::WhileSounding,
        release_ms: 600,
    });
    for cue in Cue::ALL {
        let expected = match cue {
            Cue::Victory | Cue::Defeat | Cue::Retreat => stinger,
            // The boss arrival, of a known kind or not.
            Cue::Boss | Cue::BossArrival => Some(Duck {
                atmosphere_db: -6.0,
                music_db: -9.0,
                attack_ms: 30,
                hold: DuckHold::Ms(1200),
                release_ms: 800,
            }),
            Cue::BossPhase => Some(Duck {
                atmosphere_db: -4.0,
                music_db: 0.0,
                attack_ms: mix::DUCK_ATTACK_MS,
                hold: DuckHold::Ms(600),
                release_ms: 500,
            }),
            Cue::Wave => Some(Duck {
                atmosphere_db: -4.0,
                music_db: 0.0,
                attack_ms: mix::DUCK_ATTACK_MS,
                hold: DuckHold::Ms(600),
                release_ms: 400,
            }),
            Cue::Explosion => Some(Duck {
                atmosphere_db: -3.0,
                music_db: 0.0,
                attack_ms: mix::DUCK_ATTACK_MS,
                hold: DuckHold::Ms(300),
                release_ms: 400,
            }),
            // The loot reveal and the coin batch (A6-14).
            Cue::LootReveal | Cue::CoinBatch => Some(Duck {
                atmosphere_db: -3.0,
                music_db: 0.0,
                attack_ms: mix::DUCK_ATTACK_MS,
                hold: DuckHold::Ms(400),
                release_ms: 400,
            }),
            _ => None,
        };
        assert_eq!(duck(cue), expected, "{cue:?}");
    }
    assert_eq!(DUCK_FLOOR_DB, -9.0);
    assert_eq!(mix::DUCK_ATTACK_MS, 30);
}

/// A boss arrival: the ambience is 6 dB down after the 30 ms attack, stays
/// there for the 1200 ms hold, and is back after the 800 ms release, half
/// way (in dB) at its middle. The music tracks go 9 dB down on the same
/// envelope.
#[test]
fn boss_arrival_ducks_atmosphere_6db_releases_800ms() {
    let mut ducker = Ducker::default();
    assert_eq!(ducker.advance(0.016, &[]), DuckGains::NONE);
    ducker.trigger(Cue::Boss);
    // Half the attack.
    let gains = run(&mut ducker, 15, &[]);
    assert!(close(atmosphere_db(gains), -3.0), "{gains:?}");
    let gains = run(&mut ducker, 15, &[]);
    assert!(close(atmosphere_db(gains), -6.0), "{gains:?}");
    assert!(close(music_db(gains), -9.0), "{gains:?}");
    // The hold: 1200 ms at the full depth.
    for _ in 0..12 {
        let gains = run(&mut ducker, 99, &[]);
        assert!(close(atmosphere_db(gains), -6.0), "{gains:?}");
    }
    run(&mut ducker, 12, &[]);
    // 400 ms into the release.
    let gains = run(&mut ducker, 400, &[]);
    assert!(close(atmosphere_db(gains), -3.0), "{gains:?}");
    assert!(close(music_db(gains), -4.5), "{gains:?}");
    // 790 ms in: nearly back, not there yet.
    let gains = run(&mut ducker, 390, &[]);
    assert!(
        atmosphere_db(gains) < -0.02 && atmosphere_db(gains) > -0.2,
        "{gains:?}"
    );
    // 800 ms after the hold the duck is over.
    let gains = run(&mut ducker, 12, &[]);
    assert_eq!(gains, DuckGains::NONE);
    assert!(!ducker.ducking());
}

/// No trigger and no crowd moves Master, Effects or UI: a duck row has no
/// column for them, and the ducker leaves their gains at exactly 1.
#[test]
fn effects_bus_never_ducks() {
    let crowd = live(&[Cue::Impact; 8]);
    for cue in Cue::ALL {
        let mut ducker = Ducker::default();
        ducker.trigger(cue);
        for step in 0..400 {
            let gains = ducker.advance(0.01, if step < 200 { &crowd } else { &[] });
            for bus in [Bus::Master, Bus::Effects, Bus::Ui] {
                assert_eq!(gains.duck[bus as usize], 1.0, "{cue:?} {bus:?}");
            }
            for bus in [Bus::Music, Bus::Atmosphere] {
                let gain = gains.duck[bus as usize];
                assert!(
                    gain <= 1.0 && gain >= db_to_linear(DUCK_FLOOR_DB) - 1e-6,
                    "{cue:?} {bus:?} {gain}"
                );
            }
        }
    }
    // Every trigger at once, in a crowd: the same.
    let mut ducker = Ducker::default();
    for cue in Cue::ALL {
        ducker.trigger(cue);
    }
    let gains = run(&mut ducker, 100, &crowd);
    assert_eq!(gains.duck[..3], [1.0; 3]);
}

/// Ducks add up in dB, down to the floor of -9 dB per bus, the density term
/// of the ambience included.
#[test]
fn summed_duck_is_floored_at_9db() {
    let mut ducker = Ducker::default();
    for cue in [Cue::Boss, Cue::Wave, Cue::Explosion] {
        ducker.trigger(cue);
    }
    let gains = run(&mut ducker, 100, &live(&[Cue::Impact; 8]));
    assert!(close(atmosphere_db(gains), DUCK_FLOOR_DB), "{gains:?}");
    assert!(atmosphere_db(gains) >= DUCK_FLOOR_DB - 1e-3);
    // Two of them alone add up: -4 and -3.
    let mut ducker = Ducker::default();
    ducker.trigger(Cue::Wave);
    ducker.trigger(Cue::Explosion);
    let gains = run(&mut ducker, 100, &[]);
    assert!(close(atmosphere_db(gains), -7.0), "{gains:?}");
    assert_eq!(gains.duck[Bus::Music as usize], 1.0);
}

/// A stinger holds its duck for as long as its voice sounds, then releases
/// over 600 ms.
#[test]
fn stinger_duck_holds_while_its_voice_sounds() {
    for cue in [Cue::Victory, Cue::Defeat, Cue::Retreat] {
        let mut ducker = Ducker::default();
        ducker.trigger(cue);
        let sounding = live(&[cue]);
        let gains = run(&mut ducker, 40, &sounding);
        assert!(close(atmosphere_db(gains), -6.0), "{cue:?} {gains:?}");
        let gains = run(&mut ducker, 3000, &sounding);
        assert!(close(atmosphere_db(gains), -6.0), "{cue:?} {gains:?}");
        assert!(close(music_db(gains), -9.0), "{cue:?} {gains:?}");
        // The voice ends.
        let gains = run(&mut ducker, 300, &[]);
        assert!(close(atmosphere_db(gains), -3.0), "{cue:?} {gains:?}");
        let gains = run(&mut ducker, 310, &[]);
        assert_eq!(gains, DuckGains::NONE, "{cue:?}");
    }
    // A stinger voice that never ends does not hold the duck for ever.
    let mut ducker = Ducker::default();
    ducker.trigger(Cue::Victory);
    let sounding = live(&[Cue::Victory]);
    let gains = run(&mut ducker, 40 + mix::DUCK_HOLD_MAX_MS + 700, &sounding);
    assert_eq!(gains, DuckGains::NONE);
}

/// A second trigger of a ducking cue restarts its hold without a jump.
#[test]
fn retrigger_restarts_the_hold_without_a_jump() {
    let mut ducker = Ducker::default();
    ducker.trigger(Cue::Explosion);
    // Attack 30, hold 300, 200 ms of the 400 ms release: half way back.
    let before = run(&mut ducker, 530, &[]);
    assert!(close(atmosphere_db(before), -1.5), "{before:?}");
    ducker.trigger(Cue::Explosion);
    let after = ducker.advance(0.001, &[]);
    assert!(
        (atmosphere_db(after) - atmosphere_db(before)).abs() < 0.2,
        "{before:?} {after:?}"
    );
    let gains = run(&mut ducker, 100, &[]);
    assert!(close(atmosphere_db(gains), -3.0), "{gains:?}");
}

/// The envelopes run on elapsed time: 30, 60 and 144 frames a second, and a
/// frame time that changes from frame to frame, give the same gains at the
/// same moments.
#[test]
fn ducking_is_display_rate_independent() {
    // Gains sampled every 100 ms for 3 s, stepping by `step(n)` seconds.
    let sampled = |step: &dyn Fn(u32) -> f32| {
        let mut ducker = Ducker::default();
        ducker.trigger(Cue::Boss);
        ducker.trigger(Cue::Explosion);
        let crowd = live(&[Cue::Impact; 6]);
        let (mut now, mut frame, mut samples) = (0.0_f64, 0, Vec::new());
        for tenth in 1..=30 {
            let until = f64::from(tenth) * 0.1;
            let mut gains = DuckGains::NONE;
            while now < until - 1e-9 {
                let dt = f64::from(step(frame)).min(until - now);
                frame += 1;
                // The crowd is there for the first second.
                gains = ducker.advance(dt as f32, if now < 1.0 { &crowd } else { &[] });
                now += dt;
            }
            samples.push(gains);
        }
        samples
    };
    let reference = sampled(&|_| 1.0 / 60.0);
    assert!(reference.iter().any(|gains| *gains != DuckGains::NONE));
    let rates: [&dyn Fn(u32) -> f32; 3] = [&|_| 1.0 / 30.0, &|_| 1.0 / 144.0, &|frame| {
        [0.004, 0.021, 0.009, 0.033][frame as usize % 4]
    }];
    for (index, step) in rates.into_iter().enumerate() {
        for (tenth, (a, b)) in sampled(step).iter().zip(&reference).enumerate() {
            for bus in 0..5 {
                assert!(
                    (a.duck[bus] - b.duck[bus]).abs() < 0.002,
                    "rate {index} at {tenth}: {a:?} is not {b:?}"
                );
            }
            assert!(
                (a.density - b.density).abs() < 0.002,
                "rate {index} at {tenth}: {a:?} is not {b:?}"
            );
        }
    }
    // A frame that never ends, or a broken clock, moves nothing wrongly.
    let mut ducker = Ducker::default();
    ducker.trigger(Cue::Boss);
    assert_eq!(ducker.advance(f32::NAN, &[]), DuckGains::NONE);
    assert_eq!(ducker.advance(-1.0, &[]), DuckGains::NONE);
}

/// N uncorrelated effect voices of one level sum to sqrt(N) times one
/// voice. Compensated, the sum stops growing at four voices: twice one
/// voice. Warnings and the player's own gun are not compensated, and neither
/// is anything outside the Effects bus.
#[test]
fn density_compensation_caps_effects_rms_growth_and_exempts_warnings() {
    assert_eq!(DENSITY_FULL_VOICES, 4);
    let mut last = 0.0;
    for voices in 1..=8_usize {
        let mut ducker = Ducker::default();
        let gains = run(&mut ducker, 400, &live(&vec![Cue::Impact; voices]));
        let sum = gains.density * (voices as f32).sqrt();
        if voices <= DENSITY_FULL_VOICES {
            assert_eq!(gains.density, 1.0, "{voices}");
        } else {
            assert!(gains.density < 1.0, "{voices}");
            assert!((sum - 2.0).abs() < 1e-3, "{voices} voices sum to {sum}");
        }
        assert!(sum >= last - 1e-3 && sum <= 2.0 + 1e-3, "{voices}: {sum}");
        last = sum;
        // The ambience makes room: -4 dB from six effect voices up.
        let expected = -4.0 * ((voices as f32 - 2.0) / 4.0).clamp(0.0, 1.0);
        assert!(
            close(atmosphere_db(gains), expected),
            "{voices}: {gains:?} is not {expected}"
        );
    }
    // Voices of other buses do not crowd the effects.
    let mut ducker = Ducker::default();
    let others = live(&[Cue::Confirm, Cue::Confirm, Cue::Confirm, Cue::Victory]);
    let mut voices = live(&[Cue::Impact; 4]);
    voices.extend(others);
    assert_eq!(run(&mut ducker, 400, &voices).density, 1.0);

    // The compensation comes in fast (10 ms) and lets go slowly (250 ms).
    let mut ducker = Ducker::default();
    let crowd = live(&[Cue::Impact; 8]);
    let target = (4.0_f32 / 8.0).sqrt();
    let gains = run(&mut ducker, 10, &crowd);
    let covered = (1.0 - gains.density) / (1.0 - target);
    assert!((covered - 0.632).abs() < 0.02, "{covered}");
    run(&mut ducker, 400, &crowd);
    let gains = run(&mut ducker, 250, &[]);
    let covered = (gains.density - target) / (1.0 - target);
    assert!((covered - 0.632).abs() < 0.02, "{covered}");

    // Who is compensated.
    for cue in Cue::ALL {
        let row = mix(cue);
        let warning = row.warning.is_some()
            || matches!(
                cue,
                Cue::Boss | Cue::BossArrival | Cue::Wave | Cue::Incoming
            );
        assert_eq!(
            compensated(Start::bus(cue)),
            row.bus == Bus::Effects && !warning,
            "{cue:?}"
        );
        assert!(!compensated(Start::player(cue)), "{cue:?}");
    }
    assert!(compensated(Start::bus(Cue::Rifle)));
    assert!(compensated(Start::bus(Cue::Impact)));
}
