use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use terra_bevy::asset_pack::{
    AssetPackManifest, PackTarget, ValidatedAssetPack, asset_sha256,
    authoring::{PreparedAssetPack, inspect_model},
};
use terra_sandbox::VehicleModel;

const MODEL: &[u8] = include_bytes!("../assets/transport-pack/comet.glb");
const PACK: &[u8] = include_bytes!("../assets/transport-pack/pack.json");
static NEXT: AtomicU64 = AtomicU64::new(0);

fn manifest() -> AssetPackManifest {
    serde_json::from_slice(PACK).unwrap()
}

fn validate(manifest: &AssetPackManifest) -> Result<ValidatedAssetPack, String> {
    ValidatedAssetPack::from_bytes(&serde_json::to_vec(manifest).unwrap(), |_| {
        Ok(MODEL.to_vec())
    })
}

fn altered_model(mutator: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    let length = u32::from_le_bytes(MODEL[12..16].try_into().unwrap()) as usize;
    let mut json: serde_json::Value = serde_json::from_slice(&MODEL[20..20 + length]).unwrap();
    mutator(&mut json);
    let mut text = serde_json::to_vec(&json).unwrap();
    while text.len() % 4 != 0 {
        text.push(b' ');
    }
    let tail = &MODEL[20 + length..];
    let size = 20 + text.len() + tail.len();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"glTF");
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    bytes.extend_from_slice(&(size as u32).to_le_bytes());
    bytes.extend_from_slice(&(text.len() as u32).to_le_bytes());
    bytes.extend_from_slice(b"JSON");
    bytes.extend(text);
    bytes.extend_from_slice(tail);
    bytes
}

#[test]
fn original_comet_has_named_cabin_wheels_and_admitted_preview_animation() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/transport-pack");
    let model = inspect_model(root.join("comet.glb")).unwrap();
    assert_eq!(model.sha256, asset_sha256(MODEL));
    assert_eq!(model.budget.bytes, MODEL.len());
    assert_eq!(
        model
            .clips
            .iter()
            .map(|clip| clip.name.as_str())
            .collect::<Vec<_>>(),
        ["parked", "rolling"]
    );
    for node in [
        "dashboard",
        "left-front-seat-base",
        "right-rear-seat-back",
        "front_left_wheel",
        "front_right_steer",
        "rear_right_wheel",
    ] {
        assert!(model.nodes.iter().any(|n| n == node), "{node}");
    }
    assert!(model.budget.nodes < 128 && model.budget.triangles < 10_000);
    let pack = ValidatedAssetPack::read(root.join("pack.json")).unwrap();
    assert_eq!(pack.budgets().count(), 1);
    assert_eq!(
        manifest().bindings[0].target,
        PackTarget::VehicleModel(VehicleModel::Compact)
    );
}

#[test]
fn vehicle_targets_are_bounded_specific_and_keep_original_pack_identity() {
    let original = include_bytes!("../assets/wildlife-pack/pack.json");
    let pack = ValidatedAssetPack::from_bytes(original, |name| {
        fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("assets/wildlife-pack")
                .join(name),
        )
        .map_err(|e| e.to_string())
    })
    .unwrap();
    assert_eq!(
        pack.identity(),
        "sha256:406c12494416b472ca5bee80a809e0ef30a45e382240cab01faa9081cd838d6d"
    );
    let old: AssetPackManifest = serde_json::from_slice(original).unwrap();
    assert!(
        !serde_json::to_string(&old)
            .unwrap()
            .contains("vehicle_motion")
    );
    for value in ["unknown", "human", "comet"] {
        let target = serde_json::json!({"type": "vehicle_model", "value": value});
        assert!(serde_json::from_value::<PackTarget>(target).is_err());
    }
    let mut duplicate = manifest();
    duplicate.bindings.push(duplicate.bindings[0].clone());
    assert!(validate(&duplicate).unwrap_err().contains("only once"));
    let mut species = manifest();
    species.bindings[0].target = PackTarget::Species(terra_sandbox::AnimalSpecies::Deer);
    assert!(
        validate(&species)
            .unwrap_err()
            .contains("requires a vehicle")
    );
}

#[test]
fn invalid_wheel_rigs_reject_before_publishing_or_rendering() {
    let original = manifest();
    let reject = |manifest: &AssetPackManifest| assert!(validate(manifest).is_err());
    for radius in [0., -1., 3.01] {
        let mut m = original.clone();
        m.bindings[0]
            .animation
            .as_mut()
            .unwrap()
            .vehicle_motion
            .as_mut()
            .unwrap()
            .wheel_radius_m = radius;
        reject(&m);
    }
    for wheels in [
        vec![],
        vec!["front_left_wheel".into()],
        vec!["front_left_wheel".into(), "front_left_wheel".into()],
        vec!["front_left_wheel".into(), "missing".into()],
        vec!["comet".into(), "front_left_wheel".into()],
        vec!["front_left_wheel".into(), "front_left-tire".into()],
    ] {
        let mut m = original.clone();
        m.bindings[0]
            .animation
            .as_mut()
            .unwrap()
            .vehicle_motion
            .as_mut()
            .unwrap()
            .wheels = wheels;
        reject(&m);
    }
    for steering in [
        vec!["dashboard".into()],
        vec!["front_left_wheel".into()],
        vec!["front_left_steer".into(); 5],
    ] {
        let mut m = original.clone();
        m.bindings[0]
            .animation
            .as_mut()
            .unwrap()
            .vehicle_motion
            .as_mut()
            .unwrap()
            .steering = steering;
        reject(&m);
    }
}

#[test]
fn wheel_rig_rejects_scaled_or_animated_ancestors_including_unused_clips() {
    for (bytes, expected) in [
        (
            altered_model(|json| json["nodes"][0]["scale"] = serde_json::json!([1.1, 1., 1.])),
            "ancestor scales",
        ),
        (
            altered_model(|json| {
                let mut clip = json["animations"][0].clone();
                clip["name"] = "unused-root-motion".into();
                clip["channels"][0]["target"]["node"] = 0.into();
                json["animations"].as_array_mut().unwrap().push(clip);
            }),
            "ancestors cannot be animated",
        ),
    ] {
        let mut m = manifest();
        m.models[0].sha256 = asset_sha256(&bytes);
        let error =
            ValidatedAssetPack::from_bytes(&serde_json::to_vec(&m).unwrap(), |_| Ok(bytes.clone()))
                .unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn composed_recipe_preserves_wildlife_and_builds_a_repeatable_transport_handoff() {
    let recipe =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/transport-authoring.recipe.json");
    let prepared = PreparedAssetPack::read(&recipe).unwrap();
    assert_eq!(prepared.report().sources.len(), 6);
    assert_eq!(
        prepared.report().identity,
        PreparedAssetPack::read(&recipe).unwrap().report().identity
    );
    let path = std::env::temp_dir().join(format!(
        "terra-transport-art-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    prepared.write_new(&path).unwrap();
    let loaded = ValidatedAssetPack::read(path.join("pack.json")).unwrap();
    assert_eq!(loaded.identity(), prepared.report().identity);
    assert_eq!(loaded.budgets().count(), 6);
    for source in &prepared.report().sources {
        let original = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join(&source.source);
        assert_eq!(
            fs::read(path.join(&source.output)).unwrap(),
            fs::read(original).unwrap()
        );
    }
    fs::remove_dir_all(path).unwrap();
}
