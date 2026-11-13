//! `TerraformCells`: one recorded, atomic command for a painted terrain stroke.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::kernel::commands::{CommandRegistry, SimCommand, submit};
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use serde_json::json;
use testbed_content::village::{
    frontier::{
        self,
        terrain::{self, TerraformCell, TerraformCells, TerrainState, TerrainSurface},
    },
    terrain::Terraform,
};

fn scene() -> App {
    testbed_content::new_sim_app(7, "living_world")
}

/// A short row of open ground well away from homes, sites and residents.
fn open_row(world: &World, length: i32) -> Vec<(i32, i32)> {
    let probe = |x: i32, z: i32| {
        (0..length).all(|step| {
            terrain::cell(world, x + step, z).is_some_and(|cell| {
                cell.walkable()
                    && Terraform {
                        x: x + step,
                        z,
                        radius: 0,
                        height_mm: cell.height_mm + 100,
                        surface: TerrainSurface::Soil,
                    }
                    .check(world)
                    .is_ok()
            })
        })
    };
    for z in 0..100 {
        for x in 0..100 {
            if probe(x, z) {
                return (0..length).map(|step| (x + step, z)).collect();
            }
        }
    }
    panic!("no editable row of {length} cells");
}

fn raised(world: &World, row: &[(i32, i32)]) -> Vec<TerraformCell> {
    row.iter()
        .enumerate()
        .map(|(index, (x, z))| TerraformCell {
            x: *x,
            z: *z,
            // Each cell gets its own value: a batch is not one flat brush.
            height_mm: terrain::cell(world, *x, *z).expect("cell").height_mm
                + 100
                + 50 * index as i32,
            surface: TerrainSurface::Soil,
        })
        .collect()
}

#[test]
fn a_batch_equals_the_same_single_cell_edits_with_one_revision() {
    let mut batch = scene();
    let mut single = scene();
    let row = open_row(batch.world(), 6);
    let cells = raised(batch.world(), &row);
    let revision = batch.world().resource::<TerrainState>().revision;
    TerraformCells {
        cells: cells.clone(),
    }
    .perform(batch.world_mut())
    .expect("batch applies");
    for cell in &cells {
        Terraform {
            x: cell.x,
            z: cell.z,
            radius: 0,
            height_mm: cell.height_mm,
            surface: cell.surface,
        }
        .perform(single.world_mut())
        .expect("single cell applies");
    }
    assert_eq!(
        batch.world().resource::<TerrainState>().cells,
        single.world().resource::<TerrainState>().cells
    );
    assert_eq!(
        batch.world().resource::<TerrainState>().revision,
        revision + 1,
        "a batch is one terrain revision"
    );
    assert_eq!(
        single.world().resource::<TerrainState>().revision,
        revision + cells.len() as u64
    );
    for cell in &cells {
        let now = terrain::cell(batch.world(), cell.x, cell.z).expect("cell");
        assert_eq!((now.height_mm, now.surface), (cell.height_mm, cell.surface));
    }
    frontier::terrain::validate(batch.world()).expect("terrain and navigation agree");
}

#[test]
fn one_refused_cell_leaves_the_whole_batch_unapplied() {
    let mut app = scene();
    let row = open_row(app.world(), 4);
    let mut cells = raised(app.world(), &row);
    let before = state_hash(app.world_mut());
    // The last cell floods a home entrance, which no brush may do.
    let settlement = frontier::settlements::all(app.world())[1].0;
    let home = frontier::settlements::home_def(app.world(), settlement, 0)
        .expect("home")
        .door;
    cells.push(TerraformCell {
        x: home.x.div_euclid(1000),
        z: home.z.div_euclid(1000),
        height_mm: 0,
        surface: TerrainSurface::Water,
    });
    let command = TerraformCells { cells };
    let reason = command.perform(app.world_mut()).expect_err("home overlap");
    assert!(reason.contains("overlaps"), "{reason}");
    assert_eq!(before, state_hash(app.world_mut()), "nothing was applied");

    for (cells, expected) in [
        (Vec::new(), "needs 1..=4096 cells"),
        (
            vec![
                TerraformCell {
                    x: row[0].0,
                    z: row[0].1,
                    height_mm: 300,
                    surface: TerrainSurface::Soil,
                };
                2
            ],
            "listed twice",
        ),
        (
            vec![TerraformCell {
                x: 4000,
                z: 0,
                height_mm: 300,
                surface: TerrainSurface::Soil,
            }],
            "outside the regional map",
        ),
        (
            vec![TerraformCell {
                x: row[0].0,
                z: row[0].1,
                height_mm: 300,
                surface: TerrainSurface::Water,
            }],
            "Dry terrain must be",
        ),
    ] {
        let reason = TerraformCells { cells }
            .perform(app.world_mut())
            .expect_err(expected);
        assert!(reason.contains(expected), "{reason}");
    }
    assert_eq!(before, state_hash(app.world_mut()));
}

#[test]
fn the_batch_is_one_recorded_command_and_only_the_regional_profile_has_it() {
    let mut app = scene();
    let row = open_row(app.world(), 3);
    let cells = raised(app.world(), &row);
    let payload = json!({"cells": cells});
    let parsed: TerraformCells = serde_json::from_value(payload).expect("JSON shape");
    assert!(
        serde_json::from_value::<TerraformCells>(json!({"cells": [], "radius": 1})).is_err(),
        "unknown fields are refused"
    );
    let revision = app.world().resource::<TerrainState>().revision;
    submit(app.world_mut(), None, SimCommand::content(&parsed)).expect("accepted");
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        app.world().resource::<TerrainState>().revision,
        revision + 1
    );
    let first = terrain::cell(app.world(), cells[0].x, cells[0].z).expect("cell");
    assert_eq!(first.height_mm, cells[0].height_mm);
    // The single-brush command keeps its JSON shape.
    let brush: Terraform = serde_json::from_value(
        json!({"x": row[0].0, "z": row[0].1, "radius": 0, "height_mm": cells[0].height_mm, "surface": "soil"}),
    )
    .expect("existing Terraform shape");
    brush.check(app.world()).expect("still valid");
    // Older island profiles keep their command set, and so their content hash.
    assert!(
        app.world()
            .resource::<CommandRegistry>()
            .get("TerraformCells")
            .is_some()
    );
    let old = testbed_content::new_sim_app(7, "island_colony");
    assert!(
        old.world()
            .resource::<CommandRegistry>()
            .get("Terraform")
            .is_some()
    );
    assert!(
        old.world()
            .resource::<CommandRegistry>()
            .get("TerraformCells")
            .is_none()
    );
}
