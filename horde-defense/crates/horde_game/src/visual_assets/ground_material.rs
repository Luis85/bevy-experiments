//! Read-only terrain material composition. Original atlas pixels and simulation
//! coordinates are immutable; sparse edge blending keeps the ground continuous.
use super::{ArtTextures, GROUND_SIZE};
use bevy_egui::egui::{self, Color32, Painter, Rect, Vec2, pos2, vec2};

pub(super) fn ground(p: &Painter, rect: Rect, textures: ArtTextures, theme: usize) -> bool {
    let Some(texture) = textures.ground else {
        return false;
    };
    let (biome, soil) = match theme % 5 {
        2 => (2, Color32::from_rgb(76, 53, 31)),
        3 => (1, Color32::from_rgb(36, 49, 55)),
        4 => (3, Color32::from_rgb(15, 41, 20)),
        _ => (0, Color32::from_rgb(22, 38, 27)),
    };
    p.rect_filled(rect, 0., soil);
    let clipped = p.with_clip_rect(rect.intersect(p.clip_rect()));
    // The source swatch is enlarged pixel art. Sample it at the same cluster
    // density as the actors rather than stretching each motif across a lane.
    const MATERIAL_SCALE: f32 = 0.2;
    const STEP: Vec2 = vec2(56., 48.);
    const SIZE: Vec2 = vec2(92., 73.6);
    let left = (rect.left() / STEP.x).floor() as i32 - 2;
    let top = (rect.top() / STEP.y).floor() as i32 - 2;
    let right = (rect.right() / STEP.x).ceil() as i32 + 1;
    let bottom = (rect.bottom() / STEP.y).ceil() as i32 + 1;
    // All patches share one material/clip, so submit one bounded mesh instead
    // of allocating hundreds of independent terrain shapes every frame.
    let mut mesh = egui::epaint::Mesh::with_texture(texture);
    for row in top..bottom.min(top + 32) {
        for column in left..right.min(left + 48) {
            let seed = (column.wrapping_mul(73_856_093) ^ row.wrapping_mul(19_349_663)) as u32;
            let offset =
                vec2((seed % 113) as f32 - 56., ((seed >> 8) % 97) as f32 - 48.) * MATERIAL_SCALE;
            let patch = Rect::from_min_size(
                vec2(column as f32 * STEP.x, row as f32 * STEP.y).to_pos2() + offset,
                SIZE,
            );
            if !patch.intersects(rect) {
                continue;
            }
            let source_x = biome as f32 * (GROUND_SIZE[0] / 4.) + (seed % 42) as f32;
            let source_y = ((seed >> 12) % 324) as f32;
            let first = mesh.vertices.len() as u32;
            // Preserve material contrast over most of each patch. The old20%
            // interior opacity erased its texture into an almost uniform base.
            let stops = [0., 0.09, 0.91, 1.];
            for (y, v) in stops.into_iter().enumerate() {
                for (x, u) in stops.into_iter().enumerate() {
                    let alpha = if x == 0 || x == 3 || y == 0 || y == 3 {
                        0
                    } else {
                        192
                    };
                    mesh.vertices.push(egui::epaint::Vertex {
                        pos: patch.min + vec2(u * SIZE.x, v * SIZE.y),
                        uv: pos2(
                            (source_x + u * 500.) / GROUND_SIZE[0],
                            (source_y + v * 400.) / GROUND_SIZE[1],
                        ),
                        color: Color32::from_white_alpha(alpha),
                    });
                }
            }
            for y in 0..3 {
                for x in 0..3 {
                    let a = first + y * 4 + x;
                    mesh.add_triangle(a, a + 1, a + 5);
                    mesh.add_triangle(a, a + 5, a + 4);
                }
            }
        }
    }
    if !mesh.is_empty() {
        clipped.add(egui::Shape::mesh(mesh));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_at(mesh: &egui::epaint::Mesh, point: egui::Pos2) -> f32 {
        mesh.indices
            .as_chunks::<3>()
            .0
            .iter()
            .filter_map(|ids| {
                let [a, b, c] = [
                    mesh.vertices[ids[0] as usize],
                    mesh.vertices[ids[1] as usize],
                    mesh.vertices[ids[2] as usize],
                ];
                let v = b.pos - a.pos;
                let w = c.pos - a.pos;
                let d = point - a.pos;
                let cross = |x: Vec2, y: Vec2| x.x * y.y - x.y * y.x;
                let den = cross(v, w);
                if den.abs() < 0.001 {
                    return None;
                }
                let u = cross(d, w) / den;
                let t = cross(v, d) / den;
                (u >= -0.001 && t >= -0.001 && u + t <= 1.001).then_some(
                    (1. - u - t) * f32::from(a.color.a())
                        + u * f32::from(b.color.a())
                        + t * f32::from(c.color.a()),
                )
            })
            .fold(0., f32::max)
    }

    #[test]
    fn original_material_is_visible_across_the_viewport_and_stays_in_its_biome_swatch() {
        let ctx = egui::Context::default();
        let viewport = Rect::from_min_size(pos2(0., 0.), vec2(1440., 900.));
        for (theme, biome) in [(0, 0), (1, 0), (2, 2), (3, 1), (4, 3)] {
            let texture = egui::TextureId::User(90 + theme as u64);
            let mut output = ctx.run_ui(Default::default(), |ui| {
                assert!(ground(
                    ui.painter(),
                    viewport,
                    ArtTextures {
                        ground: Some(texture),
                        ..Default::default()
                    },
                    theme
                ));
            });
            let meshes: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == texture => Some(mesh.as_ref()),
                    _ => None,
                })
                .collect();
            for mesh in &meshes {
                for vertex in &mesh.vertices {
                    assert!(
                        vertex.uv.x >= biome as f32 / 4. && vertex.uv.x < (biome + 1) as f32 / 4.
                    );
                    assert!((0.0..=1.0).contains(&vertex.uv.y));
                }
            }
            for y in [50., 200., 400., 650., 850.] {
                for x in [40., 200., 480., 720., 1000., 1250., 1400.] {
                    let alpha = meshes
                        .iter()
                        .map(|mesh| alpha_at(mesh, pos2(x, y)))
                        .fold(0., f32::max);
                    assert!(
                        alpha >= 150.,
                        "theme{theme} at{x},{y}: material faded to{alpha}; terrain must not become a flat low-opacity wash"
                    );
                }
            }
            output.textures_delta.clear();
        }
    }
}
