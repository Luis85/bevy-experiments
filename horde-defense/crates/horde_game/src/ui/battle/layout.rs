//! The main battle screen's geometry, built once per battle frame: pure `Rect`
//! math over the canvas, the attempt's frozen lane count and width, and
//! whether the attempt reserves a first-defense hint band. Nothing here reads
//! settings, the simulation or its random state.
//!
//! The arena is the bounded viewport between the HUD band and the control bar.
//! The field is `art::fit_field` of the geometry's clearing inside it: a
//! uniform zoom-to-fit that keeps the lane:yard proportion, so every
//! already-filling field (17x30, 11x20, every field with 11 or more lanes) is
//! today's rect bit for bit. Construction never moves the field: its panel is
//! an overlay anchored to the arena.
use crate::{
    art::{self, FieldFit, camera::REFERENCE_LANES},
    battle_view::EnemyView,
};
use bevy_egui::egui::{Pos2, Rect, Vec2, pos2, vec2};

/// Canvas rows above the arena: the corner HUD, the attack selector and the
/// loot receipt.
pub(super) const TOP_BAND: f32 = 124.0;
/// Canvas columns left and right of the arena, where the forest shows.
pub(super) const ARENA_SIDE_INSET: f32 = 24.0;
/// Canvas rows under the arena: the control bar and the key legend.
pub(super) const ARENA_BOTTOM_INSET: f32 = 84.0;
/// Arena rows kept clear above the field on a first personal defense: the
/// 84-pt hint callout plus 4-pt margins.
pub(super) const HINT_BAND: f32 = 92.0;
/// The hint callout's offset under the arena top.
pub(super) const HINT_CALLOUT_TOP: f32 = 4.0;
/// The hint callout's height.
pub(super) const HINT_CALLOUT_HEIGHT: f32 = 84.0;
/// Overlay effects (damage, loot, reticle) may rise this far above the arena,
/// so top-lane fly-ups are not cut at the arena edge.
pub(super) const FX_TOP_HEADROOM: f32 = 60.0;
/// The construction panel's widest size.
pub(super) const PANEL_MAX_WIDTH: f32 = 280.0;
/// The construction panel's gap to the canvas's right edge.
pub(super) const PANEL_RIGHT_MARGIN: f32 = 16.0;
/// The construction panel's gap under the arena top (and over its bottom).
pub(super) const PANEL_TOP_GAP: f32 = 8.0;
/// The construction panel's shortest size.
pub(super) const PANEL_MIN_HEIGHT: f32 = 180.0;
/// The construction panel's black fill alpha.
pub(super) const PANEL_FILL_ALPHA: u8 = 235;
/// How far left of the construction panel an INCOMING chevron's centre sits.
pub(super) const CHEVRON_GAP: f32 = 10.0;
/// The INCOMING chevron's width and height.
pub(super) const CHEVRON_SIZE: Vec2 = vec2(12.0, 16.0);

/// Every rect the main battle screen lays out, from one canvas. Later owners
/// may append fields; none is redefined.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct BattleLayout {
    /// The battle panel's whole rect.
    pub(in crate::ui) canvas: Rect,
    /// The bounded viewport the field fits into; the battlefield sprite clip.
    pub(in crate::ui) arena: Rect,
    /// The projected field: world mapping, input and the forest's inner edge.
    pub(in crate::ui) field: FieldFit,
    /// The reserved band over the field, when the attempt asked for one and
    /// the field still zooms (`None` otherwise).
    pub(in crate::ui) hint_band: Option<Rect>,
    /// Where the first-defense hint callout is drawn.
    pub(in crate::ui) hint_callout: Rect,
    /// The clip of the overlay effects passes (damage, loot, reticle).
    pub(in crate::ui) effects_clip: Rect,
    /// The defender HUD in the top-left corner.
    pub(in crate::ui) hud_left: Rect,
    /// The battle HUD in the top-right corner.
    pub(in crate::ui) hud_right: Rect,
    /// The attack selector, anchored to the arena (never to the field).
    pub(in crate::ui) selector: Rect,
    /// The last-loot receipt, anchored to the arena (never to the field).
    pub(in crate::ui) receipt_slot: Rect,
    /// The construction overlay panel.
    pub(in crate::ui) construction_panel: Rect,
    /// The READY / PAUSED banner's centre.
    pub(in crate::ui) pause_banner_center: Pos2,
    /// The control bar.
    pub(in crate::ui) bar: Rect,
    /// The key legend's left-bottom anchor.
    pub(in crate::ui) legend_anchor: Pos2,
}
impl BattleLayout {
    /// The layout of a `lanes` x `width_yards` battle in `canvas`.
    /// `hint_band` is constant for the whole attempt (a first personal
    /// deployment), never the hint toggle, so hiding hints never moves the
    /// field.
    pub(in crate::ui) fn new(canvas: Rect, lanes: u8, width_yards: f32, hint_band: bool) -> Self {
        let arena = Rect::from_min_max(
            pos2(canvas.left() + ARENA_SIDE_INSET, canvas.top() + TOP_BAND),
            pos2(
                canvas.right() - ARENA_SIDE_INSET,
                canvas.bottom() - ARENA_BOTTOM_INSET,
            ),
        );
        // The band is honoured only for a zoomed field that still zooms
        // (>= 1) under it; a field that already fills an axis keeps today's
        // rect bit for bit.
        let open = art::fit_field(arena, arena, lanes, width_yards);
        let banded = (hint_band && open.zoom > 1.0)
            .then(|| {
                let band =
                    Rect::from_min_max(arena.min, pos2(arena.right(), arena.top() + HINT_BAND));
                let below = Rect::from_min_max(band.left_bottom(), arena.max);
                (band, art::fit_field(arena, below, lanes, width_yards))
            })
            .filter(|(_, fit)| fit.zoom >= 1.0);
        let (hint_band, field) = match banded {
            Some((band, fit)) => (Some(band), fit),
            None => (None, open),
        };
        let callout_width = (arena.width() - 24.0).clamp(160.0, 620.0);
        let panel_width = PANEL_MAX_WIDTH.min(canvas.width() * 0.5 - 32.0);
        Self {
            canvas,
            arena,
            field,
            hint_band,
            hint_callout: Rect::from_min_size(
                pos2(
                    arena.center().x - callout_width * 0.5,
                    arena.top() + HINT_CALLOUT_TOP,
                ),
                vec2(callout_width, HINT_CALLOUT_HEIGHT),
            ),
            effects_clip: Rect::from_min_max(
                pos2(arena.left(), arena.top() - FX_TOP_HEADROOM),
                arena.max,
            ),
            hud_left: Rect::from_min_size(canvas.min + vec2(14., 12.), vec2(322., 108.)),
            hud_right: Rect::from_min_size(
                pos2(canvas.right() - 206., canvas.top() + 12.),
                vec2(192., 86.),
            ),
            selector: Rect::from_min_max(
                pos2(canvas.left() + 350.0, canvas.top() + 12.0),
                pos2(canvas.right() - 222.0, arena.top() - 12.0),
            ),
            receipt_slot: Rect::from_min_max(
                pos2(canvas.left() + 350.0, canvas.top() + 72.0),
                pos2(canvas.right() - 222.0, arena.top() - 8.0),
            ),
            // `right - (margin + width)`: at full width that is today's
            // `right - 296`, bit for bit.
            construction_panel: Rect::from_min_size(
                pos2(
                    canvas.right() - (PANEL_RIGHT_MARGIN + panel_width),
                    arena.top() + PANEL_TOP_GAP,
                ),
                vec2(
                    panel_width,
                    (arena.height() - 2.0 * PANEL_TOP_GAP).max(PANEL_MIN_HEIGHT),
                ),
            ),
            pause_banner_center: pos2(arena.center().x, arena.bottom() - 18.0),
            bar: Rect::from_min_max(
                pos2(canvas.left() + 12., canvas.bottom() - 76.),
                pos2(canvas.right() - 12., canvas.bottom() - 5.),
            ),
            legend_anchor: pos2(canvas.left() + 12., canvas.bottom() - 2.),
        }
    }
    /// Whether a pointer at `pointer` addresses the field: inside the field
    /// and, while construction is open, not over the construction panel.
    pub(in crate::ui) fn addresses_field(&self, pointer: Pos2, construction: bool) -> bool {
        self.field.rect.contains(pointer)
            && !(construction && self.construction_panel.contains(pointer))
    }
    /// The forest dressing's size cap: a lane of the arena's reference
    /// clearing, so zoomed fields never grow props past today's size.
    pub(in crate::ui) fn dressing_unit_cap(&self) -> f32 {
        self.arena.height() / REFERENCE_LANES
    }
}

/// The INCOMING cues while the construction `panel` is open: one per lane in
/// which a live enemy (bosses included) stands at or right of the panel's
/// left edge on the projected `field`, in lane order. Each cue is the
/// chevron's centre, `CHEVRON_GAP` left of the panel on the lane's centre
/// line, so the chevron points at what the panel hides.
pub(in crate::ui) fn occluded_spawn_cues(
    field: art::ArenaProjection,
    panel: Rect,
    enemies: &[EnemyView],
) -> Vec<(u8, Pos2)> {
    let mut lanes: Vec<u8> = enemies
        .iter()
        .filter(|e| field.world(e.position[0], 0.0).x >= panel.left())
        .map(|e| e.lane)
        .collect();
    lanes.sort_unstable();
    lanes.dedup();
    let x = panel.left() - CHEVRON_GAP;
    lanes
        .into_iter()
        .map(|lane| (lane, pos2(x, field.world(0.0, f32::from(lane) + 0.5).y)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_egui::egui::Pos2;
    use horde_core::{balance::BalanceSettings, content::Mode, sim::Game};

    /// The two target canvases: 1440x900 and 1050x700 at 1.35 points per pixel.
    fn target_canvases() -> [Rect; 2] {
        [vec2(1440.0, 900.0), vec2(1050.0 / 1.35, 700.0 / 1.35)]
            .map(|size| Rect::from_min_size(Pos2::ZERO, size))
    }

    /// The target canvases plus a wide, a tall and an offset one.
    fn canvases() -> Vec<Rect> {
        let mut all = target_canvases().to_vec();
        all.push(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0)));
        all.push(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 1000.0)));
        all.push(Rect::from_min_size(pos2(30.0, 18.0), vec2(1280.0, 720.0)));
        all
    }

    /// Every lane count and width the balance settings can produce.
    fn geometries() -> impl Iterator<Item = (u8, f32)> {
        (1..=17u8).flat_map(|lanes| (8..=30u8).map(move |width| (lanes, f32::from(width))))
    }

    /// The main battle's rects as `ui::battle` laid them out before the camera,
    /// frozen: `battle_arena`, `clearing_rect`, `attack_selector_rect`, the
    /// receipt area, the inline construction panel (over the shrunken
    /// construction arena) and the overlay positions.
    struct Legacy {
        arena: Rect,
        field: Rect,
        effects: Rect,
        hint_callout: Rect,
        hud_left: Rect,
        hud_right: Rect,
        selector: Rect,
        receipt: Rect,
        panel: Rect,
        construction_field: Rect,
        banner: Pos2,
        bar: Rect,
        legend: Pos2,
    }
    fn legacy(canvas: Rect, lanes: u8, width: f32) -> Legacy {
        fn legacy_battle_arena(canvas: Rect, construction: bool) -> Rect {
            Rect::from_min_max(
                pos2(canvas.left() + 24., canvas.top() + 124.),
                pos2(
                    canvas.right() - if construction { 276. } else { 24. },
                    canvas.bottom() - 84.,
                ),
            )
        }
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
        let maximum = legacy_battle_arena(canvas, false);
        let r = legacy_clearing_rect(maximum, lanes, width);
        let built = legacy_clearing_rect(legacy_battle_arena(canvas, true), lanes, width);
        let callout = (maximum.width() - 24.0).clamp(160.0, 620.0);
        Legacy {
            arena: maximum,
            field: r,
            effects: maximum,
            hint_callout: Rect::from_min_size(
                pos2(maximum.center().x - callout * 0.5, maximum.top() + 4.0),
                vec2(callout, 84.0),
            ),
            hud_left: Rect::from_min_size(canvas.min + vec2(14., 12.), vec2(322., 108.)),
            hud_right: Rect::from_min_size(
                pos2(canvas.right() - 206., canvas.top() + 12.),
                vec2(192., 86.),
            ),
            selector: Rect::from_min_max(
                pos2(canvas.left() + 350.0, canvas.top() + 12.0),
                pos2(canvas.right() - 222.0, r.top() - 12.0),
            ),
            receipt: Rect::from_min_max(
                pos2(canvas.left() + 350.0, canvas.top() + 72.0),
                pos2(canvas.right() - 222.0, r.top() - 8.0),
            ),
            panel: Rect::from_min_size(
                pos2(canvas.right() - 296.0, built.top() + 8.0),
                vec2(280.0, (built.height() - 16.0).max(180.0)),
            ),
            construction_field: built,
            banner: pos2(maximum.center().x, maximum.bottom() - 18.0),
            bar: Rect::from_min_max(
                pos2(canvas.left() + 12., canvas.bottom() - 76.),
                pos2(canvas.right() - 12., canvas.bottom() - 5.),
            ),
            legend: pos2(canvas.left() + 12., canvas.bottom() - 2.),
        }
    }

    /// Pin: max lanes at full width (and 11x20) lays out exactly as today, with
    /// or without a hint band, and construction's panel lands where today's
    /// did while the field no longer moves.
    #[test]
    fn battle_layout_matches_todays_geometry_at_17x30() {
        for canvas in target_canvases() {
            for (lanes, width) in [(17, 30.0), (11, 20.0)] {
                let old = legacy(canvas, lanes, width);
                for band in [false, true] {
                    let layout = BattleLayout::new(canvas, lanes, width, band);
                    let label = format!("{lanes}x{width} band {band} in {:?}", canvas.size());
                    assert_eq!(layout.canvas, canvas, "{label}");
                    assert_eq!(layout.arena, old.arena, "{label}: arena");
                    assert_eq!(layout.field.rect, old.field, "{label}: field");
                    assert_eq!(layout.field.zoom, 1.0, "{label}: zoom");
                    assert_eq!(
                        layout.hint_band, None,
                        "{label}: a filling field has no band"
                    );
                    assert_eq!(layout.hint_callout, old.hint_callout, "{label}: callout");
                    assert_eq!(layout.hud_left, old.hud_left, "{label}: left HUD");
                    assert_eq!(layout.hud_right, old.hud_right, "{label}: right HUD");
                    assert_eq!(layout.selector, old.selector, "{label}: selector");
                    assert_eq!(layout.receipt_slot, old.receipt, "{label}: receipt");
                    assert_eq!(layout.pause_banner_center, old.banner, "{label}: banner");
                    assert_eq!(layout.bar, old.bar, "{label}: bar");
                    assert_eq!(layout.legend_anchor, old.legend, "{label}: legend");
                    // The effects clip only gains headroom above the arena.
                    assert_eq!(
                        layout.effects_clip.x_range(),
                        old.effects.x_range(),
                        "{label}"
                    );
                    assert_eq!(
                        layout.effects_clip.bottom(),
                        old.effects.bottom(),
                        "{label}"
                    );
                    if lanes == 17 {
                        // Today the 17x30 construction field filled its
                        // shrunken arena; the overlay panel keeps that rect.
                        assert_eq!(layout.construction_panel, old.panel, "{label}: panel");
                    }
                }
            }
        }
    }

    /// The fix: construction no longer shrinks the arena by the panel width.
    #[test]
    fn construction_panel_overlays_instead_of_shrinking_the_field() {
        for canvas in target_canvases() {
            let old = legacy(canvas, 17, 30.0);
            let layout = BattleLayout::new(canvas, 17, 30.0, false);
            assert!(
                (old.field.width() - old.construction_field.width() - 252.0).abs() < 1e-3,
                "today's construction loses 252 pt"
            );
            assert!(layout.construction_panel.intersects(layout.field.rect));
        }
    }

    /// Pin: a field that already fills an axis never pays for the hint band;
    /// it falls back to the whole arena, and no field ever shrinks below
    /// today's clearing.
    #[test]
    fn hint_band_never_shrinks_a_filling_field() {
        let mut pinned = 0;
        for canvas in canvases() {
            for (lanes, width) in geometries() {
                let label = format!("{lanes}x{width} in {canvas:?}");
                let open = BattleLayout::new(canvas, lanes, width, false);
                let banded = BattleLayout::new(canvas, lanes, width, true);
                assert!(
                    banded.field.zoom >= 1.0,
                    "{label}: shrinks to {}",
                    banded.field.zoom
                );
                assert!(banded.arena.contains_rect(banded.field.rect), "{label}");
                if open.field.zoom == 1.0 {
                    pinned += 1;
                    assert_eq!(banded.field, open.field, "{label}: a filling field moved");
                    assert_eq!(banded.hint_band, None, "{label}");
                }
                if let Some(band) = banded.hint_band {
                    assert!(
                        banded.field.rect.top() >= band.bottom(),
                        "{label}: band over the field"
                    );
                }
            }
        }
        assert!(pinned > 800, "the pin covers only {pinned} geometries");
    }

    /// C-4: the band reserves the callout's rows over a zoomed field. At 3x12
    /// the field is width-limited, so the band costs no zoom at either size.
    #[test]
    fn first_defense_hint_band_keeps_zoom_and_clears_the_field() {
        for canvas in target_canvases() {
            let open = BattleLayout::new(canvas, 3, 12.0, false);
            let banded = BattleLayout::new(canvas, 3, 12.0, true);
            let label = format!("3x12 in {:?}", canvas.size());
            assert_eq!(
                banded.field.zoom, open.field.zoom,
                "{label}: the band costs zoom"
            );
            assert_eq!(banded.field.rect.width(), banded.arena.width(), "{label}");
            let band = banded.hint_band.expect("a zoomed field honours the band");
            assert!(
                band.contains_rect(banded.hint_callout),
                "{label}: callout outside the band"
            );
            assert!(
                banded.field.rect.top() >= banded.hint_callout.bottom(),
                "{label}: field top {} under the callout bottom {}",
                banded.field.rect.top(),
                banded.hint_callout.bottom()
            );
        }
        // Every honoured band clears the callout.
        for canvas in canvases() {
            for (lanes, width) in geometries() {
                let layout = BattleLayout::new(canvas, lanes, width, true);
                if layout.hint_band.is_some() {
                    assert!(
                        !layout.hint_callout.intersects(layout.field.rect),
                        "{lanes}x{width} in {canvas:?}: callout over the field"
                    );
                }
            }
        }
    }

    #[test]
    fn selector_and_receipt_slot_ignore_field_geometry() {
        for canvas in canvases() {
            let reference = BattleLayout::new(canvas, 17, 30.0, false);
            assert_eq!(reference.selector.bottom(), reference.arena.top() - 12.0);
            assert_eq!(reference.receipt_slot.bottom(), reference.arena.top() - 8.0);
            for (lanes, width) in geometries() {
                for band in [false, true] {
                    let layout = BattleLayout::new(canvas, lanes, width, band);
                    let label = format!("{lanes}x{width} band {band} in {canvas:?}");
                    assert_eq!(
                        layout.selector, reference.selector,
                        "{label}: selector moved"
                    );
                    assert_eq!(
                        layout.receipt_slot, reference.receipt_slot,
                        "{label}: receipt"
                    );
                    assert!(!layout.selector.intersects(layout.arena), "{label}");
                }
            }
        }
    }

    #[test]
    fn effects_clip_has_top_headroom_above_the_arena() {
        for canvas in canvases() {
            for (lanes, width) in geometries() {
                let layout = BattleLayout::new(canvas, lanes, width, false);
                let label = format!("{lanes}x{width} in {canvas:?}");
                assert_eq!(
                    layout.effects_clip,
                    Rect::from_min_max(
                        pos2(layout.arena.left(), canvas.top() + 64.0),
                        layout.arena.max
                    ),
                    "{label}"
                );
                assert!(
                    layout.effects_clip.contains_rect(layout.field.rect),
                    "{label}"
                );
                assert!(layout.effects_clip.bottom() <= layout.bar.top(), "{label}");
            }
        }
    }

    /// C-7: a lane gets one INCOMING cue when a live enemy (a boss too) stands
    /// at or right of the panel's left edge, even while it is still left of
    /// the panel's right edge; an enemy left of the panel marks nothing.
    #[test]
    fn occluded_spawn_cues_mark_lanes_hidden_by_the_panel() {
        use crate::{
            animation::SceneAnimation,
            battle_view::{BattleViewModel, SceneBlend},
        };
        use horde_core::content::EnemyKind;
        for (lanes, width) in [(3u8, 12u8), (5, 14), (7, 16)] {
            let mut game = Game::default();
            game.campaign_mut()
                .set_balance_settings(BalanceSettings {
                    start_lanes: lanes,
                    max_lanes: lanes,
                    start_width: width,
                    max_width: width,
                    ..Default::default()
                })
                .unwrap();
            game.deploy(0, Mode::Personal).unwrap();
            for canvas in target_canvases() {
                let mut a = game.attempt().unwrap().clone();
                a.enemies.clear();
                let layout = BattleLayout::new(canvas, a.lane_count, a.width_yards, false);
                let field = art::ArenaProjection::for_attempt(layout.field.rect, &a);
                let panel = layout.construction_panel;
                // Half a point either side of the panel's left edge, both
                // inside the field and left of the panel's right edge.
                let left_of = field.inverse(pos2(panel.left() - 0.5, 0.0))[0];
                let under = field.inverse(pos2(panel.left() + 0.5, 0.0))[0];
                assert!(under < a.width_yards && field.world(under, 0.0).x < panel.right());
                for (kind, lane, boss, x) in [
                    (EnemyKind::Runner, 0, false, left_of),
                    (EnemyKind::Runner, 1, false, under),
                    (EnemyKind::Brute, 2, true, under),
                    (EnemyKind::Runner, 2, false, a.width_yards - 0.2),
                ] {
                    game.spawn_enemy(&mut a, kind, lane, boss);
                    a.enemies.last_mut().unwrap().x = x;
                }
                assert_eq!(a.enemies.len(), 4);
                let view = BattleViewModel::build(
                    &a,
                    SceneAnimation::sample(&a, false),
                    SceneBlend::NONE,
                    false,
                );
                assert_eq!(view.enemies.len(), 4, "every enemy is live and in view");
                let label = format!("{lanes}x{width} in {:?}", canvas.size());
                let centre = |lane: u8| field.world(0.0, f32::from(lane) + 0.5).y;
                let x = panel.left() - CHEVRON_GAP;
                assert_eq!(
                    occluded_spawn_cues(field, panel, &view.enemies),
                    vec![(1, pos2(x, centre(1))), (2, pos2(x, centre(2)))],
                    "{label}"
                );
                for (_, cue) in occluded_spawn_cues(field, panel, &view.enemies) {
                    let chevron = Rect::from_center_size(cue, CHEVRON_SIZE);
                    assert!(chevron.right() < panel.left(), "{label}: under the panel");
                    assert!(layout.field.rect.contains_rect(chevron), "{label}");
                }
                let open: Vec<_> = view
                    .enemies
                    .iter()
                    .filter(|e| e.lane == 0)
                    .cloned()
                    .collect();
                assert_eq!(occluded_spawn_cues(field, panel, &open), vec![], "{label}");
            }
        }
    }

    /// Pin: the field is centred, so the last build column ends at the canvas
    /// centre, left of the panel at every geometry and size.
    #[test]
    fn construction_panel_never_covers_a_build_cell() {
        let mut checked = 0;
        for lanes in (3..=17u8).step_by(2) {
            for width in [8u8, 12, 16, 20, 24, 30] {
                let mut game = Game::default();
                game.campaign_mut()
                    .set_balance_settings(BalanceSettings {
                        start_lanes: lanes,
                        max_lanes: lanes,
                        start_width: width,
                        max_width: width,
                        ..Default::default()
                    })
                    .unwrap();
                game.deploy(0, Mode::Personal).unwrap();
                let a = game.attempt().unwrap();
                for canvas in canvases() {
                    for band in [false, true] {
                        let layout = BattleLayout::new(canvas, a.lane_count, a.width_yards, band);
                        let projection = art::ArenaProjection::for_attempt(layout.field.rect, a);
                        let panel = layout.construction_panel;
                        for lane in 0..a.lane_count {
                            let edge = projection.world(a.construction_bounds()[1], 0.0).x;
                            let last = a.build_cell_center(lane, 4).unwrap();
                            let half = a.construction_cell_width() * 0.5;
                            let cell = Rect::from_min_max(
                                projection.world(last[0] - half, f32::from(lane)),
                                projection.world(last[0] + half, f32::from(lane) + 1.0),
                            );
                            assert!(
                                !panel.intersects(cell) && edge < panel.left(),
                                "{lanes}x{width} band {band} in {canvas:?}: panel {panel:?} covers {cell:?}"
                            );
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert!(checked > 500);
    }
}
