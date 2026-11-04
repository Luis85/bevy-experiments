use npc_framework::prelude::*;
use testbed_content::{projection, village};

#[test]
fn expanded_export_uses_the_same_land_roads_and_plots_as_navigation() {
    let mut app = testbed_content::new_sim_app(7, "living_village");
    let geography = *village::geometry::for_world(app.world());
    let before = state_hash(app.world_mut());
    let view = projection::world_state(app.world_mut()).unwrap();
    assert_eq!(state_hash(app.world_mut()), before);
    assert_eq!(
        view["terrain"]["paths"],
        serde_json::to_value(geography.roads).unwrap()
    );
    assert_eq!(
        view["terrain"]["expansion_plots"].as_array().unwrap().len(),
        6
    );
    assert_eq!(view["terrain"]["districts"].as_array().unwrap().len(), 3);
    let cells = view["terrain"]["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 4096);
    for z in -32..32 {
        for x in -32..32 {
            let index = ((z + 32) * 64 + x + 32) as usize;
            assert_eq!(cells[index], u8::from(geography.land_cell(x, z)));
        }
    }
    assert_eq!(cells.iter().filter(|cell| **cell == 1).count(), 2360);
}

#[test]
fn compact_export_retains_original_land_without_expansion_markers() {
    let mut app = testbed_content::new_sim_app(7, "ai_village");
    let view = projection::world_state(app.world_mut()).unwrap();
    assert!(view["terrain"].get("expansion_plots").is_none());
    assert!(view["terrain"].get("districts").is_none());
    assert_eq!(view["terrain"]["paths"].as_array().unwrap().len(), 4);
    assert_eq!(
        view["terrain"]["cells"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|cell| **cell == 1)
            .count(),
        1516
    );
}
