//! Perception × rules (features `perception` + `rules`): detection by a
//! Perception roll (actor-capabilities spec §4.2 "Detection", §4.6 hooks).
//!
//! [`PerceptionRulesPlugin`] registers the detection check [`PER_ROLL`]
//! (`"per_roll"`) with `npc_perception`'s hook registry and, with
//! `activate: true` (the default), makes it the active check. For every
//! candidate detection (in a vision cone with line of sight, within
//! proximity, or a heard noise) the observer rolls 3d6 under
//! `Per + modifier`:
//!
//! - **distance band** ([`PerRollConfig::distance_bands`], nearest band whose
//!   radius contains the target; [`PerRollConfig::beyond_modifier`] past the
//!   last one);
//! - **sense** (`vision` / `proximity` / `hearing`; a robot's `camera` and
//!   `radar` never roll: they are automatic, plan Task 12; `reach` uses the
//!   proximity modifier);
//! - **target modifiers**: every registered [`TargetModifierFn`] summed in
//!   name order ([`PerRollAppExt::register_per_roll_modifier`]); the built-in
//!   `"stealthy"` one subtracts the target's [`Stealthy::penalty`].
//!
//! A success detects with a confidence from the margin (critical: full);
//! a failure does not detect. An observer without a `CharacterSheet` uses
//! [`PerRollConfig::without_sheet`] (automatic by default). A target already
//! sensed is not re-rolled unless [`PerRollConfig::reroll_sensed`]: once
//! spotted it stays tracked while it remains in view.
//!
//! **Dice key.** Pure: `(world seed, observer, tick, dice purpose, index)`,
//! so the dice never depend on the order in which observers or candidates
//! are visited, and a re-roll of the same target in a later scan uses fresh
//! dice. (The rules crate's per-tick roll counter is not used: a check only
//! gets `&World`.) Every detection of one observer in one tick gets its own
//! key ([`detect_key`]):
//!
//! - purpose `"perception.detect"`, `index = target SimId · 4 + sense` (0
//!   for a sourceless noise) while that is unambiguous: the target's
//!   `SimId` is below 2^30 and, for a noise, it is the first noise of its
//!   source (or the first sourceless one) in the tick;
//! - otherwise purpose `"perception.detect.noise"` with the noise's ordinal
//!   in the tick's sorted noise list, or `"perception.detect.vision"` /
//!   `".proximity"` / `".reach"` with the full target `SimId` (sense 3 is
//!   reach).
//!
//! (Final review cleanup 4: the plain `target · 4 + sense` index collided
//! for `SimId`s 2^30 apart and gave every sourceless noise the same dice.)
//! The journaled purpose is always `"perception.detect"`.
//!
//! **Journal.** Each roll is journaled as `Roll { skill: "Per", purpose:
//! "perception.detect", .. }` for the observer, in the tick it happened,
//! right after that tick's `Perceived` / `Lost` entries: the check buffers
//! the entries ([`PerRollLog`]) and a system in `NpcPerceptionSet`, after
//! the perception pass, records them in visit order (observers by `SimId`).
//!
//! **Hashing.** `stealthy` (component) and `per_roll_config` (resource) join
//! the state hash; the check and modifier names join the content hash.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_kernel::hash::{Fnv64, SimHash, SimHashResource};
use npc_kernel::journal::{self, JournalKind};
use npc_kernel::registry::{KernelAppExt, is_frozen};
use npc_kernel::rng::WorldSeed;
use npc_kernel::{NpcSimTick, SimId};
use npc_perception::{
    CONFIDENCE_FULL, Detection, NpcPerceptionSet, PerceptionAppExt, PerceptionMemory, SenseKind,
};
use npc_rules::roll::roll_entry;
use npc_rules::{Critical, RollOutcome, Stat, attribute, dice_total, with_rules};
use serde::{Deserialize, Serialize};

/// Name of the detection check.
pub const PER_ROLL: &str = "per_roll";
/// The roll purpose (journal `Roll.purpose`, part of the dice key).
pub const DETECT_PURPOSE: &str = "perception.detect";
/// Name of the built-in target modifier (reads [`Stealthy`]).
pub const STEALTHY_MODIFIER: &str = "stealthy";

/// Makes a target harder to detect by a `per_roll` check: `penalty` is
/// subtracted from the observer's Per (negative: easier, e.g. a bright
/// banner). Hashed as `stealthy`.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct Stealthy {
    pub penalty: i32,
}

impl SimHash for Stealthy {
    const NAME: &'static str = "stealthy";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.i32(self.penalty);
    }
}

/// Tuning of the `per_roll` check. Insert before the plugin to change it
/// (it uses `init_resource`). Hashed as `per_roll_config`.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct PerRollConfig {
    /// `(radius mm, modifier)`, ascending radius: the first band whose
    /// radius contains the target applies.
    pub distance_bands: Vec<(i32, i32)>,
    /// Modifier past the last band.
    pub beyond_modifier: i32,
    pub vision_modifier: i32,
    pub proximity_modifier: i32,
    pub hearing_modifier: i32,
    /// Confidence for an observer without a character sheet (`None`: such
    /// observers detect nothing). Default: automatic (full confidence).
    pub without_sheet: Option<u16>,
    /// Roll again for targets already sensed (default `false`: once
    /// spotted, a target stays tracked while it remains in view).
    pub reroll_sensed: bool,
}

impl Default for PerRollConfig {
    fn default() -> Self {
        Self {
            distance_bands: vec![(2_000, 3), (5_000, 0), (10_000, -2), (20_000, -4)],
            beyond_modifier: -6,
            vision_modifier: 0,
            proximity_modifier: 2,
            hearing_modifier: -2,
            without_sheet: Some(CONFIDENCE_FULL),
            reroll_sensed: false,
        }
    }
}

impl PerRollConfig {
    /// The distance-band modifier for a squared distance (mm²).
    pub fn distance_modifier(&self, dist2: i64) -> i32 {
        self.distance_bands
            .iter()
            .find(|(r, _)| {
                let r = i64::from((*r).max(0));
                dist2 <= r * r
            })
            .map_or(self.beyond_modifier, |&(_, m)| m)
    }

    /// The sense modifier.
    pub fn sense_modifier(&self, via: SenseKind) -> i32 {
        match via {
            SenseKind::Vision => self.vision_modifier,
            // Reach is touch range: it rolls like proximity (no new config
            // field, so `per_roll_config` keeps its hash).
            SenseKind::Proximity | SenseKind::Reach => self.proximity_modifier,
            SenseKind::Hearing => self.hearing_modifier,
            // Never rolled (automatic, plan Task 12); like vision if asked.
            SenseKind::Camera | SenseKind::Radar => self.vision_modifier,
        }
    }
}

impl SimHashResource for PerRollConfig {
    const NAME: &'static str = "per_roll_config";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(u32::try_from(self.distance_bands.len()).expect("too many distance bands"));
        for &(r, m) in &self.distance_bands {
            h.i32(r);
            h.i32(m);
        }
        for v in [
            self.beyond_modifier,
            self.vision_modifier,
            self.proximity_modifier,
            self.hearing_modifier,
        ] {
            h.i32(v);
        }
        h.opt(self.without_sheet.as_ref(), |h, c| h.u16(*c));
        h.u8(u8::from(self.reroll_sensed));
    }
}

/// A target modifier: added to the observer's Per for this detection
/// (negative: harder). Must be deterministic (`&World` only).
pub type TargetModifierFn = fn(&World, &Detection) -> i32;

/// Registered target modifiers by name (summed in name order). The names
/// join the content hash.
#[derive(Resource, Clone, Debug)]
pub struct PerRollModifiers {
    pub hooks: BTreeMap<&'static str, TargetModifierFn>,
}

impl Default for PerRollModifiers {
    fn default() -> Self {
        Self {
            hooks: BTreeMap::from([(STEALTHY_MODIFIER, stealthy_modifier as TargetModifierFn)]),
        }
    }
}

/// The built-in `"stealthy"` modifier: `-Stealthy::penalty` of the target.
pub fn stealthy_modifier(world: &World, d: &Detection) -> i32 {
    d.target
        .and_then(|t| world.get::<Stealthy>(t))
        .map_or(0, |s| s.penalty.saturating_neg())
}

/// Registration of target modifiers on `App`.
pub trait PerRollAppExt {
    /// Adds (or replaces) target modifier `name` (before the freeze).
    fn register_per_roll_modifier(&mut self, name: &'static str, f: TargetModifierFn) -> &mut Self;
}

impl PerRollAppExt for App {
    fn register_per_roll_modifier(&mut self, name: &'static str, f: TargetModifierFn) -> &mut Self {
        assert!(
            !is_frozen(self.world()),
            "cannot register per_roll modifier `{name}`: content is already frozen"
        );
        self.world_mut()
            .get_resource_or_init::<PerRollModifiers>()
            .hooks
            .insert(name, f);
        self
    }
}

/// Roll journal entries buffered by the check (it only gets `&World`),
/// recorded by [`flush_per_roll_log`] in the same tick. Not hashed.
#[derive(Resource, Default)]
pub struct PerRollLog(pub Mutex<Vec<(SimId, JournalKind)>>);

/// The whole modifier of one detection: distance band + sense + every
/// target modifier.
pub fn per_roll_modifier(world: &World, config: &PerRollConfig, d: &Detection) -> i32 {
    let targets = world.get_resource::<PerRollModifiers>().map_or(0, |m| {
        m.hooks
            .values()
            .fold(0i32, |acc, f| acc.saturating_add(f(world, d)))
    });
    config
        .distance_modifier(d.dist2)
        .saturating_add(config.sense_modifier(d.via))
        .saturating_add(targets)
}

/// The dice purpose and index of a detection (see the module docs): unique
/// per observer and tick.
pub fn detect_key(d: &Detection) -> (&'static str, u32) {
    // Camera and radar never roll (they are automatic, plan Task 12); if a
    // check is ever asked about them they get their own purposes below.
    let sense = match d.via {
        SenseKind::Vision => Some(0),
        SenseKind::Proximity => Some(1),
        SenseKind::Hearing => Some(2),
        SenseKind::Reach => Some(3),
        SenseKind::Camera | SenseKind::Radar => None,
    };
    let first_of_source = d.noise.is_none_or(|n| n.of_source == 0);
    match (d.target_id, sense) {
        (Some(t), Some(sense)) if t.0 < 1 << 30 && first_of_source => {
            (DETECT_PURPOSE, (t.0 << 2) | sense)
        }
        (None, _) if first_of_source => (DETECT_PURPOSE, 0),
        _ => match (d.noise, d.via) {
            (Some(n), _) => ("perception.detect.noise", n.ordinal),
            (None, SenseKind::Proximity) => (
                "perception.detect.proximity",
                d.target_id.map_or(0, |t| t.0),
            ),
            (None, SenseKind::Reach) => ("perception.detect.reach", d.target_id.map_or(0, |t| t.0)),
            (None, SenseKind::Camera) => {
                ("perception.detect.camera", d.target_id.map_or(0, |t| t.0))
            }
            (None, SenseKind::Radar) => ("perception.detect.radar", d.target_id.map_or(0, |t| t.0)),
            (None, _) => ("perception.detect.vision", d.target_id.map_or(0, |t| t.0)),
        },
    }
}

/// Confidence (permille) of a successful roll: 500 + 100 per point of
/// margin, 100..=1000; a critical success is full confidence.
fn confidence(outcome: &RollOutcome) -> u16 {
    if outcome.critical == Critical::Success {
        return CONFIDENCE_FULL;
    }
    let c = 500i32.saturating_add(outcome.margin.saturating_mul(100));
    u16::try_from(c.clamp(100, i32::from(CONFIDENCE_FULL))).unwrap_or(CONFIDENCE_FULL)
}

/// The `per_roll` detection check (see the module docs).
pub fn per_roll_check(world: &World, d: &Detection) -> Option<u16> {
    let default_config;
    let config = match world.get_resource::<PerRollConfig>() {
        Some(c) => c,
        None => {
            default_config = PerRollConfig::default();
            &default_config
        }
    };
    if !config.reroll_sensed
        && let Some(target) = d.target_id
        && let Some(p) = world
            .get::<PerceptionMemory>(d.observer)
            .and_then(|m| m.get(target))
        && p.sensed
    {
        return Some(p.confidence);
    }
    let Some(per) = attribute(world, d.observer_id, Stat::Per) else {
        return config.without_sheet;
    };
    let target = per.saturating_add(per_roll_modifier(world, config, d));
    let seed = world.get_resource::<WorldSeed>().map_or(0, |s| s.0);
    let (purpose, index) = detect_key(d);
    let outcome = with_rules(world, |r| {
        let roll = dice_total(seed, d.observer_id, d.tick, purpose, index, r.set.dice);
        r.set.resolve_roll(roll, target)
    });
    if journal::is_recording(world)
        && let Some(log) = world.get_resource::<PerRollLog>()
    {
        log.0.lock().unwrap_or_else(PoisonError::into_inner).push((
            d.observer_id,
            roll_entry(Stat::Per.name(), &outcome, DETECT_PURPOSE),
        ));
    }
    outcome.success.then(|| confidence(&outcome))
}

/// Records the buffered `Roll` entries (`NpcPerceptionSet`, after the
/// perception pass).
pub fn flush_per_roll_log(world: &mut World) {
    let entries = match world.get_resource::<PerRollLog>() {
        Some(log) => std::mem::take(&mut *log.0.lock().unwrap_or_else(PoisonError::into_inner)),
        None => return,
    };
    for (unit, kind) in entries {
        journal::record(world, Some(unit), || kind);
    }
}

fn freeze_per_roll(world: &mut World) -> Vec<(String, String)> {
    world
        .get_resource_or_init::<PerRollModifiers>()
        .hooks
        .keys()
        .map(|k| ("per_roll_modifier".to_owned(), (*k).to_owned()))
        .collect()
}

/// Perception × rules glue (see the module docs). Add it with (after)
/// `PerceptionPlugin` and `RulesPlugin`; `activate: false` only registers
/// the check (select it later with `use_detection_check("per_roll")`).
pub struct PerceptionRulesPlugin {
    /// Make `per_roll` the active detection check (default `true`).
    pub activate: bool,
}

impl Default for PerceptionRulesPlugin {
    fn default() -> Self {
        Self { activate: true }
    }
}

impl Plugin for PerceptionRulesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PerRollConfig>()
            .init_resource::<PerRollModifiers>()
            .init_resource::<PerRollLog>()
            .register_sim_component::<Stealthy>()
            .register_sim_resource::<PerRollConfig>()
            .add_freeze_hook("npc_framework.per_roll", freeze_per_roll)
            .register_detection_check(PER_ROLL, per_roll_check)
            .add_systems(
                NpcSimTick,
                flush_per_roll_log
                    .in_set(NpcPerceptionSet)
                    .after(npc_perception::perception_system),
            );
        if self.activate {
            app.use_detection_check(PER_ROLL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_bands_pick_the_first_containing_band() {
        let c = PerRollConfig::default();
        assert_eq!(c.distance_modifier(0), 3);
        assert_eq!(c.distance_modifier(2_000 * 2_000), 3);
        assert_eq!(c.distance_modifier(2_001 * 2_001), 0);
        assert_eq!(c.distance_modifier(10_000 * 10_000), -2);
        assert_eq!(c.distance_modifier(30_000 * 30_000), -6);
    }

    #[test]
    fn confidence_follows_the_margin() {
        let o = |margin, critical| RollOutcome {
            roll: 10,
            target: 10 + margin,
            margin,
            success: true,
            critical,
        };
        assert_eq!(confidence(&o(0, Critical::None)), 500);
        assert_eq!(confidence(&o(3, Critical::None)), 800);
        assert_eq!(confidence(&o(9, Critical::None)), 1000);
        assert_eq!(confidence(&o(-7, Critical::None)), 100);
        assert_eq!(confidence(&o(0, Critical::Success)), 1000);
    }
}
