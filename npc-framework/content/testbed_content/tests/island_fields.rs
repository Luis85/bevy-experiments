//! Crop fields worked by residents: founding, sow, tend, harvest, winter
//! fallow, the finite wild grain and the planner's answer to a grain shortage.
mod life_support;

use bevy_ecs::prelude::*;
use life_support::*;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use serde_json::{Value, json};
use testbed_content::village::{
    self, ResourcePatch, Role, Season, VillageState, Villager, catalog, civic, economy as econ,
    fields::{self, FieldTask, PlotStage},
    frontier::economy_spec::EconomySpec,
    view,
};

/// Spring on the first day, and a crop that ripens within a test.
fn quick_crops(world: &mut Value) {
    economy(world)["tuning"]["founding_day"] = json!(0);
    only_weather(world, "clear");
    let mut crops = serde_json::to_value(EconomySpec::default().crops).expect("crops");
    crops[0]["growth_ticks"] = json!(1_500);
    crops[0]["tend_interval_ticks"] = json!(600);
    for field in ["sow_ticks", "tend_ticks", "harvest_ticks"] {
        crops[0][field] = json!(40);
    }
    economy(world)["crops"] = crops;
}

fn farmers(world: &World, settlement: SimId) -> Vec<Entity> {
    residents(world, settlement)
        .into_iter()
        .map(|(_, actor)| actor)
        .filter(|actor| {
            world
                .get::<Villager>(*actor)
                .is_some_and(|person| person.role == Role::Farmer)
        })
        .collect()
}

#[test]
fn every_settlement_is_founded_with_a_field_that_is_also_a_granary() {
    let mut app = world_with(|_| {});
    set_journal(app.world_mut(), true, false);
    let founders = all_residents(app.world());
    assert!(
        view::fields(app.world()).is_empty(),
        "laid out at the first tick"
    );
    run_ticks(app.world_mut(), 2);
    let all = view::fields(app.world());
    assert_eq!(all.len(), 3, "one founding field per settlement");
    let last_founder = founders.iter().map(|(id, _)| *id).max().expect("founders");
    for field in &all {
        assert!(field.id > last_founder, "fields come after every founder");
        assert_eq!(field.plots.len(), 8);
        assert!(field.plots.iter().all(|plot| plot.stage == "fallow"));
        assert_eq!(field.seed_stock, 8, "seed for every plot");
        assert_eq!(field.crop, "grain");
        assert!(field.fallow_season == (village::calendar(app.world()).1 == Season::Winter));
        let entity = entity(app.world(), field.id);
        assert!(
            civic::is_depot(app.world(), entity),
            "the field has a granary"
        );
    }
    let settlements: std::collections::BTreeSet<_> =
        all.iter().map(|field| field.settlement).collect();
    assert_eq!(settlements.len(), 3);
    assert_eq!(
        civic::buildings(app.world())
            .iter()
            .filter(|building| building.crop_plots == 8)
            .count(),
        3,
        "the building list says which buildings are fields"
    );
    // Founded, not built: no site was opened and nothing was paid.
    let state = app.world().resource::<civic::CivicState>();
    assert_eq!((state.completed, state.spent), (0, 0));
    assert_eq!(effects(app.world(), "VillageSitePlaced").len(), 0);
    // Three fields, the trading post and the harbour's market stall.
    assert_eq!(effects(app.world(), "VillageBuildingFounded").len(), 5);
    assert!(
        units(app.world_mut(), catalog::GRAIN) >= 24,
        "seed counts as grain"
    );
    village::integrity::validate_colony(app.world()).expect("valid with founding fields");
    // The founders are who they were before fields existed.
    let again = world_with(|_| {});
    assert_eq!(all_residents(again.world()), founders);
}

#[test]
fn farmers_sow_tend_and_harvest_through_the_tree() {
    let mut app = world_with(quick_crops);
    set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let produced_before = app
        .world()
        .resource::<VillageState>()
        .produced
        .get(catalog::GRAIN)
        .copied()
        .unwrap_or(0);
    // The journal is a ring buffer: read it while the run goes on.
    let mut sown = Vec::new();
    let mut harvests = Vec::new();
    let mut ripe = false;
    let mut leaves = std::collections::BTreeSet::new();
    let done = until(&mut app, 12_000, 100, |app| {
        if sown.is_empty() {
            sown = effects(app.world(), "VillageFieldSown");
        }
        if harvests.is_empty() {
            harvests = effects(app.world(), "VillageFieldHarvested");
        }
        ripe |= !effects(app.world(), "VillageFieldRipe").is_empty();
        for entry in &app.world().resource::<Journal>().entries {
            if let JournalKind::Effect {
                effect,
                applied: true,
            } = &entry.kind
                && effect.get("type").and_then(Value::as_str) == Some("VillageDecision")
                && let Some(action) = effect.get("action").and_then(Value::as_str)
                && matches!(action, "life.sow" | "life.tend" | "life.harvest")
            {
                // The field leaves start only for farmers.
                let role = entry
                    .unit
                    .and_then(|unit| app.world().resource::<SimIndex>().get(unit))
                    .and_then(|actor| app.world().get::<Villager>(actor))
                    .map(|person| person.role);
                assert_eq!(role, Some(Role::Farmer), "{action} by a non-farmer");
                leaves.insert(action.to_owned());
            }
        }
        !harvests.is_empty() && leaves.len() == 3
    });
    assert!(!sown.is_empty(), "no plot was sown in spring");
    assert_eq!(sown[0]["seed_from_field"], true);
    assert_eq!(sown[0]["crop"], "grain");
    assert!(!harvests.is_empty(), "no plot was harvested");
    let harvest = &harvests[0];
    assert_eq!(harvest["kind"], catalog::GRAIN);
    assert_eq!(harvest["quantity"], 5);
    assert_eq!(harvest["seed_kept"], 1);
    assert!(done, "the three field leaves did not all run: {leaves:?}");
    assert!(!farmers(app.world(), harbour).is_empty());
    // A harvest makes grain: six units a plot, counted as production.
    let produced = app
        .world()
        .resource::<VillageState>()
        .produced
        .get(catalog::GRAIN)
        .copied()
        .unwrap_or(0);
    assert!(produced >= produced_before + 6);
    assert!(ripe, "no plot was reported ripe");
    let plots: Vec<_> = view::fields(app.world())
        .into_iter()
        .flat_map(|field| field.plots)
        .collect();
    assert!(
        plots.iter().any(|plot| plot.stage != "fallow"),
        "fields are in use in spring"
    );
    assert!(plots.iter().all(|plot| plot.growth_permille <= 1000));
    village::integrity::validate_colony(app.world()).expect("valid while farming");
}

#[test]
fn a_ripe_plot_is_harvested_only_by_a_farmer_with_room_and_only_once() {
    let mut app = world_with(quick_crops);
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let field = view::fields(app.world())
        .into_iter()
        .find(|field| field.settlement == harbour)
        .expect("harbour field");
    let target = entity(app.world(), field.id);
    {
        let mut state = app
            .world_mut()
            .get_mut::<fields::Field>(target)
            .expect("field");
        state.plots[3].stage = PlotStage::Ripe;
        state.plots[3].growth = 1_500;
    }
    let farmer = farmers(app.world(), harbour)[0];
    let other = residents(app.world(), harbour)
        .into_iter()
        .map(|(_, actor)| actor)
        .find(|actor| {
            app.world()
                .get::<Villager>(*actor)
                .is_some_and(|person| person.role == Role::Baker)
        })
        .expect("a baker");
    assert_eq!(
        fields::target(app.world(), farmer, FieldTask::Harvest),
        Some((field.id, target, 3))
    );
    assert_eq!(fields::target(app.world(), other, FieldTask::Harvest), None);
    let before = units(app.world_mut(), catalog::GRAIN);
    let carried = econ::count(app.world(), farmer, catalog::GRAIN);
    let done = until(&mut app, 4_000, 20, |app| {
        app.world()
            .get::<fields::Field>(target)
            .is_some_and(|field| field.harvested_units >= 6)
    });
    assert!(done, "the ripe plot was not harvested");
    let state = app.world().get::<fields::Field>(target).expect("field");
    assert_eq!(state.harvested_units, 6, "one plot, one harvest");
    assert_eq!(state.plots[3].stage, PlotStage::Fallow);
    // Six units came into being: five carried off, one kept as seed.
    assert_eq!(state.seed_stock, 9);
    let _ = (before, carried);
}

#[test]
fn fields_lie_fallow_in_winter_and_sowing_waits_for_spring() {
    let mut app = world_with(|world| {
        quick_crops(world);
        short_days(world, 1200);
        economy(world)["tuning"]["days_per_season"] = json!(1);
        // The last day of autumn.
        economy(world)["tuning"]["founding_day"] = json!(2);
        // A crop that cannot ripen within the test but could before winter.
        economy(world)["crops"][0]["growth_ticks"] = json!(3_000);
    });
    set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 2);
    assert_eq!(village::calendar(app.world()).1, Season::Autumn);
    let harbour = settlement(app.world(), "harbour");
    let farmer = farmers(app.world(), harbour)[0];
    assert_eq!(
        fields::target(app.world(), farmer, FieldTask::Sow),
        None,
        "autumn is not a sowing season"
    );
    let ids: Vec<_> = view::fields(app.world())
        .into_iter()
        .map(|field| field.id)
        .collect();
    for id in &ids {
        let field = entity(app.world(), *id);
        let mut state = app
            .world_mut()
            .get_mut::<fields::Field>(field)
            .expect("field");
        state.plots[0].stage = PlotStage::Growing;
        state.plots[1].stage = PlotStage::Ripe;
    }
    run_ticks(app.world_mut(), 1400);
    let effects_now = view::season_effects(app.world());
    assert_eq!(effects_now.season, Season::Winter);
    assert!(effects_now.fallow && effects_now.growth_percent == 0);
    let fallow = effects(app.world(), "VillageFieldFallow");
    assert_eq!(fallow.len(), 3, "each field is cleared once by winter");
    assert!(fallow.iter().all(|record| record["reason"] == "winter"));
    for field in view::fields(app.world()) {
        assert!(field.fallow_season);
        assert!(field.plots.iter().all(|plot| plot.stage == "fallow"));
        assert!(field.lost_plots <= 2);
    }
    for task in [FieldTask::Sow, FieldTask::Tend, FieldTask::Harvest] {
        assert_eq!(fields::target(app.world(), farmer, task), None);
    }
    // Spring: farmers may sow again, from the seed kept in the field.
    let spring = until(&mut app, 1_400, 50, |app| {
        village::calendar(app.world()).1 == Season::Spring
    });
    assert!(spring);
    assert!(view::season_effects(app.world()).growth_percent > 0);
    assert!(
        farmers(app.world(), harbour)
            .into_iter()
            .any(|farmer| fields::target(app.world(), farmer, FieldTask::Sow).is_some()),
        "a farmer may sow again in spring"
    );
}

#[test]
fn the_wild_grain_is_a_finite_founding_reserve() {
    let mut app = world_with(|_| {});
    let patches: Vec<Entity> = {
        let mut query = app.world_mut().query::<(Entity, &ResourcePatch)>();
        query
            .iter(app.world())
            .filter(|(_, patch)| patch.kind == catalog::GRAIN)
            .map(|(entity, _)| entity)
            .collect()
    };
    assert_eq!(patches.len(), 3);
    for patch in &patches {
        let mut state = app
            .world_mut()
            .get_mut::<ResourcePatch>(*patch)
            .expect("patch");
        assert_eq!(state.capacity, 120);
        state.remaining = 0;
    }
    run_ticks(app.world_mut(), 1_300);
    for patch in &patches {
        assert_eq!(
            app.world()
                .get::<ResourcePatch>(*patch)
                .expect("patch")
                .remaining,
            0,
            "wild grain does not grow back"
        );
    }
}

#[test]
fn the_planner_commissions_a_field_on_a_lasting_grain_shortage() {
    let mut app = world_with(|world| {
        quick_crops(world);
        let tuning = &mut economy(world)["tuning"];
        tuning["planner_review_ticks"] = json!(150);
        tuning["planner_sustain_reviews"] = json!(2);
        tuning["planner_crop_shortage_units"] = json!(1);
        // Nothing ripens during the test: the land stays in use.
        economy(world)["crops"][0]["growth_ticks"] = json!(1_000_000);
        economy(world)["merchant"]["enabled"] = json!(false);
    });
    set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    fund(app.world_mut(), 700);
    // Every plot of the Harbour field is sown and the grain is gone.
    let field = view::fields(app.world())
        .into_iter()
        .find(|field| field.settlement == harbour)
        .expect("harbour field");
    let target = entity(app.world(), field.id);
    {
        let mut state = app
            .world_mut()
            .get_mut::<fields::Field>(target)
            .expect("field");
        for plot in &mut state.plots {
            plot.stage = PlotStage::Growing;
            plot.tended_at = 0;
        }
        state.seed_stock = 0;
    }
    {
        let mut query = app.world_mut().query::<&mut ResourcePatch>();
        for mut patch in query.iter_mut(app.world_mut()) {
            if patch.kind == catalog::GRAIN {
                patch.remaining = 0;
            }
        }
    }
    for (_, actor) in residents(app.world(), harbour) {
        let carried = econ::count(app.world(), actor, catalog::GRAIN);
        if carried > 0 {
            econ::consume(app.world_mut(), actor, catalog::GRAIN, carried).expect("grain");
        }
    }
    assert!(fields::land_is_short(app.world(), harbour));
    let commissioned = until(&mut app, 4_000, 50, |app| {
        effects(app.world(), "VillageCivicProject")
            .iter()
            .any(|record| record["decision"] == "commissioned" && record["building"] == "field")
    });
    assert!(
        commissioned,
        "no field was commissioned: {:?}",
        civic::planner(app.world(), harbour)
    );
    let project = effects(app.world(), "VillageCivicProject")
        .into_iter()
        .find(|record| record["decision"] == "commissioned")
        .expect("project");
    assert_eq!(project["settlement"], json!(harbour));
    assert!(
        project["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("every plot is sown"))
    );
    assert_eq!(civic::sites(app.world()).len(), 1);
    assert_eq!(civic::sites(app.world())[0].building, "field");
}

#[test]
#[ignore = "diagnostic: prints where a founding field may stand"]
fn print_founding_field_sites() {
    let mut app = world_with(|_| {});
    run_ticks(app.world_mut(), 2);
    let def = civic::definition(app.world(), "field")
        .expect("field")
        .clone();
    for key in ["harbour", "alderwood", "stone_ridge"] {
        let place = settlement(app.world(), key);
        println!(
            "FIELD_SITE {key} near_centre={:?} fields={:?}",
            civic::find_site(app.world(), &def, place, 36),
            view::fields(app.world())
                .iter()
                .filter(|field| field.settlement == place)
                .map(|field| (field.plot.x, field.plot.z))
                .collect::<Vec<_>>()
        );
    }
}
