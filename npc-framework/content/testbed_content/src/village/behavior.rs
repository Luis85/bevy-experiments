//! The resident's real, reactive behavior tree. Leaf starts stage intent;
//! sorted island Effects apply the same actions available to player clients.
use super::{VillageAct, Villager, ai, catalog, economy as econ, living};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::crafting::{Crafting, cancel_craft};
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
    living(world, actor)
        .then(|| world.get::<LifeDecision>(actor).map(|d| d.goal.name()))
        .flatten()
}

fn busy(world: &World, actor: Entity) -> bool {
    world.get::<super::Gathering>(actor).is_some()
        || world.get::<Crafting>(actor).is_some()
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

fn can_interrupt(world: &World, actor: Entity) -> bool {
    let resting = world
        .get::<Villager>(actor)
        .is_some_and(|v| v.resting_until > world.resource::<SimClock>().tick);
    !resting
        || world
            .get::<LifeDecision>(actor)
            .is_some_and(|d| d.goal == LifeGoal::Recovery)
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
    let price = catalog::price(good)?;
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

fn thirsty(world: &World, actor: Entity, threshold: i32) -> bool {
    living(world, actor)
        && econ::need(world, actor, "Thirst") < threshold
        && can_satisfy(world, actor, catalog::WATER, super::Role::WaterKeeper)
}
fn hungry(world: &World, actor: Entity, threshold: i32) -> bool {
    living(world, actor)
        && econ::need(world, actor, "Hunger") < threshold
        && if super::needs_driven(world) {
            super::production::food_choice(world, actor).is_some()
                || super::ai::can_forage(world, actor, catalog::BERRIES)
        } else {
            can_satisfy(world, actor, "bread", super::Role::Baker)
        }
}

pub fn register(app: &mut App) {
    app.register_sim_component::<LifeDecision>()
        .register_action(ACTION, start, stop)
        .register_cond("island_critical_thirst", |w, e| {
            can_interrupt(w, e) && thirsty(w, e, CRITICAL_NEED)
        })
        .register_cond("island_critical_hunger", |w, e| {
            can_interrupt(w, e) && hungry(w, e, CRITICAL_NEED)
        })
        .register_cond("island_busy", busy)
        .register_cond("island_thirsty", |w, e| {
            !busy(w, e) && thirsty(w, e, 55_000)
        })
        .register_cond("island_hungry", |w, e| !busy(w, e) && hungry(w, e, 50_000))
        .register_cond("island_needs_recovery", |w, e| {
            !busy(w, e)
                && (econ::need(w, e, "Energy") < 30_000
                    || (super::calendar(w).2 && econ::need(w, e, "Energy") < 85_000))
        })
        .register_cond("island_night", |w, e| !busy(w, e) && super::calendar(w).2)
        .register_tree(TREE, |b| {
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

fn start(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    let Some(goal) = param.as_name().and_then(LifeGoal::parse) else {
        return Status::Failure;
    };
    if !living(world, actor)
        || world
            .get::<Villager>(actor)
            .is_none_or(|v| v.role == super::Role::Visitor)
    {
        return Status::Failure;
    }
    let reconsider_at = world.resource::<SimClock>().tick + DECISION_TICKS;
    world.entity_mut(actor).insert(LifeDecision {
        goal,
        reconsider_at,
        pending: true,
    });
    Status::Running
}

fn stop(world: &mut World, actor: Entity, reason: StopReason) {
    let Some(decision) = world.entity_mut(actor).take::<LifeDecision>() else {
        return;
    };
    if reason == StopReason::Aborted {
        world.entity_mut(actor).remove::<MoveIntent>();
        // Never cancel external queues here: takeover already owns their lifecycle.
        if decision.goal == LifeGoal::Work && !decision.pending {
            world.entity_mut(actor).remove::<super::Gathering>();
            cancel_craft(world, actor, "daily life interrupted");
            super::hunting::cancel(world, actor, "daily life interrupted");
            if super::needs_driven(world) {
                npc_framework::interaction::cancel(world, actor, "daily life interrupted");
                super::production::clear_intent(world, actor);
            }
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
                    ai::satisfy(
                        world,
                        actor,
                        "Thirst",
                        catalog::WATER,
                        super::Role::WaterKeeper,
                        55_000,
                        90_000,
                    );
                }
                LifeGoal::Hunger => {
                    ai::satisfy(
                        world,
                        actor,
                        "Hunger",
                        "bread",
                        super::Role::Baker,
                        50_000,
                        80_000,
                    );
                }
                LifeGoal::Recovery => {
                    let home = world.get::<Villager>(actor).expect("villager").home;
                    if ai::travel(world, actor, home) {
                        let id = *world.get::<SimId>(actor).expect("sim actor");
                        let _ = VillageAct::new(id, "rest_home").perform(world);
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
                LifeGoal::Busy => {}
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
