//! Villager content: conditions and the villager behavior tree — Task D.
//!
//! The tree (node names appear in the overlay path and in tests):
//!
//! ```text
//! Selector "Villager"
//! ├ Guard "UrgentEat"   [hunger_urgent_food, LowerPriority] → Sequence "EatSeq"   [ReserveFood, MoveToDest, EatFood]
//! ├ Guard "UrgentDrink" [thirst_urgent_well, LowerPriority] → Sequence "DrinkSeq" [ReserveWell, MoveToDest, DrinkWater]
//! ├ Guard "UrgentSleep" [energy_urgent,      LowerPriority] → Sequence "SleepSeq" [ReserveBed, MoveToDest, SleepInBed(Full)]
//! ├ Guard "Forage"      [forage,             LowerPriority] → Sequence "WorkSeq"  [ReserveWork, MoveToDest, DoWork(AnyTime)]
//! ├ Guard "FollowOrder" [has_order,          LowerPriority] → Leaf "MoveToOrder" (action FollowOrder)
//! ├ Guard "NightSleep"  [is_night,     None] → Sequence "SleepSeq" [ReserveBed, MoveToDest, SleepInBed(Morning)]
//! ├ Guard "CasualDrink" [thirsty_well, None] → Sequence "DrinkSeq" [ReserveWell, MoveToDest, DrinkWater]
//! ├ Guard "CasualEat"   [peckish_food, None] → Sequence "EatSeq"   [ReserveFood, MoveToDest, EatFood]
//! ├ Guard "DayWork"     [day_work,     None] → Sequence "WorkSeq"  [ReserveWork, MoveToDest, DoWork(DayOnly)]
//! └ Sequence "Wander" [PickWanderPoint, MoveToDest, IdleFor(200)]
//! ```
//!
//! Every `LowerPriority` guard condition must imply its child can start (see
//! [`villager_tree`]).
//!
//! Availability is always judged from the asking agent's point of view: its
//! own claims count as available to it (they are released by every `Reserve*`
//! before it re-claims), only other agents' claims are subtracted.

use bevy_ecs::prelude::*;
use npc_framework::jobs::{free_for, free_slots};
use npc_framework::kernel::orders::cond_has_order;
use npc_framework::prelude::*;

use crate::actions::{
    SLEEP_FULL, SLEEP_MORNING, WORK_ANY_TIME, WORK_DAY_ONLY, do_work_start, drink_water_start,
    eat_food_start, idle_for_start, instant_stop, move_to_dest_start, pick_wander_point_start,
    reserve_bed_start, reserve_food_start, reserve_well_start, reserve_work_start,
    sleep_in_bed_start, stop_drink, stop_eat, stop_idle, stop_move, stop_sleep, stop_work,
};
use crate::needs::{ENERGY, HUNGER, THIRST, need_id};
use crate::orders::{follow_order_start, stop_follow_order};
use crate::world::{Bed, Stockpile, Villager, Well, WorkSite};

/// Blackboard slot: `BbValue::U32(SimId)` of the place the current branch uses.
pub const BB_TARGET: BbKey = BbKey(0);
/// Blackboard slot: `BbValue::Pos(x, z)` the next `MoveToDest` walks to.
pub const BB_DEST: BbKey = BbKey(1);

/// Name of the villager tree.
pub const VILLAGER_TREE: &str = "villager";

/// Id of the registered villager tree (inserted by `VillagePlugin` once
/// content is frozen).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct VillagerTree(pub TreeId);

/// Registers the villager's actions and conditions by name.
pub fn register_actions_and_conds(app: &mut bevy_app::App) {
    app.register_action("ReserveFood", reserve_food_start, instant_stop)
        .register_action("ReserveBed", reserve_bed_start, instant_stop)
        .register_action("ReserveWork", reserve_work_start, instant_stop)
        .register_action("ReserveWell", reserve_well_start, instant_stop)
        .register_action("MoveToDest", move_to_dest_start, stop_move)
        .register_action("EatFood", eat_food_start, stop_eat)
        .register_action("DoWork", do_work_start, stop_work)
        .register_action("SleepInBed", sleep_in_bed_start, stop_sleep)
        .register_action("DrinkWater", drink_water_start, stop_drink)
        .register_action("PickWanderPoint", pick_wander_point_start, instant_stop)
        .register_action("IdleFor", idle_for_start, stop_idle)
        .register_action("FollowOrder", follow_order_start, stop_follow_order)
        .register_cond("hunger_urgent_food", cond_hunger_urgent_food)
        .register_cond("energy_urgent", cond_energy_urgent)
        .register_cond("thirst_urgent_well", cond_thirst_urgent_well)
        .register_cond("thirsty_well", cond_thirsty_well)
        .register_cond("forage", cond_forage)
        .register_cond("is_night", cond_is_night)
        .register_cond("peckish_food", cond_peckish_food)
        .register_cond("day_work", cond_day_work)
        .register_cond("has_order", cond_has_order);
}

/// Builds the villager tree (registered as [`VILLAGER_TREE`]); leaves and
/// guards name their actions and conditions.
///
/// Rule for this tree: **every `Abort::LowerPriority` guard condition must
/// imply that its child can start** (the target it reserves exists and is
/// free for this agent, and its first leaves will not fail at once). A
/// reactive guard whose child then fails still aborts the running branch, so
/// a condition that is true while its branch cannot run turns every re-think
/// into an abort and a restart of the lower-priority work. Hence
/// `hunger_urgent_food` checks for available food, `thirst_urgent_well` for
/// a well with a free slot, `energy_urgent` for a free bed, and `forage` for
/// a free work site (and that no work site is already held, which would be
/// the very work being aborted). `has_order`
/// needs nothing more: `FollowOrder` always starts (or succeeds at once).
pub fn villager_tree(b: &mut TreeBuilder) {
    fn eat_seq(b: &mut TreeBuilder) {
        b.sequence("EatSeq", |b| {
            b.leaf("ReserveFood", "ReserveFood", ());
            b.leaf("MoveToDest", "MoveToDest", ());
            b.leaf("EatFood", "EatFood", ());
        });
    }
    fn sleep_seq(b: &mut TreeBuilder, until: &str) {
        b.sequence("SleepSeq", |b| {
            b.leaf("ReserveBed", "ReserveBed", ());
            b.leaf("MoveToDest", "MoveToDest", ());
            b.leaf("SleepInBed", "SleepInBed", until);
        });
    }
    fn drink_seq(b: &mut TreeBuilder) {
        b.sequence("DrinkSeq", |b| {
            b.leaf("ReserveWell", "ReserveWell", ());
            b.leaf("MoveToDest", "MoveToDest", ());
            b.leaf("DrinkWater", "DrinkWater", ());
        });
    }
    fn work_seq(b: &mut TreeBuilder, when: &str) {
        b.sequence("WorkSeq", |b| {
            b.leaf("ReserveWork", "ReserveWork", ());
            b.leaf("MoveToDest", "MoveToDest", ());
            b.leaf("DoWork", "DoWork", when);
        });
    }

    b.selector("Villager", |b| {
        b.guard(
            "UrgentEat",
            "hunger_urgent_food",
            Abort::LowerPriority,
            eat_seq,
        );
        // Below urgent hunger (a starving villager eats first), above every
        // other branch: urgent thirst interrupts sleep, work and orders.
        // Thirst does not decay while asleep, so it only wakes a sleeper who
        // went to bed already urgently thirsty.
        b.guard(
            "UrgentDrink",
            "thirst_urgent_well",
            Abort::LowerPriority,
            drink_seq,
        );
        b.guard("UrgentSleep", "energy_urgent", Abort::LowerPriority, |b| {
            sleep_seq(b, SLEEP_FULL)
        });
        b.guard("Forage", "forage", Abort::LowerPriority, |b| {
            work_seq(b, WORK_ANY_TIME)
        });
        // RTS orders (spec §6.6): above night sleep, below the survival
        // branches. `has_order` implies the child can start.
        b.guard("FollowOrder", "has_order", Abort::LowerPriority, |b| {
            b.leaf("MoveToOrder", "FollowOrder", ());
        });
        b.guard("NightSleep", "is_night", Abort::None, |b| {
            sleep_seq(b, SLEEP_MORNING)
        });
        b.guard("CasualDrink", "thirsty_well", Abort::None, drink_seq);
        b.guard("CasualEat", "peckish_food", Abort::None, eat_seq);
        b.guard("DayWork", "day_work", Abort::None, |b| {
            work_seq(b, WORK_DAY_ONLY)
        });
        b.sequence("Wander", |b| {
            b.leaf("PickWanderPoint", "PickWanderPoint", ());
            b.leaf("MoveToDest", "MoveToDest", ());
            b.leaf("IdleFor", "IdleFor", 200);
        });
    });
}

// ---------------------------------------------------------------------------
// Smart-object queries (shared by conditions and actions)
// ---------------------------------------------------------------------------

/// `food − reservations.count(stockpile)`, saturating.
pub fn available_food(food: u32, stockpile: SimId, reservations: &Reservations) -> u32 {
    food.saturating_sub(reservations.count(stockpile))
}

/// Food in `stockpile` available to `me`: its own claim on the stockpile does
/// not count against it (only other claimants' do).
pub fn available_food_for(
    food: u32,
    stockpile: SimId,
    me: SimId,
    reservations: &Reservations,
) -> u32 {
    // Each food unit is one slot of the stockpile.
    free_slots(reservations, stockpile, food, me)
}

/// Lowest-SimId stockpile with food available to `me`.
pub fn find_food(world: &World, me: SimId) -> Option<(SimId, Entity)> {
    let index = world.get_resource::<SimIndex>()?;
    let reservations = world.get_resource::<Reservations>()?;
    index.map.iter().find_map(|(&id, &entity)| {
        let pile = world.get::<Stockpile>(entity)?;
        (available_food_for(pile.food, id, me, reservations) > 0).then_some((id, entity))
    })
}

/// Lowest-SimId work site free for `me`. With `need_room`, only sites whose
/// stockpile is below capacity count. Without it, sites with room are still
/// preferred over sites whose stockpile is full.
pub fn find_work_site(world: &World, me: SimId, need_room: bool) -> Option<(SimId, Entity)> {
    let index = world.get_resource::<SimIndex>()?;
    let reservations = world.get_resource::<Reservations>()?;
    let mut fallback = None;
    for (&id, &entity) in &index.map {
        let Some(site) = world.get::<WorkSite>(entity) else {
            continue;
        };
        if !free_for(reservations, id, me) {
            continue;
        }
        let has_room = index
            .get(site.stockpile)
            .and_then(|e| world.get::<Stockpile>(e))
            .is_some_and(|pile| pile.food < pile.capacity);
        if has_room {
            return Some((id, entity));
        }
        if !need_room && fallback.is_none() {
            fallback = Some((id, entity));
        }
    }
    fallback
}

/// True when `me` holds a reservation on any `WorkSite`.
pub fn holds_work_site(world: &World, me: SimId) -> bool {
    let (Some(index), Some(reservations)) = (
        world.get_resource::<SimIndex>(),
        world.get_resource::<Reservations>(),
    ) else {
        return false;
    };
    reservations.claims.iter().any(|(&target, claimants)| {
        claimants.contains(&me)
            && index
                .get(target)
                .is_some_and(|e| world.get::<WorkSite>(e).is_some())
    })
}

/// Lowest-SimId bed of `home` that is free for `me` and provides energy.
pub fn find_bed(world: &World, me: SimId, home: SimId) -> Option<(SimId, Entity)> {
    let energy = world.get_resource::<NeedRegistry>()?.id(ENERGY)?;
    let index = world.get_resource::<SimIndex>()?;
    let reservations = world.get_resource::<Reservations>()?;
    index.map.iter().find_map(|(&id, &entity)| {
        let bed = world.get::<Bed>(entity)?;
        let provides = world.get::<Provides>(entity)?;
        (bed.home == home && provides.need == energy && free_for(reservations, id, me))
            .then_some((id, entity))
    })
}

/// The nearest well (by its approach, ties by SimId) that provides thirst
/// and has a slot free for the agent `me` standing at `from` (`free_slots`
/// of its `Well::slots`: `me`'s own claim does not count against it).
pub fn find_well(world: &World, me: SimId, from: SimPos) -> Option<(SimId, Entity)> {
    let thirst = world.get_resource::<NeedRegistry>()?.id(THIRST)?;
    let index = world.get_resource::<SimIndex>()?;
    let reservations = world.get_resource::<Reservations>()?;
    index
        .map
        .iter()
        .filter_map(|(&id, &entity)| {
            let well = world.get::<Well>(entity)?;
            let provides = world.get::<Provides>(entity)?;
            let approach = npc_framework::jobs::approach_of(world, entity)?;
            (provides.need == thirst && free_slots(reservations, id, well.slots, me) > 0)
                .then_some((from.dist2(approach), id, entity))
        })
        .min_by_key(|&(d2, id, _)| (d2, id))
        .map(|(_, id, entity)| (id, entity))
}

/// [`find_well`] for an agent entity (at its `SimPos`).
pub fn find_well_for(world: &World, agent: Entity) -> Option<(SimId, Entity)> {
    let me = *world.get::<SimId>(agent)?;
    let from = *world.get::<SimPos>(agent)?;
    find_well(world, me, from)
}

/// It is night by `SimClock`/`SimConfig` (false when either is missing).
pub fn is_night(world: &World) -> bool {
    match (
        world.get_resource::<SimClock>(),
        world.get_resource::<SimConfig>(),
    ) {
        (Some(clock), Some(config)) => config.is_night(clock.tick),
        _ => false,
    }
}

/// The agent's id and its hunger, energy and thirst levels.
struct Agent {
    me: SimId,
    hunger: NeedLevel,
    energy: NeedLevel,
    thirst: NeedLevel,
}

fn agent(world: &World, entity: Entity) -> Option<Agent> {
    let needs = world.get::<Needs>(entity)?;
    Some(Agent {
        me: *world.get::<SimId>(entity)?,
        hunger: needs.get(need_id(world, HUNGER)).level,
        energy: needs.get(need_id(world, ENERGY)).level,
        thirst: needs.get(need_id(world, THIRST)).level,
    })
}

// ---------------------------------------------------------------------------
// Conditions
// ---------------------------------------------------------------------------

/// Hunger is Urgent and some stockpile has food available.
pub fn cond_hunger_urgent_food(world: &World, entity: Entity) -> bool {
    agent(world, entity)
        .is_some_and(|a| a.hunger == NeedLevel::Urgent && find_food(world, a.me).is_some())
}

/// Thirst is Urgent and a well has a slot free for the agent.
pub fn cond_thirst_urgent_well(world: &World, entity: Entity) -> bool {
    agent(world, entity)
        .is_some_and(|a| a.thirst == NeedLevel::Urgent && find_well_for(world, entity).is_some())
}

/// Thirst is at least Low and a well has a slot free for the agent.
pub fn cond_thirsty_well(world: &World, entity: Entity) -> bool {
    agent(world, entity)
        .is_some_and(|a| a.thirst >= NeedLevel::Low && find_well_for(world, entity).is_some())
}

/// Energy is Urgent and a bed of the agent's home is free for it.
pub fn cond_energy_urgent(world: &World, entity: Entity) -> bool {
    let Some(villager) = world.get::<Villager>(entity) else {
        return false;
    };
    agent(world, entity).is_some_and(|a| {
        a.energy == NeedLevel::Urgent && find_bed(world, a.me, villager.home).is_some()
    })
}

/// Hunger is Urgent, no food is available anywhere, a work site is free (day
/// or night: he never starves in the dark), and the agent does not already
/// hold a work site (a running work branch, e.g. DayWork, already produces
/// food; aborting it would only restart the shift from scratch).
pub fn cond_forage(world: &World, entity: Entity) -> bool {
    agent(world, entity).is_some_and(|a| {
        a.hunger == NeedLevel::Urgent
            && find_food(world, a.me).is_none()
            && !holds_work_site(world, a.me)
            && find_work_site(world, a.me, false).is_some()
    })
}

/// It is night.
pub fn cond_is_night(world: &World, _entity: Entity) -> bool {
    is_night(world)
}

/// Hunger is at least Low ("peckish") and some stockpile has food available.
pub fn cond_peckish_food(world: &World, entity: Entity) -> bool {
    agent(world, entity)
        .is_some_and(|a| a.hunger >= NeedLevel::Low && find_food(world, a.me).is_some())
}

/// Daytime, and a free work site whose stockpile is below capacity exists.
pub fn cond_day_work(world: &World, entity: Entity) -> bool {
    !is_night(world)
        && world
            .get::<SimId>(entity)
            .is_some_and(|&me| find_work_site(world, me, true).is_some())
}
