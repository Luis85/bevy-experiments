//! Shape placements are built, not conjured (review fix S2.1): in build mode
//! a plot or a prop opens a construction site with its catalog cost and
//! work; free placement needs the recorded editor switch.
mod life_support;

use bevy_ecs::prelude::*;
use island_content::village::{
    self, civic,
    frontier::{
        props::{self, PlaceProp, RemoveProp},
        settlements,
        shape::{PlaceMode, PlacePlot, SetEditorMode},
    },
};
use life_support::*;
use npc_framework::kernel::commands::{CommandError, ContentCommand};
use npc_framework::prelude::*;

fn refused<C: ContentCommand>(world: &World, command: &C) -> String {
    match command.validate(world) {
        Err(CommandError::Unavailable { reason }) => reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn spots(world: &World, settlement: SimId) -> Vec<(i32, i32)> {
    let centre = settlements::definition(world, settlement)
        .expect("settlement")
        .center;
    (-30..=30)
        .step_by(3)
        .flat_map(|dz| (-30..=30).step_by(3).map(move |dx| (dx, dz)))
        .map(|(dx, dz)| (centre.x / 1000 + dx, centre.z / 1000 + dz))
        .collect()
}

fn prop_at(prop: &str, (x, z): (i32, i32), mode: PlaceMode) -> PlaceProp {
    PlaceProp {
        prop: prop.into(),
        x,
        z,
        rotation: 0,
        mode,
    }
}

fn plot_at(plot: &str, (x, z): (i32, i32), rotation: u8, mode: PlaceMode) -> PlacePlot {
    PlacePlot {
        plot: plot.into(),
        x,
        z,
        rotation,
        mode,
    }
}

#[test]
fn editor_placement_is_refused_in_play_and_works_in_editor_mode() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let spot = spots(app.world(), harbour)
        .into_iter()
        .find(|spot| {
            prop_at("barrel", *spot, PlaceMode::Build)
                .check(app.world())
                .is_ok()
        })
        .expect("a spot");
    let editor = prop_at("barrel", spot, PlaceMode::Editor);
    assert_eq!(
        refused(app.world(), &editor),
        "Free placement is only available while editing the world."
    );
    SetEditorMode { on: true }.apply(app.world_mut());
    let id = editor.perform(app.world_mut()).expect("placed at once");
    assert!(props::props(app.world()).iter().any(|view| view.id == id));
    // The JSON the interface sends.
    let command: PlaceProp = serde_json::from_value(serde_json::json!({
        "prop": "barrel", "x": spot.0, "z": spot.1, "rotation": 0, "mode": "build"
    }))
    .expect("command JSON");
    assert_eq!(command.mode, PlaceMode::Build);
    assert!(
        serde_json::from_value::<PlaceProp>(serde_json::json!({
            "prop": "barrel", "x": 0, "z": 0
        }))
        .is_err(),
        "the mode is required"
    );
}

#[test]
fn a_prop_is_built_through_a_site_and_its_site_can_be_cancelled() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    fund(app.world_mut(), 500);
    let harbour = settlement(app.world(), "harbour");
    let all = spots(app.world(), harbour);
    let spot = *all
        .iter()
        .find(|spot| {
            prop_at("barrel", **spot, PlaceMode::Build)
                .check(app.world())
                .is_ok()
        })
        .expect("a spot");
    let site = prop_at("barrel", spot, PlaceMode::Build)
        .perform(app.world_mut())
        .expect("site");
    let view = civic::sites(app.world())
        .into_iter()
        .find(|view| view.id == site)
        .expect("the prop site is a site");
    assert_eq!(view.kind, "prop");
    assert_eq!(view.building, "barrel");
    assert_eq!(view.cost.get("plank"), Some(&1));
    assert_eq!(view.work_ticks, 60);
    assert!(props::props(app.world()).is_empty(), "nothing stands yet");
    // Built by the village: it stands once supplied and worked.
    let built = until(&mut app, 24_000, 100, |app| {
        !props::props(app.world()).is_empty()
    });
    assert!(built, "the barrel was never built");
    assert!(!effects(app.world(), "LivingPropBuilt").is_empty() || built);
    // A second one, cancelled while it is still a site.
    let other = *all
        .iter()
        .find(|spot| {
            prop_at("barrel", **spot, PlaceMode::Build)
                .check(app.world())
                .is_ok()
        })
        .expect("another spot");
    let site = prop_at("barrel", other, PlaceMode::Build)
        .perform(app.world_mut())
        .expect("site");
    RemoveProp { prop: site }
        .perform(app.world_mut())
        .expect("cancelled");
    assert!(civic::sites(app.world()).iter().all(|view| view.id != site));
    village::integrity::validate_colony(app.world()).expect("valid");
}

#[test]
fn prop_sites_have_their_own_allowance() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let mut placed = 0;
    let mut cap = None;
    for spot in spots(app.world(), harbour) {
        let command = prop_at("barrel", spot, PlaceMode::Build);
        match command.check(app.world()) {
            Ok(()) => {
                command.perform(app.world_mut()).expect("site");
                placed += 1;
            }
            Err(reason) if reason.starts_with("Eight small things") => {
                cap = Some(reason);
                break;
            }
            Err(_) => {}
        }
    }
    assert_eq!(placed, civic::MAX_OPEN_PROP_SITES);
    assert!(cap.is_some(), "the ninth prop site is refused");
}

#[test]
fn a_placed_garden_waits_for_its_logs_and_a_builder() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    fund(app.world_mut(), 500);
    let harbour = settlement(app.world(), "harbour");
    let fields_before = village::fields::fields_of(app.world(), harbour).len();
    let command = spots(app.world(), harbour)
        .into_iter()
        .flat_map(|spot| (0..4).map(move |rotation| (spot, rotation)))
        .map(|(spot, rotation)| plot_at("garden", spot, rotation, PlaceMode::Build))
        .find(|command| command.check(app.world()).is_ok())
        .expect("a spot for a garden");
    let site = command.perform(app.world_mut()).expect("site");
    let view = civic::sites(app.world())
        .into_iter()
        .find(|view| view.id == site)
        .expect("a site");
    assert_eq!(view.building, "garden");
    assert_eq!(view.cost.get("log"), Some(&2));
    assert_eq!(view.work_ticks, 200);
    assert_eq!(
        village::fields::fields_of(app.world(), harbour).len(),
        fields_before,
        "nothing grows until it is built"
    );
    let built = until(&mut app, 3 * 24_000, 200, |app| {
        village::fields::fields_of(app.world(), harbour).len() > fields_before
    });
    assert!(built, "the garden was never built");
    village::integrity::validate_colony(app.world()).expect("valid");
}
