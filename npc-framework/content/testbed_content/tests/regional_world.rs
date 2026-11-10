//! New-profile construction, ecological persistence and bounded navigation.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use testbed_content::village::{
    self, ResourcePatch, Role, Villager,
    colony::{Household, JoinIsland},
    frontier::{
        self, LivingWorldSpec, Settlement,
        ecology::{self, EcologicalPatch, EcologyWorkKind},
        routes, terrain,
    },
};

fn scene() -> App {
    testbed_content::new_sim_app(7, "living_world")
}
fn patch(world: &World, kind: &str) -> (SimId, Entity) {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .find(|(_, e)| {
            world
                .get::<ResourcePatch>(**e)
                .is_some_and(|p| p.kind == kind)
        })
        .map(|(id, e)| (*id, *e))
        .unwrap()
}

#[test]
fn regional_construction_has_three_working_settlements_and_independent_homes() {
    let app = scene();
    let world = app.world();
    assert_eq!(
        world.resource::<SpatialGrid>().bounds(),
        GridBounds::centered(256, 256)
    );
    assert_eq!(frontier::settlements::all(world).len(), 3);
    assert_eq!(
        world
            .resource::<SimIndex>()
            .map
            .values()
            .filter(|e| world
                .get::<Villager>(**e)
                .is_some_and(|v| v.role != Role::Visitor))
            .count(),
        42
    );
    assert_eq!(
        world
            .resource::<SimIndex>()
            .map
            .values()
            .filter(|e| world.get::<Household>(**e).is_some())
            .count(),
        42
    );
    for (settlement, _) in frontier::settlements::all(world) {
        let homes: Vec<_> = world
            .resource::<SimIndex>()
            .map
            .values()
            .filter(|e| frontier::settlements::member(world, **e) == Some(settlement))
            .filter_map(|e| world.get::<Household>(*e))
            .collect();
        assert_eq!(homes.len(), 14);
        assert!(homes.iter().all(|h| h.residents.len() == 1));
    }
    terrain::validate(world).unwrap();
    village::integrity::validate_colony(world).unwrap();
    assert!(
        world
            .resource::<terrain::TerrainState>()
            .cells
            .iter()
            .any(|c| c.height_mm > 4000 && c.walkable())
    );
    assert_eq!(village::homestead::plots(world).len(), 12);
}

#[test]
fn every_authored_entrance_and_resource_is_reachable_with_building_footprints() {
    let mut app = scene();
    run_ticks(app.world_mut(), 1);
    let world = app.world();
    let grid = world.resource::<SpatialGrid>();
    let arrival = frontier::spec(world).unwrap().actor_start;
    assert!(
        !grid.is_blocking(grid.cell_of(arrival)),
        "arrival must be on open ground"
    );
    let mut failures = Vec::new();
    for (id, e) in &world.resource::<SimIndex>().map {
        if world.get::<frontier::SettlementSite>(*e).is_none()
            && world.get::<ResourcePatch>(*e).is_none()
            && world.get::<Settlement>(*e).is_none()
            && world.get::<village::homestead::HomesteadPlot>(*e).is_none()
        {
            continue;
        }
        let position = *world.get::<SimPos>(*e).unwrap();
        if grid.is_blocking(grid.cell_of(position)) {
            failures.push(format!(
                "Authored entrance #{id:?} at {position:?} is blocked"
            ));
        }
        if routes::estimate(world, arrival, position).is_none() {
            failures.push(format!(
                "No bounded route to authored entrance #{id:?} at {position:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn authored_layout_rejects_edge_catchments_and_overlapping_structures() {
    let mut edge = LivingWorldSpec::default();
    edge.settlements[0].center.x = 100000;
    assert!(edge.validate().unwrap_err().contains("44 metres"));
    let mut overlap = LivingWorldSpec::default();
    overlap.settlements[0].sites[1].position = overlap.settlements[0].sites[0].position;
    assert!(overlap.validate().is_err());
    let mut outside = LivingWorldSpec::default();
    outside.settlements[0].homes[0].door.x = 127000;
    assert!(outside.validate().unwrap_err().contains("footprint"));
    let mut plot = LivingWorldSpec::default();
    let home = plot.settlements[0].homes[0].door;
    plot.plots[0].x = home.x;
    plot.plots[0].z = home.z;
    assert!(plot.validate().is_err());
}

#[test]
fn immutable_preset_and_scenario_describe_the_same_world() {
    let preset = testbed_content::worlds::builtin_preset("living_world").unwrap();
    testbed_content::worlds::validate(&preset).unwrap();
    let doc = testbed_content::scenario::scenario_doc("living_world").unwrap();
    let spec = testbed_content::scenario::WorldSpec::parse(&doc.world).unwrap();
    assert_eq!(preset.living_world, spec.living_world);
    assert_eq!(preset.living_world, Some(LivingWorldSpec::default()));
    let mut bad = LivingWorldSpec::default();
    bad.settlements[0].sites[0].role = Role::Fisher;
    assert!(bad.validate().is_err());
    let bad = LivingWorldSpec {
        actor_start: SimPos::new(900000, 0),
        ..Default::default()
    };
    assert!(bad.validate().is_err());
}

#[test]
fn old_island_keeps_its_grid_and_has_no_regional_state() {
    let mut app = testbed_content::new_sim_app(7, "island_colony");
    assert!(!frontier::enabled(app.world()));
    assert_eq!(
        app.world().resource::<SpatialGrid>().bounds(),
        GridBounds::centered(64, 64)
    );
    assert!(!app.world().contains_resource::<LivingWorldSpec>());
    assert!(!app.world().contains_resource::<terrain::TerrainState>());
    assert_eq!(
        app.world_mut()
            .query::<&Settlement>()
            .iter(app.world())
            .count(),
        0
    );
    village::integrity::validate_colony(app.world()).unwrap();
}

#[test]
fn regional_hash_and_snapshot_continue_the_same_world() {
    let mut first = scene();
    let mut second = scene();
    run_ticks(first.world_mut(), 80);
    run_ticks(second.world_mut(), 80);
    assert_eq!(
        state_hash(first.world_mut()),
        state_hash(second.world_mut())
    );
    let saved = npc_framework::snapshot::save(first.world_mut()).unwrap();
    let mut restored = scene();
    npc_framework::snapshot::load(
        restored.world_mut(),
        &saved,
        npc_framework::snapshot::LoadMode::Strict,
    )
    .unwrap();
    assert_eq!(
        state_hash(first.world_mut()),
        state_hash(restored.world_mut())
    );
    run_ticks(first.world_mut(), 60);
    run_ticks(restored.world_mut(), 60);
    assert_eq!(
        state_hash(first.world_mut()),
        state_hash(restored.world_mut())
    );
}

fn deplete(world: &mut World, kind: &str, count: u32) -> (SimId, Entity) {
    let (id, e) = patch(world, kind);
    world.get_mut::<ResourcePatch>(e).unwrap().remaining -= count;
    ecology::harvested(world, id, count);
    (id, e)
}
fn advance_ecology(world: &mut World, tick: u64) {
    world.resource_mut::<SimClock>().tick = tick;
    ecology::tick(world);
}

#[test]
fn harvested_stock_regrows_gradually_and_ore_never_refills() {
    let mut app = scene();
    let (_, logs) = deplete(app.world_mut(), "log", 100);
    let (_, ore) = deplete(app.world_mut(), "ore", 100);
    let before = app
        .world()
        .get::<EcologicalPatch>(logs)
        .unwrap()
        .biomass_milli;
    assert!(before < ecology::FULL_BIOMASS);
    advance_ecology(app.world_mut(), ecology::UPDATE_TICKS);
    let stock = app.world().get::<ResourcePatch>(logs).unwrap();
    assert!(stock.remaining > stock.capacity - 100 && stock.remaining < stock.capacity);
    assert_eq!(
        app.world()
            .get::<EcologicalPatch>(logs)
            .unwrap()
            .harvested_units,
        100
    );
    let ore = app.world().get::<ResourcePatch>(ore).unwrap();
    assert_eq!(ore.remaining, ore.capacity - 100);
    let after = state_hash(app.world_mut());
    ecology::tick(app.world_mut());
    assert_eq!(
        after,
        state_hash(app.world_mut()),
        "a repeated observation/update at the same tick cannot grow stock twice"
    );
}

#[test]
fn ecological_integrity_rejects_changed_sources_and_unearned_growth() {
    let mut app = scene();
    let (_, entity) = patch(app.world(), "log");
    let original = app.world().get::<ResourcePatch>(entity).unwrap().clone();
    {
        let mut altered = app.world_mut().get_mut::<ResourcePatch>(entity).unwrap();
        altered.capacity += 1;
        altered.remaining += 1;
    }
    assert!(
        ecology::validate(app.world()).is_err(),
        "Changing capacity and remaining together must not bypass immutable construction"
    );
    app.world_mut().entity_mut(entity).insert(original.clone());
    app.world_mut()
        .get_mut::<ResourcePatch>(entity)
        .unwrap()
        .gather_ticks += 1;
    assert!(ecology::validate(app.world()).is_err());
    app.world_mut().entity_mut(entity).insert(original.clone());
    app.world_mut()
        .get_mut::<ResourcePatch>(entity)
        .unwrap()
        .kind = "unregistered_resource".into();
    assert!(ecology::validate(app.world()).is_err());
    app.world_mut().entity_mut(entity).insert(original);
    let day = app.world().resource::<SimConfig>().day_length;
    app.world_mut()
        .get_mut::<EcologicalPatch>(entity)
        .unwrap()
        .pending_growth
        .push(ecology::GrowingCutting {
            matures_at: day,
            biomass: 5000,
        });
    assert!(
        ecology::validate(app.world()).is_err(),
        "Pending cuttings require completed planting history"
    );
    app.world_mut()
        .get_mut::<EcologicalPatch>(entity)
        .unwrap()
        .planted_units = 1;
    ecology::validate(app.world()).unwrap();
    app.world_mut()
        .get_mut::<EcologicalPatch>(entity)
        .unwrap()
        .pending_growth[0]
        .matures_at = 0;
    assert!(ecology::validate(app.world()).is_err());
}

#[test]
fn planted_cuttings_persist_and_mature_without_an_instant_stock_grant() {
    let mut app = scene();
    let (site, logs) = deplete(app.world_mut(), "log", 160);
    let actor = app.world().resource::<village::VillageState>().played;
    JoinIsland { unit: actor.0 }
        .perform(app.world_mut())
        .unwrap();
    let remaining = app.world().get::<ResourcePatch>(logs).unwrap().remaining;
    ecology::complete_work(app.world_mut(), actor, site, EcologyWorkKind::PlantTree).unwrap();
    assert_eq!(
        app.world().get::<ResourcePatch>(logs).unwrap().remaining,
        remaining
    );
    assert_eq!(
        app.world()
            .get::<EcologicalPatch>(logs)
            .unwrap()
            .pending_growth
            .len(),
        1
    );
    let snapshot = npc_framework::snapshot::save(app.world_mut()).unwrap();
    let mut restored = scene();
    npc_framework::snapshot::load(
        restored.world_mut(),
        &snapshot,
        npc_framework::snapshot::LoadMode::Strict,
    )
    .unwrap();
    // Entity values may change on load; all saved references use stable IDs.
    let loaded = restored.world().resource::<SimIndex>().get(site).unwrap();
    let initial = restored
        .world()
        .get::<EcologicalPatch>(loaded)
        .unwrap()
        .biomass_milli;
    let day = restored.world().resource::<SimConfig>().day_length;
    for tick in (ecology::UPDATE_TICKS..=day).step_by(ecology::UPDATE_TICKS as usize) {
        advance_ecology(restored.world_mut(), tick);
    }
    let state = restored.world().get::<EcologicalPatch>(loaded).unwrap();
    assert!(state.pending_growth.is_empty());
    assert_eq!(state.planted_units, 1);
    assert!(state.biomass_milli >= initial + 5000);
    assert!(state.regrown_units > 0);
}

#[test]
fn route_estimates_respect_obstacles_and_are_read_only() {
    let mut app = App::new();
    let mut grid = SpatialGrid::new(1000, GridBounds::centered(16, 16));
    for z in -8..8 {
        grid.set_static(
            CellCoord::new(0, z),
            CellInfo {
                flags: CellFlags {
                    blocking: true,
                    occluding: false,
                    tags: 0,
                },
                extra_cost: 0,
            },
        );
    }
    app.insert_resource(grid);
    let a = SimPos::new(-3500, 500);
    let b = SimPos::new(3500, 500);
    assert_eq!(routes::estimate(app.world(), a, b), None);
    app.world_mut()
        .resource_mut::<SpatialGrid>()
        .set_static(CellCoord::new(0, 6), CellInfo::default());
    let distance = routes::estimate(app.world(), a, b).unwrap().distance_mm;
    assert!(distance > 7000);
    assert_eq!(
        routes::estimate(app.world(), a, b).unwrap().distance_mm,
        distance
    );
}

#[test]
fn regional_terraform_uses_real_bounds_and_rejects_home_overlap_atomically() {
    let mut app = scene();
    let before = state_hash(app.world_mut());
    let settlement = frontier::settlements::all(app.world())[1].0;
    let home = frontier::settlements::home_def(app.world(), settlement, 0)
        .unwrap()
        .door;
    assert!(
        terrain::terraform(
            app.world_mut(),
            home.x.div_euclid(1000),
            home.z.div_euclid(1000),
            1,
            0,
            terrain::TerrainSurface::Water
        )
        .is_err()
    );
    assert_eq!(before, state_hash(app.world_mut()));
    assert!(terrain::cell(app.world(), 70, 10).is_some());
    assert!(terrain::cell(app.world(), 128, 0).is_none());
}

fn habitat_animals(world: &World, habitat: SimId) -> Vec<(SimId, Entity, SimPos)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|&(_, e)| {
            world
                .get::<village::hunting::Wildlife>(*e)
                .is_some_and(|a| a.habitat == habitat)
                && world
                    .get::<village::Vitality>(*e)
                    .is_some_and(|v| v.died_at.is_none())
        })
        .map(|(id, e)| (*id, *e, *world.get::<SimPos>(*e).unwrap()))
        .collect()
}
fn habitat_with_vacant_birth_positions(world: &mut World) -> (SimId, Entity, Vec<SimPos>) {
    let (id, entity) = world
        .resource::<SimIndex>()
        .map
        .iter()
        .find(|(_, e)| {
            world
                .get::<village::hunting::WildlifeHabitat>(**e)
                .is_some()
        })
        .map(|(id, e)| (*id, *e))
        .unwrap();
    let animals = habitat_animals(world, id);
    assert_eq!(animals.len(), 8);
    let vacant = animals[..6].iter().map(|(_, _, pos)| *pos).collect();
    for (_, animal, _) in animals.into_iter().take(6) {
        assert!(world.despawn(animal));
    }
    (id, entity, vacant)
}
fn advance_birth(world: &mut World, habitat: Entity) {
    let due = world
        .get::<ecology::HabitatEcology>(habitat)
        .unwrap()
        .next_birth;
    world.resource_mut::<SimClock>().tick = due;
    ecology::wildlife_tick(world);
}

#[test]
fn wildlife_birth_skips_flooded_cells_and_newly_completed_structures() {
    let mut app = scene();
    let (habitat, entity, vacant) = habitat_with_vacant_birth_positions(app.world_mut());
    let flooded = app.world().resource::<SpatialGrid>().cell_of(vacant[0]);
    terrain::terraform(
        app.world_mut(),
        flooded.x,
        flooded.z,
        0,
        -200,
        terrain::TerrainSurface::Water,
    )
    .unwrap();
    let blocker = GridBlocker(Footprint {
        half_x_mm: 0,
        half_z_mm: 0,
    });
    spawn_sim(app.world_mut(), (vacant[1], blocker));
    let blocked = app.world().resource::<SpatialGrid>().cell_of(vacant[1]);
    assert!(
        !app.world().resource::<SpatialGrid>().is_blocking(blocked),
        "The regression covers a structure before its derived-grid rebuild"
    );
    advance_birth(app.world_mut(), entity);
    let animals = habitat_animals(app.world(), habitat);
    assert_eq!(animals.len(), 3);
    assert!(animals.iter().any(|(_, _, at)| *at == vacant[2]));
    assert!(
        !animals
            .iter()
            .any(|(_, _, at)| *at == vacant[0] || *at == vacant[1])
    );
    assert_eq!(
        app.world()
            .get::<ecology::HabitatEcology>(entity)
            .unwrap()
            .births,
        1
    );
}

#[test]
fn wildlife_birth_defers_without_valid_land_and_resumes_after_clearance() {
    let mut app = scene();
    let (habitat, entity, vacant) = habitat_with_vacant_birth_positions(app.world_mut());
    let blockers: Vec<_> = vacant
        .iter()
        .map(|at| {
            spawn_sim(
                app.world_mut(),
                (
                    *at,
                    GridBlocker(Footprint {
                        half_x_mm: 0,
                        half_z_mm: 0,
                    }),
                ),
            )
            .1
        })
        .collect();
    let next_id = app
        .world()
        .resource::<npc_framework::kernel::ids::SimIdAllocator>()
        .next;
    advance_birth(app.world_mut(), entity);
    assert_eq!(habitat_animals(app.world(), habitat).len(), 2);
    assert_eq!(
        app.world()
            .resource::<npc_framework::kernel::ids::SimIdAllocator>()
            .next,
        next_id
    );
    let deferred = app.world().get::<ecology::HabitatEcology>(entity).unwrap();
    assert_eq!(deferred.births, 0);
    assert!(deferred.next_birth > app.world().resource::<SimClock>().tick);
    assert!(app.world_mut().despawn(blockers[0]));
    advance_birth(app.world_mut(), entity);
    assert_eq!(habitat_animals(app.world(), habitat).len(), 3);
    assert!(
        habitat_animals(app.world(), habitat)
            .iter()
            .any(|(_, _, at)| *at == vacant[0])
    );
}

#[test]
fn regional_projection_uses_authored_settlement_names_and_positions() {
    let mut spec = LivingWorldSpec::default();
    let harbour = &mut spec.settlements[0];
    harbour.label = "Dawn Harbour".into();
    harbour.center.x += 2000;
    for site in &mut harbour.sites {
        site.position.x += 2000;
    }
    for home in &mut harbour.homes {
        home.door.x += 2000;
    }
    spec.validate().unwrap();
    let mut doc = testbed_content::scenario::scenario_doc("living_world").unwrap();
    doc.world["living_world"] = serde_json::json!(spec);
    let mut app = testbed_content::scenario::load_scenario(&doc, 7)
        .unwrap()
        .app;
    let before = state_hash(app.world_mut());
    let projection = testbed_content::projection::world_state(app.world_mut()).unwrap();
    let districts = projection["terrain"]["districts"].as_array().unwrap();
    assert_eq!(districts.len(), 3);
    let harbour = districts
        .iter()
        .find(|district| district["id"] == "harbour")
        .unwrap();
    assert_eq!(harbour["label"], "Dawn Harbour");
    assert_eq!(harbour["x"], 2000);
    assert_eq!(harbour["z"], -30000);
    assert_eq!(harbour["biome"], "coast");
    assert_eq!(state_hash(app.world_mut()), before);
}
