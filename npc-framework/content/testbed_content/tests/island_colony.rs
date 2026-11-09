//! Colony policies, physical social contacts and safe replayable terraforming.
use bevy_app::App;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use testbed_content::{
    TestbedIds,
    village::{
        self, VillageState, Villager, Vitality,
        colony::{
            self, AwaitingArrival, ColonyAct, ColonyState, Household, JoinIsland, SocialLife,
        },
        terrain::{Terraform, TerrainState, TerrainSurface},
    },
};

fn scene() -> App {
    testbed_content::new_sim_app(7, "island_colony")
}
fn visitor(app: &App) -> (SimId, bevy_ecs::entity::Entity) {
    let id = app.world().resource::<VillageState>().played;
    (id, app.world().resource::<SimIndex>().get(id).unwrap())
}
fn brush(x: i32, z: i32, surface: TerrainSurface, height_mm: i32) -> Terraform {
    Terraform {
        x,
        z,
        radius: 0,
        height_mm,
        surface,
    }
}

#[test]
fn inspector_identity_has_no_life_until_a_single_recorded_arrival() {
    let mut app = scene();
    let (id, entity) = visitor(&app);
    assert!(app.world().get::<AwaitingArrival>(entity).is_some());
    assert!(app.world().get::<Vitality>(entity).is_none());
    assert!(app.world().get::<Needs>(entity).is_none());
    assert_eq!(village::activity(app.world(), entity).0, "awaiting_arrival");
    assert!(npc_framework::agent_io::verb_gate(app.world(), entity, "gather").is_some());
    assert!(npc_framework::agent_io::verb_gate(app.world(), entity, "join_island").is_none());
    let initial = colony::summary(app.world());
    assert_eq!(
        (
            initial.residents,
            initial.housed,
            initial.households,
            initial.beds
        ),
        (12, 12, 12, 24)
    );
    run_ticks(app.world_mut(), 2400);
    assert!(app.world().get::<Vitality>(entity).is_none());
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&JoinIsland { unit: id.0 }),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    assert!(app.world().get::<AwaitingArrival>(entity).is_none());
    assert!(village::living(app.world(), entity));
    assert!(colony::home_of(app.world(), id).is_some());
    assert!(JoinIsland { unit: id.0 }.check(app.world()).is_err());
    app.world_mut().get_mut::<Vitality>(entity).unwrap().died_at = Some(2401);
    assert!(JoinIsland { unit: id.0 }.perform(app.world_mut()).is_err());
    assert_eq!(
        app.world().get::<Vitality>(entity).unwrap().died_at,
        Some(2401)
    );
}

#[test]
fn house_capacity_is_real_and_assignment_changes_only_the_home_destination() {
    let mut app = scene();
    let ids = app.world().resource::<TestbedIds>();
    let ada = ids.id("Ada");
    let rowan = ids.id("Rowan");
    let mira = ids.id("Mira");
    let home = colony::home_of(app.world(), ada).unwrap();
    let actor = app.world().resource::<SimIndex>().get(rowan).unwrap();
    let position = *app.world().get::<SimPos>(actor).unwrap();
    ColonyAct {
        verb: "assign_home".into(),
        resident: Some(rowan),
        house: Some(home),
        value: 0,
    }
    .perform(app.world_mut())
    .unwrap();
    assert_eq!(colony::home_of(app.world(), rowan), Some(home));
    assert_eq!(*app.world().get::<SimPos>(actor).unwrap(), position);
    let house = app.world().resource::<SimIndex>().get(home).unwrap();
    assert_eq!(
        app.world().get::<Villager>(actor).unwrap().home,
        app.world().get::<Household>(house).unwrap().door
    );
    assert!(
        ColonyAct {
            verb: "assign_home".into(),
            resident: Some(mira),
            house: Some(home),
            value: 0
        }
        .check(app.world())
        .is_err()
    );
    assert_eq!(colony::summary(app.world()).housed, 12);
}

#[test]
fn social_relationships_require_real_proximity_and_obey_gathering_policy() {
    let mut app = scene();
    let ids = app.world().resource::<TestbedIds>();
    let ada = ids.id("Ada");
    let rowan = ids.id("Rowan");
    let a = app.world().resource::<SimIndex>().get(ada).unwrap();
    let b = app.world().resource::<SimIndex>().get(rowan).unwrap();
    // Fixture relocation isolates proximity; no production/survival claim.
    app.world_mut().entity_mut(a).insert(SimPos::new(100, 100));
    app.world_mut().entity_mut(b).insert(SimPos::new(200, 100));
    app.world_mut().resource_mut::<SimClock>().tick = 300;
    colony::tick(app.world_mut());
    assert_eq!(
        app.world()
            .get::<SocialLife>(a)
            .unwrap()
            .affinity
            .get(&rowan),
        Some(&250)
    );
    assert_eq!(
        app.world().get::<SocialLife>(b).unwrap().affinity.get(&ada),
        Some(&250)
    );
    let events = app.world().resource::<ColonyState>().social_events;
    ColonyAct {
        verb: "gatherings".into(),
        resident: None,
        house: None,
        value: 0,
    }
    .perform(app.world_mut())
    .unwrap();
    app.world_mut().resource_mut::<SimClock>().tick = 600;
    colony::tick(app.world_mut());
    assert_eq!(app.world().resource::<ColonyState>().social_events, events);
    assert!(colony::social_target(app.world(), a).is_none());
}

#[test]
fn terrain_edits_change_navigation_atomically_and_reject_occupied_sites() {
    let mut app = scene();
    run_ticks(app.world_mut(), 1);
    let edit = brush(12, 16, TerrainSurface::Stone, 500);
    edit.perform(app.world_mut()).unwrap();
    let terrain = app.world().resource::<TerrainState>();
    assert_eq!(terrain.cell(12, 16).unwrap().height_mm, 500);
    assert_eq!(terrain.revision, 1);
    assert!(
        !app.world()
            .resource::<SpatialGrid>()
            .is_blocking(CellCoord::new(12, 16))
    );
    let occupied = brush(0, -2, TerrainSurface::Water, -500);
    let before = state_hash(app.world_mut());
    assert!(occupied.perform(app.world_mut()).is_err());
    assert_eq!(state_hash(app.world_mut()), before);
    let pond = brush(12, 16, TerrainSurface::Water, -500);
    pond.perform(app.world_mut()).unwrap();
    assert!(
        app.world()
            .resource::<SpatialGrid>()
            .is_blocking(CellCoord::new(12, 16))
    );
    brush(12, 16, TerrainSurface::Grass, 0)
        .perform(app.world_mut())
        .unwrap();
    assert!(
        !app.world()
            .resource::<SpatialGrid>()
            .is_blocking(CellCoord::new(12, 16))
    );
}

#[test]
fn isolated_land_and_malformed_brushes_are_rejected_without_state_changes() {
    let mut app = scene();
    run_ticks(app.world_mut(), 1);
    let before = state_hash(app.world_mut());
    for edit in [
        brush(29, 29, TerrainSurface::Grass, 0),
        brush(i32::MAX, 0, TerrainSurface::Grass, 0),
        brush(12, 16, TerrainSurface::Grass, -500),
        brush(12, 16, TerrainSurface::Water, 500),
        Terraform {
            radius: 4,
            ..brush(12, 16, TerrainSurface::Grass, 0)
        },
    ] {
        assert!(edit.perform(app.world_mut()).is_err(), "{edit:?}");
    }
    assert_eq!(state_hash(app.world_mut()), before);
    assert!(
        app.world()
            .resource::<TerrainState>()
            .cell(i32::MAX, i32::MIN)
            .is_none()
    );
}

#[test]
fn village_roads_are_editable_soil_with_lower_navigation_cost_than_grass() {
    let app = scene();
    let terrain = app.world().resource::<TerrainState>();
    let road = *terrain.cell(0, 0).unwrap();
    let grass = *terrain.cell(12, 16).unwrap();
    assert_eq!(road.surface, TerrainSurface::Soil);
    assert_eq!(grass.surface, TerrainSurface::Grass);
    assert!(road.navigation().extra_cost < grass.navigation().extra_cost);
    assert_eq!(
        app.world()
            .resource::<SpatialGrid>()
            .static_cell(CellCoord::new(0, 0)),
        road.navigation()
    );
}

fn edited() -> App {
    let mut app = scene();
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&brush(12, 16, TerrainSurface::Soil, 250)),
    )
    .unwrap();
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&ColonyAct {
            verb: "reserve_food".into(),
            resident: None,
            house: None,
            value: 3,
        }),
    )
    .unwrap();
    app
}

#[test]
fn colony_and_terrain_snapshot_resume_reproduce_subsequent_simulation() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(edited, 51, 700, 40);
}

#[test]
fn command_replay_is_deterministic_and_legacy_worlds_do_not_register_colony_state() {
    let mut a = edited();
    let mut b = edited();
    run_ticks(a.world_mut(), 800);
    run_ticks(b.world_mut(), 800);
    assert_eq!(state_hash(a.world_mut()), state_hash(b.world_mut()));
    assert_eq!(a.world().resource::<ColonyState>().food_reserve_days, 3);
    let legacy = testbed_content::new_sim_app(7, "living_village");
    assert!(legacy.world().get_resource::<ColonyState>().is_none());
    assert!(legacy.world().get_resource::<TerrainState>().is_none());
}

fn housing_site(app: &App) -> (SimId, bevy_ecs::entity::Entity) {
    village::homestead::plots(app.world())
        .into_iter()
        .find(|(_, entity)| {
            village::homestead::definition(app.world(), *entity)
                .unwrap()
                .use_kind
                == "housing"
        })
        .unwrap()
}

#[test]
fn housing_commissions_share_physical_build_costs_and_do_not_refinance_reserved_materials() {
    let mut app = scene();
    let builder = app.world().resource::<TestbedIds>().id("Jun");
    let actor = app.world().resource::<SimIndex>().get(builder).unwrap();
    app.world_mut().entity_mut(actor).remove::<Brain>();
    let (plot_id, plot) = housing_site(&app);
    let command = ColonyAct {
        verb: "commission_home".into(),
        resident: Some(builder),
        house: Some(plot_id),
        value: 0,
    };
    command.perform(app.world_mut()).unwrap();
    assert_eq!(
        colony::construction_materials(app.world(), actor),
        vec![("plank", 8), ("log", 4)]
    );
    assert!(!colony::construction_step(app.world_mut(), actor));
    assert!(
        !app.world()
            .get::<village::homestead::HomesteadPlot>(plot)
            .unwrap()
            .cottage_built
    );
    // Material grants/relocation isolate the work contract. The following
    // independent soak verifies procurement without any fixture grants.
    for (kind, required) in [("plank", 8_u32), ("log", 4)] {
        let missing = required.saturating_sub(village::economy::count(app.world(), actor, kind));
        village::economy::grant(app.world_mut(), actor, Item::new(kind).stack(missing));
    }
    let door = *app.world().get::<SimPos>(plot).unwrap();
    app.world_mut().entity_mut(actor).insert(door);
    assert!(colony::construction_step(app.world_mut(), actor));
    assert!(
        app.world()
            .get::<village::homestead::CottageBuild>(actor)
            .is_some()
    );
    assert!(colony::construction_materials(app.world(), actor).is_empty());
    assert!(command.check(app.world()).is_err());
    run_ticks(app.world_mut(), 640);
    let cottage = app
        .world()
        .get::<village::homestead::HomesteadPlot>(plot)
        .unwrap()
        .cottage_entity
        .unwrap();
    assert_eq!(colony::home_of(app.world(), builder), Some(cottage));
    assert_eq!(village::economy::count(app.world(), actor, "plank"), 0);
    assert_eq!(village::economy::count(app.world(), actor, "log"), 0);
    assert!(app.world().resource::<ColonyState>().projects.is_empty());
    assert_eq!(colony::summary(app.world()).beds, 27);

    // Reclaiming a completed cottage must reconcile its existing household,
    // then respect a later explicit colony reassignment.
    app.world_mut().get_mut::<Vitality>(actor).unwrap().died_at = Some(640);
    let heir = app.world().resource::<TestbedIds>().id("Ada");
    let heir_entity = app.world().resource::<SimIndex>().get(heir).unwrap();
    let former_home = colony::home_of(app.world(), heir).unwrap();
    village::VillageAct::new(heir, "stop_work")
        .perform(app.world_mut())
        .unwrap();
    app.world_mut().entity_mut(heir_entity).insert(door);
    village::homestead::HomesteadAct::new(heir, "claim_plot", plot_id)
        .perform(app.world_mut())
        .unwrap();
    assert_eq!(colony::home_of(app.world(), heir), Some(cottage));
    let cottage_entity = app.world().resource::<SimIndex>().get(cottage).unwrap();
    assert!(
        !app.world()
            .get::<Household>(cottage_entity)
            .unwrap()
            .residents
            .contains(&builder)
    );
    ColonyAct {
        verb: "assign_home".into(),
        resident: Some(heir),
        house: Some(former_home),
        value: 0,
    }
    .perform(app.world_mut())
    .unwrap();
    colony::tick(app.world_mut());
    assert_eq!(colony::home_of(app.world(), heir), Some(former_home));
}

#[test]
fn household_hearth_burns_real_fuel_at_home_once_per_heating_period() {
    let mut app = scene();
    let resident = app.world().resource::<TestbedIds>().id("Ada");
    let actor = app.world().resource::<SimIndex>().get(resident).unwrap();
    let home = colony::home_of(app.world(), resident).unwrap();
    let house = app.world().resource::<SimIndex>().get(home).unwrap();
    let door = app.world().get::<Household>(house).unwrap().door;
    assert_eq!(
        colony::household_materials(app.world(), actor),
        vec![("log", 1)]
    );
    village::economy::grant(app.world_mut(), actor, Item::new("log"));
    let before = village::economy::count(app.world(), actor, "log");
    app.world_mut().entity_mut(actor).insert(door);
    app.world_mut().resource_mut::<SimClock>().tick = 18_000;
    assert!(village::living::cold(app.world()));
    colony::tick(app.world_mut());
    assert_eq!(
        village::economy::count(app.world(), actor, "log"),
        before - 1
    );
    assert!(colony::home_heated(app.world(), actor));
    assert_eq!(app.world().get::<Household>(house).unwrap().logs_burned, 1);
    assert!(colony::household_materials(app.world(), actor).is_empty());
    app.world_mut().resource_mut::<SimClock>().tick += 20;
    colony::tick(app.world_mut());
    assert_eq!(app.world().get::<Household>(house).unwrap().logs_burned, 1);
}

#[test]
fn commissioned_house_finishes_through_funded_autonomous_production() {
    let mut app = scene();
    let initial_money = colony_currency(&app);
    let builder = app.world().resource::<TestbedIds>().id("Jun");
    let (plot_id, plot) = housing_site(&app);
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&ColonyAct {
            verb: "commission_home".into(),
            resident: Some(builder),
            house: Some(plot_id),
            value: 0,
        }),
    )
    .unwrap();
    let mut trace = Vec::new();
    for sample in 0..120 {
        run_ticks(app.world_mut(), 400);
        if sample % 6 == 5 {
            let world = app.world();
            let actor = world.resource::<SimIndex>().get(builder).unwrap();
            trace.push(serde_json::json!({"tick":world.resource::<SimClock>().tick,
                "logs":village::economy::count(world,actor,"log"),"planks":village::economy::count(world,actor,"plank"),
                "coins":world.get::<village::Wallet>(actor),"position":world.get::<SimPos>(actor),
                "intent":world.get::<village::production::JobIntent>(actor),
                "goal":world.get::<village::behavior::LifeDecision>(actor),
                "build":world.get::<village::homestead::CottageBuild>(actor),
                "procurement":world.resource::<village::production::ProductionPlan>().procurement.get(&builder),
                "production":world.resource::<village::production::ProductionPlan>().production.get(&builder),
                "clothes_price":village::market::unit_price(world,village::catalog::WARM_CLOTHES)}));
        }
        if app
            .world()
            .get::<village::homestead::HomesteadPlot>(plot)
            .unwrap()
            .cottage_built
        {
            break;
        }
    }
    assert!(
        app.world()
            .get::<village::homestead::HomesteadPlot>(plot)
            .unwrap()
            .cottage_built,
        "NPC-funded planks and logs must reach the physical building site within two days: {}; trajectory: {}",
        commissioned_house_diagnostic(&app, builder, plot_id),
        serde_json::json!(trace)
    );
    assert_eq!(colony::summary(app.world()).residents, 12);
    assert!(app.world().resource::<VillageState>().transactions > 0);
    let completed_at = app.world().resource::<SimClock>().tick;
    // Sample a complete redistribution -> procurement -> production interval.
    // SimClock denotes the next tick: a wardrobe check at exactly 96,000 would
    // precede that day's dividends and cannot establish garment liveness.
    let day = app.world().resource::<SimConfig>().day_length;
    let redistribution_at = day * 4;
    let until = redistribution_at + day;
    let mut clothing_before = None;
    while app.world().resource::<SimClock>().tick < until {
        let now = app.world().resource::<SimClock>().tick;
        let checkpoint = if now < redistribution_at {
            redistribution_at
        } else {
            until
        };
        let ticks = 1200.min(checkpoint - now);
        run_ticks(app.world_mut(), ticks);
        if app.world().resource::<SimClock>().tick == redistribution_at {
            clothing_before = Some(clothing_flow(&app));
        }
        assert_eq!(colony_currency(&app), initial_money);
        for entity in app.world().resource::<SimIndex>().map.values() {
            if let Some(person) = app.world().get::<Villager>(*entity)
                && person.role != village::Role::Visitor
            {
                let vitality = app.world().get::<Vitality>(*entity).unwrap();
                assert!(
                    vitality.died_at.is_none() && vitality.health > 0,
                    "{} died after commissioning a house",
                    person.name
                );
            }
        }
    }
    let actor = app.world().resource::<SimIndex>().get(builder).unwrap();
    let clothing_before = clothing_before.unwrap();
    let clothing_after = clothing_flow(&app);
    assert!(
        clothing_after.0 > clothing_before.0 && clothing_after.1 > clothing_before.1,
        "real garment production and trading must continue through the post-dividend recovery window: {}",
        commissioned_house_diagnostic(&app, builder, plot_id)
    );
    let warmth = village::economy::need(app.world(), actor, "Warmth");
    assert!(
        warmth
            >= app
                .world()
                .resource::<village::policy::IslandAiPolicy>()
                .critical_need,
        "the builder must recover critical warmth using real clothing or home heat: {}",
        commissioned_house_diagnostic(&app, builder, plot_id)
    );
    println!(
        "COLONY_COMMISSION_EVIDENCE {}",
        serde_json::json!({
            "completed_at":completed_at,"checked_until":until,"currency":initial_money,
            "recovery_window":[redistribution_at,until],"garments_produced":clothing_after.0-clothing_before.0,
            "garments_traded":clothing_after.1-clothing_before.1,"builder_warmth":warmth,
            "residents_alive":colony::summary(app.world()).residents,
            "builder_coated":village::living::has_coat(app.world(),actor)
        })
    );
}

fn clothing_flow(app: &App) -> (u64, u64) {
    (
        app.world()
            .resource::<VillageState>()
            .produced
            .get(village::catalog::WARM_CLOTHES)
            .copied()
            .unwrap_or(0),
        app.world()
            .resource::<village::market::MarketState>()
            .commodities
            .get(village::catalog::WARM_CLOTHES)
            .map_or(0, |commodity| commodity.total_units),
    )
}

fn colony_currency(app: &App) -> u64 {
    app.world()
        .resource::<SimIndex>()
        .map
        .values()
        .map(|entity| {
            u64::from(
                app.world()
                    .get::<village::Wallet>(*entity)
                    .map_or(0, |wallet| wallet.coins),
            ) + u64::from(
                app.world()
                    .get::<village::CoinPurse>(*entity)
                    .map_or(0, |purse| purse.coins),
            )
        })
        .sum::<u64>()
        + u64::from(app.world().resource::<VillageState>().fund)
}

fn commissioned_house_diagnostic(app: &App, builder: SimId, plot_id: SimId) -> serde_json::Value {
    let world = app.world();
    let actor = world.resource::<SimIndex>().get(builder).unwrap();
    let plot = world.resource::<SimIndex>().get(plot_id).unwrap();
    let center = village::homestead::cottage_center(*world.get::<SimPos>(plot).unwrap());
    let grid = world.resource::<SpatialGrid>();
    let footprint = grid.footprint_cells(center, village::homestead::cottage_footprint());
    let occupants: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, entity)| {
            let person = world.get::<Villager>(*entity)?;
            let position = world.get::<SimPos>(*entity)?;
            (village::living(world, *entity) && footprint.contains(&grid.cell_of(*position)))
                .then(|| serde_json::json!({"id":id,"name":person.name,"position":position}))
        })
        .collect();
    let residents:Vec<_>=world.resource::<SimIndex>().map.iter().filter_map(|(id,entity)| {
        let person=world.get::<Villager>(*entity)?;
        Some(serde_json::json!({"id":id,"name":person.name,"role":person.role,
            "coins":world.get::<village::Wallet>(*entity),"position":world.get::<SimPos>(*entity),
            "move":world.get::<MoveStatus>(*entity),"route":world.get::<village::routing::RouteRecovery>(*entity),
            "job":world.get::<village::production::JobIntent>(*entity),
            "goods":village::economy::free_items(world,*entity).iter().map(|(_,item)|(&item.kind,item.stack)).collect::<Vec<_>>(),
            "coated":village::living::has_coat(world,*entity)}))
    }).collect();
    serde_json::json!({
        "tick":world.resource::<SimClock>().tick,
        "builder":npc_framework::kernel::inspect::generic_unit(world,builder),
        "plot":world.get::<village::homestead::HomesteadPlot>(plot),
        "build_check":village::homestead::HomesteadAct::new(builder,"build_cottage",plot_id).check(world),
        "claim_check":village::homestead::HomesteadAct::new(builder,"claim_plot",plot_id).check(world),
        "construction_intent":format!("{:?}",colony::construction_intent(world,actor)),
        "free_logs":village::economy::count(world,actor,"log"),
        "free_planks":village::economy::count(world,actor,"plank"),
        "production":world.resource::<village::production::ProductionPlan>(),
        "occupants":occupants,"residents":residents,
        "clothing_market":world.resource::<village::market::MarketState>().commodities.get(village::catalog::WARM_CLOTHES)
    })
}

#[test]
fn registered_actor_can_observe_and_queue_arrival_through_the_shared_resolver() {
    use npc_framework::agent_io::{api, registration};
    use serde_json::json;
    let mut app = scene();
    let (id, actor) = visitor(&app);
    app.insert_resource(registration::RegistrationRequired);
    registration::set_credential(
        app.world_mut(),
        "colony-fixture",
        registration::Credential {
            token_digest: registration::token_digest("public-colony-test-token"),
            assigned: id,
            controller: "visitor".into(),
        },
    );
    let session = api::call(
        app.world_mut(),
        "agent/register",
        Some(json!({
            "agent_id":"colony-fixture", "token":"public-colony-test-token"
        })),
    )
    .unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    for method in ["agent/enter_world", "agent/take_control"] {
        api::call(app.world_mut(), method, Some(json!({"session":session}))).unwrap();
    }
    run_ticks(app.world_mut(), 1);
    let observed = api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"session":session})),
    )
    .unwrap();
    assert!(
        observed["affordances"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["verb"] == "join_island" && a["ok"] == true)
    );
    let result = api::call(
        app.world_mut(),
        "agent/act",
        Some(json!({"session":session,"verb":"join_island"})),
    )
    .unwrap();
    assert_eq!(result["accepted"], true, "{result}");
    assert!(
        app.world().get::<Vitality>(actor).is_none(),
        "arrival must remain queued until a simulation tick"
    );
    run_ticks(app.world_mut(), 1);
    assert!(village::living(app.world(), actor));
    assert!(app.world().get::<AwaitingArrival>(actor).is_none());
}

#[test]
fn successor_life_joins_social_and_housing_state_without_reviving_previous_life() {
    let mut app = scene();
    let (old, actor) = visitor(&app);
    JoinIsland { unit: old.0 }.perform(app.world_mut()).unwrap();
    app.world_mut().get_mut::<Vitality>(actor).unwrap().died_at = Some(0);
    village::VillageAct::new(old, "new_life")
        .perform(app.world_mut())
        .unwrap();
    let (new, successor) = visitor(&app);
    assert_ne!(old, new);
    assert!(app.world().get::<SocialLife>(successor).is_some());
    assert!(colony::home_of(app.world(), new).is_some());
    assert_eq!(app.world().get::<Vitality>(actor).unwrap().died_at, Some(0));
    assert_eq!(
        app.world().get::<village::Wallet>(successor).unwrap().coins,
        0
    );
}

#[test]
fn critical_thirst_aborts_npc_construction_and_releases_real_material_reservations() {
    emergency_construction(true);
}

#[test]
fn emergency_construction_cleanup_does_not_depend_on_optional_goap_policy() {
    emergency_construction(false);
}

fn emergency_construction(with_policy: bool) {
    let mut app = if with_policy {
        scene()
    } else {
        let mut doc = testbed_content::scenario::scenario_doc("island_colony").unwrap();
        doc.world.as_object_mut().unwrap().remove("island_ai");
        testbed_content::load_scenario(&doc, 7).unwrap().app
    };
    let builder = app.world().resource::<TestbedIds>().id("Jun");
    let actor = app.world().resource::<SimIndex>().get(builder).unwrap();
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        app.world()
            .get::<village::behavior::LifeDecision>(actor)
            .unwrap()
            .goal,
        village::behavior::LifeGoal::Work
    );
    village::VillageAct::new(builder, "stop_work")
        .perform(app.world_mut())
        .unwrap();
    let (plot_id, plot) = housing_site(&app);
    ColonyAct {
        verb: "commission_home".into(),
        resident: Some(builder),
        house: Some(plot_id),
        value: 0,
    }
    .perform(app.world_mut())
    .unwrap();
    for (kind, required) in [("plank", 8_u32), ("log", 4)] {
        let missing = required.saturating_sub(village::economy::count(app.world(), actor, kind));
        village::economy::grant(app.world_mut(), actor, Item::new(kind).stack(missing));
    }
    let door = *app.world().get::<SimPos>(plot).unwrap();
    app.world_mut().entity_mut(actor).insert(door);
    assert!(colony::construction_step(app.world_mut(), actor));
    assert!(
        app.world()
            .get::<village::homestead::CottageBuild>(actor)
            .is_some()
    );
    let thirst = app.world().resource::<NeedRegistry>().id("Thirst").unwrap();
    app.world_mut().get_mut::<Needs>(actor).unwrap().values[thirst.0 as usize].value = 14_999;
    app.world_mut().get_mut::<Needs>(actor).unwrap().values[thirst.0 as usize].level =
        npc_framework::needs::NeedLevel::Low;
    for _ in 0..40 {
        run_ticks(app.world_mut(), 1);
        if village::economy::need(app.world(), actor, "Thirst") > 90_000 {
            break;
        }
    }
    assert!(village::economy::need(app.world(), actor, "Thirst") > 90_000);
    assert!(
        app.world()
            .get::<village::homestead::CottageBuild>(actor)
            .is_none()
    );
    assert_eq!(app.world().resource::<Reservations>().count(plot_id), 0);
    assert_eq!(village::economy::count(app.world(), actor, "plank"), 8);
    assert_eq!(village::economy::count(app.world(), actor, "log"), 4);
}
