//! Review-only egui probes (bevy_egui =0.43.1). In PostUpdate:
//! - `egui_pass_ms` spans `EguiPostUpdateSet::EndPass`: every multipass draw
//!   of the primary context plus `end_pass`;
//! - `egui_tessellate_ms` spans `ProcessOutput`, where `process_output_system`
//!   tessellates (bevy_egui output.rs:51);
//! - shapes and texture uploads are counted from `EguiFullOutput` between the
//!   two, before bevy_egui's texture update drains them; primitives,
//!   vertices, indices and textured quads from `EguiRenderOutput::paint_jobs`.
use super::{EguiCount, Mark, PerfProbe};
use bevy::prelude::*;
use bevy_egui::{
    EguiFullOutput, EguiPostUpdateSet, EguiRenderOutput,
    egui::{ClippedPrimitive, TextureId, TexturesDelta, epaint::Primitive},
};
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) fn add(app: &mut App) {
    app.add_systems(
        PostUpdate,
        (
            begin_pass.before(EguiPostUpdateSet::EndPass),
            end_pass
                .after(EguiPostUpdateSet::EndPass)
                .before(EguiPostUpdateSet::ProcessOutput),
            end_tessellate
                .after(EguiPostUpdateSet::ProcessOutput)
                .before(EguiPostUpdateSet::PostProcessOutput),
        ),
    );
}

/// Paint-job totals for one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EguiCounts {
    pub clipped_primitives: u64,
    pub mesh_vertices: u64,
    pub mesh_indices: u64,
    /// Σ indices / 6 over meshes whose texture is not the font atlas
    /// (`TextureId::default()`): image quads, not text or shapes.
    pub textured_quads: u64,
}
pub fn egui_counts(jobs: &[ClippedPrimitive]) -> EguiCounts {
    let mut counts = EguiCounts {
        clipped_primitives: jobs.len() as u64,
        ..default()
    };
    for job in jobs {
        if let Primitive::Mesh(mesh) = &job.primitive {
            counts.mesh_vertices += mesh.vertices.len() as u64;
            counts.mesh_indices += mesh.indices.len() as u64;
            if mesh.texture_id != TextureId::default() {
                counts.textured_quads += mesh.indices.len() as u64 / 6;
            }
        }
    }
    counts
}

/// Texture uploads one egui output asks bevy_egui for (W1-64): every
/// `textures_delta.set` entry, full or partial, and its pixel bytes. bevy_egui
/// writes each into `Assets<Image>`, so the render world re-extracts and
/// re-uploads the whole texture, not just the delta.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextureUploads {
    pub count: u64,
    pub bytes: u64,
}
pub fn texture_uploads(delta: &TexturesDelta) -> TextureUploads {
    let mut uploads = TextureUploads::default();
    for image in delta.set.values().flatten() {
        let [width, height] = image.image.size();
        uploads.count += 1;
        uploads.bytes += (width * height * image.image.bytes_per_pixel()) as u64;
    }
    uploads
}

impl PerfProbe {
    fn end_span(&self, mark: Mark, total: &AtomicU64) {
        total.fetch_add(self.since(mark), Ordering::Relaxed);
    }
    fn set_egui(&self, count: EguiCount, value: u64) {
        self.0.egui[count as usize].store(value, Ordering::Relaxed);
    }
}

fn begin_pass(probe: Res<PerfProbe>) {
    probe.mark(Mark::EguiPass);
}
fn end_pass(probe: Res<PerfProbe>, outputs: Query<&EguiFullOutput>) {
    probe.end_span(Mark::EguiPass, &probe.0.egui_pass_ns);
    let (mut shapes, mut uploads) = (0, TextureUploads::default());
    for output in outputs.iter().filter_map(|output| output.0.as_ref()) {
        shapes += output.shapes.len() as u64;
        let delta = texture_uploads(&output.textures_delta);
        uploads.count += delta.count;
        uploads.bytes += delta.bytes;
    }
    probe.set_egui(EguiCount::Shapes, shapes);
    probe.set_egui(EguiCount::TextureUploads, uploads.count);
    probe.set_egui(EguiCount::TextureUploadBytes, uploads.bytes);
    probe.mark(Mark::Tessellate);
}
fn end_tessellate(probe: Res<PerfProbe>, outputs: Query<&EguiRenderOutput>) {
    probe.end_span(Mark::Tessellate, &probe.0.tessellate_ns);
    let mut total = EguiCounts::default();
    for output in &outputs {
        let counts = egui_counts(&output.paint_jobs);
        total.clipped_primitives += counts.clipped_primitives;
        total.mesh_vertices += counts.mesh_vertices;
        total.mesh_indices += counts.mesh_indices;
        total.textured_quads += counts.textured_quads;
    }
    probe.set_egui(EguiCount::ClippedPrimitives, total.clipped_primitives);
    probe.set_egui(EguiCount::MeshVertices, total.mesh_vertices);
    probe.set_egui(EguiCount::MeshIndices, total.mesh_indices);
    probe.set_egui(EguiCount::TexturedQuads, total.textured_quads);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_egui::egui::{
        Color32, ColorImage, Rect, TextureOptions,
        epaint::{ImageDelta, Mesh},
        pos2,
    };

    fn quad_mesh(texture_id: TextureId, quads: usize) -> ClippedPrimitive {
        let mut mesh = Mesh::with_texture(texture_id);
        for _ in 0..quads {
            mesh.add_rect_with_uv(
                Rect::from_min_max(pos2(0., 0.), pos2(4., 4.)),
                Rect::from_min_max(pos2(0., 0.), pos2(1., 1.)),
                Color32::WHITE,
            );
        }
        ClippedPrimitive {
            clip_rect: Rect::EVERYTHING,
            primitive: Primitive::Mesh(mesh),
        }
    }

    #[test]
    fn egui_counts_split_textured_quads() {
        // Text and shapes share the font atlas (the default texture); only
        // meshes sampling another texture count as textured quads.
        let jobs = [
            quad_mesh(TextureId::default(), 5),
            quad_mesh(TextureId::User(7), 3),
            quad_mesh(TextureId::Managed(2), 2),
        ];
        assert_eq!(
            egui_counts(&jobs),
            EguiCounts {
                clipped_primitives: 3,
                mesh_vertices: 4 * 10,
                mesh_indices: 6 * 10,
                textured_quads: 5,
            }
        );
        assert_eq!(egui_counts(&[]), EguiCounts::default());
    }

    #[test]
    fn texture_uploads_count_every_set_entry_and_its_bytes() {
        let mut delta = TexturesDelta::default();
        assert_eq!(texture_uploads(&delta), TextureUploads::default());
        let image = |w, h| ColorImage::filled([w, h], Color32::WHITE);
        // Two partial atlas patches in one frame count as two uploads.
        for pos in [[0, 0], [8, 0]] {
            delta.push(
                TextureId::default(),
                ImageDelta::partial(pos, image(8, 2), TextureOptions::LINEAR),
            );
        }
        delta.push(
            TextureId::Managed(3),
            ImageDelta::full(image(4, 4), TextureOptions::LINEAR),
        );
        delta.free(TextureId::Managed(9));
        assert_eq!(
            texture_uploads(&delta),
            TextureUploads {
                count: 3,
                bytes: 4 * (2 * 8 * 2 + 4 * 4),
            }
        );
        delta.clear();
    }
}
