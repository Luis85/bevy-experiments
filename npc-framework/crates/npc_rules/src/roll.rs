//! Deterministic dice and the world-level rules API other crates call.
//!
//! **Dice key.** Every die is a pure function of `(world seed, unit SimId,
//! tick, purpose, roll index, die index)`: `sim_rand(seed, unit, tick,
//! purpose_id(purpose))` mixed with the roll and die index through
//! SplitMix64. `purpose` is a name chosen per call site (`"craft"`,
//! `"perception.detect"`); its id is a fixed FNV-1a hash
//! ([`purpose_id`]). The **roll index** counts this unit's earlier rolls
//! with the same purpose in the same tick ([`RollCounters`]), so a second
//! roll in the same tick differs from the first, while rolls of other units
//! or with other purposes never shift anyone's dice: the result does not
//! depend on the order in which systems or units roll.
//!
//! **Journal.** Every world-level roll records `Roll { skill, target, roll,
//! margin, success, crit, purpose }` for the rolling unit.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use npc_kernel::hash::Fnv64;
use npc_kernel::ids::SimIndex;
use npc_kernel::journal::{self, Journal, JournalKind};
use npc_kernel::rng::{WorldSeed, sim_rand, splitmix64};
use npc_kernel::{SimClock, SimId};
use serde::{Deserialize, Serialize};
use tracing::info_span;

use crate::defs::RulesRegistry;
use crate::ruleset::{ContestWinner, DiceSpec, RollOutcome, RuleSet};
use crate::sheet::{CharacterSheet, Purchase, Rules, SheetView, SkillLevel, SpendError};
use crate::stats::Stat;

/// The RNG purpose id of a purpose name: FNV-1a 64 of the length-prefixed
/// name, folded to 32 bits. Part of the determinism contract.
pub fn purpose_id(purpose: &str) -> u32 {
    let mut h = Fnv64::default();
    h.str(purpose);
    (h.0 ^ (h.0 >> 32)) as u32
}

/// One die (`1..=sides`) of roll `index` (pure; see the module docs).
pub fn die(seed: u64, unit: SimId, tick: u64, purpose: &str, index: u32, die: u8, sides: u8) -> u8 {
    let base = sim_rand(seed, unit, tick, purpose_id(purpose));
    let r = splitmix64(base ^ splitmix64((u64::from(index) << 8) | u64::from(die)));
    let sides = u64::from(sides.max(1));
    // `r % sides < sides <= 255`.
    (1 + r % sides) as u8
}

/// The dice total of roll `index` (pure).
pub fn dice_total(
    seed: u64,
    unit: SimId,
    tick: u64,
    purpose: &str,
    index: u32,
    dice: DiceSpec,
) -> i32 {
    (0..dice.count)
        .map(|d| i32::from(die(seed, unit, tick, purpose, index, d, dice.sides)))
        .sum()
}

/// One resolved roll as the opt-in [`RollFeed`] records it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RollRecord {
    pub tick: u64,
    pub unit: SimId,
    /// The skill or stat label (`"Climbing"`, `"Per"`), as journaled.
    pub skill: String,
    pub outcome: RollOutcome,
    pub purpose: String,
}

/// The opt-in roll feed: a deterministic hook for mechanics that react to
/// rolls (`npc_progression` awards XP and practice from it). Off by default,
/// so nothing is recorded without a consumer. When `enabled`, every roll
/// ([`roll_against`] and everything built on it, and [`roll_with`]) appends
/// a [`RollRecord`] in the order the rolls happen (systems that roll take
/// `ResMut<RollCounters>`, so they are ordered and the order is fixed). The
/// consumer drains it within the same tick (`npc_progression` does so in
/// `NpcSet::Cleanup`), so it is empty at every tick boundary and needs no
/// hashing. Never fed from the journal: it works with the journal off.
#[derive(Clone, Debug, Default)]
pub struct RollFeed {
    pub enabled: bool,
    pub records: Vec<RollRecord>,
}

/// Per-tick roll counters: `(unit, purpose id) → rolls so far` in `tick`.
/// Reset whenever a roll happens in another tick. Not hashed: it only
/// numbers the rolls within one tick, and is empty in effect at every tick
/// boundary. Also carries the opt-in [`RollFeed`].
#[derive(Resource, Clone, Debug, Default)]
pub struct RollCounters {
    pub tick: u64,
    pub counts: BTreeMap<(SimId, u32), u32>,
    pub feed: RollFeed,
}

impl RollCounters {
    /// Appends a roll to the feed when it is enabled.
    pub fn observe(
        &mut self,
        tick: u64,
        unit: SimId,
        label: &str,
        outcome: &RollOutcome,
        purpose: &str,
    ) {
        if self.feed.enabled {
            self.feed.records.push(RollRecord {
                tick,
                unit,
                skill: label.to_owned(),
                outcome: *outcome,
                purpose: purpose.to_owned(),
            });
        }
    }

    /// Takes every recorded roll (the feed stays enabled).
    pub fn drain_feed(&mut self) -> Vec<RollRecord> {
        std::mem::take(&mut self.feed.records)
    }

    /// The next roll index for `(unit, purpose)` in `tick`.
    pub fn next(&mut self, tick: u64, unit: SimId, purpose: u32) -> u32 {
        if self.tick != tick {
            self.tick = tick;
            self.counts.clear();
        }
        let n = self.counts.entry((unit, purpose)).or_insert(0);
        let index = *n;
        *n = n.saturating_add(1);
        index
    }
}

/// What a roll is against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RollSubject<'a> {
    Skill(&'a str),
    Stat(Stat),
}

impl RollSubject<'_> {
    /// The journal label (`"Climbing"`, `"Per"`).
    pub fn label(&self) -> &str {
        match self {
            RollSubject::Skill(s) => s,
            RollSubject::Stat(s) => s.name(),
        }
    }
}

/// One side of a contest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Contestant<'a> {
    pub unit: SimId,
    pub subject: RollSubject<'a>,
    pub modifier: i32,
}

impl<'a> Contestant<'a> {
    pub fn skill(unit: SimId, skill: &'a str, modifier: i32) -> Self {
        Self {
            unit,
            subject: RollSubject::Skill(skill),
            modifier,
        }
    }

    pub fn stat(unit: SimId, stat: Stat, modifier: i32) -> Self {
        Self {
            unit,
            subject: RollSubject::Stat(stat),
            modifier,
        }
    }
}

/// Both rolls of a quick contest and who won (`by`: margin difference).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QuickContest {
    pub a: RollOutcome,
    pub b: RollOutcome,
    pub winner: ContestWinner,
    pub by: i32,
}

/// A resisted roll: the actor succeeds only if its own roll succeeds **and**
/// it wins the contest against the resister (ties go to the resister).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResistedOutcome {
    pub contest: QuickContest,
    pub success: bool,
}

/// Runs `f` with the world's rules (defaults when the plugin is absent).
pub fn with_rules<R>(world: &World, f: impl FnOnce(Rules<'_>) -> R) -> R {
    let default_set;
    let set = match world.get_resource::<RuleSet>() {
        Some(s) => s,
        None => {
            default_set = RuleSet::default();
            &default_set
        }
    };
    let default_registry;
    let registry = match world.get_resource::<RulesRegistry>() {
        Some(r) => r,
        None => {
            default_registry = RulesRegistry::default();
            &default_registry
        }
    };
    f(Rules::new(set, registry))
}

fn entity_of(world: &World, unit: SimId) -> Option<Entity> {
    world.get_resource::<SimIndex>()?.get(unit)
}

/// The unit's sheet.
pub fn sheet(world: &World, unit: SimId) -> Option<&CharacterSheet> {
    world.get::<CharacterSheet>(entity_of(world, unit)?)
}

/// Effective value of `stat` for `unit` (`Speed` in quarters); `None`
/// without a sheet.
pub fn attribute(world: &World, unit: SimId, stat: Stat) -> Option<i32> {
    let sheet = sheet(world, unit)?;
    Some(with_rules(world, |r| r.stat(sheet, stat)))
}

/// The resolved level of `skill` for `unit` (see [`Rules::skill_level`]).
pub fn skill_level(world: &World, unit: SimId, skill: &str) -> Option<SkillLevel> {
    let sheet = sheet(world, unit)?;
    with_rules(world, |r| r.skill_level(sheet, skill))
}

/// Effective level of `skill` for `unit`; `None` without a sheet, for an
/// unknown skill, or with no points and no usable default.
pub fn effective_skill(world: &World, unit: SimId, skill: &str) -> Option<i32> {
    skill_level(world, unit, skill).map(|l| l.level)
}

/// The effective level of a subject.
pub fn subject_level(world: &World, unit: SimId, subject: RollSubject<'_>) -> Option<i32> {
    match subject {
        RollSubject::Skill(s) => effective_skill(world, unit, s),
        RollSubject::Stat(s) => attribute(world, unit, s),
    }
}

/// Rolls the dice for `unit` against `target` and journals it under `label`
/// (the lowest-level roll: crafting or interaction checks with a computed
/// target use it directly).
pub fn roll_against(
    world: &mut World,
    unit: SimId,
    label: &str,
    target: i32,
    purpose: &str,
) -> RollOutcome {
    let _span = info_span!("mechanic", name = "rules").entered();
    let seed = world.get_resource::<WorldSeed>().map_or(0, |s| s.0);
    let tick = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
    let index = world
        .get_resource_or_init::<RollCounters>()
        .next(tick, unit, purpose_id(purpose));
    let outcome = with_rules(world, |r| {
        let roll = dice_total(seed, unit, tick, purpose, index, r.set.dice);
        r.set.resolve_roll(roll, target)
    });
    world
        .resource_mut::<RollCounters>()
        .observe(tick, unit, label, &outcome, purpose);
    journal::record(world, Some(unit), || roll_entry(label, &outcome, purpose));
    outcome
}

/// Success roll of `unit` on `skill` with situational `modifier`; `None`
/// when [`effective_skill`] is `None` (nothing is rolled or journaled then).
pub fn roll_skill(
    world: &mut World,
    unit: SimId,
    skill: &str,
    modifier: i32,
    purpose: &str,
) -> Option<RollOutcome> {
    let level = effective_skill(world, unit, skill)?;
    Some(roll_against(
        world,
        unit,
        skill,
        level.saturating_add(modifier),
        purpose,
    ))
}

/// Success roll of `unit` on `stat` (e.g. `Per` for detection, `HT` to
/// resist); `None` without a sheet.
pub fn roll_stat(
    world: &mut World,
    unit: SimId,
    stat: Stat,
    modifier: i32,
    purpose: &str,
) -> Option<RollOutcome> {
    let level = attribute(world, unit, stat)?;
    Some(roll_against(
        world,
        unit,
        stat.name(),
        level.saturating_add(modifier),
        purpose,
    ))
}

/// Rolls one contestant.
pub fn roll_subject(world: &mut World, c: Contestant<'_>, purpose: &str) -> Option<RollOutcome> {
    let level = subject_level(world, c.unit, c.subject)?;
    Some(roll_against(
        world,
        c.unit,
        c.subject.label(),
        level.saturating_add(c.modifier),
        purpose,
    ))
}

/// A quick contest: both sides roll (`a` first), the rule set's contest rule
/// picks the winner. `None` if either side cannot roll (nothing is rolled).
pub fn quick_contest(
    world: &mut World,
    a: Contestant<'_>,
    b: Contestant<'_>,
    purpose: &str,
) -> Option<QuickContest> {
    subject_level(world, a.unit, a.subject)?;
    subject_level(world, b.unit, b.subject)?;
    let ra = roll_subject(world, a, purpose)?;
    let rb = roll_subject(world, b, purpose)?;
    let winner = with_rules(world, |r| r.set.contest_winner(&ra, &rb));
    Some(QuickContest {
        a: ra,
        b: rb,
        winner,
        by: ra.margin.saturating_sub(rb.margin).saturating_abs(),
    })
}

/// A resisted roll (prepared for later use: influence, poisons, spells):
/// a quick contest the actor must win while succeeding.
pub fn resisted_roll(
    world: &mut World,
    actor: Contestant<'_>,
    resister: Contestant<'_>,
    purpose: &str,
) -> Option<ResistedOutcome> {
    let contest = quick_contest(world, actor, resister, purpose)?;
    Some(ResistedOutcome {
        success: contest.a.success && contest.winner == ContestWinner::A,
        contest,
    })
}

/// Validates `purchase` and pays it from the unit's banked points; returns
/// the cost. On error nothing changes.
pub fn spend_points(
    world: &mut World,
    unit: SimId,
    purchase: &Purchase,
) -> Result<i32, SpendError> {
    let entity = entity_of(world, unit).ok_or(SpendError::NoSheet { unit: unit.0 })?;
    let mut sheet = world
        .get::<CharacterSheet>(entity)
        .cloned()
        .ok_or(SpendError::NoSheet { unit: unit.0 })?;
    let cost = with_rules(world, |r| r.apply_purchase(&mut sheet, purchase))?;
    if let Some(mut s) = world.get_mut::<CharacterSheet>(entity) {
        *s = sheet;
    }
    Ok(cost)
}

/// What `purchase` would cost the unit (validated, without the
/// enough-points check; nothing changes).
pub fn purchase_cost(world: &World, unit: SimId, purchase: &Purchase) -> Result<i32, SpendError> {
    let sheet = sheet(world, unit).ok_or(SpendError::NoSheet { unit: unit.0 })?;
    with_rules(world, |r| r.purchase_cost(sheet, purchase))
}

/// Adds `points` (may be negative, never below 0 banked) to the unit's
/// banked points (levels, rewards); returns the new balance.
pub fn grant_points(world: &mut World, unit: SimId, points: i32) -> Result<i32, SpendError> {
    let entity = entity_of(world, unit).ok_or(SpendError::NoSheet { unit: unit.0 })?;
    let mut sheet = world
        .get_mut::<CharacterSheet>(entity)
        .ok_or(SpendError::NoSheet { unit: unit.0 })?;
    let next = sheet
        .unspent
        .checked_add(points)
        .ok_or(SpendError::Overflow)?;
    if next < 0 {
        return Err(SpendError::NotEnoughPoints {
            cost: -points,
            unspent: sheet.unspent,
        });
    }
    sheet.unspent = next;
    Ok(next)
}

/// A fresh sheet from registered template `name`.
pub fn template_sheet(world: &World, name: &str) -> Option<CharacterSheet> {
    with_rules(world, |r| r.template_sheet(name))
}

/// Inserts (replaces) the sheet of `entity` from template `name`.
pub fn apply_template(world: &mut World, entity: Entity, name: &str) -> Result<(), SpendError> {
    let sheet = template_sheet(world, name).ok_or_else(|| SpendError::UnknownTemplate {
        name: name.to_owned(),
        known: with_rules(world, |r| r.registry.template_names()),
    })?;
    world.entity_mut(entity).insert(sheet);
    Ok(())
}

/// The unit's sheet with every derived value (inspect, BRP).
pub fn sheet_view(world: &World, unit: SimId) -> Option<SheetView> {
    let sheet = sheet(world, unit)?;
    Some(with_rules(world, |r| r.view(sheet)))
}

/// JSON of an entity's sheet for the hash registry / generic inspector:
/// the [`SheetView`].
pub fn sheet_json(world: &World, entity: Entity) -> Option<serde_json::Value> {
    let sheet = world.get::<CharacterSheet>(entity)?;
    serde_json::to_value(with_rules(world, |r| r.view(sheet))).ok()
}

/// The journal entry of a roll (`Roll { .. }`).
pub fn roll_entry(label: &str, outcome: &RollOutcome, purpose: &str) -> JournalKind {
    JournalKind::Roll {
        skill: label.to_owned(),
        target: outcome.target,
        roll: outcome.roll,
        margin: outcome.margin,
        success: outcome.success,
        crit: outcome.critical,
        purpose: purpose.to_owned(),
    }
}

/// System-side roll (no `&mut World`): the same dice key, counter and
/// resolution as [`roll_against`], with the journal entry pushed to
/// `journal` if given (enabled). For systems holding `Res<RuleSet>`,
/// `ResMut<RollCounters>`, `Res<WorldSeed>`, `Res<SimClock>` and
/// `Option<ResMut<Journal>>`.
#[allow(clippy::too_many_arguments)]
pub fn roll_with(
    rules: &RuleSet,
    counters: &mut RollCounters,
    journal: Option<&mut Journal>,
    seed: u64,
    tick: u64,
    unit: SimId,
    label: &str,
    target: i32,
    purpose: &str,
) -> RollOutcome {
    let index = counters.next(tick, unit, purpose_id(purpose));
    let roll = dice_total(seed, unit, tick, purpose, index, rules.dice);
    let outcome = rules.resolve_roll(roll, target);
    counters.observe(tick, unit, label, &outcome, purpose);
    if let Some(j) = journal {
        j.push(tick, Some(unit), roll_entry(label, &outcome, purpose));
    }
    outcome
}
