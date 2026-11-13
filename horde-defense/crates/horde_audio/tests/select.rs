//! Pins for cue selection (A6-04): the v1 per-frame choice and the repeat
//! gate, moved out of `horde_game` unchanged. The runtime and the offline
//! renderer both decide with them.

use horde_audio::{
    cue::Cue,
    select::{CueGate, MAX_STARTS_PER_FRAME, select_sounds},
};

#[test]
fn important_audio_cues_are_not_displaced_by_fire_and_sources_take_turns() {
    let selected = select_sounds(
        vec![Cue::Rifle, Cue::TurretShot, Cue::Wave, Cue::Incoming],
        0,
    );
    assert!(selected.contains(&Cue::Wave));
    assert_eq!(selected.len(), MAX_STARTS_PER_FRAME);
    assert_eq!(
        select_sounds(vec![Cue::Boss, Cue::Damage, Cue::Rifle], 0),
        vec![Cue::Boss, Cue::Damage]
    );
    assert_eq!(
        select_sounds(vec![Cue::Wave, Cue::Incoming], 0),
        vec![Cue::Wave]
    );
    // Rotation is exercised with cues the game still emits.
    let turns = [Cue::Armor, Cue::Rifle, Cue::TurretShot];
    assert!(turns.iter().all(|cue| !cue.retired()), "{turns:?}");
    let a = select_sounds(turns.to_vec(), 0);
    let b = select_sounds(turns.to_vec(), 1);
    assert!(a.contains(&Cue::Armor) && a.contains(&Cue::Rifle));
    assert!(b.contains(&Cue::Armor) && b.contains(&Cue::TurretShot));
}

#[test]
fn stingers_and_kills_replace_the_cues_they_subsume() {
    assert_eq!(
        select_sounds(vec![Cue::Rifle, Cue::Impact, Cue::Kill], 0),
        vec![Cue::Kill, Cue::Rifle]
    );
    assert_eq!(
        select_sounds(vec![Cue::Kill, Cue::Victory, Cue::Boss, Cue::Rifle], 0),
        vec![Cue::Victory, Cue::Boss]
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
