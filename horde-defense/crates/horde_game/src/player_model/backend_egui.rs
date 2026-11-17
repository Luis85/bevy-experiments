//! Egui is only the output adapter. It consumes the compiled topology and the
//! single resolved transform; it cannot select skins, bones or source pixels.
use super::{CompiledCatalogue, DrawPacket, LocalMesh, ResolvedPlayerPose, bind::Affine2};
use bevy_egui::egui::{self, Color32, Painter, TextureId, pos2};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

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

/// Submits `pose`'s packets, each posed in display space. A packet whose
/// topology, texture and transforms repeat from this pass or the previous one
/// submits the mesh posed then ([`MeshCache`]), so a still survivor costs no
/// per-vertex work; the vertices are the same either way.
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
    let pass = painter.ctx().cumulative_pass_nr();
    let meshes = painter.ctx().data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<MeshCache>(egui::Id::new(MESH_CACHE));
        cache.begin_pass(pass);
        pose.packets
            .iter()
            .map(|packet| cache.mesh(packet, textures[&packet.source_id]))
            .collect::<Vec<_>>()
    });
    for mesh in meshes {
        let mesh = mesh?;
        if !mesh.vertices.is_empty() {
            painter.add(egui::Shape::Mesh(mesh));
        }
    }
    Ok(())
}

/// The context-data key of the posed-mesh cache.
const MESH_CACHE: &str = "lastlight.player_model.posed_meshes.v1";

/// Everything one packet's display mesh is computed from. The key holds the
/// cached topology's `Arc`, so a cached entry can never alias another mesh.
#[derive(Clone)]
struct MeshKey {
    local_mesh: Arc<LocalMesh>,
    texture: TextureId,
    transform: [u32; 5],
    bone_transform: [u32; 6],
    source_to_bind: [u32; 6],
}
impl MeshKey {
    fn of(packet: &DrawPacket, texture: TextureId) -> Self {
        let t = packet.transform;
        let affine = |a: Affine2| [a.xx, a.xy, a.yx, a.yy, a.tx, a.ty].map(f32::to_bits);
        Self {
            local_mesh: packet.local_mesh.clone(),
            texture,
            transform: [
                t.source_origin[0],
                t.source_origin[1],
                t.source_ground[0],
                t.source_ground[1],
                t.scale,
            ]
            .map(f32::to_bits),
            bone_transform: affine(packet.bone_transform),
            source_to_bind: affine(packet.source_to_bind),
        }
    }
}
impl PartialEq for MeshKey {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.local_mesh, &other.local_mesh)
            && self.texture == other.texture
            && self.transform == other.transform
            && self.bone_transform == other.bone_transform
            && self.source_to_bind == other.source_to_bind
    }
}
impl Eq for MeshKey {}
impl std::hash::Hash for MeshKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.local_mesh).hash(state);
        self.texture.hash(state);
        self.transform.hash(state);
        self.bone_transform.hash(state);
        self.source_to_bind.hash(state);
    }
}

/// Posed display meshes of this pass and the previous one. A packet whose
/// inputs repeat reuses its mesh; a mesh no pass drew for a whole pass goes.
#[derive(Clone, Default)]
struct MeshCache {
    pass: u64,
    current: HashMap<MeshKey, Arc<egui::epaint::Mesh>>,
    previous: HashMap<MeshKey, Arc<egui::epaint::Mesh>>,
}
impl MeshCache {
    fn begin_pass(&mut self, pass: u64) {
        if self.pass != pass {
            self.pass = pass;
            self.previous = std::mem::take(&mut self.current);
        }
    }
    /// The display mesh of `packet`, built and validated on a miss.
    fn mesh(
        &mut self,
        packet: &DrawPacket,
        texture: TextureId,
    ) -> Result<Arc<egui::epaint::Mesh>, String> {
        let key = MeshKey::of(packet, texture);
        if let Some(mesh) = self.current.get(&key) {
            return Ok(mesh.clone());
        }
        let mesh = match self.previous.remove(&key) {
            Some(mesh) => mesh,
            None => Arc::new(posed_mesh(packet, texture)?),
        };
        self.current.insert(key, mesh.clone());
        Ok(mesh)
    }
}

/// `packet`'s source-local topology posed in display space.
fn posed_mesh(packet: &DrawPacket, texture: TextureId) -> Result<egui::epaint::Mesh, String> {
    if packet
        .local_mesh
        .indices
        .iter()
        .any(|i| *i as usize >= packet.local_mesh.vertices.len())
    {
        return Err("Invalid cached player mesh index".into());
    }
    let mut mesh = egui::epaint::Mesh::with_texture(texture);
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
    Ok(mesh)
}
