//! The cue pass (design-vfx-lighting §2, W5-01): every warning, gauge and
//! label of the field, collected into one [`CueBatch`] and painted after the
//! lighting grade and the vignette, so no grade, light or storm flash can
//! dim or mask it.
//!
//! Every cue primitive goes through a helper here, and each helper paints an
//! INK backing first: strokes get a halo 2 pt wider, bars one expanded 1 pt,
//! markers one grown 2 pt, labels a plate padded 2 pt. Against INK the cue
//! colours hold RED 6.1:1, AMBER 9.8:1, GREEN 9.0:1 and TEXT 14.3:1, whatever
//! ground or grade lies behind. Geometry is three meshes however many cues
//! there are (halo, fill, marker); text is one galley per label.
use super::super::{AMBER, INK};
use super::batch::MeshBuilder;
use bevy_egui::egui::{self, Align2, Color32 as C, FontId, Painter, Pos2, Rect, Vec2, pos2, vec2};
use std::borrow::Cow;

/// The INK backing under cue geometry.
pub(super) const HALO: C = C::from_rgba_unmultiplied_const(17, 23, 24, 230);
/// How much wider than its stroke a line's or ring's halo is, in points.
pub(super) const HALO_STROKE: f32 = 2.0;
/// How far a bar's halo reaches past the bar on every side, in points.
pub(super) const HALO_BAR: f32 = 1.0;
/// How far a marker's halo reaches past the marker, in points.
pub(super) const HALO_MARKER: f32 = 2.0;
/// The INK plate under every cue label (CF-2-05), at the label alpha.
pub(super) const CALLOUT_BACKING: C = C::from_rgba_unmultiplied_const(17, 23, 24, 190);
/// How far the plate reaches past its label's text on every side.
pub(super) const CALLOUT_PAD: f32 = 2.0;
/// How far inside the battlefield clip a cue label stays.
pub(super) const CALLOUT_MARGIN: f32 = 2.0;

/// Segments of a cue ring.
const RING_SEGMENTS: u32 = 20;

/// Height of a cue bar on a field of lane height `sy`: bars scale with the
/// zoom between 3 and 6 pt.
pub(super) fn bar_height(sy: f32) -> f32 {
    (0.05 * sy).clamp(3.0, 6.0)
}

/// Font size of a cue label on a field of lane height `sy`: 10 to 14 pt, in
/// half-point steps so a resize warms few glyph sizes.
pub(super) fn font(sy: f32) -> f32 {
    ((0.16 * sy).clamp(10.0, 14.0) * 2.0).round() / 2.0
}

/// The halo of a cue drawn in `color`: INK, fading with the cue.
fn halo(color: C) -> C {
    HALO.gamma_multiply(f32::from(color.a()) / 255.0)
}

/// The kinds of cue primitive, for the backing guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum CueKind {
    Line,
    Ellipse,
    Bar,
    Marker,
    Label,
    Coin,
}

/// One cue primitive as collected: its colour and the backing painted under
/// it.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CueRecord {
    pub kind: CueKind,
    pub color: C,
    pub backing: C,
}

/// One label: its anchor, the alignment there, its text, size and colour.
pub(super) struct Label {
    pub at: Pos2,
    pub align: Align2,
    pub text: Cow<'static, str>,
    pub size: f32,
    pub color: C,
}

impl Label {
    /// An AMBER label, the colour of every hostile callout.
    pub(super) fn amber(
        at: Pos2,
        align: Align2,
        text: impl Into<Cow<'static, str>>,
        size: f32,
    ) -> Self {
        Self {
            at,
            align,
            text: text.into(),
            size,
            color: AMBER,
        }
    }
}

struct Coin {
    center: Pos2,
    coins: i32,
    alpha: f32,
    reduced: bool,
}

/// The cues of one battlefield frame.
pub(super) struct CueBatch {
    halo: MeshBuilder,
    fill: MeshBuilder,
    marker: MeshBuilder,
    /// Labels that move as one block, painted in order.
    labels: Vec<Vec<Label>>,
    coins: Vec<Coin>,
    #[cfg(test)]
    pub(super) records: Vec<CueRecord>,
}

impl CueBatch {
    /// A batch sized for `enemies` hostiles.
    pub(super) fn new(enemies: usize) -> Self {
        Self {
            halo: MeshBuilder::untextured(enemies * 2 + 8),
            fill: MeshBuilder::untextured(enemies * 3 + 8),
            marker: MeshBuilder::untextured(8),
            labels: Vec::new(),
            coins: Vec::new(),
            #[cfg(test)]
            records: Vec::new(),
        }
    }

    /// Keeps what was collected for the backing guard (tests only).
    #[cfg_attr(not(test), allow(unused_variables, clippy::unused_self))]
    fn record(&mut self, kind: CueKind, color: C, backing: C) {
        #[cfg(test)]
        self.records.push(CueRecord {
            kind,
            color,
            backing,
        });
    }

    /// A translucent ground wash under every cue (a contact lane's band):
    /// an area tint, not a cue primitive; its gauge is the cue.
    pub(super) fn wash(&mut self, rect: Rect, color: C) {
        self.halo.quad(rect, color);
    }

    /// A stroke from `a` to `b`.
    pub(super) fn line(&mut self, a: Pos2, b: Pos2, width: f32, color: C) {
        let direction = (b - a).normalized();
        let backing = halo(color);
        self.halo.line(
            a - direction * (HALO_STROKE * 0.5),
            b + direction * (HALO_STROKE * 0.5),
            width + HALO_STROKE,
            backing,
        );
        self.fill.line(a, b, width, color);
        self.record(CueKind::Line, color, backing);
    }

    /// An ellipse outline of `width` centred on `radii`.
    pub(super) fn ellipse(&mut self, center: Pos2, radii: Vec2, width: f32, color: C) {
        self.ellipse_of(center, radii, width, RING_SEGMENTS, color);
    }

    /// [`Self::ellipse`] with `segments` sides, for a ring many hostiles
    /// may carry at once (an elite's ground ring).
    pub(super) fn ellipse_of(
        &mut self,
        center: Pos2,
        radii: Vec2,
        width: f32,
        segments: u32,
        color: C,
    ) {
        let backing = halo(color);
        self.halo
            .ring(center, radii, width + HALO_STROKE, segments, backing);
        self.fill.ring(center, radii, width, segments, color);
        self.record(CueKind::Ellipse, color, backing);
    }

    /// A gauge: the filled `value` fraction of `rect`. The INK halo is its
    /// track, so a bar costs the two quads the pre-halo gauge did.
    pub(super) fn bar(&mut self, rect: Rect, value: f32, color: C) {
        let backing = halo(color);
        self.halo.quad(rect.expand(HALO_BAR), backing);
        let mut filled = rect;
        filled.max.x = filled.min.x + filled.width() * value.clamp(0.0, 1.0);
        self.fill.quad(filled, color);
        self.record(CueKind::Bar, color, backing);
    }

    /// A filled triangle marker.
    pub(super) fn marker(&mut self, points: [Pos2; 3], color: C) {
        let backing = halo(color);
        let centre = pos2(
            (points[0].x + points[1].x + points[2].x) / 3.0,
            (points[0].y + points[1].y + points[2].y) / 3.0,
        );
        // Grown from the centroid: twice the halo at the corners keeps at
        // least the halo along every edge of a triangle no sharper than 60°.
        let grown = points.map(|point| point + (point - centre).normalized() * (HALO_MARKER * 2.0));
        self.halo.triangle(grown, backing);
        self.marker.triangle(points, color);
        self.record(CueKind::Marker, color, backing);
    }

    /// One label on its INK plate, kept inside the clip.
    pub(super) fn label(&mut self, label: Label) {
        self.label_block(vec![label]);
    }

    /// Labels that move as one block (a holding boss's name and phase), so
    /// clamped at the clip they keep their order and never overprint.
    pub(super) fn label_block(&mut self, block: Vec<Label>) {
        for label in &block {
            let color = label.color;
            self.record(CueKind::Label, color, CALLOUT_BACKING);
        }
        self.labels.push(block);
    }

    /// A banked-coin pop-up: amber on an eight-way INK outline.
    pub(super) fn coin(&mut self, center: Pos2, coins: i32, alpha: f32, reduced: bool) {
        self.record(
            CueKind::Coin,
            AMBER.gamma_multiply(alpha),
            coin_outline(alpha),
        );
        self.coins.push(Coin {
            center,
            coins,
            alpha,
            reduced,
        });
    }

    /// Paints the batch: halo, fill and marker meshes, then labels and coin
    /// pop-ups.
    pub(super) fn paint(self, p: &Painter) {
        self.halo.paint(p);
        self.fill.paint(p);
        self.marker.paint(p);
        for block in self.labels {
            callouts(p, block);
        }
        for coin in self.coins {
            coin_popup(p, coin.center, coin.coins, coin.alpha, coin.reduced);
        }
    }
}

/// Labels that move as one block, painted in order: each laid out at its
/// anchor, then the block (their union) moved the least that keeps it
/// `CALLOUT_MARGIN` inside the painter's clip, so a boss at the field's edge
/// or in its top lane keeps a readable name (CF-1-11). A block already
/// inside paints exactly where `Painter::text` would. Each text sits on a
/// `CALLOUT_BACKING` plate padded `CALLOUT_PAD` round it (CF-2-05; the same
/// in reduced effects).
fn callouts(p: &Painter, block: Vec<Label>) {
    let laid: Vec<(Rect, _, C)> = block
        .into_iter()
        .map(|label| {
            let galley = p.layout_no_wrap(
                label.text.into_owned(),
                FontId::proportional(label.size),
                label.color,
            );
            (
                label.align.anchor_size(label.at, galley.size()),
                galley,
                label.color,
            )
        })
        .collect();
    let bounds = laid
        .iter()
        .fold(Rect::NOTHING, |bounds, (rect, ..)| bounds.union(*rect));
    let clip = p.clip_rect().shrink(CALLOUT_MARGIN);
    let min = pos2(
        bounds
            .min
            .x
            .min(clip.right() - bounds.width())
            .max(clip.left()),
        bounds
            .min
            .y
            .min(clip.bottom() - bounds.height())
            .max(clip.top()),
    );
    // Moved only when it would be cut: a block inside keeps its exact
    // position, so labels away from the edges paint where they are anchored.
    let shift = if p.clip_rect().contains_rect(bounds) {
        Vec2::ZERO
    } else {
        min - bounds.min
    };
    // Every plate first, so none covers a text of the block (CF-2-05).
    for (rect, ..) in &laid {
        p.rect_filled(
            rect.translate(shift).expand(CALLOUT_PAD),
            2.0,
            CALLOUT_BACKING,
        );
    }
    for (rect, galley, color) in laid {
        p.galley(rect.min + shift, galley, color);
    }
}

/// The outline of a coin pop-up at `alpha`: INK, fading with the number.
fn coin_outline(alpha: f32) -> C {
    C::from_rgba_unmultiplied(
        INK.r(),
        INK.g(),
        INK.b(),
        (235.0 * alpha.clamp(0.0, 1.0)) as u8,
    )
}

/// The text of a banked-coin pop-up of `coins`, laid out with `p`'s fonts.
fn coin_galley(p: &Painter, coins: i32, reduced: bool) -> std::sync::Arc<egui::Galley> {
    p.layout_no_wrap(
        format!("+{coins}c"),
        FontId::proportional(if reduced { 15.0 } else { 16.0 }),
        AMBER,
    )
}

/// How far a coin pop-up's outline copies sit off its text.
fn coin_border(reduced: bool) -> f32 {
    if reduced { 1.0 } else { 1.5 }
}

/// The rect a banked-coin pop-up of `coins` centred at `center` paints, its
/// outline included.
pub(super) fn coin_rect(p: &Painter, center: Pos2, coins: i32, reduced: bool) -> Rect {
    Rect::from_center_size(center, coin_galley(p, coins, reduced).size())
        .expand(coin_border(reduced))
}

/// Banked-coin pop-up: amber with the same eight-way dark outline as the
/// damage numbers, so it reads against any ground. Reduced effects keep it
/// still (the caller drops the drift) with a thinner outline.
fn coin_popup(p: &Painter, center: Pos2, coins: i32, alpha: f32, reduced: bool) {
    let galley = coin_galley(p, coins, reduced);
    let origin = center - galley.size() * 0.5;
    let outline = coin_outline(alpha);
    let border = coin_border(reduced);
    for direction in [
        vec2(-1.0, -1.0),
        vec2(0.0, -1.0),
        vec2(1.0, -1.0),
        vec2(-1.0, 0.0),
        vec2(1.0, 0.0),
        vec2(-1.0, 1.0),
        vec2(0.0, 1.0),
        vec2(1.0, 1.0),
    ] {
        p.galley_with_override_text_color(origin + direction * border, galley.clone(), outline);
    }
    p.galley_with_override_text_color(origin, galley, AMBER.gamma_multiply(alpha));
}
