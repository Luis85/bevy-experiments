use serde_json::json;
use testbed_content::worlds::{self, EconomyModel, WorldScene};

#[test]
fn living_mode_is_explicit_and_old_presets_keep_their_serialized_contract() {
    let fixed = worlds::builtin_preset("island_village").unwrap();
    assert_eq!(fixed.economy, EconomyModel::Fixed);
    assert!(
        serde_json::to_value(&fixed)
            .unwrap()
            .get("economy")
            .is_none()
    );
    let living = worlds::builtin_preset("living_island").unwrap();
    assert!(worlds::validate(&living).is_ok());
    let stored = serde_json::to_value(&living).unwrap();
    assert_eq!(stored["economy"], "needs_driven");
    assert_eq!(
        serde_json::from_value::<worlds::WorldPreset>(stored).unwrap(),
        living
    );
}

#[test]
fn unsupported_layouts_reject_needs_driven_mode_instead_of_silently_ignoring_it() {
    let mut preset = worlds::builtin_preset("living_island").unwrap();
    preset.scene = WorldScene::Explorer;
    assert!(worlds::validate(&preset).is_err());
    assert!(
        testbed_content::scenario::WorldSpec::parse(
            &json!({"layout":"open","economy":"needs_driven","furnish":false})
        )
        .is_err()
    );
}
