//! Villager action components, their start/stop functions and their
//! `NpcSet::Actions` systems — Task D.
//!
//! Each running action is a `SparseSet` component on the agent. Its system
//! advances it and sets `ActiveLeaf.status` to Success/Failure when done;
//! cleanup happens in the action's `StopFn` (called by `npc_ai::think`).
//!
//! Reservation rules (no claim may outlive the branch that made it):
//! - every `Reserve*` start first releases all of the agent's claims;
//! - every stop releases all of the agent's claims on `StopReason::Aborted`;
//! - the terminal action of a branch (eat/drink/work/sleep) releases its
//!   target on `StopReason::Finished` (eat and drink already in the tick they
//!   complete);
//! - a start that returns `Failure` (which fails the whole sequence) releases
//!   all of the agent's claims too, since no stop is called for it.
//!
//! Every claim attempt is journaled as `Reserve { target, ok }` and every
//! claim actually dropped as `Release { target, reason }`.

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::jobs::{approach_of, release, release_all, reserve};
use npc_framework::kernel::hash::Fnv64;
use npc_framework::kernel::journal::{JournalKind, ReleaseReason};
use npc_framework::kernel::rng::sim_range;
use npc_framework::locomotion::{MoveIntent, MoveStatus, clear_move_intent, set_move_intent};
use npc_framework::needs::PauseDecay;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

use crate::activity::Activity;
use crate::effects::{EffectKind, SimEffect};
use crate::needs::{ENERGY, THIRST, need_id};
use crate::villager::{
    BB_DEST, BB_TARGET, find_bed, find_food, find_well_for, find_work_site, is_night,
};
use crate::world::{Bed, Stockpile, Villager, Well, WorkSite};

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct MoveTo {
    pub goal: SimPos,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct Work {
    pub site: SimId,
    pub remaining: u32,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct Eat {
    pub stockpile: SimId,
    pub remaining: u32,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct Drink {
    pub well: SimId,
    pub remaining: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum SleepUntil {
    /// Until energy is full (urgent sleep).
    Full,
    /// Until it is day and energy is above `energy.low_exit` (night sleep).
    Morning,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct Sleep {
    pub bed: SimId,
    pub until: SleepUntil,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct Idle {
    pub remaining: u32,
}

impl SimHash for MoveTo {
    const NAME: &'static str = "move_to";
    fn sim_hash(&self, h: &mut Fnv64) {
        self.goal.sim_hash(h);
    }
}

impl SimHash for Work {
    const NAME: &'static str = "work";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.site.0);
        h.u32(self.remaining);
    }
}

impl SimHash for Eat {
    const NAME: &'static str = "eat";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.stockpile.0);
        h.u32(self.remaining);
    }
}

impl SimHash for Drink {
    const NAME: &'static str = "drink";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.well.0);
        h.u32(self.remaining);
    }
}

impl SimHash for Sleep {
    const NAME: &'static str = "sleep";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.bed.0);
        h.u8(match self.until {
            SleepUntil::Full => 0,
            SleepUntil::Morning => 1,
        });
    }
}

impl SimHash for Idle {
    const NAME: &'static str = "idle";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.remaining);
    }
}

/// Ticks one meal takes.
pub const EAT_TICKS: u32 = 60;
/// Food units one meal consumes.
pub const MEAL_FOOD: u32 = 1;
/// Ticks one drink at the well takes.
pub const DRINK_TICKS: u32 = 40;
/// `SleepInBed` param (`ParamValue::Name`): sleep until energy is full.
pub const SLEEP_FULL: &str = "Full";
/// `SleepInBed` param (`ParamValue::Name`): sleep until morning (and rested).
pub const SLEEP_MORNING: &str = "Morning";
/// `DoWork` param (`ParamValue::Name`): a new shift may only start by day
/// (start fails at night).
pub const WORK_DAY_ONLY: &str = "DayOnly";
/// `DoWork` param (`ParamValue::Name`): a new shift may start at any time
/// (foraging).
pub const WORK_ANY_TIME: &str = "AnyTime";
/// Wander points lie within this many mm of the home approach, per axis.
pub const WANDER_RADIUS: i32 = 5000;
/// Wander points are clamped to ±this on each axis (the tile is ±16 000)
/// when the home lies on the tile; homes off the tile (large `village_n`
/// layouts) wander unclamped around themselves.
pub const WANDER_LIMIT: i32 = 14_000;

const RNG_WANDER_X: u32 = 1;
const RNG_WANDER_Z: u32 = 2;
const RNG_IDLE: u32 = 3;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn sim_id(world: &World, agent: Entity) -> Option<SimId> {
    world.get::<SimId>(agent).copied()
}

/// Fails the leaf (and so its sequence): drops every claim the branch made.
pub(crate) fn fail(world: &mut World, agent: Entity) -> Status {
    release_all(world, agent, ReleaseReason::Failed);
    Status::Failure
}

pub(crate) fn set_activity(world: &mut World, agent: Entity, activity: Activity) {
    if let Ok(mut e) = world.get_entity_mut(agent) {
        e.insert(activity);
    }
}

/// Common tail of every stop: back to `Idle`, and drop all claims on abort.
pub(crate) fn end(world: &mut World, agent: Entity, reason: StopReason) {
    set_activity(world, agent, Activity::Idle);
    if reason == StopReason::Aborted {
        release_all(world, agent, ReleaseReason::Aborted);
    }
}

pub(crate) fn set_target(world: &mut World, agent: Entity, target: BbValue, dest: BbValue) {
    if let Some(mut bb) = world.get_mut::<Blackboard>(agent) {
        bb.set(BB_TARGET, target);
        bb.set(BB_DEST, dest);
    }
}

fn bb_target(world: &World, agent: Entity) -> Option<(SimId, Entity)> {
    let BbValue::U32(id) = world.get::<Blackboard>(agent)?.get(BB_TARGET) else {
        return None;
    };
    let id = SimId(id);
    Some((id, world.get_resource::<SimIndex>()?.get(id)?))
}

/// Claims `target` and points the blackboard at it. Releases everything and
/// fails when the claim is refused or the place has no position.
fn claim(world: &mut World, agent: Entity, me: SimId, target: (SimId, Entity), cap: u32) -> Status {
    let (id, entity) = target;
    let Some(dest) = approach_of(world, entity) else {
        return fail(world, agent);
    };
    if !reserve(world, me, id, cap) {
        return fail(world, agent);
    }
    set_target(
        world,
        agent,
        BbValue::U32(id.0),
        BbValue::Pos(dest.x, dest.z),
    );
    Status::Success
}

// ---------------------------------------------------------------------------
// Instant actions
// ---------------------------------------------------------------------------

/// Stop for instant actions (never Running, so normally never called).
pub fn instant_stop(world: &mut World, agent: Entity, reason: StopReason) {
    if reason == StopReason::Aborted {
        release_all(world, agent, ReleaseReason::Aborted);
    }
}

/// `ReserveFood`: claim one food unit in the lowest-SimId stockpile with food
/// available; `BB_TARGET` = stockpile, `BB_DEST` = its approach.
pub fn reserve_food_start(world: &mut World, agent: Entity, _param: &ParamValue) -> Status {
    let Some(me) = sim_id(world, agent) else {
        return Status::Failure;
    };
    release_all(world, agent, ReleaseReason::Reclaim);
    let Some(target) = find_food(world, me) else {
        return Status::Failure;
    };
    let food = world.get::<Stockpile>(target.1).map_or(0, |p| p.food);
    claim(world, agent, me, target, food)
}

/// `ReserveBed`: claim the lowest-SimId free bed of the villager's home
/// (see [`find_bed`]).
pub fn reserve_bed_start(world: &mut World, agent: Entity, _param: &ParamValue) -> Status {
    let (Some(me), Some(villager)) = (sim_id(world, agent), world.get::<Villager>(agent).copied())
    else {
        return Status::Failure;
    };
    release_all(world, agent, ReleaseReason::Reclaim);
    match find_bed(world, me, villager.home) {
        Some(bed) => claim(world, agent, me, bed, 1),
        None => Status::Failure,
    }
}

/// `ReserveWork`: claim the lowest-SimId free work site (one whose stockpile
/// has room is preferred).
pub fn reserve_work_start(world: &mut World, agent: Entity, _param: &ParamValue) -> Status {
    let Some(me) = sim_id(world, agent) else {
        return Status::Failure;
    };
    release_all(world, agent, ReleaseReason::Reclaim);
    match find_work_site(world, me, false) {
        Some(site) => claim(world, agent, me, site, 1),
        None => Status::Failure,
    }
}

/// `ReserveWell`: claim a slot at the nearest well with one free (see
/// [`find_well_for`]); `BB_TARGET` = well, `BB_DEST` = its approach.
pub fn reserve_well_start(world: &mut World, agent: Entity, _param: &ParamValue) -> Status {
    let Some(me) = sim_id(world, agent) else {
        return Status::Failure;
    };
    release_all(world, agent, ReleaseReason::Reclaim);
    let Some(target) = find_well_for(world, agent) else {
        return Status::Failure;
    };
    let slots = world.get::<Well>(target.1).map_or(0, |w| w.slots);
    claim(world, agent, me, target, slots)
}

/// `PickWanderPoint`: `BB_DEST` = home approach + a counter-RNG offset of up
/// to ±`WANDER_RADIUS` per axis, clamped to ±`WANDER_LIMIT` when the home
/// approach lies within ±`WANDER_LIMIT`.
pub fn pick_wander_point_start(world: &mut World, agent: Entity, _param: &ParamValue) -> Status {
    let (Some(me), Some(villager)) = (sim_id(world, agent), world.get::<Villager>(agent).copied())
    else {
        return fail(world, agent);
    };
    let home = world
        .resource::<SimIndex>()
        .get(villager.home)
        .and_then(|e| approach_of(world, e))
        .or_else(|| world.get::<SimPos>(agent).copied());
    let Some(home) = home else {
        return fail(world, agent);
    };
    let seed = world.resource::<WorldSeed>().0;
    let tick = world.resource::<SimClock>().tick;
    let r = WANDER_RADIUS;
    let dx = sim_range(seed, me, tick, RNG_WANDER_X, -r, r + 1);
    let dz = sim_range(seed, me, tick, RNG_WANDER_Z, -r, r + 1);
    let (mut x, mut z) = (home.x.saturating_add(dx), home.z.saturating_add(dz));
    if home.x.abs() <= WANDER_LIMIT && home.z.abs() <= WANDER_LIMIT {
        x = x.clamp(-WANDER_LIMIT, WANDER_LIMIT);
        z = z.clamp(-WANDER_LIMIT, WANDER_LIMIT);
    }
    set_target(world, agent, BbValue::None, BbValue::Pos(x, z));
    Status::Success
}

// ---------------------------------------------------------------------------
// MoveToDest
// ---------------------------------------------------------------------------

/// A walk fails after the mover reported `Blocked` for this many ticks in a
/// row (only possible with blocking cells in the grid, i.e. never in the
/// plain village).
pub const MOVE_GIVE_UP_TICKS: u32 = 200;

/// Starts a walk to `goal`: `MoveTo { goal }` (the hashed record of the
/// walk) plus the locomotion seam's `MoveIntent` at the villager's speed.
pub(crate) fn start_walk(world: &mut World, agent: Entity, goal: SimPos) {
    let speed = world.get::<Villager>(agent).map_or(0, |v| v.speed);
    world.entity_mut(agent).insert(MoveTo { goal });
    set_move_intent(world, agent, MoveIntent::to(goal, speed));
}

/// Ends a walk: removes `MoveTo` and the intent. Returns the walk's goal.
pub(crate) fn end_walk(world: &mut World, agent: Entity) -> Option<SimPos> {
    let goal = world
        .get_entity_mut(agent)
        .ok()
        .and_then(|mut e| e.take::<MoveTo>())
        .map(|m| m.goal);
    clear_move_intent(world, agent);
    goal
}

/// `MoveToDest`: walk to `BB_DEST` (Success at once when already there).
/// The walk is a `MoveIntent`: the mover (`StraightLineMover`, or
/// `GridPathMover` for `NavAgent`s) moves the villager in
/// `NpcSet::Locomotion` and [`move_arrival_system`] ends the leaf.
pub fn move_to_dest_start(world: &mut World, agent: Entity, _param: &ParamValue) -> Status {
    let dest = world.get::<Blackboard>(agent).map(|bb| bb.get(BB_DEST));
    let (Some(BbValue::Pos(x, z)), Some(&pos)) = (dest, world.get::<SimPos>(agent)) else {
        return fail(world, agent);
    };
    let goal = SimPos::new(x, z);
    if pos == goal {
        return Status::Success;
    }
    start_walk(world, agent, goal);
    set_activity(world, agent, Activity::Walk);
    Status::Running
}

pub fn stop_move(world: &mut World, agent: Entity, reason: StopReason) {
    end_walk(world, agent);
    end(world, agent, reason);
}

/// Agents walking a `MoveTo` through the locomotion seam.
type Walking = (With<MoveTo>, With<MoveIntent>);

/// `NpcSet::Actions`: a walk whose leaf is no longer running (an order walk
/// failed by `order_system`) stops moving at once: its intent is removed
/// before the movers run (the leaf's stop removes the rest).
pub fn move_to_system(walkers: Query<(Entity, &ActiveLeaf), Walking>, mut commands: Commands) {
    for (agent, leaf) in &walkers {
        if leaf.status != Status::Running {
            commands.entity(agent).remove::<(MoveIntent, MoveStatus)>();
        }
    }
}

/// `NpcSet::Locomotion`, after the movers: Success in the tick the mover
/// reports `Arrived`; Failure after `MOVE_GIVE_UP_TICKS` blocked ticks.
pub fn move_arrival_system(mut walkers: Query<(&MoveStatus, &mut ActiveLeaf), Walking>) {
    for (status, mut leaf) in &mut walkers {
        if leaf.status != Status::Running {
            continue;
        }
        match *status {
            MoveStatus::Arrived => leaf.status = Status::Success,
            MoveStatus::Blocked { ticks } if ticks >= MOVE_GIVE_UP_TICKS => {
                leaf.status = Status::Failure;
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// EatFood
// ---------------------------------------------------------------------------

/// `EatFood`: eat at the `BB_TARGET` stockpile for `EAT_TICKS` ticks.
pub fn eat_food_start(world: &mut World, agent: Entity, _param: &ParamValue) -> Status {
    let Some((stockpile, entity)) = bb_target(world, agent) else {
        return fail(world, agent);
    };
    if world.get::<Stockpile>(entity).is_none() {
        return fail(world, agent);
    }
    world.entity_mut(agent).insert(Eat {
        stockpile,
        remaining: EAT_TICKS,
    });
    set_activity(world, agent, Activity::Eat);
    Status::Running
}

pub fn stop_eat(world: &mut World, agent: Entity, reason: StopReason) {
    let eat = world
        .get_entity_mut(agent)
        .ok()
        .and_then(|mut e| e.take::<Eat>());
    // `eat_system` already released the claim when the meal completed; this
    // only matters if something else ends the leaf with `Finished`.
    if reason == StopReason::Finished
        && let Some(eat) = eat
    {
        release(world, agent, eat.stockpile);
    }
    end(world, agent, reason);
}

/// Counts the meal down. Fails when the stockpile is empty or gone; after
/// `EAT_TICKS` adds the stockpile's `Provides` × `EAT_TICKS` to the matching
/// need (clamped), takes `MEAL_FOOD` food (via `SimEffect`), releases the
/// meal's stockpile claim (journaled `Release { Finished }`, in `SimId`
/// order) and succeeds. The claim goes in the same tick as the food it
/// stood for, so it never outlives it (`stop_eat` then finds nothing to
/// release). An interrupted meal restores nothing and takes no food.
pub fn eat_system(
    mut eaters: Query<(&SimId, &mut Needs, &mut Eat, &mut ActiveLeaf)>,
    stockpiles: Query<(&Stockpile, Option<&Provides>)>,
    index: Res<SimIndex>,
    mut effects: MessageWriter<SimEffect>,
    mut reservations: ResMut<Reservations>,
    journal: Option<ResMut<Journal>>,
    clock: Res<SimClock>,
) {
    let mut finished = Vec::new();
    for (&me, mut needs, mut eat, mut leaf) in &mut eaters {
        if leaf.status != Status::Running {
            continue;
        }
        let Some((pile, provides)) = index
            .get(eat.stockpile)
            .and_then(|e| stockpiles.get(e).ok())
        else {
            leaf.status = Status::Failure;
            continue;
        };
        if pile.food == 0 {
            leaf.status = Status::Failure;
            continue;
        }
        eat.remaining = eat.remaining.saturating_sub(1);
        if eat.remaining == 0 {
            if let Some(p) = provides {
                let need = needs.get_mut(p.need);
                let meal = p.amount_per_tick.saturating_mul(EAT_TICKS as i32);
                need.value = need.value.saturating_add(meal).clamp(0, NEED_MAX);
            }
            effects.write(SimEffect {
                source: me,
                kind: EffectKind::TakeFood {
                    stockpile: eat.stockpile,
                    amount: MEAL_FOOD,
                },
            });
            leaf.status = Status::Success;
            finished.push((me, eat.stockpile));
        }
    }
    // Query order is not SimId order: release (and journal) sorted.
    finished.sort_unstable();
    let mut journal = journal.filter(|j| j.enabled);
    for (me, stockpile) in finished {
        if !reservations.holds(stockpile, me) {
            continue;
        }
        reservations.release(stockpile, me);
        if let Some(journal) = &mut journal {
            journal.push(
                clock.tick,
                Some(me),
                JournalKind::Release {
                    target: stockpile,
                    reason: ReleaseReason::Finished,
                },
            );
        }
    }
}

// ---------------------------------------------------------------------------
// DrinkWater
// ---------------------------------------------------------------------------

/// `DrinkWater`: drink at the `BB_TARGET` well for `DRINK_TICKS` ticks.
pub fn drink_water_start(world: &mut World, agent: Entity, _param: &ParamValue) -> Status {
    let Some((well, entity)) = bb_target(world, agent) else {
        return fail(world, agent);
    };
    if world.get::<Well>(entity).is_none() {
        return fail(world, agent);
    }
    world.entity_mut(agent).insert(Drink {
        well,
        remaining: DRINK_TICKS,
    });
    set_activity(world, agent, Activity::Drink);
    Status::Running
}

pub fn stop_drink(world: &mut World, agent: Entity, reason: StopReason) {
    let drink = world
        .get_entity_mut(agent)
        .ok()
        .and_then(|mut e| e.take::<Drink>());
    // `drink_system` already released the slot when the drink completed.
    if reason == StopReason::Finished
        && let Some(drink) = drink
    {
        release(world, agent, drink.well);
    }
    end(world, agent, reason);
}

/// Counts the drink down. Fails when the well is gone or provides no thirst;
/// after `DRINK_TICKS` adds the well's `Provides` × `DRINK_TICKS` to thirst
/// (clamped), releases the drinker's well slot (journaled `Release {
/// Finished }`, in `SimId` order, like a meal) and succeeds. The water never
/// runs out, so there is no cross-entity effect. An interrupted drink
/// restores nothing.
pub fn drink_system(
    mut drinkers: Query<(&SimId, &mut Needs, &mut Drink, &mut ActiveLeaf)>,
    wells: Query<&Provides, With<Well>>,
    index: Res<SimIndex>,
    needs: Res<NeedRegistry>,
    mut reservations: ResMut<Reservations>,
    journal: Option<ResMut<Journal>>,
    clock: Res<SimClock>,
) {
    let Some(thirst) = needs.id(THIRST) else {
        return;
    };
    let mut finished = Vec::new();
    for (&me, mut needs, mut drink, mut leaf) in &mut drinkers {
        if leaf.status != Status::Running {
            continue;
        }
        let Some(p) = index
            .get(drink.well)
            .and_then(|e| wells.get(e).ok())
            .filter(|p| p.need == thirst)
        else {
            leaf.status = Status::Failure;
            continue;
        };
        drink.remaining = drink.remaining.saturating_sub(1);
        if drink.remaining == 0 {
            let need = needs.get_mut(thirst);
            let water = p.amount_per_tick.saturating_mul(DRINK_TICKS as i32);
            need.value = need.value.saturating_add(water).clamp(0, NEED_MAX);
            leaf.status = Status::Success;
            finished.push((me, drink.well));
        }
    }
    // Query order is not SimId order: release (and journal) sorted.
    finished.sort_unstable();
    let mut journal = journal.filter(|j| j.enabled);
    for (me, well) in finished {
        if !reservations.holds(well, me) {
            continue;
        }
        reservations.release(well, me);
        if let Some(journal) = &mut journal {
            journal.push(
                clock.tick,
                Some(me),
                JournalKind::Release {
                    target: well,
                    reason: ReleaseReason::Finished,
                },
            );
        }
    }
}

// ---------------------------------------------------------------------------
// DoWork
// ---------------------------------------------------------------------------

/// `DoWork(param)`: work the `BB_TARGET` site for its `work_ticks`. With
/// `WORK_DAY_ONLY` the start fails at night (a shift already running at
/// nightfall still completes); with `WORK_ANY_TIME` it always starts.
pub fn do_work_start(world: &mut World, agent: Entity, param: &ParamValue) -> Status {
    if param.as_name() == Some(WORK_DAY_ONLY) && is_night(world) {
        return fail(world, agent);
    }
    let Some((site, entity)) = bb_target(world, agent) else {
        return fail(world, agent);
    };
    let Some(work_site) = world.get::<WorkSite>(entity).copied() else {
        return fail(world, agent);
    };
    world.entity_mut(agent).insert(Work {
        site,
        remaining: work_site.work_ticks,
    });
    set_activity(world, agent, Activity::Work);
    Status::Running
}

pub fn stop_work(world: &mut World, agent: Entity, reason: StopReason) {
    let work = world
        .get_entity_mut(agent)
        .ok()
        .and_then(|mut e| e.take::<Work>());
    if reason == StopReason::Finished
        && let Some(work) = work
    {
        release(world, agent, work.site);
    }
    end(world, agent, reason);
}

/// Counts the shift down; at 0 adds `yield_food` to the site's stockpile (via
/// `SimEffect`) and succeeds. Interrupted shifts yield nothing.
pub fn work_system(
    mut workers: Query<(&SimId, &mut Work, &mut ActiveLeaf)>,
    sites: Query<&WorkSite>,
    index: Res<SimIndex>,
    mut effects: MessageWriter<SimEffect>,
) {
    for (&me, mut work, mut leaf) in &mut workers {
        if leaf.status != Status::Running {
            continue;
        }
        let Some(site) = index.get(work.site).and_then(|e| sites.get(e).ok()) else {
            leaf.status = Status::Failure;
            continue;
        };
        work.remaining = work.remaining.saturating_sub(1);
        if work.remaining == 0 {
            effects.write(SimEffect {
                source: me,
                kind: EffectKind::AddFood {
                    stockpile: site.stockpile,
                    amount: site.yield_food,
                },
            });
            leaf.status = Status::Success;
        }
    }
}

// ---------------------------------------------------------------------------
// SleepInBed
// ---------------------------------------------------------------------------

/// `SleepInBed(param)`: `SLEEP_FULL` sleeps until energy is full,
/// `SLEEP_MORNING` until it is day and energy > the energy `low_exit`.
/// Energy and thirst do not decay while asleep (`PauseDecay`).
pub fn sleep_in_bed_start(world: &mut World, agent: Entity, param: &ParamValue) -> Status {
    let Some((bed, entity)) = bb_target(world, agent) else {
        return fail(world, agent);
    };
    if world.get::<Bed>(entity).is_none() {
        return fail(world, agent);
    }
    let until = if param.as_name() == Some(SLEEP_MORNING) {
        SleepUntil::Morning
    } else {
        SleepUntil::Full
    };
    let (energy, thirst) = (need_id(world, ENERGY), need_id(world, THIRST));
    world
        .entity_mut(agent)
        .insert((Sleep { bed, until }, PauseDecay::of(&[energy, thirst])));
    set_activity(world, agent, Activity::Sleep);
    Status::Running
}

pub fn stop_sleep(world: &mut World, agent: Entity, reason: StopReason) {
    let sleep = world.get_entity_mut(agent).ok().and_then(|mut e| {
        e.remove::<PauseDecay>();
        e.take::<Sleep>()
    });
    if reason == StopReason::Finished
        && let Some(sleep) = sleep
    {
        release(world, agent, sleep.bed);
    }
    end(world, agent, reason);
}

/// Each tick adds the bed's energy `Provides` to energy and checks the
/// wake-up condition. Fails when the bed is gone or provides no energy.
pub fn sleep_system(
    mut sleepers: Query<(&mut Needs, &Sleep, &mut ActiveLeaf)>,
    beds: Query<&Provides, With<Bed>>,
    index: Res<SimIndex>,
    clock: Res<SimClock>,
    config: Res<SimConfig>,
    needs: Res<NeedRegistry>,
) {
    let night = config.is_night(clock.tick);
    let Some(energy) = needs.id(ENERGY) else {
        return;
    };
    let low_exit = needs.def(energy).thresholds.low_exit;
    for (mut needs, sleep, mut leaf) in &mut sleepers {
        if leaf.status != Status::Running {
            continue;
        }
        let Some(p) = index
            .get(sleep.bed)
            .and_then(|e| beds.get(e).ok())
            .filter(|p| p.need == energy)
        else {
            leaf.status = Status::Failure;
            continue;
        };
        let need = needs.get_mut(energy);
        need.value = need
            .value
            .saturating_add(p.amount_per_tick)
            .clamp(0, NEED_MAX);
        let value = need.value;
        let done = match sleep.until {
            SleepUntil::Full => value >= NEED_MAX,
            SleepUntil::Morning => !night && value > low_exit,
        };
        if done {
            leaf.status = Status::Success;
        }
    }
}

// ---------------------------------------------------------------------------
// IdleFor
// ---------------------------------------------------------------------------

/// `IdleFor(param)`: stand still for a counter-RNG number of ticks in
/// `60..param` (`ParamValue::Int`).
pub fn idle_for_start(world: &mut World, agent: Entity, param: &ParamValue) -> Status {
    let Some(me) = sim_id(world, agent) else {
        return fail(world, agent);
    };
    let seed = world.resource::<WorldSeed>().0;
    let tick = world.resource::<SimClock>().tick;
    let max = param.as_int().unwrap_or(0);
    let ticks = sim_range(seed, me, tick, RNG_IDLE, 60, max.max(61));
    world.entity_mut(agent).insert(Idle {
        remaining: ticks as u32,
    });
    set_activity(world, agent, Activity::Idle);
    Status::Running
}

pub fn stop_idle(world: &mut World, agent: Entity, reason: StopReason) {
    if let Ok(mut e) = world.get_entity_mut(agent) {
        e.remove::<Idle>();
    }
    end(world, agent, reason);
}

pub fn idle_system(mut idlers: Query<(&mut Idle, &mut ActiveLeaf)>) {
    for (mut idle, mut leaf) in &mut idlers {
        if leaf.status != Status::Running {
            continue;
        }
        idle.remaining = idle.remaining.saturating_sub(1);
        if idle.remaining == 0 {
            leaf.status = Status::Success;
        }
    }
}
