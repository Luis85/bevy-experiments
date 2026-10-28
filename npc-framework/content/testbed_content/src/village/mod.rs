//! Opt-in AI village: physical production, finite trade and permanent lives.
mod agent;
mod ai;
pub mod catalog;
mod commands;
pub mod economy;
pub mod host;
mod model;
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

pub fn intrinsic_action(verb: &str) -> bool {
    matches!(
        verb,
        "buy"
            | "sell"
            | "gather"
            | "eat"
            | "drink"
            | "rest"
            | "talk"
            | "read_diary"
            | "write_diary"
            | "new_life"
            | "ask_diary"
            | "collect_coins"
            | "stop_work"
    )
}

pub fn prepare(app: &mut App, p: &crate::worlds::WorldPreset) {
    use npc_framework::perf::PerfAppExt;
    app.register_budget("village.economy", 250, 0);
    app.insert_resource(crate::world_runtime::ActiveWorld(p.clone()));
    app.world_mut().resource_mut::<SimConfig>().day_length = p.day_night.cycle_ticks;
    app.world_mut().resource_mut::<SimConfig>().night_permille =
        if p.day_night.on { 300 } else { 0 };
    catalog::register(app);
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
            world
                .get::<Villager>(e)
                .filter(|v| v.resting_until > world.resource::<SimClock>().tick)
                .map(|_| "resting".into())
        })
        .add_busy_check("island.dead", |world, e| {
            (!living(world, e) && world.get::<Vitality>(e).is_some())
                .then(|| "this actor is dead".into())
        })
        .add_systems(NpcSimTick, tick.in_set(NpcSet::Effects));
    for verb in [
        "buy",
        "sell",
        "gather",
        "eat",
        "drink",
        "rest",
        "talk",
        "read_diary",
        "write_diary",
        "new_life",
        "ask_diary",
        "collect_coins",
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
                economy::produce(world, &output.kind, output.count);
            }
        }
    });
}
pub fn land_cell(x: i32, z: i32) -> bool {
    let x = i64::from(2 * x + 1);
    let z = i64::from(2 * z + 1);
    x * x * 400 + z * z * 576 <= 4 * 576 * 400
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
    for z in -32..32 {
        let land: Vec<_> = (-32..32).filter(|x| land_cell(*x, z)).collect();
        let (lo, hi) = land
            .first()
            .zip(land.last())
            .map_or((32, 31), |(a, b)| (*a, *b));
        for (a, b) in [(-32, lo - 1), (hi + 1, 31)] {
            if a > b {
                continue;
            }
            spawn_sim(
                world,
                (
                    SimPos::new((a + b + 1) * 500, z * 1000 + 500),
                    GridBlocker(Footprint {
                        half_x_mm: (b - a) * 500,
                        half_z_mm: 0,
                    }),
                ),
            );
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
                crate::layout::cell(x, z),
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
        world
            .entity_mut(e)
            .remove::<Brain>()
            .insert(Inventory::new(64, 500_000));
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
}
fn tick(world: &mut World) {
    ai::tick(world);
}
