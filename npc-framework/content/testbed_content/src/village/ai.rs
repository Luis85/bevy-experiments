//! Autonomous production and survival, in sorted Effects order.
use super::{VillageAct, catalog, economy as econ, model::*};
use bevy_ecs::prelude::*;
use npc_framework::crafting::{CraftSource, Crafting, RecipeRegistry, begin_craft, cancel_craft};
use npc_framework::interaction::items::{self, entity_of};
use npc_framework::interaction::{Interactor, Item};
use npc_framework::prelude::*;
use serde_json::json;

pub fn tick(world: &mut World) {
    if super::needs_driven(world) {
        super::living::climate_tick(world);
    }
    let now = world.resource::<SimClock>().tick;
    let (day, season, _) = calendar(world);
    let mut people: Vec<_> = world
        .query::<(Entity, &SimId, &Villager)>()
        .iter(world)
        .map(|(e, id, v)| (*id, e, v.clone()))
        .collect();
    people.sort_by_key(|(id, _, _)| *id);
    for (id, e, v) in &people {
        vitals(world, *id, *e, v, now);
    }
    let mut patches: Vec<_> = world
        .query::<(Entity, &SimId, &ResourcePatch)>()
        .iter(world)
        .map(|(e, id, p)| (*id, e, p.clone()))
        .collect();
    patches.sort_by_key(|(id, _, _)| *id);
    if super::frontier::enabled(world) {
        super::frontier::ecology::tick(world);
    }
    for (_, e, mut p) in patches {
        if super::frontier::enabled(world) {
            break;
        }
        if now >= p.replenish_at {
            let capacity = if p.kind == catalog::GRAIN {
                match season {
                    Season::Winter => p.capacity / 4,
                    Season::Summer => p.capacity,
                    _ => p.capacity * 3 / 4,
                }
            } else {
                p.capacity
            };
            p.remaining = capacity;
            p.replenish_at = now + world.resource::<SimConfig>().day_length.max(1);
            world.entity_mut(e).insert(p);
        }
    }
    finish_gathers(world, now);
    if super::needs_driven(world) {
        super::hunting::tick(world);
    }
    dividends(world, day);
    if super::needs_driven(world) {
        super::production::refresh(world);
        super::market::tick(world);
    }
    super::behavior::advance(world);
}

/// One profession decision. All physical work uses the same action validation,
/// inventory, crafting and navigation seams as a player-controlled actor.
pub(super) fn work(world: &mut World, e: Entity) {
    if super::needs_driven(world) {
        living_work(world, e);
        return;
    }
    let v = world.get::<Villager>(e).expect("villager").clone();
    match v.role {
        Role::Farmer => gather_for(world, e, catalog::GRAIN),
        Role::WaterKeeper => gather_for(world, e, catalog::WATER),
        Role::Woodcutter => gather_for(world, e, "log"),
        Role::Miner => gather_for(world, e, "ore"),
        Role::Miller | Role::Baker | Role::Carpenter | Role::Smith => manufacture(world, e, &v),
        Role::Innkeeper => {
            for (kind, target) in [("log", 4), (catalog::BOWL, 2), ("hammer", 1)] {
                if econ::count(world, e, kind) < target {
                    buy_input(world, e, kind, target);
                    break;
                }
            }
        }
        Role::Visitor | Role::Hunter | Role::Tanner | Role::Tailor | Role::Fisher => {}
    }
}

/// Select a real, affordable advertised seller, including explicit player offers.
/// Distance is the only settlement condition deferred while approaching.
pub(super) fn market_supplier(world: &World, buyer: Entity, kind: &str) -> Option<Entity> {
    let coins = world.get::<Wallet>(buyer)?.coins;
    let here = *world.get::<SimPos>(buyer)?;
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, seller)| {
            if *seller == buyer
                || !super::market::sells(world, *seller, kind)
                || super::market::available_to_sell(world, *seller, kind) == 0
            {
                return None;
            }
            let price = super::market::asking_price(world, *seller, kind)?;
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
            let distance = if super::frontier::enabled(world) {
                i64::from(
                    super::frontier::routes::estimate(world, here, *world.get::<SimPos>(*seller)?)?
                        .distance_mm,
                )
            } else {
                here.dist2(*world.get::<SimPos>(*seller)?)
            };
            let local = !super::frontier::enabled(world)
                || super::production::economic_settlement(world, buyer)
                    == super::production::economic_settlement(world, *seller);
            Some((!local, price, distance, *id, *seller))
        })
        .min_by_key(|(remote, price, distance, id, _)| (*remote, *price, *distance, *id))
        .map(|(_, _, _, _, seller)| seller)
}

fn buy_market_input(world: &mut World, actor: Entity, kind: &str, wanted: u32) -> bool {
    let Some(seller) = market_supplier(world, actor, kind) else {
        return false;
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
            return true;
        }
    }
    false
}

/// Actionable finite subsistence, ignoring only the approach distance. This is
/// also the need guard's source of truth; an empty patch or full pack must not
/// suppress other executable work.
pub(super) fn can_forage(world: &World, actor: Entity, kind: &str) -> bool {
    gather_source(world, actor, kind).is_some()
}

pub(super) fn gather_source(world: &World, actor: Entity, kind: &str) -> Option<(SimId, Entity)> {
    if econ::fit(world, actor, &[Item::new(kind)]).is_err() {
        return None;
    }
    let here = world.get::<SimPos>(actor).copied().unwrap_or_default();
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, e)| {
            let patch = world.get::<ResourcePatch>(*e)?;
            let pos = *world.get::<SimPos>(*e)?;
            if patch.kind != kind || patch.remaining == 0 {
                return None;
            }
            let distance = if super::frontier::enabled(world) {
                i64::from(super::frontier::routes::estimate(world, here, pos)?.distance_mm)
            } else {
                here.dist2(pos)
            };
            Some((distance, *id, *e))
        })
        .min_by_key(|(distance, id, _)| (*distance, *id))
        .map(|(_, id, entity)| (id, entity))
}

fn gather_job(world: &mut World, actor: Entity, kind: &str) -> bool {
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

fn living_work(world: &mut World, actor: Entity) {
    use super::{market, production};
    let id = *world.get::<SimId>(actor).expect("actor identity");
    let role = world.get::<Villager>(actor).expect("villager").role;
    // Lack of purchasing power is not lack of a physical survival action.
    // Use the exact finite gather command a player uses, then the ordinary
    // thirst branch consumes the real item on its next decision.
    if econ::need(world, actor, "Thirst") < 55_000
        && econ::count(world, actor, catalog::WATER) == 0
        && market_supplier(world, actor, catalog::WATER).is_none()
        && gather_job(world, actor, catalog::WATER)
    {
        return;
    }
    if !production::garment_equipped(world, actor)
        && npc_framework::agent_io::registry::verb_gate(world, actor, "equip").is_none()
        && let Some(item) =
            npc_framework::interaction::items::carried_matching(world, actor, catalog::WARM_CLOTHES)
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
        production::intent(world, actor, "equip", catalog::WARM_CLOTHES, "", Some(item));
        return;
    }
    // Subsistence remains a physical action when all posted food is absent or
    // unaffordable; this never creates a free recovery effect for NPCs.
    if econ::need(world, actor, "Hunger") < 40_000
        && production::food_choice(world, actor).is_none()
        && gather_job(world, actor, catalog::BERRIES)
    {
        return;
    }

    // A cold resident with no usable hearth fuel must procure it before the
    // night-home branch can hold them indoors. Buying and finite gathering use
    // the normal shared actions; no warmth, fuel or currency is granted here.
    if let Some(home) = super::behavior::warming_need(world, actor)
        && !home.heated
        && home.carried_fuel < home.fuel_required
        && (buy_market_input(world, actor, home.fuel_kind, home.fuel_required)
            || gather_job(world, actor, home.fuel_kind))
    {
        return;
    }

    if super::frontier::enabled(world) && super::frontier::labor::consider_assignment(world, actor)
    {
        return;
    }

    let mut procure: Vec<_> = catalog::trade_kinds(world)
        .iter()
        .filter_map(|kind| {
            let mut wanted = production::desired_stock(world, actor, kind);
            if *kind == catalog::WARM_CLOTHES && super::policy::active(world).is_some() {
                // Equipping a replacement can precede the next ledger refresh.
                // A stale forecast must not buy another coat for the new one.
                wanted = wanted.min(super::living::clothing_reserve_target(world, actor));
            }
            if wanted <= econ::count(world, actor, kind) {
                return None;
            }
            let supplier = market_supplier(world, actor, kind)?;
            let priority = if *kind == catalog::WATER {
                0
            } else if catalog::food_restore(kind).is_some() {
                1
            } else if *kind == catalog::WARM_CLOTHES {
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
    // Personal reserves retain priority. Industrial buffers must not send an
    // actor away from a complete, financed job already waiting in their pack.
    for (_, _, kind, wanted) in procure.iter().filter(|(priority, ..)| *priority < 3) {
        if buy_market_input(world, actor, kind, *wanted) {
            return;
        }
    }

    if construction_step(world, actor) || planned_work(world, actor) {
        return;
    }

    // Candidate scores use the shared integer market and actual recipe costs.
    // A pending need is required: price alone never causes inventory quotas.
    let mut candidates = Vec::new();
    for name in catalog::recipes_for(world, role) {
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
        // the actor's pack need no second payment at their current book value.
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
            catalog::food_restore(&output.kind).is_some()
                || output.kind == catalog::WARM_CLOTHES
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
    if npc_framework::agent_io::registry::verb_gate(world, actor, "craft").is_none() {
        for (_, recipe, good) in candidates {
            let workplace = world.get::<Villager>(actor).expect("villager").workplace;
            let Some(station) = entity_of(world, workplace) else {
                continue;
            };
            let Some(pos) = world.get::<SimPos>(station).copied() else {
                continue;
            };
            production::intent(world, actor, "craft", &good, &recipe, Some(workplace));
            if !travel(world, actor, pos) {
                return;
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
                return;
            }
        }
    }
    if role == Role::Hunter
        && (production::production_demand(world, actor, catalog::RAW_MEAT) > 0
            || production::production_demand(world, actor, catalog::HIDE) > 0)
        && npc_framework::interaction::items::carried_matching(world, actor, catalog::SPEAR)
            .is_some()
        && let Some((target, prey, _)) = prey_source(world, actor)
    {
        production::intent(world, actor, "hunt", catalog::RAW_MEAT, "", Some(target));
        if !approach_work(world, actor, prey) {
            return;
        }
        let mut command = VillageAct::new(id, "hunt");
        command.target = Some(target);
        if command.perform(world).is_ok() {
            return;
        }
    }
    for (_, _, kind, wanted) in procure.into_iter().filter(|(priority, ..)| *priority == 3) {
        if buy_market_input(world, actor, kind, wanted) {
            return;
        }
    }
    let mut raw: Vec<_> = catalog::outputs_for(world, role)
        .iter()
        .filter_map(|kind| {
            let demand = production::production_demand(world, actor, kind);
            (demand > 0).then_some((demand, *kind))
        })
        .collect();
    raw.sort_by(|(a, ak), (b, bk)| b.cmp(a).then_with(|| ak.cmp(bk)));
    for (_, kind) in raw {
        if gather_job(world, actor, kind) {
            return;
        }
    }
    production::clear_intent(world, actor);
    if let Some((_, meeting)) = super::colony::social_target(world, actor) {
        travel(world, actor, meeting);
        return;
    }
    world.entity_mut(actor).remove::<MoveIntent>();
}

fn prey_source(world: &World, actor: Entity) -> Option<(SimId, Entity, SimPos)> {
    let prey = super::hunting::available_prey(world);
    if !super::frontier::enabled(world) {
        return prey.into_iter().next();
    }
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

fn planned_work(world: &mut World, actor: Entity) -> bool {
    use super::goap::PlanStep;
    let Some(plan) = super::goap::plan(world, actor) else {
        return false;
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
            !travel(world, actor, pos)
                || begin_craft(
                    world,
                    actor,
                    recipe,
                    Some(*target),
                    CraftSource::Command,
                    None,
                )
                .is_ok()
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
fn vitals(world: &mut World, id: SimId, e: Entity, person: &Villager, now: u64) {
    let mut v = *world.get::<Vitality>(e).expect("vitality");
    if v.died_at.is_some() {
        return;
    }
    let hunger = econ::need(world, e, "Hunger");
    let thirst = econ::need(world, e, "Thirst");
    let energy = econ::need(world, e, "Energy");
    let rules = world
        .resource::<crate::world_runtime::ActiveWorld>()
        .0
        .clone();
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
                        * i64::from(rules.day_night.night_vision_pct)
                        / 100) as i32;
                }
            }
        }
        world.entity_mut(e).insert(sensors);
    }
    if let Some(mut c) = world.get_mut::<Commandable>(e) {
        c.speed_mm_per_tick = speed;
    }
    if let Some(mut u) = world.get_mut::<crate::TestbedUnit>(e) {
        u.speed_mm_per_tick = speed;
    }
    if let Some(mut intent) = world.get_mut::<MoveIntent>(e) {
        intent.speed_mm_per_tick = speed;
    }
    if v.health <= 0 {
        v.health = 0;
        v.died_at = Some(now);
        world.entity_mut(e).insert(v);
        die(world, id, e, person, now);
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
    if world.get::<super::homestead::CottageBuild>(e).is_some() {
        super::homestead::cancel(world, e);
    }
    cancel_craft(world, e, reason);
    super::hunting::cancel(world, e, reason);
    if super::needs_driven(world) {
        npc_framework::interaction::cancel(world, e, reason);
        super::production::clear_intent(world, e);
    }
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
fn die(world: &mut World, id: SimId, e: Entity, person: &Villager, now: u64) {
    stop_work(world, e, "dead");
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
        &json!({"permanent":true,"diary":dropped,"pos":[pos.x,pos.z],"coins_dropped":coins}),
    );
}
pub(crate) fn travel(world: &mut World, e: Entity, pos: SimPos) -> bool {
    let here = world.get::<SimPos>(e).copied().unwrap_or_default();
    if here.dist2(pos) <= 640_000 {
        world.entity_mut(e).remove::<MoveIntent>();
        super::routing::arrived(world, e);
        return true;
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
fn approach_work(world: &mut World, actor: Entity, target: Entity) -> bool {
    if econ::near(world, actor, target) {
        world.entity_mut(actor).remove::<MoveIntent>();
        super::routing::arrived(world, actor);
        return true;
    }
    let Some(pos) = world.get::<SimPos>(target).copied() else {
        return false;
    };
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
    let selected_food = super::needs_driven(world)
        .then(|| super::production::food_choice(world, e))
        .flatten();
    let good = if need == "Hunger" {
        selected_food.unwrap_or(good)
    } else {
        good
    };
    let mut consume = VillageAct::new(id, verb);
    if super::needs_driven(world) && need == "Hunger" {
        consume.kind = good.into();
    }
    if consume.perform(world).is_ok() {
        return true;
    }
    if let Some(s) = super::behavior::supplier(world, e, seller, good) {
        let price = if super::needs_driven(world) {
            super::market::asking_price(world, s, good)
        } else {
            catalog::price(good)
        };
        if world.get::<Wallet>(e).map_or(0, |w| w.coins) < price.unwrap_or(u32::MAX) {
            return false;
        }
        let reached = if super::needs_driven(world) {
            approach_work(world, e, s)
        } else {
            let pos = world.get::<SimPos>(s).copied().unwrap_or_default();
            travel(world, e, pos)
        };
        if reached {
            let mut command = VillageAct::new(id, "buy");
            command.target = world.get::<SimId>(s).copied();
            command.kind = good.into();
            if super::needs_driven(world) {
                command.limit_price = super::market::asking_price(world, s, good).map(|price| {
                    super::production::purchase_bid_limit(world, e, good)
                        .map_or(price, |limit| limit.min(price))
                });
            }
            return command.perform(world).is_ok();
        }
        return true;
    }
    // Need recovery remains available at night and during optional recovery:
    // gather a real finite item, then consume it through the ordinary branch.
    if super::needs_driven(world) {
        let subsistence = if need == "Thirst" {
            catalog::WATER
        } else {
            catalog::BERRIES
        };
        return gather_job(world, e, subsistence);
    }
    // Supply can be empty. Producers must be able to restart that chain while hungry/thirsty.
    false
}
fn gather_for(world: &mut World, e: Entity, kind: &str) {
    if econ::count(world, e, kind) >= catalog::stock_limit(kind) {
        return;
    }
    let mut patches: Vec<_> = world
        .query::<(Entity, &SimId, &ResourcePatch, &SimPos)>()
        .iter(world)
        .filter(|(_, _, p, _)| p.kind == kind && p.remaining > 0)
        .map(|(e, id, _, p)| (*id, e, *p))
        .collect();
    patches.sort_by_key(|(id, _, _)| *id);
    if let Some((id, _, pos)) = patches.first().copied()
        && travel(world, e, pos)
    {
        let mut command = VillageAct::new(*world.get::<SimId>(e).expect("sim actor"), "gather");
        command.target = Some(id);
        let _ = command.perform(world);
    }
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
    let ends = world.resource::<SimClock>().tick + u64::from(p.gather_ticks);
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
            || (p.kind == catalog::FISH && econ::count(world, e, catalog::FISHING_ROD) == 0)
        {
            continue;
        }
        world
            .get_mut::<ResourcePatch>(patch)
            .expect("patch")
            .remaining -= 1;
        if super::frontier::enabled(world) {
            super::frontier::ecology::harvested(world, g.patch, 1);
        }
        econ::grant(world, e, Item::new(&p.kind));
        econ::produce(world, &p.kind, 1);
        super::production::completed(world, id, &p.kind, 1);
        record_effect(
            world,
            Some(id),
            "VillageGathered",
            &json!({"kind":p.kind,"patch":g.patch,"quantity":1}),
        );
    }
}
fn buy_input(world: &mut World, e: Entity, kind: &str, wanted: u32) -> bool {
    let mut suppliers: Vec<_> = world
        .query::<(Entity, &SimId, &Villager, &SimPos)>()
        .iter(world)
        .filter(|(s, _, v, _)| {
            *s != e
                && v.role != Role::Visitor
                && catalog::outputs(v.role).contains(&kind)
                && living(world, *s)
                && econ::count(world, *s, kind) > 0
        })
        .map(|(s, id, _, p)| (*id, s, *p))
        .collect();
    suppliers.sort_by_key(|(id, _, _)| *id);
    let Some((_, seller, pos)) = suppliers.first().copied() else {
        return false;
    };
    if travel(world, e, pos) {
        let available = econ::count(world, seller, kind);
        let coins = world.get::<Wallet>(e).map_or(0, |w| w.coins);
        let n = wanted
            .saturating_sub(econ::count(world, e, kind))
            .min(available)
            .min(8)
            .min(coins / catalog::price(kind).unwrap_or(1));
        if n > 0 {
            let mut command = VillageAct::new(*world.get::<SimId>(e).expect("sim actor"), "buy");
            command.target = world.get::<SimId>(seller).copied();
            command.kind = kind.into();
            // Prefer the buffer, but a small pack must still be able to buy
            // the smaller usable amount through exactly the same trade checks.
            for quantity in (1..=n).rev() {
                command.quantity = quantity;
                if command.check(world).is_ok() {
                    let _ = command.perform(world);
                    break;
                }
            }
        }
    }
    true
}
fn manufacture(world: &mut World, e: Entity, v: &Villager) {
    if npc_framework::agent_io::registry::verb_gate(world, e, "craft").is_some() {
        return;
    }
    let secondary = match v.role {
        Role::Carpenter => {
            econ::count(world, e, "plank") >= 8 && econ::count(world, e, catalog::BOWL) < 6
        }
        Role::Smith => econ::count(world, e, "ingot") >= 4 && econ::count(world, e, "hammer") < 3,
        _ => false,
    };
    let Some(recipe) = catalog::recipe(v.role, secondary) else {
        return;
    };
    let def = world
        .resource::<RecipeRegistry>()
        .recipes
        .get(recipe)
        .expect("village recipe")
        .clone();
    if def
        .outputs
        .iter()
        .all(|o| econ::count(world, e, &o.kind) >= catalog::stock_limit(&o.kind))
    {
        return;
    }
    for input in &def.inputs {
        if econ::count(world, e, &input.key) < input.count {
            // A supplier is a moving resident, often across the island. Stock
            // several batches per paid visit so the food chain spends daylight
            // producing instead of repeatedly crossing the same route. This is
            // still an ordinary finite, capacity-checked trade (at most eight).
            buy_input(world, e, &input.key, input.count.max(8));
            return;
        }
    }
    let Some(station) = entity_of(world, v.workplace) else {
        return;
    };
    let pos = world.get::<SimPos>(station).copied().unwrap_or_default();
    if travel(world, e, pos) {
        let _ = begin_craft(
            world,
            e,
            recipe,
            Some(v.workplace),
            CraftSource::Command,
            None,
        );
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
    let mut fund = world.resource::<VillageState>().fund;
    let mut contributed = 0;
    let mut paid = 0;
    for (_, e) in &people {
        let mut w = world.get_mut::<Wallet>(*e).expect("wallet");
        let excess = w.coins.saturating_sub(NPC_SEED_COINS);
        w.coins -= excess;
        fund += excess;
        contributed += excess;
    }
    for (_, e) in &people {
        let mut w = world.get_mut::<Wallet>(*e).expect("wallet");
        let grant = 100_u32.saturating_sub(w.coins).min(fund);
        w.coins += grant;
        fund -= grant;
        paid += grant;
    }
    for (_, e) in &people {
        if world
            .get::<Villager>(*e)
            .is_some_and(|v| v.role == Role::Innkeeper)
        {
            let _ = econ::consume(world, *e, "log", 1);
            if day.is_multiple_of(4) {
                let _ = econ::consume(world, *e, catalog::BOWL, 1);
            }
            if day.is_multiple_of(7) {
                let _ = econ::consume(world, *e, "hammer", 1);
            }
        }
    }
    let mut state = world.resource_mut::<VillageState>();
    state.fund = fund;
    state.last_dividend_day = day;
    record_effect(
        world,
        None,
        "VillageCommunityFund",
        &json!({"day":day,"contributed":contributed,"paid":paid,"balance":fund}),
    );
}
pub(super) fn recover_found(world: &mut World, e: Entity) {
    let mut lost: Vec<_> = world
        .query::<(Entity, &SimId, &Item)>()
        .iter(world)
        .filter(|(item, _, _)| {
            world
                .get::<npc_framework::interaction::CarriedBy>(*item)
                .is_none()
                && (world.get::<DroppedDiary>(*item).is_some()
                    || world.get::<CoinPurse>(*item).is_some())
                && econ::near(world, e, *item)
        })
        .map(|(e, id, _)| (*id, e))
        .collect();
    lost.sort_by_key(|(id, _)| *id);
    if let Some((target, item)) = lost.first().copied() {
        let verb = if world.get::<CoinPurse>(item).is_some() {
            "collect_coins"
        } else {
            "recover_diary"
        };
        let mut command = VillageAct::new(*world.get::<SimId>(e).expect("sim actor"), verb);
        command.target = Some(target);
        let _ = command.perform(world);
    }
}
