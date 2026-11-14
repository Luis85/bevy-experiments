//! Opt-in AI village: physical production, finite trade and permanent lives.
mod agent;
mod ai;
pub mod behavior;
pub mod catalog;
pub mod civic;
pub mod colony;
mod commands;
pub mod demography;
mod discovery;
pub mod economy;
pub mod environment;
pub mod fields;
pub mod frontier;
pub mod geography;
pub mod goap;
pub use geography as geometry;
pub mod homestead;
pub mod host;
pub mod hunting;
pub mod indoors;
pub mod integrity;
pub mod island_save;
pub mod ledger;
pub mod live;
pub mod living;
pub mod market;
pub mod merchant;
mod model;
pub mod player_journey;
pub mod policy;
pub mod production;
pub mod public_feedback;
pub mod resident;
pub mod roles;
pub mod routing;
mod scheduler;
pub mod speech;
pub mod spoilage;
pub mod stock;
pub mod terrain;
pub mod view;
pub mod weather;
pub mod wildlife;
pub use agent::activity;
use bevy_app::App;
use bevy_ecs::prelude::*;
pub use commands::*;
pub use model::*;
use npc_framework::agent_io::AgentIoAppExt;
use npc_framework::crafting::RecipeRegistry;
use npc_framework::interaction::InteractionAppExt;
use npc_framework::needs::{NeedDef, NeedRegistry, NeedThresholds};
use npc_framework::prelude::*;
pub const REMAINS_TREE: &str = "island_remains";

/// Immutable construction data rebuilt from the saved scenario, like ActorPresets.
/// Never inferred from registry ordering or read from the host's preset files during play.
#[derive(Resource)]
pub(crate) struct BirthPreset(pub String);

pub fn intrinsic_action(verb: &str) -> bool {
    verb == "join_island"
        || frontier::activities::VERBS.contains(&verb)
        || homestead::VERBS.contains(&verb)
        || matches!(
            verb,
            "buy"
                | "sell"
                | "gather"
                | "hunt"
                | "offer_goods"
                | "withdraw_offer"
                | "eat"
                | "drink"
                | "rest"
                | "rest_home"
                | "rest_ground"
                | "talk"
                | "read_diary"
                | "write_diary"
                | "new_life"
                | "ask_diary"
                | "collect_coins"
                | "recover_diary"
                | "stop_work"
        )
}

pub fn prepare(app: &mut App, p: &crate::worlds::WorldPreset) {
    use npc_framework::perf::PerfAppExt;
    app.register_budget("village.economy", 250, 0);
    app.insert_resource(crate::world_runtime::ActiveWorld(p.clone()))
        .register_sim_resource::<crate::world_runtime::ActiveWorld>();
    frontier::register(app);
    // Host observation only: no sim registry/hash/snapshot entry and no tick system.
    app.insert_resource(
        npc_framework::agent_io::scheduler::SchedulerEventClassifier(scheduler::classify),
    );
    app.world_mut().resource_mut::<SimConfig>().day_length = p.day_night.cycle_ticks;
    app.world_mut().resource_mut::<SimConfig>().night_permille =
        if p.day_night.on { 300 } else { 0 };
    catalog::register(app);
    if p.colony {
        colony::register(app);
        terrain::register(app);
        discovery::register(app);
    }
    homestead::register(app);
    living::register(app);
    hunting::register(app);
    wildlife::register(app);
    production::register(app);
    market::register(app);
    for verb in ["equip", "unequip"] {
        app.register_action_handler(
            verb,
            npc_framework::glue::interaction_agent::interaction_handler,
        );
    }
    behavior::register(app);
    app.register_action(
        "IslandRemains",
        |_, _, _| Status::Success,
        crate::actions::instant_stop,
    )
    .register_tree(REMAINS_TREE, |b| {
        b.leaf("Remains", "IslandRemains", ());
    });
    for (i, (name, spec)) in crate::worlds::need_specs(p).into_iter().take(3).enumerate() {
        app.register_need(NeedDef {
            name,
            key: ["hunger", "thirst", "energy"][i],
            order: i as i32,
            decay_per_tick: spec.rise_milli_per_tick,
            thresholds: NeedThresholds {
                // Clamped as well as validated: a threshold of 0 would
                // overflow the need range.
                low_enter: (100_001 - spec.low_milli).clamp(1, 100_000),
                low_exit: (102_000 - spec.low_milli).clamp(1, 100_000),
                urgent_enter: (100_001 - spec.out_milli).clamp(1, 100_000),
                urgent_exit: (102_000 - spec.out_milli).clamp(1, 100_000),
            },
        });
    }
    app.register_sim_component::<Villager>()
        .register_sim_component::<Wallet>()
        .register_sim_component::<Vitality>()
        .register_sim_component::<VillagePlace>()
        .register_sim_component::<ResourcePatch>()
        .register_sim_component::<Gathering>()
        .register_sim_component::<VillageDiary>()
        .register_sim_component::<DroppedDiary>()
        .register_sim_component::<CoinPurse>()
        .register_sim_resource::<VillageState>()
        .register_command::<VillageAct>()
        .register_command::<PublishDiary>()
        .register_verb_gate("island.world_rules", crate::world_runtime::verb_gate)
        .register_snapshot_component_named::<VillageSight>("island_base_sight")
        .register_observation("village", agent::observe)
        .register_affordances("village", agent::affordances)
        .register_describer("island.name", agent::describe)
        .register_verb_gate("island.life", agent::verb_gate)
        .register_busy("island.work", |world, e| {
            if !living(world, e) {
                return None;
            }
            if world.get::<Gathering>(e).is_some() {
                return Some("gathering".into());
            }
            if world.get::<hunting::Hunting>(e).is_some() {
                return Some("hunting".into());
            }
            world
                .get::<Villager>(e)
                .filter(|v| v.resting_until > world.resource::<SimClock>().tick)
                .map(|_| "resting".into())
        })
        .add_busy_check("island.dead", |world, e| {
            (!living(world, e) && world.get::<Vitality>(e).is_some())
                .then(|| "this actor is dead".into())
        })
        .add_busy_check("island.hunting", |world, e| {
            world
                .get::<hunting::Hunting>(e)
                .map(|_| "hunting; finish or stop the hunt first".into())
        })
        .add_systems(NpcSimTick, tick.in_set(NpcSet::Effects));
    for verb in [
        "buy",
        "sell",
        "gather",
        "hunt",
        "offer_goods",
        "withdraw_offer",
        "eat",
        "drink",
        "rest",
        "rest_home",
        "rest_ground",
        "talk",
        "read_diary",
        "write_diary",
        "new_life",
        "ask_diary",
        "collect_coins",
        "recover_diary",
        "stop_work",
    ] {
        app.register_action_handler(verb, agent::handle);
    }
    app.add_craft_hook("island.production", |world, completed| {
        if completed.quality.is_some()
            && world
                .resource::<SimIndex>()
                .get(completed.unit)
                .is_some_and(|e| world.get::<Villager>(e).is_some())
        {
            let outputs = world
                .resource::<RecipeRegistry>()
                .recipes
                .get(&completed.recipe)
                .map(|r| r.outputs.clone())
                .unwrap_or_default();
            for output in outputs {
                production::completed(world, completed.unit, &output.kind, output.count);
                economy::produce(world, &output.kind, output.count);
                live::booked(
                    world,
                    completed.unit,
                    live::Book::Made,
                    &output.kind,
                    output.count,
                );
            }
        }
    });
}
pub fn land_cell(x: i32, z: i32) -> bool {
    geography::FIXED.land_cell(x, z)
}
pub const PLACES: [(Role, i32, i32, &str); 9] = [
    (Role::Farmer, -12, 6, "Ada's farm"),
    (Role::Miller, -6, 6, "Rowan's mill"),
    (Role::Baker, 0, 6, "Mira's bakery"),
    (Role::WaterKeeper, 7, 3, "Nell's well"),
    (Role::Woodcutter, -12, -5, "Oak's timber yard"),
    (Role::Carpenter, -6, -5, "Jun's workshop"),
    (Role::Miner, 13, 8, "Bram's mine"),
    (Role::Smith, 12, -5, "Ivo's smithy"),
    (Role::Innkeeper, 2, -7, "The Lantern Inn"),
];
pub fn spawn_places(world: &mut World, ids: &mut crate::IslandIds) {
    frontier::spawn_places(world, ids);
}
pub fn apply(world: &mut World, played: SimId) {
    if world.contains_resource::<VillageState>() {
        return;
    }
    let alias = world
        .resource::<crate::IslandIds>()
        .names
        .iter()
        .find(|(_, id)| **id == played)
        .map(|(n, _)| n.clone())
        .unwrap_or_else(|| "human".into());
    world.insert_resource(VillageState {
        played,
        agent_alias: alias.clone(),
        life: 1,
        fund: 0,
        last_dividend_day: 0,
        produced: Default::default(),
        transactions: 0,
    });
    let e = world
        .resource::<SimIndex>()
        .get(played)
        .expect("played actor");
    let home = world.get::<SimPos>(e).copied().unwrap_or_default();
    initialise_person(world, e, Role::Visitor, &alias, home, SimId(0));
    world.entity_mut(e).insert(VillageDiary::default());
    frontier::spawn(world);
}
pub(crate) fn initialise_person(
    world: &mut World,
    e: Entity,
    role: Role,
    name: &str,
    home: SimPos,
    workplace: SimId,
) {
    let needs = world.resource::<NeedRegistry>().full();
    let speed = world
        .get::<Commandable>(e)
        .map_or(70, |c| c.speed_mm_per_tick);
    let life = if role == Role::Visitor {
        world.resource::<VillageState>().life
    } else {
        0
    };
    let sight = world
        .get::<npc_framework::perception::Sensors>(e)
        .cloned()
        .unwrap_or_default();
    world.entity_mut(e).insert(VillageSight(sight));
    world.entity_mut(e).insert((
        needs,
        Wallet::default(),
        Vitality::default(),
        Villager {
            role,
            name: name.into(),
            home,
            workplace,
            base_speed: speed,
            next_decision: 0,
            resting_until: 0,
            rest_rate: 0,
            life,
        },
    ));
    living::prepare_person(world, e);
    colony::participant_spawned(world, e);
}
/// Cross-context integration: civic demand changed, so its production owner
/// invalidates the cached funded plan. Colony modules never mutate that plan.
fn colony_demand_changed(world: &mut World) {
    production::invalidate(world);
}

/// A character still to arrive has not begun a life (closing fix C1): any
/// needs it carries stay full, so it cannot starve, thirst, tire or freeze,
/// and raises no alerts, until the recorded arrival.
fn hold_arriving_needs(world: &mut World) {
    let Some(mut query) = world
        .try_query_filtered::<&mut npc_framework::needs::Needs, With<colony::AwaitingArrival>>()
    else {
        return;
    };
    for mut needs in query.iter_mut(world) {
        for need in &mut needs.values {
            need.value = 100_000;
        }
    }
}

fn tick(world: &mut World) {
    // The ledger closes a season before anything of the new day happens
    // (closing fix: it ran after the day's levy and grants, so a season's
    // closing fund already held the next day's flows).
    if world.contains_resource::<ledger::SeasonLedger>() {
        ledger::tick(world);
    }
    hold_arriving_needs(world);
    ai::tick(world);
    homestead::tick(world);
    colony::tick(world);
    frontier::tick(world);
}
