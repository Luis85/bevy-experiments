//! Verb definitions ([`InteractionDef`]), the registry and the hooks.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_kernel::{SimId, SimPos};
use serde::{Deserialize, Serialize};

use crate::items::ItemKindDef;

/// One interaction being checked or run: who does it, to what, with which
/// (optional) item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ctx {
    pub actor: Entity,
    pub actor_id: SimId,
    pub target: Entity,
    pub target_id: SimId,
    /// The instrument or object (`give`'s item, an explicit tool or key).
    pub item: Option<(SimId, Entity)>,
}

/// Whether a verb is relevant to `ctx.target` at all (affordance listing:
/// a door is offered `open`, an item `pick_up`). Cheap, no reasons.
pub type OfferFn = fn(&World, &Ctx) -> bool;
/// The verb's own preconditions (target state, items, tools); `Err` is the
/// human-readable reason (`"door is locked"`, `"needs tool:axe"`).
pub type PreconditionFn = fn(&World, &Ctx) -> Result<(), String>;
/// Applies the verb's effects (all preconditions held when called).
pub type EffectFn = fn(&mut World, &Ctx);
/// A duration or a check that depends on the target.
pub type DurationFn = fn(&World, &Ctx) -> u32;
pub type CheckFn = fn(&World, &Ctx) -> Option<SkillCheck>;
/// Preconditions to list in an affordance's `requires` (`["key:gate"]`).
pub type RequiresFn = fn(&World, &Ctx) -> Vec<String>;

/// A skill check: roll `skill` at `modifier` (through the skill-check hook;
/// without one, checks succeed).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct SkillCheck {
    pub skill: String,
    pub modifier: i32,
}

impl SkillCheck {
    pub fn new(skill: impl Into<String>, modifier: i32) -> Self {
        Self {
            skill: skill.into(),
            modifier,
        }
    }
}

/// A noise an interaction makes when it succeeds (through the noise hook).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct NoiseSpec {
    pub radius_mm: i32,
    /// Content bit set (perception's `Noise::kind`).
    pub kind: u32,
}

/// Whether a verb takes an item argument.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ItemArg {
    #[default]
    None,
    /// Optional (an explicit tool or key; otherwise picked).
    Optional,
    /// Required (`give`).
    Required,
}

/// A verb, registered by stable name. Built with [`InteractionDef::new`] and
/// the builder methods.
#[derive(Clone, Copy)]
pub struct InteractionDef {
    pub name: &'static str,
    pub offer: OfferFn,
    pub precondition: PreconditionFn,
    pub effect: EffectFn,
    /// Ticks from start to effect (0: the effect applies in the tick it
    /// starts). Overridden by `duration_fn`.
    pub duration_ticks: u32,
    pub duration_fn: Option<DurationFn>,
    /// Rolled when the duration is over; a failed check consumes the time
    /// and applies no effect. Overridden by `check_fn`.
    pub check: Option<(&'static str, i32)>,
    pub check_fn: Option<CheckFn>,
    /// The actor must be within this distance of the target (`None`: the
    /// configured default; see `InteractionConfig::reach_mm`). Ignored when
    /// `needs_reach` is false (verbs on carried items).
    pub reach_mm: Option<i32>,
    pub needs_reach: bool,
    /// Claim the target (capacity 1) for the duration.
    pub reserve_target: bool,
    pub noise: Option<NoiseSpec>,
    pub item: ItemArg,
    pub requires: Option<RequiresFn>,
}

fn always(_: &World, _: &Ctx) -> bool {
    true
}

impl InteractionDef {
    /// A verb offered for every target, instant, reserving the target and
    /// needing reach.
    pub fn new(name: &'static str, precondition: PreconditionFn, effect: EffectFn) -> Self {
        Self {
            name,
            offer: always,
            precondition,
            effect,
            duration_ticks: 0,
            duration_fn: None,
            check: None,
            check_fn: None,
            reach_mm: None,
            needs_reach: true,
            reserve_target: true,
            noise: None,
            item: ItemArg::None,
            requires: None,
        }
    }

    pub fn offer(mut self, f: OfferFn) -> Self {
        self.offer = f;
        self
    }

    pub fn duration(mut self, ticks: u32) -> Self {
        self.duration_ticks = ticks;
        self
    }

    pub fn duration_fn(mut self, f: DurationFn) -> Self {
        self.duration_fn = Some(f);
        self
    }

    pub fn check(mut self, skill: &'static str, modifier: i32) -> Self {
        self.check = Some((skill, modifier));
        self
    }

    pub fn check_fn(mut self, f: CheckFn) -> Self {
        self.check_fn = Some(f);
        self
    }

    pub fn reach(mut self, mm: i32) -> Self {
        self.reach_mm = Some(mm);
        self.needs_reach = true;
        self
    }

    /// The target is carried by the actor: no distance check.
    pub fn no_reach(mut self) -> Self {
        self.needs_reach = false;
        self
    }

    pub fn no_reservation(mut self) -> Self {
        self.reserve_target = false;
        self
    }

    pub fn noise(mut self, radius_mm: i32, kind: u32) -> Self {
        self.noise = Some(NoiseSpec { radius_mm, kind });
        self
    }

    pub fn item(mut self, arg: ItemArg) -> Self {
        self.item = arg;
        self
    }

    pub fn requires(mut self, f: RequiresFn) -> Self {
        self.requires = Some(f);
        self
    }

    /// The duration for `ctx`.
    pub fn duration_for(&self, world: &World, ctx: &Ctx) -> u32 {
        self.duration_fn
            .map_or(self.duration_ticks, |f| f(world, ctx))
    }

    /// The skill check for `ctx`, if any.
    pub fn check_for(&self, world: &World, ctx: &Ctx) -> Option<SkillCheck> {
        match self.check_fn {
            Some(f) => f(world, ctx),
            None => self.check.map(|(s, m)| SkillCheck::new(s, m)),
        }
    }
}

/// Rolls a skill check for `actor` (`purpose` names the use site, e.g.
/// `"interaction.unlock"`). `Some(success)`, or `None` when the actor has no
/// rules (the check then succeeds).
pub type SkillCheckFn = fn(&mut World, Entity, &SkillCheck, &str) -> Option<bool>;
/// Emits a noise made by `source` at `pos`.
pub type NoiseFn = fn(&mut World, SimId, SimPos, NoiseSpec);

/// The optional hooks other capabilities plug in, by name (the names join
/// the content hash). Without a skill-check hook every check succeeds;
/// without a noise hook interactions are silent.
#[derive(Resource, Clone, Copy, Default)]
pub struct InteractionHooks {
    pub skill_check: Option<(&'static str, SkillCheckFn)>,
    pub noise: Option<(&'static str, NoiseFn)>,
}

/// A busy predicate: what else keeps `actor` from starting an interaction
/// (`Some("craft bread")`), besides a running interaction. Registered by
/// name ([`InteractionAppExt::add_busy_check`](crate::InteractionAppExt::add_busy_check)),
/// e.g. by the crafting glue, so `begin` refuses while the unit crafts.
pub type BusyFn = fn(&World, Entity) -> Option<String>;
/// Called whenever an interaction ends (after its effects and journal
/// entry): the actor, the interaction, the outcome (`"success"`,
/// `"failed"`, `"invalid"`, `"cancelled"`) and the reason. Registered by
/// name ([`InteractionAppExt::add_finished_hook`](crate::InteractionAppExt::add_finished_hook)),
/// e.g. by the command glue to finish `Interact` orders.
pub type FinishedFn = fn(&mut World, Entity, &crate::run::Interacting, &str, Option<&str>);

/// Busy predicates and finished hooks by name (each list sorted by name;
/// the names join the content hash). Empty by default, so an app that
/// registers none hashes exactly as before.
#[derive(Resource, Clone, Default)]
pub struct InteractionListeners {
    pub busy: Vec<(&'static str, BusyFn)>,
    pub finished: Vec<(&'static str, FinishedFn)>,
}

impl InteractionListeners {
    /// Adds (or replaces) busy predicate `name`, keeping name order.
    pub fn add_busy(&mut self, name: &'static str, f: BusyFn) {
        match self.busy.binary_search_by_key(&name, |(n, _)| n) {
            Ok(i) => self.busy[i].1 = f,
            Err(i) => self.busy.insert(i, (name, f)),
        }
    }

    /// Adds (or replaces) finished hook `name`, keeping name order.
    pub fn add_finished(&mut self, name: &'static str, f: FinishedFn) {
        match self.finished.binary_search_by_key(&name, |(n, _)| n) {
            Ok(i) => self.finished[i].1 = f,
            Err(i) => self.finished.insert(i, (name, f)),
        }
    }
}

/// Registered item kinds and verbs, by name.
#[derive(Resource, Clone, Default)]
pub struct InteractionRegistry {
    pub kinds: BTreeMap<String, ItemKindDef>,
    pub verbs: BTreeMap<&'static str, InteractionDef>,
}

impl InteractionRegistry {
    pub fn verb(&self, name: &str) -> Option<&InteractionDef> {
        self.verbs.get(name)
    }

    /// Every `(category, name)` for the content hash.
    pub fn manifest(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .kinds
            .keys()
            .map(|k| ("item_kind".to_owned(), k.clone()))
            .collect();
        out.extend(
            self.verbs
                .keys()
                .map(|v| ("interaction".to_owned(), (*v).to_owned())),
        );
        out
    }
}
