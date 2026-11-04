//! Egui is only the output adapter. It consumes the compiled topology and the
//! single resolved transform; it cannot select skins, bones or source pixels.
use super::{CompiledCatalogue, ResolvedPlayerPose};
use bevy_egui::egui::{self, Color32, Painter, TextureId, pos2};
use std::collections::BTreeMap;

#[derive(Clone)]
struct SourceTextures {
    catalogue_identity: usize,
    handles: BTreeMap<String, egui::TextureHandle>,
}
/// Shared production/review upload path. Call during context/asset startup;
/// subsequent draws only clone TextureIds, never decode or scan image pixels.
pub fn textures(context: &egui::Context) -> Result<BTreeMap<String, TextureId>, String> {
    let catalogue = super::startup::active_or_bundled()?;
    let identity = catalogue as *const CompiledCatalogue as usize;
    let key = egui::Id::new("lastlight.player_model.source_textures.v1");
    if let Some(cache) = context.data(|d| d.get_temp::<std::sync::Arc<SourceTextures>>(key))
        && cache.catalogue_identity == identity
    {
        return Ok(cache
            .handles
            .iter()
            .map(|(id, h)| (id.clone(), h.id()))
            .collect());
    }
    let cap = context.input(|input| input.max_texture_side);
    let mut handles = BTreeMap::new();
    for source in catalogue.sources.values() {
        if source.width as usize > cap || source.height as usize > cap {
            return Err(format!(
                "Player source {} exceeds actual texture-side budget {cap}",
                source.id
            ));
        }
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [source.width as usize, source.height as usize],
            &source.rgba,
        );
        let texture = context.load_texture(
            format!(
                "player-model-{}-{}-{}",
                source.id, source.rgba_fnv1a64, source.path
            ),
            image,
            egui::TextureOptions::NEAREST,
        );
        handles.insert(source.id.clone(), texture);
    }
    let ids = handles.iter().map(|(id, h)| (id.clone(), h.id())).collect();
    context.data_mut(|d| {
        d.insert_temp(
            key,
            std::sync::Arc::new(SourceTextures {
                catalogue_identity: identity,
                handles,
            }),
        )
    });
    Ok(ids)
}

pub fn draw_egui(
    painter: &Painter,
    pose: &ResolvedPlayerPose,
    textures: &BTreeMap<String, TextureId>,
) -> Result<(), String> {
    for packet in &pose.packets {
        if !textures.contains_key(&packet.source_id) {
            return Err(format!(
                "Missing player source texture {}",
                packet.source_id
            ));
        }
    }
    for packet in &pose.packets {
        if packet
            .local_mesh
            .indices
            .iter()
            .any(|i| *i as usize >= packet.local_mesh.vertices.len())
        {
            return Err("Invalid cached player mesh index".into());
        }
        let mut mesh = egui::epaint::Mesh::with_texture(textures[&packet.source_id]);
        mesh.indices = packet.local_mesh.indices.clone();
        mesh.vertices = packet
            .local_mesh
            .vertices
            .iter()
            .map(|v| {
                let p = packet.transform.point(
                    packet
                        .bone_transform
                        .point(packet.source_to_bind.point(v.position)),
                );
                egui::epaint::Vertex {
                    pos: pos2(p[0], p[1]),
                    uv: pos2(v.uv[0], v.uv[1]),
                    color: Color32::WHITE,
                }
            })
            .collect();
        if !mesh.vertices.is_empty() {
            painter.add(egui::Shape::mesh(mesh));
        }
    }
    Ok(())
}
