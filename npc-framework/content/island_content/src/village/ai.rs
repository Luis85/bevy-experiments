//! Autonomous production and survival, in sorted Effects order.
use super::{VillageAct, catalog, economy as econ, model::*};
use bevy_ecs::prelude::*;
use npc_framework::crafting::{CraftSource, Crafting, RecipeRegistry, begin_craft, cancel_craft};
use npc_framework::interaction::items::{self, entity_of};
use npc_framework::interaction::{Interactor, Item};
use npc_framework::prelude::*;
use serde_json::json;

pub fn tick(world: &mut World) {
    super::living::climate_tick(world);
    let now = world.resource::<SimClock>().tick;
    let (day, _, _) = calendar(world);
    let mut people: Vec<_> = world
        .query::<(Entity, &SimId, &Villager)>()
        .iter(world)
        .map(|(e, id, v)| (*id, e, v.clone()))
        .collect();
    people.sort_by_key(|(id, _, _)| *id);
    for (id, e, v) in &people {
        vitals(world, *id, *e, v, now);
    }
    super::frontier::ecology::tick(world);
    finish_gathers(world, now);
    super::hunting::tick(world);
    super::wildlife::tick(world);
    dividends(world, day);
    super::production::refresh(world);
    super::market::tick(world);
    super::behavior::advance(world);
}

/// One profession decision. All physical work uses the same action validation,
/// inventory, crafting and navigation seams as a player-controlled actor.
pub(super) fn work(world: &mut World, e: Entity) {
    living_work(world, e);
}

/// Select a real, affordable advertised seller, including explicit player offers.
/// Distance is the only settlement condition deferred while approaching.
/// A seller on the move is a target only this near (regional): farther off
/// the buyer would trail it, and a slow buyer never catches up.
pub const WALKING_SELLER_REACH_MM: i64 = 6_000;

pub(super) fn market_supplier(world: &World, buyer: Entity, kind: &str) -> Option<Entity> {
    let coins = world.get::<Wallet>(buyer)?.coins;
    let here = *world.get::<SimPos>(buyer)?;
    // Only the living can sell, so only actors with a vitality are looked at.
    // The ranking key ends in the identity, so the order of the query does
    // not matter.
    let sellers: Vec<(SimId, Entity)> = world
        .try_query::<(Entity, &SimId, &Vitality)>()
        .map(|mut query| query.iter(world).map(|(e, id, _)| (*id, e)).collect())
        .unwrap_or_default();
    sellers
        .iter()
        .filter_map(|(id, seller)| {
            if *seller == buyer
                || !super::market::person_sells(world, *seller, kind)
                || super::market::available_to_sell(world, *seller, kind) == 0
                // Buy at a fixed place: a store, a stall, or somebody who
                // stands still. A seller walking somewhere else is not one.
                || (world.get::<MoveIntent>(*seller).is_some()
                    && here.dist2(*world.get::<SimPos>(*seller)?)
                        > WALKING_SELLER_REACH_MM.pow(2))
                || (super::routing::avoiding(
                        world,
                        buyer,
                        *world.get::<SimPos>(*seller)?,
                        Some(*id),
                    ))
            {
                return None;
            }
            let price = super::market::price_for(world, buyer, *seller, kind)?;
            if super::production::purchase_bid_limit(world, buyer, kind)
                .is_some_and(|limit| price > limit)
            {
                return None;
            }
            if price > coins
                || world
                    .get::<Wallet>(*seller)?
                    .coins
                    .checked_add(price)
                    .is_none()
            {
                return None;
            }
            let goods = econ::selected(world, *seller, kind, 1).ok()?;
            econ::fit(
                world,
                buyer,
                &goods.into_iter().map(|(_, item)| item).collect::<Vec<_>>(),
            )
            .ok()?;
            let distance = {
                i64::from(
                    super::frontier::routes::estimate(world, here, *world.get::<SimPos>(*seller)?)?
                        .distance_mm,
                )
            };
            let local = super::production::economic_settlement(world, buyer)
                == super::production::economic_settlement(world, *seller);
            Some((!local, price, distance, *id, *seller))
        })
        .chain(depot_offers(world, buyer, kind, coins, here))
        .min_by_key(|(remote, price, distance, id, _)| (*remote, *price, *distance, *id))
        .map(|(_, _, _, _, seller)| seller)
}

/// Stores and stalls that could sell `kind` to this buyer now, with the same
/// ranking key as a person who sells: `(remote, price, distance, id, seller)`.
fn depot_offers(
    world: &World,
    buyer: Entity,
    kind: &str,
    coins: u32,
    here: SimPos,
) -> Vec<(bool, u32, i64, SimId, Entity)> {
    use super::civic;
    if !civic::any_depot(world) {
        return Vec::new();
    }
    let buyer_id = world.get::<SimId>(buyer).copied();
    let Some(price) = super::market::unit_price(world, kind) else {
        return Vec::new();
    };
    if price > coins
        || super::production::purchase_bid_limit(world, buyer, kind)
            .is_some_and(|limit| price > limit)
        || econ::fit(world, buyer, &[Item::new(kind)]).is_err()
    {
        return Vec::new();
    }
    let region = super::production::economic_settlement(world, buyer);
    civic::all_depots(world)
        .into_iter()
        .filter_map(|(id, depot)| {
            let pos = *world.get::<SimPos>(depot)?;
            if civic::depot_available(world, depot, kind, buyer_id) == 0
                || !civic::serves(world, depot, buyer)
                || super::routing::avoiding(world, buyer, pos, Some(id))
            {
                return None;
            }
            let distance =
                i64::from(super::frontier::routes::estimate(world, here, pos)?.distance_mm);
            let local = super::frontier::settlements::member(world, depot) == region;
            Some((!local, price, distance, id, depot))
        })
        .collect()
}

/// How long a buyer keeps the seller it chose (S3.4).
pub const ERRAND_TICKS: u64 = 600;

/// The seller a buyer chose and walks to: kept until the purchase, until
/// it no longer offers the good at a price the buyer can pay, or for
/// [`ERRAND_TICKS`] (review fix S3.4: errands remember their target).
#[derive(
    Component,
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    bevy_reflect::Reflect,
)]
#[reflect(Component)]
pub struct Errand {
    pub good: String,
    pub target: SimId,
    pub until: u64,
}
impl SimHash for Errand {
    const NAME: &'static str = "island_errand_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.good);
        h.u32(self.target.0);
        h.u64(self.until);
    }
}

/// Whether `seller` still sells `kind` to `buyer` at a price it can pay.
fn still_sells(world: &World, buyer: Entity, seller: Entity, kind: &str) -> bool {
    let coins = world.get::<Wallet>(buyer).map_or(0, |wallet| wallet.coins);
    let limit = super::production::purchase_bid_limit(world, buyer, kind);
    if world.get::<super::civic::Depot>(seller).is_some() {
        let buyer_id = world.get::<SimId>(buyer).copied();
        return super::civic::depot_available(world, seller, kind, buyer_id) > 0
            && super::market::unit_price(world, kind)
                .is_some_and(|price| price <= coins && limit.is_none_or(|limit| price <= limit));
    }
    living(world, seller)
        && super::market::person_sells(world, seller, kind)
        && super::market::available_to_sell(world, seller, kind) > 0
        && super::market::asking_price(world, seller, kind)
            .is_some_and(|price| price <= coins && limit.is_none_or(|limit| price <= limit))
}

/// The remembered seller of `kind`, while the errand holds.
fn recall_seller(world: &World, buyer: Entity, kind: &str) -> Option<Entity> {
    let errand = world.get::<Errand>(buyer)?;
    let now = world.resource::<SimClock>().tick;
    if errand.good != kind || errand.until <= now {
        return None;
    }
    let seller = world.resource::<SimIndex>().get(errand.target)?;
    still_sells(world, buyer, seller, kind).then_some(seller)
}

pub(super) fn buy_market_input(world: &mut World, actor: Entity, kind: &str, wanted: u32) -> bool {
    let seller = match recall_seller(world, actor, kind) {
        Some(seller) => seller,
        None => {
            world.entity_mut(actor).remove::<Errand>();
            let Some(seller) = market_supplier(world, actor, kind) else {
                return false;
            };
            let target = *world.get::<SimId>(seller).expect("seller identity");
            let until = world
                .resource::<SimClock>()
                .tick
                .saturating_add(ERRAND_TICKS);
            world.entity_mut(actor).insert(Errand {
                good: kind.to_owned(),
                target,
                until,
            });
            seller
        }
    };
    let target = *world.get::<SimId>(seller).expect("seller identity");
    super::production::intent(world, actor, "buy", kind, "", Some(target));
    if !approach_work(world, actor, seller) {
        return true;
    }
    let price = super::market::asking_price(world, seller, kind)
        .unwrap_or(u32::MAX)
        .max(1);
    let quantity = wanted
        .saturating_sub(econ::count(world, actor, kind))
        .min(super::market::available_to_sell(world, seller, kind))
        .min(8)
        .min(world.get::<Wallet>(actor).map_or(0, |wallet| wallet.coins) / price);
    let mut command = VillageAct::new(*world.get::<SimId>(actor).expect("actor"), "buy");
    command.target = Some(target);
    command.kind = kind.into();
    command.limit_price = Some(
        super::production::purchase_bid_limit(world, actor, kind)
            .map_or(price, |limit| limit.min(price)),
    );
    for count in (1..=quantity).rev() {
        command.quantity = count;
        if command.perform(world).is_ok() {
            // Done: the next purchase chooses afresh.
            world.entity_mut(actor).remove::<Errand>();
            return true;
        }
    }
    world.entity_mut(actor).remove::<Errand>();
    false
}

/// Actionable finite subsistence, ignoring only the approach distance. This is
/// also the need guard's source of truth; an empty patch or full pack must not
/// suppress other executable work.
pub(super) fn can_forage(world: &World, actor: Entity, kind: &str) -> bool {
    gather_source(world, actor, kind).is_some()
}

pub(crate) fn gather_source(world: &World, actor: Entity, kind: &str) -> Option<(SimId, Entity)> {
    if econ::fit(world, actor, &[Item::new(kind)]).is_err() {
        return None;
    }
    let here = world.get::<SimPos>(actor).copied().unwrap_or_default();
    // A query over the patches; the ranking key ends in the identity.
    let patches: Vec<(SimId, Entity)> = world
        .try_query::<(Entity, &SimId, &ResourcePatch)>()
        .map(|mut query| query.iter(world).map(|(e, id, _)| (*id, e)).collect())
        .unwrap_or_default();
    patches
        .iter()
        .filter_map(|(id, e)| {
            let patch = world.get::<ResourcePatch>(*e)?;
            let pos = *world.get::<SimPos>(*e)?;
            if patch.kind != kind
                || patch.remaining == 0
                || (super::routing::avoiding(world, actor, pos, Some(*id)))
            {
                return None;
            }
            let distance =
                { i64::from(super::frontier::routes::estimate(world, here, pos)?.distance_mm) };
            Some((distance, *id, *e))
        })
        .min_by_key(|(distance, id, _)| (*distance, *id))
        .map(|(_, id, entity)| (id, entity))
}

/// Regional residents fetch urgent hearth fuel from a patch that is closer than
/// the nearest seller instead of freezing on a walk to another settlement.
pub(super) fn patch_nearer_than_seller(world: &World, actor: Entity, kind: &str) -> bool {
    let Some(here) = world.get::<SimPos>(actor).copied() else {
        return false;
    };
    let Some(patch) = gather_source(world, actor, kind)
        .and_then(|(_, source)| world.get::<SimPos>(source).copied())
    else {
        return false;
    };
    market_supplier(world, actor, kind)
        .and_then(|seller| world.get::<SimPos>(seller).copied())
        .is_none_or(|seller| seller.dist2(here) > patch.dist2(here))
}

pub(super) fn gather_job(world: &mut World, actor: Entity, kind: &str) -> bool {
    let Some((target, source)) = gather_source(world, actor, kind) else {
        return false;
    };
    super::production::intent(world, actor, "gather", kind, "", Some(target));
    if approach_work(world, actor, source) {
        let mut command = VillageAct::new(*world.get::<SimId>(actor).expect("actor"), "gather");
        command.target = Some(target);
        return command.perform(world).is_ok();
    }
    true
}

/// One wanted purchase: `(priority, distance squared, good, wanted stock)`.
/// Priority 0 is water, 1 food, 2 clothing and 3 every industrial buffer.
pub(super) type Procurement = (u8, i64, &'static str, u32);

/// Goods the funded plan of this actor wants and an affordable seller offers
/// now, nearest first within each priority.
pub(super) fn procurement(world: &World, actor: Entity) -> Vec<Procurement> {
    use super::production;
    let mut procure: Vec<_> = catalog::trade_kinds(world)
        .iter()
        .filter_map(|kind| {
            let mut wanted = production::desired_stock(world, actor, kind);
            if *kind == super::roles::roles(world).garment && super::policy::active(world).is_some()
            {
                // Equipping a replacement can precede the next ledger refresh.
                // A stale forecast must not buy another coat for the new one.
                wanted = wanted.min(super::living::clothing_reserve_target(world, actor));
            }
            if wanted <= econ::count(world, actor, kind) {
                return None;
            }
            let supplier = market_supplier(world, actor, kind)?;
            let priority = if *kind == super::roles::roles(world).drink {
                0
            } else if catalog::food_restore_in(world, kind).is_some() {
                1
            } else if *kind == super::roles::roles(world).garment {
                2
            } else {
                3
            };
            let here = *world.get::<SimPos>(actor)?;
            let distance = here.dist2(*world.get::<SimPos>(supplier)?);
            Some((priority, distance, *kind, wanted))
        })
        .collect();
    procure.sort_by_key(|(priority, distance, kind, _)| (*priority, *distance, *kind));
    procure
}

/// Buys the first listed good whose priority passes `wanted`.
pub(super) fn buy_listed(
    world: &mut World,
    actor: Entity,
    procure: &[Procurement],
    wanted: impl Fn(u8) -> bool,
) -> bool {
    for (_, _, kind, target) in procure.iter().filter(|(priority, ..)| wanted(*priority)) {
        if buy_market_input(world, actor, kind, *target) {
            return true;
        }
    }
    false
}

/// Puts on a carried coat through the shared equip interaction.
pub(super) fn equip_coat(world: &mut World, actor: Entity) -> bool {
    use super::production;
    if !production::garment_equipped(world, actor)
        && npc_framework::agent_io::registry::verb_gate(world, actor, "equip").is_none()
        && let Some(item) = npc_framework::interaction::items::carried_matching(
            world,
            actor,
            super::roles::roles(world).garment,
        )
        && npc_framework::interaction::begin(
            world,
            actor,
            "equip",
            item,
            None,
            npc_framework::interaction::Source::Command,
        )
        .is_ok()
    {
        production::intent(
            world,
            actor,
            "equip",
            super::roles::roles(world).garment,
            "",
            Some(item),
        );
        return true;
    }
    false
}

/// A cold resident with no usable hearth fuel must procure it before the
/// night-home branch can hold them indoors. Buying and finite gathering use
/// the normal shared actions; no warmth, fuel or currency is granted here.
fn fetch_fuel(world: &mut World, actor: Entity) -> bool {
    if let Some(home) = super::behavior::warming_need(world, actor)
        && !home.heated
        && home.carried_fuel < home.fuel_required
        && ((patch_nearer_than_seller(world, actor, home.fuel_kind)
            && gather_job(world, actor, home.fuel_kind))
            || buy_market_input(world, actor, home.fuel_kind, home.fuel_required)
            || gather_job(world, actor, home.fuel_kind))
    {
        return true;
    }
    false
}

/// Starts the best funded recipe whose inputs are already in the pack; with
/// `only`, considers just that recipe. Candidate scores use the shared integer
/// market and actual recipe costs. A pending need is required: price alone
/// never causes inventory quotas. The ranked candidates are appended to `seen`.
pub(super) fn craft_best(
    world: &mut World,
    actor: Entity,
    only: Option<&str>,
    seen: &mut Vec<(u64, String)>,
) -> bool {
    use super::{market, production};
    let role = world.get::<Villager>(actor).expect("villager").role;
    let mut candidates = Vec::new();
    for name in catalog::recipes_for(world, role) {
        if only.is_some_and(|only| only != *name) {
            continue;
        }
        let Some(recipe) = world.resource::<RecipeRegistry>().recipes.get(*name) else {
            continue;
        };
        let demand: u32 = recipe
            .outputs
            .iter()
            .map(|output| production::production_demand(world, actor, &output.kind))
            .fold(0, u32::saturating_add);
        if demand == 0
            || recipe
                .inputs
                .iter()
                .any(|input| econ::count(world, actor, &input.key) < input.count)
        {
            continue;
        }
        let revenue = recipe
            .outputs
            .iter()
            .map(|output| {
                u64::from(market::unit_price(world, &output.kind).unwrap_or(0))
                    * u64::from(output.count)
            })
            .sum::<u64>();
        let cost = u64::from(market::input_cost(world, name).unwrap_or(u32::MAX));
        // The planner has already financed this genuine job. Real inputs in
        // the pack need no second payment at their current book value.
        // Keep that value for ranking, not as a refusal of ready owned work.
        let new_cash_required = recipe
            .inputs
            .iter()
            .map(|input| {
                u64::from(
                    input
                        .count
                        .saturating_sub(econ::count(world, actor, &input.key)),
                ) * u64::from(market::unit_price(world, &input.key).unwrap_or(u32::MAX))
            })
            .sum::<u64>();
        let essential = recipe.outputs.iter().any(|output| {
            catalog::food_restore_in(world, &output.kind).is_some()
                || output.kind == super::roles::roles(world).garment
                || production::internal_demand(world, actor, &output.kind) > 0
        });
        if revenue <= new_cash_required && !essential {
            continue;
        }
        let score = revenue
            .saturating_sub(cost)
            .max(1)
            .saturating_mul(u64::from(demand.min(16)))
            .saturating_mul(10_000)
            / u64::from(recipe.duration_ticks.max(1));
        candidates.push((score, name.to_string(), recipe.outputs[0].kind.clone()));
    }
    candidates.sort_by(|(a, ar, _), (b, br, _)| b.cmp(a).then_with(|| ar.cmp(br)));
    seen.extend(
        candidates
            .iter()
            .map(|(score, recipe, _)| (*score, recipe.clone())),
    );
    if npc_framework::agent_io::registry::verb_gate(world, actor, "craft").is_none() {
        for (_, recipe, good) in candidates {
            let workplace = world.get::<Villager>(actor).expect("villager").workplace;
            let Some(station) = entity_of(world, workplace) else {
                continue;
            };
            let Some(pos) = world.get::<SimPos>(station).copied() else {
                continue;
            };
            if super::routing::avoiding(world, actor, pos, Some(workplace)) {
                continue;
            }
            production::intent(world, actor, "craft", &good, &recipe, Some(workplace));
            if !travel(world, actor, pos) {
                return true;
            }
            if begin_craft(
                world,
                actor,
                &recipe,
                Some(workplace),
                CraftSource::Command,
                None,
            )
            .is_ok()
            {
                pace_craft(world, actor);
                return true;
            }
        }
    }
    false
}

/// A hunter with a spear and funded demand approaches the nearest prey and hunts.
pub(super) fn hunt(world: &mut World, actor: Entity) -> bool {
    use super::production;
    let id = *world.get::<SimId>(actor).expect("actor identity");
    let role = world.get::<Villager>(actor).expect("villager").role;
    if role == super::roles::roles(world).hunter
        && (production::production_demand(world, actor, super::roles::roles(world).meat) > 0
            || production::production_demand(world, actor, super::roles::roles(world).hide) > 0)
        && npc_framework::interaction::items::carried_matching(
            world,
            actor,
            super::roles::roles(world).hunt_tool,
        )
        .is_some()
        && let Some((target, prey, at)) =
            stalked_prey(world, actor).or_else(|| prey_source(world, actor))
    {
        production::intent(
            world,
            actor,
            "hunt",
            super::roles::roles(world).meat,
            "",
            Some(target),
        );
        // Living world: the hunter keeps walking while its prey stays near
        // where it was going; it re-paths only once the prey moved away, or
        // once it is blocked, so the route recovery's detours and its giving
        // up apply (closing fix: a hunter stood blocked for 1433 ticks).
        let blocked = matches!(
            world.get::<MoveStatus>(actor),
            Some(MoveStatus::Blocked { .. })
        );
        if !blocked
            && !econ::near(world, actor, prey)
            && world
                .get::<MoveIntent>(actor)
                .is_some_and(|intent| intent.goal.dist2(at) <= STALK_REPATH_MM.pow(2))
        {
            return true;
        }
        if !approach_work(world, actor, prey) {
            return true;
        }
        let mut command = VillageAct::new(id, "hunt");
        command.target = Some(target);
        if command.perform(world).is_ok() {
            return true;
        }
    }
    false
}

/// Gathers the raw output of the profession with the most funded demand.
pub(super) fn gather_outputs(world: &mut World, actor: Entity) -> bool {
    use super::production;
    let role = world.get::<Villager>(actor).expect("villager").role;
    let mut raw: Vec<_> = catalog::outputs_for(world, role)
        .iter()
        .filter_map(|kind| {
            let demand = production::production_demand(world, actor, kind);
            // Regional job ranking weighs funded demand by the realised margin
            // of the good, so a glutted good yields to a better-paid one.
            let weight = {
                (1000 + super::frontier::labor::good_margin_permille(world, role, kind)).max(100)
                    as u64
            };
            // Regional: ingredients of food come before other raw goods.
            let staple = super::frontier::economy_spec::registry(world)
                .is_some_and(|registry| registry.feeds_food(kind));
            (demand > 0).then_some((staple, u64::from(demand) * weight, *kind))
        })
        .collect();
    raw.sort_by(|(af, a, ak), (bf, b, bk)| {
        bf.cmp(af).then_with(|| b.cmp(a)).then_with(|| ak.cmp(bk))
    });
    for (_, _, kind) in raw {
        if gather_job(world, actor, kind) {
            return true;
        }
    }
    false
}

/// Walks towards a free neighbour for a leisure meeting.
pub(super) fn socialise(world: &mut World, actor: Entity) -> bool {
    if let Some((_, meeting)) = super::colony::social_target(world, actor) {
        travel(world, actor, meeting);
        return true;
    }
    false
}

/// The work cascade of the profiles whose tree still has one decision leaf.
/// The regional profile runs the same steps as separate tree leaves.
fn living_work(world: &mut World, actor: Entity) {
    use super::production;
    // Lack of purchasing power is not lack of a physical survival action.
    // Use the exact finite gather command a player uses, then the ordinary
    // thirst branch consumes the real item on its next decision.
    if econ::need(world, actor, "Thirst") < 55_000
        && econ::count(world, actor, super::roles::roles(world).drink) == 0
        && market_supplier(world, actor, super::roles::roles(world).drink).is_none()
        && gather_job(world, actor, super::roles::roles(world).drink)
    {
        return;
    }
    if equip_coat(world, actor) {
        return;
    }
    // Subsistence remains a physical action when all posted food is absent or
    // unaffordable; this never creates a free recovery effect for NPCs.
    if econ::need(world, actor, "Hunger") < 40_000
        && production::food_choice(world, actor).is_none()
        && gather_job(world, actor, super::roles::roles(world).foraged)
    {
        return;
    }
    if fetch_fuel(world, actor) {
        return;
    }
    if super::frontier::labor::consider_assignment(world, actor) {
        return;
    }
    let procure = procurement(world, actor);
    // Personal reserves retain priority. Industrial buffers must not send an
    // actor away from a complete, financed job already waiting in their pack.
    if buy_listed(world, actor, &procure, |priority| priority < 3) {
        return;
    }
    if construction_step(world, actor) || planned_work(world, actor) {
        return;
    }
    if craft_best(world, actor, None, &mut Vec::new())
        || hunt(world, actor)
        || buy_listed(world, actor, &procure, |priority| priority == 3)
        || gather_outputs(world, actor)
    {
        return;
    }
    production::clear_intent(world, actor);
    if socialise(world, actor) {
        return;
    }
    world.entity_mut(actor).remove::<MoveIntent>();
}

/// A hunter re-paths to the animal it stalks only once the animal is this far
/// from where the hunter is walking to.
const STALK_REPATH_MM: i64 = 3_000;

/// Living world: the animal this hunter already stalks (its hunt intent), if
/// it is still prey: the hunter keeps it instead of choosing again.
fn stalked_prey(world: &World, actor: Entity) -> Option<(SimId, Entity, SimPos)> {
    let target = world
        .get::<super::production::JobIntent>(actor)
        .filter(|intent| intent.verb == "hunt")?
        .target?;
    super::hunting::available_prey(world)
        .into_iter()
        .find(|(id, ..)| *id == target)
}

fn prey_source(world: &World, actor: Entity) -> Option<(SimId, Entity, SimPos)> {
    let prey = super::hunting::available_prey(world);
    let here = *world.get::<SimPos>(actor)?;
    let region = super::production::economic_settlement(world, actor);
    prey.into_iter()
        .filter_map(|(id, entity, position)| {
            let distance = super::frontier::routes::estimate(world, here, position)?.distance_mm;
            let local = super::frontier::settlements::member(world, entity) == region;
            Some((!local, distance, id, entity, position))
        })
        .min_by_key(|(remote, distance, id, ..)| (*remote, *distance, *id))
        .map(|(_, _, id, entity, position)| (id, entity, position))
}

/// Execute the colony's read-only work request through the same movement and
/// homestead actions as a player. Housing owns the request, never the mover.
pub fn construction_step(world: &mut World, actor: Entity) -> bool {
    use super::colony::ConstructionIntent;
    let Some(intent) = super::colony::construction_intent(world, actor) else {
        return false;
    };
    let ConstructionIntent::Build {
        builder,
        plot,
        door,
        claim_required,
    } = intent
    else {
        return true;
    };
    super::production::intent(world, actor, "build_cottage", "cottage", "", Some(plot));
    if !travel(world, actor, door) {
        return true;
    }
    if claim_required
        && super::homestead::HomesteadAct::new(builder, "claim_plot", plot)
            .perform(world)
            .is_err()
    {
        return false;
    }
    super::homestead::HomesteadAct::new(builder, "build_cottage", plot)
        .perform(world)
        .is_ok()
}

/// Morale paces a resident's own craft within the bounded band. Inputs,
/// reservations and outputs are untouched; only the completion tick moves.
fn pace_craft(world: &mut World, actor: Entity) {
    let pace = u64::from(super::colony::work_pace_percent(world, actor));
    if let Some(mut craft) = world.get_mut::<Crafting>(actor) {
        let duration = craft.ends.saturating_sub(craft.started);
        craft.ends = craft.started.saturating_add((duration * pace / 100).max(1));
    }
}

/// Stuck exhaustion: the caller's travel has failed for good. Fail the running
/// life leaf so the tree chooses again instead of pushing at the obstacle.
fn fail_travel(world: &mut World, actor: Entity, goal: SimPos, target: Option<SimId>) {
    super::routing::abandon(world, actor, goal, target);
    super::production::clear_intent(world, actor);
    super::goap::forget(world, actor);
    if let Some(mut leaf) = world.get_mut::<ActiveLeaf>(actor) {
        leaf.status = Status::Failure;
    }
    // A leaf that is still starting has no `ActiveLeaf` yet.
    super::resident::travel_failed(world);
}

pub(super) fn planned_work(world: &mut World, actor: Entity) -> bool {
    use super::goap::{PlanStep, Recall};
    let plan = match super::goap::recall(world, actor) {
        Recall::Hit(plan) => plan,
        outcome => {
            if let Recall::Failed(reason) = outcome {
                let actor_id = world.get::<SimId>(actor).copied();
                record_effect(
                    world,
                    actor_id,
                    "VillagePlanInvalidated",
                    &json!({"reason":reason}),
                );
            }
            let Some(plan) = super::goap::plan(world, actor) else {
                super::goap::forget(world, actor);
                return false;
            };
            super::goap::remember(world, actor, &plan);
            plan
        }
    };
    let Some(step) = plan.steps.first() else {
        return false;
    };
    let previous = world.get::<super::production::JobIntent>(actor).cloned();
    let started = match step {
        PlanStep::Buy { kind, .. } => {
            let wanted = super::production::purchase_target(world, actor, kind)
                .max(econ::count(world, actor, kind).saturating_add(1));
            buy_market_input(world, actor, kind, wanted)
        }
        PlanStep::Gather { kind, .. } => gather_job(world, actor, kind),
        PlanStep::Craft {
            recipe,
            good,
            target,
        } => {
            let Some(station) = entity_of(world, *target) else {
                return false;
            };
            let Some(pos) = world.get::<SimPos>(station).copied() else {
                return false;
            };
            super::production::intent(world, actor, "craft", good, recipe, Some(*target));
            if !travel(world, actor, pos) {
                true
            } else {
                let begun = begin_craft(
                    world,
                    actor,
                    recipe,
                    Some(*target),
                    CraftSource::Command,
                    None,
                )
                .is_ok();
                if begun {
                    pace_craft(world, actor);
                }
                begun
            }
        }
    };
    if started && previous.as_ref() != world.get::<super::production::JobIntent>(actor) {
        let actor_id = world.get::<SimId>(actor).copied();
        record_effect(
            world,
            actor_id,
            "VillagePlanSelected",
            &json!({"goal":plan.goal,"steps":plan.steps,"expanded":plan.expanded,"cost":plan.cost}),
        );
    }
    started
}
struct VitalRules {
    needs: crate::worlds::NeedsSpec,
    night_vision_pct: u8,
}
fn vitals(world: &mut World, id: SimId, e: Entity, person: &Villager, now: u64) {
    let mut v = *world.get::<Vitality>(e).expect("vitality");
    if v.died_at.is_some() {
        return;
    }
    // A character still to arrive has not begun a life (closing fix C1): its
    // needs stay full, it cannot starve, thirst or freeze, and it raises no
    // alerts until the recorded arrival.
    if world.get::<super::colony::AwaitingArrival>(e).is_some() {
        if let Some(mut needs) = world.get_mut::<npc_framework::needs::Needs>(e) {
            for need in &mut needs.values {
                need.value = 100_000;
            }
        }
        return;
    }
    let hunger = econ::need(world, e, "Hunger");
    let thirst = econ::need(world, e, "Thirst");
    let energy = econ::need(world, e, "Energy");
    // Only the need rules and night sight are read; cloning the whole preset
    // (settlements, economy and behaviour data) per resident per tick is waste.
    let rules = {
        let preset = &world.resource::<crate::world_runtime::ActiveWorld>().0;
        VitalRules {
            needs: preset.needs.clone(),
            night_vision_pct: preset.day_night.night_vision_pct,
        }
    };
    v.starved_ticks = if hunger <= 100_000 - rules.needs.hunger.out_milli {
        v.starved_ticks.saturating_add(1)
    } else {
        0
    };
    v.dehydrated_ticks = if thirst <= 100_000 - rules.needs.thirst.out_milli {
        v.dehydrated_ticks.saturating_add(1)
    } else {
        0
    };
    // Onset events for the alerts feed; the journal never feeds the sim.
    if v.starved_ticks == 1 {
        record_effect(
            world,
            Some(id),
            "VillageStarvationOnset",
            &json!({"hunger":hunger,"health_loss_after_ticks":600}),
        );
    }
    if v.dehydrated_ticks == 1 {
        record_effect(
            world,
            Some(id),
            "VillageThirstOnset",
            &json!({"thirst":thirst,"health_loss_after_ticks":600}),
        );
    }
    if v.starved_ticks > 600 {
        v.health = v.health.saturating_sub(40);
    }
    if v.dehydrated_ticks > 600 {
        v.health = v.health.saturating_sub(80);
    }
    if energy <= 100_000 - rules.needs.energy.out_milli && now >= person.resting_until {
        v.exhaustion_ticks = v.exhaustion_ticks.saturating_add(1);
        stop_work(world, e, "exhausted");
        start_rest(world, e, 1200, 40);
        record_effect(
            world,
            Some(id),
            "VillageExhausted",
            &json!({"recovery":"slow ground rest; hunger and thirst continue"}),
        );
    }
    if person.resting_until > now {
        world.entity_mut(e).remove::<MoveIntent>();
        econ::restore(world, e, "Energy", person.rest_rate);
        if hunger > 50_000 && thirst > 50_000 {
            v.health = (v.health + 10).min(100_000);
        }
    }
    let pct = [
        (energy, rules.needs.energy),
        (hunger, rules.needs.hunger),
        (thirst, rules.needs.thirst),
    ]
    .into_iter()
    .filter(|(value, spec)| 100_000 - *value >= spec.low_milli)
    .map(|(_, spec)| i32::from(spec.low_speed_pct))
    .min()
    .unwrap_or(100);
    let speed = if person.resting_until > now {
        0
    } else {
        person.base_speed * pct / 100
    };
    if let Some(base) = world.get::<VillageSight>(e) {
        let mut sensors = base.0.clone();
        if world.resource::<SimConfig>().is_night(now) {
            for s in &mut sensors.sensors {
                if let npc_framework::perception::Sensor::Vision(vision) = s {
                    vision.range_mm = (i64::from(vision.range_mm)
                        * i64::from(rules.night_vision_pct)
                        / 100) as i32;
                }
            }
        }
        world.entity_mut(e).insert(sensors);
    }
    if let Some(mut c) = world.get_mut::<Commandable>(e) {
        c.speed_mm_per_tick = speed;
    }
    if let Some(mut u) = world.get_mut::<crate::IslandUnit>(e) {
        u.speed_mm_per_tick = speed;
    }
    if let Some(mut intent) = world.get_mut::<MoveIntent>(e) {
        intent.speed_mm_per_tick = speed;
    }
    if v.health <= 0 {
        v.health = 0;
        v.died_at = Some(now);
        let cause = if v.dehydrated_ticks > 600 {
            "thirst"
        } else if v.starved_ticks > 600 {
            "hunger"
        } else if econ::need(world, e, "Warmth") < 20_000 {
            "cold"
        } else {
            "exhaustion"
        };
        world.entity_mut(e).insert(v);
        die(world, id, e, person, now, cause);
    } else {
        world.entity_mut(e).insert(v);
    }
}
pub(crate) fn stop_work(world: &mut World, e: Entity, reason: &str) {
    npc_framework::agent_io::control::cancel_all(world, e);
    npc_framework::command::cancel_orders(world, e, reason);
    cancel_owned_work(world, e, reason);
}

/// One teardown for resident work and full external stops. Emergency arbitration
/// releases reservations and replans; it never resumes a stale action cursor.
/// External queues remain the responsibility of `stop_work` / control ownership.
pub(crate) fn cancel_owned_work(world: &mut World, e: Entity, reason: &str) {
    super::frontier::activities::cancel(world, e);
    // A building stage interrupted by daily life (a critical need) is
    // suspended, not lost: the builder finishes it later (S3.5).
    if reason == "daily life interrupted"
        && let Some(work) = world.get::<super::civic::SiteWork>(e).copied()
    {
        let now = world.resource::<SimClock>().tick;
        world.entity_mut(e).insert(super::civic::SuspendedWork {
            site: work.site,
            ticks: work.ticks,
            remaining: work.ends.saturating_sub(now).max(1),
            expires_at: now.saturating_add(super::civic::SUSPEND_TICKS),
        });
    }
    super::civic::cancel_work(world, e);
    super::live::cancel_work(world, e);
    super::fields::cancel(world, e);
    if world.get::<super::homestead::CottageBuild>(e).is_some() {
        super::homestead::cancel(world, e);
    }
    cancel_craft(world, e, reason);
    super::hunting::cancel(world, e, reason);
    npc_framework::interaction::cancel(world, e, reason);
    super::production::clear_intent(world, e);
    world.entity_mut(e).remove::<(Gathering, MoveIntent)>();
}
pub(crate) fn start_rest(world: &mut World, e: Entity, ticks: u64, rate: i32) {
    let now = world.resource::<SimClock>().tick;
    if let Some(mut v) = world.get_mut::<Villager>(e) {
        v.resting_until = now + ticks;
        v.rest_rate = rate;
    }
    world.entity_mut(e).remove::<MoveIntent>();
}
/// Ends a life for a cause other than a failed need (old age). Regional only.
pub(super) fn kill(world: &mut World, id: SimId, e: Entity, cause: &str) {
    let now = world.resource::<SimClock>().tick;
    let Some(person) = world.get::<Villager>(e).cloned() else {
        return;
    };
    let Some(mut vitality) = world.get_mut::<Vitality>(e) else {
        return;
    };
    if vitality.died_at.is_some() {
        return;
    }
    vitality.died_at = Some(now);
    die(world, id, e, &person, now, cause);
}

fn die(world: &mut World, id: SimId, e: Entity, person: &Villager, now: u64, cause: &str) {
    stop_work(world, e, "dead");
    // Regional residents leave their coins and plain goods to the household,
    // else to the civic fund; what is left is dropped as before.
    {
        super::demography::bequeath(world, id, e, cause);
    }
    let pos = world.get::<SimPos>(e).copied().unwrap_or_default();
    for item in items::carried_items(world, e) {
        if let Some(item) = entity_of(world, item) {
            items::place_in_world(world, item, pos);
        }
    }
    let coins = world.get_mut::<Wallet>(e).map_or(0, |mut w| {
        let c = w.coins;
        w.coins = 0;
        c
    });
    if coins > 0 {
        spawn_sim(
            world,
            (
                pos,
                Item::new(catalog::COINS),
                CoinPurse { coins },
                Perceivable::new(crate::world::kinds::ITEM),
            ),
        );
    }
    let diary = world.get::<VillageDiary>(e).cloned();
    let dropped = diary.map(|book| {
        spawn_sim(
            world,
            (
                pos,
                Item::new(catalog::DIARY),
                DroppedDiary {
                    author: id,
                    author_name: person.name.clone(),
                    life: person.life,
                    died_at: now,
                    pages: book.pages,
                    sealed: false,
                },
                Perceivable::new(crate::world::kinds::ITEM),
            ),
        )
        .0
    });
    world
        .entity_mut(e)
        .remove::<(BrainState, ActiveLeaf, CurrentOrder, MoveIntent, Interactor)>();
    if world.get::<Brain>(e).is_some() {
        let tree = world
            .resource::<AiRegistry>()
            .tree_id(super::REMAINS_TREE)
            .expect("registered remains tree");
        world.entity_mut(e).insert(Brain { tree });
    }
    world.entity_mut(e).remove::<(
        npc_framework::perception::RevealsFog,
        npc_framework::perception::ReachSense,
    )>();
    world
        .entity_mut(e)
        .insert(npc_framework::perception::Sensors::new());
    if let Some(mut c) = world.get_mut::<Commandable>(e) {
        c.speed_mm_per_tick = 0;
    }
    record_effect(
        world,
        Some(id),
        "VillageDeath",
        &json!({"permanent":true,"diary":dropped,"pos":[pos.x,pos.z],"coins_dropped":coins,
            "cause":cause,"name":person.name}),
    );
}
pub(crate) fn travel(world: &mut World, e: Entity, pos: SimPos) -> bool {
    let here = world.get::<SimPos>(e).copied().unwrap_or_default();
    if here.dist2(pos) <= 640_000 {
        world.entity_mut(e).remove::<MoveIntent>();
        super::routing::arrived(world, e);
        return true;
    }
    if super::routing::avoiding(world, e, pos, None) {
        world.entity_mut(e).remove::<MoveIntent>();
        return false;
    }
    if super::routing::exhausted(world, e, pos) {
        fail_travel(world, e, pos, None);
        return false;
    }
    let speed = world
        .get::<Commandable>(e)
        .map_or(70, |c| c.speed_mm_per_tick);
    let mut intent = MoveIntent::to(pos, speed);
    intent.tolerance_mm = 600;
    let intent = super::routing::intent(world, e, intent);
    if world.get::<MoveIntent>(e) != Some(&intent) {
        world.entity_mut(e).insert(intent);
    }
    false
}

/// Living work uses the very same reach as its shared actor action. Home rest
/// deliberately keeps the original tighter approach in `travel`.
pub(super) fn approach_work(world: &mut World, actor: Entity, target: Entity) -> bool {
    if econ::near(world, actor, target) {
        world.entity_mut(actor).remove::<MoveIntent>();
        super::routing::arrived(world, actor);
        return true;
    }
    let Some(pos) = world.get::<SimPos>(target).copied() else {
        return false;
    };
    {
        let id = world.get::<SimId>(target).copied();
        if super::routing::avoiding(world, actor, pos, id) {
            world.entity_mut(actor).remove::<MoveIntent>();
            return false;
        }
        if super::routing::exhausted(world, actor, pos) {
            fail_travel(world, actor, pos, id);
            return false;
        }
    }
    let speed = world
        .get::<Commandable>(actor)
        .map_or(70, |c| c.speed_mm_per_tick);
    let mut intent = MoveIntent::to(pos, speed);
    intent.tolerance_mm = 1500;
    let intent = super::routing::intent(world, actor, intent);
    if world.get::<MoveIntent>(actor) != Some(&intent) {
        world.entity_mut(actor).insert(intent);
    }
    false
}
pub(super) fn satisfy(
    world: &mut World,
    e: Entity,
    need: &str,
    good: &str,
    seller: Role,
    threshold: i32,
    _amount: i32,
) -> bool {
    if econ::need(world, e, need) >= threshold {
        return false;
    }
    let id = *world.get::<SimId>(e).expect("sim actor");
    let verb = if need == "Thirst" { "drink" } else { "eat" };
    let selected_food = true
        .then(|| super::production::food_choice(world, e))
        .flatten();
    let good = if need == "Hunger" {
        selected_food.unwrap_or(good)
    } else {
        good
    };
    let mut consume = VillageAct::new(id, verb);
    if need == "Hunger" {
        consume.kind = good.into();
    }
    if consume.perform(world).is_ok() {
        return true;
    }
    if let Some(s) = super::behavior::supplier(world, e, seller, good) {
        let price = super::market::asking_price(world, s, good);
        if world.get::<Wallet>(e).map_or(0, |w| w.coins) < price.unwrap_or(u32::MAX) {
            return false;
        }
        let reached = approach_work(world, e, s);
        if reached {
            let mut command = VillageAct::new(id, "buy");
            command.target = world.get::<SimId>(s).copied();
            command.kind = good.into();
            command.limit_price = super::market::asking_price(world, s, good).map(|price| {
                super::production::purchase_bid_limit(world, e, good)
                    .map_or(price, |limit| limit.min(price))
            });
            return command.perform(world).is_ok();
        }
        return true;
    }
    // Need recovery remains available at night and during optional recovery:
    // gather a real finite item, then consume it through the ordinary branch.
    let subsistence = if need == "Thirst" {
        super::roles::roles(world).drink
    } else {
        super::roles::roles(world).foraged
    };
    gather_job(world, e, subsistence)
}
pub(crate) fn begin_gather(
    world: &mut World,
    e: Entity,
    patch: Entity,
    id: SimId,
) -> Result<(), String> {
    let p = world
        .get::<ResourcePatch>(patch)
        .ok_or("not a resource patch")?;
    if !living(world, e) || !econ::near(world, e, patch) {
        return Err("move within 1.5 m of the resource patch".into());
    }
    if p.remaining == 0 {
        return Err("depleted until the next day".into());
    }
    if world.get::<Gathering>(e).is_some() || world.get::<Crafting>(e).is_some() {
        return Err("already working".into());
    }
    if world.get::<super::hunting::Hunting>(e).is_some() {
        return Err("already hunting".into());
    }
    econ::fit(world, e, &[Item::new(&p.kind)])?;
    let ends = world.resource::<SimClock>().tick
        + u64::from(
            super::living::gather_ticks(world, e, &p.kind, p.gather_ticks)
                // A played character's tool shed shortens its gathering.
                * super::live::gather_percent(world, e)
                / 100,
        )
        .max(1);
    world
        .entity_mut(e)
        .remove::<MoveIntent>()
        .insert(Gathering { patch: id, ends });
    Ok(())
}
fn finish_gathers(world: &mut World, now: u64) {
    let mut people: Vec<_> = world
        .query::<(Entity, &SimId, &Gathering)>()
        .iter(world)
        .filter(|(_, _, g)| g.ends <= now)
        .map(|(e, id, g)| (*id, e, *g))
        .collect();
    people.sort_by_key(|(id, _, _)| *id);
    for (id, e, g) in people {
        world.entity_mut(e).remove::<Gathering>();
        if !living(world, e) {
            continue;
        }
        let Some(patch) = entity_of(world, g.patch) else {
            continue;
        };
        let Some(p) = world.get::<ResourcePatch>(patch).cloned() else {
            continue;
        };
        if p.remaining == 0
            || !econ::near(world, e, patch)
            || econ::fit(world, e, &[Item::new(&p.kind)]).is_err()
            || catalog::gather_tool_required_in(world, &p.kind)
                .is_some_and(|tool| super::living::usable_tool(world, e, tool).is_none())
        {
            continue;
        }
        world
            .get_mut::<ResourcePatch>(patch)
            .expect("patch")
            .remaining -= 1;
        super::frontier::ecology::harvested(world, g.patch, 1);
        econ::grant(world, e, Item::new(&p.kind));
        super::living::wear_gather_tool(world, e, id, &p.kind);
        econ::produce(world, &p.kind, 1);
        super::production::completed(world, id, &p.kind, 1);
        super::live::booked(world, id, super::live::Book::Gathered, &p.kind, 1);
        record_effect(
            world,
            Some(id),
            "VillageGathered",
            &json!({"kind":p.kind,"patch":g.patch,"quantity":1}),
        );
        if p.remaining == 1 {
            // The unit just taken was the last one: an alert without a scan.
            record_effect(
                world,
                Some(id),
                "VillagePatchDepleted",
                &json!({"kind":p.kind,"patch":g.patch}),
            );
        }
    }
}
fn dividends(world: &mut World, day: u64) {
    if day <= world.resource::<VillageState>().last_dividend_day {
        return;
    }
    let mut people: Vec<_> = world
        .query::<(Entity, &SimId, &Villager)>()
        .iter(world)
        .filter(|(e, _, v)| v.role != Role::Visitor && living(world, *e))
        .map(|(e, id, _)| (*id, e))
        .collect();
    people.sort_by_key(|(id, _)| *id);
    civic_levy(world, day, &people);
}
/// Regional replacement for the daily wealth cap. Profit above the threshold is
/// only levied proportionally, so earnings accumulate and prices keep meaning.
/// The fund pays a destitution floor first; whatever exceeds its reserve returns
/// as an equal per-resident dividend, which leaves relative wealth unchanged and
/// keeps coins in circulation. No coin is created or destroyed.
fn civic_levy(world: &mut World, day: u64, people: &[(SimId, Entity)]) {
    let (threshold, percent, floor, reserve_each) = super::frontier::economy_spec::tuning(world)
        .map_or(
            (
                LEVY_THRESHOLD,
                LEVY_PERCENT,
                DESTITUTION_FLOOR,
                FUND_RESERVE_PER_RESIDENT,
            ),
            |tuning| {
                (
                    tuning.levy_threshold,
                    tuning.levy_percent,
                    tuning.destitution_floor,
                    tuning.fund_reserve_per_resident,
                )
            },
        );
    let mut fund = world.resource::<VillageState>().fund;
    let (mut contributed, mut granted, mut dividend) = (0_u32, 0_u32, 0_u32);
    for (_, e) in people {
        let mut wallet = world.get_mut::<Wallet>(*e).expect("wallet");
        let levy =
            (u64::from(wallet.coins.saturating_sub(threshold)) * u64::from(percent) / 100) as u32;
        let levy = levy.min(u32::MAX - fund);
        wallet.coins -= levy;
        fund += levy;
        contributed += levy;
    }
    // When the fund cannot bring everybody up to the floor, the rest is raised
    // from coins above it (regional relief levy). Floor grants never take the
    // coins the fund holds for open building sites: with them the sites
    // starved for weeks while the grants grew (pass 4).
    let wanted = people
        .iter()
        .map(|(_, e)| floor.saturating_sub(world.get::<Wallet>(*e).map_or(floor, |w| w.coins)))
        .fold(0_u32, u32::saturating_add);
    world.resource_mut::<VillageState>().fund = fund;
    let site_hold = if super::frontier::economy_spec::tuning(world)
        .is_some_and(|tuning| tuning.grants_spare_sites)
    {
        super::civic::fund_hold(world).min(fund)
    } else {
        0
    };
    let mut relief = 0;
    if wanted > fund - site_hold {
        relief = super::civic::raise_relief_levy(world, wanted - (fund - site_hold));
        fund = world.resource::<VillageState>().fund;
    }
    // Floor grants never outrun what the day's levies raised (closing fix
    // A): they spent the fund's reserve, and the fund sat at 0. The village
    // feeds itself through funded demand at the makers' cost
    // (`market::price_for`); the grant only tops up from today's levy.
    let mut grantable = fund
        .saturating_sub(site_hold)
        .min(contributed.saturating_add(relief));
    for (_, e) in people {
        let mut wallet = world.get_mut::<Wallet>(*e).expect("wallet");
        let grant = floor.saturating_sub(wallet.coins).min(grantable);
        grantable -= grant;
        wallet.coins += grant;
        fund -= grant;
        granted += grant;
    }
    super::civic::note_grants(world, granted, contributed);
    let count = people.len() as u32;
    // The reserve follows recent grants: what the planner and the grants may
    // still need stays in the fund, and only the rest is handed back.
    let reserve = count
        .saturating_mul(reserve_each)
        .max(super::civic::reserve(world));
    // Coins open building sites still need are spent there, not handed back:
    // the dividend returns only what the civic planner cannot use.
    let hold = super::civic::fund_hold(world);
    // Coins the building levy raised are already part of what sites hold.
    let share = fund
        .saturating_sub(reserve)
        .saturating_sub(hold)
        .checked_div(count)
        .unwrap_or(0);
    if share > 0 {
        for (_, e) in people {
            let mut wallet = world.get_mut::<Wallet>(*e).expect("wallet");
            if wallet.coins.checked_add(share).is_some() {
                wallet.coins += share;
                fund -= share;
                dividend += share;
            }
        }
    }
    let mut state = world.resource_mut::<VillageState>();
    state.fund = fund;
    state.last_dividend_day = day;
    // Grants came first; what building still needs beyond the fund's means
    // is raised from everybody's coins above the floor.
    super::civic::settle_earmark(world);
    super::civic::raise_building_levy(world);
    let fund = world.resource::<VillageState>().fund;
    record_effect(
        world,
        None,
        "VillageCivicLevy",
        &json!({"day":day,"levy_percent":percent,"threshold":threshold,
            "contributed":contributed,"floor":floor,"floor_grants":granted,
            "dividend":dividend,"balance":fund,"held_for_building":hold,
            "reserve":reserve,"relief_levy":relief}),
    );
}
pub(super) fn recover_found(world: &mut World, e: Entity) -> bool {
    // Only purses and diaries on the ground are lost property: two small
    // queries instead of a pass over every item in the world.
    let mut lost: Vec<(SimId, Entity)> = Vec::new();
    for (item, id, _) in world
        .query_filtered::<(Entity, &SimId, &Item), bevy_ecs::query::Or<(With<DroppedDiary>, With<CoinPurse>)>>()
        .iter(world)
    {
        if world
            .get::<npc_framework::interaction::CarriedBy>(item)
            .is_none()
            && econ::near(world, e, item)
        {
            lost.push((*id, item));
        }
    }
    lost.sort_by_key(|(id, _)| *id);
    if let Some((target, item)) = lost.first().copied() {
        let verb = if world.get::<CoinPurse>(item).is_some() {
            "collect_coins"
        } else {
            "recover_diary"
        };
        let mut command = VillageAct::new(*world.get::<SimId>(e).expect("sim actor"), verb);
        command.target = Some(target);
        return command.perform(world).is_ok();
    }
    false
}
