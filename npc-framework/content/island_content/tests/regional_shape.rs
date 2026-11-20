//! Shaping the living world: resource patches placed and removed by recorded
//! command, several patches of one good serving a settlement, and the world
//! exported as world JSON and built again.
mod life_support;

use bevy_ecs::prelude::*;
use island_content::village::{
    self, ResourcePatch, civic,
    frontier::{self, PlaceResourcePatch, RemoveResourcePatch, settlements, terrain::TerrainState},
};
use life_support::*;
use npc_framework::prelude::*;
use serde_json::json;

/// `(good, x, z, capacity, settlement)` of every patch, sorted.
fn patches(world: &World) -> Vec<(String, i32, i32, u32, SimId)> {
    let mut out: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .values()
        .filter_map(|entity| {
            let patch = world.get::<ResourcePatch>(*entity)?;
            let pos = world.get::<SimPos>(*entity)?;
            Some((
                patch.kind.clone(),
                pos.x,
                pos.z,
                patch.capacity,
                settlements::member(world, *entity)?,
            ))
        })
        .collect();
    out.sort();
    out
}

/// `(building, x, z, rotation, finished)` of every building and site placed
/// after the founding, sorted.
fn placed(world: &World) -> Vec<(String, i32, i32, u8, bool)> {
    let founded = world.resource::<civic::CivicState>().founded.clone();
    let mut out: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, entity)| {
            if let Some(building) = world.get::<civic::Building>(*entity) {
                return (!founded.contains(id)).then(|| {
                    (
                        building.building.clone(),
                        building.plot.x,
                        building.plot.z,
                        building.plot.rotation,
                        true,
                    )
                });
            }
            let site = world.get::<civic::ConstructionSite>(*entity)?;
            Some((
                site.building.clone(),
                site.plot.x,
                site.plot.z,
                site.plot.rotation,
                false,
            ))
        })
        .collect();
    out.sort();
    out
}

/// A free spot for a patch of `good` near the centre of `settlement`.
fn spot(world: &World, good: &str, settlement: SimId) -> PlaceResourcePatch {
    let centre = settlements::definition(world, settlement)
        .expect("settlement")
        .center;
    for dz in (-30..=30).step_by(3) {
        for dx in (-30..=30).step_by(3) {
            let command = PlaceResourcePatch {
                good: good.into(),
                x: centre.x + dx * 1000 + 500,
                z: centre.z + dz * 1000 + 500,
                richness: 150,
            };
            if command.check(world).is_ok() {
                return command;
            }
        }
    }
    panic!("no free spot for a {good} patch");
}

#[test]
fn resource_patches_are_placed_and_removed_and_a_settlement_keeps_one() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let berries = village::catalog::BERRIES;
    let before = frontier::shape::patches_of(app.world(), harbour, berries);
    assert_eq!(before.len(), 1, "one authored berry patch");
    // Rejections the interface shows.
    let mut command = spot(app.world(), berries, harbour);
    let rich = PlaceResourcePatch {
        richness: 10,
        ..command.clone()
    };
    assert!(rich.check(app.world()).unwrap_err().contains("richness"));
    let unknown = PlaceResourcePatch {
        good: "plank".into(),
        ..command.clone()
    };
    assert!(
        unknown
            .check(app.world())
            .unwrap_err()
            .contains("does not grow in patches")
    );
    let far = PlaceResourcePatch {
        x: 0,
        z: -120_000,
        ..command.clone()
    };
    assert!(far.check(app.world()).is_err());
    // A second berry patch, recorded.
    submit(app.world_mut(), None, SimCommand::content(&command)).expect("accepted");
    run_ticks(app.world_mut(), 1);
    let after = frontier::shape::patches_of(app.world(), harbour, berries);
    assert_eq!(after.len(), 2);
    let new = after
        .iter()
        .find(|(id, _)| !before.contains(&(*id, before[0].1)))
        .expect("the new patch")
        .1;
    let stock = app.world().get::<ResourcePatch>(new).expect("patch");
    assert_eq!(stock.capacity, stock.remaining);
    assert_eq!(stock.capacity, 160 * 150 / 100);
    village::integrity::validate_colony(app.world()).expect("two berry patches are valid");
    // The authored patch goes; the new one is now the last and stays.
    let remove = RemoveResourcePatch { patch: before[0].0 };
    remove.perform(app.world_mut()).expect("removed");
    let last = frontier::shape::patches_of(app.world(), harbour, berries);
    assert_eq!(last.len(), 1);
    let refused = RemoveResourcePatch { patch: last[0].0 };
    assert!(refused.check(app.world()).unwrap_err().contains("last"));
    // Gatherers use the patch that is left.
    run_ticks(app.world_mut(), 6_000);
    village::integrity::validate_colony(app.world()).expect("valid after removal");
    // Command JSON as the host sends it.
    command.x += 3_000;
    let parsed: PlaceResourcePatch = serde_json::from_value(json!({
        "good": berries, "x": command.x, "z": command.z, "richness": 150
    }))
    .expect("JSON");
    assert_eq!(parsed, command);
    let parsed: RemoveResourcePatch = serde_json::from_value(json!({"patch": 12})).expect("JSON");
    assert_eq!(parsed.patch, SimId(12));
}

#[test]
fn an_exported_world_builds_again_with_its_terrain_patches_and_buildings() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    // Shape it: terrain, a patch, a finished building and an open site.
    let centre = settlements::definition(app.world(), harbour)
        .expect("harbour")
        .center;
    let terraform = (0..40)
        .flat_map(|dz| (0..40).map(move |dx| (dx, dz)))
        .map(|(dx, dz)| frontier::terrain::TerraformCells {
            cells: vec![frontier::terrain::TerraformCell {
                x: centre.x / 1000 + 10 + dx,
                z: centre.z / 1000 - 10 - dz,
                height_mm: 900,
                surface: frontier::terrain::TerrainSurface::Stone,
            }],
        })
        .find(|command| command.check(app.world()).is_ok())
        .expect("a cell to terraform");
    terraform.perform(app.world_mut()).expect("terraformed");
    spot(app.world(), village::catalog::FIBER, harbour)
        .perform(app.world_mut())
        .expect("patch");
    let stockpile = civic::definition(app.world(), "stockpile")
        .expect("stockpile")
        .clone();
    let (x, z, rotation) = civic::find_site(app.world(), &stockpile, harbour, 36).expect("site");
    civic::PlaceBuilding {
        building: "stockpile".into(),
        x,
        z,
        rotation,
    }
    .perform(app.world_mut())
    .expect("placed");
    run_ticks(app.world_mut(), 2);

    let text = frontier::export_world(app.world());
    let exported: serde_json::Value = serde_json::from_str(&text).expect("world JSON");
    let living = &exported["living_world"];
    assert!(living["terrain"]["heights"].is_array());
    assert!(
        living["resource_patches"]
            .as_array()
            .is_some_and(|p| !p.is_empty())
    );
    assert!(
        living["buildings"]
            .as_array()
            .is_some_and(|b| !b.is_empty())
    );

    let mut doc = island_content::scenario::scenario_doc("living_world").expect("scenario");
    doc.world["construction"]["world"]["preset"]["living_world"] = living.clone();
    only_weather(&mut doc.world, "clear");
    let mut again = island_content::scenario::load_scenario(&doc, SEED)
        .unwrap_or_else(|error| panic!("{error}"))
        .app;
    run_ticks(again.world_mut(), 2);
    let (a, b) = (app.world(), again.world());
    let cells = |world: &World| {
        world
            .resource::<TerrainState>()
            .cells
            .iter()
            .map(|cell| (cell.height_mm, cell.surface))
            .collect::<Vec<_>>()
    };
    assert_eq!(cells(a), cells(b), "the terrain matches");
    assert_eq!(patches(a), patches(b), "the resource patches match");
    assert_eq!(placed(a), placed(b), "the buildings and sites match");
    assert!(!placed(b).is_empty());
    village::integrity::validate_colony(b).expect("the rebuilt world is valid");
}
