//! Opt-in AI village: physical production, finite trade and permanent lives.
mod agent;
mod ai;
pub mod behavior;
pub mod catalog;
mod commands;
pub mod economy;
pub mod environment;
pub mod geography;
pub use geography as geometry;
pub mod homestead;
pub mod host;
pub mod hunting;
pub mod living;
pub mod market;
mod model;
pub mod production;
pub mod public_feedback;
mod scheduler;
pub mod speech;
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

pub fn needs_driven(world: &World) -> bool {
    world
        .get_resource::<crate::world_runtime::ActiveWorld>()
        .is_some_and(|active| active.0.economy == crate::worlds::EconomyModel::NeedsDriven)
}

/// Immutable construction data rebuilt from the saved scenario, like ActorPresets.
/// Never inferred from registry ordering or read from the host's preset files during play.
#[derive(Resource)]
pub(crate) struct BirthPreset(pub String);

pub fn intrinsic_action(verb: &str) -> bool {
    homestead::VERBS.contains(&verb)
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
    app.insert_resource(crate::world_runtime::ActiveWorld(p.clone()));
    // Host observation only: no sim registry/hash/snapshot entry and no tick system.
    app.insert_resource(
        npc_framework::agent_io::scheduler::SchedulerEventClassifier(scheduler::classify),
    );
    app.world_mut().resource_mut::<SimConfig>().day_length = p.day_night.cycle_ticks;
    app.world_mut().resource_mut::<SimConfig>().night_permille =
        if p.day_night.on { 300 } else { 0 };
    catalog::register(app);
    if needs_driven(app.world()) {
        if geography::is_expanded(app.world()) {
            geography::register_living(app);
            homestead::register(app);
        }
        catalog::register_living(app);
        living::register(app);
        hunting::register(app);
        production::register(app);
        market::register(app);
        for verb in ["equip", "unequip"] {
            app.register_action_handler(
                verb,
                npc_framework::glue::interaction_agent::interaction_handler,
            );
        }
        for verb in catalog::LIVING_RECIPE_VERBS {
            app.register_action_handler(verb, npc_framework::glue::crafting_agent::craft_handler);
        }
    }
    for verb in catalog::RECIPE_VERBS {
        app.register_action_handler(verb, npc_framework::glue::crafting_agent::craft_handler);
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
                low_enter: 100_001 - spec.low_milli,
                low_exit: (102_000 - spec.low_milli).min(100_000),
                urgent_enter: 100_001 - spec.out_milli,
                urgent_exit: (102_000 - spec.out_milli).min(100_000),
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
        .register_verb_gate("testbed.world_rules", crate::world_runtime::verb_gate)
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
                if needs_driven(world) {
                    production::completed(world, completed.unit, &output.kind, output.count);
                }
                economy::produce(world, &output.kind, output.count);
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
pub fn spawn_places(world: &mut World, ids: &mut crate::TestbedIds) {
    let day_length = world.resource::<SimConfig>().day_length;
    let geography = *geography::for_world(world);
    // Allocate exactly the original shoreline identities before any incumbent
    // workplace/actor/item. Expanded rows reuse those identities in place;
    // their ten additional right spans are appended only after construction.
    for z in geography::MIN_CELL..geography::MAX_CELL_EXCLUSIVE {
        let expanded = geography.shoreline_segments(z);
        for (index, old) in geography::FIXED
            .shoreline_segments(z)
            .into_iter()
            .enumerate()
        {
            if old.is_none() {
                continue;
            }
            let segment = expanded[index].expect("expanded shoreline retains incumbent row spans");
            spawn_sim(world, (segment.pos, GridBlocker(segment.footprint)));
        }
    }
    for (role, x, z, label) in PLACES {
        let pos = crate::layout::cell(x, z);
        let station = match role {
            Role::Miller => catalog::MILL,
            Role::Baker => catalog::BAKE,
            Role::Carpenter => catalog::SAW,
            Role::Smith => catalog::SMELT,
            _ => catalog::INN,
        };
        let (id, _) = spawn_sim(
            world,
            (
                pos,
                VillagePlace {
                    role,
                    label: label.into(),
                },
                Workstation::new([station]),
                Approach(pos),
                Perceivable::new(crate::world::kinds::STATION),
            ),
        );
        ids.name(format!("village_{}", role.tag()), id);
        if role != Role::WaterKeeper {
            spawn_sim(
                world,
                (
                    SimPos::new(pos.x, pos.z + 2000),
                    GridBlocker(Footprint {
                        half_x_mm: 1000,
                        half_z_mm: 1000,
                    }),
                    GridOccluder(Footprint {
                        half_x_mm: 1000,
                        half_z_mm: 1000,
                    }),
                ),
            );
        }
    }
    for (kind, x, z, ticks, capacity) in [
        (catalog::GRAIN, -16, 7, 180, 80),
        ("log", -18, -5, 240, 60),
        ("ore", 17, 8, 300, 60),
        (catalog::WATER, 8, 3, 100, 100),
    ] {
        let (id, _) = spawn_sim(
            world,
            (
                geography
                    .resource_position(kind)
                    .unwrap_or_else(|| crate::layout::cell(x, z)),
                ResourcePatch {
                    kind: kind.into(),
                    remaining: capacity,
                    capacity,
                    replenish_at: day_length,
                    gather_ticks: ticks,
                },
                Perceivable::new(crate::world::kinds::STATION),
            ),
        );
        ids.name(format!("patch_{kind}"), id);
    }
}
pub fn apply(world: &mut World, played: SimId) {
    if world.contains_resource::<VillageState>() {
        return;
    }
    let alias = world
        .resource::<crate::TestbedIds>()
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
    for role in Role::RESIDENTS {
        let station = world
            .resource::<crate::TestbedIds>()
            .id(&format!("village_{}", role.tag()));
        let place = world.resource::<SimIndex>().get(station).expect("station");
        let pos = *world.get::<SimPos>(place).expect("station pos");
        let spec = crate::layout::UnitSpec {
            template: match role {
                Role::Smith => "smith",
                Role::Carpenter => "carpenter",
                Role::Woodcutter => "woodcutter",
                _ => "novice",
            }
            .into(),
            behaviour: crate::Behaviour::Idle,
            ..Default::default()
        };
        let id = crate::layout::spawn_unit(world, SimPos::new(pos.x, pos.z - 1000), &spec)
            .expect("resident template");
        let e = world.resource::<SimIndex>().get(id).expect("resident");
        initialise_person(
            world,
            e,
            role,
            role.person(),
            SimPos::new(pos.x - 1000, pos.z - 1000),
            station,
        );
        let tree = world
            .resource::<AiRegistry>()
            .tree_id(behavior::TREE)
            .expect("island life tree");
        world
            .entity_mut(e)
            .remove::<(BrainState, ActiveLeaf)>()
            .insert((Brain { tree }, Inventory::new(64, 500_000)));
        world.get_mut::<Wallet>(e).expect("wallet").coins = NPC_SEED_COINS;
        for kind in catalog::outputs(role) {
            let n = if *kind == "hammer" { 2 } else { 8 };
            economy::grant(world, e, Item::new(*kind).stack(n));
        }
        if role == Role::Innkeeper {
            for (kind, n) in [("log", 4), (catalog::BOWL, 2), ("hammer", 1)] {
                economy::grant(world, e, Item::new(kind).stack(n));
            }
        }
        if role != Role::Baker {
            economy::grant(world, e, Item::new("bread").stack(2));
        }
        if role != Role::WaterKeeper {
            economy::grant(world, e, Item::new(catalog::WATER).stack(2));
        }
        let mut ids = world.resource_mut::<crate::TestbedIds>();
        ids.name(role.person(), id);
        ids.units.push(id);
    }
    if needs_driven(world) {
        living::spawn(world);
        let mut ids = world
            .remove_resource::<crate::TestbedIds>()
            .expect("world identities");
        hunting::spawn(world, &mut ids);
        world.insert_resource(ids);
        geography::append_boundary(world);
        homestead::spawn(world);
        production::validate(world).expect("living production graph must be acyclic");
    }
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
}
fn tick(world: &mut World) {
    ai::tick(world);
    homestead::tick(world);
}
