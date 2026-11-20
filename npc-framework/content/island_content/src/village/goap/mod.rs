//! Bounded, deterministic forward planning over actual inventory and finite offers.
//!
//! The production ledger chooses funded goals; this planner searches for a cheap
//! sequence of physical actions. It grants nothing. The first step is rechecked
//! through the same action implementation used by players, then the next life
//! decision replans from the resulting world. Stale offers cannot spend money.
//!
//! The search itself ([`search`]) names no island content. This module builds
//! its domain from the island's data: the trade's recipes at its workplace
//! (a chain of at most `max_depth` recipe levels below the goal), buying
//! inputs from the best seller, gathering the trade's own raw goods (and,
//! for demand elsewhere, a raw input nobody offers), and a
//! missing gathering tool the ledger funds, bought first when the time it
//! saves on the funded units pays for the trip. Goals come from two
//! sources, in [`Scope`] order: the ledger's production targets of the
//! resident (`Own`), and demand elsewhere on the island the trade can serve
//! ([`elsewhere`]): the plan then ends by carrying the goods there.
pub mod elsewhere;
pub mod search;

use super::{ResourcePatch, Villager, Wallet, catalog, economy, market, policy, production};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::crafting::{RecipeDef, RecipeRegistry, Workstation};
use npc_framework::prelude::*;
use search::{Action, Domain, Facts, Limits, Outcome};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PlanStep {
    Buy {
        kind: String,
        target: SimId,
    },
    Gather {
        kind: String,
        target: SimId,
    },
    Craft {
        recipe: String,
        good: String,
        target: SimId,
    },
    /// Carry up to `count` spare units to a store, stall or the trading post
    /// of the demand being served, and lay them there for sale.
    Deliver {
        kind: String,
        count: u32,
        target: SimId,
    },
}

impl PlanStep {
    pub fn label(&self) -> String {
        match self {
            Self::Buy { kind, .. } => format!("buy {kind}"),
            Self::Gather { kind, .. } => format!("gather {kind}"),
            Self::Craft { recipe, .. } => format!("craft {recipe}"),
            Self::Deliver { kind, .. } => format!("deliver {kind}"),
        }
    }
    /// The good this step adds to (or, delivering, takes from) the pack.
    pub fn product(&self) -> &str {
        match self {
            Self::Buy { kind, .. } | Self::Gather { kind, .. } | Self::Deliver { kind, .. } => kind,
            Self::Craft { good, .. } => good,
        }
    }
    pub fn target(&self) -> SimId {
        match self {
            Self::Buy { target, .. }
            | Self::Gather { target, .. }
            | Self::Craft { target, .. }
            | Self::Deliver { target, .. } => *target,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct WorkPlan {
    pub goal: String,
    pub steps: Vec<PlanStep>,
    pub expanded: u32,
    pub cost: u32,
    /// The plan serves funded demand no maker is planned to meet
    /// (`Scope::Elsewhere`), not the resident's own production targets.
    #[serde(default)]
    pub elsewhere: bool,
    /// Where such a plan's goods go: a store or stall of that settlement, or
    /// the trading post. `None` when they stay in the pack for sale (no
    /// outlet there takes them: buyers come to the maker).
    #[serde(default)]
    pub deliver_to: Option<SimId>,
}

/// Which goals a planning leaf considers: `Own` the ledger's production
/// targets of the resident (`life.pursue_goal`), `Elsewhere` demand of other
/// settlements and the boat (`life.pursue_goal elsewhere`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Own,
    Elsewhere,
}

/// A cached plan is searched again after this many ticks at the latest.
pub const PLAN_TTL_TICKS: u64 = 600;

/// Units carried to demand elsewhere on one trip, at most.
pub const DELIVERY_BATCH: u32 = 4;

/// The resident's last searched plan (regional profile only). It is reused while
/// its first step is still pending and that step's preconditions hold, so a long
/// walk to a seller or source no longer repeats the whole search every decision.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct PlanMemo {
    pub plan: WorkPlan,
    pub made_at: u64,
    pub role: super::Role,
    pub workplace: SimId,
    /// Carried units of the first step's product when the plan was made.
    pub head_stock: u32,
}
impl SimHash for PlanMemo {
    const NAME: &'static str = "regional_plan_memo_v2";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.plan.goal);
        h.u32(self.plan.steps.len() as u32);
        for step in &self.plan.steps {
            h.str(&step.label());
            h.u32(step.target().0);
            if let PlanStep::Deliver { count, .. } = step {
                h.u32(*count);
            }
        }
        h.u32(self.plan.expanded);
        h.u32(self.plan.cost);
        h.u8(u8::from(self.plan.elsewhere));
        h.opt(self.plan.deliver_to.as_ref(), |h, id| h.u32(id.0));
        h.u64(self.made_at);
        self.role.sim_hash(h);
        h.u32(self.workplace.0);
        h.u32(self.head_stock);
    }
}

/// Outcome of looking up a resident's cached plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recall {
    /// The cached plan is still executable from its first step.
    Hit(WorkPlan),
    /// Nothing cached (or caching does not apply to this world).
    Miss,
    /// The first step completed or the plan aged out: search again, silently.
    Finished,
    /// A precondition of the first step no longer holds: search again.
    Failed(&'static str),
}

/// Units of `kind` the resident carries beyond what it keeps for itself:
/// what a delivery may take.
pub fn spare(world: &World, actor: Entity, kind: &str) -> u32 {
    economy::count(world, actor, kind).saturating_sub(production::desired_stock(world, actor, kind))
}

/// The cached plan of either scope.
pub fn recall(world: &World, actor: Entity) -> Recall {
    let Some(memo) = world.get::<PlanMemo>(actor) else {
        return Recall::Miss;
    };
    let scope = if memo.plan.elsewhere {
        Scope::Elsewhere
    } else {
        Scope::Own
    };
    recall_in(world, actor, scope)
}

/// The cached plan, when it belongs to `scope`; a plan of the other scope is
/// a miss (and stays cached for its own leaf).
pub fn recall_in(world: &World, actor: Entity, scope: Scope) -> Recall {
    let (Some(memo), Some(person)) = (world.get::<PlanMemo>(actor), world.get::<Villager>(actor))
    else {
        return Recall::Miss;
    };
    if memo.plan.elsewhere != (scope == Scope::Elsewhere) {
        return Recall::Miss;
    }
    let Some(step) = memo.plan.steps.first() else {
        return Recall::Finished;
    };
    let now = world.resource::<SimClock>().tick;
    if now >= memo.made_at.saturating_add(PLAN_TTL_TICKS)
        || economy::count(world, actor, step.product()) != memo.head_stock
    {
        return Recall::Finished;
    }
    if person.role != memo.role || person.workplace != memo.workplace {
        return Recall::Failed("occupation changed");
    }
    // Demand elsewhere is rechecked by the replan at the latest after
    // `PLAN_TTL_TICKS`; recounting the island's stock on every decision of
    // a long walk would cost more than the walk.
    if !memo.plan.elsewhere && production::production_demand(world, actor, &memo.plan.goal) == 0 {
        return Recall::Failed("goal is no longer funded");
    }
    let target = world.resource::<SimIndex>().get(step.target());
    let reachable = |entity: Entity| {
        world
            .get::<SimPos>(entity)
            .is_some_and(|pos| !super::routing::avoiding(world, actor, *pos, Some(step.target())))
    };
    match step {
        PlanStep::Buy { kind, .. } => {
            let Some(seller) = target.filter(|seller| {
                super::living(world, *seller) || super::civic::is_depot(world, *seller)
            }) else {
                return Recall::Failed("seller is gone");
            };
            if !market::sells(world, seller, kind)
                || market::available_to_sell(world, seller, kind) == 0
            {
                return Recall::Failed("offer withdrawn or sold out");
            }
            let coins = world.get::<Wallet>(actor).map_or(0, |wallet| wallet.coins);
            if market::asking_price(world, seller, kind).is_none_or(|price| price > coins) {
                return Recall::Failed("offer is no longer affordable");
            }
            if !reachable(seller) {
                return Recall::Failed("route to the seller was abandoned");
            }
        }
        PlanStep::Gather { kind, .. } => {
            let Some(source) = target else {
                return Recall::Failed("source is gone");
            };
            if world
                .get::<ResourcePatch>(source)
                .is_none_or(|patch| patch.remaining == 0)
            {
                return Recall::Failed("source is depleted");
            }
            if economy::fit(world, actor, &[npc_framework::interaction::Item::new(kind)]).is_err() {
                return Recall::Failed("pack is full");
            }
            if !reachable(source) {
                return Recall::Failed("route to the source was abandoned");
            }
        }
        PlanStep::Craft { recipe, .. } => {
            let Some(def) = world.resource::<RecipeRegistry>().recipes.get(recipe) else {
                return Recall::Failed("recipe is gone");
            };
            if target.is_none() {
                return Recall::Failed("workstation is gone");
            }
            if def
                .inputs
                .iter()
                .any(|input| economy::count(world, actor, &input.key) < input.count)
            {
                return Recall::Failed("craft inputs are missing");
            }
        }
        PlanStep::Deliver { kind, .. } => {
            let Some(place) = target else {
                return Recall::Failed("the store is gone");
            };
            let now = world.resource::<SimClock>().tick;
            let me = world.get::<SimId>(actor).copied();
            if world
                .get::<super::civic::Depot>(place)
                .is_none_or(|depot| !depot.accepts(kind) || depot.room(me, now) == 0)
            {
                return Recall::Failed("the store has no room for the goods");
            }
            if spare(world, actor, kind) == 0 {
                return Recall::Failed("nothing spare to deliver");
            }
            if !reachable(place) {
                return Recall::Failed("route to the store was abandoned");
            }
        }
    }
    Recall::Hit(memo.plan.clone())
}

pub(super) fn remember(world: &mut World, actor: Entity, plan: &WorkPlan) {
    let (Some(step), Some(person)) = (plan.steps.first(), world.get::<Villager>(actor)) else {
        return;
    };
    let memo = PlanMemo {
        plan: plan.clone(),
        made_at: world.resource::<SimClock>().tick,
        role: person.role,
        workplace: person.workplace,
        head_stock: economy::count(world, actor, step.product()),
    };
    if world.get::<PlanMemo>(actor) != Some(&memo) {
        world.entity_mut(actor).insert(memo);
    }
}

pub(super) fn forget(world: &mut World, actor: Entity) {
    if world.get::<PlanMemo>(actor).is_some() {
        world.entity_mut(actor).remove::<PlanMemo>();
    }
}

/// How a goal's purchases are paid for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Funding {
    /// The ledger's targets: buy what `production::purchase_target` funds.
    Ledger,
    /// Demand elsewhere: buy inputs up to what the goods fetch there, never
    /// more coins than that.
    Budget(u32),
}

/// The units a gathering tool's time saving is weighed over: the funded
/// units of the goal, at most `tool_horizon`, and fewer than `max_steps` so
/// the purchase and the gathers fit in one plan.
fn tool_horizon(tuning: &policy::GoapPolicy, units: u32) -> u32 {
    units.clamp(
        1,
        tuning
            .tool_horizon
            .min(tuning.max_steps.saturating_sub(1))
            .max(1),
    )
}

/// The part of a tool's price a plan of `units` gathers of `good` bears, in
/// ticks: the price in the time bare-handed gathering of the good takes to
/// earn it at its quote, spread over the tool's uses (its durability). So a
/// tool is bought when the time it saves on the funded units pays for the
/// trip to the seller and their share of its price.
fn tool_share(
    world: &World,
    actor: Entity,
    tool: &str,
    good: &str,
    seller: Entity,
    units: u32,
) -> u32 {
    let price = u64::from(market::asking_price(world, seller, tool).unwrap_or(u32::MAX));
    let quote = u64::from(market::unit_price(world, good).unwrap_or(1).max(1));
    let base = super::ai::gather_source(world, actor, good)
        .and_then(|(_, patch)| world.get::<ResourcePatch>(patch))
        .map_or(1, |patch| patch.gather_ticks);
    let bare = u64::from(super::living::gather_ticks(world, actor, good, base).max(1));
    let uses = u64::from(catalog::durability_in(world, tool).unwrap_or(1).max(1));
    (price * bare * u64::from(units) / quote / uses).min(u64::from(u32::MAX)) as u32
}

/// Search the prerequisites of one goal: `units` more of `goal` (more than
/// one only to weigh a missing tool). Recipe names, item kinds, source IDs
/// and equal-cost search insertion are all stable.
fn domain(
    world: &World,
    actor: Entity,
    goal: &str,
    units: u32,
    funding: Funding,
    tuning: &policy::GoapPolicy,
) -> Option<Domain<PlanStep>> {
    let person = world.get::<Villager>(actor)?;
    let here = *world.get::<SimPos>(actor)?;
    let station = world.resource::<SimIndex>().get(person.workplace)?;
    let station_pos = *world.get::<SimPos>(station)?;
    let workstation = world.get::<Workstation>(station)?;
    // A pen whose animals are all fed today has no batch to work.
    let pen_full = !super::livestock::can_feed(world, person.workplace);
    let registry = world.resource::<RecipeRegistry>();
    let mut recipes: Vec<&RecipeDef> = catalog::recipes_for(world, person.role)
        .iter()
        .filter_map(|name| registry.recipes.get(*name))
        .filter(|recipe| {
            !pen_full && recipe.hosted_by(&workstation.kinds) && recipe.skill.is_none()
        })
        .collect();
    recipes.sort_by(|a, b| a.name.cmp(&b.name));
    // The goal and the inputs of the recipes that make it, `max_depth`
    // recipe levels deep at most.
    let mut relevant = BTreeSet::from([goal.to_owned()]);
    for _ in 0..tuning.max_depth {
        let old = relevant.len();
        for recipe in &recipes {
            if recipe
                .outputs
                .iter()
                .any(|output| relevant.contains(&output.kind))
            {
                relevant.extend(recipe.inputs.iter().map(|input| input.key.clone()));
                relevant.extend(recipe.tools.iter().cloned());
            }
        }
        if old == relevant.len() {
            break;
        }
    }
    recipes.retain(|recipe| {
        recipe
            .outputs
            .iter()
            .any(|output| relevant.contains(&output.kind))
            && recipe
                .inputs
                .iter()
                .all(|input| relevant.contains(&input.key))
            && recipe.tools.iter().all(|tool| relevant.contains(tool))
    });
    // Raw goods the plan may gather: the trade's own, and, for demand
    // elsewhere (which funds no supplier through the ledger), a raw input
    // nobody offers that needs no tool the resident lacks (a smith cutting
    // the ore for the tools another settlement wants).
    let gather_kinds: Vec<String> = relevant
        .iter()
        .filter(|kind| {
            !recipes
                .iter()
                .any(|recipe| recipe.outputs.iter().any(|output| &output.kind == *kind))
                && (catalog::outputs_for(world, person.role).contains(&kind.as_str())
                    || (matches!(funding, Funding::Budget(_))
                        && kind.as_str() != goal
                        && catalog::gather_tool_required_in(world, kind).is_none_or(|tool| {
                            super::living::usable_tool(world, actor, tool).is_some()
                        })
                        && super::ai::market_supplier(world, actor, kind).is_none()))
        })
        .cloned()
        .collect();
    // A gathering tool the resident lacks for a raw good of the domain, when
    // the ledger funds one (a trade's working tool is standing demand).
    let tool_of = |kind: &str| -> Option<&'static str> {
        let tool = catalog::gather_tool_in(world, kind)?;
        (super::living::usable_tool(world, actor, tool).is_none()
            && funding == Funding::Ledger
            && production::purchase_target(world, actor, tool) > economy::count(world, actor, tool))
        .then_some(tool)
    };
    // Each missing tool, with the raw good it is weighed on (the goal when
    // the tool serves it, else the first in key order).
    let mut tools: BTreeMap<String, String> = BTreeMap::new();
    for kind in &gather_kinds {
        if let Some(tool) = tool_of(kind) {
            let entry = tools.entry(tool.to_owned()).or_insert_with(|| kind.clone());
            if kind == goal {
                entry.clone_from(kind);
            }
        }
    }
    let mut names: BTreeSet<String> = relevant.clone();
    names.extend(tools.keys().cloned());
    let names: Vec<String> = names.into_iter().collect();
    let index: BTreeMap<_, _> = names
        .iter()
        .enumerate()
        .map(|(i, name)| (name.as_str(), i))
        .collect();
    let initial_goods: Vec<_> = names
        .iter()
        .map(|kind| economy::count(world, actor, kind))
        .collect();
    let goal_index = *index.get(goal)?;
    let units = if tools.is_empty() {
        1
    } else {
        tool_horizon(tuning, units)
    };
    let target = initial_goods[goal_index].saturating_add(units);
    let mut ceiling = initial_goods.clone();
    ceiling[goal_index] = target;
    for recipe in &recipes {
        for input in &recipe.inputs {
            let i = index[input.key.as_str()];
            ceiling[i] = ceiling[i].max(input.count.saturating_mul(4));
        }
    }
    for tool in tools.keys() {
        let i = index[tool.as_str()];
        ceiling[i] = ceiling[i].max(1);
    }
    let coins = world.get::<Wallet>(actor)?.coins;
    let mut domain = Domain {
        initial: Facts {
            goods: initial_goods,
            sources: Vec::new(),
            coins: match funding {
                Funding::Ledger => coins,
                Funding::Budget(budget) => coins.min(budget),
            },
            x: here.x,
            z: here.z,
        },
        actions: Vec::new(),
        ceiling,
        goal: goal_index,
        target,
        speed: world
            .get::<Commandable>(actor)
            .map_or(70, |command| command.speed_mm_per_tick.max(1) as u32),
    };
    if npc_framework::agent_io::registry::verb_gate(world, actor, "craft").is_none() {
        for recipe in &recipes {
            domain.actions.push(Action {
                step: PlanStep::Craft {
                    recipe: recipe.name.clone(),
                    good: recipe.outputs[0].kind.clone(),
                    target: person.workplace,
                },
                inputs: recipe
                    .inputs
                    .iter()
                    .map(|input| (index[input.key.as_str()], input.count))
                    .collect(),
                requires: recipe
                    .tools
                    .iter()
                    .map(|tool| (index[tool.as_str()], 1))
                    .collect(),
                outputs: recipe
                    .outputs
                    .iter()
                    .filter_map(|output| {
                        index.get(output.kind.as_str()).map(|i| (*i, output.count))
                    })
                    .collect(),
                source: None,
                price: 0,
                cost: recipe
                    .duration_ticks
                    .max(1)
                    .saturating_mul(tuning.craft_cost),
                pos: station_pos,
            });
        }
    }
    let buying = npc_framework::agent_io::registry::verb_gate(world, actor, "buy").is_none();
    let gathering = npc_framework::agent_io::registry::verb_gate(world, actor, "gather").is_none();
    for (i, kind) in names.iter().enumerate() {
        // Finished commissions must be physically produced, not bought from a
        // competing seller and counted as our own output; nor is the good
        // carried to demand elsewhere bought from somebody else.
        if kind != goal
            && buying
            && let Some(seller) = super::ai::market_supplier(world, actor, kind)
            // Demand elsewhere never buys an input the seller's settlement
            // is itself short of (its staple or feed makers need it).
            && (funding == Funding::Ledger
                || super::stock::settlement_of(world, seller)
                    .is_none_or(|region| !elsewhere::short(world, region, kind)))
        {
            let limit = match funding {
                Funding::Ledger => production::purchase_target(world, actor, kind),
                Funding::Budget(_) => domain.ceiling[i],
            }
            .saturating_sub(domain.initial.goods[i]);
            let stock = market::available_to_sell(world, seller, kind).min(limit);
            if stock > 0 {
                let source = domain.initial.sources.len();
                domain.initial.sources.push(stock);
                domain.actions.push(Action {
                    step: PlanStep::Buy {
                        kind: kind.clone(),
                        target: *world.get::<SimId>(seller)?,
                    },
                    inputs: Vec::new(),
                    requires: Vec::new(),
                    outputs: vec![(i, 1)],
                    source: Some(source),
                    price: market::asking_price(world, seller, kind)?,
                    cost: 20_u32.saturating_mul(tuning.buy_cost).saturating_add(
                        tools.get(kind).map_or(0, |good| {
                            tool_share(world, actor, kind, good, seller, units)
                        }),
                    ),
                    pos: *world.get::<SimPos>(seller)?,
                });
            }
        }
        // Raw goods the trade gathers itself.
        if gather_kinds.contains(kind)
            && gathering
            && let Some((target, source_entity)) = super::ai::gather_source(world, actor, kind)
        {
            let patch = world.get::<ResourcePatch>(source_entity)?;
            let source = domain.initial.sources.len();
            domain.initial.sources.push(patch.remaining);
            let pos = *world.get::<SimPos>(source_entity)?;
            let step = PlanStep::Gather {
                kind: kind.clone(),
                target,
            };
            // As things stand (bare-handed when the tool is missing).
            domain.actions.push(Action {
                step: step.clone(),
                inputs: Vec::new(),
                requires: Vec::new(),
                outputs: vec![(i, 1)],
                source: Some(source),
                price: 0,
                cost: super::living::gather_ticks(world, actor, kind, patch.gather_ticks)
                    .max(1)
                    .saturating_mul(tuning.gather_cost),
                pos,
            });
            // With the missing tool, once bought.
            if let Some(tool) = tool_of(kind) {
                domain.actions.push(Action {
                    step,
                    inputs: Vec::new(),
                    requires: vec![(index[tool], 1)],
                    outputs: vec![(i, 1)],
                    source: Some(source),
                    price: 0,
                    cost: super::living::tooled_gather_ticks(world, actor, patch.gather_ticks)
                        .max(1)
                        .saturating_mul(tuning.gather_cost),
                    pos,
                });
            }
        }
    }
    Some(domain)
}

/// Read-only plan of the resident's own funded production, used by autonomy
/// and the inspector. Deterministic goal priority and bounded search.
pub fn plan(world: &World, actor: Entity) -> Option<WorkPlan> {
    plan_in(world, actor, Scope::Own)
}

/// Read-only plan of one scope.
pub fn plan_in(world: &World, actor: Entity, scope: Scope) -> Option<WorkPlan> {
    match assess(world, actor, scope) {
        PlanAssessment::Ready(plan) => Some(plan),
        _ => None,
    }
}

/// Diagnostic results are derived afresh and never become authoritative state.
enum PlanAssessment {
    Ready(WorkPlan),
    PolicyDisabled,
    ActorUnavailable,
    NoFundedGoal,
    UnavailablePrerequisites,
    SearchLimited,
}

fn limits(tuning: &policy::GoapPolicy) -> Limits {
    Limits {
        max_expansions: tuning.max_expansions,
        max_steps: tuning.max_steps,
    }
}

/// The ledger's goals for the resident's trade, best first.
fn own_goals(world: &World, actor: Entity, role: super::Role) -> Vec<(&'static str, u32)> {
    let mut goals: Vec<_> = catalog::outputs_for(world, role)
        .iter()
        .filter_map(|kind| {
            let demand = production::production_demand(world, actor, kind);
            // Regional: an ingredient of food is as urgent as the food. Without
            // this a farmer with a long order book for fibre never cuts grain,
            // and bread stays unbaked however much is bid for it.
            let essential = catalog::food_restore_in(world, kind).is_some()
                || *kind == super::roles::roles(world).drink
                || super::frontier::economy_spec::registry(world)
                    .is_some_and(|registry| registry.feeds_food(kind));
            // Food-chain goals rank by the food value their demand stands
            // for, not by raw units (S4: twenty berries outranked five grain
            // for ten loaves); other goals by units.
            let weight = catalog::food_value_per_unit(world, kind).max(1);
            (demand > 0).then_some((essential, u64::from(demand) * weight, *kind, demand))
        })
        .collect();
    goals.sort_by(|(ae, ad, ak, _), (be, bd, bk, _)| {
        be.cmp(ae).then_with(|| bd.cmp(ad)).then_with(|| ak.cmp(bk))
    });
    goals
        .into_iter()
        .map(|(_, _, kind, demand)| (kind, demand))
        .collect()
}

fn assess(world: &World, actor: Entity, scope: Scope) -> PlanAssessment {
    let Some(policy) = policy::active(world) else {
        return PlanAssessment::PolicyDisabled;
    };
    let tuning = &policy.goap;
    let Some(person) = world.get::<Villager>(actor) else {
        return PlanAssessment::ActorUnavailable;
    };
    if !super::living(world, actor) {
        return PlanAssessment::ActorUnavailable;
    }
    let mut limited = false;
    // One decision searches at most `decision_expansions` states over all
    // its goals.
    let mut budget = tuning.decision_expansions;
    let mut run =
        |domain: Domain<PlanStep>, budget: &mut u32| -> Option<(Vec<PlanStep>, u32, u32)> {
            if *budget == 0 {
                limited = true;
                return None;
            }
            let bounds = Limits {
                max_expansions: limits(tuning).max_expansions.min(*budget),
                ..limits(tuning)
            };
            let outcome = search::search(&domain, bounds);
            *budget = budget.saturating_sub(outcome.expanded());
            match outcome {
                Outcome::Found(found) => Some((found.steps, found.expanded, found.cost)),
                Outcome::Limited { .. } => {
                    limited = true;
                    None
                }
                Outcome::Unavailable { .. } => None,
            }
        };
    match scope {
        Scope::Own => {
            let goals = own_goals(world, actor, person.role);
            if goals.is_empty() {
                return PlanAssessment::NoFundedGoal;
            }
            for (goal, demand) in goals {
                // Weighing a missing tool plans several units; when they
                // cannot all be made (a patch nearly cut), one still can.
                let tries: &[u32] = if demand > 1 { &[demand, 1] } else { &[1] };
                for units in tries {
                    let Some(problem) = domain(world, actor, goal, *units, Funding::Ledger, tuning)
                    else {
                        break;
                    };
                    let several = problem.target > problem.initial.goods[problem.goal] + 1;
                    if let Some((steps, expanded, cost)) = run(problem, &mut budget) {
                        return PlanAssessment::Ready(WorkPlan {
                            goal: goal.into(),
                            steps,
                            expanded,
                            cost,
                            elsewhere: false,
                            deliver_to: None,
                        });
                    }
                    if !several {
                        break;
                    }
                }
            }
        }
        Scope::Elsewhere => {
            if !tuning.serve_elsewhere {
                return PlanAssessment::NoFundedGoal;
            }
            let goals = elsewhere::goals(world, actor);
            if goals.is_empty() {
                return PlanAssessment::NoFundedGoal;
            }
            let speed = world
                .get::<Commandable>(actor)
                .map_or(70, |command| command.speed_mm_per_tick.max(1) as u32);
            let here = world.get::<SimPos>(actor).copied().unwrap_or_default();
            for remote in goals {
                let carried = spare(world, actor, remote.kind);
                let walk = |from: SimPos| {
                    remote
                        .place
                        .map_or(0, |(_, pos)| search::travel((from.x, from.z), pos, speed))
                };
                let ready = |steps: Vec<PlanStep>, expanded: u32, cost: u32| {
                    PlanAssessment::Ready(WorkPlan {
                        goal: remote.kind.into(),
                        steps,
                        expanded,
                        cost,
                        elsewhere: true,
                        deliver_to: remote.place.map(|(place, _)| place),
                    })
                };
                let Some((place, _)) = remote.place else {
                    // No outlet: the goods stay in the pack for sale. Made
                    // until the pack holds the share.
                    if carried >= remote.units {
                        continue;
                    }
                    let revenue = remote.unit_price.saturating_mul(remote.units - carried);
                    if let Some((steps, expanded, cost)) = domain(
                        world,
                        actor,
                        remote.kind,
                        1,
                        Funding::Budget(revenue),
                        tuning,
                    )
                    .and_then(|problem| run(problem, &mut budget))
                    {
                        return ready(steps, expanded, cost);
                    }
                    continue;
                };
                let batch = remote.units.min(DELIVERY_BATCH);
                let deliver = |count: u32| PlanStep::Deliver {
                    kind: remote.kind.to_owned(),
                    count,
                    target: place,
                };
                // A full load (or what there is, once no more can be made):
                // carry it there.
                if carried >= batch {
                    return ready(vec![deliver(batch)], 0, walk(here));
                }
                let revenue = remote.unit_price.saturating_mul(batch - carried);
                let made = domain(
                    world,
                    actor,
                    remote.kind,
                    1,
                    Funding::Budget(revenue),
                    tuning,
                )
                .and_then(|problem| run(problem, &mut budget));
                if let Some((mut steps, expanded, cost)) = made {
                    let end = steps
                        .last()
                        .and_then(|step| world.resource::<SimIndex>().get(step.target()))
                        .and_then(|entity| world.get::<SimPos>(entity).copied())
                        .unwrap_or(here);
                    steps.push(deliver(batch));
                    return ready(steps, expanded, cost.saturating_add(walk(end)));
                }
                if carried > 0 {
                    return ready(vec![deliver(carried)], 0, walk(here));
                }
            }
        }
    }
    if limited {
        PlanAssessment::SearchLimited
    } else {
        PlanAssessment::UnavailablePrerequisites
    }
}

/// A short key for why the planner has (or has no) plan for this actor, for
/// probes and tests: `plan`, `plan elsewhere`, `no funded goal`,
/// `prerequisites unavailable`, `search limited`, `policy off` or
/// `unavailable`. Read-only.
pub fn diagnose(world: &World, actor: Entity) -> &'static str {
    match assess(world, actor, Scope::Own) {
        PlanAssessment::Ready(_) => "plan",
        PlanAssessment::PolicyDisabled => "policy off",
        PlanAssessment::ActorUnavailable => "unavailable",
        own => match assess(world, actor, Scope::Elsewhere) {
            PlanAssessment::Ready(_) => "plan elsewhere",
            _ => match own {
                PlanAssessment::NoFundedGoal => "no funded goal",
                PlanAssessment::SearchLimited => "search limited",
                _ => "prerequisites unavailable",
            },
        },
    }
}

/// What the resident would plan now for its own production, else, while it
/// is serving demand elsewhere (its cached plan), for that.
fn current(world: &World, actor: Entity) -> PlanAssessment {
    let serving = world
        .get::<PlanMemo>(actor)
        .is_some_and(|memo| memo.plan.elsewhere);
    match assess(world, actor, Scope::Own) {
        PlanAssessment::Ready(plan) => PlanAssessment::Ready(plan),
        own if !serving => own,
        own => match assess(world, actor, Scope::Elsewhere) {
            PlanAssessment::Ready(plan) => PlanAssessment::Ready(plan),
            _ => own,
        },
    }
}

fn place_name(world: &World, place: SimId) -> String {
    world
        .resource::<SimIndex>()
        .get(place)
        .and_then(|entity| world.get::<super::VillagePlace>(entity))
        .map_or_else(|| format!("store {}", place.0), |place| place.label.clone())
}

/// [`explain`] in words a player reads: goods and recipes by their display
/// names from the world's data, never raw keys ("Goal: fiber", "buy thread
/// -> Spin thread"). The raw text stays in [`explain`] for tools.
pub fn explain_readable(world: &World, actor: Entity) -> String {
    let PlanAssessment::Ready(plan) = current(world, actor) else {
        return explain(world, actor);
    };
    let good = |kind: &str| {
        super::catalog::display_in(world, kind)
            .map_or_else(|| kind.replace('_', " "), str::to_owned)
    };
    let recipe = |key: &str| {
        super::frontier::economy_spec::registry(world)
            .and_then(|registry| {
                registry
                    .spec
                    .recipes
                    .iter()
                    .find(|recipe| recipe.key == key)
            })
            .map_or_else(|| key.replace('_', " "), |recipe| recipe.display.clone())
    };
    let steps: Vec<String> = plan
        .steps
        .iter()
        .map(|step| match step {
            PlanStep::Buy { kind, .. } => format!("buy {}", good(kind)),
            PlanStep::Gather { kind, .. } => format!("gather {}", good(kind)),
            PlanStep::Craft { recipe: key, .. } => recipe(key),
            PlanStep::Deliver { kind, target, .. } => {
                format!("deliver {} to {}", good(kind), place_name(world, *target))
            }
        })
        .collect();
    let goal = match (plan.elsewhere, plan.deliver_to) {
        (_, Some(place)) => format!("{} for {}", good(&plan.goal), place_name(world, place)),
        (true, None) => format!("{} for buyers who want it", good(&plan.goal)),
        (false, None) => good(&plan.goal),
    };
    format!(
        "Goal: {goal}
{}",
        steps.join(" -> ")
    )
}

pub fn explain(world: &World, actor: Entity) -> String {
    let plan = match current(world, actor) {
        PlanAssessment::Ready(plan) => plan,
        PlanAssessment::PolicyDisabled => return "Daily-life behavior tree; production ledger".into(),
        PlanAssessment::ActorUnavailable => return "This actor is unavailable for resident work.".into(),
        PlanAssessment::NoFundedGoal => return "No funded production goal currently needs this profession. Shared needs and leisure rules remain active.".into(),
        PlanAssessment::UnavailablePrerequisites => return "No plan is available from the current supplies, purchasing power and permitted recipe prerequisites. The shared work policy remains active.".into(),
        PlanAssessment::SearchLimited => {
            let tuning = &policy::active(world).expect("assessed active policy").goap;
            return format!("Planning reached its search limit ({} expanded states or {} steps). This does not prove the goal is impossible; the shared work policy remains active.", tuning.max_expansions, tuning.max_steps);
        }
    };
    let goal = match (plan.elsewhere, plan.deliver_to) {
        (_, Some(place)) => format!("{} for {}", plan.goal, place.0),
        (true, None) => format!("{} for sale", plan.goal),
        (false, None) => plan.goal.clone(),
    };
    format!(
        "Goal: {goal}\n{}\n{} states searched; cost {}",
        plan.steps
            .iter()
            .map(PlanStep::label)
            .collect::<Vec<_>>()
            .join(" -> "),
        plan.expanded,
        plan.cost
    )
}
