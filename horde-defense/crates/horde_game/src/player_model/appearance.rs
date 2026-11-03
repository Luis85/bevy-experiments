//! Stage 1–4 selects the declared original appearance only. No inventory guessing.
use super::{CompiledCatalogue, SkinDefinition, SourceChannel};
use horde_core::survivors::{EquipmentVisual, EquipmentVisualItem};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct ResolvedAppearance {
    pub model_id: String,
    pub skin_id: String,
    pub part_indices: Vec<usize>,
    pub attachment_ids: Vec<String>,
    pub hold_id: Option<String>,
    pub frozen_equipment: Option<horde_core::survivors::EquipmentVisual>,
}

fn item<'a>(equipment: &'a EquipmentVisual, slot: &str) -> Option<&'a EquipmentVisualItem> {
    match slot {
        "ranged" => equipment.ranged.as_ref(),
        "melee" => equipment.melee.as_ref(),
        "outfit" => equipment.outfit.as_ref(),
        "accessory" => equipment.accessory.as_ref(),
        _ => None,
    }
}
fn matches_equipment(skin: &SkinDefinition, equipment: &EquipmentVisual) -> bool {
    skin.equipment.iter().all(|requirement| {
        let selected = item(equipment, &requirement.slot);
        if requirement.style.is_none() && requirement.weapon_class.is_none() {
            return selected.is_none();
        }
        let Some(selected) = selected else {
            return false;
        };
        requirement
            .style
            .as_ref()
            .is_none_or(|style| *style == selected.style)
            && requirement.weapon_class.as_ref().is_none_or(|class| {
                equipment
                    .weapon_class
                    .is_some_and(|value| format!("{value:?}") == *class)
            })
    })
}
pub fn resolve_appearance(
    catalogue: &CompiledCatalogue,
    model_id: &str,
    equipment: &EquipmentVisual,
) -> Result<ResolvedAppearance, String> {
    equipment.validate()?;
    let model = catalogue
        .model(model_id)
        .ok_or_else(|| format!("Unknown player model {model_id}"))?;
    let equipment_slots = ["outfit", "accessory", "ranged", "melee"];
    let mut originals = model
        .parts
        .iter()
        .filter(|part| {
            !equipment_slots.contains(&part.definition.slot.as_str())
                && part.definition.channel != SourceChannel::HiddenFill
        })
        .map(|part| part.definition.id.clone())
        .collect::<BTreeSet<_>>();
    let mut remove = BTreeSet::new();
    let mut attachments = BTreeSet::new();
    let mut hold = None;
    let mut matched = BTreeSet::new();
    for skin in catalogue
        .definition
        .skins
        .iter()
        .filter(|skin| skin.model == model_id && skin.id != model.definition.default_skin)
    {
        if !matches_equipment(skin, equipment) {
            continue;
        }
        originals.extend(skin.parts.iter().cloned());
        remove.extend(skin.replace_parts.iter().cloned());
        attachments.extend(skin.attachments.iter().cloned());
        for requirement in &skin.equipment {
            matched.insert(requirement.slot.as_str());
        }
        if let Some(id) = &skin.hold {
            if hold.as_ref().is_some_and(|old| old != id) {
                return Err("Conflicting selected player holds".into());
            }
            hold = Some(id.clone());
        }
    }
    for id in remove {
        originals.remove(&id);
    }
    // Stowed melee has no pixels in the original still. An active melee pose
    // separately requires a declared skin/hold rather than inventing a weapon.
    for slot in ["outfit", "accessory", "ranged"] {
        if item(equipment, slot).is_some() && !matched.contains(slot) {
            return Err(format!(
                "No compatible {slot} appearance for model {model_id}"
            ));
        }
    }
    if hold.is_none() && equipment.ranged.is_some() {
        let class = equipment
            .weapon_class
            .ok_or("Equipped ranged appearance lacks weapon class")?;
        let candidates = catalogue
            .definition
            .holds
            .iter()
            .filter(|h| {
                h.model == model_id
                    && h.weapon_class == format!("{class:?}")
                    && h.weapon_attachment.is_none()
            })
            .collect::<Vec<_>>();
        if candidates.len() == 1 {
            hold = Some(candidates[0].id.clone());
        } else if candidates.len() > 1 {
            return Err("Ambiguous original weapon hold".into());
        }
    }
    let mut part_indices = model
        .parts
        .iter()
        .enumerate()
        .filter_map(|(index, part)| originals.contains(&part.definition.id).then_some(index))
        .collect::<Vec<_>>();
    part_indices.sort_by(|a, b| {
        let a = &model.parts[*a].definition;
        let b = &model.parts[*b].definition;
        a.draw_order.cmp(&b.draw_order).then(a.id.cmp(&b.id))
    });
    Ok(ResolvedAppearance {
        model_id: model_id.into(),
        skin_id: format!("{model_id}.equipped"),
        part_indices,
        attachment_ids: attachments.into_iter().collect(),
        hold_id: hold,
        frozen_equipment: Some(equipment.clone()),
    })
}
pub fn reference_appearance(
    catalogue: &CompiledCatalogue,
    model_id: &str,
) -> Result<ResolvedAppearance, String> {
    let model = catalogue
        .model(model_id)
        .ok_or_else(|| format!("Unknown player model {model_id}"))?;
    let skin = catalogue
        .definition
        .skin(&model.definition.default_skin)
        .ok_or("Missing original skin")?;
    let mut indices = Vec::with_capacity(skin.parts.len());
    for id in &skin.parts {
        let index = model
            .parts
            .iter()
            .position(|p| p.definition.id == *id)
            .ok_or("Unknown original part")?;
        if model.parts[index].definition.channel == SourceChannel::HiddenFill {
            return Err("Hidden fill selected at bind".into());
        }
        indices.push(index);
    }
    indices.sort_by(|a, b| {
        let a = &model.parts[*a].definition;
        let b = &model.parts[*b].definition;
        a.draw_order.cmp(&b.draw_order).then(a.id.cmp(&b.id))
    });
    Ok(ResolvedAppearance {
        model_id: model_id.into(),
        skin_id: skin.id.clone(),
        part_indices: indices,
        attachment_ids: vec![],
        hold_id: None,
        frozen_equipment: None,
    })
}
