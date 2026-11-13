//! Ducking and density compensation: the gains that make room in the mix.
//! The runtime and the offline renderer both run [`Ducker`], once a frame.
//!
//! - **Ducking.** A starting cue with a [`mix::duck`] row pulls the ambience
//!   (and the music tracks) down, holds them and lets them back. Ducks of one
//!   bus add up in dB, down to [`DUCK_FLOOR_DB`]. Only Atmosphere and Music
//!   duck: the gains of Master, Effects and UI are always exactly 1, so a
//!   warning is never pulled down.
//! - **Density.** With more than [`mix::DENSITY_FULL_VOICES`] effect voices
//!   sounding, every compensated voice ([`compensated`]) plays at
//!   [`mix::density_gain`], and the ambience gives up to
//!   [`mix::DENSITY_ATMOSPHERE_DB`]. Warnings and the root defender's own gun
//!   are not compensated.
//!
//! Everything runs on elapsed time, so the envelopes are the same at every
//! frame rate. Nothing here is switched off by reduced effects: ducking and
//! compensation only ever lower a level.

use crate::{
    cue::Cue,
    mix::{
        self, Bus, DENSITY_ATMOSPHERE_RELEASE_MS, DENSITY_ATTACK_MS, DENSITY_RELEASE_MS,
        DUCK_ATTACK_MS, DUCK_FLOOR_DB, DUCK_HOLD_MAX_MS, Duck, DuckHold, db_to_linear, mix,
    },
    select::{Lane, Start},
    voices::{Family, LiveVoice, family},
};

/// The longest frame the envelopes advance by, in seconds: a stall does not
/// skip a duck.
pub const MAX_STEP_SECONDS: f32 = 0.25;

/// What the executor applies this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DuckGains {
    /// The linear duck of each bus, indexed by [`Bus`]. Master, Effects and
    /// UI are always 1. Atmosphere multiplies the ambience loops; Music
    /// multiplies the music tracks, not the stingers.
    pub duck: [f32; 5],
    /// The linear gain of every compensated effect voice ([`compensated`]).
    pub density: f32,
}
impl DuckGains {
    /// Nothing ducks.
    pub const NONE: Self = Self {
        duck: [1.0; 5],
        density: 1.0,
    };
}
impl Default for DuckGains {
    fn default() -> Self {
        Self::NONE
    }
}

/// Whether the voice of `start` takes the density gain: an Effects voice
/// that is neither a warning nor the root defender's own shot.
pub fn compensated(start: Start) -> bool {
    let row = mix(start.cue);
    row.bus == Bus::Effects && start.lane != Lane::PlayerWeapon && !warns(start.cue)
}

/// The cues that announce danger: a row with a visual twin, the radio
/// warnings and the boss arrival.
fn warns(cue: Cue) -> bool {
    mix(cue).warning.is_some() || matches!(family(cue), Some(Family::Radio | Family::Boss))
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Attack,
    /// At full depth for this many more ms.
    Hold(f32),
    /// At full depth while the cue sounds, this many more ms at most.
    Sounding(f32),
    Release,
}

/// One cue's duck in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Active {
    cue: Cue,
    row: Duck,
    /// The share of the row's depth, 0..=1.
    level: f32,
    phase: Phase,
}
impl Active {
    /// Advances by `ms`; false when the duck is over.
    fn advance(&mut self, mut ms: f32, sounding: bool) -> bool {
        loop {
            match self.phase {
                Phase::Attack => {
                    let attack = self.row.attack_ms as f32;
                    let need = (1.0 - self.level) * attack;
                    if ms < need {
                        self.level += ms / attack;
                        return true;
                    }
                    ms -= need;
                    self.level = 1.0;
                    self.phase = match self.row.hold {
                        DuckHold::Ms(hold) => Phase::Hold(hold as f32),
                        DuckHold::WhileSounding => Phase::Sounding(DUCK_HOLD_MAX_MS as f32),
                    };
                }
                Phase::Hold(left) => {
                    if ms < left {
                        self.phase = Phase::Hold(left - ms);
                        return true;
                    }
                    ms -= left;
                    self.phase = Phase::Release;
                }
                Phase::Sounding(left) => {
                    if sounding && ms < left {
                        self.phase = Phase::Sounding(left - ms);
                        return true;
                    }
                    if sounding {
                        ms -= left;
                    }
                    self.phase = Phase::Release;
                }
                Phase::Release => {
                    let release = self.row.release_ms as f32;
                    let need = self.level * release;
                    if ms < need {
                        self.level -= ms / release;
                        return true;
                    }
                    return false;
                }
            }
        }
    }
}

/// `current` moved towards `target` for `ms` with time constant `tau_ms`;
/// it lands on the target once it is within `snap`.
fn approach(current: f32, target: f32, ms: f32, tau_ms: f32, snap: f32) -> f32 {
    let next = target + (current - target) * (-ms / tau_ms).exp();
    if (next - target).abs() < snap {
        target
    } else {
        next
    }
}

/// The ducking and density state. Presentation only: it reads the cues that
/// started and the voices that sound, never the game.
#[derive(Clone, Debug)]
pub struct Ducker {
    /// At most one per cue.
    active: Vec<Active>,
    /// The smoothed density gain of the compensated voices.
    density: f32,
    /// The smoothed density term of the ambience, in dB.
    density_atmosphere_db: f32,
}
impl Default for Ducker {
    fn default() -> Self {
        Self {
            active: Vec::new(),
            density: 1.0,
            density_atmosphere_db: 0.0,
        }
    }
}
impl Ducker {
    /// A voice of `cue` started. A cue without a duck row does nothing; a cue
    /// already ducking goes back to its attack from the level it is at.
    pub fn trigger(&mut self, cue: Cue) {
        let Some(row) = mix::duck(cue) else {
            return;
        };
        match self.active.iter_mut().find(|active| active.cue == cue) {
            Some(active) => active.phase = Phase::Attack,
            None => self.active.push(Active {
                cue,
                row,
                level: 0.0,
                phase: Phase::Attack,
            }),
        }
    }

    /// Whether any cue's duck is in progress.
    pub fn ducking(&self) -> bool {
        !self.active.is_empty()
    }

    /// Advances by `seconds` of real time (clamped to 0..=
    /// [`MAX_STEP_SECONDS`]; a non-finite step is none) and returns the
    /// frame's gains. `live` lists the sounding voices, this frame's starts
    /// included and the fading ones left out.
    pub fn advance(&mut self, seconds: f32, live: &[LiveVoice]) -> DuckGains {
        let ms = if seconds.is_finite() {
            seconds.clamp(0.0, MAX_STEP_SECONDS) * 1000.0
        } else {
            0.0
        };
        self.active.retain_mut(|active| {
            let sounding = live.iter().any(|voice| voice.cue == active.cue);
            active.advance(ms, sounding)
        });
        let effects = live
            .iter()
            .filter(|voice| mix(voice.cue).bus == Bus::Effects)
            .count();
        let target = mix::density_gain(effects);
        let tau = if target < self.density {
            DENSITY_ATTACK_MS
        } else {
            DENSITY_RELEASE_MS
        };
        self.density = approach(self.density, target, ms, tau, 1e-4);
        let target = mix::density_atmosphere_db(effects);
        let tau = if target < self.density_atmosphere_db {
            DUCK_ATTACK_MS as f32
        } else {
            DENSITY_ATMOSPHERE_RELEASE_MS
        };
        self.density_atmosphere_db = approach(self.density_atmosphere_db, target, ms, tau, 1e-3);

        let sum = |depth: fn(&Duck) -> f32| -> f32 {
            self.active
                .iter()
                .map(|active| active.level * depth(&active.row))
                .sum()
        };
        let atmosphere = sum(|row| row.atmosphere_db) + self.density_atmosphere_db;
        let music = sum(|row| row.music_db);
        let gain = |db: f32| db_to_linear(db.clamp(DUCK_FLOOR_DB, 0.0));
        let mut gains = DuckGains {
            density: self.density,
            ..DuckGains::NONE
        };
        gains.duck[Bus::Atmosphere as usize] = gain(atmosphere);
        gains.duck[Bus::Music as usize] = gain(music);
        gains
    }
}
