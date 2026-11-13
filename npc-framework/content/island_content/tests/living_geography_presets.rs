use island_content::scenario::{WorldSpec, build_app};
use island_content::world_runtime::ActiveWorld;
use island_content::worlds::{self, IslandGeography};
use serde_json::json;

#[test]
fn saved_presets_without_a_geography_profile_retain_the_compact_island() {
    let mut saved = serde_json::to_value(worlds::builtin_preset("living_island").unwrap()).unwrap();
    saved.as_object_mut().unwrap().remove("island_geography");
    let restored: worlds::WorldPreset = serde_json::from_value(saved).unwrap();
    assert_eq!(restored.island_geography, IslandGeography::Compact);
    assert!(
        serde_json::to_value(restored)
            .unwrap()
            .get("island_geography")
            .is_none()
    );
    for name in ["living_island", "living_homestead"] {
        let fresh = worlds::builtin_preset(name).unwrap();
        assert_eq!(fresh.island_geography, IslandGeography::Expanded);
        assert!(worlds::validate(&fresh).is_ok());
    }
}

#[test]
fn raw_saved_scenarios_do_not_inherit_a_new_builtin_shoreline() {
    let old =
        WorldSpec::parse(&json!({"layout":"island","economy":"needs_driven","furnish":false}))
            .unwrap();
    let old_app = build_app(7, &old, |_| {});
    assert_eq!(
        old_app.world().resource::<ActiveWorld>().0.island_geography,
        IslandGeography::Compact
    );
    let expanded = WorldSpec::parse(&json!({"layout":"island","economy":"needs_driven","island_geography":"expanded","furnish":false})).unwrap();
    let new_app = build_app(7, &expanded, |_| {});
    assert_eq!(
        new_app.world().resource::<ActiveWorld>().0.island_geography,
        IslandGeography::Expanded
    );
}

#[test]
fn expanded_geography_rejects_incompatible_world_models() {
    let mut preset = worlds::plain_preset();
    preset.island_geography = IslandGeography::Expanded;
    assert!(worlds::validate(&preset).is_err());
    assert!(
        WorldSpec::parse(&json!({"layout":"island","island_geography":"expanded","furnish":false}))
            .is_err()
    );
}
