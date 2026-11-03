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
        eprintln!(
            "Player models: {} models, {} sources from {} (asset root {})",
            loaded.catalogue.models.len(),
            loaded.catalogue.sources.len(),
            loaded.provenance,
            loaded.asset_root.display()
        );
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
        super::equipped_builtin()
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
            catalogue: decode_embedded(super::bundled_equipment_catalogue()?)?,
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
    let specs = source_specs(&definition);
    let mut images = Vec::with_capacity(specs.len());
    for source in &specs {
        let path = checked_path(&root, &root, &source.path)?;
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
    let catalogue = compile_catalogue(definition, &views)?;
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
pub(super) fn decode_embedded(
    definition: PlayerModelCatalogue,
) -> Result<CompiledCatalogue, String> {
    let specs = source_specs(&definition);
    let mut images = Vec::new();
    for source in &specs {
        let bytes: &[u8] = match source.path.as_str() {
            "art/pixel/menu-survivors-v1.png" => {
                include_bytes!("../../../../assets/art/pixel/menu-survivors-v1.png")
            }
            "art/pixel/defender-rig-v1.png" => {
                include_bytes!("../../../../assets/art/pixel/defender-rig-v1.png")
            }
            "art/pixel/canonical-parts-v1.png" => {
                include_bytes!("../../../../assets/art/pixel/canonical-parts-v1.png")
            }
            "art/pixel/player-underlayers-v1.png" => {
                include_bytes!("../../../../assets/art/pixel/player-underlayers-v1.png")
            }
            "art/pixel/player-equipment-v1.png" => {
                include_bytes!("../../../../assets/art/pixel/player-equipment-v1.png")
            }
            "art/pixel/player-hidden-torsos-v1.png" => {
                include_bytes!("../../../../assets/art/pixel/player-hidden-torsos-v1.png")
            }
            _ => {
                return Err(format!(
                    "Unbundled source {}; provide a model manifest",
                    source.path
                ));
            }
        };
        images.push(decode_png(bytes, source.size)?);
    }
    let views = specs
        .iter()
        .zip(&images)
        .map(|(source, image)| SourceImageView {
            id: &source.id,
            width: image.width(),
            height: image.height(),
            rgba: image.data.as_deref().unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    compile_catalogue(definition, &views)
}
