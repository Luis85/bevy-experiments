//! Bounded, immutable presentation packs. No field here enters world identity,
//! gameplay saves, actor ownership, contacts or the multiplayer protocol.
//!
//! The importer accepts self-contained GLB 2.0, named nodes, triangle meshes,
//! embedded PNG textures, and linear/step node-transform animation. Its
//! deliberately explicit subset is documented in `docs/ASSET_PACKS.md`.
use image::{ImageFormat, ImageReader, Limits};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Read},
    path::{Component, Path},
    sync::Arc,
};
use terra_core::ActorKind;
use terra_sandbox::{AnimalSpecies, VehicleModel};

#[path = "asset_authoring.rs"]
pub mod authoring;

pub const MAX_PACK_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_MODEL_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_MODEL_NODES: usize = 256;
pub const MAX_MODEL_VERTICES: usize = 100_000;
pub const MAX_MODEL_ANIMATION_KEYS: usize = 120_000;
pub const MAX_MODEL_DECODED_SCALARS: usize = 4_000_000;
const MAX_MANIFEST_BYTES: usize = 64 * 1024;
const MAX_CHANNELS: usize = 512;
const MAX_TEXTURE_DECODE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetPackManifest {
    pub schema_version: u32,
    pub name: String,
    pub author: String,
    pub license: String,
    pub models: Vec<PackModel>,
    pub bindings: Vec<PackBinding>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackModel {
    pub id: String,
    pub file: String,
    pub sha256: String,
    #[serde(default)]
    pub scene: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum PackTarget {
    Kind(ActorKind),
    Species(AnimalSpecies),
    /// Presentation for this existing catalog model only. Older schema-1
    /// readers reject this new target; original kind/species packs are intact.
    VehicleModel(VehicleModel),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackBinding {
    pub target: PackTarget,
    pub model: String,
    #[serde(default = "one")]
    pub scale: f32,
    #[serde(default)]
    pub ground_offset_m: f32,
    #[serde(default)]
    pub yaw_offset_degrees: f32,
    pub animation: Option<AnimationPolicy>,
    /// Optional presentation-only hip anchor for the existing known driver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver_seat: Option<DriverSeatPolicy>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverSeatPolicy {
    pub translation_m: [f32; 3],
}
fn one() -> f32 {
    1.
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationPolicy {
    pub idle: String,
    pub walk: String,
    pub run: String,
    pub feed: String,
    pub drink: String,
    pub walk_speed_m_s: f32,
    pub run_speed_m_s: f32,
    #[serde(default = "blend_default")]
    pub blend_seconds: f32,
    /// Optional semantic wheel rig. Omission serializes exactly as before,
    /// preserving every existing pack identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vehicle_motion: Option<VehicleMotionPolicy>,
    /// Explicit customization and seated-pose contract for a human binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human: Option<HumanAnimationPolicy>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanAnimationPolicy {
    pub seated: String,
    pub skin_material: String,
    pub hair_material: String,
    pub outfit_material: String,
}
impl HumanAnimationPolicy {
    pub fn material_names(&self) -> [&str; 3] {
        [
            &self.skin_material,
            &self.hair_material,
            &self.outfit_material,
        ]
    }
}

/// Wheel motion is derived from the admitted body velocity, not from the
/// acceleration intent or an animation clip's root motion. Local +X rolls the
/// wheel and local +Y steers its separate parent pivot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VehicleMotionPolicy {
    pub wheel_radius_m: f32,
    pub wheels: Vec<String>,
    pub steering: Vec<String>,
}
fn blend_default() -> f32 {
    0.2
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ModelBudget {
    pub nodes: usize,
    /// Declared mesh geometry, including unused meshes and aliased primitives.
    pub vertices: usize,
    pub triangles: usize,
    /// Geometry instantiated by every reachable mesh node in the scene.
    pub scene_vertices: usize,
    pub scene_triangles: usize,
    /// Bevy duplicates indexed vertices when it must generate flat normals.
    pub renderer_vertices: usize,
    pub scene_renderer_vertices: usize,
    pub primitives: usize,
    pub scene_primitives: usize,
    pub materials: usize,
    pub clips: usize,
    pub channels: usize,
    pub animation_keys: usize,
    pub runtime_animation_keys: usize,
    pub decoded_scalars: usize,
    pub texture_pixels: usize,
    /// Bevy creates one image per texture slot, including aliases of one PNG.
    pub texture_slot_pixels: usize,
    pub bytes: usize,
}

/// Its fields are private: callers cannot change bytes after validation or
/// accidentally associate another model's validation result with a file.
#[derive(Clone, Debug)]
pub struct ValidatedAssetPack {
    pub(crate) manifest: AssetPackManifest,
    pub(crate) identity: String,
    pub(crate) models: BTreeMap<String, Arc<ValidatedModel>>,
}
#[derive(Clone, Debug)]
#[cfg_attr(not(feature = "render"), allow(dead_code))] // Retained for optional renderer installation.
pub(crate) struct ValidatedModel {
    pub bytes: Vec<u8>,
    pub hash: String,
    pub scene: usize,
    pub budget: ModelBudget,
    pub nodes: Vec<ModelNode>,
    pub parents: Vec<Option<usize>>,
    pub clips: BTreeMap<String, ModelClip>,
    /// All labels the supported Bevy glTF/PBR loaders may retain. Retirement
    /// keeps its reservation until these assets have left the asset server.
    pub asset_labels: Vec<String>,
    pub materials: Vec<ModelMaterial>,
}
#[derive(Clone, Debug)]
#[cfg_attr(not(feature = "render"), allow(dead_code))]
pub(crate) struct ModelMaterial {
    pub name: Option<String>,
    pub customizable: bool,
    pub scene_primitives: usize,
}
#[derive(Clone, Debug)]
#[cfg_attr(not(feature = "render"), allow(dead_code))]
pub(crate) struct ModelNode {
    pub name: String,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}
#[derive(Clone, Debug)]
#[cfg_attr(not(feature = "render"), allow(dead_code))]
pub(crate) struct ModelClip {
    pub duration: f32,
    pub channels: Vec<ModelChannel>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ChannelProperty {
    Translation,
    Rotation,
    Scale,
}
#[derive(Clone, Debug)]
#[cfg_attr(not(feature = "render"), allow(dead_code))]
pub(crate) struct ModelChannel {
    pub node: usize,
    pub property: ChannelProperty,
    pub step: bool,
    pub times: Vec<f32>,
    pub values: Vec<[f32; 4]>,
}

fn require(ok: bool, message: impl Into<String>) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message.into()) }
}
fn short_text(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.len() <= max && !text.chars().any(char::is_control)
}
fn id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}
fn safe_file(file: &str) -> bool {
    !file.is_empty()
        && file.len() <= 240
        && file.ends_with(".glb")
        && !file.contains(['\\', ':', '#', '?'])
        && Path::new(file)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
pub fn asset_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read_bounded(path: &Path, max: usize) -> Result<Vec<u8>, String> {
    let metadata =
        fs::metadata(path).map_err(|e| format!("Cannot inspect {}: {e}", path.display()))?;
    require(
        metadata.is_file(),
        format!("{} must be a regular file", path.display()),
    )?;
    require(
        metadata.len() <= max as u64,
        format!("{} exceeds its {max}-byte import budget", path.display()),
    )?;
    let mut file =
        fs::File::open(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take((max + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    require(
        bytes.len() <= max,
        format!("{} exceeds its {max}-byte import budget", path.display()),
    )?;
    Ok(bytes)
}

impl ValidatedAssetPack {
    pub fn read(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let manifest = read_bounded(path, MAX_MANIFEST_BYTES)?;
        let root = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()
            .map_err(|e| e.to_string())?;
        Self::from_bytes(&manifest, |file| {
            let path = root
                .join(file)
                .canonicalize()
                .map_err(|e| format!("Missing pack model {file}: {e}"))?;
            require(
                path.starts_with(&root),
                "Model path resolves outside the asset pack",
            )?;
            read_bounded(&path, MAX_MODEL_BYTES)
        })
    }
    /// Embedded games can provide immutable bytes without shipping loose files.
    /// The callback is called only with validated, relative GLB filenames.
    pub fn from_bytes(
        manifest: &[u8],
        mut read_model: impl FnMut(&str) -> Result<Vec<u8>, String>,
    ) -> Result<Self, String> {
        require(
            manifest.len() <= MAX_MANIFEST_BYTES,
            "Asset manifest exceeds 64 KiB",
        )?;
        let manifest: AssetPackManifest =
            serde_json::from_slice(manifest).map_err(|e| format!("Invalid asset manifest: {e}"))?;
        require(
            manifest.schema_version == 1,
            "Asset pack schema_version must be 1",
        )?;
        require(
            short_text(&manifest.name, 120)
                && short_text(&manifest.author, 160)
                && short_text(&manifest.license, 160),
            "Pack name, author and license are required and must be bounded plain text",
        )?;
        require(
            (1..=16).contains(&manifest.models.len())
                && (1..=22).contains(&manifest.bindings.len()),
            "Asset pack requires 1..16 models and 1..22 bindings",
        )?;
        let mut models = BTreeMap::new();
        let mut total_bytes = 0;
        for entry in &manifest.models {
            require(
                id(&entry.id) && safe_file(&entry.file) && !models.contains_key(&entry.id),
                "Model IDs must be unique; files must be relative .glb paths",
            )?;
            require(
                entry.sha256.len() == 64
                    && entry
                        .sha256
                        .bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
                "Model sha256 must be 64 lowercase hexadecimal digits",
            )?;
            let bytes = read_model(&entry.file)?;
            total_bytes += bytes.len();
            require(
                bytes.len() <= MAX_MODEL_BYTES && total_bytes <= MAX_PACK_BYTES,
                "Asset pack exceeds model or total byte budget",
            )?;
            require(
                asset_sha256(&bytes) == entry.sha256,
                format!(
                    "{}: SHA-256 mismatch; update the manifest after exporting the model",
                    entry.id
                ),
            )?;
            let model = validate_model(bytes, entry).map_err(|e| format!("{}: {e}", entry.id))?;
            models.insert(entry.id.clone(), Arc::new(model));
        }
        let mut targets = BTreeSet::new();
        for binding in &manifest.bindings {
            require(
                targets.insert(format!("{:?}", binding.target)),
                "Each actor kind, species or vehicle model may be bound only once",
            )?;
            let model = models
                .get(&binding.model)
                .ok_or_else(|| format!("Unknown bound model {}", binding.model))?;
            require(
                binding.scale.is_finite()
                    && (0.001..=100.).contains(&binding.scale)
                    && binding.ground_offset_m.is_finite()
                    && binding.ground_offset_m.abs() <= 100.
                    && binding.yaw_offset_degrees.is_finite()
                    && binding.yaw_offset_degrees.abs() <= 3600.,
                "Binding transform exceeds presentation limits",
            )?;
            if let Some(seat) = &binding.driver_seat {
                require(
                    matches!(binding.target, PackTarget::VehicleModel(_))
                        && seat.translation_m.iter().all(|v| {
                            v.is_finite() && v.abs() <= 10. && (v * binding.scale).abs() <= 30.
                        }),
                    "A driver seat requires a vehicle-model target and bounded finite model-space position",
                )?;
            }
            if let Some(policy) = &binding.animation {
                require(
                    policy.walk_speed_m_s.is_finite()
                        && (0.1..=100.).contains(&policy.walk_speed_m_s)
                        && policy.run_speed_m_s.is_finite()
                        && policy.run_speed_m_s > policy.walk_speed_m_s
                        && policy.run_speed_m_s <= 200.
                        && policy.blend_seconds.is_finite()
                        && (0.0..=2.).contains(&policy.blend_seconds),
                    "Animation speed and blend policy is invalid",
                )?;
                for name in [
                    &policy.idle,
                    &policy.walk,
                    &policy.run,
                    &policy.feed,
                    &policy.drink,
                ] {
                    require(
                        model.clips.contains_key(name),
                        format!(
                            "Binding {} references missing animation {name}",
                            binding.model
                        ),
                    )?;
                }
                if let Some(human) = &policy.human {
                    require(
                        binding.target == PackTarget::Kind(ActorKind::Human)
                            && policy.vehicle_motion.is_none(),
                        "Human customization requires a human kind target without a vehicle rig",
                    )?;
                    require(
                        model.clips.contains_key(&human.seated),
                        "Human policy references a missing seated animation",
                    )?;
                    require(
                        model
                            .nodes
                            .iter()
                            .all(|node| node.scale.iter().all(|v| *v > 0.))
                            && model
                                .clips
                                .values()
                                .flat_map(|clip| &clip.channels)
                                .filter(|channel| channel.property == ChannelProperty::Scale)
                                .all(|channel| {
                                    channel
                                        .values
                                        .iter()
                                        .all(|value| value[..3].iter().all(|v| *v > 0.))
                                }),
                        "Customized human hierarchy and animations require positive scales",
                    )?;
                    let mut roles = BTreeSet::new();
                    for name in human.material_names() {
                        let matches: Vec<_> = model
                            .materials
                            .iter()
                            .filter(|material| material.name.as_deref() == Some(name))
                            .collect();
                        require(
                            short_text(name, 96)
                                && roles.insert(name)
                                && matches.len() == 1
                                && matches[0].customizable
                                && matches[0].scene_primitives > 0,
                            format!(
                                "Human material {name} must be a distinct, uniquely named, used opaque white material without color textures, emission or vertex colors"
                            ),
                        )?;
                    }
                }
                if let Some(motion) = &policy.vehicle_motion {
                    require(
                        matches!(
                            binding.target,
                            PackTarget::Kind(ActorKind::Vehicle) | PackTarget::VehicleModel(_)
                        ),
                        "Vehicle wheel motion requires a vehicle kind/model target",
                    )?;
                    require(
                        motion.wheel_radius_m.is_finite()
                            && (0.05..=3.).contains(&motion.wheel_radius_m)
                            && (2..=8).contains(&motion.wheels.len())
                            && motion.steering.len() <= 4,
                        "Vehicle motion requires a 0.05..3 m wheel radius, 2..8 wheels and at most 4 steering pivots",
                    )?;
                    let mut rig_nodes = BTreeSet::new();
                    for name in motion.wheels.iter().chain(&motion.steering) {
                        require(
                            rig_nodes.insert(name)
                                && model.nodes.iter().any(|node| node.name == *name),
                            format!(
                                "Vehicle motion node {name} must be a distinct existing named node"
                            ),
                        )?;
                    }
                    validate_vehicle_rig(model, motion)?;
                }
            }
        }
        // Hash normalized manifest content, including clip policies and every
        // model digest. JSON whitespace and file path alone are not identity.
        let identity = format!(
            "sha256:{}",
            asset_sha256(&serde_json::to_vec(&manifest).map_err(|e| e.to_string())?)
        );
        Ok(Self {
            manifest,
            identity,
            models,
        })
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn name(&self) -> &str {
        &self.manifest.name
    }
    pub fn budgets(&self) -> impl Iterator<Item = (&str, &ModelBudget)> {
        self.models
            .iter()
            .map(|(id, model)| (id.as_str(), &model.budget))
    }
}

fn validate_vehicle_rig(
    model: &ValidatedModel,
    motion: &VehicleMotionPolicy,
) -> Result<(), String> {
    let indices = |names: &[String]| -> Vec<usize> {
        names
            .iter()
            .map(|name| {
                model
                    .nodes
                    .iter()
                    .position(|node| node.name == *name)
                    .unwrap()
            })
            .collect()
    };
    let wheels = indices(&motion.wheels);
    let steering = indices(&motion.steering);
    let rig: BTreeSet<_> = wheels.iter().chain(&steering).copied().collect();
    let mut ancestors = BTreeSet::new();
    for &wheel in &wheels {
        require(
            model.parents[wheel].is_some(),
            "A wheel pivot cannot be a scene root",
        )?;
        let mut cursor = Some(wheel);
        let mut steering_count = 0;
        while let Some(node) = cursor {
            require(
                node == wheel || !wheels.contains(&node),
                "Rolling wheel pivots cannot contain other rolling wheel pivots",
            )?;
            steering_count += usize::from(steering.contains(&node));
            require(
                steering_count <= 1,
                "A wheel cannot have nested steering pivots",
            )?;
            require(
                model.nodes[node].scale == [1.; 3],
                "Apply wheel-rig ancestor scales before export; use binding scale for uniform sizing",
            )?;
            ancestors.insert(node);
            cursor = model.parents[node];
        }
    }
    for &node in &steering {
        require(
            model.parents[node].is_some() && ancestors.contains(&node),
            "Each steering pivot must be a non-root ancestor of a rolling wheel",
        )?;
    }
    for channel in model.clips.values().flat_map(|clip| &clip.channels) {
        if ancestors.contains(&channel.node) {
            require(
                rig.contains(&channel.node) && channel.property == ChannelProperty::Rotation,
                "Vehicle rig ancestors cannot be animated; wheel/steering pivots may contain preview rotation channels only",
            )?;
        }
    }
    Ok(())
}

fn array<'a>(root: &'a Value, name: &str) -> Result<&'a [Value], String> {
    match root.get(name) {
        None => Ok(&[]),
        Some(Value::Array(value)) => Ok(value),
        _ => Err(format!("{name} must be an array")),
    }
}
fn index(value: &Value, name: &str) -> Result<usize, String> {
    value
        .get(name)
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| format!("Missing or invalid {name}"))
}
fn default_index(value: &Value, name: &str, default: usize) -> Result<usize, String> {
    if value.get(name).is_none() {
        Ok(default)
    } else {
        index(value, name)
    }
}
fn numbers<const N: usize>(value: Option<&Value>, default: [f32; N]) -> Result<[f32; N], String> {
    let Some(value) = value else {
        return Ok(default);
    };
    let values = value
        .as_array()
        .filter(|a| a.len() == N)
        .ok_or("Invalid transform vector length")?;
    let mut out = default;
    for (to, from) in out.iter_mut().zip(values) {
        *to = from
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or("Transform contains non-finite values")? as f32;
        require(
            to.is_finite() && to.abs() <= 10_000.,
            "Model transform exceeds 10 km local limits",
        )?;
    }
    Ok(out)
}
#[derive(Clone, Copy)]
struct Accessor {
    start: usize,
    stride: usize,
    count: usize,
    components: usize,
    component_type: usize,
}
fn accessor(root: &Value, bin: &[u8], which: usize) -> Result<Accessor, String> {
    let accessors = array(root, "accessors")?;
    let views = array(root, "bufferViews")?;
    let a = accessors.get(which).ok_or("Accessor index out of bounds")?;
    require(
        a.get("sparse").is_none(),
        "Sparse accessors are outside the validated pack profile",
    )?;
    let view = views
        .get(index(a, "bufferView")?)
        .ok_or("Buffer view index out of bounds")?;
    require(
        index(view, "buffer")? == 0,
        "Only the embedded GLB buffer is supported",
    )?;
    let component_type = index(a, "componentType")?;
    let size = match component_type {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        _ => return Err("Unsupported accessor component type".into()),
    };
    let components = match a.get("type").and_then(Value::as_str) {
        Some("SCALAR") => 1,
        Some("VEC2") => 2,
        Some("VEC3") => 3,
        Some("VEC4") => 4,
        Some("MAT4") => 16,
        _ => return Err("Only scalar/vector/MAT4 accessors are supported".into()),
    };
    let element_bytes = size * components;
    let stride = default_index(view, "byteStride", element_bytes)?;
    let count = index(a, "count")?;
    let start = default_index(view, "byteOffset", 0)?
        .checked_add(default_index(a, "byteOffset", 0)?)
        .ok_or("Accessor overflow")?;
    let length = index(view, "byteLength")?;
    let view_end = default_index(view, "byteOffset", 0)?
        .checked_add(length)
        .ok_or("Buffer view overflow")?;
    let end = count
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|n| start.checked_add(n))
        .and_then(|n| n.checked_add(element_bytes))
        .ok_or("Accessor extent overflow")?;
    require(
        count > 0
            && count <= 1_000_000
            && stride >= element_bytes
            && stride <= 256
            && stride % size == 0
            && start % size == 0
            && end <= view_end
            && view_end <= bin.len(),
        "Accessor exceeds its buffer, alignment or allocation budget",
    )?;
    Ok(Accessor {
        start,
        stride,
        count,
        components,
        component_type,
    })
}
fn floats(a: Accessor, bin: &[u8], width: usize) -> Result<Vec<[f32; 4]>, String> {
    require(
        a.component_type == 5126 && a.components == width && width <= 4,
        "Animation/position accessor must be FLOAT with the correct vector width",
    )?;
    let mut values = Vec::with_capacity(a.count);
    for i in 0..a.count {
        let mut value = [0.; 4];
        for (component, target) in value.iter_mut().enumerate().take(width) {
            let offset = a.start + i * a.stride + component * 4;
            *target = f32::from_le_bytes(
                bin[offset..offset + 4]
                    .try_into()
                    .map_err(|_| "Truncated FLOAT")?,
            );
            require(
                target.is_finite(),
                "Accessor contains non-finite FLOAT values",
            )?;
        }
        values.push(value);
    }
    Ok(values)
}

/// Declaring no extensions is not sufficient: loaders can interpret extension
/// payloads even when an exporter omitted extensionsUsed. Extras are opaque
/// authoring metadata and never participate in glTF runtime interpretation.
fn reject_extension_payloads(value: &Value) -> Result<(), String> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if key == "extras" {
                    continue;
                }
                if key == "extensions" {
                    require(
                        value
                            .as_object()
                            .is_some_and(|extensions| extensions.is_empty()),
                        "glTF extension payloads require an explicitly extended importer, including undeclared extensions",
                    )?;
                } else {
                    reject_extension_payloads(value)?;
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                reject_extension_payloads(value)?;
            }
        }
        _ => (),
    }
    Ok(())
}

fn validate_model(bytes: Vec<u8>, entry: &PackModel) -> Result<ValidatedModel, String> {
    require(
        bytes.len() >= 28 && bytes.len() <= MAX_MODEL_BYTES && &bytes[0..4] == b"glTF",
        "Models must be self-contained GLB 2.0",
    )?;
    let u32_at =
        |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
    require(
        u32_at(4) == 2 && u32_at(8) == bytes.len() && &bytes[16..20] == b"JSON",
        "Invalid GLB version, length or JSON chunk",
    )?;
    let json_end = 20usize
        .checked_add(u32_at(12))
        .ok_or("GLB JSON length overflow")?;
    require(
        json_end + 8 <= bytes.len() && &bytes[json_end + 4..json_end + 8] == b"BIN\0",
        "A single embedded BIN chunk is required",
    )?;
    let bin_end = (json_end + 8)
        .checked_add(u32_at(json_end))
        .ok_or("GLB BIN length overflow")?;
    require(bin_end == bytes.len(), "GLB has truncated or extra chunks")?;
    let root: Value = serde_json::from_slice(&bytes[20..json_end]).map_err(|e| e.to_string())?;
    let bin = &bytes[json_end + 8..bin_end];
    require(
        array(&root, "extensionsRequired")?.is_empty()
            && array(&root, "extensionsUsed")?.is_empty(),
        "glTF extensions require an explicitly extended importer; export the core glTF profile",
    )?;
    reject_extension_payloads(&root)?;
    require(
        array(&root, "buffers")?.len() == 1
            && root["buffers"][0].get("uri").is_none()
            && index(&root["buffers"][0], "byteLength")? <= bin.len(),
        "External/data-URI buffers are not allowed",
    )?;
    let nodes = array(&root, "nodes")?;
    require(
        !nodes.is_empty() && nodes.len() <= MAX_MODEL_NODES,
        "Model requires 1..256 nodes",
    )?;
    require(
        array(&root, "meshes")?.len() <= 128
            && array(&root, "materials")?.len() <= 64
            && array(&root, "accessors")?.len() <= 2048
            && array(&root, "bufferViews")?.len() <= 2048
            && array(&root, "textures")?.len() <= 32
            && array(&root, "images")?.len() <= 16
            && array(&root, "animations")?.len() <= 32
            && array(&root, "skins")?.len() <= 16,
        "Model object count exceeds the import budget",
    )?;
    let primitive_count = array(&root, "meshes")?
        .iter()
        .try_fold(0_usize, |sum, mesh| {
            Ok::<_, String>(sum + array(mesh, "primitives")?.len())
        })?;
    let channel_count = array(&root, "animations")?
        .iter()
        .try_fold(0_usize, |sum, clip| {
            Ok::<_, String>(sum + array(clip, "channels")?.len())
        })?;
    require(
        primitive_count <= 256 && channel_count <= MAX_CHANNELS,
        "Primitive or animation-channel object budget exceeded",
    )?;
    for node in nodes {
        require(
            array(node, "children")?.len() <= MAX_MODEL_NODES,
            "Child node budget exceeded",
        )?;
    }
    for clip in array(&root, "animations")? {
        require(
            array(clip, "samplers")?.len() <= MAX_CHANNELS,
            "Animation sampler object budget exceeded",
        )?;
    }
    for skin in array(&root, "skins")? {
        require(
            array(skin, "joints")?.len() <= 128,
            "Skin joint object budget exceeded",
        )?;
    }
    // Parse the typed standard document only after cheap object budgets. The
    // library validates schema/index relationships, not binary or PBR ranges.
    gltf::Gltf::from_slice(&bytes).map_err(|e| format!("glTF validation: {e}"))?;
    for material in array(&root, "materials")? {
        let pbr = &material["pbrMetallicRoughness"];
        for values in [pbr.get("baseColorFactor"), material.get("emissiveFactor")]
            .into_iter()
            .flatten()
        {
            require(
                values.as_array().is_some_and(|v| {
                    v.iter().all(|n| {
                        n.as_f64()
                            .is_some_and(|n| n.is_finite() && (0.0..=1.0).contains(&n))
                    })
                }),
                "Material color factors must be finite values in 0..1",
            )?;
        }
        for value in [
            pbr.get("metallicFactor"),
            pbr.get("roughnessFactor"),
            material["occlusionTexture"].get("strength"),
            material.get("alphaCutoff"),
        ]
        .into_iter()
        .flatten()
        {
            require(
                value
                    .as_f64()
                    .is_some_and(|n| n.is_finite() && (0.0..=1.0).contains(&n)),
                "Material metalness, roughness, occlusion and alpha cutoff must be in 0..1",
            )?;
        }
        if let Some(scale) = material["normalTexture"].get("scale") {
            require(
                scale
                    .as_f64()
                    .is_some_and(|n| n.is_finite() && n.abs() <= 16.),
                "Normal texture scale must be finite and within -16..16",
            )?;
        }
    }
    let mut accessors = Vec::new();
    let mut decoded_scalars = 0_usize;
    for i in 0..array(&root, "accessors")?.len() {
        let a = accessor(&root, bin, i)?;
        decoded_scalars = decoded_scalars
            .checked_add(
                a.count
                    .checked_mul(a.components)
                    .ok_or("Accessor scalar count overflow")?,
            )
            .ok_or("Accessor scalar budget overflow")?;
        require(
            decoded_scalars <= MAX_MODEL_DECODED_SCALARS,
            "Aggregate accessor scalar budget exceeds 4,000,000, including aliased and unused accessors",
        )?;
        accessors.push(a);
    }
    // Admission above precedes every binary scan. Aliasing the same BIN range
    // through thousands of accessors must not amplify import CPU unboundedly.
    for a in accessors {
        // Check binary payloads as well as JSON. This covers normals, tangents,
        // UVs, colors and inverse-bind matrices, including unused accessors.
        if a.component_type == 5126 {
            for row in 0..a.count {
                for component in 0..a.components {
                    let offset = a.start + row * a.stride + component * 4;
                    let value = f32::from_le_bytes(bin[offset..offset + 4].try_into().unwrap());
                    require(
                        value.is_finite() && value.abs() <= 1_000_000.,
                        "FLOAT payload contains non-finite or unbounded values",
                    )?;
                }
            }
        }
    }
    let mut parents = vec![None; nodes.len()];
    let mut names = BTreeSet::new();
    let mut model_nodes = Vec::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        let name = node
            .get("name")
            .and_then(Value::as_str)
            .ok_or("Every node needs a unique name")?;
        require(
            short_text(name, 96) && names.insert(name),
            "Node names must be unique bounded plain text",
        )?;
        require(
            node.get("matrix").is_none()
                && node.get("camera").is_none()
                && node.get("weights").is_none(),
            "Export node TRS; cameras, matrices and morph targets are outside this profile",
        )?;
        let translation = numbers(node.get("translation"), [0.; 3])?;
        let rotation = numbers(node.get("rotation"), [0., 0., 0., 1.])?;
        let scale = numbers(node.get("scale"), [1.; 3])?;
        require(
            (rotation.iter().map(|v| v * v).sum::<f32>() - 1.).abs() <= 0.01
                && scale.iter().all(|s| s.abs() >= 0.0001 && s.abs() <= 100.),
            "Node rotations must be unit quaternions and scales must be nonzero and bounded",
        )?;
        model_nodes.push(ModelNode {
            name: name.into(),
            translation,
            rotation,
            scale,
        });
        for child in array(node, "children")? {
            let child = child
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or("Invalid child node")?;
            require(
                child < nodes.len() && child != i && parents[child].is_none(),
                "Model hierarchy has a cycle, duplicate child or multiple parents",
            )?;
            parents[child] = Some(i);
        }
    }
    for i in 0..nodes.len() {
        let mut cursor = Some(i);
        let mut depth = 0;
        while let Some(node) = cursor {
            depth += 1;
            require(
                depth <= 32,
                "Model hierarchy is cyclic or deeper than 32 nodes",
            )?;
            cursor = parents[node];
        }
    }
    let scenes = array(&root, "scenes")?;
    require(
        scenes.len() == 1 && entry.scene == 0,
        "Validated packs contain exactly one scene, indexed 0",
    )?;
    let mut reached = BTreeSet::new();
    let mut stack = Vec::new();
    for value in array(&scenes[0], "nodes")? {
        let node = value
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .ok_or("Invalid scene root")?;
        require(
            node < nodes.len() && parents[node].is_none(),
            "Scene root has a parent or invalid index",
        )?;
        stack.push(node);
    }
    while let Some(node) = stack.pop() {
        require(reached.insert(node), "Duplicate scene root")?;
        for child in array(&nodes[node], "children")? {
            stack.push(child.as_u64().unwrap() as usize);
        }
    }
    require(
        reached.len() == nodes.len(),
        "All nodes must be reachable in the selected scene",
    )?;
    require(
        nodes.iter().any(|node| {
            node.get("mesh")
                .and_then(Value::as_u64)
                .and_then(|which| root["meshes"].get(which as usize))
                .is_some_and(|mesh| {
                    mesh["primitives"]
                        .as_array()
                        .is_some_and(|primitives| !primitives.is_empty())
                })
        }),
        "Selected scene has no drawable mesh instances",
    )?;
    let mut budget = ModelBudget {
        nodes: nodes.len(),
        bytes: bytes.len(),
        decoded_scalars,
        primitives: primitive_count,
        materials: array(&root, "materials")?.len(),
        ..Default::default()
    };
    let mut total_primitives = 0;
    let mut mesh_radii = vec![0_f64; array(&root, "meshes")?.len()];
    let mut mesh_vertices = vec![0_usize; mesh_radii.len()];
    let mut mesh_triangles = vec![0_usize; mesh_radii.len()];
    let mut mesh_renderer_vertices = vec![0_usize; mesh_radii.len()];
    for (mesh_index, mesh) in array(&root, "meshes")?.iter().enumerate() {
        require(
            mesh.get("weights").is_none(),
            "Morph targets are outside the validated profile",
        )?;
        for primitive in array(mesh, "primitives")? {
            total_primitives += 1;
            require(
                default_index(primitive, "mode", 4)? == 4
                    && primitive.get("targets").is_none()
                    && total_primitives <= 256,
                "Only triangle meshes without morph targets are supported (256 primitive maximum)",
            )?;
            let attributes = primitive
                .get("attributes")
                .and_then(Value::as_object)
                .ok_or("Missing mesh attributes")?;
            let positions = accessor(&root, bin, index(&primitive["attributes"], "POSITION")?)?;
            budget.vertices += positions.count;
            mesh_vertices[mesh_index] += positions.count;
            require(
                budget.vertices <= MAX_MODEL_VERTICES,
                "Model exceeds 100,000 vertices",
            )?;
            for position in floats(positions, bin, 3)? {
                require(
                    position[..3].iter().all(|v| v.abs() <= 1000.),
                    "Mesh position exceeds local presentation bounds",
                )?;
                mesh_radii[mesh_index] = mesh_radii[mesh_index].max(
                    position[..3]
                        .iter()
                        .map(|v| f64::from(*v).powi(2))
                        .sum::<f64>()
                        .sqrt(),
                );
            }
            for (semantic, which) in attributes {
                let which = which.as_u64().ok_or("Invalid mesh attribute")? as usize;
                let a = accessor(&root, bin, which)?;
                require(
                    a.count == positions.count,
                    format!("{semantic} vertex count differs from POSITION"),
                )?;
            }
            let indices = if primitive.get("indices").is_some() {
                let a = accessor(&root, bin, index(primitive, "indices")?)?;
                require(
                    a.components == 1 && matches!(a.component_type, 5121 | 5123 | 5125),
                    "Triangle indices must be unsigned scalars",
                )?;
                require(
                    a.count.is_multiple_of(3) && budget.triangles + a.count / 3 <= 100_000,
                    "Triangle scan exceeds 100,000 triangle budget",
                )?;
                for i in 0..a.count {
                    let start = a.start + i * a.stride;
                    let value = match a.component_type {
                        5121 => usize::from(bin[start]),
                        5123 => usize::from(u16::from_le_bytes(
                            bin[start..start + 2].try_into().unwrap(),
                        )),
                        _ => u32::from_le_bytes(bin[start..start + 4].try_into().unwrap()) as usize,
                    };
                    require(value < positions.count, "Mesh index exceeds vertex count")?;
                }
                a.count
            } else {
                positions.count
            };
            require(
                indices % 3 == 0,
                "Triangle index count must be divisible by three",
            )?;
            budget.triangles += indices / 3;
            mesh_triangles[mesh_index] += indices / 3;
            let renderer_vertices = if attributes.contains_key("NORMAL") {
                positions.count
            } else {
                indices
            };
            budget.renderer_vertices += renderer_vertices;
            mesh_renderer_vertices[mesh_index] += renderer_vertices;
            require(
                budget.triangles <= 100_000,
                "Model exceeds 100,000 triangles",
            )?;
        }
    }
    require(
        budget.vertices > 0 && budget.triangles > 0 && budget.triangles <= 100_000,
        "Model needs visible geometry within 100,000 triangles",
    )?;
    for node in nodes {
        if node.get("mesh").is_none() {
            continue;
        }
        let mesh = index(node, "mesh")?;
        budget.scene_vertices += mesh_vertices[mesh];
        budget.scene_triangles += mesh_triangles[mesh];
        budget.scene_renderer_vertices += mesh_renderer_vertices[mesh];
        budget.scene_primitives += array(&root["meshes"][mesh], "primitives")?.len();
        require(
            budget.scene_vertices <= MAX_MODEL_VERTICES && budget.scene_triangles <= 100_000,
            "Instantiated scene exceeds 100,000 vertices or triangles; repeated mesh nodes count separately",
        )?;
    }
    let mut image_pixels = Vec::new();
    for (image_index, image) in array(&root, "images")?.iter().enumerate() {
        require(
            image.get("uri").is_none()
                && image.get("mimeType").and_then(Value::as_str) == Some("image/png"),
            "Only embedded PNG textures are supported",
        )?;
        let view = array(&root, "bufferViews")?
            .get(index(image, "bufferView")?)
            .ok_or("Invalid image buffer view")?;
        let start = default_index(view, "byteOffset", 0)?;
        let length = index(view, "byteLength")?;
        let end = start.checked_add(length).ok_or("Image buffer overflow")?;
        require(
            index(view, "buffer")? == 0 && end <= bin.len() && length >= 24,
            "Invalid PNG buffer extent",
        )?;
        let png = &bin[start..end];
        require(
            &png[..8] == b"\x89PNG\r\n\x1a\n" && &png[12..16] == b"IHDR",
            "Invalid PNG header",
        )?;
        let w = u32::from_be_bytes(png[16..20].try_into().unwrap()) as usize;
        let h = u32::from_be_bytes(png[20..24].try_into().unwrap()) as usize;
        require(
            (1..=4096).contains(&w) && (1..=4096).contains(&h),
            "Texture dimensions must be 1..4096",
        )?;
        budget.texture_pixels += w * h;
        image_pixels.push(w * h);
        require(
            budget.texture_pixels <= 16_777_216,
            "Decoded texture budget exceeds 16 megapixels",
        )?;
        // Header-only admission accepts truncated or corrupt compressed image
        // data. Decode once during preflight, with PNG internal allocation and
        // output limits installed before its decoder reads metadata or pixels.
        let mut reader = ImageReader::with_format(Cursor::new(png), ImageFormat::Png);
        let mut limits = Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(MAX_TEXTURE_DECODE_BYTES);
        reader.limits(limits);
        let decoded = reader.decode().map_err(|error| {
            format!("PNG texture {image_index} failed bounded decoding: {error}")
        })?;
        require(
            decoded.width() as usize == w && decoded.height() as usize == h,
            "Decoded PNG dimensions differ from the admitted header",
        )?;
    }
    for texture in array(&root, "textures")? {
        budget.texture_slot_pixels += *image_pixels
            .get(index(texture, "source")?)
            .ok_or("Texture references an invalid PNG")?;
    }
    for skin in array(&root, "skins")? {
        let joints = array(skin, "joints")?;
        let joint_ids: BTreeSet<_> = joints.iter().filter_map(Value::as_u64).collect();
        require(
            !joints.is_empty()
                && joints.len() <= 128
                && joint_ids.len() == joints.len()
                && joint_ids.iter().all(|i| *i < nodes.len() as u64),
            "Skin requires 1..128 distinct valid joints",
        )?;
        if skin.get("inverseBindMatrices").is_some() {
            let a = accessor(&root, bin, index(skin, "inverseBindMatrices")?)?;
            require(
                a.component_type == 5126 && a.components == 16 && a.count == joints.len(),
                "Inverse-bind matrix count must match joints",
            )?;
        }
    }
    let mut skinned_vertices = 0_usize;
    for node in nodes {
        let Some(skin_index) = node.get("skin") else {
            continue;
        };
        let skin = array(&root, "skins")?
            .get(skin_index.as_u64().ok_or("Invalid skin index")? as usize)
            .ok_or("Skin index out of bounds")?;
        let joint_count = array(skin, "joints")?.len();
        let mesh = array(&root, "meshes")?
            .get(index(node, "mesh")?)
            .ok_or("Skinned node has no valid mesh")?;
        for primitive in array(mesh, "primitives")? {
            let attributes = &primitive["attributes"];
            require(
                attributes.get("JOINTS_1").is_none() && attributes.get("WEIGHTS_1").is_none(),
                "Only four skin influences per vertex are supported",
            )?;
            let joints = accessor(&root, bin, index(attributes, "JOINTS_0")?)?;
            let weights = accessor(&root, bin, index(attributes, "WEIGHTS_0")?)?;
            require(
                joints.components == 4
                    && matches!(joints.component_type, 5121 | 5123)
                    && weights.component_type == 5126
                    && weights.components == 4
                    && weights.count == joints.count,
                "Skin requires unsigned-byte/short JOINTS_0 and FLOAT VEC4 WEIGHTS_0",
            )?;
            skinned_vertices += joints.count;
            require(
                skinned_vertices <= MAX_MODEL_VERTICES,
                "Instanced skin validation exceeds vertex budget",
            )?;
            for row in 0..joints.count {
                for component in 0..4 {
                    let start = joints.start
                        + row * joints.stride
                        + component * if joints.component_type == 5121 { 1 } else { 2 };
                    let joint = if joints.component_type == 5121 {
                        usize::from(bin[start])
                    } else {
                        usize::from(u16::from_le_bytes(
                            bin[start..start + 2].try_into().unwrap(),
                        ))
                    };
                    require(
                        joint < joint_count,
                        "JOINTS_0 index exceeds referenced skin joint count",
                    )?;
                }
            }
            for weights in floats(weights, bin, 4)? {
                require(
                    weights.iter().all(|v| (0.0..=1.0).contains(v))
                        && (weights.iter().sum::<f32>() - 1.).abs() <= 0.01,
                    "Skin weights must be finite, nonnegative and sum to one; zero-weight vertices are rejected",
                )?;
            }
        }
    }
    let mut clips = BTreeMap::new();
    for clip in array(&root, "animations")? {
        let name = clip
            .get("name")
            .and_then(Value::as_str)
            .ok_or("All animation clips need names")?;
        require(
            short_text(name, 96) && !clips.contains_key(name),
            "Animation names must be unique and bounded",
        )?;
        budget.clips += 1;
        let samplers = array(clip, "samplers")?;
        require(
            !samplers.is_empty() && samplers.len() <= MAX_CHANNELS,
            "Invalid animation sampler count",
        )?;
        // Count and validate EVERY sampler in EVERY clip, even if no binding or
        // channel references it. Bevy imports the whole GLB, not only our clips.
        for sampler in samplers {
            let input = accessor(&root, bin, index(sampler, "input")?)?;
            let output = accessor(&root, bin, index(sampler, "output")?)?;
            budget.animation_keys += input.count + output.count;
            require(
                budget.animation_keys <= MAX_MODEL_ANIMATION_KEYS,
                "Animation import exceeds 120,000 keys, including unused clips/samplers",
            )?;
            require(
                input.component_type == 5126
                    && input.components == 1
                    && input.count >= 2
                    && output.component_type == 5126
                    && matches!(output.components, 3 | 4)
                    && output.count == input.count,
                "Animation sampler input/output counts or types are invalid",
            )?;
            let interpolation = sampler
                .get("interpolation")
                .and_then(Value::as_str)
                .unwrap_or("LINEAR");
            require(
                matches!(interpolation, "LINEAR" | "STEP"),
                "Export linear/step animation; cubic splines require baking",
            )?;
            let times = floats(input, bin, 1)?;
            require(
                times[0][0] == 0.
                    && times.last().unwrap()[0] <= 600.
                    && times.windows(2).all(|p| p[0][0] < p[1][0]),
                "Animation times must start at zero, increase, and fit 600 seconds",
            )?;
            let _ = floats(output, bin, output.components)?;
        }
        let mut channels = Vec::new();
        let mut targets = BTreeSet::new();
        let mut duration = 0_f32;
        for channel in array(clip, "channels")? {
            budget.channels += 1;
            require(
                budget.channels <= MAX_CHANNELS,
                "Model exceeds 512 animation channels",
            )?;
            let node = index(&channel["target"], "node")?;
            require(node < nodes.len(), "Animation node out of bounds")?;
            let property = match channel["target"]["path"].as_str() {
                Some("translation") => ChannelProperty::Translation,
                Some("rotation") => ChannelProperty::Rotation,
                Some("scale") => ChannelProperty::Scale,
                _ => {
                    return Err(
                        "Only node translation, rotation and scale animations are supported".into(),
                    );
                }
            };
            require(
                targets.insert((node, property)),
                "Duplicate animation target within a clip",
            )?;
            let sampler = samplers
                .get(index(channel, "sampler")?)
                .ok_or("Animation sampler index out of bounds")?;
            let input = accessor(&root, bin, index(sampler, "input")?)?;
            let output = accessor(&root, bin, index(sampler, "output")?)?;
            budget.runtime_animation_keys += input.count + output.count;
            require(
                budget.runtime_animation_keys <= MAX_MODEL_ANIMATION_KEYS,
                "Runtime channel allocation exceeds 120,000 keys, including aliased samplers",
            )?;
            let times: Vec<_> = floats(input, bin, 1)?.into_iter().map(|v| v[0]).collect();
            let width = if property == ChannelProperty::Rotation {
                4
            } else {
                3
            };
            let values = floats(output, bin, width)?;
            for value in &values {
                require(
                    value[..width].iter().all(|v| v.abs() <= 10_000.),
                    "Animation transform exceeds local presentation limits",
                )?;
                if property == ChannelProperty::Rotation {
                    require(
                        (value.iter().map(|v| v * v).sum::<f32>() - 1.).abs() <= 0.01,
                        "Animation rotations must be unit quaternions",
                    )?;
                } else if property == ChannelProperty::Scale {
                    require(
                        value[..3].iter().enumerate().all(|(i, v)| {
                            v.abs() >= 0.0001
                                && v.abs() <= 100.
                                && v.is_sign_positive()
                                    == model_nodes[node].scale[i].is_sign_positive()
                        }),
                        "Animated scales must remain nonzero, bounded and keep their rest-pose sign across every clip",
                    )?;
                }
            }
            duration = duration.max(*times.last().unwrap());
            channels.push(ModelChannel {
                node,
                property,
                step: sampler.get("interpolation").and_then(Value::as_str) == Some("STEP"),
                times,
                values,
            });
        }
        require(
            !channels.is_empty(),
            "Animation clips must contain channels",
        )?;
        clips.insert(name.into(), ModelClip { duration, channels });
    }
    let norm = |value: &[f32]| {
        value
            .iter()
            .map(|v| f64::from(*v).powi(2))
            .sum::<f64>()
            .sqrt()
    };
    let mut scale_bounds: Vec<f64> = model_nodes
        .iter()
        .map(|node| {
            node.scale
                .iter()
                .map(|v| f64::from(v.abs()))
                .fold(0., f64::max)
        })
        .collect();
    let mut translation_bounds: Vec<f64> = model_nodes
        .iter()
        .map(|node| norm(&node.translation))
        .collect();
    for clip in clips.values() {
        for channel in &clip.channels {
            for value in &channel.values {
                match channel.property {
                    ChannelProperty::Scale => {
                        scale_bounds[channel.node] = scale_bounds[channel.node].max(
                            value[..3]
                                .iter()
                                .map(|v| f64::from(v.abs()))
                                .fold(0., f64::max),
                        )
                    }
                    ChannelProperty::Translation => {
                        translation_bounds[channel.node] =
                            translation_bounds[channel.node].max(norm(&value[..3]))
                    }
                    ChannelProperty::Rotation => {}
                }
            }
        }
    }
    // Bounds include every clip and the rest pose, so crossfades cannot leave
    // the admitted envelope. This prevents finite local scales multiplying to
    // non-finite GlobalTransforms when a valid-looking hierarchy is spawned.
    for (i, node) in nodes.iter().enumerate() {
        let mut radius = if node.get("mesh").is_some() {
            mesh_radii[index(node, "mesh")?]
        } else {
            0.
        };
        let mut scale = 1_f64;
        let mut cursor = Some(i);
        while let Some(node) = cursor {
            radius = translation_bounds[node] + scale_bounds[node] * radius;
            scale *= scale_bounds[node];
            require(
                radius.is_finite() && radius <= 10_000. && scale.is_finite() && scale <= 1000.,
                "Composed hierarchy/animation envelope exceeds 10 km or 1000x scale",
            )?;
            cursor = parents[node];
        }
    }
    let mut asset_labels = vec![
        String::new(),
        "Scene0".into(),
        "DefaultMaterial".into(),
        "DefaultMaterial/std".into(),
    ];
    for i in 0..nodes.len() {
        asset_labels.push(format!("Node{i}"));
    }
    for (i, mesh) in array(&root, "meshes")?.iter().enumerate() {
        asset_labels.push(format!("Mesh{i}"));
        for j in 0..array(mesh, "primitives")?.len() {
            asset_labels.push(format!("Mesh{i}/Primitive{j}"));
        }
    }
    for i in 0..array(&root, "textures")?.len() {
        asset_labels.push(format!("Texture{i}"));
    }
    for i in 0..budget.materials {
        for suffix in ["", " (inverted)"] {
            asset_labels.push(format!("Material{i}{suffix}"));
            asset_labels.push(format!("Material{i}{suffix}/std"));
        }
    }
    for i in 0..array(&root, "skins")?.len() {
        asset_labels.push(format!("Skin{i}"));
        asset_labels.push(format!("Skin{i}/InverseBindMatrices"));
    }
    let materials = array(&root, "materials")?
        .iter()
        .enumerate()
        .map(|(index, material)| {
            let pbr = &material["pbrMetallicRoughness"];
            let mut scene_primitives = 0;
            let mut vertex_colors = false;
            for node in nodes {
                if let Some(mesh) = node.get("mesh").and_then(Value::as_u64) {
                    if let Some(primitives) = root["meshes"][mesh as usize]["primitives"].as_array()
                    {
                        for primitive in primitives {
                            if primitive.get("material").and_then(Value::as_u64)
                                == Some(index as u64)
                            {
                                scene_primitives += 1;
                                vertex_colors |= primitive["attributes"].get("COLOR_0").is_some();
                            }
                        }
                    }
                }
            }
            ModelMaterial {
                name: material
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                customizable: material
                    .get("alphaMode")
                    .and_then(Value::as_str)
                    .unwrap_or("OPAQUE")
                    == "OPAQUE"
                    && numbers::<4>(pbr.get("baseColorFactor"), [1.; 4])
                        .is_ok_and(|v| v == [1.; 4])
                    && pbr.get("baseColorTexture").is_none()
                    && material.get("emissiveTexture").is_none()
                    && numbers::<3>(material.get("emissiveFactor"), [0.; 3])
                        .is_ok_and(|v| v == [0.; 3])
                    && !vertex_colors,
                scene_primitives,
            }
        })
        .collect();
    Ok(ValidatedModel {
        bytes,
        hash: entry.sha256.clone(),
        scene: entry.scene,
        budget,
        nodes: model_nodes,
        parents,
        clips,
        asset_labels,
        materials,
    })
}

#[cfg(test)]
#[path = "asset_pack_tests.rs"]
mod tests;
