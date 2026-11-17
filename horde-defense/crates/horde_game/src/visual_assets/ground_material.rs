//! Read-only terrain material composition. Original atlas pixels and simulation
//! coordinates are immutable; sparse edge blending keeps the ground continuous.
//!
//! Both ground meshes of a battle (the canvas around the field and the field
//! itself) are built once and painted again from a small cache of
//! `Arc<Mesh>` in egui temp data, keyed by [`GroundKey`]: the patches depend
//! only on the rectangle, the hidden field, the theme and the texture, so a
//! pass that changes none of them paints the very same mesh.
use super::{ArtTextures, GROUND_SIZE};
use bevy_egui::egui::{self, Color32, Painter, Rect, TextureId, Vec2, epaint::Mesh, pos2, vec2};
use std::sync::Arc;

/// How far inside a hidden field a canvas ground patch must lie to be left
/// out. The field's opaque soil is rounded to whole pixels and feathered by
/// one, so its fully opaque interior can sit up to a pixel inside its rect:
/// four points cover that down to a quarter pixel per point.
pub(crate) const HIDDEN_GROUND_MARGIN: f32 = 4.0;

/// What a ground mesh was built for. Any change builds another one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GroundKey {
    rect_bits: [u32; 4],
    /// The field the caller paints over this ground afterwards, if any.
    hidden_bits: Option<[u32; 4]>,
    theme: usize,
    texture: TextureId,
}

/// The most recently painted ground meshes, newest first. Eight covers the
/// canvas and field grounds of the main battle and of a few observer views
/// in one pass; a window resize builds new meshes and the old ones age out.
#[derive(Clone, Default)]
struct GroundMeshes(Vec<(GroundKey, Arc<Mesh>)>);
const GROUND_CACHE_SLOTS: usize = 8;

fn bits(rect: Rect) -> [u32; 4] {
    [rect.min.x, rect.min.y, rect.max.x, rect.max.y].map(f32::to_bits)
}

fn cache_id() -> egui::Id {
    egui::Id::new("lastlight_ground_meshes")
}

/// Paints the soil and material patches of `rect`. `hidden` is a field the
/// caller covers afterwards with its own opaque ground: patches lying
/// [`HIDDEN_GROUND_MARGIN`] inside it are never seen and are left out.
pub(super) fn ground(
    p: &Painter,
    rect: Rect,
    hidden: Option<Rect>,
    textures: ArtTextures,
    theme: usize,
) -> bool {
    let Some(texture) = textures.ground else {
        return false;
    };
    p.rect_filled(rect, 0., soil(theme));
    let key = GroundKey {
        rect_bits: bits(rect),
        hidden_bits: hidden.map(bits),
        theme,
        texture,
    };
    let id = cache_id();
    let cached = p.ctx().data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<GroundMeshes>(id);
        let n = cache.0.iter().position(|(k, _)| *k == key)?;
        let entry = cache.0.remove(n);
        let mesh = Arc::clone(&entry.1);
        cache.0.insert(0, entry);
        Some(mesh)
    });
    let mesh = cached.unwrap_or_else(|| {
        let mesh = Arc::new(build(rect, hidden, texture, theme));
        p.ctx().data_mut(|data| {
            let cache = data.get_temp_mut_or_default::<GroundMeshes>(id);
            cache.0.insert(0, (key, Arc::clone(&mesh)));
            cache.0.truncate(GROUND_CACHE_SLOTS);
        });
        mesh
    });
    if !mesh.is_empty() {
        p.with_clip_rect(rect.intersect(p.clip_rect()))
            .add(egui::Shape::Mesh(mesh));
    }
    true
}

fn soil(theme: usize) -> Color32 {
    match theme % 5 {
        2 => Color32::from_rgb(76, 53, 31),
        3 => Color32::from_rgb(36, 49, 55),
        4 => Color32::from_rgb(15, 41, 20),
        _ => Color32::from_rgb(22, 38, 27),
    }
}

/// The material patches of `rect`, minus those `hidden` covers.
fn build(rect: Rect, hidden: Option<Rect>, texture: TextureId, theme: usize) -> Mesh {
    let biome = match theme % 5 {
        2 => 2,
        3 => 1,
        4 => 3,
        _ => 0,
    };
    let covered = hidden.map(|field| field.shrink(HIDDEN_GROUND_MARGIN));
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
    let mut mesh = Mesh::with_texture(texture);
    for row in top..bottom.min(top + 32) {
        for column in left..right.min(left + 48) {
            let seed = (column.wrapping_mul(73_856_093) ^ row.wrapping_mul(19_349_663)) as u32;
            let offset =
                vec2((seed % 113) as f32 - 56., ((seed >> 8) % 97) as f32 - 48.) * MATERIAL_SCALE;
            let patch = Rect::from_min_size(
                vec2(column as f32 * STEP.x, row as f32 * STEP.y).to_pos2() + offset,
                SIZE,
            );
            if !patch.intersects(rect) || covered.is_some_and(|field| field.contains_rect(patch)) {
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
    mesh
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

    fn ground_meshes(
        output: &egui::FullOutput,
        texture: egui::TextureId,
    ) -> Vec<std::sync::Arc<egui::epaint::Mesh>> {
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == texture => Some(mesh.clone()),
                _ => None,
            })
            .collect()
    }

    /// One pass painting the canvas ground around `field`, then the field's
    /// own ground: their two meshes.
    fn ground_pass(
        ctx: &egui::Context,
        canvas: Rect,
        field: Rect,
        theme: usize,
        texture: u64,
    ) -> [std::sync::Arc<egui::epaint::Mesh>; 2] {
        let textures = ArtTextures {
            ground: Some(egui::TextureId::User(texture)),
            ..Default::default()
        };
        let mut output = ctx.run_ui(Default::default(), |ui| {
            assert!(ground(ui.painter(), canvas, Some(field), textures, theme));
            assert!(ground(ui.painter(), field, None, textures, theme));
        });
        output.textures_delta.clear();
        let meshes = ground_meshes(&output, egui::TextureId::User(texture));
        assert_eq!(meshes.len(), 2, "one canvas and one field mesh");
        [meshes[0].clone(), meshes[1].clone()]
    }

    #[test]
    fn ground_mesh_is_reused_across_passes() {
        use std::sync::Arc;
        let ctx = egui::Context::default();
        let canvas = Rect::from_min_size(pos2(0., 0.), vec2(1440., 900.));
        let field = Rect::from_min_max(pos2(24., 124.), pos2(1416., 816.));
        let first = ground_pass(&ctx, canvas, field, 0, 7);
        let second = ground_pass(&ctx, canvas, field, 0, 7);
        for (which, (a, b)) in ["canvas", "field"]
            .into_iter()
            .zip(first.iter().zip(&second))
        {
            assert!(
                Arc::ptr_eq(a, b),
                "an unchanged {which} ground must paint its cached mesh"
            );
        }
        let narrower = Rect::from_min_max(pos2(260., 124.), pos2(1180., 816.));
        // (canvas, field, theme, texture) and which of the two meshes change.
        let cases = [
            (canvas, field, 2, 7, [true, true]),
            (canvas.shrink(40.), field, 0, 7, [true, false]),
            (canvas, narrower, 0, 7, [true, true]),
            (canvas, field, 0, 8, [true, true]),
        ];
        for (canvas_rect, field_rect, theme, texture, rebuilt) in cases {
            let label = format!("{canvas_rect:?} around {field_rect:?}, theme {theme}");
            let changed = ground_pass(&ctx, canvas_rect, field_rect, theme, texture);
            let fresh = ground_pass(
                &egui::Context::default(),
                canvas_rect,
                field_rect,
                theme,
                texture,
            );
            for n in 0..2 {
                assert_eq!(changed[n], fresh[n], "{label}: a stale ground was painted");
                if !rebuilt[n] {
                    assert!(
                        Arc::ptr_eq(&changed[n], &first[n]),
                        "{label}: an unchanged ground went back to its cached mesh"
                    );
                } else {
                    assert!(
                        !Arc::ptr_eq(&changed[n], &first[n]),
                        "{label}: the ground was not rebuilt"
                    );
                }
            }
        }
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
                    None,
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
