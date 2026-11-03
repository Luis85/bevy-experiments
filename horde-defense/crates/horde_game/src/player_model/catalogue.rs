//! Authored source/bind/skin contracts. No image decoder, renderer or game mutation.
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDefinition {
    pub id: String,
    pub path: String,
    pub png_sha256: String,
    pub rgba_fnv1a64: String,
    pub size: [u32; 2],
    pub tile: [u32; 4],
    pub layout_frame: [u32; 4],
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MaskMode {
    Runs,
    PriorityPolygons,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceChannel {
    VisibleOriginal,
    HiddenFill,
    Residue,
    SkinReplacement,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoneBind {
    pub id: String,
    pub parent: Option<String>,
    pub bind_origin: [f32; 2],
    pub bind_tip: Option<[f32; 2]>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocketDefinition {
    pub id: String,
    pub bone: String,
    pub bind_position: [f32; 2],
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartDefinition {
    pub id: String,
    pub bone: String,
    pub slot: String,
    pub material: String,
    pub draw_order: i32,
    pub channel: SourceChannel,
    #[serde(default)]
    pub source_runs: Vec<[u32; 3]>,
    #[serde(default)]
    pub authoring_polygons: Vec<Vec<[f32; 2]>>,
    #[serde(default)]
    pub priority: i32,
    pub remainder_max_alpha: Option<u8>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelDefinition {
    pub schema_version: u32,
    pub id: String,
    pub source: SourceDefinition,
    pub ground: [f32; 2],
    pub root_bone: String,
    pub default_skin: String,
    pub mask_mode: MaskMode,
    pub slots: Vec<String>,
    pub materials: Vec<String>,
    pub bones: Vec<BoneBind>,
    pub parts: Vec<PartDefinition>,
    pub sockets: Vec<SocketDefinition>,
    pub occlusion: Vec<[String; 2]>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentRequirement {
    pub slot: String,
    pub style: Option<String>,
    pub weapon_class: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkinDefinition {
    pub id: String,
    pub model: String,
    pub parts: Vec<String>,
    pub equipment: Vec<EquipmentRequirement>,
    #[serde(default)]
    pub replace_parts: Vec<String>,
    #[serde(default)]
    pub attachments: Vec<String>,
    pub hold: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SkinsDocument {
    schema_version: u32,
    skins: Vec<SkinDefinition>,
    #[serde(default)]
    sources: Vec<super::attachments::SourceAssetDefinition>,
    #[serde(default)]
    attachments: Vec<super::attachments::AttachmentDefinition>,
    #[serde(default)]
    holds: Vec<super::attachments::HoldDefinition>,
    #[serde(default)]
    pose_tracks: Vec<super::attachments::PoseTrack>,
    #[serde(default)]
    support_tracks: Vec<super::attachments::SupportTrack>,
}
#[derive(Clone, Debug)]
pub struct PlayerModelCatalogue {
    pub models: Vec<ModelDefinition>,
    pub skins: Vec<SkinDefinition>,
    pub sources: Vec<super::attachments::SourceAssetDefinition>,
    pub attachments: Vec<super::attachments::AttachmentDefinition>,
    pub holds: Vec<super::attachments::HoldDefinition>,
    pub pose_tracks: Vec<super::attachments::PoseTrack>,
    pub support_tracks: Vec<super::attachments::SupportTrack>,
}

impl PlayerModelCatalogue {
    pub fn from_json(models: &[&str], skins: &str) -> Result<Self, String> {
        Self::from_documents(models, &[skins])
    }
    pub fn from_documents(models: &[&str], skin_documents: &[&str]) -> Result<Self, String> {
        if models.is_empty()
            || models.len() > 32
            || models.iter().any(|s| s.len() > 16 * 1024 * 1024)
            || skin_documents.is_empty()
            || skin_documents.len() > 32
            || skin_documents.iter().any(|s| s.len() > 16 * 1024 * 1024)
        {
            return Err("Player model catalogue input budget exceeded".into());
        }
        let models = models
            .iter()
            .map(|text| serde_json::from_str::<ModelDefinition>(text).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut result = Self {
            models,
            skins: vec![],
            sources: vec![],
            attachments: vec![],
            holds: vec![],
            pose_tracks: vec![],
            support_tracks: vec![],
        };
        for text in skin_documents {
            let document: SkinsDocument = serde_json::from_str(text).map_err(|e| e.to_string())?;
            if document.schema_version != 1 {
                return Err("Unsupported player skin schema version".into());
            }
            result.skins.extend(document.skins);
            result.sources.extend(document.sources);
            result.attachments.extend(document.attachments);
            result.holds.extend(document.holds);
            result.pose_tracks.extend(document.pose_tracks);
            result.support_tracks.extend(document.support_tracks);
        }
        result.validate()?;
        Ok(result)
    }
    pub fn model(&self, id: &str) -> Option<&ModelDefinition> {
        self.models.iter().find(|model| model.id == id)
    }
    pub fn skin(&self, id: &str) -> Option<&SkinDefinition> {
        self.skins.iter().find(|skin| skin.id == id)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.models.is_empty() || self.models.len() > 32 {
            return Err("Invalid player model count".into());
        }
        unique(self.models.iter().map(|v| v.id.as_str()), "model")?;
        unique(self.skins.iter().map(|v| v.id.as_str()), "skin")?;
        if self.skins.len() > 256 {
            return Err("Too many player skins".into());
        }
        let mut sources: BTreeMap<&str, &SourceDefinition> = BTreeMap::new();
        for model in &self.models {
            validate_model(model)?;
            if let Some(old) = sources.insert(&model.source.id, &model.source)
                && (old.path != model.source.path
                    || old.size != model.source.size
                    || old.png_sha256 != model.source.png_sha256
                    || old.rgba_fnv1a64 != model.source.rgba_fnv1a64)
            {
                return Err(format!(
                    "Conflicting shared player source {}",
                    model.source.id
                ));
            }
            let skin = self
                .skin(&model.default_skin)
                .ok_or_else(|| format!("Missing default skin {}", model.default_skin))?;
            if skin.model != model.id {
                return Err(format!("Default skin model mismatch for {}", model.id));
            }
        }
        let source_pixels: u64 = sources
            .values()
            .map(|s| u64::from(s.size[0]) * u64::from(s.size[1]))
            .sum();
        let tile_pixels: u64 = self
            .models
            .iter()
            .map(|m| {
                u64::from(m.source.tile[2] - m.source.tile[0])
                    * u64::from(m.source.tile[3] - m.source.tile[1])
            })
            .sum();
        if source_pixels > 32_000_000 || tile_pixels > 8_000_000 {
            return Err("Aggregate player source/ownership budget exceeded".into());
        }
        for skin in &self.skins {
            let model = self
                .model(&skin.model)
                .ok_or_else(|| format!("Unknown skin model {}", skin.model))?;
            unique(skin.parts.iter().map(String::as_str), "skin part")?;
            for id in &skin.parts {
                let part = model
                    .parts
                    .iter()
                    .find(|part| part.id == *id)
                    .ok_or_else(|| format!("Unknown skin part {id}"))?;
                if skin.id == model.default_skin && part.channel == SourceChannel::HiddenFill {
                    return Err("Default bind skin cannot select hidden fill".into());
                }
            }
            if skin.id == model.default_skin
                && model.parts.iter().any(|part| {
                    part.channel != SourceChannel::HiddenFill && !skin.parts.contains(&part.id)
                })
            {
                return Err(format!("Default skin {} omits original owners", skin.id));
            }
            unique(
                skin.equipment.iter().map(|e| e.slot.as_str()),
                "skin equipment slot",
            )?;
            for e in &skin.equipment {
                if !["ranged", "melee", "outfit", "accessory"].contains(&e.slot.as_str())
                    || e.style
                        .as_ref()
                        .is_some_and(|s| s.is_empty() || s.len() > 96)
                    || e.weapon_class.as_ref().is_some_and(|c| {
                        ![
                            "Pistol",
                            "Smg",
                            "Shotgun",
                            "Rifle",
                            "Marksman",
                            "MachineGun",
                        ]
                        .contains(&c.as_str())
                    })
                {
                    return Err(format!("Invalid skin equipment requirement {}", skin.id));
                }
            }
        }
        super::attachments::validate_extra(self)?;
        Ok(())
    }
}
pub(super) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}
fn unique<'a>(ids: impl Iterator<Item = &'a str>, kind: &str) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !valid_id(id) || !seen.insert(id) {
            return Err(format!("Invalid/duplicate {kind} ID {id}"));
        }
    }
    Ok(())
}
fn in_rect(p: [f32; 2], r: [u32; 4]) -> bool {
    p.iter().all(|v| v.is_finite())
        && p[0] >= r[0] as f32
        && p[0] <= r[2] as f32
        && p[1] >= r[1] as f32
        && p[1] <= r[3] as f32
}
fn validate_model(model: &ModelDefinition) -> Result<(), String> {
    let source = &model.source;
    let tile = source.tile;
    let frame = source.layout_frame;
    if model.schema_version != 1
        || !valid_id(&source.id)
        || source.path.is_empty()
        || source.path.len() > 512
        || source.path.contains("..")
        || (source.path.starts_with('/') || source.path.starts_with('\\'))
        || source.path.contains(':')
        || source.size.iter().any(|v| *v == 0 || *v > 8192)
        || u64::from(source.size[0]) * u64::from(source.size[1]) > 32_000_000
        || tile[0] >= tile[2]
        || tile[1] >= tile[3]
        || tile[2] > source.size[0]
        || tile[3] > source.size[1]
        || frame[0] < tile[0]
        || frame[1] < tile[1]
        || frame[2] > tile[2]
        || frame[3] > tile[3]
        || frame[0] >= frame[2]
        || frame[1] >= frame[3]
        || source.png_sha256.len() != 64
        || !source.png_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || source.rgba_fnv1a64.len() != 16
        || !source.rgba_fnv1a64.bytes().all(|b| b.is_ascii_hexdigit())
        || !in_rect(model.ground, tile)
    {
        return Err(format!("Invalid source/frame for {}", model.id));
    }
    if model.parts.is_empty()
        || model.parts.len() > 512
        || model.bones.is_empty()
        || model.bones.len() > 128
        || model.sockets.len() > 128
        || model.slots.len() > 128
        || model.materials.len() > 128
        || model.occlusion.len() > 4096
    {
        return Err(format!("Model definition budget exceeded {}", model.id));
    }
    unique(model.parts.iter().map(|p| p.id.as_str()), "part")?;
    unique(model.bones.iter().map(|b| b.id.as_str()), "bone")?;
    unique(model.slots.iter().map(String::as_str), "slot")?;
    unique(model.materials.iter().map(String::as_str), "material")?;
    unique(model.sockets.iter().map(|s| s.id.as_str()), "socket")?;
    let bones: BTreeMap<_, _> = model.bones.iter().map(|b| (b.id.as_str(), b)).collect();
    let root = bones
        .get(model.root_bone.as_str())
        .ok_or("Unknown root bone")?;
    if root.parent.is_some() {
        return Err("Root bone has a parent".into());
    }
    for bone in &model.bones {
        if !in_rect(bone.bind_origin, tile) || bone.bind_tip.is_some_and(|p| !in_rect(p, tile)) {
            return Err(format!("Invalid bone anchor {}", bone.id));
        }
        let mut chain = BTreeSet::new();
        let mut cursor = bone;
        while let Some(parent) = &cursor.parent {
            if !chain.insert(cursor.id.as_str()) {
                return Err("Bone hierarchy cycle".into());
            }
            cursor = bones
                .get(parent.as_str())
                .ok_or_else(|| format!("Unknown parent bone {parent}"))?;
        }
        if cursor.id != model.root_bone {
            return Err("Disconnected bone root".into());
        }
    }
    let mut remainder = 0;
    let mut total_runs = 0;
    let mut points = 0;
    for part in &model.parts {
        if part.channel == SourceChannel::SkinReplacement {
            return Err("Skin replacement belongs in an additive attachment document".into());
        }
        if !bones.contains_key(part.bone.as_str())
            || !model.slots.contains(&part.slot)
            || !model.materials.contains(&part.material)
            || part.draw_order.unsigned_abs() > 1_000_000
        {
            return Err(format!("Unknown part binding {}", part.id));
        }
        if let Some(max_alpha) = part.remainder_max_alpha {
            remainder += 1;
            if part.channel != SourceChannel::Residue
                || max_alpha > 6
                || !part.source_runs.is_empty()
                || !part.authoring_polygons.is_empty()
            {
                return Err("Invalid residue fallback".into());
            }
        }
        if part.channel == SourceChannel::Residue && part.remainder_max_alpha.is_none() {
            return Err("Residue requires an explicit faint-alpha limit".into());
        }
        if model.mask_mode == MaskMode::Runs && !part.authoring_polygons.is_empty()
            || model.mask_mode == MaskMode::PriorityPolygons && !part.source_runs.is_empty()
        {
            return Err("Mixed ownership authoring modes".into());
        }
        total_runs += part.source_runs.len();
        for [y, x0, x1] in &part.source_runs {
            if *y < tile[1] || *y >= tile[3] || *x0 < tile[0] || *x1 > tile[2] || x0 >= x1 {
                return Err(format!("Out-of-bounds mask run {}", part.id));
            }
        }
        for polygon in &part.authoring_polygons {
            points += polygon.len();
            if polygon.len() < 3
                || polygon.len() > 4096
                || polygon.iter().any(|p| !in_rect(*p, tile))
            {
                return Err(format!("Invalid source polygon {}", part.id));
            }
        }
    }
    if remainder > 1 || total_runs > 150_000 || points > 100_000 {
        return Err("Player source mask budget exceeded".into());
    }
    for socket in &model.sockets {
        if !bones.contains_key(socket.bone.as_str()) || !in_rect(socket.bind_position, tile) {
            return Err(format!("Invalid socket {}", socket.id));
        }
    }
    for id in [
        "muzzle",
        "throw_origin",
        "palm.near",
        "palm.far",
        "wrist.near",
        "wrist.far",
    ] {
        if !model.sockets.iter().any(|s| s.id == id) {
            return Err(format!("Missing required player socket {id}"));
        }
    }
    let parts: BTreeMap<_, _> = model.parts.iter().map(|p| (p.id.as_str(), p)).collect();
    let mut edges = BTreeSet::new();
    for [behind, front] in &model.occlusion {
        let a = parts.get(behind.as_str()).ok_or("Unknown occlusion part")?;
        let b = parts.get(front.as_str()).ok_or("Unknown occlusion part")?;
        if behind == front
            || !edges.insert((behind, front))
            || (a.draw_order, a.id.as_str()) >= (b.draw_order, b.id.as_str())
        {
            return Err("Invalid/cyclic or contradictory occlusion order".into());
        }
    }
    Ok(())
}
