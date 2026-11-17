//! Battlefield camera: uniform zoom-to-fit. Pure `Rect` math, presentation
//! only; nothing here reads the attempt, the simulation or its random state.
//!
//! The base of every fit is today's per-geometry clearing (`clearing_size`):
//! lane height = reference.h / 11 and yard width = reference.w / 30, clamped to
//! `MIN_CLEARING` and to the reference. `fit_field` scales that base uniformly
//! by the smaller of the two axis ratios, so the lane:yard proportion is kept,
//! fewer lanes or yards zoom in, the limiting axis fills the area exactly and
//! the field is centred. When the area is the reference (`fit_field(r, r, ..)`,
//! the open battle arena and the observer box), a field that already fills an
//! axis (every geometry with 11 or more lanes, or the full 30-yard width) is
//! today's clearing bit for bit. A different area (the first-defense hint
//! band's `below`) still sizes the base from the reference, so the zoom is
//! `area / reference` on the limiting axis and a filling field shrinks under 1.0;
//! `ui::battle::layout` keeps the open fit then.
use bevy_egui::egui::{Rect, Vec2, vec2};

/// Lane count whose clearing fills the reference height.
pub const REFERENCE_LANES: f32 = 11.0;
/// Width in yards whose clearing fills the reference width.
pub const REFERENCE_WIDTH_YARDS: f32 = 30.0;
/// The smallest clearing ever drawn, in points, before zoom.
pub const MIN_CLEARING: Vec2 = vec2(160.0, 86.0);

/// A fitted field: the arena rect the projection maps onto, and the uniform
/// zoom applied to the base clearing (1.0 = today's size).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldFit {
    pub rect: Rect,
    pub zoom: f32,
}

/// Today's clearing size for a geometry inside `reference`, unzoomed: a small
/// opening is a smaller clearing at the normal yard scale, up to the reference.
pub fn clearing_size(reference: Rect, lanes: u8, width_yards: f32) -> Vec2 {
    vec2(
        (reference.width() * (width_yards / REFERENCE_WIDTH_YARDS).clamp(0.0, 1.0))
            .max(MIN_CLEARING.x)
            .min(reference.width()),
        (reference.height() * (f32::from(lanes) / REFERENCE_LANES).clamp(0.0, 1.0))
            .max(MIN_CLEARING.y)
            .min(reference.height()),
    )
}

/// Uniformly zooms the `reference` clearing for (lanes, width_yards) to fit
/// `area`: the smaller axis ratio wins, that axis snaps exactly to the area and
/// the field is centred. No texel or magnification clamp. Zoom 1.0 and the
/// legacy rect for a filling field hold only when `area == reference`; any
/// other area scales the reference clearing by the area's limiting ratio.
pub fn fit_field(reference: Rect, area: Rect, lanes: u8, width_yards: f32) -> FieldFit {
    let base = clearing_size(reference, lanes, width_yards);
    let (zx, zy) = (area.width() / base.x, area.height() / base.y);
    let zoom = zx.min(zy);
    let size = base * zoom;
    let rect = if size == area.size() {
        area
    } else {
        let centred = Rect::from_center_size(area.center(), size).intersect(area);
        if zoom == 1.0 {
            // The base fits the area as is: centred and clipped like the
            // legacy clearing (bit for bit when area == reference).
            centred
        } else if zx <= zy {
            Rect::from_x_y_ranges(area.x_range(), centred.y_range())
        } else {
            Rect::from_x_y_ranges(centred.x_range(), area.y_range())
        }
    };
    FieldFit { rect, zoom }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::ArenaProjection;
    use bevy_egui::egui::pos2;

    /// `ui::battle::clearing_rect` as it stood before the camera, frozen.
    fn legacy_clearing_rect(maximum: Rect, lanes: u8, width: f32) -> Rect {
        let size = vec2(
            (maximum.width() * (width / 30.0).clamp(0.0, 1.0))
                .max(160.0)
                .min(maximum.width()),
            (maximum.height() * (f32::from(lanes) / 11.0).clamp(0.0, 1.0))
                .max(86.0)
                .min(maximum.height()),
        );
        if size == maximum.size() {
            maximum
        } else {
            Rect::from_center_size(maximum.center(), size).intersect(maximum)
        }
    }

    /// Today's `battle_arena(canvas, false)` at 1440x900 and 1050x700@1.35.
    fn target_arenas() -> [Rect; 2] {
        [vec2(1440.0, 900.0), vec2(1050.0 / 1.35, 700.0 / 1.35)].map(|canvas| {
            Rect::from_min_max(pos2(24.0, 124.0), pos2(canvas.x - 24.0, canvas.y - 84.0))
        })
    }

    /// The target arenas plus observer-sized boxes.
    fn fit_areas() -> Vec<Rect> {
        let mut areas = target_arenas().to_vec();
        for size in [vec2(180.0, 180.0), vec2(360.0, 360.0), vec2(360.0, 180.0)] {
            areas.push(Rect::from_min_size(pos2(40.0, 60.0), size));
        }
        areas
    }

    fn close(actual: f32, expected: f32, tolerance: f32) -> bool {
        (actual - expected).abs() <= tolerance
    }

    #[test]
    fn fit_field_zoom_table_at_both_target_sizes() {
        /// (lanes, width, field at 1440x900, field at 1050x700@1.35, zoom)
        type ZoomRow = (u8, f32, [f32; 2], [f32; 2], f32);
        const TABLE: [ZoomRow; 4] = [
            (3, 12.0, [1392.0, 471.8], [729.8, 215.0], 2.5),
            (5, 14.0, [1392.0, 674.0], [729.8, 302.5], 2.143),
            (7, 16.0, [1166.6, 692.0], [611.6, 310.5], 1.571),
            (9, 18.0, [1020.8, 692.0], [535.2, 310.5], 1.222),
        ];
        let [large, small] = target_arenas();
        for (lanes, width, at_large, at_small, zoom) in TABLE {
            for (arena, expected) in [(large, at_large), (small, at_small)] {
                let fit = fit_field(arena, arena, lanes, width);
                let label = format!("{lanes}x{width} in {:?}", arena.size());
                assert!(
                    close(fit.rect.width(), expected[0], 0.05)
                        && close(fit.rect.height(), expected[1], 0.05),
                    "{label}: field {:?}, expected {expected:?}",
                    fit.rect.size()
                );
                assert!(
                    close(fit.zoom, zoom, 0.05),
                    "{label}: zoom {}, expected {zoom}",
                    fit.zoom
                );
                assert!(arena.contains_rect(fit.rect), "{label}: leaves the arena");
                assert!(
                    (fit.rect.center() - arena.center()).length() < 0.01,
                    "{label}: not centred"
                );
            }
        }
    }

    #[test]
    fn arena_fits_full_lanes_and_width_on_the_limiting_axis() {
        for area in fit_areas() {
            for lanes in (3..=17).step_by(2) {
                for width in 8..=30u8 {
                    let width = f32::from(width);
                    let label = format!("{lanes}x{width} in {:?}", area.size());
                    let base = clearing_size(area, lanes, width);
                    let fit = fit_field(area, area, lanes, width);
                    assert!(fit.zoom >= 1.0, "{label}: zoom {} shrinks", fit.zoom);
                    assert!(area.contains_rect(fit.rect), "{label}: leaves the area");
                    // Uniform: both axes carry the same zoom, so lane:yard holds.
                    let scale = fit.rect.size() / base;
                    assert!(
                        close(scale.x, fit.zoom, 1e-4) && close(scale.y, fit.zoom, 1e-4),
                        "{label}: axis scales {scale:?} against zoom {}",
                        fit.zoom
                    );
                    // The limiting axis fills the area; exactly when zoomed.
                    let fills_x = fit.rect.x_range() == area.x_range();
                    let fills_y = fit.rect.y_range() == area.y_range();
                    if fit.zoom > 1.0 {
                        assert!(fills_x || fills_y, "{label}: {:?} snaps no axis", fit.rect);
                    } else {
                        assert!(
                            close(fit.rect.width(), area.width(), 1e-3)
                                || close(fit.rect.height(), area.height(), 1e-3),
                            "{label}: {:?} fills no axis",
                            fit.rect
                        );
                    }
                    assert!(
                        (fit.rect.center() - area.center()).length() < 0.01,
                        "{label}: not centred"
                    );
                    // Every lane and the whole width project inside the area.
                    let projection = ArenaProjection::with_geometry(fit.rect, lanes, width);
                    let near = projection.world(0.0, 0.0);
                    let far = projection.world(width, f32::from(lanes));
                    assert!(
                        area.contains(near) && area.contains(far),
                        "{label}: corners {near:?} {far:?} outside"
                    );
                    assert!(
                        close(
                            projection.lane_height(),
                            base.y / f32::from(lanes) * fit.zoom,
                            1e-3
                        ),
                        "{label}: lane height {}",
                        projection.lane_height()
                    );
                }
            }
        }
    }

    /// `fit_field(reference, area, ..)` with `area` the hint band's `below`
    /// (the reference minus its top 92 pt): the base stays the reference's
    /// clearing, so a field filling the reference is zoomed below 1.0 to the
    /// shorter area, not kept bit for bit, and a zoomed field never zooms more.
    #[test]
    fn fit_field_in_a_smaller_area_scales_the_reference_clearing() {
        for reference in target_arenas() {
            let below = Rect::from_min_max(reference.min + vec2(0.0, 92.0), reference.max);
            for (lanes, width) in [(17, 30.0), (11, 20.0), (3, 12.0), (7, 16.0)] {
                let label = format!("{lanes}x{width} in {:?}", reference.size());
                let base = clearing_size(reference, lanes, width);
                let open = fit_field(reference, reference, lanes, width);
                let fit = fit_field(reference, below, lanes, width);
                let expected = (below.width() / base.x).min(below.height() / base.y);
                assert!(
                    close(fit.zoom, expected, 1e-5),
                    "{label}: zoom {}",
                    fit.zoom
                );
                assert!(fit.zoom <= open.zoom, "{label}: the band zooms it in");
                let scale = fit.rect.size() / base;
                assert!(
                    close(scale.x, fit.zoom, 1e-4) && close(scale.y, fit.zoom, 1e-4),
                    "{label}: axis scales {scale:?} against zoom {}",
                    fit.zoom
                );
                assert!(below.contains_rect(fit.rect), "{label}: leaves the area");
                assert!(
                    fit.rect.x_range() == below.x_range() || fit.rect.y_range() == below.y_range(),
                    "{label}: {:?} snaps no axis",
                    fit.rect
                );
                assert!(
                    (fit.rect.center() - below.center()).length() < 0.01,
                    "{label}: not centred"
                );
                if open.zoom == 1.0 {
                    // Filling the reference: shrunk under the band, never
                    // today's rect.
                    assert!(fit.zoom < 1.0, "{label}: zoom {}", fit.zoom);
                    assert_ne!(fit.rect, legacy_clearing_rect(reference, lanes, width));
                }
            }
        }
    }

    /// Pin: with `area == reference`, every geometry whose clearing already
    /// fills an axis (all fields with 11 or more lanes, and the full 30-yard
    /// width) keeps today's rect bit for bit and is not zoomed.
    #[test]
    fn fit_field_keeps_every_filling_geometry_bit_identical() {
        // The two named sentinels: max lanes at full width, and 11 lanes at
        // 20 yards (height-filling, width-short).
        for arena in target_arenas() {
            for (lanes, width) in [(17, 30.0), (11, 20.0)] {
                let fit = fit_field(arena, arena, lanes, width);
                assert_eq!(
                    (fit.rect, fit.zoom),
                    (legacy_clearing_rect(arena, lanes, width), 1.0),
                    "{lanes}x{width} in {:?} moved",
                    arena.size()
                );
            }
        }
        let mut moved = Vec::new();
        let mut pinned = 0;
        for area in fit_areas() {
            for lanes in 1..=17u8 {
                for width in 8..=30u8 {
                    let width = f32::from(width);
                    let legacy = legacy_clearing_rect(area, lanes, width);
                    let fills = legacy.width() == area.width() || legacy.height() == area.height();
                    if lanes < 11 && !fills {
                        continue;
                    }
                    pinned += 1;
                    let fit = fit_field(area, area, lanes, width);
                    if fit.rect != legacy || fit.zoom != 1.0 {
                        moved.push(format!(
                            "{lanes}x{width} in {:?}: {:?} z{} vs legacy {:?}",
                            area.size(),
                            fit.rect,
                            fit.zoom,
                            legacy
                        ));
                    }
                }
            }
        }
        assert!(pinned > 500, "the pin covers only {pinned} geometries");
        assert!(
            moved.is_empty(),
            "{} filling geometries moved, first: {:#?}",
            moved.len(),
            &moved[..moved.len().min(4)]
        );
    }
}
