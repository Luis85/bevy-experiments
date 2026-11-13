//! Lane pan: where a cue sits between the two channels.
//!
//! **Mapping.** A positional cue pans along the lane, relative to the
//! listener, who stands with the defender at the left end of the field:
//! `p = clamp((x - DEFENDER_X) / (width_yards - DEFENDER_X), 0, 1)`. The
//! position reads only the event's x in yards and the arena width; never the
//! lane index (lanes are rows, not columns), the lane count or the camera.
//!
//! **Law.** A peak-safe balance law: the right channel stays at 1.0 and the
//! left falls to `1 - D x p`. No channel ever exceeds the centred level, so
//! pan never raises a peak and the calibration does not depend on it. At the
//! far end (`p = 1`, `D = 0.30`) the level difference is 3.1 dB and the mono
//! fold-down loses 1.4 dB.
//!
//! `D` is [`PAN_DEPTH`], [`PAN_DEPTH_REDUCED`] in reduced effects, and 0 with
//! the "Mono audio" setting, which plays every voice centred.

use crate::cue::{CUE_COUNT, Cue};

/// The listener's x in yards: `Attempt::defender_position()[0]`.
pub const DEFENDER_X: f32 = 0.5;
/// How far the left channel falls at the far end of the lane.
pub const PAN_DEPTH: f32 = 0.30;
/// [`PAN_DEPTH`] in reduced effects.
pub const PAN_DEPTH_REDUCED: f32 = 0.15;

/// The pan depth `D` for the current settings.
pub fn depth(reduced: bool, mono: bool) -> f32 {
    if mono {
        0.0
    } else if reduced {
        PAN_DEPTH_REDUCED
    } else {
        PAN_DEPTH
    }
}

/// The pan position of `x` yards on a field `width_yards` wide: 0 at the
/// defender, 1 at the far end. A non-finite input or a field no wider than
/// the defender's own position is centred.
pub fn lane_position(x: f32, width_yards: f32) -> f32 {
    let span = width_yards - DEFENDER_X;
    if !x.is_finite() || !span.is_finite() || span <= 0.0 {
        return 0.0;
    }
    ((x - DEFENDER_X) / span).clamp(0.0, 1.0)
}

/// The `[left, right]` gains of pan position `p` at depth `depth`.
pub fn gains(p: f32, depth: f32) -> [f32; 2] {
    let p = if p.is_finite() {
        p.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let depth = if depth.is_finite() {
        depth.clamp(0.0, 1.0)
    } else {
        0.0
    };
    [1.0 - depth * p, 1.0]
}

/// The centred gains: both channels at the clip's level.
pub const CENTRE: [f32; 2] = [1.0, 1.0];

/// The level of `gains` folded down to mono, in dB against the centred voice.
pub fn mono_fold_db(gains: [f32; 2]) -> f32 {
    20.0 * ((gains[0] + gains[1]) * 0.5).log10()
}

/// Whether `cue` pans with its event's position. The defender's own gun, the
/// reload, the throw, injuries, melee, interface sounds, stingers and the
/// wave and boss warnings are centred.
pub const fn positional(cue: Cue) -> bool {
    matches!(
        cue,
        Cue::TurretShot
            | Cue::Impact
            | Cue::Armor
            | Cue::Explosion
            | Cue::Molotov
            | Cue::Fire
            | Cue::Structure
            | Cue::Trap
            | Cue::Incoming
            | Cue::Kill
    )
}

/// The pan positions of one frame's requests: one per cue, because a frame
/// requests each cue at most once. The first position a cue gets stands, as
/// the first request does.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Places([Option<f32>; CUE_COUNT]);
impl Places {
    /// Places `cue` at pan position `p` unless it already has a place. A cue
    /// that is not [`positional`] stays centred.
    pub fn place(&mut self, cue: Cue, p: f32) {
        let slot = &mut self.0[cue as usize];
        if positional(cue) && slot.is_none() && p.is_finite() {
            *slot = Some(p.clamp(0.0, 1.0));
        }
    }
    /// The pan position of `cue`; `None` is centred.
    pub fn of(&self, cue: Cue) -> Option<f32> {
        self.0[cue as usize]
    }
}

/// The `[left, right]` gains a voice of `cue` starts with: its place in
/// `places` through the balance law at the settings' depth.
pub fn voice_gains(cue: Cue, places: &Places, reduced: bool, mono: bool) -> [f32; 2] {
    request_gains(cue, places.of(cue), reduced, mono)
}

/// [`voice_gains`] of one request at pan position `p` (`None` is centred).
pub fn request_gains(cue: Cue, p: Option<f32>, reduced: bool, mono: bool) -> [f32; 2] {
    match p {
        Some(p) if positional(cue) => gains(p, depth(reduced, mono)),
        _ => CENTRE,
    }
}
