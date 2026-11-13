//! The resident's real, reactive behavior tree. Leaf starts stage intent;
//! sorted island Effects apply the same actions available to player clients.
//!
//! The regional profile (`living_world`) does not use the tree built here: its
//! tree is data (`super::resident`) made of granular leaves, each of which
//! takes its step when it starts. Both share [`LifeDecision`], the guards'
//! predicates, [`stop`] and [`advance`].
use super::{VillageAct, Villager, ai, catalog, economy as econ, living};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::crafting::Crafting;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

pub const TREE: &str = "island_life";
const ACTION: &str = "IslandLifeDecision";
const DECISION_TICKS: u64 = 20;
const CRITICAL_NEED: i32 = 15_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(rename_all = "snake_case")]
pub enum LifeGoal {
    Thirst,
    Hunger,
    Busy,
    Recovery,
    Home,
    Work,
    /// Regional profile only: free time (a walk, a meeting, standing idle).
    Leisure,
}

impl LifeGoal {
    pub fn name(self) -> &'static str {
        match self {
            Self::Thirst => "find water",
            Self::Hunger => "find food",
            Self::Busy => "finish current activity",
            Self::Recovery => "recover at home",
            Self::Home => "return home for the night",
            Self::Work => "supply the village",
            Self::Leisure => "enjoy free time",
        }
    }
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "thirst" => Self::Thirst,
            "hunger" => Self::Hunger,
            "busy" => Self::Busy,
            "recovery" => Self::Recovery,
            "home" => Self::Home,
            "work" => Self::Work,
            _ => return None,
        })
    }
}

/// The current running leaf, including its deterministic execution/cooldown state.
/// Saved and hashed so a resume neither repeats nor skips a staged action.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct LifeDecision {
    pub goal: LifeGoal,
    pub reconsider_at: u64,
    pub pending: bool,
}

impl SimHash for LifeDecision {
    const NAME: &'static str = "island_life_decision";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u8(self.goal as u8);
        h.u64(self.reconsider_at);
        h.u8(u8::from(self.pending));
    }
}

/// Read-only goal from the currently running autonomous leaf, never inferred
/// from wall time or a second copy of the decision rules.
pub fn decision(world: &World, actor: Entity) -> Option<&'static str> {
    if !living(world, actor) {
        return None;
    }
    let decision = world.get::<LifeDecision>(actor)?;
    // Regional residents run granular leaves: name the leaf, not its class.
    super::frontier::enabled(world)
        .then(|| super::resident::label(world, actor))
        .flatten()
        .or(Some(decision.goal.name()))
}

pub(super) fn busy(world: &World, actor: Entity) -> bool {
    world
        .get::<super::frontier::activities::EcologyWork>(actor)
        .is_some()
        || world.get::<super::Gathering>(actor).is_some()
        || world.get::<Crafting>(actor).is_some()
        || world.get::<super::civic::SiteWork>(actor).is_some()
        || world.get::<super::live::UpgradeWork>(actor).is_some()
        || world.get::<super::fields::FieldWork>(actor).is_some()
        || (super::needs_driven(world)
            && (world.get::<super::hunting::Hunting>(actor).is_some()
                || world.get::<super::homestead::CottageBuild>(actor).is_some()
                || world
                    .get::<npc_framework::interaction::Interacting>(actor)
                    .is_some()))
        || world
            .get::<Villager>(actor)
            .is_some_and(|v| v.resting_until > world.resource::<SimClock>().tick)
}

pub(super) fn can_interrupt(world: &World, actor: Entity) -> bool {
    let resting = world
        .get::<Villager>(actor)
        .is_some_and(|v| v.resting_until > world.resource::<SimClock>().tick);
    !resting
        || world
            .get::<LifeDecision>(actor)
            .is_some_and(|d| d.goal == LifeGoal::Recovery)
        // A sleeper wakes for a critical need (regional).
        || super::indoors::asleep(world, actor)
}

fn can_satisfy(world: &World, actor: Entity, good: &str, role: super::Role) -> bool {
    if econ::count(world, actor, good) > 0 {
        return true;
    }
    supplier(world, actor, role, good).is_some()
        || (super::needs_driven(world) && super::ai::can_forage(world, actor, good))
}

/// A real purchasable unit of supply, excluding only distance because the leaf
/// approaches it. Capacity and both wallets must permit the eventual trade.
pub(super) fn supplier(
    world: &World,
    actor: Entity,
    role: super::Role,
    good: &str,
) -> Option<Entity> {
    if super::needs_driven(world) {
        return super::ai::market_supplier(world, actor, good);
    }
    let price = catalog::price_in(world, good)?;
    if world
        .get::<super::Wallet>(actor)
        .is_none_or(|w| w.coins < price)
    {
        return None;
    }
    world
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .find(|seller| {
            *seller != actor
                && living(world, *seller)
                && world
                    .get::<Villager>(*seller)
                    .is_some_and(|v| v.role == role)
                && world
                    .get::<super::Wallet>(*seller)
                    .is_some_and(|w| w.coins.checked_add(price).is_some())
                && econ::selected(world, *seller, good, 1).is_ok_and(|goods| {
                    econ::fit(
                        world,
                        actor,
                        &goods.into_iter().map(|(_, item)| item).collect::<Vec<_>>(),
                    )
                    .is_ok()
                })
        })
}

pub(super) fn thirsty(world: &World, actor: Entity, threshold: i32) -> bool {
    living(world, actor)
        && econ::need(world, actor, "Thirst") < threshold
        && can_satisfy(
            world,
            actor,
            super::roles::roles(world).drink,
            super::Role::WaterKeeper,
        )
}
pub(super) fn hungry(world: &World, actor: Entity, threshold: i32) -> bool {
    living(world, actor)
        && econ::need(world, actor, "Hunger") < threshold
        && if super::needs_driven(world) {
            super::production::food_choice(world, actor).is_some()
                || super::ai::can_forage(world, actor, super::roles::roles(world).foraged)
        } else {
            can_satisfy(
                world,
                actor,
                super::roles::roles(world).baked,
                super::Role::Baker,
            )
        }
}

/// One colony warmth decision shared by the tree and its physical executor.
/// A usable worn coat already restores warmth during ordinary activity.
pub(super) fn warming_need(world: &World, actor: Entity) -> Option<super::colony::HomeHeating> {
    let home = super::colony::home_heating(world, actor)?;
    let threshold = super::policy::active(world).map_or_else(
        || super::policy::IslandAiPolicy::default().recovery_threshold,
        |policy| policy.recovery_threshold,
    );
    (home.cold
        && !super::living::has_coat(world, actor)
        && econ::need(world, actor, "Warmth") < threshold)
        .then_some(home)
}

pub(super) fn needs_hearth_fuel(world: &World, actor: Entity) -> bool {
    warming_need(world, actor)
        .is_some_and(|home| !home.heated && home.carried_fuel < home.fuel_required)
}

fn needs_recovery(world: &World, actor: Entity) -> bool {
    let policy = super::policy::active(world);
    econ::need(world, actor, "Energy") < policy.map_or(30_000, |policy| policy.recovery_threshold)
        || (super::calendar(world).2
            && econ::need(world, actor, "Energy")
                < policy.map_or(85_000, |policy| policy.night_recovery_threshold))
        || warming_need(world, actor)
            .is_some_and(|home| home.heated || home.carried_fuel >= home.fuel_required)
        || freezing(world, actor)
}

/// Regional residents in deep cold shelter at home even without hearth fuel:
/// an unheated cottage still warms them, a walk for fuel does not.
pub(super) fn freezing(world: &World, actor: Entity) -> bool {
    super::frontier::enabled(world)
        && !super::living::has_coat(world, actor)
        && econ::need(world, actor, "Warmth")
            < super::policy::active(world).map_or(CRITICAL_NEED, |policy| policy.critical_need)
        && super::colony::home_heating(world, actor).is_some_and(|home| home.cold)
}

/// Warmth a cold regional resident rests up to before leaving the hearth.
const WARM_REST_TARGET: i32 = 80_000;
/// Longest single warming rest, so hunger and thirst are reconsidered.
const WARM_REST_MAX_TICKS: u64 = 1_500;

/// A cold resident without a coat stays by the hearth until warm, not only
/// until rested. The ordinary home rest stays the minimum.
pub(super) fn warming_rest_ticks(world: &World, actor: Entity) -> Option<u64> {
    if !super::frontier::enabled(world) || super::living::has_coat(world, actor) {
        return None;
    }
    let home = super::colony::home_heating(world, actor).filter(|home| home.cold)?;
    let missing = WARM_REST_TARGET - econ::need(world, actor, "Warmth");
    if missing <= 0 {
        return None;
    }
    // Climate restores 100 Warmth per tick in a heated home and 20 in a cold one.
    let rate = if home.heated || home.carried_fuel >= home.fuel_required {
        100
    } else {
        20
    };
    Some((missing as u64 / rate).clamp(super::commands::HOME_REST_TICKS, WARM_REST_MAX_TICKS))
}

pub fn register(app: &mut App) {
    super::policy::register(app);
    if super::frontier::enabled(app.world()) {
        // Data-authored tree, granular leaves and named conditions.
        app.register_sim_component::<LifeDecision>();
        super::resident::register(app);
        return;
    }
    app.register_sim_component::<LifeDecision>()
        .register_action(ACTION, start, stop)
        .register_cond("island_critical_thirst", |w, e| {
            can_interrupt(w, e)
                && thirsty(
                    w,
                    e,
                    super::policy::active(w).map_or(CRITICAL_NEED, |p| p.critical_need),
                )
        })
        .register_cond("island_critical_hunger", |w, e| {
            can_interrupt(w, e)
                && hungry(
                    w,
                    e,
                    super::policy::active(w).map_or(CRITICAL_NEED, |p| p.critical_need),
                )
        })
        .register_cond("island_busy", busy)
        .register_cond("island_thirsty", |w, e| {
            !busy(w, e)
                && thirsty(
                    w,
                    e,
                    super::policy::active(w).map_or(55_000, |p| p.thirst_threshold),
                )
        })
        .register_cond("island_hungry", |w, e| {
            !busy(w, e)
                && hungry(
                    w,
                    e,
                    super::policy::active(w).map_or(50_000, |p| p.hunger_threshold),
                )
        })
        .register_cond("island_needs_recovery", |w, e| {
            !busy(w, e) && needs_recovery(w, e)
        })
        .register_cond("island_night", |w, e| {
            !busy(w, e) && super::calendar(w).2 && !needs_hearth_fuel(w, e)
        });
    if super::policy::active(app.world()).is_some() {
        app.register_cond("island_routine_0", |w, e| routine_passes(w, e, 0))
            .register_cond("island_routine_1", |w, e| routine_passes(w, e, 1))
            .register_cond("island_routine_2", |w, e| routine_passes(w, e, 2))
            .register_cond("island_routine_3", |w, e| routine_passes(w, e, 3))
            .register_tree(TREE, configured_tree);
    } else {
        app.register_tree(TREE, |b| {
            b.selector("Daily life", |b| {
                // Each guard implies the leaf can start, even while working:
                // a critical need cancels only work owned by an aborted life leaf.
                for (name, condition, goal) in [
                    ("Critical thirst", "island_critical_thirst", "thirst"),
                    ("Critical hunger", "island_critical_hunger", "hunger"),
                ] {
                    b.guard(name, condition, Abort::LowerPriority, |b| {
                        b.leaf(name, ACTION, goal);
                    });
                }
                b.guard("Player orders", HAS_UNIT_ORDER, Abort::LowerPriority, |b| {
                    b.leaf("Execute orders", EXECUTE_ORDER_ACTION, ());
                });
                for (name, condition, goal) in [
                    ("Current activity", "island_busy", "busy"),
                    ("Thirst", "island_thirsty", "thirst"),
                    ("Hunger", "island_hungry", "hunger"),
                    ("Recovery", "island_needs_recovery", "recovery"),
                    ("Night", "island_night", "home"),
                ] {
                    let abort = if goal == "busy" {
                        Abort::None
                    } else {
                        Abort::LowerPriority
                    };
                    b.guard(name, condition, abort, |b| {
                        b.leaf(name, ACTION, goal);
                    });
                }
                b.leaf("Profession", ACTION, "work");
            });
        });
    }
}

fn routine_goal(world: &World, index: usize) -> Option<LifeGoal> {
    use super::policy::RoutineGoal;
    Some(match super::policy::active(world)?.routine.get(index)? {
        RoutineGoal::Thirst => LifeGoal::Thirst,
        RoutineGoal::Hunger => LifeGoal::Hunger,
        RoutineGoal::Recovery => LifeGoal::Recovery,
        RoutineGoal::Home => LifeGoal::Home,
        RoutineGoal::Work => LifeGoal::Work,
    })
}

fn routine_passes(world: &World, actor: Entity, index: usize) -> bool {
    let Some(policy) = super::policy::active(world) else {
        return false;
    };
    if busy(world, actor) || !living(world, actor) {
        return false;
    }
    match routine_goal(world, index) {
        Some(LifeGoal::Thirst) => thirsty(world, actor, policy.thirst_threshold),
        Some(LifeGoal::Hunger) => hungry(world, actor, policy.hunger_threshold),
        Some(LifeGoal::Recovery) => needs_recovery(world, actor),
        Some(LifeGoal::Home) => super::calendar(world).2 && !needs_hearth_fuel(world, actor),
        _ => false,
    }
}

fn configured_tree(b: &mut TreeBuilder) {
    b.selector("Configured daily life", |b| {
        for (name, condition, goal) in [
            ("Critical thirst", "island_critical_thirst", "thirst"),
            ("Critical hunger", "island_critical_hunger", "hunger"),
        ] {
            b.guard(name, condition, Abort::LowerPriority, |b| {
                b.leaf(name, ACTION, goal);
            });
        }
        b.guard("Player orders", HAS_UNIT_ORDER, Abort::LowerPriority, |b| {
            b.leaf("Execute orders", EXECUTE_ORDER_ACTION, ());
        });
        b.guard("Current activity", "island_busy", Abort::None, |b| {
            b.leaf("Current activity", ACTION, "busy");
        });
        for (name, condition, goal) in [
            ("Routine priority 1", "island_routine_0", "routine_0"),
            ("Routine priority 2", "island_routine_1", "routine_1"),
            ("Routine priority 3", "island_routine_2", "routine_2"),
            ("Routine priority 4", "island_routine_3", "routine_3"),
        ] {
            b.guard(name, condition, Abort::LowerPriority, |b| {
                b.leaf(name, ACTION, goal);
            });
        }
        b.leaf("Goal-oriented profession", ACTION, "work");
    });
}

fn start(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    let Some(goal) = param.as_name().and_then(|name| {
        name.strip_prefix("routine_")
            .and_then(|index| index.parse::<usize>().ok())
            .and_then(|index| routine_goal(world, index))
            .or_else(|| LifeGoal::parse(name))
    }) else {
        return Status::Failure;
    };
    if !living(world, actor)
        || world
            .get::<Villager>(actor)
            .is_none_or(|v| v.role == super::Role::Visitor)
    {
        return Status::Failure;
    }
    let reconsider_at = world.resource::<SimClock>().tick
        + super::policy::active(world).map_or(DECISION_TICKS, |policy| policy.decision_ticks);
    world.entity_mut(actor).insert(LifeDecision {
        goal,
        reconsider_at,
        pending: true,
    });
    Status::Running
}

pub(super) fn stop(world: &mut World, actor: Entity, reason: StopReason) {
    let Some(decision) = world.entity_mut(actor).take::<LifeDecision>() else {
        return;
    };
    if reason == StopReason::Aborted {
        world.entity_mut(actor).remove::<MoveIntent>();
        // Never cancel external queues here: takeover already owns their lifecycle.
        if decision.goal == LifeGoal::Work && !decision.pending {
            super::ai::cancel_owned_work(world, actor, "daily life interrupted");
        }
        if decision.goal == LifeGoal::Recovery && !decision.pending {
            let now = world.resource::<SimClock>().tick;
            if let Some(mut v) = world.get_mut::<Villager>(actor) {
                v.resting_until = now;
                v.rest_rate = 0;
            }
        }
    }
}

/// Called by the island's exclusive Effects system after survival, replenishment,
/// production completions and dividends, in the same stable SimId order.
pub(super) fn advance(world: &mut World) {
    super::resident::record::flush(world);
    let now = world.resource::<SimClock>().tick;
    let actors: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|e| world.get::<LifeDecision>(*e).is_some())
        .collect();
    for actor in actors {
        if !living(world, actor) {
            world.entity_mut(actor).remove::<LifeDecision>();
            continue;
        }
        let mut decision = *world.get::<LifeDecision>(actor).expect("decision");
        if decision.pending {
            decision.pending = false;
            world.entity_mut(actor).insert(decision);
            if let Some(mut v) = world.get_mut::<Villager>(actor) {
                v.next_decision = decision.reconsider_at;
            }
            match decision.goal {
                LifeGoal::Thirst => {
                    let threshold =
                        super::policy::active(world).map_or(55_000, |p| p.thirst_threshold);
                    ai::satisfy(
                        world,
                        actor,
                        "Thirst",
                        super::roles::roles(world).drink,
                        super::Role::WaterKeeper,
                        threshold,
                        90_000,
                    );
                }
                LifeGoal::Hunger => {
                    let threshold =
                        super::policy::active(world).map_or(50_000, |p| p.hunger_threshold);
                    ai::satisfy(
                        world,
                        actor,
                        "Hunger",
                        super::roles::roles(world).baked,
                        super::Role::Baker,
                        threshold,
                        80_000,
                    );
                }
                LifeGoal::Recovery => {
                    let home = world.get::<Villager>(actor).expect("villager").home;
                    if ai::travel(world, actor, home) {
                        let id = *world.get::<SimId>(actor).expect("sim actor");
                        let warming = warming_rest_ticks(world, actor);
                        if VillageAct::new(id, "rest_home").perform(world).is_ok()
                            && let Some(ticks) = warming
                        {
                            ai::start_rest(world, actor, ticks, super::commands::HOME_REST_RATE);
                        }
                    }
                }
                LifeGoal::Home => {
                    let home = world.get::<Villager>(actor).expect("villager").home;
                    ai::travel(world, actor, home);
                }
                LifeGoal::Work => {
                    ai::recover_found(world, actor);
                    ai::work(world, actor);
                }
                LifeGoal::Busy | LifeGoal::Leisure => {}
            }
        }
        let held = match decision.goal {
            LifeGoal::Work | LifeGoal::Recovery | LifeGoal::Busy => busy(world, actor),
            LifeGoal::Thirst | LifeGoal::Hunger => {
                super::needs_driven(world) && world.get::<super::Gathering>(actor).is_some()
            }
            _ => false,
        };
        if !held
            && now.saturating_add(1) >= decision.reconsider_at
            && let Some(mut leaf) = world.get_mut::<ActiveLeaf>(actor)
        {
            leaf.status = Status::Success;
        }
    }
}
