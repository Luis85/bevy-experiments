//! Needs-rooted, bounded material planning for the opt-in living economy.
//!
//! Procurement targets and additional production are different quantities. A
//! round reserves personal goods first, then expands funded shortages in reverse
//! recipe order. Each physical or committed output lot is credited only once.
use super::{Role, Season, Villager, Wallet, catalog, economy as econ, market};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::crafting::{Crafting, RecipeDef, RecipeRegistry};
use npc_framework::interaction::items;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::needs::NeedRegistry;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

type Goods = BTreeMap<String, u32>;
type Targets = BTreeMap<SimId, Goods>;
type JobInputs = Vec<(String, u32)>;
type CapitalJob = (u32, u32, String, JobInputs);
const MAX_DEPTH: u32 = 24;

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct ProductionPlan {
    pub refreshed_at: Option<u64>,
    pub procurement: Targets,
    /// Utility/material targets before the purse cap. These are desires, not
    /// funded bids; market recovery may quote a real affordable counter-bid.
    #[serde(default)]
    pub wanted: Targets,
    /// Feasible own-output commissions are material work, not cash bids for
    /// purchasing the finished product from the market.
    #[serde(default)]
    pub self_commissions: Targets,
    /// Unit reservation prices for jointly financed material purchases.
    #[serde(default)]
    pub bid_limits: Targets,
    pub production: Targets,
    /// Outputs already underway when this round was calculated. They were
    /// credited as future supply, and are not additional work in this round.
    pub committed: Targets,
}

fn hash_targets(h: &mut Fnv64, targets: &Targets) {
    h.u32(targets.len() as u32);
    for (id, goods) in targets {
        h.u32(id.0);
        h.u32(goods.len() as u32);
        for (kind, count) in goods {
            h.str(kind);
            h.u32(*count);
        }
    }
}
impl SimHashResource for ProductionPlan {
    const NAME: &'static str = "island_production_plan";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.opt(self.refreshed_at.as_ref(), |h, tick| h.u64(*tick));
        hash_targets(h, &self.procurement);
        hash_targets(h, &self.wanted);
        hash_targets(h, &self.self_commissions);
        hash_targets(h, &self.bid_limits);
        hash_targets(h, &self.production);
        hash_targets(h, &self.committed);
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

/// A public description of actual work, with no wallet or private need values.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct JobIntent {
    pub verb: String,
    pub good: String,
    pub recipe: String,
    pub target: Option<SimId>,
}
impl SimHash for JobIntent {
    const NAME: &'static str = "island_job_intent";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.verb);
        h.str(&self.good);
        h.str(&self.recipe);
        h.opt(self.target.as_ref(), |h, id| h.u32(id.0));
    }
}

pub fn register(app: &mut App) {
    app.init_resource::<ProductionPlan>()
        .register_sim_resource::<ProductionPlan>()
        .register_sim_component::<JobIntent>();
}

pub fn desired_stock(world: &World, actor: Entity, kind: &str) -> u32 {
    target(world, actor, kind, false)
}
pub fn wanted_stock(world: &World, actor: Entity, kind: &str) -> u32 {
    let Some(plan) = world.get_resource::<ProductionPlan>() else {
        return 0;
    };
    world
        .get::<SimId>(actor)
        .and_then(|id| plan.wanted.get(id))
        .and_then(|goods| goods.get(kind))
        .copied()
        .unwrap_or(0)
}
pub fn purchase_wanted(world: &World, actor: Entity, kind: &str) -> u32 {
    let own_work = world
        .get::<SimId>(actor)
        .and_then(|id| {
            world
                .get_resource::<ProductionPlan>()?
                .self_commissions
                .get(id)
        })
        .and_then(|goods| goods.get(kind))
        .copied()
        .unwrap_or(0);
    wanted_stock(world, actor, kind).saturating_sub(own_work)
}
pub fn purchase_target(world: &World, actor: Entity, kind: &str) -> u32 {
    let own_work = world
        .get::<SimId>(actor)
        .and_then(|id| {
            world
                .get_resource::<ProductionPlan>()?
                .self_commissions
                .get(id)
        })
        .and_then(|goods| goods.get(kind))
        .copied()
        .unwrap_or(0);
    desired_stock(world, actor, kind).saturating_sub(own_work)
}
pub fn purchase_bid_limit(world: &World, actor: Entity, kind: &str) -> Option<u32> {
    let id = world.get::<SimId>(actor)?;
    world
        .get_resource::<ProductionPlan>()?
        .bid_limits
        .get(id)?
        .get(kind)
        .copied()
}
pub fn internal_demand(world: &World, actor: Entity, kind: &str) -> u32 {
    let Some(id) = world.get::<SimId>(actor) else {
        return 0;
    };
    world
        .get_resource::<ProductionPlan>()
        .and_then(|plan| plan.self_commissions.get(id))
        .and_then(|goods| goods.get(kind))
        .copied()
        .unwrap_or(0)
}
pub fn production_demand(world: &World, actor: Entity, kind: &str) -> u32 {
    target(world, actor, kind, true)
}

/// Called only after successful physical production, never on accepted work or
/// cancelled/failed actions. Prevents repeating a fulfilled round's job before
/// the next bounded planning refresh.
pub fn completed(world: &mut World, actor: SimId, kind: &str, count: u32) {
    let Some(mut plan) = world.get_resource_mut::<ProductionPlan>() else {
        return;
    };
    let prior = plan
        .committed
        .entry(actor)
        .or_default()
        .entry(kind.into())
        .or_default();
    let credited = (*prior).min(count);
    *prior -= credited;
    let additional = count - credited;
    if let Some(target) = plan
        .production
        .get_mut(&actor)
        .and_then(|goods| goods.get_mut(kind))
    {
        *target = target.saturating_sub(additional);
    }
}
fn target(world: &World, actor: Entity, kind: &str, production: bool) -> u32 {
    let Some(plan) = world.get_resource::<ProductionPlan>() else {
        return 0;
    };
    let Some(id) = world.get::<SimId>(actor) else {
        return 0;
    };
    let targets = if production {
        &plan.production
    } else {
        &plan.procurement
    };
    targets
        .get(id)
        .and_then(|goods| goods.get(kind))
        .copied()
        .unwrap_or(0)
}

pub fn job_summary(world: &World, actor: Entity) -> Option<String> {
    let decision = world.get::<super::behavior::LifeDecision>(actor)?;
    if decision.goal != super::behavior::LifeGoal::Work {
        return None;
    }
    let job = world.get::<JobIntent>(actor)?;
    let good = match job.good.as_str() {
        catalog::GRAIN => "grain",
        catalog::FLOUR => "flour",
        catalog::WATER => "clean water",
        catalog::BOWL => "bowls",
        catalog::RAW_MEAT => "raw meat",
        catalog::HIDE => "hides",
        catalog::LEATHER => "leather",
        catalog::FIBER => "fiber",
        catalog::THREAD => "thread",
        catalog::COOKED_MEAL => "cooked meals",
        catalog::WARM_CLOTHES => "warm clothes",
        catalog::SPEAR => "hunting spears",
        catalog::BERRIES => "berries",
        "bread" => "bread",
        "log" => "logs",
        "plank" => "planks",
        "ore" => "ore",
        "ingot" => "ingots",
        "hammer" => "hammers",
        _ => "supplies",
    };
    Some(match job.verb.as_str() {
        "buy" => format!("Buy {good}"),
        "gather" => format!("Gather {good}"),
        "hunt" => "Hunt wildlife".into(),
        "equip" => "Put on warm clothes".into(),
        "craft" => match job.recipe.as_str() {
            catalog::MILL => "Mill flour",
            catalog::BAKE => "Bake bread",
            catalog::COOK => "Cook meals",
            catalog::SAW => "Saw planks",
            catalog::CARVE => "Carve bowls",
            catalog::SMELT => "Smelt ingots",
            catalog::TOOL => "Forge hammers",
            catalog::FORGE_SPEAR => "Make hunting spears",
            catalog::TAN => "Tan leather",
            catalog::SPIN => "Spin thread",
            catalog::TAILOR => "Sew warm clothes",
            _ => "Craft supplies",
        }
        .into(),
        _ => "Work".into(),
    })
}

pub(super) fn intent(
    world: &mut World,
    actor: Entity,
    verb: &str,
    good: &str,
    recipe: &str,
    target: Option<SimId>,
) {
    let job = JobIntent {
        verb: verb.into(),
        good: good.into(),
        recipe: recipe.into(),
        target,
    };
    if world.get::<JobIntent>(actor) != Some(&job) {
        world.entity_mut(actor).insert(job);
    }
}
pub(super) fn clear_intent(world: &mut World, actor: Entity) {
    world.entity_mut(actor).remove::<JobIntent>();
}

pub fn food_preference(world: &World, actor: Entity) -> &'static str {
    // Item/entity spawn counts must not accidentally give every customer the
    // same diet. Stable profession identities provide a genuine consumer mix.
    if super::needs_driven(world)
        && world
            .get::<Villager>(actor)
            .is_some_and(|person| person.role != Role::Visitor && person.role.tag() % 3 == 0)
    {
        catalog::COOKED_MEAL
    } else {
        "bread"
    }
}

pub fn food_choice(world: &World, actor: Entity) -> Option<&'static str> {
    let preferred = food_preference(world, actor);
    let alternate = if preferred == "bread" {
        catalog::COOKED_MEAL
    } else {
        "bread"
    };
    [preferred, alternate, catalog::BERRIES]
        .into_iter()
        .find(|kind| econ::count(world, actor, kind) > 0)
        .or_else(|| {
            [preferred, alternate, catalog::BERRIES]
                .into_iter()
                .find(|kind| super::ai::market_supplier(world, actor, kind).is_some())
        })
}

pub fn garment_equipped(world: &World, actor: Entity) -> bool {
    super::living::has_coat(world, actor)
}

fn recipes(world: &World) -> Vec<RecipeDef> {
    let registry = world.resource::<RecipeRegistry>();
    let mut names = BTreeSet::new();
    for role in Role::RESIDENTS
        .into_iter()
        .chain([Role::Hunter, Role::Tanner, Role::Tailor])
    {
        names.extend(catalog::recipes_for(world, role).iter().copied());
    }
    names
        .into_iter()
        .filter_map(|name| registry.recipes.get(name).cloned())
        .collect()
}

fn depth(
    kind: &str,
    recipes: &[RecipeDef],
    visiting: &mut BTreeSet<String>,
    depths: &mut BTreeMap<String, u32>,
) -> Result<u32, String> {
    if let Some(value) = depths.get(kind) {
        return Ok(*value);
    }
    if !visiting.insert(kind.into()) {
        return Err(format!("cyclic living-economy material: {kind}"));
    }
    let mut value = 0;
    if matches!(kind, catalog::RAW_MEAT | catalog::HIDE) {
        value = depth(catalog::SPEAR, recipes, visiting, depths)?.saturating_add(1);
    }
    for recipe in recipes
        .iter()
        .filter(|recipe| recipe.outputs.iter().any(|output| output.kind == kind))
    {
        for input in &recipe.inputs {
            value = value.max(depth(&input.key, recipes, visiting, depths)?.saturating_add(1));
        }
    }
    visiting.remove(kind);
    if value > MAX_DEPTH {
        return Err("living-economy material graph exceeds 24 stages".into());
    }
    depths.insert(kind.into(), value);
    Ok(value)
}

/// The host calls this before play. Includes the hunting spear dependency,
/// because hunting is physical production even though it is not a craft recipe.
pub fn validate(world: &World) -> Result<(), String> {
    let recipes = recipes(world);
    let mut depths = BTreeMap::new();
    for recipe in &recipes {
        for output in &recipe.outputs {
            depth(&output.kind, &recipes, &mut BTreeSet::new(), &mut depths)?;
        }
    }
    Ok(())
}

#[derive(Default)]
struct Ledger {
    // Remaining, unallocated lots. Physical and already committed outputs are
    // pooled once; consuming a lot here never grants or transfers a game item.
    lots: BTreeMap<String, BTreeMap<SimId, u32>>,
    sellable: BTreeSet<(SimId, String)>,
    work: BTreeMap<String, BTreeMap<SimId, u32>>,
}
impl Ledger {
    fn own_available(&self, owner: SimId, kind: &str) -> u32 {
        self.lots
            .get(kind)
            .and_then(|owners| owners.get(&owner))
            .copied()
            .unwrap_or(0)
            .saturating_add(
                self.work
                    .get(kind)
                    .and_then(|owners| owners.get(&owner))
                    .copied()
                    .unwrap_or(0),
            )
    }
    fn reserve_work(&mut self, owner: SimId, kind: &str, count: u32) {
        let quantity = self.take_own(owner, kind, count);
        *self
            .work
            .entry(kind.into())
            .or_default()
            .entry(owner)
            .or_default() += quantity;
    }
    fn take_work(&mut self, owner: SimId, kind: &str, count: u32) -> u32 {
        let amount = self
            .work
            .get_mut(kind)
            .and_then(|owners| owners.get_mut(&owner));
        let taken = amount.map_or(0, |amount| {
            let taken = (*amount).min(count);
            *amount -= taken;
            taken
        });
        taken + self.take_own(owner, kind, count - taken)
    }
    fn release_unused_work(&mut self, kind: &str) {
        if let Some(owners) = self.work.remove(kind) {
            for (owner, quantity) in owners {
                self.add(owner, kind, quantity);
            }
        }
    }
    fn add(&mut self, owner: SimId, kind: &str, count: u32) {
        let amount = self
            .lots
            .entry(kind.into())
            .or_default()
            .entry(owner)
            .or_default();
        *amount = amount.saturating_add(count);
    }
    fn take_own(&mut self, owner: SimId, kind: &str, count: u32) -> u32 {
        let Some(lot) = self
            .lots
            .get_mut(kind)
            .and_then(|owners| owners.get_mut(&owner))
        else {
            return 0;
        };
        let taken = (*lot).min(count);
        *lot -= taken;
        taken
    }
    fn take_any(&mut self, kind: &str, count: u32) -> u32 {
        let Some(lots) = self.lots.get_mut(kind) else {
            return 0;
        };
        let mut left = count;
        for (owner, amount) in lots {
            if !self.sellable.contains(&(*owner, kind.into())) {
                continue;
            }
            let taken = (*amount).min(left);
            *amount -= taken;
            left -= taken;
            if left == 0 {
                break;
            }
        }
        count - left
    }
}

struct Builder<'a> {
    world: &'a World,
    people: Vec<(SimId, Entity, Role)>,
    recipes: Vec<RecipeDef>,
    plan: ProductionPlan,
    ledger: Ledger,
    budget: BTreeMap<SimId, u32>,
    requests: BTreeMap<String, u32>,
    work_budget: BTreeMap<SimId, u32>,
    industrial: bool,
}
#[derive(Default)]
struct MaterialBundle {
    leaves: Goods,
    // (amount needed internally, physical recipe yield that must be made)
    internal: BTreeMap<String, (u32, u32)>,
}
impl Builder<'_> {
    fn bundle(&self, actor: SimId, recipe: &RecipeDef, batches: u32) -> MaterialBundle {
        let mut bundle = MaterialBundle::default();
        let mut used = Goods::new();
        for input in &recipe.inputs {
            self.expand_material(
                actor,
                &input.key,
                batches.saturating_mul(input.count),
                &mut used,
                &mut bundle,
                0,
            );
        }
        bundle
    }
    fn expand_material(
        &self,
        actor: SimId,
        kind: &str,
        count: u32,
        used: &mut Goods,
        bundle: &mut MaterialBundle,
        depth: u32,
    ) {
        if count == 0 || depth > MAX_DEPTH {
            return;
        }
        let prior = used.get(kind).copied().unwrap_or(0);
        let owned = count.min(self.ledger.own_available(actor, kind).saturating_sub(prior));
        *used.entry(kind.into()).or_default() += owned;
        *bundle.leaves.entry(kind.into()).or_default() += owned;
        let missing = count - owned;
        if missing == 0 {
            return;
        }
        let role = self
            .people
            .iter()
            .find(|(id, _, _)| *id == actor)
            .map(|(_, _, role)| *role);
        let external = self.ledger.lots.get(kind).is_some_and(|owners| {
            owners.iter().any(|(owner, quantity)| {
                *owner != actor
                    && *quantity > 0
                    && self.ledger.sellable.contains(&(*owner, kind.into()))
            })
        });
        let recipe = role
            .filter(|role| catalog::outputs_for(self.world, *role).contains(&kind))
            .and_then(|role| {
                catalog::recipes_for(self.world, role)
                    .iter()
                    .filter_map(|name| self.recipes.iter().find(|recipe| recipe.name == *name))
                    .find(|recipe| recipe.outputs.iter().any(|output| output.kind == kind))
            });
        if let Some(recipe) = recipe.filter(|_| !external) {
            let output = recipe
                .outputs
                .iter()
                .find(|output| output.kind == kind)
                .expect("matching output");
            let batches = missing.div_ceil(output.count.max(1));
            let value = bundle.internal.entry(kind.into()).or_default();
            value.0 = value.0.saturating_add(missing);
            value.1 = value.1.saturating_add(batches.saturating_mul(output.count));
            for input in &recipe.inputs {
                self.expand_material(
                    actor,
                    &input.key,
                    batches.saturating_mul(input.count),
                    used,
                    bundle,
                    depth + 1,
                );
            }
        } else {
            *bundle.leaves.entry(kind.into()).or_default() += missing;
        }
    }
    fn bundle_cost(&self, actor: SimId, bundle: &MaterialBundle) -> u64 {
        bundle
            .leaves
            .iter()
            .map(|(kind, count)| {
                u64::from(count.saturating_sub(self.ledger.own_available(actor, kind)))
                    * u64::from(market::unit_price(self.world, kind).unwrap_or(u32::MAX))
            })
            .fold(0, u64::saturating_add)
    }
    fn bundle_minimum(&self, actor: SimId, bundle: &MaterialBundle) -> u32 {
        bundle
            .leaves
            .iter()
            .map(|(kind, count)| count.saturating_sub(self.ledger.own_available(actor, kind)))
            .fold(0, u32::saturating_add)
    }
    fn self_production_capacity(&self, actor: SimId, kind: &str, quantity: u32) -> u32 {
        let Some((_, entity, role)) = self.people.iter().find(|(id, _, _)| *id == actor) else {
            return 0;
        };
        if !catalog::outputs_for(self.world, *role).contains(&kind)
            || npc_framework::agent_io::registry::verb_gate(self.world, *entity, "craft").is_some()
        {
            return 0;
        }
        let Some(recipe) = catalog::recipes_for(self.world, *role)
            .iter()
            .filter_map(|name| self.recipes.iter().find(|recipe| recipe.name == *name))
            .find(|recipe| recipe.outputs.iter().any(|output| output.kind == kind))
        else {
            return 0;
        };
        let output = recipe
            .outputs
            .iter()
            .find(|output| output.kind == kind)
            .expect("matching output");
        let budget = self
            .budget
            .get(&actor)
            .copied()
            .unwrap_or(0)
            .saturating_add(self.work_budget.get(&actor).copied().unwrap_or(0));
        let requested = quantity.div_ceil(output.count.max(1)).min(4);
        let current = (1..=requested).rev().find(|batches| {
            self.bundle_cost(actor, &self.bundle(actor, recipe, *batches)) <= u64::from(budget)
        });
        let batches = current.unwrap_or_else(|| {
            if requested > 0 && self.bundle_minimum(actor, &self.bundle(actor, recipe, 1)) <= budget
            {
                1
            } else {
                0
            }
        });
        batches.saturating_mul(output.count).min(quantity)
    }
    fn request(&mut self, actor: SimId, kind: &str, quantity: u32) {
        if quantity == 0 {
            return;
        }
        let wanted = self
            .plan
            .wanted
            .entry(actor)
            .or_default()
            .entry(kind.into())
            .or_default();
        *wanted = wanted.saturating_add(quantity);
        let own = if self.industrial {
            self.ledger.take_work(actor, kind, quantity)
        } else {
            self.ledger.take_own(actor, kind, quantity)
        };
        let missing = quantity - own;
        // A producer need not buy her own finished product before making it.
        // Only an actually owned/financeable input bundle backs this branch;
        // its missing material payments are reserved later, exactly once.
        let self_made = self.self_production_capacity(actor, kind, missing);
        if self_made > 0 {
            let work = self
                .plan
                .self_commissions
                .entry(actor)
                .or_default()
                .entry(kind.into())
                .or_default();
            *work = work.saturating_add(self_made);
        }
        let price = market::unit_price(self.world, kind)
            .unwrap_or(u32::MAX)
            .max(1);
        let budget = self.budget.entry(actor).or_default();
        let buying = missing.saturating_sub(self_made);
        let mut funded = buying.min(*budget / price);
        if funded == 0 && buying > 0 && *budget > 0 {
            // A one-unit household counter-bid is real cash, not fictitious
            // executable demand at a quote the actor cannot afford.
            let limit = *budget;
            self.plan
                .bid_limits
                .entry(actor)
                .or_default()
                .entry(kind.into())
                .and_modify(|prior| *prior = (*prior).min(limit))
                .or_insert(limit);
            funded = 1;
            *budget = 0;
        } else {
            *budget -= funded * price;
        }
        let target = self
            .plan
            .procurement
            .entry(actor)
            .or_default()
            .entry(kind.into())
            .or_default();
        *target = target
            .saturating_add(own)
            .saturating_add(funded)
            .saturating_add(self_made);
        let requests = self.requests.entry(kind.into()).or_default();
        *requests = requests.saturating_add(funded).saturating_add(self_made);
    }
    fn finance_bundle(&mut self, actor: SimId, bundle: MaterialBundle) -> bool {
        let available = self.budget.get(&actor).copied().unwrap_or(0);
        let mut missing = Goods::new();
        for (kind, count) in &bundle.leaves {
            let quantity = count.saturating_sub(self.ledger.own_available(actor, kind));
            if quantity > 0 {
                missing.insert(kind.clone(), quantity);
            }
        }
        let quoted = missing
            .iter()
            .map(|(kind, count)| {
                u64::from(*count)
                    * u64::from(market::unit_price(self.world, kind).unwrap_or(u32::MAX))
            })
            .fold(0_u64, u64::saturating_add);
        let slack = missing
            .iter()
            .map(|(kind, count)| {
                u64::from(*count)
                    * u64::from((market::unit_price(self.world, kind).unwrap_or(1) / 16).max(1))
            })
            .fold(0_u64, u64::saturating_add);
        let budget = if quoted <= u64::from(available) {
            u64::from(available).min(quoted.saturating_add(slack))
        } else {
            u64::from(available)
        };
        let mut limits = Goods::new();
        let mut spent = 0_u64;
        for (kind, count) in &missing {
            let commissioned = self
                .plan
                .self_commissions
                .get(&actor)
                .and_then(|goods| goods.get(kind))
                .copied()
                .unwrap_or(0);
            let target = self
                .plan
                .procurement
                .get(&actor)
                .and_then(|goods| goods.get(kind))
                .copied()
                .unwrap_or(0)
                .saturating_sub(commissioned);
            let owned = self
                .people
                .iter()
                .find(|(id, _, _)| *id == actor)
                .map_or(0, |(_, entity, _)| econ::count(self.world, *entity, kind));
            let existing_paid = target > owned;
            let prior = self
                .plan
                .bid_limits
                .get(&actor)
                .and_then(|goods| goods.get(kind))
                .copied()
                .unwrap_or_else(|| {
                    if existing_paid {
                        market::unit_price(self.world, kind).unwrap_or(u32::MAX)
                    } else {
                        1
                    }
                });
            let floor = if quoted <= u64::from(available) {
                prior.max(market::unit_price(self.world, kind).unwrap_or(u32::MAX))
            } else {
                prior
            };
            limits.insert(kind.clone(), floor);
            spent = spent.saturating_add(u64::from(*count) * u64::from(floor));
        }
        if spent > budget {
            for (kind, count) in &bundle.leaves {
                *self
                    .plan
                    .wanted
                    .entry(actor)
                    .or_default()
                    .entry(kind.clone())
                    .or_default() += *count;
            }
            return false;
        }
        let extra = budget - spent;
        let weight = quoted.max(1);
        for (kind, count) in &missing {
            let share = extra.saturating_mul(
                u64::from(*count) * u64::from(market::unit_price(self.world, kind).unwrap_or(1)),
            ) / weight;
            let increase = (share / u64::from(*count)).min(u64::from(u32::MAX)) as u32;
            *limits.get_mut(kind).expect("missing limit") += increase;
            spent += u64::from(*count) * u64::from(increase);
        }
        for (kind, count) in &missing {
            if budget - spent >= u64::from(*count) {
                *limits.get_mut(kind).expect("missing limit") += 1;
                spent += u64::from(*count);
            }
        }
        // These caps finance the WHOLE joined bundle. No ingredient can spend
        // the complementary ingredient's reserved money, including repricing.
        *self.budget.entry(actor).or_default() -= spent as u32;
        for (kind, count) in bundle.leaves {
            let own = self.ledger.take_work(actor, &kind, count);
            let buying = count - own;
            *self
                .plan
                .wanted
                .entry(actor)
                .or_default()
                .entry(kind.clone())
                .or_default() += count;
            *self
                .plan
                .procurement
                .entry(actor)
                .or_default()
                .entry(kind.clone())
                .or_default() += count;
            if buying > 0 {
                let limit = limits[&kind];
                self.plan
                    .bid_limits
                    .entry(actor)
                    .or_default()
                    .entry(kind.clone())
                    .and_modify(|prior| *prior = (*prior).min(limit))
                    .or_insert(limit);
                *self.requests.entry(kind).or_default() += buying;
            }
        }
        for (kind, (needed, produced)) in bundle.internal {
            *self
                .plan
                .wanted
                .entry(actor)
                .or_default()
                .entry(kind.clone())
                .or_default() += needed;
            *self
                .plan
                .procurement
                .entry(actor)
                .or_default()
                .entry(kind.clone())
                .or_default() += needed;
            *self
                .plan
                .self_commissions
                .entry(actor)
                .or_default()
                .entry(kind.clone())
                .or_default() += needed;
            *self
                .plan
                .production
                .entry(actor)
                .or_default()
                .entry(kind.clone())
                .or_default() += produced;
            self.ledger
                .add(actor, &kind, produced.saturating_sub(needed));
        }
        true
    }
    fn producer(&self, kind: &str) -> Option<(SimId, Entity, Role)> {
        self.people
            .iter()
            .copied()
            .find(|(_, _, role)| catalog::outputs_for(self.world, *role).contains(&kind))
    }
    fn reserve_working_capital(
        &mut self,
        needed: &BTreeSet<String>,
        depths: &BTreeMap<String, u32>,
    ) {
        for (id, entity, role) in self.people.clone() {
            let budget = self.budget.get(&id).copied().unwrap_or(0);
            let mut jobs: Vec<CapitalJob> = Vec::new();
            if npc_framework::agent_io::registry::verb_gate(self.world, entity, "craft").is_none() {
                for name in catalog::recipes_for(self.world, role) {
                    let Some(recipe) = self.recipes.iter().find(|recipe| recipe.name == *name)
                    else {
                        continue;
                    };
                    let stage = recipe
                        .outputs
                        .iter()
                        .filter(|output| needed.contains(&output.kind))
                        .filter_map(|output| depths.get(&output.kind))
                        .copied()
                        .max();
                    let Some(stage) = stage else {
                        continue;
                    };
                    if recipe
                        .tools
                        .iter()
                        .any(|tool| items::carried_matching(self.world, entity, tool).is_none())
                    {
                        continue;
                    }
                    let bundle = self.bundle(id, recipe, 1);
                    let quoted = self.bundle_cost(id, &bundle);
                    if self.bundle_minimum(id, &bundle) <= budget {
                        jobs.push((
                            stage,
                            quoted.min(u64::from(budget)) as u32,
                            recipe.name.clone(),
                            bundle.leaves.into_iter().collect(),
                        ));
                    }
                }
            }
            if role == Role::Hunter
                && (needed.contains(catalog::RAW_MEAT) || needed.contains(catalog::HIDE))
            {
                let cost = if self.ledger.own_available(id, catalog::SPEAR) > 0 {
                    0
                } else {
                    market::unit_price(self.world, catalog::SPEAR).unwrap_or(u32::MAX)
                };
                if cost <= budget || budget > 0 {
                    jobs.push((
                        depths.get(catalog::HIDE).copied().unwrap_or(0),
                        cost.min(budget),
                        "hunt".into(),
                        vec![(catalog::SPEAR.into(), 1)],
                    ));
                }
            }
            jobs.sort_by(|(a, ac, an, _), (b, bc, bn, _)| {
                b.cmp(a).then_with(|| ac.cmp(bc)).then_with(|| an.cmp(bn))
            });
            if let Some((_, cost, _, inputs)) = jobs.into_iter().next() {
                *self.budget.get_mut(&id).expect("resident budget") -= cost;
                self.work_budget.insert(id, cost);
                for (kind, count) in inputs {
                    self.ledger.reserve_work(id, &kind, count);
                }
            }
        }
    }
    fn manufacture(&mut self, kind: &str, missing: u32) {
        let Some((id, entity, role)) = self.producer(kind) else {
            return;
        };
        if matches!(kind, catalog::RAW_MEAT | catalog::HIDE) {
            let per_hunt = if kind == catalog::RAW_MEAT { 2 } else { 1 };
            let price = market::unit_price(self.world, catalog::SPEAR)
                .unwrap_or(u32::MAX)
                .max(1);
            let cash = self.budget.get(&id).copied().unwrap_or(0);
            let owned = self.ledger.own_available(id, catalog::SPEAR);
            let financed = owned.saturating_add(cash / price).max(u32::from(cash > 0));
            let hunts = missing.div_ceil(per_hunt).min(4).min(financed);
            if hunts == 0 {
                return;
            }
            let bundle = MaterialBundle {
                leaves: [(catalog::SPEAR.into(), hunts)].into(),
                ..Default::default()
            };
            if !self.finance_bundle(id, bundle) {
                return;
            }
            *self
                .plan
                .production
                .entry(id)
                .or_default()
                .entry(kind.into())
                .or_default() += hunts * per_hunt;
            let other = if kind == catalog::RAW_MEAT {
                catalog::HIDE
            } else {
                catalog::RAW_MEAT
            };
            self.ledger.add(
                id,
                other,
                hunts * if other == catalog::RAW_MEAT { 2 } else { 1 },
            );
            return;
        }
        let recipe = catalog::recipes_for(self.world, role)
            .iter()
            .filter_map(|name| self.recipes.iter().find(|recipe| recipe.name == *name))
            .find(|recipe| recipe.outputs.iter().any(|output| output.kind == kind))
            .cloned();
        if let Some(recipe) = recipe {
            if npc_framework::agent_io::registry::verb_gate(self.world, entity, "craft").is_some() {
                return;
            }
            let output = recipe
                .outputs
                .iter()
                .find(|output| output.kind == kind)
                .expect("matching output");
            let requested = missing.div_ceil(output.count.max(1)).min(4);
            let budget = self.budget.get(&id).copied().unwrap_or(0);
            let batches = (1..=requested)
                .rev()
                .find(|batches| {
                    self.bundle_cost(id, &self.bundle(id, &recipe, *batches)) <= u64::from(budget)
                })
                .unwrap_or(1);
            let bundle = self.bundle(id, &recipe, batches);
            if !self.finance_bundle(id, bundle) {
                return;
            }
            let produced = batches.saturating_mul(output.count);
            *self
                .plan
                .production
                .entry(id)
                .or_default()
                .entry(kind.into())
                .or_default() += produced;
            self.ledger.add(id, kind, produced.saturating_sub(missing));
            for output in recipe.outputs.iter().filter(|output| output.kind != kind) {
                self.ledger
                    .add(id, &output.kind, batches.saturating_mul(output.count));
            }
        } else {
            self.plan
                .production
                .entry(id)
                .or_default()
                .insert(kind.into(), missing);
        }
    }
}

fn forecast(world: &World, actor: Entity, need: &str, restoration: u32) -> u32 {
    forecast_amount(world, actor, need)
        .div_ceil(u64::from(restoration.max(1)))
        .min(16) as u32
}

fn forecast_amount(world: &World, actor: Entity, need: &str) -> u64 {
    let registry = world.resource::<NeedRegistry>();
    let decay = registry
        .id(need)
        .map_or(0, |id| registry.def(id).decay_per_tick.max(0) as u64);
    let horizon = world.resource::<SimConfig>().day_length.saturating_mul(2);
    let depleted = (100_000 - econ::need(world, actor, need)).max(0) as u64;
    depleted.saturating_add(decay.saturating_mul(horizon))
}

fn needed_materials(
    world: &World,
    people: &[(SimId, Entity, Role)],
    recipes: &[RecipeDef],
    winter_soon: bool,
) -> BTreeSet<String> {
    let mut needed = BTreeSet::new();
    for (_, actor, role) in people {
        let owned_food = ["bread", catalog::COOKED_MEAL, catalog::BERRIES]
            .into_iter()
            .map(|kind| {
                u64::from(econ::count(world, *actor, kind))
                    * catalog::food_restore(kind).unwrap_or(0).max(0) as u64
            })
            .sum::<u64>();
        if owned_food < forecast_amount(world, *actor, "Hunger") {
            needed.insert(food_preference(world, *actor).into());
        }
        if econ::count(world, *actor, catalog::WATER) < forecast(world, *actor, "Thirst", 90_000) {
            needed.insert(catalog::WATER.into());
        }
        if (winter_soon || econ::need(world, *actor, "Warmth") < 80_000)
            && !garment_equipped(world, *actor)
            && econ::count(world, *actor, catalog::WARM_CLOTHES) == 0
        {
            needed.insert(catalog::WARM_CLOTHES.into());
        }
        if *role == Role::Innkeeper {
            for (kind, count) in [("log", 2), (catalog::BOWL, 1), ("hammer", 1)] {
                if econ::count(world, *actor, kind) < count {
                    needed.insert(kind.into());
                }
            }
        }
    }
    for _ in 0..=MAX_DEPTH {
        let previous = needed.len();
        for recipe in recipes {
            if recipe
                .outputs
                .iter()
                .any(|output| needed.contains(&output.kind))
            {
                needed.extend(recipe.inputs.iter().map(|input| input.key.clone()));
            }
        }
        if needed.contains(catalog::RAW_MEAT) || needed.contains(catalog::HIDE) {
            needed.insert(catalog::SPEAR.into());
        }
        if needed.len() == previous {
            break;
        }
    }
    needed
}

fn compute(world: &World) -> ProductionPlan {
    let people: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, e)| {
            let person = world.get::<Villager>(*e)?;
            (person.role != Role::Visitor && super::living(world, *e)).then_some((
                *id,
                *e,
                person.role,
            ))
        })
        .collect();
    let mut builder = Builder {
        world,
        people: people.clone(),
        recipes: recipes(world),
        plan: ProductionPlan {
            refreshed_at: Some(world.resource::<SimClock>().tick),
            ..Default::default()
        },
        ledger: Ledger::default(),
        budget: BTreeMap::new(),
        requests: BTreeMap::new(),
        work_budget: BTreeMap::new(),
        industrial: false,
    };
    for (id, e, role) in &people {
        for kind in catalog::outputs_for(world, *role) {
            builder.ledger.sellable.insert((*id, (*kind).into()));
        }
        builder.budget.insert(
            *id,
            world.get::<Wallet>(*e).map_or(0, |wallet| wallet.coins),
        );
        for (_, item) in econ::free_items(world, *e) {
            builder.ledger.add(*id, &item.kind, item.stack);
        }
        if let Some(craft) = world.get::<Crafting>(*e)
            && let Some(recipe) = world
                .resource::<RecipeRegistry>()
                .recipes
                .get(&craft.recipe)
        {
            for output in &recipe.outputs {
                builder.ledger.add(*id, &output.kind, output.count);
                builder
                    .plan
                    .committed
                    .entry(*id)
                    .or_default()
                    .insert(output.kind.clone(), output.count);
            }
        }
        if let Some(gather) = world.get::<super::Gathering>(*e)
            && let Some(patch) = world
                .resource::<SimIndex>()
                .get(gather.patch)
                .and_then(|p| world.get::<super::ResourcePatch>(p))
        {
            builder.ledger.add(*id, &patch.kind, 1);
            builder
                .plan
                .committed
                .entry(*id)
                .or_default()
                .insert(patch.kind.clone(), 1);
        }
        if world.get::<super::hunting::Hunting>(*e).is_some() {
            builder.ledger.add(*id, catalog::RAW_MEAT, 2);
            builder.ledger.add(*id, catalog::HIDE, 1);
            builder
                .plan
                .committed
                .entry(*id)
                .or_default()
                .insert(catalog::RAW_MEAT.into(), 2);
            builder
                .plan
                .committed
                .entry(*id)
                .or_default()
                .insert(catalog::HIDE.into(), 1);
        }
    }
    // Explicit player offers are usable supply, never an inspection of an
    // unlisted visitor's private inventory.
    for (id, e) in &world.resource::<SimIndex>().map {
        if world
            .get::<Villager>(*e)
            .is_some_and(|v| v.role == Role::Visitor)
        {
            for kind in catalog::trade_kinds(world) {
                if market::asking_price(world, *e, kind) > market::unit_price(world, kind) {
                    continue;
                }
                let amount = market::available_to_sell(world, *e, kind);
                if amount > 0 {
                    builder.ledger.add(*id, kind, amount);
                    builder.ledger.sellable.insert((*id, (*kind).into()));
                }
            }
        }
    }
    let (day, season, _) = super::calendar(world);
    let winter_soon = season == Season::Winter || ((day + 2) / super::DAYS_PER_SEASON) % 4 == 3;
    let mut depths = BTreeMap::new();
    for kind in catalog::trade_kinds(world) {
        if depth(kind, &builder.recipes, &mut BTreeSet::new(), &mut depths).is_err() {
            return ProductionPlan {
                refreshed_at: Some(world.resource::<SimClock>().tick),
                ..Default::default()
            };
        }
    }
    // Executable, urgent bodily needs go first. Future two-day shopping must
    // not consume the same cash and material lots needed to keep makers alive.
    let mut critical = Targets::new();
    for (id, e, _) in &people {
        if econ::need(world, *e, "Thirst") < 55_000 {
            builder.request(*id, catalog::WATER, 1);
            critical
                .entry(*id)
                .or_default()
                .insert(catalog::WATER.into(), 1);
        }
        if econ::need(world, *e, "Hunger") < 50_000 {
            let food = food_choice(world, *e).unwrap_or_else(|| food_preference(world, *e));
            builder.request(*id, food, 1);
            critical.entry(*id).or_default().insert(food.into(), 1);
        }
    }
    let needed = needed_materials(world, &people, &builder.recipes, winter_soon);
    builder.reserve_working_capital(&needed, &depths);
    for (id, e, role) in &people {
        let primary = food_preference(world, *e);
        let alternate = if primary == "bread" {
            catalog::COOKED_MEAL
        } else {
            "bread"
        };
        let provisioned = critical.get(id);
        let water = provisioned
            .and_then(|goods| goods.get(catalog::WATER))
            .copied()
            .unwrap_or(0);
        builder.request(
            *id,
            catalog::WATER,
            forecast(world, *e, "Thirst", 90_000).saturating_sub(water),
        );
        let mut hunger = forecast_amount(world, *e, "Hunger");
        if let Some(goods) = provisioned {
            for (kind, count) in goods {
                if let Some(restoration) = catalog::food_restore(kind) {
                    hunger = hunger.saturating_sub(u64::from(*count) * restoration.max(0) as u64);
                }
            }
        }
        for kind in [primary, alternate, catalog::BERRIES] {
            let restoration = catalog::food_restore(kind).unwrap_or(1).max(1) as u64;
            let critical = provisioned
                .and_then(|goods| goods.get(kind))
                .copied()
                .unwrap_or(0);
            let reserve = econ::count(world, *e, kind)
                .saturating_sub(critical)
                .min(hunger.div_ceil(restoration).min(16) as u32);
            builder.request(*id, kind, reserve);
            hunger = hunger.saturating_sub(u64::from(reserve).saturating_mul(restoration));
        }
        let restoration = catalog::food_restore(primary).unwrap_or(80_000).max(1) as u64;
        builder.request(*id, primary, hunger.div_ceil(restoration).min(16) as u32);
        if (winter_soon || econ::need(world, *e, "Warmth") < 80_000) && !garment_equipped(world, *e)
        {
            builder.request(*id, catalog::WARM_CLOTHES, 1);
        }
        if *role == Role::Innkeeper {
            builder.request(*id, "log", 2);
            builder.request(*id, catalog::BOWL, 1);
            builder.request(*id, "hammer", 1);
        }
    }
    // Escrow is virtual allocation only. Release its actual cash for the
    // balanced material requests below; no wallet or item has been changed.
    for (id, cash) in std::mem::take(&mut builder.work_budget) {
        *builder.budget.entry(id).or_default() += cash;
    }
    builder.industrial = true;
    let mut order: Vec<_> = depths.into_iter().collect();
    order.sort_by(|(a, da), (b, db)| db.cmp(da).then_with(|| a.cmp(b)));
    for (kind, _) in order {
        let requested = builder.requests.remove(&kind).unwrap_or(0);
        builder.ledger.release_unused_work(&kind);
        let supplied = builder.ledger.take_any(&kind, requested);
        let missing = requested - supplied;
        if missing > 0 {
            builder.manufacture(&kind, missing);
        }
    }
    builder.plan
}

pub fn refresh(world: &mut World) {
    if !super::needs_driven(world) {
        return;
    }
    let now = world.resource::<SimClock>().tick;
    let interval = (world.resource::<SimConfig>().day_length / 8).clamp(40, 300);
    if world
        .get_resource::<ProductionPlan>()
        .and_then(|plan| plan.refreshed_at)
        .is_some_and(|last| now < last.saturating_add(interval))
    {
        return;
    }
    let plan = compute(world);
    world.insert_resource(plan);
}
