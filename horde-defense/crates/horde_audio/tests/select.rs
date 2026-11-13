//! Cue selection (A6-08): the per-frame start budget of each bus, the
//! reserved player-weapon lane and the repeat gate. The v1 cases (important
//! cues are not displaced by fire, fire sources take turns, stingers and
//! kills replace what they subsume) are re-expressed on the budgets. The
//! runtime and the offline renderer both decide with this code.

use horde_audio::{
    cue::Cue,
    events,
    mix::{Bus, Sliders, mix},
    pan::Places,
    render::{self, VoicePolicy},
    select::{
        CueGate, Lane, START_BUDGET, SUBSUMES, Start, select_sounds, select_starts, start_budget,
    },
    voices::LiveVoice,
};
use horde_core::{
    content::{Mode, StructureKind},
    sim::{Actor, SimEvent},
    weapons::WeaponClass,
};

/// The reserved lane: the root defender's own shot starts whatever else the
/// frame holds, and takes no slot of the Effects budget.
#[test]
fn player_shot_starts_even_with_two_critical_cues_in_frame() {
    // Two critical cues and the player's rifle: all three start.
    assert_eq!(
        select_starts(
            vec![Cue::Boss, Cue::Damage, Cue::Rifle],
            Some(Cue::Rifle),
            0
        ),
        vec![
            Start::bus(Cue::Boss),
            Start::bus(Cue::Damage),
            Start::player(Cue::Rifle)
        ]
    );
    // Every budget full of critical cues: the shot still starts, on its lane.
    let crowded = vec![
        Cue::Boss,
        Cue::Explosion,
        Cue::Molotov,
        Cue::Structure,
        Cue::Victory,
        Cue::Confirm,
        Cue::Rifle,
    ];
    assert!(
        crowded
            .iter()
            .all(|cue| *cue == Cue::Rifle || mix(*cue).priority <= 3)
    );
    let starts = select_starts(crowded.clone(), Some(Cue::Rifle), 0);
    assert_eq!(
        starts,
        vec![
            Start::bus(Cue::Boss),
            Start::bus(Cue::Victory),
            Start::bus(Cue::Explosion),
            Start::bus(Cue::Molotov),
            Start::bus(Cue::Confirm),
            Start::player(Cue::Rifle),
        ]
    );
    // The lane holds exactly the player's shot; the bus starts are the same
    // with or without it, and somebody else's rifle in the same frame waits.
    assert_eq!(
        starts
            .iter()
            .filter(|start| start.lane == Lane::PlayerWeapon)
            .count(),
        1
    );
    let without = select_starts(crowded, None, 0);
    assert_eq!(without, starts[..5]);
    assert!(!without.iter().any(|start| start.cue == Cue::Rifle));
    // A shot that reaches selection only through the lane starts too, for
    // every weapon class and at any rotation.
    for class in [
        Cue::Pistol,
        Cue::Smg,
        Cue::Shotgun,
        Cue::Marksman,
        Cue::MachineGun,
    ] {
        let frame = vec![Cue::Boss, Cue::Explosion, Cue::Structure];
        let starts = select_starts(frame, Some(class), 7);
        assert_eq!(starts.len(), 4, "{class:?}");
        assert_eq!(starts[3], Start::player(class));
    }
    // The player's own shot is one start, never a second one on the bus.
    assert_eq!(
        select_starts(vec![Cue::Smg, Cue::Smg], Some(Cue::Smg), 3),
        vec![Start::player(Cue::Smg)]
    );
}

/// Effects 3, UI 2, Music 1 per frame; each bus counts its own starts.
#[test]
fn each_bus_has_its_own_start_budget() {
    assert_eq!(start_budget(Bus::Effects), 3);
    assert_eq!(start_budget(Bus::Ui), 2);
    assert_eq!(start_budget(Bus::Music), 1);
    assert_eq!(start_budget(Bus::Master), 0);
    assert_eq!(start_budget(Bus::Atmosphere), 0);
    assert_eq!(START_BUDGET.iter().sum::<usize>(), 6);
    // Four effects cues: three start, most important first.
    assert_eq!(
        select_sounds(vec![Cue::Impact, Cue::Armor, Cue::Reload, Cue::Throw], 0),
        vec![Cue::Throw, Cue::Reload, Cue::Impact]
    );
    // Three stingers: one starts, and it takes no effects or UI slot.
    assert_eq!(
        select_sounds(
            vec![
                Cue::Victory,
                Cue::Defeat,
                Cue::Retreat,
                Cue::Confirm,
                Cue::Damage,
                Cue::Throw,
                Cue::Reload,
                Cue::Trap
            ],
            0
        ),
        vec![
            Cue::Victory,
            Cue::Confirm,
            Cue::Damage,
            Cue::Throw,
            Cue::Reload
        ]
    );
    // No frame starts more than its budgets, whatever is requested.
    let every: Vec<Cue> = Cue::ALL.into_iter().filter(|cue| !cue.retired()).collect();
    let starts = select_starts(every, None, 5);
    for bus in Bus::ALL {
        let count = starts
            .iter()
            .filter(|start| mix(start.cue).bus == bus)
            .count();
        assert!(count <= start_budget(bus), "{bus:?}: {count}");
    }
    assert_eq!(starts.len(), 3 + 1 + 1, "{starts:?}");
}

#[test]
fn important_audio_cues_are_not_displaced_by_fire_and_sources_take_turns() {
    // A warning keeps its slot against three fire sources, and subsumes the
    // spawn cue.
    let frame = vec![
        Cue::Rifle,
        Cue::TurretShot,
        Cue::Pistol,
        Cue::Wave,
        Cue::Incoming,
    ];
    assert_eq!(
        select_sounds(frame.clone(), 0),
        vec![Cue::Wave, Cue::Rifle, Cue::Pistol]
    );
    assert_eq!(
        select_sounds(frame.clone(), 1),
        vec![Cue::Wave, Cue::Pistol, Cue::TurretShot]
    );
    assert_eq!(
        select_sounds(frame, 2),
        vec![Cue::Wave, Cue::TurretShot, Cue::Rifle]
    );
    // Critical cues go first and fill the budget before any fire.
    assert_eq!(
        select_sounds(vec![Cue::Rifle, Cue::Boss, Cue::Damage, Cue::Explosion], 0),
        vec![Cue::Boss, Cue::Explosion, Cue::Damage]
    );
    assert_eq!(
        select_sounds(vec![Cue::Boss, Cue::Damage, Cue::Rifle], 0),
        vec![Cue::Boss, Cue::Damage, Cue::Rifle]
    );
    assert_eq!(
        select_sounds(vec![Cue::Wave, Cue::Incoming], 0),
        vec![Cue::Wave]
    );
    // Without a critical cue one non-fire cue is kept against the fire.
    // Rotation is exercised with cues the game still emits.
    let turns = [Cue::Armor, Cue::Rifle, Cue::TurretShot, Cue::Pistol];
    assert!(turns.iter().all(|cue| !cue.retired()), "{turns:?}");
    assert_eq!(
        select_sounds(turns.to_vec(), 0),
        vec![Cue::Armor, Cue::Rifle, Cue::Pistol]
    );
    assert_eq!(
        select_sounds(turns.to_vec(), 1),
        vec![Cue::Armor, Cue::Pistol, Cue::TurretShot]
    );
    assert_eq!(
        select_sounds(turns.to_vec(), u64::MAX),
        select_sounds(turns.to_vec(), u64::MAX % 3)
    );
}

#[test]
fn stingers_and_kills_replace_the_cues_they_subsume() {
    assert_eq!(
        SUBSUMES,
        [
            (Cue::Wave, Cue::Incoming),
            (Cue::Kill, Cue::Impact),
            (Cue::Defeat, Cue::Structure),
            (Cue::Victory, Cue::Wave),
        ]
    );
    for (by, cue) in SUBSUMES {
        assert_eq!(select_sounds(vec![*cue, *by], 0), vec![*by], "{by:?}");
        assert_eq!(select_sounds(vec![*cue], 0), vec![*cue]);
    }
    assert_eq!(
        select_sounds(vec![Cue::Rifle, Cue::Impact, Cue::Kill], 0),
        vec![Cue::Kill, Cue::Rifle]
    );
    assert_eq!(
        select_sounds(vec![Cue::Kill, Cue::Victory, Cue::Boss, Cue::Rifle], 0),
        vec![Cue::Victory, Cue::Boss, Cue::Rifle, Cue::Kill]
    );
    assert_eq!(
        select_sounds(vec![Cue::Structure, Cue::Damage, Cue::Defeat], 0),
        vec![Cue::Defeat, Cue::Damage]
    );
    assert_eq!(
        select_sounds(vec![Cue::Fire, Cue::Confirm], 0),
        vec![Cue::Confirm, Cue::Fire]
    );
}

/// A cue requested twice in a frame starts once, wherever the repeats sit.
#[test]
fn repeated_requests_start_once_in_request_order() {
    assert_eq!(
        select_sounds(vec![Cue::Rifle, Cue::Pistol, Cue::Rifle, Cue::Pistol], 0),
        vec![Cue::Rifle, Cue::Pistol]
    );
    assert_eq!(
        select_sounds(
            vec![Cue::Impact, Cue::Armor, Cue::Impact, Cue::Trap, Cue::Armor],
            0
        ),
        vec![Cue::Impact, Cue::Armor, Cue::Trap]
    );
}

#[test]
fn gate_holds_a_cue_inside_its_cooldown_and_longer_in_reduced_effects() {
    let mut gate = CueGate::default();
    assert!(gate.accept(Cue::Fire, 0, false));
    assert!(!gate.accept(Cue::Fire, 1, false));
    assert!(gate.accept(Cue::Boss, 1, false));
    assert!(gate.accept(Cue::Fire, 850, false));
    assert!(!gate.accept(Cue::Fire, 1700, true));
    assert!(gate.accept(Cue::Fire, 2125, true));
}

/// The player's shot is gated against the player's own last shot only:
/// another defender's shot of the same class never silences it. It still
/// starts the shared cooldown, so the others do not stack on it.
#[test]
fn player_lane_gate_ignores_other_sources_of_the_same_cue() {
    let cooldown = u64::from(mix(Cue::Rifle).cooldown_ms);
    let mut gate = CueGate::default();
    assert!(gate.accept_start(Start::bus(Cue::Rifle), 0, false));
    assert!(gate.accept_start(Start::player(Cue::Rifle), 30, false));
    // Inside the player's own cooldown.
    assert!(!gate.accept_start(Start::player(Cue::Rifle), 30 + cooldown - 1, false));
    // The shared cooldown runs from the player's start.
    assert!(!gate.accept_start(Start::bus(Cue::Rifle), 30 + cooldown - 1, false));
    assert!(gate.accept_start(Start::player(Cue::Rifle), 30 + cooldown, false));
    // A change of weapon is not held by the old weapon's cooldown.
    assert!(gate.accept_start(Start::player(Cue::Shotgun), 31 + cooldown, false));
    // Reduced effects lengthens the lane's cooldown like any other.
    let reduced = mix(Cue::Shotgun).cooldown(true);
    assert!(reduced > u64::from(mix(Cue::Shotgun).cooldown_ms));
    let from = 31 + cooldown;
    assert!(!gate.accept_start(Start::player(Cue::Shotgun), from + reduced - 1, true));
    assert!(gate.accept_start(Start::player(Cue::Shotgun), from + reduced, true));
}

fn shot(shooter: Actor) -> SimEvent {
    SimEvent::Shot {
        shooter,
        target_id: None,
        lane: Some(0),
        from: [0.5, 0.5],
        to: [15.0, 0.5],
    }
}

/// Only a root-battle shot by the battle's defender marks the lane: a
/// turret, a trap and an expedition's defender do not.
#[test]
fn root_defender_shot_marks_the_player_weapon_lane() {
    for mode in [Mode::Personal, Mode::Npc] {
        let weapon = WeaponClass::Shotgun;
        let event = shot(Actor::Defender { mode, weapon });
        assert_eq!(events::defender_shot(&event), Some(Cue::Shotgun));
        let (mut cues, mut places) = (Vec::new(), Places::default());
        events::root_event_sound(&mut cues, &mut places, &event, Some(30.0));
        assert_eq!(cues, vec![Cue::Shotgun]);
        assert_eq!(places.player_shot(), Some(Cue::Shotgun));
        // Heard from an expedition, the same shot is an ordinary bus cue.
        let (mut cues, mut places) = (Vec::new(), Places::default());
        events::event_sound_at(&mut cues, &mut places, &event, None);
        assert_eq!((cues, places.player_shot()), (vec![Cue::Shotgun], None));
    }
    let turret = shot(Actor::Structure {
        id: 9000,
        kind: StructureKind::Turret,
    });
    assert_eq!(events::defender_shot(&turret), None);
    let (mut cues, mut places) = (Vec::new(), Places::default());
    events::root_event_sound(&mut cues, &mut places, &turret, Some(30.0));
    assert_eq!((cues, places.player_shot()), (vec![Cue::TurretShot], None));
    assert_eq!(events::defender_shot(&SimEvent::Reload), None);
}

/// The policy end to end: the player's shot starts beside a full Effects
/// budget, through another source's cooldown, and at the voice caps.
#[test]
fn policy_starts_the_player_shot_beside_a_full_budget_and_at_the_voice_cap() {
    let audible = Sliders::MAX.bus_gains();
    let cues = |actions: &render::FrameActions| -> Vec<Cue> {
        actions.starts.iter().map(|start| start.cue).collect()
    };
    let rifle = Some(Cue::Rifle);
    let crowded = vec![Cue::Boss, Cue::Explosion, Cue::Structure, Cue::Rifle];
    let mut policy = VoicePolicy::default();
    let with = policy.frame_requests(crowded.clone(), rifle, 0, false, &audible, &[]);
    assert_eq!(
        cues(&with),
        vec![Cue::Boss, Cue::Explosion, Cue::Structure, Cue::Rifle]
    );
    // The same frame without the lane: the rifle waits.
    let without = VoicePolicy::default().frame(crowded, 0, false, &audible, &[]);
    assert_eq!(
        cues(&without),
        vec![Cue::Boss, Cue::Explosion, Cue::Structure]
    );
    // Another defender's rifle 30 ms ago does not hold the player's.
    let mut policy = VoicePolicy::default();
    let other = policy.frame(vec![Cue::Rifle], 0, false, &audible, &[]);
    assert_eq!(cues(&other), vec![Cue::Rifle]);
    let player = policy.frame_requests(vec![Cue::Rifle], rifle, 30, false, &audible, &[]);
    assert_eq!(cues(&player), vec![Cue::Rifle]);
    let held = policy.frame_requests(vec![Cue::Rifle], rifle, 60, false, &audible, &[]);
    assert!(held.starts.is_empty(), "{held:?}");
    // Two guns sound, the gun family's cap: the player's shot fades the
    // older one out (`tests/voices.rs` has the caps).
    let voice = |id, cue, started_ms| LiveVoice {
        id,
        cue,
        started_ms,
    };
    let live = [voice(100, Cue::Smg, 40), voice(101, Cue::Pistol, 10)];
    let lane =
        VoicePolicy::default().frame_requests(vec![Cue::Rifle], rifle, 60, false, &audible, &live);
    assert_eq!((cues(&lane), lane.fades), (vec![Cue::Rifle], vec![101]));
    // At a full Effects bus it never fades a more important voice.
    let important: Vec<LiveVoice> = (100..108).map(|id| voice(id, Cue::Structure, 0)).collect();
    let kept = VoicePolicy::default().frame_requests(
        vec![Cue::Rifle],
        rifle,
        0,
        false,
        &audible,
        &important,
    );
    assert!(kept.starts.is_empty() && kept.fades.is_empty(), "{kept:?}");
    // On a muted Effects bus the lane starts nothing.
    let muted = Sliders {
        effects: 0.0,
        ..Sliders::MAX
    }
    .bus_gains();
    let none =
        VoicePolicy::default().frame_requests(vec![Cue::Rifle], rifle, 0, false, &muted, &[]);
    assert!(none.starts.is_empty());
}
