//! Cue selection: the repeat gate and the per-frame choice of which requested
//! cues start. Both are the v1 policy, moved here unchanged so the runtime
//! and the offline renderer decide with the same code.

use crate::{
    cue::{CUE_COUNT, Cue},
    mix::{GateGroup, mix},
};

/// Cues that may start in one frame.
pub const MAX_STARTS_PER_FRAME: usize = 2;

/// Coalesce repeated ticks and accelerated observation into a restrained real-
/// time soundscape. A cue dropped by the gate never delays a simulation action.
#[derive(Debug, Default)]
pub struct CueGate {
    last_started: [Option<u64>; CUE_COUNT],
}
impl CueGate {
    pub fn accept(&mut self, cue: Cue, now_ms: u64, reduced: bool) -> bool {
        let last = &mut self.last_started[cue as usize];
        if last.is_some_and(|previous| {
            now_ms >= previous && now_ms - previous < mix(cue).cooldown(reduced)
        }) {
            return false;
        }
        *last = Some(now_ms);
        true
    }
}

fn rank(cue: Cue) -> u8 {
    mix(cue).priority
}
fn fires(cue: Cue) -> bool {
    mix(cue).gate == GateGroup::Fire
}

/// The cues of one frame's requests that may start, most important first.
/// `rotation` counts the frames that had requests, so fire sources take turns.
pub fn select_sounds(mut sounds: Vec<Cue>, rotation: u64) -> Vec<Cue> {
    // A warning already communicates arrival; avoid stacking a second spawn cue.
    if sounds.contains(&Cue::Wave) {
        sounds.retain(|cue| *cue != Cue::Incoming);
    }
    // A kill subsumes the hit that caused it, and a defeat stinger the
    // collapse that ended the defense (they share the same bank).
    if sounds.contains(&Cue::Kill) {
        sounds.retain(|cue| *cue != Cue::Impact);
    }
    if sounds.contains(&Cue::Defeat) {
        sounds.retain(|cue| *cue != Cue::Structure);
    }
    if sounds.contains(&Cue::Victory) {
        sounds.retain(|cue| *cue != Cue::Wave);
    }
    sounds.sort_by_key(|cue| rank(*cue));
    sounds.dedup();
    let mut selected = Vec::with_capacity(MAX_STARTS_PER_FRAME);
    selected.extend(
        sounds
            .iter()
            .copied()
            .filter(|cue| rank(*cue) <= 3)
            .take(MAX_STARTS_PER_FRAME),
    );
    if selected.is_empty()
        && let Some(cue) = sounds
            .iter()
            .copied()
            .find(|cue| !fires(*cue) && rank(*cue) <= 8)
    {
        selected.push(cue);
    }
    let fire: Vec<_> = sounds.iter().copied().filter(|cue| fires(*cue)).collect();
    for n in 0..fire.len() {
        if selected.len() == MAX_STARTS_PER_FRAME {
            break;
        }
        selected.push(fire[(rotation as usize + n) % fire.len()]);
    }
    for cue in sounds {
        if selected.len() == MAX_STARTS_PER_FRAME {
            break;
        }
        if !selected.contains(&cue) {
            selected.push(cue);
        }
    }
    selected
}
