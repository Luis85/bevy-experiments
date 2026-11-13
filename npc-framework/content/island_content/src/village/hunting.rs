//! Finite wildlife and one shared, physical hunting action for every controller.
use super::{ResourcePatch, Villager, Vitality, catalog, economy, living};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::crafting::Crafting;
use npc_framework::interaction::items::{self, entity_of};
use npc_framework::interaction::{CarriedBy, Item};
use npc_framework::kernel::journal::ReleaseReason;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const HUNT_TICKS: u32 = 240;
pub const MAX_WILDLIFE: u32 = 8;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Wildlife {
    pub habitat: SimId,
}
impl SimHash for Wildlife {
    const NAME: &'static str = "island_wildlife";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.habitat.0);
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct WildlifeHabitat {
    pub home: SimPos,
    pub capacity: u32,
    pub renew_at: u64,
}
impl SimHash for WildlifeHabitat {
    const NAME: &'static str = "island_wildlife_habitat";
    fn sim_hash(&self, h: &mut Fnv64) {
        self.home.sim_hash(h);
        h.u32(self.capacity);
        h.u64(self.renew_at);
    }
}

/// The exact prey and spent projectile are reserved for this bounded work.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct Hunting {
    pub target: SimId,
    pub tool: SimId,
    pub ends: u64,
}
impl SimHash for Hunting {
    const NAME: &'static str = "island_hunting";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.target.0);
        h.u32(self.tool.0);
        h.u64(self.ends);
    }
}

pub fn register(app: &mut App) {
    app.register_sim_component::<Wildlife>()
        .register_sim_component::<WildlifeHabitat>()
        .register_sim_component::<Hunting>();
}

fn next_day(world: &World) -> u64 {
    let now = world.resource::<SimClock>().tick;
    let day = world.resource::<SimConfig>().day_length.max(1);
    (now / day).saturating_add(1).saturating_mul(day)
}

/// Must run after the original island actors, stock and new professions spawn.
/// Appending here preserves every incumbent SimId.
pub fn spawn(world: &mut World, ids: &mut crate::IslandIds) {
    let renew_at = next_day(world);
    for (kind, capacity, gather_ticks) in [(catalog::FIBER, 80, 120), (catalog::BERRIES, 80, 80)] {
        let pos = super::geography::for_world(world)
            .resource_position(kind)
            .expect("living resource geography");
        let (id, _) = spawn_sim(
            world,
            (
                pos,
                ResourcePatch {
                    kind: kind.into(),
                    remaining: capacity,
                    capacity,
                    replenish_at: renew_at,
                    gather_ticks,
                },
                Perceivable::new(crate::world::kinds::STATION),
            ),
        );
        ids.name(format!("patch_{kind}"), id);
    }
    let home = super::geography::for_world(world).wildlife_home;
    let (habitat, _) = spawn_sim(
        world,
        (
            home,
            WildlifeHabitat {
                home,
                capacity: MAX_WILDLIFE,
                renew_at,
            },
            Perceivable::new(crate::world::kinds::STATION),
        ),
    );
    ids.name("wildlife_habitat", habitat);
    replenish(world, habitat, home, MAX_WILDLIFE);
}

/// An NPC's internal source search; clients discover individual animals through
/// ordinary perception and cannot use this helper as an omniscient RPC.
pub fn available_prey(world: &World) -> Vec<(SimId, Entity, SimPos)> {
    // The wild animals only, ascending by id: the order a pass over the whole
    // index gave, without visiting every entity of the world.
    let Some(mut query) = world.try_query::<(Entity, &SimId, &SimPos, &Wildlife)>() else {
        return Vec::new();
    };
    let mut prey: Vec<(SimId, Entity, SimPos)> = query
        .iter(world)
        .filter(|(entity, id, ..)| {
            alive_prey(world, *entity) && (super::frontier::ecology::harvestable_prey(world, **id))
        })
        .map(|(entity, id, at, _)| (*id, entity, *at))
        .collect();
    prey.sort_unstable_by_key(|(id, ..)| *id);
    prey
}

fn alive_prey(world: &World, entity: Entity) -> bool {
    world.get::<Wildlife>(entity).is_some()
        && world
            .get::<Vitality>(entity)
            .is_some_and(|v| v.health > 0 && v.died_at.is_none())
}

/// What one hunt yields: the hunting trade's yields in the catalog data.
fn yield_items(world: &World) -> [Item; 2] {
    let roles = super::roles::roles(world);
    [
        Item::new(roles.meat).stack(roles.meat_per_hunt),
        Item::new(roles.hide).stack(roles.hide_per_hunt),
    ]
}

fn prey_check(world: &World, actor: Entity, target: SimId) -> Result<Entity, String> {
    let prey = entity_of(world, target).ok_or("a living wild animal is required")?;
    if prey == actor || !alive_prey(world, prey) {
        return Err("a living wild animal is required".into());
    }
    if !super::frontier::ecology::harvestable_prey(world, target) {
        return Err(
            "Leave the breeding refuge undisturbed; this habitat needs its remaining adults".into(),
        );
    }
    if !economy::near(world, actor, prey) {
        return Err("move within 1.5 m of the living animal".into());
    }
    Ok(prey)
}

fn plan(world: &World, actor: Entity, target: SimId) -> Result<SimId, String> {
    if !living(world, actor) || world.get::<Villager>(actor).is_none() {
        return Err("only a living villager can hunt".into());
    }
    let me = *world.get::<SimId>(actor).ok_or("actor is missing")?;
    if world.get::<Hunting>(actor).is_some()
        || world.get::<super::Gathering>(actor).is_some()
        || world.get::<Crafting>(actor).is_some()
        || world
            .get::<Villager>(actor)
            .is_some_and(|v| v.resting_until > world.resource::<SimClock>().tick)
    {
        return Err("already working or resting".into());
    }
    prey_check(world, actor, target)?;
    if world.resource::<Reservations>().count(target) > 0 {
        return Err("this animal is already being hunted".into());
    }
    let tool = economy::free_items(world, actor)
        .into_iter()
        .find(|(_, item)| item.kind == super::roles::roles(world).hunt_tool && item.stack == 1)
        .map(|(id, _)| id)
        .ok_or("carry an unreserved hunting spear first")?;
    let tool_entity = entity_of(world, tool).ok_or("carry an unreserved hunting spear first")?;
    if world
        .get::<CarriedBy>(tool_entity)
        .is_none_or(|held| held.holder != me)
    {
        return Err("carry an unreserved hunting spear first".into());
    }
    economy::fit_without(world, actor, &yield_items(world), &[tool])?;
    Ok(tool)
}

pub fn check(world: &World, actor: Entity, target: SimId) -> Result<(), String> {
    plan(world, actor, target).map(|_| ())
}

pub fn begin(world: &mut World, actor: Entity, target: SimId) -> Result<(), String> {
    let tool = plan(world, actor, target)?;
    let me = *world.get::<SimId>(actor).expect("planned actor");
    let mut claimed = [target, tool];
    claimed.sort_unstable();
    for (index, id) in claimed.into_iter().enumerate() {
        if !npc_framework::jobs::reserve(world, me, id, 1) {
            for previous in claimed.into_iter().take(index) {
                release(world, actor, previous, ReleaseReason::Failed);
            }
            return Err("the hunt's animal or spear is already claimed".into());
        }
    }
    let ends = world
        .resource::<SimClock>()
        .tick
        .saturating_add(u64::from(HUNT_TICKS));
    world
        .entity_mut(actor)
        .remove::<MoveIntent>()
        .insert(Hunting { target, tool, ends });
    record_effect(
        world,
        Some(me),
        "VillageHuntStarted",
        &json!({"target":target,"ends":ends}),
    );
    Ok(())
}

fn release(world: &mut World, actor: Entity, target: SimId, reason: ReleaseReason) {
    let Some(me) = world.get::<SimId>(actor).copied() else {
        return;
    };
    let mut claims = world.resource_mut::<Reservations>();
    let held = claims.holds(target, me);
    claims.release(target, me);
    if held {
        record_effect_release(world, me, target, reason);
    }
}

fn record_effect_release(world: &mut World, me: SimId, target: SimId, reason: ReleaseReason) {
    npc_framework::kernel::journal::record(world, Some(me), || JournalKind::Release {
        target,
        reason,
    });
}

pub fn cancel(world: &mut World, actor: Entity, _reason: &str) {
    let Some(work) = world.entity_mut(actor).take::<Hunting>() else {
        return;
    };
    let mut claims = [work.target, work.tool];
    claims.sort_unstable();
    for target in claims {
        release(world, actor, target, ReleaseReason::Aborted);
    }
    let me = world.get::<SimId>(actor).copied();
    record_effect(
        world,
        me,
        "VillageHuntStopped",
        &json!({"outcome":"cancelled"}),
    );
}

fn finish_check(world: &World, actor: Entity, work: Hunting) -> Result<(), String> {
    if !living(world, actor) {
        return Err("hunter is no longer living".into());
    }
    prey_check(world, actor, work.target)?;
    let me = *world.get::<SimId>(actor).ok_or("actor is missing")?;
    if [work.target, work.tool]
        .into_iter()
        .any(|target| !world.resource::<Reservations>().holds(target, me))
    {
        return Err("hunt reservation was lost".into());
    }
    let spear = entity_of(world, work.tool).ok_or("hunting spear is missing")?;
    if world
        .get::<Item>(spear)
        .is_none_or(|item| item.kind != super::roles::roles(world).hunt_tool || item.stack != 1)
        || world
            .get::<CarriedBy>(spear)
            .is_none_or(|held| held.holder != me)
    {
        return Err("hunting spear is no longer carried".into());
    }
    economy::fit_without(world, actor, &yield_items(world), &[work.tool])
}

/// The host calls once in sorted island Effects, before autonomous decisions.
pub fn tick(world: &mut World) {
    let now = world.resource::<SimClock>().tick;
    let hunters: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, entity)| {
            world
                .get::<Hunting>(*entity)
                .map(|work| (*id, *entity, *work))
        })
        .collect();
    for (id, actor, work) in hunters {
        // Invalidated targets release claims promptly. Ownership, capacity and
        // reach are rechecked at completion before any physical goods change.
        let invalid = !living(world, actor)
            || entity_of(world, work.target).is_none_or(|prey| !alive_prey(world, prey));
        if invalid {
            cancel(world, actor, "invalidated");
            continue;
        }
        if work.ends > now {
            continue;
        }
        if finish_check(world, actor, work).is_err() {
            cancel(world, actor, "invalidated");
            record_effect(
                world,
                Some(id),
                "VillageHuntFailed",
                &json!({"outcome":"world_changed"}),
            );
            continue;
        }
        world.entity_mut(actor).remove::<Hunting>();
        let mut claims = [work.target, work.tool];
        claims.sort_unstable();
        for target in claims {
            release(world, actor, target, ReleaseReason::Finished);
        }
        let tool = entity_of(world, work.tool).expect("checked spear");
        items::take_from_holder(world, tool);
        world.despawn(tool);
        let prey = entity_of(world, work.target).expect("checked animal");
        world.despawn(prey);
        for item in yield_items(world) {
            economy::produce(world, &item.kind, item.stack);
            super::production::completed(world, id, &item.kind, item.stack);
            super::live::booked(
                world,
                id,
                super::live::Book::Gathered,
                &item.kind,
                item.stack,
            );
            economy::grant(world, actor, item);
        }
        record_effect(
            world,
            Some(id),
            "VillageHunted",
            &json!({"target":work.target,"meat":2,"hide":1,"spear_spent":true}),
        );
    }
    super::frontier::ecology::wildlife_tick(world);
}

fn replenish(world: &mut World, habitat: SimId, home: SimPos, capacity: u32) {
    let individuals: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|entity| {
            world
                .get::<Wildlife>(*entity)
                .is_some_and(|wildlife| wildlife.habitat == habitat)
        })
        .collect();
    let mut positions = Vec::new();
    let mut living_count = 0_usize;
    for entity in individuals {
        if alive_prey(world, entity) {
            living_count += 1;
            if let Some(pos) = world.get::<SimPos>(entity) {
                positions.push(*pos);
            }
        } else {
            world.despawn(entity);
        }
    }
    for (x, z) in [
        (0, 0),
        (-1500, 0),
        (1500, 0),
        (0, -1500),
        (0, 1500),
        (-1500, -1500),
        (1500, 1500),
        (-1500, 1500),
    ] {
        if living_count >= capacity.min(MAX_WILDLIFE) as usize {
            break;
        }
        let pos = SimPos::new(home.x + x, home.z + z);
        if positions.contains(&pos) {
            continue;
        }
        spawn_sim(
            world,
            (
                pos,
                Wildlife { habitat },
                Vitality::default(),
                Perceivable::new(crate::world::kinds::UNIT),
            ),
        );
        positions.push(pos);
        living_count += 1;
    }
}
