//! Oversee and Shape controls of roadmap phase 4: profession targets,
//! village priorities, farm and garden plots and props, each a recorded
//! command with a plain refusal and a view.
mod life_support;

use bevy_ecs::prelude::*;
use island_content::village::{
    self, Role, civic,
    frontier::{
        self,
        oversee::{self, PriorityLevel, PriorityTopic, SetProfessionTarget, SetVillagePriority},
        props::{self, PlaceProp, RemoveProp},
        settlements,
        shape::{PlacePlot, RemovePlot},
    },
};
use life_support::*;
use npc_framework::kernel::commands::ContentCommand;
use npc_framework::prelude::*;
use serde_json::json;

fn refused<C: ContentCommand>(world: &World, command: &C) -> String {
    match command.validate(world) {
        Err(npc_framework::kernel::commands::CommandError::Unavailable { reason }) => reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

// --- Profession targets -----------------------------------------------------

#[test]
fn profession_targets_refuse_plainly() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let world = app.world();
    let harbour = settlement(world, "harbour");
    let reason = refused(
        world,
        &SetProfessionTarget {
            settlement: harbour,
            profession: Role::from_key("astronaut"),
            wanted: Some(2),
        },
    );
    assert!(reason.contains("not a trade"), "{reason}");
    let reason = refused(
        world,
        &SetProfessionTarget {
            settlement: harbour,
            profession: Role::Innkeeper,
            wanted: Some(2),
        },
    );
    assert!(reason.contains("build or remove one"), "{reason}");
    let reason = refused(
        world,
        &SetProfessionTarget {
            settlement: SimId(u32::MAX - 7),
            profession: Role::Miller,
            wanted: Some(2),
        },
    );
    assert!(reason.contains("not a settlement"), "{reason}");
    let reason = refused(
        world,
        &SetProfessionTarget {
            settlement: harbour,
            profession: Role::Miller,
            wanted: Some(oversee::MAX_TARGET + 1),
        },
    );
    assert!(reason.contains("at most"), "{reason}");
    // The JSON shape the UI sends.
    let command: SetProfessionTarget = serde_json::from_value(
        json!({"settlement": harbour.0, "profession": "miller", "wanted": null}),
    )
    .expect("command JSON");
    assert_eq!(command.wanted, None);
}

#[test]
fn a_raised_target_is_met_within_days_and_cleared_again() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    let day = app.world().resource::<SimConfig>().day_length;
    run_ticks(app.world_mut(), day / 4);
    let harbour = settlement(app.world(), "harbour");
    // The first trade with a free seat and somebody able to take it.
    let rows = oversee::profession_table(app.world(), harbour);
    assert!(
        rows.iter()
            .any(|row| row.key == "innkeeper" && !row.adjustable)
    );
    let row = rows
        .iter()
        .filter(|row| row.adjustable && row.free_seats > 0 && row.current >= 1)
        .find(|row| {
            frontier::labor::anyone_may_take(app.world(), harbour, Role::from_key(&row.key))
        })
        .cloned()
        .expect("a trade to raise");
    let role = Role::from_key(&row.key);
    let wanted = row.current + 1;
    SetProfessionTarget {
        settlement: harbour,
        profession: role,
        wanted: Some(wanted),
    }
    .perform(app.world_mut())
    .expect("target set");
    let set = oversee::profession_table(app.world(), harbour)
        .into_iter()
        .find(|candidate| candidate.key == row.key)
        .expect("row");
    assert_eq!(set.wanted, Some(wanted));
    assert!(set.reason.is_some(), "a gap is explained");
    let met = until(&mut app, 4 * day, day / 8, |app| {
        oversee::headcount(app.world(), harbour, role) >= wanted
    });
    let after = oversee::profession_table(app.world(), harbour)
        .into_iter()
        .find(|candidate| candidate.key == row.key)
        .expect("row");
    assert!(met, "the {} target was not met: {after:?}", row.key);
    // Cleared: the village decides again and the row shows no target.
    SetProfessionTarget {
        settlement: harbour,
        profession: role,
        wanted: None,
    }
    .perform(app.world_mut())
    .expect("cleared");
    let cleared = oversee::profession_table(app.world(), harbour)
        .into_iter()
        .find(|candidate| candidate.key == row.key)
        .expect("row");
    assert_eq!(cleared.wanted, None);
    assert_eq!(cleared.reason, None);
    village::integrity::validate_colony(app.world()).expect("valid");
}

// --- Village priorities -----------------------------------------------------

#[test]
fn village_priorities_refuse_plainly_and_read_back() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let reason = refused(
        app.world(),
        &SetVillagePriority {
            settlement: harbour,
            topic: PriorityTopic::Housing,
            profession: Some(Role::Miller),
            level: PriorityLevel::High,
        },
    );
    assert!(reason.contains("only a workplace priority"), "{reason}");
    let reason = refused(
        app.world(),
        &SetVillagePriority {
            settlement: harbour,
            topic: PriorityTopic::Workplace,
            profession: Some(Role::from_key("astronaut")),
            level: PriorityLevel::High,
        },
    );
    assert!(reason.contains("not a trade"), "{reason}");
    assert!(
        serde_json::from_value::<SetVillagePriority>(json!({
            "settlement": harbour.0, "topic": "parties", "level": "high"
        }))
        .is_err(),
        "an unknown topic is not a command"
    );
    let rows = oversee::priorities(app.world(), harbour);
    assert_eq!(rows.len(), 7);
    assert!(rows.iter().all(|row| row.level == PriorityLevel::Normal));
    let command: SetVillagePriority = serde_json::from_value(json!({
        "settlement": harbour.0, "topic": "workplace", "profession": "smith", "level": "high"
    }))
    .expect("command JSON");
    command.perform(app.world_mut()).expect("set");
    SetVillagePriority {
        settlement: harbour,
        topic: PriorityTopic::Coats,
        profession: None,
        level: PriorityLevel::Low,
    }
    .perform(app.world_mut())
    .expect("set");
    let rows = oversee::priorities(app.world(), harbour);
    assert_eq!(rows.len(), 8, "the smith's workplaces get their own row");
    let smith = rows
        .iter()
        .find(|row| row.profession.as_deref() == Some("smith"))
        .expect("smith row");
    assert!(smith.effect.starts_with("High:"), "{}", smith.effect);
    let coats = rows.iter().find(|row| row.topic == "coats").expect("coats");
    assert_eq!(coats.level, PriorityLevel::Low);
    assert!(coats.effect.contains("orders one"), "{}", coats.effect);
    // Back to normal removes the entry: the default is the plain behaviour.
    SetVillagePriority {
        settlement: harbour,
        topic: PriorityTopic::Coats,
        profession: None,
        level: PriorityLevel::Normal,
    }
    .perform(app.world_mut())
    .expect("set");
    assert_eq!(
        app.world()
            .resource::<oversee::Oversight>()
            .priorities
            .len(),
        1
    );
}

#[test]
fn a_high_food_reserve_keeps_more_food_on_the_shelves() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    let day = app.world().resource::<SimConfig>().day_length;
    run_ticks(app.world_mut(), day);
    let harbour = settlement(app.world(), "harbour");
    let foods: Vec<&'static str> = frontier::economy_spec::registry(app.world())
        .expect("registry")
        .foods()
        .to_vec();
    let targets = |world: &World| -> Vec<u32> {
        foods
            .iter()
            .map(|kind| village::stock::target(world, harbour, kind))
            .collect()
    };
    let normal = targets(app.world());
    SetVillagePriority {
        settlement: harbour,
        topic: PriorityTopic::FoodReserve,
        profession: None,
        level: PriorityLevel::High,
    }
    .perform(app.world_mut())
    .expect("set");
    let high = targets(app.world());
    assert!(
        normal.iter().zip(&high).all(|(n, h)| h >= n),
        "{normal:?} -> {high:?}"
    );
    assert!(
        normal.iter().zip(&high).any(|(n, h)| h > n),
        "some food target grows: {normal:?} -> {high:?}"
    );
    // A high priority is produced first: the plan orders more food at once.
    run_ticks(app.world_mut(), day / 2);
    village::integrity::validate_colony(app.world()).expect("valid");
}

// --- Plots and props --------------------------------------------------------

fn plot_spot(world: &World, plot: &str, settlement: SimId) -> PlacePlot {
    let centre = settlements::definition(world, settlement)
        .expect("settlement")
        .center;
    for dz in (-30..=30).step_by(2) {
        for dx in (-30..=30).step_by(2) {
            for rotation in 0..4 {
                let command = PlacePlot {
                    plot: plot.into(),
                    x: centre.x / 1000 + dx,
                    z: centre.z / 1000 + dz,
                    rotation,
                };
                if command.check(world).is_ok() {
                    return command;
                }
            }
        }
    }
    panic!("no free spot for a {plot}");
}

fn prop_spot(world: &World, prop: &str, settlement: SimId) -> PlaceProp {
    let centre = settlements::definition(world, settlement)
        .expect("settlement")
        .center;
    for dz in (-30..=30).step_by(3) {
        for dx in (-30..=30).step_by(3) {
            let command = PlaceProp {
                prop: prop.into(),
                x: centre.x / 1000 + dx,
                z: centre.z / 1000 + dz,
                rotation: 0,
            };
            if command.check(world).is_ok() {
                return command;
            }
        }
    }
    panic!("no free spot for a {prop}");
}

#[test]
fn a_garden_plot_is_placed_refused_where_it_cannot_lie_and_undone() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let reason = refused(
        app.world(),
        &PlacePlot {
            plot: "cottage".into(),
            x: 0,
            z: 0,
            rotation: 0,
        },
    );
    assert!(reason.contains("not a farm or garden plot"), "{reason}");
    let command = plot_spot(app.world(), "garden", harbour);
    let fields_before = village::fields::fields_of(app.world(), harbour).len();
    let id = command.perform(app.world_mut()).expect("placed");
    assert_eq!(
        village::fields::fields_of(app.world(), harbour).len(),
        fields_before + 1,
        "the garden is a field of the settlement"
    );
    // Not twice in one place.
    let reason = refused(app.world(), &command);
    assert!(
        reason.contains("overlaps") || reason.contains("entrance"),
        "{reason}"
    );
    // Never on water or over a road.
    let world = app.world();
    let bounds = frontier::terrain::bounds(world);
    let wet = (0..bounds.len() as u32)
        .map(|index| bounds.coord(index))
        .find(|cell| {
            frontier::terrain::cell(world, cell.x, cell.z).is_some_and(|value| !value.walkable())
        })
        .expect("a water cell");
    let reason = refused(
        world,
        &PlacePlot {
            plot: "garden".into(),
            x: wet.x,
            z: wet.z,
            rotation: 0,
        },
    );
    assert!(!reason.is_empty());
    // Undo.
    RemovePlot { plot: id }
        .perform(app.world_mut())
        .expect("removed");
    assert_eq!(
        village::fields::fields_of(app.world(), harbour).len(),
        fields_before
    );
    assert!(
        command.check(app.world()).is_ok(),
        "the ground is free again"
    );
    let reason = refused(app.world(), &RemovePlot { plot: id });
    assert!(reason.contains("no longer exists"), "{reason}");
    village::integrity::validate_colony(app.world()).expect("valid");
}

#[test]
fn props_come_from_the_catalog_block_cells_and_are_undone() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let catalog = props::catalog(app.world());
    for key in ["barrel", "fence", "flower_bed", "signpost"] {
        let def = catalog
            .iter()
            .find(|def| def.id == key)
            .expect("shipped prop");
        assert!(!def.model.is_empty(), "{key} has a model");
    }
    let reason = refused(
        app.world(),
        &PlaceProp {
            prop: "statue".into(),
            x: 0,
            z: 0,
            rotation: 0,
        },
    );
    assert!(reason.contains("not a prop"), "{reason}");
    let barrel = prop_spot(app.world(), "barrel", harbour);
    let id = barrel.perform(app.world_mut()).expect("placed");
    // The walking grid takes the new blocker at the next tick.
    run_ticks(app.world_mut(), 1);
    let views = props::props(app.world());
    let view = views.iter().find(|view| view.id == id).expect("view");
    assert!(view.solid);
    assert_eq!(view.display, "Barrel");
    let cell = CellCoord::new(view.body_min.0, view.body_min.1);
    assert!(
        app.world().resource::<SpatialGrid>().is_blocking(cell),
        "a solid prop blocks walking"
    );
    let reason = refused(app.world(), &barrel);
    assert!(
        !reason.is_empty(),
        "a second prop on the same cell is refused"
    );
    // Not on a road: the road from the arrival point to a settlement.
    let world = app.world();
    let start = frontier::spec(world).expect("spec").actor_start;
    let centre = settlements::definition(world, harbour)
        .expect("harbour")
        .center;
    let on_road = (1..20)
        .map(|step| {
            SimPos::new(
                start.x + (centre.x - start.x) * step / 20,
                start.z + (centre.z - start.z) * step / 20,
            )
        })
        .find(|p| frontier::terrain::on_road(world, *p))
        .expect("a road cell");
    let reason = refused(
        world,
        &PlaceProp {
            prop: "flower_bed".into(),
            x: on_road.x.div_euclid(1000),
            z: on_road.z.div_euclid(1000),
            rotation: 0,
        },
    );
    assert!(!reason.is_empty());
    // Undo.
    RemoveProp { prop: id }
        .perform(app.world_mut())
        .expect("removed");
    run_ticks(app.world_mut(), 1);
    assert!(props::props(app.world()).is_empty());
    assert!(!app.world().resource::<SpatialGrid>().is_blocking(cell));
    assert!(barrel.check(app.world()).is_ok(), "the cell is free again");
}

#[test]
fn plots_and_props_are_exported_and_built_again() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    plot_spot(app.world(), "garden", harbour)
        .perform(app.world_mut())
        .expect("garden");
    prop_spot(app.world(), "barrel", harbour)
        .perform(app.world_mut())
        .expect("barrel");
    prop_spot(app.world(), "flower_bed", harbour)
        .perform(app.world_mut())
        .expect("flower bed");
    let text = frontier::export_world(app.world());
    let exported: serde_json::Value = serde_json::from_str(&text).expect("world JSON");
    let living = &exported["living_world"];
    assert_eq!(living["props"].as_array().map(Vec::len), Some(2));
    assert!(
        living["buildings"]
            .as_array()
            .is_some_and(|b| b.iter().any(|b| b["building"] == "garden"))
    );
    let mut doc = island_content::scenario::scenario_doc("living_world").expect("scenario");
    doc.world["construction"]["world"]["preset"]["living_world"] = living.clone();
    only_weather(&mut doc.world, "clear");
    let mut again = island_content::scenario::load_scenario(&doc, SEED)
        .unwrap_or_else(|error| panic!("{error}"))
        .app;
    run_ticks(again.world_mut(), 2);
    let shape = |world: &World| {
        props::props(world)
            .into_iter()
            .map(|view| (view.prop, view.x, view.z, view.rotation))
            .collect::<Vec<_>>()
    };
    assert_eq!(shape(app.world()), shape(again.world()), "the props match");
    let gardens = |world: &World| {
        civic::buildings(world)
            .into_iter()
            .filter(|building| building.building == "garden")
            .count()
    };
    assert_eq!(gardens(again.world()), 1, "the garden is laid out again");
    village::integrity::validate_colony(again.world()).expect("valid");
}

/// `check` results of every cell around a spot, for "as if never placed".
fn neighbourhood<C: ContentCommand + Clone>(
    world: &World,
    command: &C,
    at: impl Fn(&C, i32, i32) -> C,
) -> Vec<Result<(), String>> {
    (-6..=6)
        .flat_map(|dz| (-6..=6).map(move |dx| (dx, dz)))
        .map(|(dx, dz)| {
            at(command, dx, dz)
                .validate(world)
                .map_err(|error| format!("{error:?}"))
        })
        .collect()
}

#[test]
fn a_removed_plot_or_prop_leaves_nothing_behind() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let shift_plot = |c: &PlacePlot, dx: i32, dz: i32| PlacePlot {
        x: c.x + dx,
        z: c.z + dz,
        ..c.clone()
    };
    for plot in ["field", "garden"] {
        let command = plot_spot(app.world(), plot, harbour);
        let before = neighbourhood(app.world(), &command, shift_plot);
        let id = command.perform(app.world_mut()).expect("placed");
        run_ticks(app.world_mut(), 20);
        RemovePlot { plot: id }
            .perform(app.world_mut())
            .expect("removed");
        run_ticks(app.world_mut(), 20);
        assert_eq!(
            neighbourhood(app.world(), &command, shift_plot),
            before,
            "the {plot} left something behind"
        );
        command
            .perform(app.world_mut())
            .unwrap_or_else(|reason| panic!("the {plot} cannot be put back: {reason}"));
    }
    let shift_prop = |c: &PlaceProp, dx: i32, dz: i32| PlaceProp {
        x: c.x + dx,
        z: c.z + dz,
        ..c.clone()
    };
    let barrel = prop_spot(app.world(), "barrel", harbour);
    let before = neighbourhood(app.world(), &barrel, shift_prop);
    let id = barrel.perform(app.world_mut()).expect("placed");
    run_ticks(app.world_mut(), 20);
    RemoveProp { prop: id }
        .perform(app.world_mut())
        .expect("removed");
    run_ticks(app.world_mut(), 20);
    assert_eq!(
        neighbourhood(app.world(), &barrel, shift_prop),
        before,
        "the barrel left something behind"
    );
    barrel
        .perform(app.world_mut())
        .unwrap_or_else(|reason| panic!("the barrel cannot be put back: {reason}"));
}
