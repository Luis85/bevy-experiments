use super::ArenaProjection;
use crate::visual_assets::{self, ArtTextures, Prop};
use bevy_egui::egui::{
    self, Color32 as C, Painter, Pos2, Rect, Shape, Stroke, TextureId, Vec2, pos2, vec2,
};
use std::sync::Arc;

pub(super) fn line(p: &Painter, a: Pos2, b: Pos2, width: f32, c: C) {
    p.line_segment([a, b], Stroke::new(width, c));
}
pub(super) fn box_at(p: &Painter, center: Pos2, size: Vec2, c: C) {
    p.rect_filled(Rect::from_center_size(center, size), 0.0, c);
}

pub(super) fn paint_ground(
    p: &Painter,
    r: Rect,
    hidden: Option<Rect>,
    theme: usize,
    textures: ArtTextures,
) {
    if !visual_assets::ground(p, r, hidden, textures, theme) {
        let soils = [
            C::from_rgb(46, 65, 52),
            C::from_rgb(48, 62, 62),
            C::from_rgb(91, 79, 55),
            C::from_rgb(59, 70, 72),
            C::from_rgb(40, 65, 49),
        ];
        p.rect_filled(r, 0., soils[theme % soils.len()]);
    }
}

/// The forest belongs to the world surrounding the complete projected arena.
/// Clip every prop to its peripheral strip so dressing can never conceal a
/// combat lane, a contact position, a construction cell or the aiming origin.
///
/// Props scale with the lane height up to `max_unit` (a lane of the reference
/// clearing), so a zoomed field never grows them past today's size; fields
/// with 11 or more lanes are below the cap and unchanged.
///
/// The caller paints the field (`art::battlefield`) over `projection.rect`
/// afterwards, so the canvas ground leaves out the patches under it. The
/// dressing is placed once per [`DressingKey`] and painted again from a
/// small cache in egui temp data, like the ground meshes.
pub fn world_backdrop(
    p: &Painter,
    canvas: Rect,
    projection: ArenaProjection,
    theme: usize,
    textures: ArtTextures,
    max_unit: f32,
) {
    let arena = projection.rect;
    paint_ground(p, canvas, Some(arena), theme, textures);
    let Some(props) = textures.props else {
        return;
    };
    let unit = dressing_unit(projection.lane_height(), max_unit);
    let key = DressingKey {
        canvas: bits(canvas),
        arena: bits(arena),
        clip: bits(p.clip_rect()),
        theme,
        props,
        unit: unit.to_bits(),
    };
    let id = egui::Id::new("lastlight_backdrop_dressing");
    let cached = p.ctx().data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<DressingCache>(id);
        let n = cache.0.iter().position(|(k, _)| *k == key)?;
        let entry = cache.0.remove(n);
        let strips = Arc::clone(&entry.1);
        cache.0.insert(0, entry);
        Some(strips)
    });
    let strips = cached.unwrap_or_else(|| {
        let strips: Arc<[DressedStrip]> =
            dressing(canvas, arena, p.clip_rect(), theme, textures, unit).into();
        p.ctx().data_mut(|data| {
            let cache = data.get_temp_mut_or_default::<DressingCache>(id);
            cache.0.insert(0, (key, Arc::clone(&strips)));
            cache.0.truncate(DRESSING_CACHE_SLOTS);
        });
        strips
    });
    for (clip, shapes) in strips.iter() {
        let clipped = p.with_clip_rect(*clip);
        for shape in shapes {
            clipped.add(shape.clone());
        }
    }
}

/// What a backdrop's dressing was placed for. Any change places it again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DressingKey {
    canvas: [u32; 4],
    arena: [u32; 4],
    /// The painter's clip, which every strip's clip is cut from.
    clip: [u32; 4],
    theme: usize,
    props: TextureId,
    unit: u32,
}

/// One peripheral strip: its clip and its props, far feet first.
type DressedStrip = (Rect, Vec<Shape>);

/// The most recently painted dressings, newest first: the main battle and a
/// few observer views.
#[derive(Clone, Default)]
struct DressingCache(Vec<(DressingKey, Arc<[DressedStrip]>)>);
const DRESSING_CACHE_SLOTS: usize = 4;

fn bits(rect: Rect) -> [u32; 4] {
    [rect.min.x, rect.min.y, rect.max.x, rect.max.y].map(f32::to_bits)
}

/// Places the dressing of every peripheral strip around `arena`.
fn dressing(
    canvas: Rect,
    arena: Rect,
    clip: Rect,
    theme: usize,
    textures: ArtTextures,
    unit: f32,
) -> Vec<DressedStrip> {
    let strips = peripheral_strips(canvas, arena);
    let mut dressed = Vec::with_capacity(strips.len());
    for (edge, strip) in strips.into_iter().enumerate() {
        if !strip.is_positive() {
            continue;
        }
        let horizontal = edge < 2;
        let span = if horizontal {
            arena.width()
        } else {
            arena.height()
        };
        let spacing = match theme % 5 {
            2 => 2.8,
            4 => 1.05,
            _ => 1.35,
        };
        let count = ((span / (unit * spacing)).ceil() as usize).clamp(1, 44);
        let mut dressing = Vec::with_capacity(count + 3);
        for i in 0..count {
            // Presentation-only variation never consumes simulation randomness.
            let noise = ((i * 37 + edge * 13 + theme * 7) % 23) as f32 / 23.;
            let along = (i as f32 + 0.15 + noise * 0.7) / count as f32;
            let prop = peripheral_kind(theme, i + edge * 3);
            let height = unit
                * match prop {
                    Prop::TreeBroadleaf | Prop::TreePine => 1.35 + noise * 0.5,
                    Prop::Bush => 0.55 + noise * 0.35,
                    _ => 0.45 + noise * 0.3,
                };
            let tint = if theme % 5 == 2 {
                C::from_rgb(174, 157, 123)
            } else {
                C::from_rgb(176, 193, 174)
            };
            let size = vec2(unit * 1.8, height);
            if let Some(foot) = perimeter_foot(prop, size, arena, strip, edge, along, unit * 0.08) {
                dressing.push((prop, foot, size, tint));
            }
        }
        // A few outpost objects share the trees' ground plane. Their feet,
        // rather than their image centers, determine depth and placement.
        if horizontal {
            for &(prop, along) in peripheral_details(theme, edge) {
                let height = if prop == Prop::Lamp {
                    unit * 1.35
                } else {
                    unit * 0.75
                };
                let size = vec2(unit * 1.5, height);
                if let Some(foot) =
                    perimeter_foot(prop, size, arena, strip, edge, along, unit * 0.08)
                {
                    dressing.push((prop, foot, size, C::from_rgb(158, 170, 153)));
                }
            }
        }
        dressing.sort_by(|a, b| a.1.y.total_cmp(&b.1.y).then(a.1.x.total_cmp(&b.1.x)));
        let shapes = dressing
            .into_iter()
            .filter_map(|(prop, foot, size, tint)| {
                visual_assets::prop_grounded_shape(textures, prop, foot, size, tint)
            })
            .collect();
        dressed.push((strip.intersect(clip), shapes));
    }
    dressed
}

/// The dressing's size unit: the lane height, capped at `max_unit`, at least
/// one point.
pub(super) fn dressing_unit(lane_height: f32, max_unit: f32) -> f32 {
    lane_height.min(max_unit).max(1.)
}

pub(super) fn peripheral_strips(canvas: Rect, arena: Rect) -> [Rect; 4] {
    [
        Rect::from_min_max(canvas.min, pos2(canvas.right(), arena.top())),
        Rect::from_min_max(pos2(canvas.left(), arena.bottom()), canvas.max),
        Rect::from_min_max(pos2(canvas.left(), arena.top()), arena.left_bottom()),
        Rect::from_min_max(arena.right_top(), pos2(canvas.right(), arena.bottom())),
    ]
}

/// Place the complete sprite just outside its legal clearing edge. Grounded
/// source bounds, rather than sprite centers or the window edge, set the gap.
pub(super) fn perimeter_foot(
    prop: Prop,
    size: Vec2,
    arena: Rect,
    strip: Rect,
    edge: usize,
    along: f32,
    gap: f32,
) -> Option<Pos2> {
    let bounds = visual_assets::prop_ground_rect(prop, Pos2::ZERO, size);
    let foot = match edge {
        0 => pos2(
            arena.left() + arena.width() * along - bounds.center().x,
            arena.top() - gap - bounds.bottom(),
        ),
        1 => pos2(
            arena.left() + arena.width() * along - bounds.center().x,
            arena.bottom() + gap - bounds.top(),
        ),
        2 => pos2(
            arena.left() - gap - bounds.right(),
            arena.top() + arena.height() * along - bounds.center().y,
        ),
        _ => pos2(
            arena.right() + gap - bounds.left(),
            arena.top() + arena.height() * along - bounds.center().y,
        ),
    };
    inset_tree_foot(prop, foot, size, strip)
}

pub(super) fn inset_tree_foot(prop: Prop, foot: Pos2, size: Vec2, strip: Rect) -> Option<Pos2> {
    let safe = strip.shrink(2.0);
    let sprite = visual_assets::prop_ground_rect(prop, foot, size);
    if !safe.is_positive() || sprite.width() > safe.width() || sprite.height() > safe.height() {
        return None;
    }
    let dx = if sprite.left() < safe.left() {
        safe.left() - sprite.left()
    } else if sprite.right() > safe.right() {
        safe.right() - sprite.right()
    } else {
        0.0
    };
    let dy = if sprite.top() < safe.top() {
        safe.top() - sprite.top()
    } else if sprite.bottom() > safe.bottom() {
        safe.bottom() - sprite.bottom()
    } else {
        0.0
    };
    Some(foot + vec2(dx, dy))
}

pub(super) fn peripheral_kind(theme: usize, index: usize) -> Prop {
    match theme % 5 {
        2 => match index % 7 {
            1 => Prop::Debris,
            4 => Prop::Fence,
            6 => Prop::Bush,
            _ => Prop::Rock,
        },
        4 => {
            if index % 4 == 1 {
                Prop::Bush
            } else {
                Prop::TreeBroadleaf
            }
        }
        3 => {
            if index % 7 == 2 {
                Prop::Bush
            } else {
                Prop::TreePine
            }
        }
        _ => match index % 5 {
            1 => Prop::Bush,
            2 | 4 => Prop::TreeBroadleaf,
            _ => Prop::TreePine,
        },
    }
}
pub(super) fn peripheral_details(theme: usize, edge: usize) -> &'static [(Prop, f32)] {
    match (theme % 5, edge) {
        (2, 0) => &[(Prop::Wreck, 0.66), (Prop::Fence, 0.24)],
        (2, _) => &[(Prop::Debris, 0.45)],
        (4, 0) => &[(Prop::Fence, 0.23)],
        (4, _) => &[(Prop::Rock, 0.74)],
        (3, 0) => &[(Prop::Wreck, 0.68)],
        (3, _) => &[(Prop::Debris, 0.26)],
        (_, 0) => &[(Prop::Fence, 0.18), (Prop::Wreck, 0.66), (Prop::Lamp, 0.72)],
        _ => &[(Prop::Debris, 0.35)],
    }
}

/// The vignette pass of a battlefield (`battlefield::render`, after the
/// lighting grade and under every cue): one gentle edge falloff over the
/// complete world. Central combat remains fully lit; the fixed aim origin
/// and geometry do not move.
pub(super) fn edge_shade(p: &Painter, canvas: Rect) {
    let mut mesh = egui::epaint::Mesh::default();
    let depth = canvas.height().min(canvas.width()) * 0.16;
    // Smooth vertex color interpolation gives darker corners and gentler side
    // falloff, preserving the survivor anchored close to the left-hand edge.
    for (inset, strength) in [(0., 1.), (depth * 0.45, 0.35), (depth, 0.)] {
        let r = canvas.shrink(inset);
        let positions = [
            r.left_top(),
            r.center_top(),
            r.right_top(),
            r.right_center(),
            r.right_bottom(),
            r.center_bottom(),
            r.left_bottom(),
            r.left_center(),
        ];
        for (at, opacity) in positions
            .into_iter()
            .zip([185., 110., 185., 75., 185., 125., 185., 75.])
        {
            mesh.colored_vertex(at, C::from_black_alpha((opacity * strength) as u8));
        }
    }
    for ring in 0..2 {
        for side in 0..8 {
            let a = ring * 8 + side;
            let b = ring * 8 + (side + 1) % 8;
            mesh.add_triangle(a, b, b + 8);
            mesh.add_triangle(a, b + 8, a + 8);
        }
    }
    p.add(egui::Shape::mesh(mesh));
}
