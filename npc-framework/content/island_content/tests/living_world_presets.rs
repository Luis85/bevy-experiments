use island_content::worlds::{self, WorldScene};
use serde_json::json;

#[test]
fn unsupported_layouts_reject_needs_driven_mode_instead_of_silently_ignoring_it() {
    let mut preset = worlds::builtin_preset("living_world").unwrap();
    preset.scene = WorldScene::Explorer;
    assert!(worlds::validate(&preset).is_err());
    assert!(
        island_content::scenario::WorldSpec::parse(
            &json!({"layout":"open","economy":"needs_driven","furnish":false})
        )
        .is_err()
    );
}
