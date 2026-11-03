//! Additive equipment/repair/hold authoring. Original model documents and masks
//! retain their version-one meaning and are never rewritten by these recipes.
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAssetDefinition {
    pub id: String,
    pub path: String,
    pub png_sha256: String,
    pub rgba_fnv1a64: String,
    pub size: [u32; 2],
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SourceToBind {
    pub source_origin: [f32; 2],
    pub bind_origin: [f32; 2],
    pub rotation: f32,
    pub scale: f32,
}
impl Default for SourceToBind {
    fn default() -> Self {
        Self {
            source_origin: [0.; 2],
            bind_origin: [0.; 2],
            rotation: 0.,
            scale: 1.,
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PoseActionKind {
    Aim,
    Recoil,
    Reload,
    Throw,
    Melee,
    Brace,
    Hit,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivationKind {
    SkinSelected,
    Revealed,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentActivation {
    pub kind: ActivationKind,
    #[serde(default)]
    pub missing_parts: Vec<String>,
    #[serde(default)]
    pub required_parts: Vec<String>,
    #[serde(default)]
    pub actions: Vec<PoseActionKind>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentSocket {
    pub id: String,
    pub source_position: [f32; 2],
    /// Public semantic sockets supplied by the currently painted cel. The
    /// attachment-qualified socket remains available for geometry inspection.
    #[serde(default)]
    pub publish_as: Vec<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentDefinition {
    pub id: String,
    pub model: String,
    pub source_id: String,
    pub bone: String,
    pub slot: String,
    pub material: String,
    pub draw_order: i32,
    pub channel: super::SourceChannel,
    pub source_runs: Vec<[u32; 3]>,
    #[serde(default)]
    pub source_to_bind: SourceToBind,
    pub activation: AttachmentActivation,
    #[serde(default)]
    pub sockets: Vec<AttachmentSocket>,
    pub provenance: String,
    /// Action/cel-local source owners to suppress only while this attachment
    /// passes its activation predicate. Reference appearance is unchanged.
    #[serde(default)]
    pub replace_parts: Vec<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactDefinition {
    pub bone: String,
    pub bind_position: [f32; 2],
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandContactDefinitions {
    pub near: ContactDefinition,
    pub far: ContactDefinition,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponSocketNames {
    pub grip: String,
    pub support: Option<String>,
    pub stock: Option<String>,
    pub muzzle: String,
    pub barrel_back: String,
    pub magazine: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldDefinition {
    pub id: String,
    pub model: String,
    pub weapon_class: String,
    pub weapon_attachment: Option<String>,
    pub pivot_socket: String,
    pub weapon_sockets: WeaponSocketNames,
    pub hand_contacts: HandContactDefinitions,
    pub recoil_pixels: f32,
    pub aim_range: [f32; 2],
    pub reload_track: Option<String>,
    pub throw_track: Option<String>,
    pub melee_track: Option<String>,
    pub brace_track: Option<String>,
    #[serde(default)]
    pub aim_track: Option<String>,
    #[serde(default)]
    pub recoil_track: Option<String>,
    #[serde(default)]
    pub hit_track: Option<String>,
    #[serde(default)]
    pub support_track: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportKey {
    pub phase: f32,
    pub source_position: [f32; 2],
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportTrack {
    pub id: String,
    pub model: String,
    pub weapon_attachment: String,
    pub source_runs: Vec<[u32; 3]>,
    pub keys: Vec<SupportKey>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoneDelta {
    pub bone: String,
    pub rotation: f32,
    pub translation: [f32; 2],
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoseKey {
    pub phase: f32,
    pub bones: Vec<BoneDelta>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoseTrack {
    pub id: String,
    pub model: String,
    pub action: PoseActionKind,
    pub keys: Vec<PoseKey>,
}

impl From<&super::SourceDefinition> for SourceAssetDefinition {
    fn from(s: &super::SourceDefinition) -> Self {
        Self {
            id: s.id.clone(),
            path: s.path.clone(),
            png_sha256: s.png_sha256.clone(),
            rgba_fnv1a64: s.rgba_fnv1a64.clone(),
            size: s.size,
        }
    }
}
pub(super) fn validate_extra(c: &super::PlayerModelCatalogue) -> Result<(), String> {
    use super::catalogue::valid_id;
    use std::collections::{BTreeMap, BTreeSet};
    let unique = |ids: Vec<&str>, kind: &str| -> Result<(), String> {
        let mut seen = BTreeSet::new();
        for id in ids {
            if !valid_id(id) || !seen.insert(id) {
                return Err(format!("Invalid/duplicate {kind} {id}"));
            }
        }
        Ok(())
    };
    unique(
        c.sources.iter().map(|v| v.id.as_str()).collect(),
        "extra source",
    )?;
    unique(
        c.attachments.iter().map(|v| v.id.as_str()).collect(),
        "attachment",
    )?;
    unique(c.holds.iter().map(|v| v.id.as_str()).collect(), "hold")?;
    unique(
        c.pose_tracks.iter().map(|v| v.id.as_str()).collect(),
        "pose track",
    )?;
    unique(
        c.support_tracks.iter().map(|v| v.id.as_str()).collect(),
        "support track",
    )?;
    if c.sources.len() > 16
        || c.attachments.len() > 1024
        || c.holds.len() > 128
        || c.pose_tracks.len() > 256
        || c.support_tracks.len() > 128
    {
        return Err("Player equipment catalogue budget exceeded".into());
    }
    let mut sources: BTreeMap<String, SourceAssetDefinition> = BTreeMap::new();
    for m in &c.models {
        sources.insert(m.source.id.clone(), (&m.source).into());
    }
    for s in &c.sources {
        if s.path.is_empty()
            || s.path.len() > 512
            || s.path.contains("..")
            || s.path.starts_with('/')
            || s.path.starts_with('\\')
            || s.path.contains(':')
            || s.size.iter().any(|v| *v == 0 || *v > 8192)
            || s.png_sha256.len() != 64
            || !s.png_sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || s.rgba_fnv1a64.len() != 16
            || !s.rgba_fnv1a64.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(format!("Invalid extra source {}", s.id));
        }
        if let Some(old) = sources.get(&s.id)
            && (old.path != s.path
                || old.size != s.size
                || old.png_sha256 != s.png_sha256
                || old.rgba_fnv1a64 != s.rgba_fnv1a64)
        {
            return Err("Conflicting player source descriptor".into());
        }
        sources.insert(s.id.clone(), s.clone());
    }
    if sources
        .values()
        .map(|s| u64::from(s.size[0]) * u64::from(s.size[1]))
        .sum::<u64>()
        > 32_000_000
    {
        return Err("Player equipment source budget exceeded".into());
    }
    let attachments: BTreeMap<_, _> = c.attachments.iter().map(|a| (a.id.as_str(), a)).collect();
    let holds: BTreeMap<_, _> = c.holds.iter().map(|a| (a.id.as_str(), a)).collect();
    let tracks: BTreeMap<_, _> = c.pose_tracks.iter().map(|a| (a.id.as_str(), a)).collect();
    let support_tracks: BTreeMap<_, _> = c
        .support_tracks
        .iter()
        .map(|a| (a.id.as_str(), a))
        .collect();
    let finite = |p: [f32; 2], limit: f32| p.iter().all(|v| v.is_finite() && v.abs() <= limit);
    let mut run_count = 0;
    for a in &c.attachments {
        let m = c
            .model(&a.model)
            .ok_or_else(|| format!("Unknown attachment model {}", a.model))?;
        let s = sources
            .get(&a.source_id)
            .ok_or_else(|| format!("Unknown attachment source {}", a.source_id))?;
        if !m.bones.iter().any(|b| b.id == a.bone)
            || !m.slots.contains(&a.slot)
            || !valid_id(&a.material)
            || a.draw_order.unsigned_abs() > 1_000_000
            || !matches!(
                a.channel,
                super::SourceChannel::HiddenFill | super::SourceChannel::SkinReplacement
            )
            || a.provenance.is_empty()
            || a.provenance.len() > 8192
        {
            return Err(format!("Invalid attachment binding {}", a.id));
        }
        unique(
            a.replace_parts.iter().map(String::as_str).collect(),
            "active replacement owner",
        )?;
        if !a.replace_parts.is_empty() {
            if a.channel != super::SourceChannel::SkinReplacement
                || a.activation.actions.is_empty()
                || a.activation.kind != ActivationKind::SkinSelected
            {
                return Err(
                    "Action-local replacements require an explicitly activated skin/cel".into(),
                );
            }
            for id in &a.replace_parts {
                let owner = m
                    .parts
                    .iter()
                    .find(|p| p.id == *id)
                    .ok_or("Unknown active replacement owner")?;
                if owner.slot == "body"
                    || owner.channel == super::SourceChannel::Residue
                    || owner.slot != a.slot
                {
                    return Err("Active replacement cannot overwrite protected identity/body or another slot".into());
                }
            }
        }
        let t = a.source_to_bind;
        if !finite(t.source_origin, 8192.)
            || t.source_origin[0] < 0.
            || t.source_origin[1] < 0.
            || t.source_origin[0] > s.size[0] as f32
            || t.source_origin[1] > s.size[1] as f32
            || !finite(t.bind_origin, 1_000_000.)
            || !t.rotation.is_finite()
            || t.rotation.abs() > std::f32::consts::TAU * 2.
            || !t.scale.is_finite()
            || t.scale <= 0.
            || t.scale > 32.
        {
            return Err(format!("Invalid uniform source binding {}", a.id));
        }
        run_count += a.source_runs.len();
        if run_count > 200_000 {
            return Err("Attachment run budget exceeded".into());
        }
        for [y, x0, x1] in &a.source_runs {
            if *y >= s.size[1] || x0 >= x1 || *x1 > s.size[0] {
                return Err(format!("Invalid attachment run {}", a.id));
            }
        }
        for id in a
            .activation
            .missing_parts
            .iter()
            .chain(&a.activation.required_parts)
        {
            if !m.parts.iter().any(|p| p.id == *id) {
                return Err(format!("Unknown reveal owner {id}"));
            }
        }
        if a.activation.actions.len() > 7
            || a.activation
                .actions
                .iter()
                .enumerate()
                .any(|(i, v)| a.activation.actions[..i].contains(v))
        {
            return Err("Duplicate/excessive reveal actions".into());
        }
        if a.channel == super::SourceChannel::HiddenFill
            && a.activation.kind != ActivationKind::Revealed
        {
            return Err("Hidden fill requires explicit exposure".into());
        }
        unique(
            a.sockets.iter().map(|s| s.id.as_str()).collect(),
            "attachment socket",
        )?;
        for socket in &a.sockets {
            if !finite(socket.source_position, 8192.)
                || socket.source_position[0] < 0.
                || socket.source_position[1] < 0.
                || socket.source_position[0] > s.size[0] as f32
                || socket.source_position[1] > s.size[1] as f32
            {
                return Err("Invalid attachment socket position".into());
            }
            unique(
                socket.publish_as.iter().map(String::as_str).collect(),
                "published socket alias",
            )?;
            for alias in &socket.publish_as {
                if a.channel != super::SourceChannel::SkinReplacement
                    || !matches!(a.slot.as_str(), "hands" | "melee")
                    || !matches!(
                        alias.as_str(),
                        "palm.near"
                            | "palm.far"
                            | "throw_origin"
                            | "contact.near"
                            | "contact.far"
                            | "melee.grip"
                    )
                    || (alias == "melee.grip" && a.slot != "melee")
                {
                    return Err(format!("Invalid public socket alias {} on {}", alias, a.id));
                }
            }
        }
    }
    for track in &c.pose_tracks {
        let m = c.model(&track.model).ok_or("Unknown track model")?;
        if track.keys.is_empty()
            || track.keys.len() > 128
            || track.keys[0].phase != 0.
            || (track.keys.len() > 1 && track.keys.last().unwrap().phase != 1.)
        {
            return Err(format!("Invalid pose-track timeline {}", track.id));
        }
        let expected: BTreeSet<_> = track.keys[0]
            .bones
            .iter()
            .map(|b| b.bone.as_str())
            .collect();
        for (i, key) in track.keys.iter().enumerate() {
            if !key.phase.is_finite()
                || !(0. ..=1.).contains(&key.phase)
                || (i > 0 && key.phase <= track.keys[i - 1].phase)
                || key.bones.len() > 128
            {
                return Err("Invalid pose key phase".into());
            }
            unique(
                key.bones.iter().map(|b| b.bone.as_str()).collect(),
                "pose-key bone",
            )?;
            if key
                .bones
                .iter()
                .map(|b| b.bone.as_str())
                .collect::<BTreeSet<_>>()
                != expected
            {
                return Err("Pose key bone sets differ".into());
            }
            for b in &key.bones {
                if !m.bones.iter().any(|bone| bone.id == b.bone)
                    || !b.rotation.is_finite()
                    || b.rotation.abs() > std::f32::consts::TAU * 2.
                    || !finite(b.translation, 1000.)
                {
                    return Err("Invalid pose-key bone delta".into());
                }
            }
        }
    }
    for hold in &c.holds {
        let m = c.model(&hold.model).ok_or("Unknown hold model")?;
        if ![
            "Pistol",
            "Smg",
            "Shotgun",
            "Rifle",
            "Marksman",
            "MachineGun",
        ]
        .contains(&hold.weapon_class.as_str())
            || !hold.recoil_pixels.is_finite()
            || !(0. ..=100.).contains(&hold.recoil_pixels)
            || hold
                .aim_range
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 180.)
            || hold.aim_range[0] >= hold.aim_range[1]
        {
            return Err("Invalid hold class/range".into());
        }
        let socket_ids: BTreeSet<&str> = if let Some(id) = &hold.weapon_attachment {
            let a = attachments
                .get(id.as_str())
                .ok_or("Unknown held weapon attachment")?;
            if a.model != m.id || a.channel != super::SourceChannel::SkinReplacement {
                return Err("Incompatible held attachment".into());
            }
            a.sockets.iter().map(|s| s.id.as_str()).collect()
        } else {
            m.sockets.iter().map(|s| s.id.as_str()).collect()
        };
        let names = &hold.weapon_sockets;
        if let Some(id) = &hold.support_track {
            let track = support_tracks
                .get(id.as_str())
                .ok_or("Unknown support rail")?;
            if track.model != m.id
                || hold.weapon_attachment.as_deref() != Some(track.weapon_attachment.as_str())
                || names.support.is_none()
            {
                return Err("Incompatible support rail".into());
            }
        }
        for id in [&names.grip, &names.muzzle, &names.barrel_back]
            .into_iter()
            .chain(names.support.iter())
            .chain(names.stock.iter())
            .chain(names.magazine.iter())
        {
            if !socket_ids.contains(id.as_str()) {
                return Err(format!("Unknown weapon hold socket {id}"));
            }
        }
        match hold.pivot_socket.as_str() {
            "shoulder.near" => {
                if !m.sockets.iter().any(|s| s.id == "shoulder.near") {
                    return Err("Hold lacks shoulder socket".into());
                }
            }
            "grip" => {}
            "stock" => {
                if names.stock.is_none() {
                    return Err("Hold has no stock".into());
                }
            }
            _ => return Err("Unknown hold pivot".into()),
        }
        for contact in [&hold.hand_contacts.near, &hold.hand_contacts.far] {
            if !m.bones.iter().any(|b| b.id == contact.bone)
                || !finite(contact.bind_position, 8192.)
            {
                return Err("Invalid anatomical hand contact".into());
            }
        }
        for (id, action) in [
            (&hold.aim_track, PoseActionKind::Aim),
            (&hold.recoil_track, PoseActionKind::Recoil),
            (&hold.hit_track, PoseActionKind::Hit),
            (&hold.reload_track, PoseActionKind::Reload),
            (&hold.throw_track, PoseActionKind::Throw),
            (&hold.melee_track, PoseActionKind::Melee),
            (&hold.brace_track, PoseActionKind::Brace),
        ] {
            if let Some(id) = id {
                let track = tracks.get(id.as_str()).ok_or("Unknown hold pose track")?;
                if track.model != m.id || track.action != action {
                    return Err("Hold pose track mismatch".into());
                }
            }
        }
    }
    for track in &c.support_tracks {
        let a = attachments
            .get(track.weapon_attachment.as_str())
            .ok_or("Unknown support-track weapon")?;
        let source = sources
            .get(&a.source_id)
            .ok_or("Unknown support-track source")?;
        if a.model != track.model
            || track.keys.len() < 2
            || track.keys.len() > 64
            || track.keys[0].phase != 0.
            || track.keys.last().unwrap().phase != 1.
            || track.source_runs.len() > 8192
        {
            return Err("Invalid support-track declaration".into());
        }
        for [y, x0, x1] in &track.source_runs {
            if *y >= source.size[1] || x0 >= x1 || *x1 > source.size[0] {
                return Err("Invalid support surface mask".into());
            }
        }
        for (i, key) in track.keys.iter().enumerate() {
            if !key.phase.is_finite()
                || !(0. ..=1.).contains(&key.phase)
                || i > 0 && key.phase <= track.keys[i - 1].phase
                || !finite(key.source_position, 8192.)
            {
                return Err("Invalid support key".into());
            }
        }
    }
    for skin in &c.skins {
        let m = c.model(&skin.model).ok_or("Unknown skin model")?;
        unique(
            skin.replace_parts.iter().map(String::as_str).collect(),
            "replaced part",
        )?;
        unique(
            skin.attachments.iter().map(String::as_str).collect(),
            "skin attachment",
        )?;
        for id in &skin.replace_parts {
            let p = m
                .parts
                .iter()
                .find(|p| p.id == *id)
                .ok_or("Unknown replaced source owner")?;
            if p.slot == "body" || skin.parts.contains(id) {
                return Err("Skin replaces identity or contradicts its source parts".into());
            }
        }
        for id in &skin.attachments {
            if attachments.get(id.as_str()).is_none_or(|a| a.model != m.id) {
                return Err("Unknown/incompatible skin attachment".into());
            }
        }
        if let Some(id) = &skin.hold {
            let hold = holds.get(id.as_str()).ok_or("Unknown selected hold")?;
            if hold.model != m.id
                || hold
                    .weapon_attachment
                    .as_ref()
                    .is_some_and(|id| !skin.attachments.contains(id))
            {
                return Err("Incompatible selected hold".into());
            }
        }
        if skin.id == m.default_skin
            && (!skin.replace_parts.is_empty()
                || !skin.attachments.is_empty()
                || skin.hold.is_some())
        {
            return Err("Reference bind skin cannot include additive recipes".into());
        }
        if skin.equipment.is_empty()
            && skin.id != m.default_skin
            && (!skin.parts.is_empty()
                || !skin.replace_parts.is_empty()
                || skin.hold.is_some()
                || skin.attachments.iter().any(|id| {
                    attachments[id.as_str()].channel != super::SourceChannel::HiddenFill
                        || attachments[id.as_str()].activation.kind != ActivationKind::Revealed
                }))
        {
            return Err(
                "Unconditional recipes may only contain revealed repair attachments".into(),
            );
        }
    }
    Ok(())
}
