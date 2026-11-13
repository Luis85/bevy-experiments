//! The gun tracker: the sounds of handling the defender's gun that the
//! simulation has no event for.
//!
//! [`GunTracker`] reads the attempt after every tick, with that tick's
//! input, and compares it with what it saw the tick before:
//!
//! - **Reload stages.** `reload_left` rising is the reload's start:
//!   `ReloadOut`. Half way through, `ReloadIn`. A shotgun pushes shells in
//!   instead: `ShellInsert` at `k / (n + 1)` of the reload for each of the
//!   `n = min(magazine - ammo, 6)` rounds it is short. The finish is the
//!   core's own `SimEvent::Reload` (the v1 `Reload` cue, mapped in
//!   `events`); the tracker only tunes it to the weapon class.
//! - **The silent refill.** An NPC defender whose reload takes one turn
//!   refills with no `Reload` event; the tracker plays `Reload` for it.
//! - **The bolt.** A marksman rifle cycles its bolt [`BOLT_DELAY_TICKS`]
//!   after each shot, unless a reload began in between.
//! - **The dry trigger.** `DryFire` on the press of the trigger, and every
//!   [`DRY_FIRE_REPEAT_TICKS`] while it is held, when the player's own gun
//!   is empty, not reloading, not in a melee commit and auto reload is off
//!   (the case in which `manual.rs` does nothing at all).
//!
//! Every stage plays at the speed of the weapon class ([`stage_speed`]).
//!
//! The tracker is presentation: it reads the attempt and the input, changes
//! neither, and is not saved. It primes on an attempt it has not seen (a
//! deploy, a load), so a save loaded mid-reload plays no stage of a reload
//! it did not hear begin. Its timing is simulated ticks; the cue gates
//! (`select::CueGate`) bound the starts in real time at any speed.

use crate::{cue::Cue, events::push_cue, pan::Places, voicing::Voicing};
use horde_core::{
    content::Mode,
    sim::{Attempt, BattleId, Input},
    weapons::WeaponClass,
};
use std::collections::BTreeMap;

/// The marksman rifle's bolt follows its shot by 300 ms.
pub const BOLT_DELAY_TICKS: u32 = 18;
/// A held dry trigger clicks again after this many ticks: 400 ms, the first
/// whole frames clear of the cue's 350 ms gate, so that a repeat is never a
/// frame early for the gate and lost.
pub const DRY_FIRE_REPEAT_TICKS: u64 = 24;
/// The most shells one shotgun reload is heard to push in.
pub const MAX_SHELL_INSERTS: u32 = 6;

/// The playback speed of a class's handling stages: a small gun is quicker
/// and higher, a heavy one slower and lower.
pub const fn stage_speed(class: WeaponClass) -> f32 {
    match class {
        WeaponClass::Pistol => 1.08,
        WeaponClass::Smg => 1.04,
        WeaponClass::Rifle => 1.00,
        WeaponClass::Marksman => 0.95,
        WeaponClass::Shotgun => 0.92,
        WeaponClass::MachineGun => 0.90,
    }
}

/// A reload the tracker heard begin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Reloading {
    /// `reload_left` at the start.
    total: u32,
    /// Shells to push in; 0 for a gun with a magazine.
    shells: u32,
    shells_played: u32,
    in_played: bool,
}

/// What the tracker saw of one battle's gun after the last tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Gun {
    attempt_id: u64,
    ammo: u32,
    reload_left: u32,
    shots: u32,
    reloading: Option<Reloading>,
    /// The attempt tick at which the bolt cycles.
    bolt_due: Option<u64>,
    fire_held: bool,
    /// The attempt tick of the last dry click.
    dry_at: Option<u64>,
}
impl Gun {
    fn primed(attempt: &Attempt, input: &Input) -> Self {
        Self {
            attempt_id: attempt.id,
            ammo: attempt.ammo,
            reload_left: attempt.reload_left,
            shots: attempt.shots,
            reloading: None,
            bolt_due: None,
            fire_held: input.fire,
            dry_at: None,
        }
    }
}

/// The guns of the battles that are heard in full, by battle.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GunTracker {
    guns: BTreeMap<BattleId, Gun>,
}
impl GunTracker {
    /// Hears `battle`'s gun after a tick that ran with `input`: queues the
    /// stage cues in `cues`, each voiced in `places` at the speed of the
    /// weapon class. The first observation of an attempt only primes.
    pub fn observe(
        &mut self,
        battle: BattleId,
        attempt: &Attempt,
        input: &Input,
        cues: &mut Vec<Cue>,
        places: &mut Places,
    ) {
        let gun = match self.guns.get_mut(&battle) {
            Some(gun) if gun.attempt_id == attempt.id => gun,
            _ => {
                self.guns.insert(battle, Gun::primed(attempt, input));
                return;
            }
        };
        let class = attempt.profile.weapon.class;
        let speed = stage_speed(class);
        // Queues a stage and tunes it to the class. Tuning the reload's
        // finish alone queues nothing: the core's event does.
        let tune = |places: &mut Places, cue: Cue| {
            places.voice(cue, Voicing::tuned(cue.bank(), speed, 0.0));
        };
        let mut stage = |places: &mut Places, cue: Cue| {
            push_cue(cues, cue);
            tune(places, cue);
        };
        let before = *gun;

        if attempt.reload_left > before.reload_left {
            // A reload began.
            let shells = if class == WeaponClass::Shotgun {
                attempt
                    .magazine
                    .saturating_sub(attempt.ammo)
                    .min(MAX_SHELL_INSERTS)
            } else {
                0
            };
            gun.reloading = Some(Reloading {
                total: attempt.reload_left,
                shells,
                shells_played: 0,
                in_played: false,
            });
            // Its stages chamber the next round.
            gun.bolt_due = None;
            stage(places, Cue::ReloadOut);
        } else if attempt.reload_left > 0 {
            if let Some(reload) = &mut gun.reloading {
                let elapsed = u64::from(reload.total.saturating_sub(attempt.reload_left));
                let total = u64::from(reload.total);
                if class == WeaponClass::Shotgun {
                    // Shell k of n goes in at k / (n + 1) of the reload.
                    let parts = u64::from(reload.shells) + 1;
                    while reload.shells_played < reload.shells
                        && elapsed * parts >= u64::from(reload.shells_played + 1) * total
                    {
                        reload.shells_played += 1;
                        stage(places, Cue::ShellInsert);
                    }
                } else if !reload.in_played && elapsed * 2 >= total {
                    reload.in_played = true;
                    stage(places, Cue::ReloadIn);
                }
            }
        } else {
            let full = attempt.ammo == attempt.magazine;
            if before.reload_left > 0 {
                // The reload is over. Finished, the core's Reload event
                // sounds it and the class tunes it; canceled (a melee
                // strike), nothing follows.
                gun.reloading = None;
                if full {
                    tune(places, Cue::Reload);
                }
            } else if attempt.mode == Mode::Npc && full && attempt.ammo > before.ammo {
                // The NPC's one-turn refill has no event.
                stage(places, Cue::Reload);
            }
        }

        if class == WeaponClass::Marksman
            && attempt.shots > before.shots
            && attempt.reload_left == 0
        {
            gun.bolt_due = Some(attempt.tick + u64::from(BOLT_DELAY_TICKS));
        } else if gun.bolt_due.is_some_and(|due| attempt.tick >= due) {
            gun.bolt_due = None;
            stage(places, Cue::BoltCycle);
        }

        // The tick ran against the state before it, so the gun was empty
        // then and is empty now: the shot that empties it is not a click.
        let dry = attempt.mode == Mode::Personal
            && input.fire
            && !input.auto_reload
            && before.ammo == 0
            && attempt.ammo == 0
            && before.reload_left == 0
            && attempt.reload_left == 0
            && attempt.melee_commit == 0;
        if dry
            && (!before.fire_held
                || before
                    .dry_at
                    .is_none_or(|at| attempt.tick >= at + DRY_FIRE_REPEAT_TICKS))
        {
            gun.dry_at = Some(attempt.tick);
            stage(places, Cue::DryFire);
        }

        gun.ammo = attempt.ammo;
        gun.reload_left = attempt.reload_left;
        gun.shots = attempt.shots;
        gun.fire_held = input.fire;
    }
    /// Whether `battle`'s gun is tracked.
    pub fn tracks(&self, battle: BattleId) -> bool {
        self.guns.contains_key(&battle)
    }
    /// Drops every battle but those in `keep`: a battle that is heard again
    /// later primes anew.
    pub fn retain(&mut self, keep: impl Fn(BattleId) -> bool) {
        self.guns.retain(|battle, _| keep(*battle));
    }
}
