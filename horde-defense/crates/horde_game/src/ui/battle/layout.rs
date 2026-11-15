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
use super::boss_banner::{BOSS_BANNER_CALLOUT_GAP, BOSS_BANNER_TOP, banner_rect};
use crate::{
    art::{self, FieldFit, camera::REFERENCE_LANES},
    battle_view::EnemyView,
};
use bevy_egui::egui::{Pos2, Rect, Response, Vec2, pos2, vec2};

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
pub(in crate::ui) const PANEL_FILL_ALPHA: u8 = 235;
/// The READY / PAUSED banner's height at full size: 18 pt type on a 5 pt
/// padded backing. A bottom letterbox this tall holds the banner under the
/// field.
pub(super) const PAUSE_BANNER_HEIGHT: f32 = 36.0;
/// The banner's full type size and backing padding.
pub(super) const PAUSE_BANNER_FONT: f32 = 18.0;
pub(super) const PAUSE_BANNER_PADDING: Vec2 = vec2(10.0, 5.0);
/// The backing padding of the compact banner, in the tag slot under the
/// battle HUD.
pub(super) const PAUSE_TAG_PADDING: Vec2 = vec2(6.0, 2.0);
/// The tag slot's gap under the battle HUD and over the arena, and its gap
/// right of the attack selector's column.
pub(super) const PAUSE_TAG_GAP: f32 = 3.0;
pub(super) const PAUSE_TAG_LEFT_GAP: f32 = 8.0;
/// How far left of the construction panel an INCOMING chevron's centre sits.
pub(super) const CHEVRON_GAP: f32 = 10.0;
/// The INCOMING chevron's width and height.
pub(super) const CHEVRON_SIZE: Vec2 = vec2(12.0, 16.0);
/// How far the INCOMING chevron's INK backing reaches past the chevron.
pub(super) const CHEVRON_BACKING: f32 = 3.0;
/// The least gap between the backings of INCOMING cues in adjacent lanes.
pub(super) const CHEVRON_CLEARANCE: f32 = 1.0;

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
    /// The READY / PAUSED banner's centre: the centre of
    /// `pause_banner_slot`.
    pub(in crate::ui) pause_banner_center: Pos2,
    /// The control bar.
    pub(in crate::ui) bar: Rect,
    /// The key legend's left-bottom anchor.
    pub(in crate::ui) legend_anchor: Pos2,
    /// The rect the READY / PAUSED banner is fitted into and centred on
    /// (CF-1-01). It never meets the field: the arena's bottom 36 pt where
    /// the field leaves that letterbox, and otherwise the strip between the
    /// battle HUD and the arena.
    pub(in crate::ui) pause_banner_slot: Rect,
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
        let hud_right = Rect::from_min_size(
            pos2(canvas.right() - 206., canvas.top() + 12.),
            vec2(192., 86.),
        );
        // The banner never lies on a lane (CF-1-01). A bottom letterbox that
        // holds it keeps today's place, bottom-centre of the arena. A field
        // that reaches the arena bottom (the height-limited 7x16, a first
        // defense under its hint band, every filling field) has no free row
        // in the arena, so the banner becomes a compact tag in the one free
        // strip beside it: under the battle HUD, over the arena, right of
        // the selector and receipt column.
        let pause_banner_slot = if arena.bottom() - field.rect.bottom() >= PAUSE_BANNER_HEIGHT {
            Rect::from_min_max(
                pos2(arena.left(), arena.bottom() - PAUSE_BANNER_HEIGHT),
                arena.max,
            )
        } else {
            Rect::from_min_max(
                pos2(
                    canvas.right() - 222.0 + PAUSE_TAG_LEFT_GAP,
                    hud_right.bottom() + PAUSE_TAG_GAP,
                ),
                pos2(hud_right.right(), arena.top() - PAUSE_TAG_GAP),
            )
        };
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
            hud_right,
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
            pause_banner_center: pause_banner_slot.center(),
            pause_banner_slot,
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
    /// The boss banner (ruling C-11), computed from the arena, the hint
    /// callout and the construction panel: centred on the arena, `min(380,
    /// 0.6 x arena width)` by 34, 4 pt under the hint callout while that
    /// shows (`callout`) and 6 pt under the arena top otherwise. While
    /// `construction` is open it ends `art::OCCLUDER_GAP` left of the panel,
    /// so the warning stays readable while building.
    pub(in crate::ui) fn boss_banner(&self, callout: bool, construction: bool) -> Rect {
        let top = if callout {
            self.hint_callout.bottom() + BOSS_BANNER_CALLOUT_GAP
        } else {
            self.arena.top() + BOSS_BANNER_TOP
        };
        banner_rect(
            self.arena,
            top,
            construction.then_some(self.construction_panel),
        )
    }
    /// The READY / PAUSED banner's type size and backing padding for a text
    /// that measures `text` at [`PAUSE_BANNER_FONT`]: full size where the
    /// slot holds it, and otherwise the largest half-point size whose padded
    /// backing stays inside the slot.
    pub(in crate::ui) fn pause_banner_fit(&self, text: Vec2) -> (f32, Vec2) {
        let slot = self.pause_banner_slot.size();
        let fits = |pad: Vec2| text.x + 2.0 * pad.x <= slot.x && text.y + 2.0 * pad.y <= slot.y;
        if fits(PAUSE_BANNER_PADDING) {
            return (PAUSE_BANNER_FONT, PAUSE_BANNER_PADDING);
        }
        let room = slot - 2.0 * PAUSE_TAG_PADDING;
        let scale = (room.x / text.x).min(room.y / text.y).min(1.0);
        // Half a point of slack covers glyph rounding at the smaller size.
        let font = ((PAUSE_BANNER_FONT * scale * 2.0).floor() * 0.5 - 0.5).max(1.0);
        (font, PAUSE_TAG_PADDING)
    }
    /// The forest dressing's size cap: a lane of the arena's reference
    /// clearing, so zoomed fields never grow props past today's size.
    pub(in crate::ui) fn dressing_unit_cap(&self) -> f32 {
        self.arena.height() / REFERENCE_LANES
    }
    /// The top-band rects the effects headroom shares with the HUD: the two
    /// corner panels, the attack selector, the loot receipt and, where the
    /// field fills the arena, the READY / PAUSED tag's slot between the
    /// battle HUD and the arena (CF-3-09; `Rect::NOTHING`, which meets
    /// nothing, while the banner sits in the bottom letterbox). Damage, MISS
    /// and loot popups keep clear of them (CF-1-09). The slot is kept clear
    /// while the battle runs too, so pausing never moves a popup.
    pub(in crate::ui) fn effects_keep_clear(&self) -> [Rect; 5] {
        let tag = self.pause_banner_slot;
        [
            self.hud_left,
            self.hud_right,
            self.selector,
            self.receipt_slot,
            if tag.bottom() <= self.arena.top() {
                tag
            } else {
                Rect::NOTHING
            },
        ]
    }
}

/// The gap between the control bar's two rows, and its buttons' vertical
/// padding: two rows of buttons end above the key legend drawn at
/// `legend_anchor` (CF-1-08), inside the unchanged `bar` rect.
pub(super) const BAR_ROW_GAP: f32 = 3.0;
pub(super) const BAR_BUTTON_PADDING_Y: f32 = 4.0;

/// Counts the rows the control bar takes in one frame (C-10). The bar adds
/// each widget's rect in layout order, so left to right and top to bottom: a
/// rect whose vertical centre lies below the current row's bottom starts the
/// next row, and anything else joins the current row.
#[derive(Clone, Copy, Debug)]
pub(in crate::ui) struct BarRows {
    rows: u8,
    bottom: f32,
    bounds: Rect,
}
impl Default for BarRows {
    fn default() -> Self {
        Self {
            rows: 0,
            bottom: f32::NEG_INFINITY,
            bounds: Rect::NOTHING,
        }
    }
}
impl BarRows {
    /// Counts the widget at `rect`.
    pub(in crate::ui) fn add(&mut self, rect: Rect) {
        self.bounds |= rect;
        if rect.center().y > self.bottom {
            self.rows = self.rows.saturating_add(1);
            self.bottom = rect.bottom();
        } else {
            self.bottom = self.bottom.max(rect.bottom());
        }
    }
    /// Counts the widget of `response` and hands the response back.
    pub(in crate::ui) fn widget(&mut self, response: Response) -> Response {
        self.add(response.rect);
        response
    }
    /// The rows counted so far.
    pub(in crate::ui) fn rows(&self) -> u8 {
        self.rows
    }
    /// The rect around every widget counted so far (`None` before the first).
    pub(in crate::ui) fn bounds(&self) -> Option<Rect> {
        (self.rows > 0).then_some(self.bounds)
    }
}

/// The least gap between a popup and a top-band rect it keeps clear of.
pub(in crate::ui) const KEEP_CLEAR_GAP: f32 = 2.0;

/// How far `popup` must move down so it meets none of `keep_clear`: each rect
/// it meets pushes its top `KEEP_CLEAR_GAP` under that rect's bottom, until
/// it meets none (0 when it meets none already). The headroom only ever
/// lies over the arena, so moving down never leaves the effects clip.
pub(in crate::ui) fn keep_clear_shift(popup: Rect, keep_clear: &[Rect]) -> f32 {
    let mut moved = popup;
    for _ in 0..=keep_clear.len() {
        let Some(bottom) = keep_clear
            .iter()
            .filter(|rect| rect.intersects(moved))
            .map(|rect| rect.bottom())
            .max_by(f32::total_cmp)
        else {
            break;
        };
        moved = moved.translate(vec2(0.0, bottom + KEEP_CLEAR_GAP - moved.top()));
    }
    moved.top() - popup.top()
}

/// The chevron of the INCOMING cue centred at `at` on a field whose lanes are
/// `lane_height` pt apart: `CHEVRON_SIZE`, scaled down uniformly where the
/// lanes are too close for it, so its backing stays a point clear of the
/// next lane's (15 or more lanes at 1050x700@1.35 are about 18.3 pt apart).
pub(in crate::ui) fn cue_chevron(at: Pos2, lane_height: f32) -> Rect {
    let fit = (lane_height - 2.0 * CHEVRON_BACKING - CHEVRON_CLEARANCE) / CHEVRON_SIZE.y;
    Rect::from_center_size(at, CHEVRON_SIZE * fit.min(1.0))
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
                    // W3-29 (CF-1-01): a filling field has no letterbox for
                    // the banner, which leaves the bottom lane for the tag
                    // slot over the arena. A letterboxed field in the same
                    // canvas keeps today's centre.
                    assert_eq!(
                        layout.pause_banner_center,
                        layout.pause_banner_slot.center(),
                        "{label}: banner"
                    );
                    assert!(
                        layout.pause_banner_slot.bottom() <= layout.arena.top(),
                        "{label}: banner over the arena"
                    );
                    assert_eq!(
                        BattleLayout::new(canvas, 3, 12.0, false).pause_banner_center,
                        old.banner,
                        "{label}: letterboxed banner"
                    );
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

    /// Whether two rects share area (egui's `intersects` also counts a shared
    /// edge).
    fn overlaps(a: Rect, b: Rect) -> bool {
        let shared = a.intersect(b);
        shared.width() > 0.0 && shared.height() > 0.0
    }

    /// CF-1-01 (W3-29): the READY / PAUSED banner never lies on a lane. On
    /// the zoomed 7x16 field and the 3x12 first defense under its hint band,
    /// at both target sizes, its slot is clear of the bottom lane; and at
    /// every geometry and canvas the slot is clear of the whole field, the
    /// corner HUDs, the selector, the receipt, the hint callout, the boss
    /// banner and the bar. A field with a 36 pt bottom letterbox keeps
    /// today's place under it.
    #[test]
    fn pause_banner_never_covers_the_bottom_lane() {
        for canvas in target_canvases() {
            for (lanes, width, band) in [(7u8, 16.0, false), (3, 12.0, true)] {
                let layout = BattleLayout::new(canvas, lanes, width, band);
                assert!(layout.field.zoom > 1.0, "{lanes}x{width}: zoomed");
                let field = art::ArenaProjection::with_geometry(layout.field.rect, lanes, width);
                let bottom_lane = Rect::from_min_max(
                    field.world(0.0, f32::from(lanes - 1)),
                    field.world(width, f32::from(lanes)),
                );
                assert!(
                    !overlaps(layout.pause_banner_slot, bottom_lane),
                    "{lanes}x{width} band {band} in {:?}: the banner slot {:?} covers the bottom lane {bottom_lane:?}",
                    canvas.size(),
                    layout.pause_banner_slot
                );
            }
        }
        let (mut letterboxed, mut tagged) = (0, 0);
        for canvas in canvases() {
            for (lanes, width) in geometries() {
                for band in [false, true] {
                    let layout = BattleLayout::new(canvas, lanes, width, band);
                    let slot = layout.pause_banner_slot;
                    let name = format!("{lanes}x{width} band {band} in {canvas:?}: slot {slot:?}");
                    assert_eq!(layout.pause_banner_center, slot.center(), "{name}");
                    assert!(canvas.contains_rect(slot), "{name}");
                    assert!(slot.width() >= 190.0 && slot.height() >= 18.0, "{name}");
                    assert!(!overlaps(slot, layout.field.rect), "{name} on the field");
                    for (what, rect) in [
                        ("left HUD", layout.hud_left),
                        ("right HUD", layout.hud_right),
                        ("selector", layout.selector),
                        ("receipt", layout.receipt_slot),
                        ("hint callout", layout.hint_callout),
                        ("boss banner", layout.boss_banner(false, false)),
                        (
                            "boss banner under the callout",
                            layout.boss_banner(true, false),
                        ),
                        ("bar", layout.bar),
                    ] {
                        assert!(!overlaps(slot, rect), "{name} on the {what} {rect:?}");
                    }
                    let letterbox = layout.arena.bottom() - layout.field.rect.bottom();
                    if letterbox >= PAUSE_BANNER_HEIGHT {
                        // Today's place: bottom-centre of the arena.
                        assert_eq!(
                            slot.center(),
                            pos2(layout.arena.center().x, layout.arena.bottom() - 18.0),
                            "{name}"
                        );
                        letterboxed += 1;
                    } else {
                        // The tag slot between the battle HUD and the arena.
                        assert!(
                            slot.top() >= layout.hud_right.bottom()
                                && slot.bottom() <= layout.arena.top(),
                            "{name}"
                        );
                        tagged += 1;
                    }
                }
            }
        }
        assert!(
            letterboxed > 100 && tagged > 100,
            "{letterboxed} / {tagged}"
        );
    }

    /// The banner fit: full size where the slot holds the text, and in the
    /// tag slot a size whose padded backing stays inside the slot.
    #[test]
    fn pause_banner_fit_keeps_the_backing_inside_the_slot() {
        for canvas in target_canvases() {
            for text in [vec2(215.0, 22.0), vec2(270.0, 22.0), vec2(520.0, 24.0)] {
                let under = BattleLayout::new(canvas, 3, 12.0, false);
                assert_eq!(
                    under.pause_banner_fit(text),
                    (PAUSE_BANNER_FONT, PAUSE_BANNER_PADDING),
                    "{text:?} under a letterboxed field"
                );
                let tag = BattleLayout::new(canvas, 7, 16.0, false);
                let (font, padding) = tag.pause_banner_fit(text);
                assert!((9.0..PAUSE_BANNER_FONT).contains(&font) || text.x > 400.0);
                assert_eq!(padding, PAUSE_TAG_PADDING);
                let drawn = text * (font / PAUSE_BANNER_FONT) + 2.0 * padding;
                let slot = tag.pause_banner_slot.size();
                assert!(
                    drawn.x <= slot.x && drawn.y <= slot.y,
                    "{text:?} at {font} pt draws {drawn:?} in {slot:?}"
                );
            }
        }
    }

    /// CF-1-02 (W3-29): the construction grid's midpoint label ends at least
    /// `OCCLUDER_GAP` left of the open construction panel at every geometry
    /// and canvas, wrapped into the room it has without breaking a word; with
    /// no panel over it, it keeps its field point on one line.
    #[test]
    fn midpoint_label_stays_left_of_the_panel_at_every_geometry() {
        use bevy_egui::egui::{Color32, FontId};
        let mut cases = Vec::new();
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
                    let layout = BattleLayout::new(canvas, a.lane_count, a.width_yards, false);
                    let field = art::ArenaProjection::for_attempt(layout.field.rect, a);
                    let build_right = a.construction_bounds()[1];
                    cases.push((
                        format!("{lanes}x{width} in {canvas:?}"),
                        field.world(build_right + 0.2, 0.2),
                        art::midpoint_label_place(field, build_right, None),
                        art::midpoint_label_place(
                            field,
                            build_right,
                            Some(layout.construction_panel),
                        ),
                        layout.construction_panel,
                    ));
                }
            }
        }
        assert!(cases.len() > 200);
        let ctx = bevy_egui::egui::Context::default();
        let mut wrapped = 0;
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let measure = |wrap: f32| {
                ui.painter().layout(
                    art::MIDPOINT_LABEL.to_owned(),
                    FontId::proportional(art::MIDPOINT_LABEL_SIZE),
                    Color32::WHITE,
                    wrap,
                )
            };
            for (name, point, open, built, panel) in &cases {
                assert_eq!(*open, (*point, f32::INFINITY), "{name}: no panel");
                let (at, wrap) = *built;
                assert!(wrap >= art::MIDPOINT_LABEL_MIN_WRAP, "{name}: wrap {wrap}");
                let galley = measure(wrap);
                let text = Rect::from_min_size(at, galley.size());
                assert!(
                    text.right() <= panel.left() - art::OCCLUDER_GAP && !text.intersects(*panel),
                    "{name}: the midpoint label {text:?} lies under the panel {panel:?}"
                );
                // Four words: more rows would break one.
                assert!(galley.rows.len() <= 4, "{name}: {} rows", galley.rows.len());
                assert_eq!(at.y, point.y, "{name}");
                if galley.rows.len() == 1 {
                    assert_eq!(at, *point, "{name}: one line keeps the field point");
                } else {
                    wrapped += 1;
                }
            }
        });
        output.textures_delta.clear();
        assert!(wrapped > 0, "some panel wraps the label");
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
    /// the panel's right edge; an enemy left of the panel marks nothing. The
    /// boss is alone in its lane, so only the boss can cue lane 2, and lane 1
    /// holds two hidden enemies for one cue.
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
                    (EnemyKind::Runner, 1, false, a.width_yards - 0.2),
                    (EnemyKind::Brute, 2, true, under),
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
                    let chevron = cue_chevron(cue, field.lane_height());
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

    /// The INCOMING cues of adjacent lanes stay apart: at every geometry and
    /// both target sizes (1050x700@1.35 with 15 or more lanes puts lanes
    /// about 18.3 pt apart), each cue's backed chevron stays apart from the
    /// next, left of the panel and between the field's edges, so neighbouring cues
    /// never merge into one zigzag. Wide lanes keep the full 12x16 chevron.
    #[test]
    fn incoming_cues_of_adjacent_lanes_never_overlap() {
        for canvas in target_canvases() {
            for (lanes, width) in geometries() {
                let layout = BattleLayout::new(canvas, lanes, width, false);
                let field = art::ArenaProjection::with_geometry(layout.field.rect, lanes, width);
                let panel = layout.construction_panel;
                let pitch = field.lane_height();
                let label = format!("{lanes}x{width} in {:?}", canvas.size());
                let backed: Vec<Rect> = (0..lanes)
                    .map(|lane| {
                        let at = pos2(
                            panel.left() - CHEVRON_GAP,
                            field.world(0.0, f32::from(lane) + 0.5).y,
                        );
                        let chevron = cue_chevron(at, pitch);
                        assert!(chevron.right() < panel.left(), "{label}");
                        // A narrow field may end left of the panel (then no
                        // spawn is ever hidden); the lanes still bound the cue.
                        assert!(
                            layout.field.rect.y_range().contains(chevron.top()),
                            "{label}"
                        );
                        assert!(
                            layout.field.rect.y_range().contains(chevron.bottom()),
                            "{label}"
                        );
                        assert!(chevron.height() >= 8.0, "{label}: still reads as a chevron");
                        if pitch >= CHEVRON_SIZE.y + 2.0 * CHEVRON_BACKING + CHEVRON_CLEARANCE {
                            assert_eq!(
                                chevron,
                                Rect::from_center_size(at, CHEVRON_SIZE),
                                "{label}"
                            );
                        }
                        chevron.expand(CHEVRON_BACKING)
                    })
                    .collect();
                for pair in backed.windows(2) {
                    assert!(
                        pair[0].bottom() < pair[1].top(),
                        "{label}: {:?} meets {:?}",
                        pair[0],
                        pair[1]
                    );
                }
            }
        }
    }

    /// W1 review: the boss banner and the REINFORCEMENTS label are the
    /// visual twins of the wave sound, so the open construction overlay
    /// never covers them: at every geometry and both target sizes the banner
    /// (W3-14: `BattleLayout::boss_banner`, with and without the hint
    /// callout) and, on every lane, the label end at least `OCCLUDER_GAP`
    /// left of the panel. With the panel closed the banner is centred on the
    /// arena and the label keeps its field point.
    #[test]
    fn construction_panel_never_covers_the_wave_warning_callouts() {
        let ctx = bevy_egui::egui::Context::default();
        let mut label = Vec2::ZERO;
        let mut output = ctx.run_ui(Default::default(), |ui| {
            label = ui
                .painter()
                .layout_no_wrap(
                    "REINFORCEMENTS".into(),
                    bevy_egui::egui::FontId::proportional(10.0),
                    bevy_egui::egui::Color32::WHITE,
                )
                .size();
        });
        output.textures_delta.clear();
        assert!(label.x > 0.0);
        for canvas in target_canvases() {
            for (lanes, width) in geometries() {
                for band in [false, true] {
                    let layout = BattleLayout::new(canvas, lanes, width, band);
                    let field =
                        art::ArenaProjection::with_geometry(layout.field.rect, lanes, width);
                    let panel = layout.construction_panel;
                    let name = format!("{lanes}x{width} band {band} in {:?}", canvas.size());
                    for callout in [false, true] {
                        let banner = layout.boss_banner(callout, true);
                        assert!(
                            banner.right() <= panel.left() - art::OCCLUDER_GAP
                                && !banner.intersects(panel),
                            "{name}: banner {banner:?} under the panel {panel:?}"
                        );
                        // It keeps its size and stays in the arena.
                        let open = layout.boss_banner(callout, false);
                        assert!(
                            (banner.size() - open.size()).length() < 1e-3,
                            "{name}: {banner:?} is not the size of {open:?}"
                        );
                        assert!(layout.arena.contains_rect(banner), "{name}: {banner:?}");
                        assert!(
                            (open.center().x - layout.arena.center().x).abs() < 1e-3,
                            "{name}: {open:?} is not centred"
                        );
                    }
                    for lane in 0..lanes {
                        let lane = f32::from(lane) + 0.5;
                        let anchor = art::reinforcements_anchor(field, width, lane, Some(panel));
                        let text =
                            Rect::from_min_size(anchor - vec2(label.x, label.y * 0.5), label);
                        assert!(
                            text.right() <= panel.left() - art::OCCLUDER_GAP
                                && !text.intersects(panel),
                            "{name}: REINFORCEMENTS {text:?} under the panel {panel:?}"
                        );
                        // The observer, and the closed panel, keep the
                        // field point.
                        assert_eq!(
                            art::reinforcements_anchor(field, width, lane, None),
                            field.world(width - 1.0, lane),
                            "{name}"
                        );
                    }
                }
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
