//! The resident's real, reactive behavior tree. Leaf starts stage intent;
//! sorted island Effects apply the same actions available to player clients.
//!
//! The regional profile (`living_world`) does not use the tree built here: its
//! tree is data (`super::resident`) made of granular leaves, each of which
//! takes its step when it starts. Both share [`LifeDecision`], the guards'
//! predicates, [`stop`] and [`advance`].
use super::{VillageAct, Villager, ai, economy as econ, living};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::crafting::Crafting;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

pub const TREE: &str = "island_life";
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
    true.then(|| super::resident::label(world, actor))
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
        || (world.get::<super::hunting::Hunting>(actor).is_some()
            || world.get::<super::homestead::CottageBuild>(actor).is_some()
            || world
                .get::<npc_framework::interaction::Interacting>(actor)
                .is_some())
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
    supplier(world, actor, role, good).is_some() || (super::ai::can_forage(world, actor, good))
}

/// A real purchasable unit of supply, excluding only distance because the leaf
/// approaches it. Capacity and both wallets must permit the eventual trade.
pub(super) fn supplier(
    world: &World,
    actor: Entity,
    _role: super::Role,
    good: &str,
) -> Option<Entity> {
    super::ai::market_supplier(world, actor, good)
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
    living(world, actor) && econ::need(world, actor, "Hunger") < threshold && {
        super::production::food_choice(world, actor).is_some()
            || super::ai::can_forage(world, actor, super::roles::roles(world).foraged)
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

/// Regional residents in deep cold shelter at home even without hearth fuel:
/// an unheated cottage still warms them, a walk for fuel does not.
pub(super) fn freezing(world: &World, actor: Entity) -> bool {
    !super::living::has_coat(world, actor)
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
    if super::living::has_coat(world, actor) {
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
    // Data-authored tree, granular leaves and named conditions.
    app.register_sim_component::<LifeDecision>();
    super::resident::register(app);
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
    let mut actors: Vec<(SimId, Entity)> = world
        .try_query::<(Entity, &SimId, &LifeDecision)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(entity, id, _)| (*id, entity))
                .collect()
        })
        .unwrap_or_default();
    actors.sort_unstable_by_key(|(id, _)| *id);
    let actors: Vec<Entity> = actors.into_iter().map(|(_, entity)| entity).collect();
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
                    let threshold = super::resident::need_threshold(
                        world,
                        actor,
                        super::policy::active(world).map_or(55_000, |p| p.thirst_threshold),
                    );
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
                    let threshold = super::resident::need_threshold(
                        world,
                        actor,
                        super::policy::active(world).map_or(50_000, |p| p.hunger_threshold),
                    );
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
            LifeGoal::Thirst | LifeGoal::Hunger => world.get::<super::Gathering>(actor).is_some(),
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
