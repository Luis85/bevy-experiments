//! Headless authoring over the runtime's exact immutable-pack importer.
//!
//! Recipes are editable source inputs. Preparation snapshots and validates all
//! bytes before publication; publication creates a new directory and never
//! updates either source files or an existing pack.
use super::*;
use std::{io::Write, path::PathBuf};

/// Source recipe version 1. Runtime packs keep their existing schema version.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetPackRecipe {
    pub schema_version: u32,
    pub name: String,
    pub author: String,
    pub license: String,
    pub models: Vec<RecipeModel>,
    pub bindings: Vec<PackBinding>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeModel {
    pub id: String,
    /// Relative GLB path below the recipe directory, without symlinks.
    pub source: String,
    #[serde(default)]
    pub scene: usize,
    /// The author's attribution/license/source statement; not a legal check.
    pub provenance: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClipInspection {
    pub name: String,
    pub duration_seconds: f32,
    pub channels: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ModelInspection {
    pub sha256: String,
    pub scene: usize,
    pub budget: ModelBudget,
    pub nodes: Vec<String>,
    pub clips: Vec<ClipInspection>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthoringSource {
    pub id: String,
    pub source: String,
    pub provenance: String,
    pub output: String,
    pub inspection: ModelInspection,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthoringReport {
    pub schema_version: u32,
    pub accepted: bool,
    pub name: String,
    pub identity: String,
    pub recipe_sha256: String,
    pub sources: Vec<AuthoringSource>,
    pub scope: String,
}

/// Fully validated immutable snapshots. Files may change after preparation;
/// publication still writes these exact accepted bytes.
#[derive(Clone, Debug)]
pub struct PreparedAssetPack {
    manifest: Vec<u8>,
    files: BTreeMap<String, Vec<u8>>,
    report: AuthoringReport,
}

fn inspection(model: &ValidatedModel) -> ModelInspection {
    ModelInspection {
        sha256: model.hash.clone(),
        scene: model.scene,
        budget: model.budget.clone(),
        nodes: model.nodes.iter().map(|node| node.name.clone()).collect(),
        clips: model
            .clips
            .iter()
            .map(|(name, clip)| ClipInspection {
                name: name.clone(),
                duration_seconds: clip.duration,
                channels: clip.channels.len(),
            })
            .collect(),
    }
}

/// Reject symlink components rather than silently borrowing an export outside
/// the author's declared source tree. A missing last component is allowed only
/// when choosing a new output directory. This is a local-workspace workflow,
/// not a security boundary against concurrent hostile filesystem replacement.
fn checked_path(path: &Path, missing_leaf: bool) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("Cannot locate working directory: {e}"))?
            .join(path)
    };
    let mut inspected = PathBuf::new();
    let components: Vec<_> = absolute.components().collect();
    for (i, component) in components.iter().enumerate() {
        match component {
            Component::ParentDir => {
                return Err(format!(
                    "{} contains '..'; choose an explicit path without parent traversal",
                    path.display()
                ));
            }
            Component::CurDir => continue,
            _ => inspected.push(component.as_os_str()),
        }
        match fs::symlink_metadata(&inspected) {
            Ok(metadata) => require(
                !metadata.file_type().is_symlink(),
                format!(
                    "{} uses a symlink; copy exports into the recipe tree and choose a real output parent",
                    inspected.display()
                ),
            )?,
            Err(error)
                if missing_leaf
                    && i + 1 == components.len()
                    && error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!("Cannot inspect {}: {error}", inspected.display()));
            }
        }
    }
    Ok(inspected)
}

/// Inspect one supported scene without inventing binding or locomotion policy.
/// All resource budgets, including unselected animations, use the runtime
/// validator. Axis, units, foot placement and animation intent need inspection
/// in the DCC/native game; they cannot be inferred reliably from a GLB.
pub fn inspect_model(path: impl AsRef<Path>) -> Result<ModelInspection, String> {
    let path = checked_path(path.as_ref(), false)?;
    let bytes = read_bounded(&path, MAX_MODEL_BYTES)?;
    let entry = PackModel {
        id: "inspection".into(),
        file: "inspection.glb".into(),
        sha256: asset_sha256(&bytes),
        scene: 0,
    };
    let model = validate_model(bytes, &entry).map_err(|e| {
        format!(
            "{}: {e}. See docs/ASSET_AUTHORING.md for the export profile",
            path.display()
        )
    })?;
    Ok(inspection(&model))
}

impl PreparedAssetPack {
    /// Read a bounded local recipe and run complete importer validation before
    /// any output is created. Source paths are relative to the recipe, not cwd.
    pub fn read(recipe_path: impl AsRef<Path>) -> Result<Self, String> {
        let recipe_path = checked_path(recipe_path.as_ref(), false)?;
        let recipe_bytes = read_bounded(&recipe_path, MAX_MANIFEST_BYTES)?;
        let recipe: AssetPackRecipe = serde_json::from_slice(&recipe_bytes)
            .map_err(|e| format!("Invalid authoring recipe: {e}"))?;
        require(
            recipe.schema_version == 1,
            "Authoring recipe schema_version must be 1",
        )?;
        require(
            (1..=16).contains(&recipe.models.len()) && (1..=22).contains(&recipe.bindings.len()),
            "Authoring recipe requires 1..16 models and 1..22 bindings",
        )?;
        require(
            short_text(&recipe.name, 120)
                && short_text(&recipe.author, 160)
                && short_text(&recipe.license, 160),
            "Recipe name, author and license are required bounded plain text",
        )?;
        let root = recipe_path
            .parent()
            .ok_or("Recipe must be a file below an existing directory")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let mut ids = BTreeSet::new();
        let mut total_bytes = 0usize;
        let mut files = BTreeMap::new();
        let mut models = Vec::new();
        for model in &recipe.models {
            require(
                id(&model.id) && ids.insert(model.id.clone()),
                format!(
                    "Model {:?}: use a unique 1..64 character ASCII letter/digit/'_'/'-' id",
                    model.id
                ),
            )?;
            require(
                safe_file(&model.source) && !model.source.chars().any(char::is_control),
                format!(
                    "Model {}: source must be a relative .glb path below the recipe; no '..', URL, query or backslash",
                    model.id
                ),
            )?;
            require(
                short_text(&model.provenance, 1024),
                format!(
                    "Model {}: supply a bounded plain-text provenance statement identifying the creator, source and permitted license",
                    model.id
                ),
            )?;
            let path = checked_path(&root.join(&model.source), false)?;
            let canonical = path.canonicalize().map_err(|e| e.to_string())?;
            require(
                canonical.starts_with(&root),
                "Source resolves outside the recipe tree",
            )?;
            let bytes = read_bounded(&canonical, MAX_MODEL_BYTES)
                .map_err(|e| format!("Model {}: {e}", model.id))?;
            total_bytes = total_bytes
                .checked_add(bytes.len())
                .ok_or("Source byte budget overflow")?;
            require(
                total_bytes <= MAX_PACK_BYTES,
                "Authoring sources exceed the 64 MiB aggregate model budget",
            )?;
            let hash = asset_sha256(&bytes);
            let file = format!("models/{hash}.glb");
            models.push(PackModel {
                id: model.id.clone(),
                file: file.clone(),
                sha256: hash,
                scene: model.scene,
            });
            files.entry(file).or_insert(bytes);
        }
        let manifest = AssetPackManifest {
            schema_version: 1,
            name: recipe.name,
            author: recipe.author,
            license: recipe.license,
            models,
            bindings: recipe.bindings,
        };
        let mut manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
        manifest_bytes.push(b'\n');
        let pack = ValidatedAssetPack::from_bytes(&manifest_bytes, |file| {
            files.get(file).cloned().ok_or_else(|| format!("Missing prepared model {file}"))
        }).map_err(|e| format!("Recipe does not produce an accepted pack: {e}. Use 'terra-assets inspect SOURCE.glb' to check clip names and budgets; see docs/ASSET_AUTHORING.md"))?;
        let report = AuthoringReport {
            schema_version: 1, accepted: true, name: pack.name().into(),
            identity: pack.identity().into(), recipe_sha256: asset_sha256(&recipe_bytes),
            sources: recipe.models.iter().zip(&manifest.models).map(|(source, model)| {
                AuthoringSource { id: source.id.clone(), source: source.source.clone(),
                    provenance: source.provenance.clone(), output: model.file.clone(),
                    inspection: inspection(&pack.models[&model.id]) }
            }).collect(),
            scope: "Exact runtime importer acceptance of immutable source bytes and binding policies. Author-declared provenance is retained, not legally verified. Units, axes, foot placement, visual quality and native readiness require a playthrough.".into(),
        };
        Ok(Self {
            manifest: manifest_bytes,
            files,
            report,
        })
    }

    pub fn report(&self) -> &AuthoringReport {
        &self.report
    }

    /// Publish into a new directory whose parent already exists. The manifest
    /// is written last. Invalid recipes never reach publication; I/O failures
    /// attempt to remove only this call's created files/directories. There is no
    /// cross-filesystem atomic-directory or power-loss durability promise.
    pub fn write_new(&self, destination: impl AsRef<Path>) -> Result<(), String> {
        let destination = checked_path(destination.as_ref(), true)?;
        require(
            !destination.exists(),
            format!(
                "{} already exists; choose a new output directory",
                destination.display()
            ),
        )?;
        let mut report = serde_json::to_vec_pretty(&self.report).map_err(|e| e.to_string())?;
        report.push(b'\n');
        fs::create_dir(&destination).map_err(|e| {
            format!(
                "Cannot create new pack directory {}: {e}",
                destination.display()
            )
        })?;
        let model_dir = destination.join("models");
        let mut written = Vec::new();
        let result: Result<(), String> = (|| {
            fs::create_dir(&model_dir).map_err(|e| e.to_string())?;
            for (name, bytes) in self
                .files
                .iter()
                .map(|(n, b)| (n.as_str(), b.as_slice()))
                .chain([
                    ("authoring-report.json", report.as_slice()),
                    ("pack.json", self.manifest.as_slice()),
                ])
            {
                let path = destination.join(name);
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|e| format!("Cannot create {}: {e}", path.display()))?;
                written.push(path.clone());
                file.write_all(bytes)
                    .and_then(|()| file.sync_all())
                    .map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            let mut cleanup_failed = false;
            for path in written.iter().rev() {
                cleanup_failed |= fs::remove_file(path).is_err();
            }
            if model_dir.exists() {
                cleanup_failed |= fs::remove_dir(&model_dir).is_err();
            }
            cleanup_failed |= fs::remove_dir(&destination).is_err();
            return Err(format!(
                "Pack publication failed: {error}. {}",
                if cleanup_failed {
                    "Cleanup was incomplete; inspect the new output directory before retrying."
                } else {
                    "New output was removed; source files were not changed."
                }
            ));
        }
        Ok(())
    }
}
