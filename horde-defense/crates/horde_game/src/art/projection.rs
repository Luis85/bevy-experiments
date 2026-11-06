use bevy_egui::egui::{Pos2, Rect, Vec2, pos2, vec2};
use horde_core::sim::Attempt;

/// One projection contract for drawing, cursor inversion, cells and area radii.
/// The vertical world extent is the frozen attempt lane count, never UI state.
#[derive(Clone, Copy, Debug)]
pub struct ArenaProjection {
    pub rect: Rect,
    pub lanes: u8,
    pub width: f32,
    /// Presentation-only forest space; world mapping and input always use rect.
    pub effects_clip: Rect,
}
impl ArenaProjection {
    #[cfg(test)]
    pub fn new(rect: Rect, lanes: u8) -> Self {
        Self::with_geometry(rect, lanes, 30.0)
    }
    pub fn with_geometry(rect: Rect, lanes: u8, width: f32) -> Self {
        assert!(lanes > 0 && width.is_finite() && width > 0.0);
        Self {
            rect,
            lanes,
            width,
            effects_clip: rect,
        }
    }
    pub fn with_effects_clip(mut self, clip: Rect) -> Self {
        assert!(clip.contains_rect(self.rect));
        self.effects_clip = clip;
        self
    }
    pub fn for_attempt(rect: Rect, attempt: &Attempt) -> Self {
        Self::with_geometry(rect, attempt.lane_count, attempt.width_yards)
    }
    pub fn yard_width(self) -> f32 {
        self.rect.width() / self.width
    }
    pub fn world(self, x: f32, y: f32) -> Pos2 {
        pos2(
            self.rect.left() + x / self.width * self.rect.width(),
            self.rect.top() + y / f32::from(self.lanes) * self.rect.height(),
        )
    }
    pub fn inverse(self, p: Pos2) -> [f32; 2] {
        [
            (p.x - self.rect.left()) / self.rect.width() * self.width,
            (p.y - self.rect.top()) / self.rect.height() * f32::from(self.lanes),
        ]
    }
    pub fn lane_height(self) -> f32 {
        self.rect.height() / f32::from(self.lanes)
    }
    pub fn radius(self, radius: f32) -> Vec2 {
        vec2(
            radius / self.width * self.rect.width(),
            radius * self.lane_height(),
        )
    }
}
#[cfg(test)]
pub fn world(r: Rect, x: f32, y: f32) -> Pos2 {
    ArenaProjection::new(r, horde_core::sim::LANES).world(x, y)
}
#[cfg(test)]
pub fn inverse(r: Rect, p: Pos2) -> [f32; 2] {
    ArenaProjection::new(r, horde_core::sim::LANES).inverse(p)
}
