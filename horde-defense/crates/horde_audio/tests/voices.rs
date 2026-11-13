//! Voice caps, stealing and the 40 ms fade (A6-09): per-bus and per-family
//! caps, which voice a start steals, and the gain ramp a stolen voice ends
//! with. The runtime and the offline renderer both decide with this code.

use horde_audio::{
    cue::Cue,
    mix::{Bus, GateGroup, Sliders, mix},
    render::VoicePolicy,
    select::Start,
    voices::{
        Admission, BUS_CAP, CONTROL_PERIOD, FADE_MS, Family, LiveVoice, Ramp, VoiceControl, admit,
        bus_cap, family,
    },
};

fn voice(id: u64, cue: Cue, started_ms: u64) -> LiveVoice {
    LiveVoice {
        id,
        cue,
        started_ms,
    }
}

/// The gains of a voice whose control says stop at sample `stop_at`, pulled
/// in blocks of `block` samples as a frame of that length would.
fn faded(rate: u32, stop_at: usize, block: usize) -> Vec<f32> {
    let control = VoiceControl::default();
    let mut ramp = Ramp::new(rate);
    let mut gains = Vec::new();
    'voice: loop {
        for _ in 0..block {
            if gains.len() == stop_at {
                control.fade_out();
            }
            match ramp.next(&control) {
                Some(gain) => gains.push(gain),
                None => break 'voice,
            }
            assert!(gains.len() < 10 * rate as usize, "the voice never ends");
        }
    }
    gains
}

#[test]
fn caps_are_the_design_table() {
    assert_eq!(BUS_CAP, [0, 8, 3, 1, 6]);
    assert_eq!(bus_cap(Bus::Effects), 8);
    assert_eq!(bus_cap(Bus::Ui), 3);
    assert_eq!(bus_cap(Bus::Music), 1);
    let caps: Vec<(Family, usize)> = Family::ALL.iter().map(|f| (*f, f.cap())).collect();
    assert_eq!(
        caps,
        vec![
            (Family::Guns, 2),
            (Family::Turret, 1),
            (Family::Death, 2),
            (Family::Impact, 2),
            (Family::Armor, 1),
            (Family::Explosion, 2),
            (Family::Molotov, 1),
            (Family::Fire, 1),
            (Family::Reload, 1),
            (Family::Radio, 1),
            (Family::Boss, 1),
            (Family::Windup, 2),
            (Family::Contact, 2),
            (Family::BossPhase, 1),
            (Family::BossDeath, 1),
        ]
    );
    for cue in Cue::ALL {
        // The fire cues are exactly the gun and turret families.
        let fires = mix(cue).gate == GateGroup::Fire;
        let gun = matches!(family(cue), Some(Family::Guns | Family::Turret));
        assert_eq!(fires, gun, "{cue:?}");
        // A family lives on one bus, so a family steal stays on its bus.
        if family(cue).is_some() {
            assert_eq!(mix(cue).bus, Bus::Effects, "{cue:?}");
        }
    }
    // The radio warnings share one voice.
    assert_eq!(family(Cue::Wave), family(Cue::Incoming));
    assert_eq!(family(Cue::Confirm), None);
    // Only gunfire and impacts take an equal.
    let equals: Vec<Family> = Family::ALL
        .into_iter()
        .filter(|f| f.steals_equals())
        .collect();
    assert_eq!(equals, vec![Family::Guns, Family::Turret, Family::Impact]);
}

/// A start into a full family steals that family's oldest voice, whatever
/// else sounds and however important the rest is.
#[test]
fn family_cap_steals_own_oldest() {
    // Two guns sound (the cap), the pistol older than the rifle.
    let live = [
        voice(1, Cue::Damage, 0),
        voice(2, Cue::Rifle, 50),
        voice(3, Cue::Pistol, 20),
        voice(4, Cue::Fire, 5),
    ];
    assert_eq!(admit(Start::bus(Cue::Smg), &live), Admission::Steal(3));
    assert_eq!(admit(Start::player(Cue::Rifle), &live), Admission::Steal(3));
    // Below the cap the gun takes a free voice.
    assert_eq!(admit(Start::bus(Cue::Smg), &live[..2]), Admission::Free);
    // A one-voice family replaces its only voice; Wave and Incoming share it.
    let radio = [voice(7, Cue::Incoming, 0), voice(8, Cue::Kill, 1)];
    assert_eq!(admit(Start::bus(Cue::Wave), &radio), Admission::Steal(7));
    // A cue without a family is capped by its bus alone.
    let traps: Vec<LiveVoice> = (0..5).map(|n| voice(n, Cue::Trap, n)).collect();
    assert_eq!(admit(Start::bus(Cue::Trap), &traps), Admission::Free);
    // Equal start times: the lower voice id is the older.
    let same = [voice(9, Cue::Kill, 10), voice(4, Cue::Kill, 10)];
    assert_eq!(admit(Start::bus(Cue::Kill), &same), Admission::Steal(4));
}

/// At a full bus the least important voice goes, the oldest of the equally
/// unimportant ones first; an equal is taken only by gunfire and impacts;
/// and the victim fades for 40 ms, sample-exact at any frame rate.
#[test]
fn steal_prefers_oldest_among_equal_priority_and_fades_40ms() {
    // Eight effects: six important ones and two of priority 7.
    let mut live = vec![
        voice(10, Cue::Explosion, 0),
        voice(11, Cue::Explosion, 1),
        voice(12, Cue::Structure, 2),
        voice(13, Cue::Structure, 3),
        voice(14, Cue::Damage, 4),
        voice(15, Cue::Damage, 5),
        voice(16, Cue::Impact, 300),
        voice(17, Cue::Armor, 200),
    ];
    assert_eq!(live.len(), bus_cap(Bus::Effects));
    assert_eq!(mix(Cue::Impact).priority, mix(Cue::Armor).priority);
    // A more important start steals the oldest of the two least important.
    assert_eq!(admit(Start::bus(Cue::Melee), &live), Admission::Steal(17));
    // An impact is as important as they are and still steals the oldest.
    assert_eq!(admit(Start::bus(Cue::Impact), &live), Admission::Steal(17));
    // The order of the list does not matter, the start time does.
    live.swap(6, 7);
    assert_eq!(admit(Start::bus(Cue::Impact), &live), Admission::Steal(17));
    live[6].started_ms = 400;
    assert_eq!(admit(Start::bus(Cue::Impact), &live), Admission::Steal(16));
    // A less important start is refused.
    assert_eq!(admit(Start::bus(Cue::Trap), &live), Admission::Refuse);
    // An equally important start outside gunfire and impacts is refused.
    let important: Vec<LiveVoice> = (0..8).map(|n| voice(n, Cue::Structure, n)).collect();
    assert_eq!(
        admit(Start::bus(Cue::Structure), &important),
        Admission::Refuse
    );
    // Six more important voices and two guns, the gun family's cap: a turret
    // shot is the least important of all and is refused; the player's own
    // shot takes the oldest gun.
    let mut crowd: Vec<LiveVoice> = (10..16).map(|n| voice(n, Cue::Damage, n)).collect();
    crowd.extend([voice(1, Cue::Rifle, 9), voice(2, Cue::Smg, 3)]);
    assert!(mix(Cue::TurretShot).priority > mix(Cue::Rifle).priority);
    assert_eq!(
        admit(Start::bus(Cue::TurretShot), &crowd),
        Admission::Refuse
    );
    assert_eq!(
        admit(Start::player(Cue::Rifle), &crowd),
        Admission::Steal(2)
    );
    // Seven more important voices and one gun: a bus shot takes the gun, its
    // equal.
    let mut one_gun: Vec<LiveVoice> = (10..17).map(|n| voice(n, Cue::Damage, n)).collect();
    one_gun.push(voice(1, Cue::Rifle, 9));
    assert_eq!(
        admit(Start::bus(Cue::Pistol), &one_gun),
        Admission::Steal(1)
    );

    // The fade: 40 ms from the stop, then the voice ends.
    assert_eq!(FADE_MS, 40);
    for rate in [44_100_u32, 22_050, 48_000] {
        let fade = (rate * FADE_MS / 1000) as usize;
        let stop_at = 3 * CONTROL_PERIOD as usize;
        let gains = faded(rate, stop_at, 1 << 20);
        // The stop lands on a control read here, so the fade is exact.
        assert_eq!(gains.len(), stop_at + fade, "{rate} Hz");
        assert!(gains[..stop_at].iter().all(|gain| *gain == 1.0));
        // It is down 20 dB after half the fade and inaudible at its end.
        assert!(gains[stop_at + fade / 2] < 0.1, "{rate} Hz");
        assert!(*gains.last().unwrap() < 0.01, "{rate} Hz");
        // A stop between two control reads is seen at the next one.
        let late = faded(rate, stop_at + 1, 1 << 20);
        assert_eq!(late.len(), stop_at + CONTROL_PERIOD as usize + fade);
    }
    // The fade runs on samples: a 30 FPS and a 144 FPS frame loop get the
    // same gains, bit for bit.
    let rate = 44_100;
    let whole = faded(rate, 5_000, 1 << 20);
    assert_eq!(faded(rate, 5_000, rate as usize / 30), whole);
    assert_eq!(faded(rate, 5_000, rate as usize / 144), whole);
}

/// Each bus counts and steals among its own voices.
#[test]
fn ui_and_music_never_stolen_by_effects() {
    // Eight effects, two interface sounds and a stinger sound.
    let mut live: Vec<LiveVoice> = (0..8).map(|n| voice(n, Cue::Trap, 100 + n)).collect();
    live.push(voice(20, Cue::Confirm, 0));
    live.push(voice(21, Cue::Confirm, 1));
    live.push(voice(22, Cue::Victory, 2));
    for cue in Cue::ALL {
        if mix(cue).bus != Bus::Effects {
            continue;
        }
        if let Admission::Steal(victim) = admit(Start::bus(cue), &live) {
            assert!(victim < 8, "{cue:?} stole voice {victim}");
        }
    }
    // The most important effect steals an effect, never the older stinger.
    assert_eq!(admit(Start::bus(Cue::Boss), &live), Admission::Steal(0));
    // The other buses have room of their own while Effects is full.
    assert_eq!(admit(Start::bus(Cue::Confirm), &live), Admission::Free);
    // A second stinger is as important as the first: refused, not stolen.
    assert_eq!(admit(Start::bus(Cue::Defeat), &live), Admission::Refuse);
    // The third interface sound fills the UI bus; a fourth is refused.
    live.push(voice(23, Cue::Confirm, 3));
    assert_eq!(admit(Start::bus(Cue::Confirm), &live), Admission::Refuse);
    // Without the effects the UI and Music answers are the same.
    assert_eq!(
        admit(Start::bus(Cue::Confirm), &live[8..]),
        Admission::Refuse
    );
    assert_eq!(admit(Start::bus(Cue::Boss), &live[8..]), Admission::Free);
}

/// The ramp never jumps: not when a fade starts, not when the voice ends,
/// not when the target moves.
#[test]
fn voice_control_fade_has_no_step_over_0_1() {
    for rate in [22_050_u32, 44_100, 48_000] {
        let mut gains = faded(rate, 1_000, 1 << 20);
        // The end of the voice is a step to silence.
        gains.push(0.0);
        let step = gains
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0_f32, f32::max);
        assert!(step <= 0.1, "{rate} Hz: step {step}");
        assert!(step > 0.0);
        // A target change ramps too, and settles on the target.
        let control = VoiceControl::default();
        let mut ramp = Ramp::new(rate);
        let mut last = ramp.next(&control).unwrap();
        control.set_target(0.25);
        for _ in 0..rate / 5 {
            let gain = ramp.next(&control).unwrap();
            assert!((gain - last).abs() <= 0.1);
            last = gain;
        }
        assert!((last - 0.25).abs() < 0.001, "{rate} Hz: {last}");
        assert!(!control.stopping());
    }
    // A bad target is silence, not noise.
    let control = VoiceControl::default();
    control.set_target(f32::NAN);
    assert_eq!(control.target(), 0.0);
    control.set_target(7.0);
    assert_eq!(control.target(), 1.0);
}

/// The policy end to end: a start at a cap fades its victim, and the victim
/// gives up its slot at once.
#[test]
fn policy_fades_the_stolen_voice_and_frees_its_slot() {
    let audible = Sliders::MAX.bus_gains();
    let live = [voice(100, Cue::Pistol, 0), voice(101, Cue::Rifle, 30)];
    let mut policy = VoicePolicy::default();
    let actions = policy.frame(vec![Cue::Smg], 60, false, &audible, &live);
    assert_eq!(actions.fades, vec![100]);
    assert_eq!(actions.starts.len(), 1);
    assert_eq!(actions.starts[0].cue, Cue::Smg);
    // A refused start does not spend its cooldown.
    let full: Vec<LiveVoice> = (0..8).map(|n| voice(n, Cue::Structure, n)).collect();
    let mut policy = VoicePolicy::default();
    let refused = policy.frame(vec![Cue::Trap], 0, false, &audible, &full);
    assert!(refused.starts.is_empty() && refused.fades.is_empty());
    let later = policy.frame(vec![Cue::Trap], 17, false, &audible, &[]);
    assert_eq!(later.starts.len(), 1);
    // Equals of the live voices, outside gunfire and impacts: refused.
    let mut policy = VoicePolicy::default();
    let cues = vec![Cue::Explosion, Cue::Molotov];
    let none = policy.frame(cues.clone(), 0, false, &audible, &full);
    assert!(none.starts.is_empty() && none.fades.is_empty(), "{none:?}");
    // Two starts in one frame fade two different voices: the second start
    // sees the first's victim gone and its slot taken.
    let traps: Vec<LiveVoice> = (0..8).map(|n| voice(n, Cue::Trap, n)).collect();
    let two = policy.frame(cues, 20, false, &audible, &traps);
    assert_eq!(two.fades, vec![0, 1]);
    assert_eq!(two.starts.len(), 2);
}
