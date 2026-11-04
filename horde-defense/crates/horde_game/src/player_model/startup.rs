//! Read-only, bounded startup asset loading. No widget/frame performs filesystem
//! work; missing manifests use the bundled package, malformed ones fail closed.
use super::*;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelManifest {
    pub schema_version: u32,
    pub models: Vec<String>,
    pub skins: Vec<String>,
    pub default_model_map: BTreeMap<String, String>,
}
#[derive(Debug)]
pub struct StartupPlayerModels {
    pub catalogue: CompiledCatalogue,
    pub manifest: ModelManifest,
    pub provenance: String,
    pub asset_root: PathBuf,
}
static ACTIVE: OnceLock<Result<StartupPlayerModels, String>> = OnceLock::new();
pub fn initialize_player_models(asset_root: &Path) -> Result<(), String> {
    match ACTIVE.get_or_init(|| {
        let loaded = load_from_asset_root(asset_root)?;
        // Runs before LogPlugin installs the subscriber, so stderr is the only channel.
        #[allow(clippy::print_stderr)]
        {
            eprintln!(
                "Player models: {} models, {} sources from {} (asset root {})",
                loaded.catalogue.models.len(),
                loaded.catalogue.sources.len(),
                loaded.provenance,
                loaded.asset_root.display()
            );
        }
        Ok(loaded)
    }) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.clone()),
    }
}
pub fn active_catalogue() -> Result<&'static CompiledCatalogue, String> {
    match ACTIVE.get() {
        Some(Ok(value)) => Ok(&value.catalogue),
        Some(Err(e)) => Err(e.clone()),
        None => Err("Player model startup cache has not been initialized".into()),
    }
}
pub fn active_or_bundled() -> Result<&'static CompiledCatalogue, String> {
    if ACTIVE.get().is_some() {
        active_catalogue()
    } else {
        super::builtin()
    }
}
pub fn model_for_appearance_key(key: &str) -> Result<String, String> {
    match ACTIVE.get() {
        Some(Ok(value)) => value
            .manifest
            .default_model_map
            .get(key)
            .cloned()
            .ok_or_else(|| format!("Unknown player appearance key {key}")),
        Some(Err(e)) => Err(e.clone()),
        None => bundled_manifest()
            .default_model_map
            .get(key)
            .cloned()
            .ok_or_else(|| format!("Unknown player appearance key {key}")),
    }
}
fn bundled_manifest() -> ModelManifest {
    ModelManifest {
        schema_version: 1,
        models: vec!["rook.json".into(), "june.json".into()],
        skins: vec!["skins.json".into(), "attachments.json".into()],
        default_model_map: [
            ("male".into(), "rook".into()),
            ("female".into(), "june".into()),
            ("rook".into(), "rook".into()),
            ("june".into(), "june".into()),
        ]
        .into_iter()
        .collect(),
    }
}
pub fn load_from_asset_root(asset_root: &Path) -> Result<StartupPlayerModels, String> {
    let root = asset_root
        .canonicalize()
        .map_err(|e| format!("Invalid player asset root: {e}"))?;
    if !root.is_dir() {
        return Err("Player asset root is not a directory".into());
    }
    let manifest_path = root.join("player_models/manifest.json");
    if !manifest_path
        .try_exists()
        .map_err(|e| format!("Cannot inspect model manifest: {e}"))?
    {
        return Ok(StartupPlayerModels {
            catalogue: compile_from_root(&root, super::bundled_catalogue()?)?,
            manifest: bundled_manifest(),
            provenance: "bundled fallback (manifest absent)".into(),
            asset_root: root,
        });
    }
    let bytes = read_bounded(&manifest_path, 1024 * 1024)?;
    let manifest: ModelManifest =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid model manifest: {e}"))?;
    if manifest.schema_version != 1
        || manifest.models.is_empty()
        || manifest.models.len() > 32
        || manifest.skins.is_empty()
        || manifest.skins.len() > 32
        || manifest.default_model_map.is_empty()
        || manifest.default_model_map.len() > 128
    {
        return Err("Invalid player model manifest counts/version".into());
    }
    let folder = manifest_path
        .parent()
        .ok_or("Missing model manifest directory")?;
    let models = manifest
        .models
        .iter()
        .map(|path| read_document(&root, folder, path))
        .collect::<Result<Vec<_>, _>>()?;
    let skins = manifest
        .skins
        .iter()
        .map(|path| read_document(&root, folder, path))
        .collect::<Result<Vec<_>, _>>()?;
    if models.iter().chain(&skins).map(String::len).sum::<usize>() > 32 * 1024 * 1024 {
        return Err("Player model JSON aggregate budget exceeded".into());
    }
    let definition = PlayerModelCatalogue::from_documents(
        &models.iter().map(String::as_str).collect::<Vec<_>>(),
        &skins.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    for (key, id) in &manifest.default_model_map {
        if !super::catalogue::valid_id(key) || definition.model(id).is_none() {
            return Err("Unknown player model manifest mapping".into());
        }
    }
    let catalogue = compile_from_root(&root, definition)?;
    Ok(StartupPlayerModels {
        catalogue,
        manifest,
        provenance: manifest_path.display().to_string(),
        asset_root: root,
    })
}
fn source_specs(
    definition: &PlayerModelCatalogue,
) -> Vec<super::attachments::SourceAssetDefinition> {
    let mut sources = BTreeMap::new();
    for model in &definition.models {
        sources.insert(
            model.source.id.clone(),
            super::attachments::SourceAssetDefinition::from(&model.source),
        );
    }
    for source in &definition.sources {
        sources.insert(source.id.clone(), source.clone());
    }
    sources.into_values().collect()
}
fn read_bounded(path: &Path, max: usize) -> Result<Vec<u8>, String> {
    if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > max as u64 {
        return Err(format!("Player model file too large: {}", path.display()));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if bytes.len() > max {
        return Err("Player model file grew beyond read budget".into());
    }
    Ok(bytes)
}
fn checked_path(root: &Path, base: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty()
        || relative.len() > 512
        || Path::new(relative).is_absolute()
        || relative.contains(':')
        || relative.split(['/', '\\']).any(|v| v == "..")
    {
        return Err("Invalid relative player model asset path".into());
    }
    let path = base
        .join(relative)
        .canonicalize()
        .map_err(|e| format!("Missing player model asset {relative}: {e}"))?;
    if !path.starts_with(root) {
        return Err("Player model asset escapes asset root".into());
    }
    Ok(path)
}
fn read_document(root: &Path, base: &Path, path: &str) -> Result<String, String> {
    String::from_utf8(read_bounded(
        &checked_path(root, base, path)?,
        16 * 1024 * 1024,
    )?)
    .map_err(|e| e.to_string())
}
fn decode_png(bytes: &[u8], size: [u32; 2]) -> Result<bevy::prelude::Image, String> {
    use bevy::{
        asset::RenderAssetUsages,
        image::{CompressedImageFormats, ImageSampler, ImageType},
        prelude::Image,
    };
    if bytes.len() < 33 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return Err("Player source must be a bounded PNG".into());
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    if [width, height] != size || bytes[24] != 8 || bytes[25] != 6 {
        return Err("Player source PNG dimensions/RGBA8 mismatch".into());
    }
    Image::from_buffer(
        bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::nearest(),
        RenderAssetUsages::MAIN_WORLD,
    )
    .map_err(|e| e.to_string())
}
/// Where the bundled catalogue's source images live: the startup asset root,
/// or the development `assets/` folder only when startup has not run (tests).
/// A failed startup is an error, never a silent switch to another root.
pub(super) fn bundled_asset_root() -> Result<PathBuf, String> {
    match ACTIVE.get() {
        Some(Ok(value)) => Ok(value.asset_root.clone()),
        Some(Err(error)) => Err(error.clone()),
        None => Ok(PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets"
        ))),
    }
}
pub(super) fn compile_bundled(asset_root: &Path) -> Result<CompiledCatalogue, String> {
    let root = asset_root
        .canonicalize()
        .map_err(|e| format!("Invalid player asset root: {e}"))?;
    compile_from_root(&root, super::bundled_catalogue()?)
}
/// Reads, bounds and decodes every source PNG a definition names from the
/// canonical asset root: the same files the AssetServer serves, never an
/// embedded copy.
fn compile_from_root(
    root: &Path,
    definition: PlayerModelCatalogue,
) -> Result<CompiledCatalogue, String> {
    let specs = source_specs(&definition);
    let mut images = Vec::with_capacity(specs.len());
    for source in &specs {
        let path = checked_path(root, root, &source.path)?;
        let bytes = read_bounded(&path, 64 * 1024 * 1024)?;
        images.push(decode_png(&bytes, source.size)?);
    }
    let views = specs
        .iter()
        .zip(&images)
        .map(|(spec, image)| SourceImageView {
            id: &spec.id,
            width: image.width(),
            height: image.height(),
            rgba: image.data.as_deref().unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    compile_catalogue(definition, &views)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Removes the scratch asset root even when an assertion fails first.
    struct TempRoot(PathBuf);
    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn summary(catalogue: &CompiledCatalogue) -> Vec<(String, String, String)> {
        catalogue
            .sources
            .values()
            .map(|s| (s.id.clone(), s.path.clone(), s.rgba_fnv1a64.clone()))
            .collect()
    }

    #[test]
    fn bundled_catalogue_matches_the_manifest_and_reads_sources_from_the_asset_root() {
        let root = bundled_asset_root().unwrap();
        let manifest = load_from_asset_root(&root).unwrap();
        let bundled = super::super::builtin().unwrap();
        assert_eq!(summary(&manifest.catalogue), summary(bundled));
        assert_eq!(
            manifest.catalogue.models.keys().collect::<Vec<_>>(),
            bundled.models.keys().collect::<Vec<_>>()
        );
        assert_eq!(
            manifest.catalogue.attachments.keys().collect::<Vec<_>>(),
            bundled.attachments.keys().collect::<Vec<_>>()
        );
        assert_eq!(
            format!("{:?}", manifest.catalogue.definition.skins),
            format!("{:?}", bundled.definition.skins)
        );
        assert_eq!(
            manifest.manifest.default_model_map,
            bundled_manifest().default_model_map
        );

        // Without a manifest the bundled documents still read their PNGs from
        // the asset root; with no source images there is nothing embedded to use.
        let guard = TempRoot(std::env::temp_dir().join(format!(
            "lastlight-player-model-root-{}",
            std::process::id()
        )));
        let temp = guard.0.as_path();
        let _ = std::fs::remove_dir_all(temp);
        std::fs::create_dir_all(temp.join("art/pixel")).unwrap();
        let missing = load_from_asset_root(temp).unwrap_err();
        assert!(missing.contains("Missing player model asset"), "{missing}");
        for (_, path, _) in summary(bundled) {
            std::fs::copy(root.join(&path), temp.join(&path)).unwrap();
        }
        let fallback = load_from_asset_root(temp).unwrap();
        assert!(fallback.provenance.contains("manifest absent"));
        assert_eq!(summary(&fallback.catalogue), summary(bundled));
    }
}
