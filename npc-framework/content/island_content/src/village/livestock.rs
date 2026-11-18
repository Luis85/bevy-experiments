//! Animals kept in a workplace (living world only).
//!
//! A building with the `livestock` attribute (`economy.buildings[].livestock`)
//! keeps a herd as counted stock: no animal is an actor. Its keeper's trade
//! works the building's recipes as ordinary make-to-order and standing work;
//! each finished batch feeds `fed_per_batch` animals (the feed is the recipe's
//! input, the product its output), and a batch can only start while the herd
//! has unfed animals for it today. At dawn the day is settled: a herd left
//! with unfed animals is hungry, and after `starve_days` hungry days in a row
//! it loses an animal (never below `min_head`); a herd fed in full for
//! `breed_days` days in a row outside winter gains one (up to `max_head`).
//!
//! Nothing here runs per tick beyond a clock check: the work is one pass a
//! day over the pens, and the start gate reads the keepers of one pen.
use super::Season;
use super::civic::Building;
use super::frontier::building_spec::LivestockDef;
use super::frontier::economy_spec;
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::crafting::{CraftCompleted, Crafting, RecipeRegistry};
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The herd of one pen.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Livestock {
    /// Animals in the pen now.
    pub head: u32,
    /// Animals fed since dawn.
    pub fed_today: u32,
    /// Days in a row the herd was left with unfed animals.
    pub hungry_days: u32,
    /// Days in a row every animal was fed.
    pub fed_days: u32,
    pub born: u64,
    pub died: u64,
    /// Animal-days fed, over the pen's life.
    pub feedings: u64,
}
impl SimHash for Livestock {
    const NAME: &'static str = "island_livestock_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.head);
        h.u32(self.fed_today);
        h.u32(self.hungry_days);
        h.u32(self.fed_days);
        h.u64(self.born);
        h.u64(self.died);
        h.u64(self.feedings);
    }
}

/// The pens, ascending by identity.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct LivestockState {
    pub pens: Vec<SimId>,
}
impl SimHashResource for LivestockState {
    const NAME: &'static str = "island_livestock_state_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.pens.len() as u32);
        for id in &self.pens {
            h.u32(id.0);
        }
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

/// Living world only.
pub fn register(app: &mut App) {
    app.init_resource::<LivestockState>()
        .register_sim_resource::<LivestockState>()
        .register_sim_component::<Livestock>();
    app.add_craft_hook("island.livestock", fed);
}

/// The livestock definition of a pen's building.
fn def_of(world: &World, entity: Entity) -> Option<&LivestockDef> {
    let building = world.get::<Building>(entity)?;
    economy_spec::registry(world)?
        .spec
        .building(&building.building)?
        .livestock
        .as_ref()
}

/// Called when a building with the `livestock` attribute is finished or founded.
pub(super) fn established(world: &mut World, entity: Entity, def: &LivestockDef) {
    let Some(id) = world.get::<SimId>(entity).copied() else {
        return;
    };
    world.entity_mut(entity).insert(Livestock {
        head: def.head,
        fed_today: 0,
        hungry_days: 0,
        fed_days: 0,
        born: 0,
        died: 0,
        feedings: 0,
    });
    let mut state = world.resource_mut::<LivestockState>();
    if let Err(at) = state.pens.binary_search(&id) {
        state.pens.insert(at, id);
    }
}

/// Batches of the pen's recipes now being worked there.
fn in_progress(world: &World, pen: SimId) -> u32 {
    let Some(mut query) = world.try_query::<&Crafting>() else {
        return 0;
    };
    query
        .iter(world)
        .filter(|craft| craft.workstation == Some(pen))
        .count() as u32
}

/// Whether another batch may start at `workplace`: always outside a pen; in a
/// pen while it has animals not yet fed today for one more batch, counting
/// the batches being worked.
pub fn can_feed(world: &World, workplace: SimId) -> bool {
    let Some(entity) = world.resource::<SimIndex>().get(workplace) else {
        return true;
    };
    let Some(herd) = world.get::<Livestock>(entity) else {
        return true;
    };
    let per_batch = def_of(world, entity).map_or(1, |def| def.fed_per_batch.max(1));
    let planned = herd
        .fed_today
        .saturating_add(in_progress(world, workplace).saturating_mul(per_batch));
    planned.saturating_add(per_batch) <= herd.head
}

/// Craft hook: a finished batch at a pen feeds its animals.
fn fed(world: &mut World, completed: &CraftCompleted) {
    let Some(pen) = completed.workstation else {
        return;
    };
    let Some(entity) = world.resource::<SimIndex>().get(pen) else {
        return;
    };
    if world.get::<Livestock>(entity).is_none() {
        return;
    }
    let per_batch = def_of(world, entity).map_or(1, |def| def.fed_per_batch.max(1));
    let mut herd = world.get_mut::<Livestock>(entity).expect("herd");
    let fed = per_batch.min(herd.head.saturating_sub(herd.fed_today));
    herd.fed_today += fed;
    herd.feedings += u64::from(fed);
}

/// Pens of a settlement, ascending by identity.
pub fn pens_of(world: &World, settlement: SimId) -> Vec<(SimId, Entity)> {
    let Some(state) = world.get_resource::<LivestockState>() else {
        return Vec::new();
    };
    let index = world.resource::<SimIndex>();
    state
        .pens
        .iter()
        .filter_map(|id| Some((*id, index.get(*id)?)))
        .filter(|(_, entity)| {
            world
                .get::<Building>(*entity)
                .is_some_and(|building| building.settlement == settlement)
        })
        .collect()
}

/// Units of `kind` a settlement's pens yield in a day with every animal fed:
/// the most a batch of a pen's recipes makes of it, per animal fed.
pub fn daily_yield(world: &World, settlement: SimId, kind: &str) -> u32 {
    yield_of(world, settlement, kind, |_, herd, _| herd.head)
}

/// Units of `kind` the animals of a settlement's pens not yet fed today (nor
/// being fed) would yield. The keeper feeds them whatever the stock of the
/// product: an animal is fed every day, and surplus goes to the stores and
/// the boat.
pub fn unfed_yield(world: &World, settlement: SimId, kind: &str) -> u32 {
    yield_of(world, settlement, kind, |id, herd, per_batch| {
        herd.head.saturating_sub(
            herd.fed_today
                .saturating_add(in_progress(world, id).saturating_mul(per_batch)),
        )
    })
}

/// The yield of `kind` of `animals(pen, herd, fed per batch)` animals of each
/// pen of the settlement.
fn yield_of(
    world: &World,
    settlement: SimId,
    kind: &str,
    animals: impl Fn(SimId, &Livestock, u32) -> u32,
) -> u32 {
    let Some(state) = world.get_resource::<LivestockState>() else {
        return 0;
    };
    if state.pens.is_empty() {
        return 0;
    }
    let recipes = world.resource::<RecipeRegistry>();
    pens_of(world, settlement)
        .into_iter()
        .filter_map(|(id, entity)| {
            let herd = world.get::<Livestock>(entity)?;
            let def = def_of(world, entity)?;
            let station = world.get::<npc_framework::crafting::Workstation>(entity)?;
            let out = recipes
                .recipes
                .values()
                .filter(|recipe| recipe.hosted_by(&station.kinds))
                .flat_map(|recipe| &recipe.outputs)
                .filter(|output| output.kind == kind)
                .map(|output| output.count)
                .max()?;
            let per_batch = def.fed_per_batch.max(1);
            Some(animals(id, herd, per_batch) / per_batch * out)
        })
        .fold(0, u32::saturating_add)
}

/// Once per tick in the island's Effects pass: settles the day at dawn.
pub fn tick(world: &mut World) {
    let Some(state) = world.get_resource::<LivestockState>() else {
        return;
    };
    if state.pens.is_empty() {
        return;
    }
    let now = world.resource::<SimClock>().tick;
    let day = world.resource::<SimConfig>().day_length.max(1);
    if now == 0 || !now.is_multiple_of(day) {
        return;
    }
    let winter = super::calendar(world).1 == Season::Winter;
    let pens: Vec<SimId> = world.resource::<LivestockState>().pens.clone();
    for id in pens {
        let Some(entity) = world.resource::<SimIndex>().get(id) else {
            continue;
        };
        let Some(def) = def_of(world, entity).cloned() else {
            continue;
        };
        let Some(mut herd) = world.get_mut::<Livestock>(entity) else {
            continue;
        };
        let unfed = herd.head.saturating_sub(herd.fed_today);
        herd.fed_today = 0;
        let mut change = None;
        if unfed > 0 {
            herd.fed_days = 0;
            herd.hungry_days += 1;
            if herd.hungry_days >= def.starve_days && herd.head > def.min_head {
                herd.head -= 1;
                herd.died += 1;
                herd.hungry_days = 0;
                change = Some(("VillageLivestockLost", herd.head, unfed));
            }
        } else if herd.head > 0 {
            herd.hungry_days = 0;
            herd.fed_days += 1;
            if !winter && herd.fed_days >= def.breed_days && herd.head < def.max_head {
                herd.head += 1;
                herd.born += 1;
                herd.fed_days = 0;
                change = Some(("VillageLivestockBorn", herd.head, 0));
            }
        }
        if let Some((effect, head, unfed)) = change {
            record_effect(
                world,
                None,
                effect,
                &json!({"pen":id,"animals":def.display,"head":head,"unfed":unfed}),
            );
        }
    }
}

/// One pen, for drawing and for panels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PenView {
    pub id: SimId,
    pub label: String,
    pub settlement: SimId,
    pub plot: super::civic::Plot,
    /// The animals, as players read them (`hens`).
    pub animals: String,
    pub head: u32,
    pub max_head: u32,
    pub fed_today: u32,
    pub hungry_days: u32,
    pub born: u64,
    pub died: u64,
}

/// Every pen, ascending by identity.
pub fn views(world: &World) -> Vec<PenView> {
    let Some(state) = world.get_resource::<LivestockState>() else {
        return Vec::new();
    };
    state
        .pens
        .iter()
        .filter_map(|id| {
            let entity = world.resource::<SimIndex>().get(*id)?;
            let herd = world.get::<Livestock>(entity)?;
            let building = world.get::<Building>(entity)?;
            let def = def_of(world, entity)?;
            Some(PenView {
                id: *id,
                label: building.label.clone(),
                settlement: building.settlement,
                plot: building.plot,
                animals: def.display.clone(),
                head: herd.head,
                max_head: def.max_head,
                fed_today: herd.fed_today,
                hungry_days: herd.hungry_days,
                born: herd.born,
                died: herd.died,
            })
        })
        .collect()
}

/// Checks the pen index against the entities it names, after a load.
pub fn validate(world: &World) -> Result<(), String> {
    let Some(state) = world.get_resource::<LivestockState>() else {
        return Ok(());
    };
    let index = world.resource::<SimIndex>();
    for id in &state.pens {
        let entity = index.get(*id);
        if entity
            .and_then(|entity| world.get::<Livestock>(entity))
            .is_none()
            || entity.and_then(|entity| def_of(world, entity)).is_none()
        {
            return Err(format!("Pen {} is missing or keeps no livestock", id.0));
        }
    }
    Ok(())
}
