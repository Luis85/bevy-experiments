//! Independent, lossless inspection of the actual bind-pose draw packets.
//!
//! This is not a second renderer. Source texels are copied only to audit the
//! packet coverage/payload; native alpha blending is checked separately.
use super::{CompiledSource, DrawPacket, ResolvedPlayerPose, SourceChannel};
use serde::Serialize;

/// Exact transform chain used by the production backend for a source vertex.
pub fn packet_point(packet: &DrawPacket, point: [f32; 2]) -> [f32; 2] {
    packet.transform.point(
        packet
            .bone_transform
            .point(packet.source_to_bind.point(point)),
    )
}

/// Synthetic review-only frozen visuals built from real authored item recipes.
/// This neither owns a campaign nor grants, buys or equips an inventory item.
pub fn review_equipment(
    class: Option<horde_core::weapons::WeaponClass>,
    outfit: Option<&str>,
    accessory: Option<&str>,
) -> Result<horde_core::survivors::EquipmentVisual, String> {
    use horde_core::{
        content::{Slot, content},
        survivors::{EquipmentVisual, EquipmentVisualItem},
        weapons::WeaponSpec,
    };
    let item =
        |slot: Slot, style: Option<&str>, class: Option<horde_core::weapons::WeaponClass>| {
            let (definition, recipe) = content()
                .items
                .iter()
                .enumerate()
                .find(|(_, recipe)| {
                    recipe.slot == slot
                        && !recipe.npc_only
                        && style.is_none_or(|style| recipe.visual_style == style)
                        && class.is_none_or(|class| WeaponSpec::from_item(recipe).class == class)
                })
                .ok_or_else(|| {
                    format!("Missing authored review equipment: {slot:?}/{style:?}/{class:?}")
                })?;
            Ok::<_, String>(EquipmentVisualItem {
                definition: horde_core::content::ItemDefId::at(definition),
                level: 1,
                quality: recipe.base_quality,
                style: recipe.visual_style.clone(),
            })
        };
    let equipment = EquipmentVisual {
        ranged: class
            .map(|class| item(Slot::Ranged, None, Some(class)))
            .transpose()?,
        outfit: outfit
            .map(|style| item(Slot::Outfit, Some(style), None))
            .transpose()?,
        accessory: accessory
            .map(|style| item(Slot::Accessory, Some(style), None))
            .transpose()?,
        // Match the real starter's stowed crowbar for legal active-melee
        // diagnostics; a completely empty fixture still owns nothing.
        melee: class
            .map(|_| item(Slot::Melee, Some("melee.crowbar"), None))
            .transpose()?,
        weapon_class: class,
    };
    equipment.validate()?;
    Ok(equipment)
}

/// Compare actual immutable output rather than labels or a fresh approximate
/// skeleton. All emitted source UVs and transformed vertices participate.
#[cfg(test)]
pub fn equivalent_pose(a: &ResolvedPlayerPose, b: &ResolvedPlayerPose) -> bool {
    a.model_id == b.model_id
        && a.skin_id == b.skin_id
        && a.sockets == b.sockets
        && a.source_sockets == b.source_sockets
        && a.muzzle == b.muzzle
        && a.throw_origin == b.throw_origin
        && a.packets.len() == b.packets.len()
        && a.packets.iter().zip(&b.packets).all(|(a, b)| {
            a.part_id == b.part_id
                && a.source_id == b.source_id
                && a.slot == b.slot
                && a.material == b.material
                && a.channel == b.channel
                && a.draw_order == b.draw_order
                && a.rectangles == b.rectangles
                && a.local_mesh.indices == b.local_mesh.indices
                && a.local_mesh.vertices.len() == b.local_mesh.vertices.len()
                && a.local_mesh
                    .vertices
                    .iter()
                    .zip(&b.local_mesh.vertices)
                    .all(|(av, bv)| {
                        av.uv == bv.uv
                            && packet_point(a, av.position) == packet_point(b, bv.position)
                    })
        })
}

#[derive(Clone, Debug, Serialize)]
pub struct PartAudit {
    pub part_id: String,
    pub slot: String,
    pub material: String,
    pub rectangles: usize,
    pub visible_pixels: usize,
    pub alpha_classes: [usize; 3],
    pub vertices: usize,
    pub indices: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct BindAudit {
    pub model_id: String,
    pub skin_id: String,
    /// Full ownership tile, including faint source residue outside the body.
    pub source_tile: [u32; 4],
    pub source_pixels: usize,
    pub alpha_histogram: Vec<usize>,
    pub missing_visible_pixels: usize,
    pub duplicate_visible_pixels: usize,
    pub differing_pixels: usize,
    pub differing_bytes: usize,
    pub hidden_fill_packets: usize,
    pub foreign_source_packets: usize,
    pub malformed_rectangles: usize,
    pub nonidentity_rectangles: usize,
    pub geometry_mismatches: usize,
    pub nonidentity_packet_mappings: usize,
    pub rectangles: usize,
    pub vertices: usize,
    pub indices: usize,
    pub parts: Vec<PartAudit>,
}

impl BindAudit {
    pub fn is_exact(&self) -> bool {
        self.missing_visible_pixels == 0
            && self.duplicate_visible_pixels == 0
            && self.differing_pixels == 0
            && self.differing_bytes == 0
            && self.hidden_fill_packets == 0
            && self.foreign_source_packets == 0
            && self.malformed_rectangles == 0
            && self.nonidentity_rectangles == 0
            && self.geometry_mismatches == 0
            && self.nonidentity_packet_mappings == 0
    }
}

/// Audit a bind resolved with `source_origin == model.ground` and scale one.
///
/// The original alpha-zero RGB is retained as raw payload, not painted as a
/// background. Every visible byte must instead arrive through a real packet.
pub fn audit_bind(
    source: &CompiledSource,
    tile: [u32; 4],
    pose: &ResolvedPlayerPose,
) -> Result<BindAudit, String> {
    let [left, top, right, bottom] = tile;
    let source_bytes = u64::from(source.width) * u64::from(source.height) * 4;
    if left >= right
        || top >= bottom
        || right > source.width
        || bottom > source.height
        || source_bytes != source.rgba.len() as u64
    {
        return Err("Invalid verification source/tile payload".into());
    }
    let width = (right - left) as usize;
    let height = (bottom - top) as usize;
    let pixels = width * height;
    let mut coverage = vec![0_u16; pixels];
    let mut reconstructed = vec![0_u8; pixels * 4];
    let mut audit = BindAudit {
        model_id: pose.model_id.clone(),
        skin_id: pose.skin_id.clone(),
        source_tile: tile,
        source_pixels: pixels,
        alpha_histogram: vec![0; 256],
        missing_visible_pixels: 0,
        duplicate_visible_pixels: 0,
        differing_pixels: 0,
        differing_bytes: 0,
        hidden_fill_packets: 0,
        foreign_source_packets: 0,
        malformed_rectangles: 0,
        nonidentity_rectangles: 0,
        geometry_mismatches: 0,
        nonidentity_packet_mappings: 0,
        rectangles: 0,
        vertices: 0,
        indices: 0,
        parts: Vec::with_capacity(pose.packets.len()),
    };
    let source_index = |x: u32, y: u32| ((y * source.width + x) * 4) as usize;
    let tile_index = |x: u32, y: u32| (y - top) as usize * width + (x - left) as usize;
    for y in top..bottom {
        for x in left..right {
            let input = source_index(x, y);
            let output = tile_index(x, y) * 4;
            let alpha = source.rgba[input + 3];
            audit.alpha_histogram[usize::from(alpha)] += 1;
            if alpha == 0 {
                // Invisible RGB must round-trip too, but has no draw owner.
                reconstructed[output..output + 4].copy_from_slice(&source.rgba[input..input + 4]);
            }
        }
    }
    for packet in &pose.packets {
        if packet.source_to_bind != super::bind::Affine2::IDENTITY
            || packet.bone_transform != super::bind::Affine2::IDENTITY
        {
            audit.nonidentity_packet_mappings += 1;
        }
        if packet.channel == SourceChannel::HiddenFill {
            audit.hidden_fill_packets += 1;
        }
        if packet.source_id != source.id {
            audit.foreign_source_packets += 1;
            continue;
        }
        let mut part = PartAudit {
            part_id: packet.part_id.clone(),
            slot: packet.slot.clone(),
            material: packet.material.clone(),
            rectangles: packet.rectangles.len(),
            visible_pixels: 0,
            alpha_classes: [0; 3],
            vertices: packet.local_mesh.vertices.len(),
            indices: packet.local_mesh.indices.len(),
        };
        audit.rectangles += packet.rectangles.len();
        audit.vertices += packet.local_mesh.vertices.len();
        audit.indices += packet.local_mesh.indices.len();
        if packet.local_mesh.vertices.len() != packet.rectangles.len() * 4
            || packet.local_mesh.indices.len() != packet.rectangles.len() * 6
        {
            audit.geometry_mismatches += 1;
        }
        for (rect_index, rect) in packet.rectangles.iter().enumerate() {
            if rect.x0 >= rect.x1
                || rect.y0 >= rect.y1
                || rect.x0 < left
                || rect.y0 < top
                || rect.x1 > right
                || rect.y1 > bottom
            {
                audit.malformed_rectangles += 1;
                continue;
            }
            let corners = [
                [rect.x0 as f32, rect.y0 as f32],
                [rect.x1 as f32, rect.y0 as f32],
                [rect.x1 as f32, rect.y1 as f32],
                [rect.x0 as f32, rect.y1 as f32],
            ];
            let vertices = packet
                .local_mesh
                .vertices
                .get(rect_index * 4..rect_index * 4 + 4);
            let indices = packet
                .local_mesh
                .indices
                .get(rect_index * 6..rect_index * 6 + 6);
            let first = rect_index as u32 * 4;
            let expected_indices = [first, first + 1, first + 2, first, first + 2, first + 3];
            if vertices.is_none_or(|vertices| {
                vertices.iter().zip(corners).any(|(vertex, position)| {
                    vertex.position != position
                        || vertex.uv
                            != [
                                position[0] / source.width as f32,
                                position[1] / source.height as f32,
                            ]
                })
            }) || indices != Some(expected_indices.as_slice())
            {
                audit.geometry_mismatches += 1;
            }
            if corners
                .iter()
                .any(|point| packet_point(packet, *point) != *point)
            {
                audit.nonidentity_rectangles += 1;
            }
            for y in rect.y0..rect.y1 {
                for x in rect.x0..rect.x1 {
                    let input = source_index(x, y);
                    let index = tile_index(x, y);
                    let alpha = source.rgba[input + 3];
                    part.alpha_classes[if alpha == 0 {
                        0
                    } else if alpha == 255 {
                        2
                    } else {
                        1
                    }] += 1;
                    if alpha != 0 {
                        part.visible_pixels += 1;
                        coverage[index] = coverage[index].saturating_add(1);
                    }
                    reconstructed[index * 4..index * 4 + 4]
                        .copy_from_slice(&source.rgba[input..input + 4]);
                }
            }
        }
        audit.parts.push(part);
    }
    for y in top..bottom {
        for x in left..right {
            let input = source_index(x, y);
            let index = tile_index(x, y);
            if source.rgba[input + 3] != 0 {
                audit.missing_visible_pixels += usize::from(coverage[index] == 0);
                audit.duplicate_visible_pixels += usize::from(coverage[index] > 1);
            }
            let expected = &source.rgba[input..input + 4];
            let actual = &reconstructed[index * 4..index * 4 + 4];
            audit.differing_pixels += usize::from(expected != actual);
            audit.differing_bytes += expected
                .iter()
                .zip(actual)
                .filter(|(expected, actual)| expected != actual)
                .count();
        }
    }
    // Vertices and indices above are measured from the actual cached backend
    // payload, never inferred from rectangles or truncated to satisfy a budget.
    Ok(audit)
}
