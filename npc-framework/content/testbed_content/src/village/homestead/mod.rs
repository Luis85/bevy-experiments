//! Shared, recorded homestead work. The host orders `tick` after village vitals.
mod agent;
mod commands;
mod crops;
mod model;
use super::{Villager, catalog, economy, geography, living};
pub use agent::{affordances, describe, handle, observe, plot_json};
use bevy_app::App;
use bevy_ecs::prelude::*;
pub use commands::HomesteadAct;
pub use crops::{CropDef, crop_catalog, crop_definition};
pub use model::{BuildMaterial, Cottage, CottageBuild, Crop, HomesteadPlot};
use npc_framework::agent_io::AgentIoAppExt;
use npc_framework::interaction::items::{self, entity_of};
use npc_framework::interaction::{CarriedBy, InteractionAppExt, Item};
use npc_framework::kernel::journal::ReleaseReason;
use npc_framework::prelude::*;
use serde_json::json;

pub const BUILD_TICKS: u32 = 600;
pub const GROW_TICKS: u32 = 6000;
pub const PLANK_COST: u32 = 8;
pub const LOG_COST: u32 = 4;
pub const HARVEST_GRAIN: u32 = 3;
pub const SEED_COUNT: u32 = 1;
pub const WATER_COUNT: u32 = 1;
pub const MAX_HOUSING_PLOTS: usize = 1;
pub const MAX_GARDEN_PLOTS: usize = 1;
pub const VERBS: &[&str] = &[
    "claim_plot",
    "build_cottage",
    "plant",
    "water_crop",
    "harvest",
];

pub fn register(app: &mut App) {
    if !geography::is_expanded(app.world()) {
        return;
    }
    app.register_sim_component::<HomesteadPlot>()
        .register_sim_component::<Cottage>()
        .register_sim_component::<CottageBuild>()
        .register_command::<HomesteadAct>()
        .register_observation("homestead", agent::observe)
        .register_affordances("island.homestead", agent::affordances)
        .register_describer("island.homestead", agent::describe)
        .register_possessions("island.homestead.owned", agent::owned_known)
        .register_busy("island.homestead", |world, actor| {
            busy(world, actor).map(str::to_owned)
        })
        .add_busy_check("island.homestead", |world, actor| {
            busy(world, actor).map(str::to_owned)
        });
    for verb in VERBS {
        app.register_action_handler(verb, agent::handle);
    }
}

/// Call once after all geography, residents, inventory and boundary entities.
pub fn spawn(world: &mut World) {
    if !geography::is_expanded(world) {
        return;
    }
    for definition in geography::for_world(world).expansion_plots {
        spawn_sim(
            world,
            (
                SimPos::new(definition.x, definition.z),
                HomesteadPlot {
                    plot_id: definition.id.into(),
                    owner: None,
                    cottage_built: false,
                    cottage_entity: None,
                    beds: vec![
                        None;
                        if definition.use_kind == "housing" {
                            3
                        } else {
                            6
                        }
                    ],
                },
                Perceivable::new(crate::world::kinds::STATION),
            ),
        );
    }
}

pub fn plots(world: &World) -> Vec<(SimId, Entity)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| world.get::<HomesteadPlot>(**entity).is_some())
        .map(|(id, entity)| (*id, *entity))
        .collect()
}

pub fn definition(world: &World, plot: Entity) -> Option<&geography::ExpansionPlot> {
    let state = world.get::<HomesteadPlot>(plot)?;
    geography::for_world(world)
        .expansion_plots
        .iter()
        .find(|definition| definition.id == state.plot_id)
}

pub fn cottage_center(center: SimPos) -> SimPos {
    SimPos::new(center.x, center.z - 1800)
}
pub fn front_door(center: SimPos) -> SimPos {
    center
}
pub fn cottage_footprint() -> Footprint {
    Footprint {
        half_x_mm: 1000,
        half_z_mm: 1000,
    }
}

pub fn bed_position(world: &World, plot: Entity, index: u32) -> Option<SimPos> {
    let definition = definition(world, plot)?;
    bed_position_in(definition, index)
}

pub fn bed_position_in(definition: &geography::ExpansionPlot, index: u32) -> Option<SimPos> {
    let offsets: &[(i32, i32)] = match definition.use_kind {
        "housing" => &[(0, 1400), (-1500, 1400), (1500, 1400)],
        "garden" => &[
            (0, -1000),
            (-1500, -1000),
            (1500, -1000),
            (0, 1000),
            (-1500, 1000),
            (1500, 1000),
        ],
        _ => return None,
    };
    let (x, z) = *offsets.get(index as usize)?;
    Some(SimPos::new(definition.x + x, definition.z + z))
}

pub fn crop_stage(crop: Option<&Crop>, now: u64) -> &'static str {
    match crop {
        None => "empty",
        Some(crop) if crop.watered_at.is_none() => "needs_water",
        Some(crop) if crop.ready_at.is_some_and(|ready| ready <= now) => "ready",
        _ => "growing",
    }
}

pub fn busy(world: &World, actor: Entity) -> Option<&'static str> {
    world
        .get::<CottageBuild>(actor)
        .map(|_| "building a cottage; finish or stop_work first")
}

pub(super) fn release(world: &mut World, actor: Entity, target: SimId, reason: ReleaseReason) {
    let Some(me) = world.get::<SimId>(actor).copied() else {
        return;
    };
    let mut reservations = world.resource_mut::<Reservations>();
    let held = reservations.holds(target, me);
    reservations.release(target, me);
    if held {
        npc_framework::kernel::journal::record(world, Some(me), || JournalKind::Release {
            target,
            reason,
        });
    }
}

pub fn cancel(world: &mut World, actor: Entity) {
    let Some(build) = world.entity_mut(actor).take::<CottageBuild>() else {
        return;
    };
    let mut claims: Vec<_> = build
        .materials
        .iter()
        .map(|material| material.item)
        .chain([build.plot])
        .collect();
    claims.sort_unstable();
    claims.dedup();
    for claim in claims {
        release(world, actor, claim, ReleaseReason::Aborted);
    }
    let unit = world.get::<SimId>(actor).copied();
    record_effect(
        world,
        unit,
        "VillageCottageCancelled",
        &json!({"plot":build.plot}),
    );
}

fn finish_build(world: &mut World, actor: Entity, build: CottageBuild) {
    if commands::finish_check(world, actor, &build).is_err() {
        cancel(world, actor);
        let unit = world.get::<SimId>(actor).copied();
        record_effect(
            world,
            unit,
            "VillageCottageFailed",
            &json!({"plot":build.plot,"outcome":"world_changed"}),
        );
        return;
    }
    let me = *world.get::<SimId>(actor).expect("checked actor");
    let plot = entity_of(world, build.plot).expect("checked plot");
    let center = *world.get::<SimPos>(plot).expect("plot position");
    let plot_id = world
        .get::<HomesteadPlot>(plot)
        .expect("plot")
        .plot_id
        .clone();
    world.entity_mut(actor).remove::<CottageBuild>();
    let mut claims: Vec<_> = build
        .materials
        .iter()
        .map(|material| material.item)
        .chain([build.plot])
        .collect();
    claims.sort_unstable();
    claims.dedup();
    for claim in claims {
        release(world, actor, claim, ReleaseReason::Finished);
    }
    for material in &build.materials {
        let entity = entity_of(world, material.item).expect("checked material");
        let remaining = world
            .get::<Item>(entity)
            .expect("checked material item")
            .stack
            - material.count;
        if remaining == 0 {
            items::take_from_holder(world, entity);
            world.despawn(entity);
        } else {
            world.get_mut::<Item>(entity).expect("material").stack = remaining;
        }
    }
    let (cottage, _) = spawn_sim(
        world,
        (
            cottage_center(center),
            Cottage {
                plot: build.plot,
                plot_id,
            },
            GridBlocker(cottage_footprint()),
            GridOccluder(cottage_footprint()),
            Perceivable::new(crate::world::kinds::STATION),
        ),
    );
    let mut estate = world.get_mut::<HomesteadPlot>(plot).expect("plot");
    estate.cottage_built = true;
    estate.cottage_entity = Some(cottage);
    world.get_mut::<Villager>(actor).expect("owner").home = front_door(center);
    super::colony::cottage_claimed(world, plot, me);
    record_effect(
        world,
        Some(me),
        "VillageCottageBuilt",
        &json!({"plot":build.plot,"cottage":cottage}),
    );
}

pub fn tick(world: &mut World) {
    if !geography::is_expanded(world) {
        return;
    }
    let now = world.resource::<SimClock>().tick;
    let builders: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter_map(|actor| {
            world
                .get::<CottageBuild>(actor)
                .cloned()
                .map(|build| (actor, build))
        })
        .collect();
    for (actor, build) in builders {
        if !living(world, actor) {
            cancel(world, actor);
        } else if build.ends <= now {
            finish_build(world, actor, build);
        }
    }
    let mut ready = Vec::new();
    for (plot_id, plot) in plots(world) {
        let mut estate = world.get_mut::<HomesteadPlot>(plot).expect("plot");
        let owner = estate.owner;
        for (index, bed) in estate.beds.iter_mut().enumerate() {
            if let Some(crop) = bed
                && !crop.ready_announced
                && crop.ready_at.is_some_and(|tick| tick <= now)
            {
                crop.ready_announced = true;
                ready.push((owner, plot_id, index));
            }
        }
    }
    for (owner, plot, bed) in ready {
        record_effect(
            world,
            owner,
            "VillageCropReady",
            &json!({"plot":plot,"bed":bed}),
        );
    }
}
