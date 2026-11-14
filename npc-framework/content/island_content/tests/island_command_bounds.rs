//! Commanded cells far outside the island are refused with a sentence, never
//! an overflow (review fix S1.1), and bad world rules refuse at load (S1.4).
use bevy_ecs::prelude::*;
use island_content::village::civic::PlaceBuilding;
use island_content::village::frontier::props::PlaceProp;
use island_content::village::frontier::shape::PlaceMode;
use island_content::village::frontier::shape::PlacePlot;
use island_content::village::frontier::terrain::{TerraformCell, TerraformCells, TerrainSurface};
use npc_framework::prelude::*;

const OUTSIDE: &str = "That spot is outside the island.";

#[test]
fn place_commands_refuse_extreme_cells() {
    let mut app = island_content::new_sim_app(7, "living_world");
    run_ticks(app.world_mut(), 2);
    let world: &World = app.world();
    for (x, z) in [
        (i32::MAX, 0),
        (0, i32::MAX),
        (i32::MIN, 0),
        (0, i32::MIN),
        (i32::MAX, i32::MIN),
    ] {
        for rotation in 0..4 {
            let building = PlaceBuilding {
                building: "cottage".into(),
                x,
                z,
                rotation,
            };
            assert_eq!(building.check(world), Err(OUTSIDE.into()), "{x},{z}");
            let plot = PlacePlot {
                plot: "garden".into(),
                x,
                z,
                rotation,
                mode: PlaceMode::Build,
            };
            assert_eq!(plot.check(world), Err(OUTSIDE.into()), "{x},{z}");
            let prop = PlaceProp {
                prop: "barrel".into(),
                x,
                z,
                rotation,
                mode: PlaceMode::Build,
            };
            assert_eq!(prop.check(world), Err(OUTSIDE.into()), "{x},{z}");
        }
        let cells = TerraformCells {
            cells: vec![TerraformCell {
                x,
                z,
                height_mm: 1000,
                surface: TerrainSurface::Grass,
            }],
        };
        assert!(cells.check(world).is_err());
        assert!(
            island_content::village::civic::placement_plot(world, "cottage", x, z, 0).is_none()
        );
    }
}

fn load_with_preset(edit: impl FnOnce(&mut serde_json::Value)) -> Result<(), String> {
    let mut doc = island_content::scenario::scenario_doc("living_world")?;
    edit(&mut doc.world["construction"]["world"]["preset"]);
    island_content::scenario::load_scenario(&doc, 7).map(|_| ())
}

#[test]
fn zero_low_threshold_is_refused() {
    let error = load_with_preset(|preset| {
        preset["needs"]["hunger"]["low_milli"] = serde_json::json!(0);
    })
    .expect_err("refused");
    assert!(error.contains("low_milli"), "{error}");
}

#[test]
fn recipe_loop_is_refused_not_a_panic() {
    let error = load_with_preset(|preset| {
        let recipes = preset["living_world"]["economy"]["recipes"]
            .as_array_mut()
            .expect("recipes");
        // Planks back into logs: a loop through `log`.
        let saw = recipes
            .iter()
            .find(|recipe| recipe["key"] == "village_saw")
            .cloned()
            .expect("saw");
        let mut back = saw.clone();
        back["key"] = serde_json::json!("unsaw");
        back["inputs"] = saw["outputs"].clone();
        back["outputs"] = saw["inputs"].clone();
        recipes.push(back);
    })
    .expect_err("refused");
    assert!(error.contains("economy"), "{error}");
}

#[test]
fn world_without_actor_is_refused() {
    let mut doc = island_content::scenario::scenario_doc("living_world").expect("scenario");
    doc.actors.clear();
    let error = island_content::scenario::load_scenario(&doc, 7)
        .err()
        .expect("refused");
    assert_eq!(error, "This world has no character to play.");
}
