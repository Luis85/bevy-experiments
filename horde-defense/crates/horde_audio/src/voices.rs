//! The voice pool: how many voices sound at once, which voice a new start
//! steals at a cap, and the fade a stolen voice ends with. The runtime and
//! the offline renderer decide with [`admit`] and fade with [`Ramp`].
//!
//! Caps count sounding voices only: a voice that is fading out has already
//! given up its slot.
//!
//! - A bus holds [`BUS_CAP`] voices. Each bus counts and steals among its own
//!   voices, so a combat cue never takes an interface sound or a stinger.
//! - A [`Family`] of cues holds [`Family::cap`] voices. A start into a full
//!   family steals that family's oldest voice that is no more important than
//!   the start (the highest priority number is the least important), and is
//!   refused when every member is more important.
//! - A start into a full bus steals the bus's least important voice (the
//!   highest priority number), the oldest of them first. The victim must be
//!   less important than the start; an equally important one is taken only
//!   by gunfire and impacts ([`Family::steals_equals`]) and by the root
//!   defender's own shot.
//! - The stolen voice fades for [`FADE_MS`] and then ends ([`VoiceControl`]).

use crate::{
    cue::Cue,
    mix::{Bus, mix},
    select::{Lane, Start},
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Voices that may sound at once on each bus, indexed by [`Bus`]: eight
/// combat effects, three interface sounds, one stinger. Atmosphere is the six
/// ambience loops, which are not cue voices; Master carries none.
pub const BUS_CAP: [usize; 5] = [0, 8, 3, 1, 6];

/// The voice cap of `bus`.
pub const fn bus_cap(bus: Bus) -> usize {
    BUS_CAP[bus as usize]
}

/// Cues that share one concurrency cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Every defender gun.
    Guns,
    Turret,
    /// Kills.
    Death,
    Impact,
    Armor,
    Explosion,
    Molotov,
    /// The fire bed.
    Fire,
    Reload,
    /// The radio warnings: Wave and Incoming.
    Radio,
    /// The boss arrival, of a known kind or not.
    Boss,
    /// Enemy windups.
    Windup,
    /// Enemy contacts.
    Contact,
    BossPhase,
    BossDeath,
    /// Battle drops.
    LootDrop,
}
impl Family {
    pub const ALL: [Self; 16] = [
        Self::Guns,
        Self::Turret,
        Self::Death,
        Self::Impact,
        Self::Armor,
        Self::Explosion,
        Self::Molotov,
        Self::Fire,
        Self::Reload,
        Self::Radio,
        Self::Boss,
        Self::Windup,
        Self::Contact,
        Self::BossPhase,
        Self::BossDeath,
        Self::LootDrop,
    ];
    /// Voices of the family that may sound at once.
    pub const fn cap(self) -> usize {
        match self {
            Self::Guns
            | Self::Death
            | Self::Impact
            | Self::Explosion
            | Self::Windup
            | Self::Contact => 2,
            Self::Turret
            | Self::Armor
            | Self::Molotov
            | Self::Fire
            | Self::Reload
            | Self::Radio
            | Self::Boss
            | Self::BossPhase
            | Self::BossDeath
            | Self::LootDrop => 1,
        }
    }
    /// Whether a start of this family may steal a voice exactly as important
    /// as itself at a bus cap: gunfire and impacts, where the newest is the
    /// one that matters.
    pub const fn steals_equals(self) -> bool {
        matches!(self, Self::Guns | Self::Turret | Self::Impact)
    }
}

/// The family of `cue`; `None` for a cue only its bus caps.
pub const fn family(cue: Cue) -> Option<Family> {
    match cue {
        Cue::Pistol
        | Cue::Smg
        | Cue::Shotgun
        | Cue::Rifle
        | Cue::Marksman
        | Cue::MachineGun
        | Cue::NpcShot => Some(Family::Guns),
        Cue::TurretShot => Some(Family::Turret),
        Cue::Kill | Cue::EnemyDeath => Some(Family::Death),
        Cue::Impact => Some(Family::Impact),
        Cue::Armor | Cue::Block => Some(Family::Armor),
        Cue::Explosion => Some(Family::Explosion),
        Cue::Molotov => Some(Family::Molotov),
        Cue::Fire => Some(Family::Fire),
        // One reload-stage voice at a time: a stage takes over from the last.
        Cue::Reload | Cue::ReloadOut | Cue::ReloadIn | Cue::ShellInsert => Some(Family::Reload),
        Cue::Wave | Cue::Incoming => Some(Family::Radio),
        Cue::Boss | Cue::BossArrival => Some(Family::Boss),
        Cue::EnemyWindup => Some(Family::Windup),
        Cue::EnemyContact => Some(Family::Contact),
        Cue::BossPhase => Some(Family::BossPhase),
        Cue::BossDeath => Some(Family::BossDeath),
        Cue::LootDrop => Some(Family::LootDrop),
        Cue::Throw
        | Cue::Retreat
        | Cue::Damage
        | Cue::Melee
        | Cue::Ready
        | Cue::Structure
        | Cue::Trap
        | Cue::Victory
        | Cue::Defeat
        | Cue::Confirm
        | Cue::BossExposed
        | Cue::Ability
        | Cue::DryFire
        | Cue::BoltCycle
        | Cue::LootReveal
        | Cue::CoinBatch => None,
    }
}

/// One sounding voice. A voice that is fading out is not listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveVoice {
    pub id: u64,
    pub cue: Cue,
    pub started_ms: u64,
}

/// What a start does to the pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// It starts on a free voice.
    Free,
    /// It starts, and the named voice fades out.
    Steal(u64),
    /// It does not start.
    Refuse,
}

/// Whether `start` may sound beside the `live` voices, and at whose cost.
pub fn admit(start: Start, live: &[LiveVoice]) -> Admission {
    let row = mix(start.cue);
    let family = family(start.cue);
    // Oldest first; of two voices started together, the lower id.
    let age = |voice: &&LiveVoice| (voice.started_ms, voice.id);
    if let Some(family) = family {
        let members = || {
            live.iter()
                .filter(|voice| self::family(voice.cue) == Some(family))
        };
        if members().count() >= family.cap() {
            // Only a member no more important than the start is taken: the
            // spawn squelch must not cut a sounding boss warning.
            return match members()
                .filter(|voice| mix(voice.cue).priority >= row.priority)
                .min_by_key(age)
            {
                Some(oldest) => Admission::Steal(oldest.id),
                None => Admission::Refuse,
            };
        }
    }
    let on_bus = || live.iter().filter(|voice| mix(voice.cue).bus == row.bus);
    if on_bus().count() < bus_cap(row.bus) {
        return Admission::Free;
    }
    let Some(least) = on_bus().map(|voice| mix(voice.cue).priority).max() else {
        // A bus without voices of its own.
        return Admission::Refuse;
    };
    let takes_equal = start.lane == Lane::PlayerWeapon || family.is_some_and(Family::steals_equals);
    if row.priority < least || (row.priority == least && takes_equal) {
        on_bus()
            .filter(|voice| mix(voice.cue).priority == least)
            .min_by_key(age)
            .map_or(Admission::Refuse, |victim| Admission::Steal(victim.id))
    } else {
        Admission::Refuse
    }
}

/// The fade of a stolen voice, in ms.
pub const FADE_MS: u32 = 40;
/// The time constant of the gain ramp, in ms.
pub const RAMP_TAU_MS: f32 = 8.0;
/// Samples between two reads of a [`VoiceControl`].
pub const CONTROL_PERIOD: u32 = 64;

/// What the frame thread tells a playing voice: the gain to ramp to, and
/// whether to end. Shared with the audio thread; lock-free.
#[derive(Debug)]
pub struct VoiceControl {
    /// The bits of the f32 target gain.
    target: AtomicU32,
    stop: AtomicBool,
}
impl Default for VoiceControl {
    fn default() -> Self {
        Self {
            target: AtomicU32::new(1.0_f32.to_bits()),
            stop: AtomicBool::new(false),
        }
    }
}
impl VoiceControl {
    pub fn target(&self) -> f32 {
        f32::from_bits(self.target.load(Ordering::Relaxed))
    }
    /// Ramp to `gain` (0..=1; a non-finite gain is silence).
    pub fn set_target(&self, gain: f32) {
        let gain = if gain.is_finite() {
            gain.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.target.store(gain.to_bits(), Ordering::Relaxed);
    }
    /// Fade to silence and end [`FADE_MS`] later.
    pub fn fade_out(&self) {
        self.set_target(0.0);
        self.stop.store(true, Ordering::Relaxed);
    }
    /// Whether the voice is fading out.
    pub fn stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

/// The per-sample gain of one voice: a one-pole ramp towards its control's
/// target, which ends the voice [`FADE_MS`] after the control says stop. It
/// runs on samples, so the fade is the same at every frame rate.
#[derive(Clone, Debug)]
pub struct Ramp {
    gain: f32,
    target: f32,
    /// The share of the remaining distance one sample covers.
    step: f32,
    /// Samples until the next read of the control.
    until_read: u32,
    /// Samples a fading voice still plays.
    left: Option<u32>,
    fade_samples: u32,
}
impl Ramp {
    /// A ramp at `rate` samples per second, starting at full gain.
    pub fn new(rate: u32) -> Self {
        let tau_samples = RAMP_TAU_MS / 1000.0 * rate.max(1) as f32;
        Self {
            gain: 1.0,
            target: 1.0,
            step: 1.0 - (-1.0 / tau_samples).exp(),
            until_read: 0,
            left: None,
            fade_samples: (u64::from(rate) * u64::from(FADE_MS) / 1000) as u32,
        }
    }
    /// The gain of the next sample; `None` when the voice has ended. The
    /// control is read every [`CONTROL_PERIOD`] samples.
    pub fn next(&mut self, control: &VoiceControl) -> Option<f32> {
        if self.until_read == 0 {
            self.until_read = CONTROL_PERIOD;
            self.target = control.target();
            if self.left.is_none() && control.stopping() {
                self.left = Some(self.fade_samples);
            }
        }
        self.until_read -= 1;
        if let Some(left) = &mut self.left {
            if *left == 0 {
                return None;
            }
            *left -= 1;
        }
        self.gain += (self.target - self.gain) * self.step;
        Some(self.gain)
    }
}
