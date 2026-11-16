//! Bounded, deterministic forward planning over actual inventory and finite offers.
//!
//! The production ledger chooses funded goals; this planner searches for a cheap
//! sequence of physical actions. It grants nothing. The first step is rechecked
//! through the same action implementation used by players, then the next life
//! decision replans from the resulting world. Stale offers cannot spend money.
use super::{ResourcePatch, Villager, Wallet, catalog, economy, market, policy, production};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::crafting::{RecipeDef, RecipeRegistry, Workstation};
use npc_framework::prelude::*;
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
}

impl PlanStep {
    pub fn label(&self) -> String {
        match self {
            Self::Buy { kind, .. } => format!("buy {kind}"),
            Self::Gather { kind, .. } => format!("gather {kind}"),
            Self::Craft { recipe, .. } => format!("craft {recipe}"),
        }
    }
    /// The good this step adds to the actor's pack.
    pub fn product(&self) -> &str {
        match self {
            Self::Buy { kind, .. } | Self::Gather { kind, .. } => kind,
            Self::Craft { good, .. } => good,
        }
    }
    pub fn target(&self) -> SimId {
        match self {
            Self::Buy { target, .. } | Self::Gather { target, .. } | Self::Craft { target, .. } => {
                *target
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct WorkPlan {
    pub goal: String,
    pub steps: Vec<PlanStep>,
    pub expanded: u32,
    pub cost: u32,
}

/// A cached plan is searched again after this many ticks at the latest.
pub const PLAN_TTL_TICKS: u64 = 600;

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
    const NAME: &'static str = "regional_plan_memo_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.plan.goal);
        h.u32(self.plan.steps.len() as u32);
        for step in &self.plan.steps {
            h.str(&step.label());
            h.u32(step.target().0);
        }
        h.u32(self.plan.expanded);
        h.u32(self.plan.cost);
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

pub fn recall(world: &World, actor: Entity) -> Recall {
    let (Some(memo), Some(person)) = (world.get::<PlanMemo>(actor), world.get::<Villager>(actor))
    else {
        return Recall::Miss;
    };
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
    if production::production_demand(world, actor, &memo.plan.goal) == 0 {
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Facts {
    goods: Vec<u32>,
    sources: Vec<u32>,
    coins: u32,
    x: i32,
    z: i32,
}

struct Action {
    step: PlanStep,
    inputs: Vec<(usize, u32)>,
    outputs: Vec<(usize, u32)>,
    source: Option<usize>,
    price: u32,
    cost: u32,
    pos: SimPos,
}

struct Domain {
    initial: Facts,
    actions: Vec<Action>,
    ceiling: Vec<u32>,
    goal: usize,
    target: u32,
    speed: u32,
}

struct Node {
    facts: Facts,
    steps: Vec<PlanStep>,
    cost: u32,
}

#[derive(Debug, PartialEq, Eq)]
enum SearchOutcome {
    Found(WorkPlan),
    Limited,
    Unavailable,
}

fn search(domain: &Domain, tuning: &policy::GoapPolicy, goal: &str) -> SearchOutcome {
    let mut frontier = BTreeMap::new();
    let mut visited = BTreeMap::new();
    let mut sequence = 0_u32;
    frontier.insert(
        (0, sequence),
        Node {
            facts: domain.initial.clone(),
            steps: Vec::new(),
            cost: 0,
        },
    );
    visited.insert(domain.initial.clone(), 0);
    let mut expanded = 0;
    let mut step_limited = false;
    while let Some((_, node)) = frontier.pop_first() {
        if node.facts.goods[domain.goal] >= domain.target {
            return SearchOutcome::Found(WorkPlan {
                goal: goal.into(),
                steps: node.steps,
                expanded,
                cost: node.cost,
            });
        }
        if expanded >= tuning.max_expansions {
            return SearchOutcome::Limited;
        }
        expanded += 1;
        if node.steps.len() >= tuning.max_steps as usize {
            step_limited = true;
            continue;
        }
        if visited
            .get(&node.facts)
            .is_some_and(|cost| *cost < node.cost)
        {
            continue;
        }
        for action in &domain.actions {
            if node.facts.coins < action.price
                || action
                    .source
                    .is_some_and(|source| node.facts.sources[source] == 0)
                || action
                    .inputs
                    .iter()
                    .any(|(index, count)| node.facts.goods[*index] < *count)
                || action
                    .outputs
                    .iter()
                    .all(|(index, _)| node.facts.goods[*index] >= domain.ceiling[*index])
            {
                continue;
            }
            let mut facts = node.facts.clone();
            for (index, count) in &action.inputs {
                facts.goods[*index] -= *count;
            }
            for (index, count) in &action.outputs {
                facts.goods[*index] = facts.goods[*index].saturating_add(*count);
            }
            if let Some(source) = action.source {
                facts.sources[source] -= 1;
            }
            facts.coins -= action.price;
            // A conservative Manhattan travel estimate, integer and host-independent.
            let distance = node
                .facts
                .x
                .abs_diff(action.pos.x)
                .saturating_add(node.facts.z.abs_diff(action.pos.z));
            let cost = node
                .cost
                .saturating_add(action.cost)
                .saturating_add(distance / domain.speed.max(1));
            facts.x = action.pos.x;
            facts.z = action.pos.z;
            if visited.get(&facts).is_some_and(|old| *old <= cost) {
                continue;
            }
            visited.insert(facts.clone(), cost);
            let mut steps = node.steps.clone();
            steps.push(action.step.clone());
            sequence = sequence.saturating_add(1);
            frontier.insert((cost, sequence), Node { facts, steps, cost });
        }
    }
    if step_limited {
        SearchOutcome::Limited
    } else {
        SearchOutcome::Unavailable
    }
}

/// Search only the prerequisites of one funded profession goal. Recipe names,
/// item kinds, source IDs and equal-cost search insertion are all stable.
fn domain(world: &World, actor: Entity, goal: &str, tuning: &policy::GoapPolicy) -> Option<Domain> {
    let person = world.get::<Villager>(actor)?;
    let here = *world.get::<SimPos>(actor)?;
    let station = world.resource::<SimIndex>().get(person.workplace)?;
    let station_pos = *world.get::<SimPos>(station)?;
    let workstation = world.get::<Workstation>(station)?;
    let registry = world.resource::<RecipeRegistry>();
    let mut recipes: Vec<&RecipeDef> = catalog::recipes_for(world, person.role)
        .iter()
        .filter_map(|name| registry.recipes.get(*name))
        .filter(|recipe| {
            recipe.hosted_by(&workstation.kinds)
                && recipe.tools.is_empty()
                && recipe.skill.is_none()
        })
        .collect();
    recipes.sort_by(|a, b| a.name.cmp(&b.name));
    let mut relevant = BTreeSet::from([goal.to_owned()]);
    for _ in 0..24 {
        let old = relevant.len();
        for recipe in &recipes {
            if recipe
                .outputs
                .iter()
                .any(|output| relevant.contains(&output.kind))
            {
                relevant.extend(recipe.inputs.iter().map(|input| input.key.clone()));
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
    });
    let names: Vec<_> = relevant.into_iter().collect();
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
    let target = initial_goods[goal_index].saturating_add(1);
    let mut ceiling = initial_goods.clone();
    ceiling[goal_index] = target;
    for recipe in &recipes {
        for input in &recipe.inputs {
            let i = index[input.key.as_str()];
            ceiling[i] = ceiling[i].max(input.count.saturating_mul(4));
        }
    }
    let mut domain = Domain {
        initial: Facts {
            goods: initial_goods,
            sources: Vec::new(),
            coins: world.get::<Wallet>(actor)?.coins,
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
    for (i, kind) in names.iter().enumerate() {
        // Finished commissions must be physically produced, not bought from a
        // competing seller and counted as our own output.
        if kind != goal
            && npc_framework::agent_io::registry::verb_gate(world, actor, "buy").is_none()
            && let Some(seller) = super::ai::market_supplier(world, actor, kind)
        {
            let limit = production::purchase_target(world, actor, kind)
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
                    outputs: vec![(i, 1)],
                    source: Some(source),
                    price: market::asking_price(world, seller, kind)?,
                    cost: 20_u32.saturating_mul(tuning.buy_cost),
                    pos: *world.get::<SimPos>(seller)?,
                });
            }
        }
        if catalog::outputs_for(world, person.role).contains(&kind.as_str())
            && npc_framework::agent_io::registry::verb_gate(world, actor, "gather").is_none()
            && let Some((target, source_entity)) = super::ai::gather_source(world, actor, kind)
        {
            let patch = world.get::<ResourcePatch>(source_entity)?;
            let source = domain.initial.sources.len();
            domain.initial.sources.push(patch.remaining);
            domain.actions.push(Action {
                step: PlanStep::Gather {
                    kind: kind.clone(),
                    target,
                },
                inputs: Vec::new(),
                outputs: vec![(i, 1)],
                source: Some(source),
                price: 0,
                cost: super::living::gather_ticks(world, actor, kind, patch.gather_ticks)
                    .max(1)
                    .saturating_mul(tuning.gather_cost),
                pos: *world.get::<SimPos>(source_entity)?,
            });
        }
    }
    Some(domain)
}

/// Read-only plan used by autonomy and the inspector. Work remains backed by
/// real production demand, with deterministic goal priority and bounded search.
pub fn plan(world: &World, actor: Entity) -> Option<WorkPlan> {
    match assess(world, actor) {
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

fn assess(world: &World, actor: Entity) -> PlanAssessment {
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
    let mut goals: Vec<_> = catalog::outputs_for(world, person.role)
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
            (demand > 0).then_some((essential, u64::from(demand) * weight, *kind))
        })
        .collect();
    goals.sort_by(|(ae, ad, ak), (be, bd, bk)| {
        be.cmp(ae).then_with(|| bd.cmp(ad)).then_with(|| ak.cmp(bk))
    });
    if goals.is_empty() {
        return PlanAssessment::NoFundedGoal;
    }
    let mut limited = false;
    for (_, _, goal) in goals {
        if let Some(domain) = domain(world, actor, goal, tuning) {
            match search(&domain, tuning, goal) {
                SearchOutcome::Found(plan) => return PlanAssessment::Ready(plan),
                SearchOutcome::Limited => limited = true,
                SearchOutcome::Unavailable => {}
            }
        }
    }
    if limited {
        PlanAssessment::SearchLimited
    } else {
        PlanAssessment::UnavailablePrerequisites
    }
}

/// [`explain`] in words a player reads: goods and recipes by their display
/// names from the world's data, never raw keys ("Goal: fiber", "buy thread
/// -> Spin thread"). The raw text stays in [`explain`] for tools.
pub fn explain_readable(world: &World, actor: Entity) -> String {
    let PlanAssessment::Ready(plan) = assess(world, actor) else {
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
        })
        .collect();
    format!(
        "Goal: {}
{}",
        good(&plan.goal),
        steps.join(" -> ")
    )
}

pub fn explain(world: &World, actor: Entity) -> String {
    let plan = match assess(world, actor) {
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
    format!(
        "Goal: {}\n{}\n{} states searched; cost {}",
        plan.goal,
        plan.steps
            .iter()
            .map(PlanStep::label)
            .collect::<Vec<_>>()
            .join(" -> "),
        plan.expanded,
        plan.cost
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example(stock: u32, coins: u32) -> Domain {
        Domain {
            initial: Facts {
                goods: vec![0, 0, 0],
                sources: vec![stock],
                coins,
                x: 0,
                z: 0,
            },
            ceiling: vec![2, 2, 1],
            goal: 2,
            target: 1,
            speed: 70,
            actions: vec![
                Action {
                    step: PlanStep::Buy {
                        kind: "grain".into(),
                        target: SimId(1),
                    },
                    inputs: vec![],
                    outputs: vec![(0, 1)],
                    source: Some(0),
                    price: 3,
                    cost: 1,
                    pos: SimPos::default(),
                },
                Action {
                    step: PlanStep::Craft {
                        recipe: "mill".into(),
                        good: "flour".into(),
                        target: SimId(2),
                    },
                    inputs: vec![(0, 1)],
                    outputs: vec![(1, 1)],
                    source: None,
                    price: 0,
                    cost: 2,
                    pos: SimPos::default(),
                },
                Action {
                    step: PlanStep::Craft {
                        recipe: "bake".into(),
                        good: "bread".into(),
                        target: SimId(2),
                    },
                    inputs: vec![(1, 2)],
                    outputs: vec![(2, 4)],
                    source: None,
                    price: 0,
                    cost: 3,
                    pos: SimPos::default(),
                },
            ],
        }
    }

    #[test]
    fn forward_search_resolves_multistage_preconditions_and_is_deterministic() {
        let domain = example(2, 6);
        let tuning = policy::GoapPolicy::default();
        let SearchOutcome::Found(a) = search(&domain, &tuning, "bread") else {
            panic!("valid plan")
        };
        assert_eq!(a.steps.len(), 5);
        assert!(matches!(&a.steps[0], PlanStep::Buy {kind,..} if kind=="grain"));
        assert!(matches!(&a.steps[4], PlanStep::Craft {recipe,..} if recipe=="bake"));
        assert_eq!(SearchOutcome::Found(a), search(&domain, &tuning, "bread"));
    }

    #[test]
    fn finite_stock_cash_and_search_budgets_are_real_preconditions() {
        let tuning = policy::GoapPolicy::default();
        assert_eq!(
            search(&example(1, 6), &tuning, "bread"),
            SearchOutcome::Unavailable
        );
        assert_eq!(
            search(&example(2, 5), &tuning, "bread"),
            SearchOutcome::Unavailable
        );
        let short = policy::GoapPolicy {
            max_steps: 4,
            ..tuning.clone()
        };
        assert_eq!(
            search(&example(2, 6), &short, "bread"),
            SearchOutcome::Limited
        );
        let bounded = policy::GoapPolicy {
            max_expansions: 1,
            ..tuning
        };
        assert_eq!(
            search(&example(2, 6), &bounded, "bread"),
            SearchOutcome::Limited
        );
    }
}
