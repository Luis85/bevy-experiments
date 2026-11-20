use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use terra_bevy::asset_pack::{
    AssetPackManifest, ValidatedAssetPack, asset_sha256,
    authoring::{AssetPackRecipe, PreparedAssetPack, RecipeModel, inspect_model},
};

const DEER: &[u8] = include_bytes!("../assets/wildlife-pack/deer.glb");
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "terra-asset-authoring-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("exports")).unwrap();
        fs::write(root.join("exports/deer.glb"), DEER).unwrap();
        Self(root)
    }
    fn recipe(&self) -> AssetPackRecipe {
        let original: AssetPackManifest =
            serde_json::from_slice(include_bytes!("../assets/wildlife-pack/pack.json")).unwrap();
        AssetPackRecipe {
            schema_version: 1,
            name: "Artist handoff".into(),
            author: "Terra World contributors".into(),
            license: "MIT OR Apache-2.0".into(),
            models: vec![RecipeModel {
                id: "deer".into(),
                source: "exports/deer.glb".into(),
                scene: 0,
                provenance: "Original Terra wildlife generator; MIT OR Apache-2.0".into(),
            }],
            bindings: vec![original.bindings[0].clone()],
        }
    }
    fn write_recipe(&self, recipe: &AssetPackRecipe) -> PathBuf {
        let path = self.0.join("recipe.json");
        fs::write(&path, serde_json::to_vec_pretty(recipe).unwrap()).unwrap();
        path
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli(args: &[&Path], operation: &str, cwd: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_terra-assets"))
        .current_dir(cwd)
        .arg(operation)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn preparation_snapshots_bytes_and_publishes_identical_content_identified_packs() {
    let case = Case::new();
    let path = case.write_recipe(&case.recipe());
    let prepared = PreparedAssetPack::read(&path).unwrap();
    let second = PreparedAssetPack::read(&path).unwrap();
    assert_eq!(prepared.report().identity, second.report().identity);
    // The artist may start a new export while the accepted one is publishing.
    // Only the accepted immutable snapshot may become a runtime pack.
    fs::write(case.0.join("exports/deer.glb"), b"new incomplete export").unwrap();
    let a = case.0.join("first");
    let b = case.0.join("second");
    prepared.write_new(&a).unwrap();
    second.write_new(&b).unwrap();
    let model_file = format!("models/{}.glb", asset_sha256(DEER));
    for name in ["pack.json", "authoring-report.json", &model_file] {
        assert_eq!(
            fs::read(a.join(name)).unwrap(),
            fs::read(b.join(name)).unwrap()
        );
    }
    assert_eq!(fs::read(a.join(&model_file)).unwrap(), DEER);
    assert_eq!(
        fs::read(case.0.join("exports/deer.glb")).unwrap(),
        b"new incomplete export"
    );
    let loaded = ValidatedAssetPack::read(a.join("pack.json")).unwrap();
    assert_eq!(loaded.identity(), prepared.report().identity);
    assert_eq!(prepared.report().sources[0].source, "exports/deer.glb");
    assert!(prepared.report().sources[0].provenance.contains("MIT"));
}

#[test]
fn model_inspection_names_real_clips_and_complete_resource_budgets() {
    let case = Case::new();
    let path = case.0.join("exports/deer.glb");
    let model = inspect_model(&path).unwrap();
    assert_eq!(model.sha256, asset_sha256(DEER));
    assert_eq!(model.budget.bytes, DEER.len());
    assert_eq!(model.budget.nodes, model.nodes.len());
    assert_eq!(
        model
            .clips
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["drink", "feed", "idle", "run", "walk"]
    );
    assert!(
        model
            .clips
            .iter()
            .all(|c| c.channels > 0 && c.duration_seconds > 0.)
    );
    let output = cli(&[&path], "inspect", &case.0);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["accepted"], true);
    assert_eq!(report["model"]["sha256"], model.sha256);
    assert!(report["scope"].as_str().unwrap().contains("do not prove"));
}

#[test]
fn cli_build_resolves_sources_from_recipe_and_refuses_existing_output_without_writes() {
    let case = Case::new();
    let recipe_path = case.write_recipe(&case.recipe());
    let output_dir = case.0.join("published");
    let unrelated = case.0.join("different-working-directory");
    fs::create_dir(&unrelated).unwrap();
    let output = cli(&[&recipe_path, &output_dir], "build", &unrelated);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report,
        serde_json::from_slice::<serde_json::Value>(
            &fs::read(output_dir.join("authoring-report.json")).unwrap()
        )
        .unwrap()
    );
    let validation = cli(&[&output_dir.join("pack.json")], "validate", &unrelated);
    assert!(validation.status.success());
    let validated: serde_json::Value = serde_json::from_slice(&validation.stdout).unwrap();
    assert_eq!(validated["identity"], report["identity"]);
    fs::write(output_dir.join("keep.txt"), b"do not replace this pack").unwrap();
    let before = fs::read(output_dir.join("pack.json")).unwrap();
    let rejected = cli(&[&recipe_path, &output_dir], "build", &unrelated);
    assert!(!rejected.status.success());
    assert!(rejected.stdout.is_empty());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("already exists"));
    assert_eq!(fs::read(output_dir.join("pack.json")).unwrap(), before);
    assert_eq!(
        fs::read(output_dir.join("keep.txt")).unwrap(),
        b"do not replace this pack"
    );
    assert_eq!(fs::read(case.0.join("exports/deer.glb")).unwrap(), DEER);
}

#[test]
fn rejected_recipes_leave_no_output_and_explain_binding_or_input_failure() {
    let case = Case::new();
    let good = serde_json::to_value(case.recipe()).unwrap();
    let mut cases = Vec::new();
    let mut bad = good.clone();
    bad["bindings"][0]["animation"]["idle"] = "not-exported".into();
    cases.push((bad, "missing animation not-exported"));
    let mut bad = good.clone();
    bad["models"][0]["source"] = "../outside.glb".into();
    cases.push((bad, "relative .glb"));
    let mut bad = good.clone();
    bad["models"][0]["source"] = "/outside.glb".into();
    cases.push((bad, "relative .glb"));
    let mut bad = good.clone();
    bad["models"][0]["source"] = "https://example.invalid/model.glb".into();
    cases.push((bad, "relative .glb"));
    let mut bad = good.clone();
    bad["models"][0]["provenance"] = "".into();
    cases.push((bad, "provenance"));
    let mut bad = good.clone();
    bad["models"]
        .as_array_mut()
        .unwrap()
        .push(good["models"][0].clone());
    cases.push((bad, "unique"));
    let mut bad = good.clone();
    bad["bindings"]
        .as_array_mut()
        .unwrap()
        .push(good["bindings"][0].clone());
    cases.push((bad, "bound only once"));
    let mut bad = good.clone();
    bad["bindings"][0]["animation"]["walk_speed_m_s"] = 0.into();
    cases.push((bad, "speed and blend"));
    let mut bad = good.clone();
    bad["bindings"][0]["model"] = "absent".into();
    cases.push((bad, "Unknown bound model"));
    let mut bad = good.clone();
    bad["unexpected"] = true.into();
    cases.push((bad, "unknown field"));
    for (i, (bad, expected)) in cases.into_iter().enumerate() {
        let path = case.0.join("bad.json");
        fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        let output = case.0.join(format!("rejected-{i}"));
        let rejected = cli(&[&path, &output], "build", &case.0);
        assert!(!rejected.status.success(), "case {i}");
        assert!(rejected.stdout.is_empty());
        assert!(!output.exists());
        assert!(
            String::from_utf8_lossy(&rejected.stderr).contains(expected),
            "case {i}: {}",
            String::from_utf8_lossy(&rejected.stderr)
        );
    }
}

fn glb_with_json(change: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    let length = u32::from_le_bytes(DEER[12..16].try_into().unwrap()) as usize;
    let mut root: serde_json::Value = serde_json::from_slice(&DEER[20..20 + length]).unwrap();
    change(&mut root);
    let mut encoded = serde_json::to_vec(&root).unwrap();
    while !encoded.len().is_multiple_of(4) {
        encoded.push(b' ');
    }
    let binary_chunk = &DEER[20 + length..];
    let mut result = b"glTF".to_vec();
    result.extend(2u32.to_le_bytes());
    result.extend(((20 + encoded.len() + binary_chunk.len()) as u32).to_le_bytes());
    result.extend((encoded.len() as u32).to_le_bytes());
    result.extend(b"JSON");
    result.extend(encoded);
    result.extend(binary_chunk);
    result
}

#[test]
fn authoring_keeps_unused_animation_and_hierarchy_validation() {
    let case = Case::new();
    let recipe = case.write_recipe(&case.recipe());
    let oversized = glb_with_json(|root| {
        let mut unused = root["animations"][0].clone();
        let sampler = unused["samplers"][0].clone();
        unused["samplers"]
            .as_array_mut()
            .unwrap()
            .resize(512, sampler);
        for i in 0..8 {
            unused["name"] = format!("unused-{i}").into();
            root["animations"]
                .as_array_mut()
                .unwrap()
                .push(unused.clone());
        }
    });
    fs::write(case.0.join("exports/deer.glb"), oversized).unwrap();
    assert!(
        PreparedAssetPack::read(&recipe)
            .unwrap_err()
            .contains("including unused")
    );
    let cycle = glb_with_json(|root| {
        root["nodes"][0]["children"]
            .as_array_mut()
            .unwrap()
            .push(0.into());
    });
    fs::write(case.0.join("exports/deer.glb"), cycle).unwrap();
    assert!(PreparedAssetPack::read(&recipe).is_err());
}

#[test]
fn recipe_and_source_size_limits_apply_before_any_output() {
    let case = Case::new();
    let path = case.write_recipe(&case.recipe());
    let output = case.0.join("output");
    fs::write(&path, vec![b' '; 65537]).unwrap();
    let result = cli(&[&path, &output], "build", &case.0);
    assert!(!result.status.success());
    assert!(!output.exists());
    let path = case.write_recipe(&case.recipe());
    let file = fs::File::create(case.0.join("exports/deer.glb")).unwrap();
    file.set_len((terra_bevy::asset_pack::MAX_MODEL_BYTES + 1) as u64)
        .unwrap();
    let result = cli(&[&path, &output], "build", &case.0);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("byte import budget"));
    assert!(!output.exists());
}

#[test]
fn shared_model_bytes_keep_distinct_binding_policies_and_provenance() {
    let case = Case::new();
    let mut recipe = case.recipe();
    let mut shared = recipe.models[0].clone();
    shared.id = "alternate".into();
    shared.provenance =
        "Same original authored deer used as a deliberate appearance variant; MIT OR Apache-2.0"
            .into();
    recipe.models.push(shared);
    let mut binding = recipe.bindings[0].clone();
    binding.target =
        terra_bevy::asset_pack::PackTarget::Species(terra_bevy::rules::AnimalSpecies::Boar);
    binding.model = "alternate".into();
    binding.scale = 0.8;
    binding.animation.as_mut().unwrap().walk = "run".into();
    binding.animation.as_mut().unwrap().walk_speed_m_s = 1.1;
    recipe.bindings.push(binding);
    let path = case.write_recipe(&recipe);
    let prepared = PreparedAssetPack::read(&path).unwrap();
    let output = case.0.join("shared");
    prepared.write_new(&output).unwrap();
    assert_eq!(fs::read_dir(output.join("models")).unwrap().count(), 1);
    let manifest: AssetPackManifest =
        serde_json::from_slice(&fs::read(output.join("pack.json")).unwrap()).unwrap();
    assert_eq!(manifest.models[0].file, manifest.models[1].file);
    assert_eq!(
        manifest.bindings[0].animation.as_ref().unwrap().walk,
        "walk"
    );
    assert_eq!(manifest.bindings[1].animation.as_ref().unwrap().walk, "run");
    assert_eq!(
        manifest.bindings[1]
            .animation
            .as_ref()
            .unwrap()
            .walk_speed_m_s,
        1.1
    );
    assert_ne!(
        prepared.report().sources[0].provenance,
        prepared.report().sources[1].provenance
    );
    recipe.bindings[1]
        .animation
        .as_mut()
        .unwrap()
        .walk_speed_m_s = 1.2;
    let changed = PreparedAssetPack::read(case.write_recipe(&recipe)).unwrap();
    assert_ne!(changed.report().identity, prepared.report().identity);
}

#[cfg(unix)]
#[test]
fn symlink_sources_and_output_parents_are_rejected_even_when_contained() {
    use std::os::unix::fs::symlink;
    let case = Case::new();
    symlink(case.0.join("exports"), case.0.join("linked-exports")).unwrap();
    let mut recipe = case.recipe();
    recipe.models[0].source = "linked-exports/deer.glb".into();
    let path = case.write_recipe(&recipe);
    assert!(
        PreparedAssetPack::read(&path)
            .unwrap_err()
            .contains("symlink")
    );
    let path = case.write_recipe(&case.recipe());
    let prepared = PreparedAssetPack::read(&path).unwrap();
    symlink(case.0.join("exports"), case.0.join("linked-output-parent")).unwrap();
    assert!(
        prepared
            .write_new(case.0.join("linked-output-parent/new-pack"))
            .unwrap_err()
            .contains("symlink")
    );
    assert!(!case.0.join("exports/new-pack").exists());
    symlink(case.0.join("missing"), case.0.join("dangling-output")).unwrap();
    assert!(
        prepared
            .write_new(case.0.join("dangling-output"))
            .unwrap_err()
            .contains("symlink")
    );
    symlink(&path, case.0.join("recipe-alias.json")).unwrap();
    assert!(
        PreparedAssetPack::read(case.0.join("recipe-alias.json"))
            .unwrap_err()
            .contains("symlink")
    );
}

#[test]
fn checked_in_working_recipe_builds_all_species_without_changing_original_identity() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let original = ValidatedAssetPack::read(root.join("assets/wildlife-pack/pack.json")).unwrap();
    assert_eq!(
        original.identity(),
        "sha256:406c12494416b472ca5bee80a809e0ef30a45e382240cab01faa9081cd838d6d"
    );
    let recipe =
        PreparedAssetPack::read(root.join("assets/wildlife-authoring.recipe.json")).unwrap();
    assert_eq!(recipe.report().sources.len(), 5);
    let case = Case::new();
    let output = case.0.join("full-pack");
    recipe.write_new(&output).unwrap();
    let accepted = ValidatedAssetPack::read(output.join("pack.json")).unwrap();
    assert_eq!(accepted.identity(), recipe.report().identity);
    assert_eq!(accepted.budgets().count(), 5);
    assert_ne!(accepted.identity(), original.identity());
}
