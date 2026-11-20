//! Needs-rooted, bounded material planning for the opt-in living economy.
//!
//! Procurement targets and additional production are different quantities. A
//! round reserves personal goods first, then expands funded shortages in reverse
//! recipe order. Each physical or committed output lot is credited only once.
use super::roles::{Roles, roles};
use super::{Role, Villager, Wallet, catalog, economy as econ, market};
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
    /// The tick the current refresh cycle started.
    pub refreshed_at: Option<u64>,
    /// Settlements of the current cycle still to plan, one a tick in
    /// identity order (S4: planning all of them in one tick cost up to 26 ms
    /// every 300 ticks).
    #[serde(default)]
    pub pending: Vec<SimId>,
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
        h.u32(self.pending.len() as u32);
        for id in &self.pending {
            h.u32(id.0);
        }
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

/// What staple makers owe the civic fund for advances on their standing work
/// (closing design). The fund advances at most `staple_advance_per_day` a day
/// and `staple_advance_per_maker` to one maker; a maker repays half of every
/// sale's proceeds until it owes nothing. Coins only move between the fund
/// and the maker, so the island's coins stay exactly the same.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StapleAdvances {
    pub owed: BTreeMap<SimId, u32>,
    /// Coins advanced and repaid since the founding.
    pub advanced: u64,
    pub repaid: u64,
}
impl SimHashResource for StapleAdvances {
    const NAME: &'static str = "island_staple_advances_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.owed.len() as u32);
        for (id, coins) in &self.owed {
            h.u32(id.0);
            h.u32(*coins);
        }
        h.u64(self.advanced);
        h.u64(self.repaid);
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

/// Whether `kind` is a planned staple or goes into one (up to two recipes
/// deep): grain and flour for bread, thread and leather for the coat.
pub fn feeds_staple(world: &World, kind: &str) -> bool {
    catalog::trade_kinds(world).iter().any(|staple| {
        super::stock::planned_staple(world, staple) && feeds_good(world, kind, staple, 2)
    })
}

/// Whether `kind` is `staple` or goes into it, up to two recipes deep.
pub(crate) fn feeds_good(world: &World, kind: &str, staple: &str, depth: u32) -> bool {
    if kind == staple {
        return true;
    }
    if depth == 0 {
        return false;
    }
    super::frontier::economy_spec::registry(world).is_some_and(|registry| {
        registry
            .spec
            .recipes
            .iter()
            .filter(|recipe| recipe.inputs.iter().any(|input| input.good == kind))
            .flat_map(|recipe| recipe.outputs.iter())
            .any(|output| feeds_good(world, &output.good, staple, depth - 1))
    })
}

/// The planned staples short in `settlement`: less on offer than the
/// planned stock.
pub(crate) fn short_staples(world: &World, settlement: SimId) -> Vec<&'static str> {
    catalog::trade_kinds(world)
        .iter()
        .copied()
        .filter(|staple| {
            super::stock::planned_staple(world, staple)
                && super::stock::on_offer(world, settlement, staple)
                    < super::stock::target(world, settlement, staple)
        })
        .collect()
}

/// Coins `actor` lacks to buy the inputs of one batch of `kind` at today's
/// quotes, beyond what it holds of them (the cheapest recipe of `kind` its
/// trade makes whose inputs it holds or can buy now: what it lacks of each is
/// on offer in its settlement); 0 when it can pay, when `kind` is not made
/// from inputs, or when no recipe's inputs can be had (an advance would buy
/// nothing). A recipe whose input nobody sells made the batch look cheap: a
/// keeper short of grain "could afford" two coins of turnips in a winter
/// with no turnip on offer, and got no advance.
fn batch_shortfall(world: &World, actor: Entity, kind: &str) -> u32 {
    let Some(registry) = super::frontier::economy_spec::registry(world) else {
        return 0;
    };
    let coins = world.get::<Wallet>(actor).map_or(0, |wallet| wallet.coins);
    let settlement = economic_settlement(world, actor);
    let obtainable = |recipe: &&super::frontier::economy_spec::RecipeSpec| {
        recipe.inputs.iter().all(|input| {
            let missing = input
                .count
                .saturating_sub(econ::count(world, actor, &input.good));
            missing == 0
                || settlement.is_some_and(|region| {
                    super::stock::on_offer(world, region, &input.good) >= missing
                })
        })
    };
    registry
        .spec
        .recipes
        .iter()
        .filter(|recipe| recipe.outputs.iter().any(|output| output.good == kind))
        .filter(obtainable)
        .map(|recipe| {
            recipe
                .inputs
                .iter()
                .map(|input| {
                    let missing =
                        input
                            .count
                            .saturating_sub(econ::count(world, actor, &input.good));
                    missing.saturating_mul(market::unit_price(world, &input.good).unwrap_or(1))
                })
                .fold(0_u32, u32::saturating_add)
        })
        .min()
        .map_or(0, |cost| cost.saturating_sub(coins))
}

/// Daily, after the floor grants: the fund advances staple makers whose
/// purse cannot pay for the inputs of a batch of their standing work, within
/// the caps (closing design). A maker qualifies when its trade makes from
/// inputs a good that is, or goes into, a planned staple short in its
/// settlement (S4 fix: the gate looked for a staple entry in the production
/// plan, which only a maker who could already pay for its inputs ever got,
/// so no advance was ever made).
pub fn advance_staple_makers(world: &mut World, people: &[(SimId, Entity)]) {
    let Some((per_day, per_maker)) = super::frontier::economy_spec::tuning(world).map(|tuning| {
        (
            tuning.staple_advance_per_day,
            tuning.staple_advance_per_maker,
        )
    }) else {
        return;
    };
    if per_day == 0 || per_maker == 0 {
        return;
    }
    let mut short: BTreeMap<SimId, Vec<&'static str>> = BTreeMap::new();
    let mut makers: Vec<(SimId, Entity, u32)> = Vec::new();
    for (id, actor) in people.iter().copied() {
        if !super::living(world, actor) || !super::frontier::labor::work_feasible(world, actor) {
            continue;
        }
        let (Some(role), Some(settlement)) = (
            world.get::<Villager>(actor).map(|person| person.role),
            economic_settlement(world, actor),
        ) else {
            continue;
        };
        let staples = short
            .entry(settlement)
            .or_insert_with(|| short_staples(world, settlement));
        let lacking = catalog::outputs_for(world, role)
            .iter()
            .filter(|kind| {
                staples
                    .iter()
                    .any(|staple| feeds_good(world, kind, staple, 2))
            })
            .map(|kind| batch_shortfall(world, actor, kind))
            .max()
            .unwrap_or(0);
        if lacking > 0 {
            makers.push((id, actor, lacking));
        }
    }
    // The fund keeps half of what it holds for its other duties.
    let mut left = per_day.min(world.resource::<super::VillageState>().fund / 2);
    for (id, actor, lacking) in makers {
        if left == 0 {
            break;
        }
        let owed = world
            .get_resource::<StapleAdvances>()
            .and_then(|advances| advances.owed.get(&id).copied())
            .unwrap_or(0);
        let give = lacking.min(per_maker.saturating_sub(owed)).min(left);
        if give == 0 {
            continue;
        }
        left -= give;
        world.resource_mut::<super::VillageState>().fund -= give;
        world.get_mut::<Wallet>(actor).expect("maker wallet").coins += give;
        let mut advances = world.resource_mut::<StapleAdvances>();
        *advances.owed.entry(id).or_default() += give;
        advances.advanced += u64::from(give);
        record_effect(
            world,
            Some(id),
            "VillageStapleAdvance",
            &serde_json::json!({"coins":give,"owed":owed + give}),
        );
    }
}

/// A maker who owes the fund repays half of `proceeds` (at least one coin,
/// at most what it owes) from the coins it was just paid.
pub(crate) fn repay_advance(world: &mut World, seller: Entity, proceeds: u32) {
    let Some(id) = world.get::<SimId>(seller).copied() else {
        return;
    };
    let owed = world
        .get_resource::<StapleAdvances>()
        .and_then(|advances| advances.owed.get(&id).copied())
        .unwrap_or(0);
    if owed == 0 || proceeds == 0 {
        return;
    }
    let coins = world.get::<Wallet>(seller).map_or(0, |wallet| wallet.coins);
    let repay = proceeds.div_ceil(2).min(owed).min(coins);
    if repay == 0 {
        return;
    }
    world
        .get_mut::<Wallet>(seller)
        .expect("seller wallet")
        .coins -= repay;
    world.resource_mut::<super::VillageState>().fund += repay;
    let mut advances = world.resource_mut::<StapleAdvances>();
    let left = owed - repay;
    if left == 0 {
        advances.owed.remove(&id);
    } else {
        advances.owed.insert(id, left);
    }
    advances.repaid += u64::from(repay);
}

pub fn register(app: &mut App) {
    app.init_resource::<StapleAdvances>()
        .register_sim_resource::<StapleAdvances>();
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
    // Display names come from the catalog data; the Actor Testbed's
    // scenarios, which have none, keep their own labels.
    let good = catalog::display_in(world, &job.good).unwrap_or(match job.good.as_str() {
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
        "axe" => "axes",
        "pickaxe" => "pickaxes",
        _ => "supplies",
    });
    Some(match job.verb.as_str() {
        "build_cottage" => "Build a cottage".into(),
        "build" => "Work on a building site".into(),
        "haul" => format!("Carry {good} to a building site"),
        "fetch" => format!("Fetch {good} from the store"),
        "store" => format!("Store {good}"),
        "deliver" => format!("Carry {good} to sell where they are wanted"),
        "restock" => format!("Restock the stall with {good}"),
        "buy" => format!("Buy {good}"),
        "gather" => format!("Gather {good}"),
        "hunt" => "Hunt wildlife".into(),
        "equip" => "Put on warm clothes".into(),
        "craft" => super::frontier::economy_spec::registry(world)
            .and_then(|registry| registry.recipe_label(&job.recipe))
            .unwrap_or(match job.recipe.as_str() {
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
            })
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
    if let Some(profile) = super::frontier::labor::profile(world, actor) {
        return super::frontier::economy_spec::registry(world)
            .and_then(|registry| registry.good(&profile.preferred_food))
            .filter(|good| good.food_value.is_some())
            .map_or(roles(world).baked, |good| good.key);
    }
    // Item/entity spawn counts must not accidentally give every customer the
    // same diet. Stable profession identities provide a genuine consumer mix.
    if world
        .get::<Villager>(actor)
        .is_some_and(|person| person.role != Role::Visitor && person.role.tag() % 3 == 0)
    {
        roles(world).cooked
    } else {
        roles(world).baked
    }
}

/// Every food of the catalog in eating order: the resident's preferred
/// food, then by food value (highest first), then by key (review fix S2.9:
/// any good with a `food_value` is eaten, not only three named roles).
pub fn diet(world: &World, actor: Entity) -> Vec<&'static str> {
    let preferred = food_preference(world, actor);
    let mut foods: Vec<(bool, i32, &'static str)> = catalog::trade_kinds(world)
        .iter()
        .filter_map(|kind| {
            let value = catalog::food_restore_in(world, kind)?;
            Some((*kind != preferred, -value, *kind))
        })
        .collect();
    foods.sort_unstable();
    foods.into_iter().map(|(_, _, kind)| kind).collect()
}

pub fn food_choice(world: &World, actor: Entity) -> Option<&'static str> {
    let foods = diet(world, actor);
    foods
        .iter()
        .copied()
        .find(|kind| econ::count(world, actor, kind) > 0)
        .or_else(|| {
            foods
                .iter()
                .copied()
                .find(|kind| super::ai::market_supplier(world, actor, kind).is_some())
        })
}

pub fn garment_equipped(world: &World, actor: Entity) -> bool {
    super::living::has_coat(world, actor)
}

fn recipes(world: &World) -> Vec<RecipeDef> {
    let registry = world.resource::<RecipeRegistry>();
    let mut names = BTreeSet::new();
    // Every trade of the catalog data; the Actor Testbed's fixed trades.
    let trades: Vec<Role> = super::frontier::economy_spec::registry(world).map_or_else(
        || {
            Role::RESIDENTS
                .into_iter()
                .chain([Role::Hunter, Role::Tanner, Role::Tailor])
                .collect()
        },
        |registry| registry.spec.professions.iter().map(|p| p.role).collect(),
    );
    for role in trades {
        names.extend(catalog::recipes_for(world, role).iter().copied());
    }
    names
        .into_iter()
        .filter_map(|name| registry.recipes.get(name).cloned())
        .collect()
}

fn depth(
    roles: &Roles,
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
    // What a hunt yields needs the hunting tool.
    if kind == roles.meat || kind == roles.hide {
        value = depth(roles, roles.hunt_tool, recipes, visiting, depths)?.saturating_add(1);
    }
    for recipe in recipes
        .iter()
        .filter(|recipe| recipe.outputs.iter().any(|output| output.kind == kind))
    {
        for input in &recipe.inputs {
            value =
                value.max(depth(roles, &input.key, recipes, visiting, depths)?.saturating_add(1));
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
            depth(
                roles(world),
                &output.kind,
                &recipes,
                &mut BTreeSet::new(),
                &mut depths,
            )?;
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
    /// The settlement of this regional round.
    settlement: Option<SimId>,
    /// Makers with ordered work this round: standing work goes to the others
    /// only, so a planned stock never displaces an order (closing design).
    ordered: Option<BTreeSet<SimId>>,
}
#[derive(Default)]
struct MaterialBundle {
    leaves: Goods,
    // (amount needed internally, physical recipe yield that must be made)
    internal: BTreeMap<String, (u32, u32)>,
}
impl Builder<'_> {
    fn recipe_for(&self, actor: SimId, role: Role, kind: &str) -> Option<&RecipeDef> {
        let recipes = catalog::recipes_for(self.world, role)
            .iter()
            .filter_map(|name| self.recipes.iter().find(|recipe| recipe.name == *name))
            .filter(|recipe| recipe.outputs.iter().any(|output| output.kind == kind));
        recipes.min_by_key(|recipe| {
            let unavailable = recipe
                .inputs
                .iter()
                .filter(|input| {
                    self.ledger.own_available(actor, &input.key) < input.count
                        && !self.material_possible(&input.key, &mut BTreeSet::new())
                })
                .count();
            let cost = recipe
                .inputs
                .iter()
                .map(|input| {
                    u64::from(
                        input
                            .count
                            .saturating_sub(self.ledger.own_available(actor, &input.key)),
                    ) * u64::from(market::unit_price(self.world, &input.key).unwrap_or(u32::MAX))
                })
                .sum::<u64>();
            (
                unavailable,
                cost,
                recipe.duration_ticks,
                recipe.name.as_str(),
            )
        })
    }
    fn material_possible(&self, kind: &str, visiting: &mut BTreeSet<String>) -> bool {
        if self
            .ledger
            .lots
            .get(kind)
            .is_some_and(|owners| owners.values().any(|count| *count > 0))
        {
            return true;
        }
        if !visiting.insert(kind.into()) {
            return false;
        }
        let region = self
            .people
            .first()
            .and_then(|(_, actor, _)| economic_settlement(self.world, *actor));
        let raw = self
            .world
            .resource::<SimIndex>()
            .map
            .values()
            .any(|entity| {
                self.world
                    .get::<super::ResourcePatch>(*entity)
                    .is_some_and(|patch| {
                        patch.kind == kind
                            && patch.remaining > 0
                            && super::frontier::settlements::member(self.world, *entity) == region
                    })
            });
        let r = roles(self.world);
        let prey = (kind == r.meat || kind == r.hide)
            && self.people.iter().any(|(_, _, role)| *role == r.hunter)
            && super::hunting::available_prey(self.world)
                .into_iter()
                .any(|(_, entity, _)| {
                    super::frontier::settlements::member(self.world, entity) == region
                });
        let crafted = self
            .recipes
            .iter()
            .filter(|recipe| recipe.outputs.iter().any(|output| output.kind == kind))
            .filter(|recipe| {
                self.people.iter().any(|(_, _, role)| {
                    catalog::recipes_for(self.world, *role).contains(&recipe.name.as_str())
                })
            })
            .any(|recipe| {
                recipe
                    .inputs
                    .iter()
                    .all(|input| self.material_possible(&input.key, visiting))
            });
        visiting.remove(kind);
        raw || prey || crafted
    }
    fn bundle(&self, actor: SimId, recipe: &RecipeDef, batches: u32) -> MaterialBundle {
        self.bundle_inputs(
            actor,
            recipe
                .inputs
                .iter()
                .map(|input| (input.key.as_str(), batches.saturating_mul(input.count))),
        )
    }
    fn bundle_inputs<'a>(
        &self,
        actor: SimId,
        inputs: impl IntoIterator<Item = (&'a str, u32)>,
    ) -> MaterialBundle {
        let mut bundle = MaterialBundle::default();
        let mut used = Goods::new();
        for (kind, count) in inputs {
            self.expand_material(actor, kind, count, &mut used, &mut bundle, 0);
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
            .and_then(|role| self.recipe_for(actor, role, kind));
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
        let Some(recipe) = self.recipe_for(actor, *role, kind) else {
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
        // A staple's need about to go unmet is demand even when the buyer
        // cannot pay today's price (closing fix A): it is funded at what the
        // good costs its makers (`stock::reservation`: input cost plus margin,
        // or the raw floor), and the buyer bids that; a seller takes the bid
        // only while it covers its own inputs (`market::price_for`).
        let floor = if market::is_staple(self.world, kind) {
            super::stock::reservation(self.world, kind)
                .unwrap_or(price)
                .clamp(1, price)
        } else {
            price
        };
        let budget = self.budget.entry(actor).or_default();
        let buying = missing.saturating_sub(self_made);
        let at_price = buying.min(*budget / price);
        if floor < price && at_price < buying && *budget > 0 {
            let funded = buying.min(*budget / floor).max(at_price);
            if funded > at_price {
                let limit = (*budget / funded.max(1)).clamp(floor, price);
                self.plan
                    .bid_limits
                    .entry(actor)
                    .or_default()
                    .entry(kind.into())
                    .and_modify(|prior| *prior = (*prior).min(limit))
                    .or_insert(limit);
                *budget = budget.saturating_sub(funded * limit);
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
                return;
            }
        }
        let mut funded = at_price;
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
            let r = roles(self.world);
            if role == r.hunter && (needed.contains(r.meat) || needed.contains(r.hide)) {
                let cost = if self.ledger.own_available(id, r.hunt_tool) > 0 {
                    0
                } else {
                    market::unit_price(self.world, r.hunt_tool).unwrap_or(u32::MAX)
                };
                if cost <= budget || budget > 0 {
                    jobs.push((
                        depths.get(r.hide).copied().unwrap_or(0),
                        cost.min(budget),
                        "hunt".into(),
                        vec![(r.hunt_tool.into(), 1)],
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
    /// Regional make-to-stock rule: no new batch while the settlement's
    /// unsold stock and open offers of the good exceed its stock target.
    fn stocked(&self, kind: &str) -> bool {
        self.settlement
            .is_some_and(|region| super::stock::over_target(self.world, region, kind))
    }
    fn manufacture(&mut self, kind: &str, missing: u32) {
        if self.stocked(kind) {
            return;
        }
        let mut producers: Vec<_> = self
            .people
            .iter()
            .copied()
            .filter(|(_, entity, role)| {
                catalog::outputs_for(self.world, *role).contains(&kind)
                    && super::frontier::labor::work_feasible(self.world, *entity)
            })
            .filter(|(id, _, _)| self.ordered.as_ref().is_none_or(|busy| !busy.contains(id)))
            .collect();
        producers.sort_by_key(|(id, _, _)| {
            (
                self.plan
                    .production
                    .get(id)
                    .map_or(0, |goods| goods.values().copied().sum::<u32>()),
                *id,
            )
        });
        let mut remaining = missing;
        let mut slots = producers.len() as u32;
        for (id, entity, role) in producers {
            if remaining == 0 {
                break;
            }
            let before = self
                .plan
                .production
                .get(&id)
                .and_then(|goods| goods.get(kind))
                .copied()
                .unwrap_or(0);
            let allocation = remaining.div_ceil(slots.max(1));
            self.manufacture_for(id, entity, role, kind, allocation);
            let after = self
                .plan
                .production
                .get(&id)
                .and_then(|goods| goods.get(kind))
                .copied()
                .unwrap_or(0);
            // Recipe over-yield has already returned to the lot ledger.
            // Do not credit those same units toward two different demands.
            remaining = remaining.saturating_sub(after.saturating_sub(before).min(allocation));
            slots = slots.saturating_sub(1);
        }
    }
    /// Regional: a gatherer whose good sells above its base price keeps a few
    /// units on hand beyond its orders. Everybody else works to order, so
    /// supply never exceeds demand, and a price that rose in a shortage would
    /// have nothing to bring it down again once the makers can keep up. A
    /// gatherer who cannot keep up never fills the shelf, and the price stays.
    fn stock_shelves(&mut self) {
        let Some(shelf) = super::frontier::economy_spec::tuning(self.world)
            .map(|tuning| tuning.gatherer_shelf_units)
            .filter(|shelf| *shelf > 0)
        else {
            return;
        };
        for (id, entity, role) in self.people.clone() {
            if !super::frontier::labor::work_feasible(self.world, entity) {
                continue;
            }
            for kind in catalog::outputs_for(self.world, role) {
                if (*kind == roles(self.world).meat || *kind == roles(self.world).hide)
                    || self.recipe_for(id, role, kind).is_some()
                {
                    continue;
                }
                let dear = market::unit_price(self.world, kind)
                    .zip(catalog::price_in(self.world, kind))
                    .is_some_and(|(price, base)| price > base);
                let spare = self.ledger.own_available(id, kind);
                if !dear || spare >= shelf || self.stocked(kind) {
                    continue;
                }
                *self
                    .plan
                    .production
                    .entry(id)
                    .or_default()
                    .entry((*kind).into())
                    .or_default() += shelf - spare;
            }
        }
    }
    /// Living world: while anybody of the settlement lacks a coat, every
    /// maker of coats keeps a shelf of `max(2, residents / 15)` of them, in
    /// every season, financed from its own purse like any made-to-stock
    /// batch. Production demand, not a grant.
    fn stock_coats(&mut self) {
        let coatless = self
            .people
            .iter()
            .filter(|(_, entity, _)| {
                !super::living::has_coat(self.world, *entity)
                    && econ::count(self.world, *entity, roles(self.world).garment) == 0
            })
            .count();
        let per = super::frontier::economy_spec::tuning(self.world)
            .map_or(0, |tuning| tuning.coat_shelf_residents);
        if per == 0 || coatless == 0 || self.stocked(roles(self.world).garment) {
            return;
        }
        // The overseer's coat priority: low makes coats to order only; high
        // stocks one for every resident without one.
        let level = self.settlement.map_or_else(Default::default, |region| {
            super::frontier::oversee::level(
                self.world,
                region,
                super::frontier::oversee::PriorityTopic::Coats,
                None,
            )
        });
        let shelf = match level {
            super::frontier::oversee::PriorityLevel::Low => return,
            super::frontier::oversee::PriorityLevel::Normal => {
                (self.people.len() as u32 / per).max(2)
            }
            super::frontier::oversee::PriorityLevel::High => {
                (self.people.len() as u32 / per).max(2).max(coatless as u32)
            }
        };
        for (id, entity, role) in self.people.clone() {
            if !catalog::outputs_for(self.world, role).contains(&roles(self.world).garment)
                || !super::frontier::labor::work_feasible(self.world, entity)
            {
                continue;
            }
            let spare = self.ledger.own_available(id, roles(self.world).garment);
            let planned = self
                .plan
                .production
                .get(&id)
                .and_then(|goods| goods.get(roles(self.world).garment))
                .copied()
                .unwrap_or(0);
            let missing = shelf.saturating_sub(spare.saturating_add(planned));
            if missing > 0 {
                self.manufacture_for(id, entity, role, roles(self.world).garment, missing);
            }
        }
    }
    fn manufacture_for(&mut self, id: SimId, entity: Entity, role: Role, kind: &str, missing: u32) {
        if kind == roles(self.world).meat || kind == roles(self.world).hide {
            let per_hunt = if kind == roles(self.world).meat {
                roles(self.world).meat_per_hunt
            } else {
                roles(self.world).hide_per_hunt
            };
            let price = market::unit_price(self.world, roles(self.world).hunt_tool)
                .unwrap_or(u32::MAX)
                .max(1);
            let cash = self.budget.get(&id).copied().unwrap_or(0);
            let owned = self.ledger.own_available(id, roles(self.world).hunt_tool);
            let financed = owned.saturating_add(cash / price).max(u32::from(cash > 0));
            let hunts = missing.div_ceil(per_hunt).min(4).min(financed);
            if hunts == 0 {
                return;
            }
            let bundle = MaterialBundle {
                leaves: [(roles(self.world).hunt_tool.into(), hunts)].into(),
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
            let other = if kind == roles(self.world).meat {
                roles(self.world).hide
            } else {
                roles(self.world).meat
            };
            self.ledger.add(
                id,
                other,
                hunts
                    * if other == roles(self.world).meat {
                        roles(self.world).meat_per_hunt
                    } else {
                        roles(self.world).hide_per_hunt
                    },
            );
            return;
        }
        let recipe = self.recipe_for(id, role, kind).cloned();
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

/// Standing stock a profession orders: `(good, count, reserve its inputs)`.
/// The regional profile reads it from data; older profiles keep the inn's list.
fn keeps(world: &World, role: Role) -> Vec<(&'static str, u32, bool)> {
    // Standing stock is catalog data; the Actor Testbed keeps the inn's list.
    let Some(registry) = super::frontier::economy_spec::registry(world) else {
        return match role {
            Role::Innkeeper => vec![
                ("log", 2, true),
                (catalog::BOWL, 1, true),
                ("hammer", 1, true),
            ],
            Role::Fisher => vec![(catalog::FISHING_ROD, 1, false)],
            _ => Vec::new(),
        };
    };
    let Some(entry) = registry.definition(role) else {
        return Vec::new();
    };
    entry
        .keeps
        .iter()
        .filter_map(|keep| Some((registry.name(&keep.good)?, keep.count, keep.reserve_inputs)))
        .chain(
            entry
                .tool
                .as_ref()
                .and_then(|tool| registry.name(tool))
                .map(|tool| (tool, 1, true)),
        )
        .collect()
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
    let reserve_days = world
        .get_resource::<super::colony::ColonyState>()
        .map_or(2, |colony| colony.food_reserve_days.clamp(1, 4));
    // A storm tomorrow keeps everybody indoors for a day: residents who can
    // read the sky lay in one more day of provisions (regional weather).
    let reserve_days = reserve_days + u32::from(super::weather::storm_tomorrow(world));
    let day = world.resource::<SimConfig>().day_length;
    // A free spender lays in more, a thrifty resident less (review npc-ai#13).
    let horizon = (day.saturating_mul(u64::from(reserve_days)) as i64
        + super::resident::thrift_stock_shift(world, actor, day))
    .max(0) as u64;
    let depleted = (100_000 - econ::need(world, actor, need)).max(0) as u64;
    // Children eat from the packs of the household's adults (regional only).
    let dependants = if need == "Hunger" {
        super::demography::dependants_hunger(world, actor, horizon)
    } else {
        0
    };
    depleted
        .saturating_add(decay.saturating_mul(horizon))
        .saturating_add(dependants)
}

fn needed_materials(
    world: &World,
    people: &[(SimId, Entity, Role)],
    recipes: &[RecipeDef],
    civic: &Goods,
) -> BTreeSet<String> {
    let mut needed: BTreeSet<String> = civic.keys().cloned().collect();
    for (_, actor, role) in people {
        // Every food held counts (a second crop or a pen's product too).
        let owned_food = diet(world, *actor)
            .into_iter()
            .map(|kind| {
                u64::from(econ::count(world, *actor, kind))
                    * catalog::food_restore_in(world, kind).unwrap_or(0).max(0) as u64
            })
            .sum::<u64>();
        if owned_food < forecast_amount(world, *actor, "Hunger") {
            needed.insert(food_preference(world, *actor).into());
        }
        if econ::count(world, *actor, roles(world).drink)
            < forecast(world, *actor, "Thirst", 90_000)
        {
            needed.insert(roles(world).drink.into());
        }
        if econ::count(world, *actor, roles(world).garment)
            < super::living::clothing_reserve_target(world, *actor)
        {
            needed.insert(roles(world).garment.into());
        }
        for (kind, count, reserve_inputs) in keeps(world, *role) {
            if reserve_inputs && econ::count(world, *actor, kind) < count {
                needed.insert(kind.into());
            }
        }
        for (kind, quantity) in super::colony::construction_materials(world, *actor) {
            if econ::count(world, *actor, kind) < quantity {
                needed.insert(kind.into());
            }
        }
        for (kind, quantity) in super::colony::household_materials(world, *actor) {
            if econ::count(world, *actor, kind) < quantity {
                needed.insert(kind.into());
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
        if needed.contains(roles(world).meat) || needed.contains(roles(world).hide) {
            needed.insert(roles(world).hunt_tool.into());
        }
        if needed.len() == previous {
            break;
        }
    }
    needed
}

/// Each regional production round accounts for one workplace catchment. This
/// prevents remote inventory from being credited as already delivered supply.
/// Shared reference prices and the existing finite world fund remain regional
/// trade's common currency; they are not represented as separate local treasuries.
pub fn economic_settlement(world: &World, actor: Entity) -> Option<SimId> {
    world
        .get::<Villager>(actor)
        .and_then(|person| world.resource::<SimIndex>().get(person.workplace))
        .and_then(|site| super::frontier::settlements::member(world, site))
        .or_else(|| super::frontier::settlements::member(world, actor))
        .or_else(|| {
            let here = world.get::<SimPos>(actor)?;
            super::frontier::settlements::all(world)
                .into_iter()
                .filter_map(|(id, _)| {
                    Some((
                        here.dist2(super::frontier::settlements::definition(world, id)?.center),
                        id,
                    ))
                })
                .min()
                .map(|(_, id)| id)
        })
}

fn compute(world: &World) -> ProductionPlan {
    let mut combined = ProductionPlan {
        refreshed_at: Some(world.resource::<SimClock>().tick),
        ..Default::default()
    };
    for (settlement, _) in super::frontier::settlements::all(world) {
        merge(&mut combined, compute_region(world, Some(settlement)));
    }
    combined
}

fn merge(plan: &mut ProductionPlan, region: ProductionPlan) {
    plan.procurement.extend(region.procurement);
    plan.wanted.extend(region.wanted);
    plan.self_commissions.extend(region.self_commissions);
    plan.bid_limits.extend(region.bid_limits);
    plan.production.extend(region.production);
    plan.committed.extend(region.committed);
}

/// Replans one settlement in place: the entries of its people (and of
/// anybody no longer living or of no settlement) are replaced by a fresh
/// plan of the settlement; the other settlements' entries stay.
fn refresh_region(world: &mut World, settlement: SimId) {
    let region = compute_region(world, Some(settlement));
    let stale = |world: &World, id: &SimId| {
        world
            .resource::<SimIndex>()
            .get(*id)
            .filter(|actor| super::living(world, *actor))
            .and_then(|actor| economic_settlement(world, actor))
            .is_none_or(|home| home == settlement)
    };
    let mut plan = world
        .remove_resource::<ProductionPlan>()
        .unwrap_or_default();
    for targets in [
        &mut plan.procurement,
        &mut plan.wanted,
        &mut plan.self_commissions,
        &mut plan.bid_limits,
        &mut plan.production,
        &mut plan.committed,
    ] {
        targets.retain(|id, _| !stale(world, id));
    }
    merge(&mut plan, region);
    world.insert_resource(plan);
}

fn compute_region(world: &World, settlement: Option<SimId>) -> ProductionPlan {
    let phase = super::probe::probe(world, "plan.a_inventory");
    // A query over the people in identity order, not a scan of every identity.
    let mut everybody: Vec<(SimId, Entity, Role)> = world
        .try_query::<(Entity, &SimId, &Villager)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(e, id, person)| (*id, e, person.role))
                .collect()
        })
        .unwrap_or_default();
    everybody.sort_unstable_by_key(|(id, _, _)| *id);
    let people: Vec<_> = everybody
        .iter()
        .filter(|(_, e, role)| {
            *role != Role::Visitor
                && super::living(world, *e)
                && settlement.is_none_or(|region| economic_settlement(world, *e) == Some(region))
        })
        .copied()
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
        settlement,
        ordered: None,
    };
    for (id, e, role) in &people {
        for kind in catalog::outputs_for(world, *role) {
            builder.ledger.sellable.insert((*id, (*kind).into()));
        }
        builder.budget.insert(
            *id,
            world.get::<Wallet>(*e).map_or(0, |wallet| wallet.coins),
        );
        // Regional: the hearth's fuel reserve is never handed to a building
        // site (`civic::kept`), so it is no supply for the sites' orders. The
        // part the resident's own procurement already wants stays: own
        // requests use it.
        let mut reserved: BTreeMap<String, u32> = BTreeMap::new();
        for (_, item) in econ::free_items(world, *e) {
            if market::sells(world, *e, &item.kind) {
                builder.ledger.sellable.insert((*id, item.kind.clone()));
            }
            let mut stack = item.stack;
            if settlement.is_some() {
                let left = reserved.entry(item.kind.clone()).or_insert_with(|| {
                    super::civic::hearth_reserve(world, *e, &item.kind)
                        .saturating_sub(desired_stock(world, *e, &item.kind))
                });
                let kept = (*left).min(stack);
                *left -= kept;
                stack -= kept;
            }
            builder.ledger.add(*id, &item.kind, stack);
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
            builder
                .ledger
                .add(*id, roles(world).meat, roles(world).meat_per_hunt);
            builder
                .ledger
                .add(*id, roles(world).hide, roles(world).hide_per_hunt);
            builder
                .plan
                .committed
                .entry(*id)
                .or_default()
                .insert(roles(world).meat.into(), roles(world).meat_per_hunt);
            builder
                .plan
                .committed
                .entry(*id)
                .or_default()
                .insert(roles(world).hide.into(), roles(world).hide_per_hunt);
        }
    }
    // Explicit player offers are usable supply, never an inspection of an
    // unlisted visitor's private inventory.
    for (id, e, role) in &everybody {
        if *role != Role::Visitor
            || settlement.is_some_and(|region| economic_settlement(world, *e) != Some(region))
        {
            continue;
        }
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
    drop(phase);
    let phase = super::probe::probe(world, "plan.b_depths");
    let mut depths = BTreeMap::new();
    for kind in catalog::trade_kinds(world) {
        if depth(
            roles(world),
            kind,
            &builder.recipes,
            &mut BTreeSet::new(),
            &mut depths,
        )
        .is_err()
        {
            return ProductionPlan {
                refreshed_at: Some(world.resource::<SimClock>().tick),
                ..Default::default()
            };
        }
    }
    // Executable, urgent bodily needs go first. Future two-day shopping must
    // not consume the same cash and material lots needed to keep makers alive.
    drop(phase);
    let phase = super::probe::probe(world, "plan.c_needs");
    let mut critical = Targets::new();
    for (id, e, _) in &people {
        if econ::need(world, *e, "Thirst") < 55_000 {
            builder.request(*id, roles(world).drink, 1);
            critical
                .entry(*id)
                .or_default()
                .insert(roles(world).drink.into(), 1);
        }
        if econ::need(world, *e, "Hunger") < 50_000 {
            let food = food_choice(world, *e).unwrap_or_else(|| food_preference(world, *e));
            builder.request(*id, food, 1);
            critical.entry(*id).or_default().insert(food.into(), 1);
        }
        // A resident who wears no coat and carries none has nothing against
        // the cold of every night: the coat it plans for is a bodily need
        // like today's water and food, funded before stock for the days
        // ahead. Planned after that stock, its bid was what the stock left
        // over (a coin), below what any maker takes, so a coatless resident
        // went on without one while coats lay unsold.
        let garment = roles(world).garment;
        if !super::living::has_coat(world, *e) && econ::count(world, *e, garment) == 0 {
            let clothing = super::living::clothing_reserve_target(world, *e);
            if clothing > 0 {
                builder.request(*id, garment, clothing);
                critical
                    .entry(*id)
                    .or_default()
                    .insert(garment.into(), clothing);
            }
        }
        {
            let threshold = super::policy::active(world).map_or_else(
                || super::policy::IslandAiPolicy::default().critical_need,
                |policy| policy.critical_need,
            );
            if econ::need(world, *e, "Warmth") < threshold {
                for (kind, quantity) in super::colony::household_materials(world, *e) {
                    builder.request(*id, kind, quantity);
                    critical
                        .entry(*id)
                        .or_default()
                        .insert(kind.into(), quantity);
                }
            }
        }
    }
    // A recorded housing commission is a complete physical job. Protect its
    // actual input bundle after urgent needs, before speculative comfort stock.
    // Expanding all inputs together prevents the same owned log from backing
    // both missing planks and the cottage's retained timber. The shared financier
    // caps all missing ingredients by this actor's remaining real purse.
    for (id, e, _) in &people {
        let materials = super::colony::construction_materials(world, *e);
        if materials.is_empty() {
            continue;
        }
        let critical_need = super::policy::active(world).map_or_else(
            || super::policy::IslandAiPolicy::default().critical_need,
            |policy| policy.critical_need,
        );
        let clothing = super::living::clothing_reserve_target(world, *e);
        let planned = critical
            .get(id)
            .and_then(|goods| goods.get(roles(world).garment))
            .copied()
            .unwrap_or(0);
        if econ::need(world, *e, "Warmth") < critical_need && clothing > planned {
            builder.request(*id, roles(world).garment, clothing - planned);
            critical
                .entry(*id)
                .or_default()
                .insert(roles(world).garment.into(), clothing);
        }
        let bundle = builder.bundle_inputs(*id, materials);
        builder.finance_bundle(*id, bundle);
    }
    // Open building sites order their materials like any buyer, paid by the
    // civic fund on delivery. Goods on store shelves are supply.
    // Players' homestead orders are demand the same way.
    let mut civic_orders = settlement
        .map(|region| super::civic::material_orders(world, region))
        .unwrap_or_default();
    if let Some(region) = settlement {
        for (kind, units) in super::live::order_demand(world, region) {
            *civic_orders.entry(kind).or_default() += units;
        }
        // Other settlements' sites whose shortfall is assigned to this one:
        // their own settlement can neither take it from its stores nor make
        // it (`civic::unmet_elsewhere`). Its makers get the work; its haulers
        // carry it there (S4 housing fix).
        for (kind, units) in super::civic::assigned_orders(world, region) {
            *civic_orders.entry(kind).or_default() += units;
        }
        // The boat's announced order, at the port.
        // The overseer's boat priority: none at low, twice at high.
        let boat = super::frontier::oversee::level(
            world,
            region,
            super::frontier::oversee::PriorityTopic::BoatOrders,
            None,
        );
        for (kind, units) in super::merchant::order_demand(world, region) {
            let units = match boat {
                super::frontier::oversee::PriorityLevel::Low => 0,
                super::frontier::oversee::PriorityLevel::Normal => units,
                super::frontier::oversee::PriorityLevel::High => units.saturating_mul(2),
            };
            if units > 0 {
                *civic_orders.entry(kind).or_default() += units;
            }
        }
    }
    if let Some(region) = settlement {
        for (store, kind, units) in super::civic::depot_supply(world, region) {
            builder.ledger.add(store, &kind, units);
            builder.ledger.sellable.insert((store, kind));
        }
    }
    drop(phase);
    let phase = super::probe::probe(world, "plan.d_materials");
    let needed = needed_materials(world, &people, &builder.recipes, &civic_orders);
    builder.reserve_working_capital(&needed, &depths);
    for (id, e, role) in &people {
        let primary = food_preference(world, *e);
        let alternate = if primary == roles(world).baked {
            roles(world).cooked
        } else {
            roles(world).baked
        };
        let provisioned = critical.get(id);
        let water = provisioned
            .and_then(|goods| goods.get(roles(world).drink))
            .copied()
            .unwrap_or(0);
        builder.request(
            *id,
            roles(world).drink,
            forecast(world, *e, "Thirst", 90_000).saturating_sub(water),
        );
        let mut hunger = forecast_amount(world, *e, "Hunger");
        if let Some(goods) = provisioned {
            for (kind, count) in goods {
                if let Some(restoration) = catalog::food_restore_in(world, kind) {
                    hunger = hunger.saturating_sub(u64::from(*count) * restoration.max(0) as u64);
                }
            }
        }
        // Food already held is kept, in eating order: the preferred food,
        // the other finished staple, then every other food by food value.
        let mut kept = vec![primary, alternate];
        kept.extend(
            diet(world, *e)
                .into_iter()
                .filter(|kind| *kind != primary && *kind != alternate),
        );
        for kind in kept {
            let restoration = catalog::food_restore_in(world, kind).unwrap_or(1).max(1) as u64;
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
        let restoration = catalog::food_restore_in(world, primary)
            .unwrap_or(80_000)
            .max(1) as u64;
        builder.request(*id, primary, hunger.div_ceil(restoration).min(16) as u32);
        builder.request(
            *id,
            roles(world).garment,
            super::living::clothing_reserve_target(world, *e).saturating_sub(
                provisioned
                    .and_then(|goods| goods.get(roles(world).garment))
                    .copied()
                    .unwrap_or(0),
            ),
        );
        // Standing stock, including one working tool, is ordinary funded demand:
        // when a tool wears out the smith receives a real commission, and
        // bare-handed work continues meanwhile.
        for (kind, count, _) in keeps(world, *role) {
            builder.request(*id, kind, count);
        }
        for (kind, quantity) in super::colony::household_materials(world, *e) {
            let urgent = provisioned
                .and_then(|goods| goods.get(kind))
                .copied()
                .unwrap_or(0);
            builder.request(*id, kind, quantity.saturating_sub(urgent));
        }
    }
    // Escrow is virtual allocation only. Release its actual cash for the
    // balanced material requests below; no wallet or item has been changed.
    for (id, cash) in std::mem::take(&mut builder.work_budget) {
        *builder.budget.entry(id).or_default() += cash;
    }
    builder.industrial = true;
    for (kind, units) in civic_orders {
        let requested = builder.requests.entry(kind).or_default();
        *requested = requested.saturating_add(units);
    }
    let mut order: Vec<_> = depths.into_iter().collect();
    order.sort_by(|(a, da), (b, db)| db.cmp(da).then_with(|| a.cmp(b)));
    let run = |builder: &mut Builder<'_>| {
        for (kind, _) in &order {
            let requested = builder.requests.remove(kind).unwrap_or(0);
            builder.ledger.release_unused_work(kind);
            let supplied = builder.ledger.take_any(kind, requested);
            let missing = requested - supplied;
            if missing > 0 {
                builder.manufacture(kind, missing);
            }
        }
    };
    drop(phase);
    let phase = super::probe::probe(world, "plan.e_run");
    run(&mut builder);
    drop(phase);
    let phase = super::probe::probe(world, "plan.f_standing");
    // Standing work (closing design), after every order: the settlement wants
    // its staples' planned stock on hand whether or not a buyer has ordered
    // yet, so makers fill spare capacity and cash with it, never ahead of a
    // funded order. What is left for sale supplies it first (`take_any`);
    // makers get the shortfall, inputs through the recipes. `manufacture`
    // stops at the target (`stocked`), and the target never exceeds what keeps.
    // The made inputs of a staple's recipe (flour for bread; thread and
    // leather for the coat) keep their own stock target the same way (S4 fix: flour was made
    // only to the bakers' orders of the moment, so in winter the harbour's
    // flour sat at 1 of a target of 18 while its miller stood idle with no
    // funded goal, and bread fell short).
    if let Some(region) = settlement {
        for kind in catalog::trade_kinds(world) {
            // Made goods a staple's own recipe consumes, one step up only:
            // further up (the bowls a tanner uses, the planks for those) the
            // batches leave remainders that stand unsold (S4: with bowls
            // stocked, planks sat at their floor price on seed 11).
            let made_input = || {
                super::frontier::economy_spec::registry(world).is_some_and(|registry| {
                    let recipes = &registry.spec.recipes;
                    recipes
                        .iter()
                        .any(|recipe| recipe.outputs.iter().any(|output| output.good == *kind))
                        && recipes.iter().any(|recipe| {
                            recipe.inputs.iter().any(|input| input.good == *kind)
                                && recipe
                                    .outputs
                                    .iter()
                                    .any(|output| super::stock::planned_staple(world, &output.good))
                        })
                })
            };
            if !super::stock::planned_staple(world, kind) && !made_input() {
                continue;
            }
            let target = super::stock::target(world, region, kind);
            if target == 0 {
                continue;
            }
            let held = super::stock::on_offer(world, region, kind);
            let wanted = target.saturating_sub(held);
            if wanted > 0 {
                let requested = builder.requests.entry((*kind).into()).or_default();
                *requested = requested.saturating_add(wanted);
            }
        }
        builder.ordered = Some(
            builder
                .plan
                .production
                .iter()
                .filter(|(_, goods)| goods.values().any(|count| *count > 0))
                .map(|(id, _)| *id)
                .collect(),
        );
        run(&mut builder);
        builder.ordered = None;
    }
    drop(phase);
    let _phase = super::probe::probe(world, "plan.g_shelves");
    if settlement.is_some() {
        builder.stock_shelves();
        builder.stock_coats();
    }
    builder.plan
}

/// Explicit application-layer invalidation; only production owns its plan state.
pub(super) fn invalidate(world: &mut World) {
    if let Some(mut plan) = world.get_resource_mut::<ProductionPlan>() {
        plan.refreshed_at = None;
    }
}

/// Every `day_length / 8` ticks (at most 300) a refresh cycle starts and
/// plans the first settlement; the others follow one a tick, in identity
/// order. An invalidation starts a new cycle at the next tick.
pub fn refresh(world: &mut World) {
    let now = world.resource::<SimClock>().tick;
    let interval = (world.resource::<SimConfig>().day_length / 8).clamp(40, 300);
    let due = world
        .get_resource::<ProductionPlan>()
        .and_then(|plan| plan.refreshed_at)
        .is_none_or(|last| now >= last.saturating_add(interval));
    if due {
        let settlements: Vec<SimId> = super::frontier::settlements::all(world)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        if settlements.is_empty() {
            let plan = compute(world);
            world.insert_resource(plan);
            return;
        }
        let mut plan = world
            .remove_resource::<ProductionPlan>()
            .unwrap_or_default();
        plan.refreshed_at = Some(now);
        plan.pending = settlements;
        world.insert_resource(plan);
    }
    let next = world
        .get_resource_mut::<ProductionPlan>()
        .and_then(|mut plan| (!plan.pending.is_empty()).then(|| plan.pending.remove(0)));
    if let Some(settlement) = next {
        refresh_region(world, settlement);
    }
}

/// Plans every settlement now, at once (tests and tools; the simulation
/// plans one settlement a tick through [`refresh`]).
pub fn refresh_all(world: &mut World) {
    let plan = compute(world);
    world.insert_resource(plan);
}
