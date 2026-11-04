//! A cursor readout of frozen authoritative ammunition and action timers.
use crate::{
    art::{self, AMBER, GREEN, MUTED, RED, TEXT},
    battle_view::ReticleReadout,
};
use bevy_egui::egui::{self, Color32, FontId, Rect, Stroke, pos2, vec2};
fn arc(p: &egui::Painter, at: egui::Pos2, radius: f32, progress: f32, color: Color32) {
    let count = (48.0 * progress).ceil().max(1.0) as usize;
    let points = (0..=count)
        .map(|n| {
            let angle = -std::f32::consts::FRAC_PI_2
                + std::f32::consts::TAU * progress * n as f32 / count as f32;
            at + vec2(angle.cos(), angle.sin()) * radius
        })
        .collect();
    p.add(egui::Shape::line(points, Stroke::new(2.0, color)));
}
fn badge_center(rect: Rect, at: egui::Pos2) -> egui::Pos2 {
    pos2(
        at.x.clamp(rect.left() + 26.0, rect.right() - 26.0),
        at.y.clamp(rect.top() + 26.0, rect.bottom() - 43.0),
    )
}
fn readout_origin(rect: Rect, center: egui::Pos2, text: egui::Vec2) -> egui::Pos2 {
    pos2(
        (center.x - text.x * 0.5).clamp(rect.left() + 4.0, rect.right() - text.x - 4.0),
        center.y + 27.0,
    )
}
pub(super) fn paint(
    ui: &egui::Ui,
    state: &ReticleReadout,
    arena: art::ArenaProjection,
    aim: [f32; 2],
    building: bool,
    reduced: bool,
) {
    let at = arena.world(aim[0], aim[1]);
    let p = ui.painter_at(arena.effects_clip);
    let color = if state.remaining == 0 {
        RED
    } else if state.observed {
        TEXT
    } else {
        AMBER
    };
    for d in [
        vec2(1.0, 0.0),
        vec2(-1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(0.0, -1.0),
    ] {
        p.line_segment(
            [at + d * 4.0, at + d * 10.0],
            Stroke::new(3.0, Color32::BLACK),
        );
        p.line_segment([at + d * 4.0, at + d * 10.0], Stroke::new(1.3, color));
    }
    if building {
        return;
    }
    // Keep resource readouts clear of arena edges without moving the actual
    // aim crosshair. The small edge badge remains anchored to that crosshair.
    let center = badge_center(arena.effects_clip, at);
    let segments = state.capacity.min(if reduced { 12 } else { 24 }).max(1);
    for index in 0..segments {
        let angle =
            -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * index as f32 / segments as f32;
        let d = vec2(angle.cos(), angle.sin());
        let filled = u64::from(index) * u64::from(state.capacity)
            < u64::from(state.remaining) * u64::from(segments);
        p.line_segment(
            [center + d * 16.0, center + d * 19.0],
            Stroke::new(3.5, Color32::BLACK),
        );
        p.line_segment(
            [center + d * 16.0, center + d * 19.0],
            Stroke::new(
                1.8,
                if filled {
                    color
                } else {
                    MUTED.gamma_multiply(0.4)
                },
            ),
        );
    }
    if let Some(progress) = state.progress {
        arc(&p, center, 23.0, progress, GREEN);
    }
    let galley = p.layout_no_wrap(state.label.clone(), FontId::proportional(11.0), color);
    let pos = readout_origin(arena.effects_clip, center, galley.size());
    p.rect_filled(
        Rect::from_min_size(pos - vec2(3.0, 1.0), galley.size() + vec2(6.0, 2.0)),
        0.0,
        Color32::from_black_alpha(200),
    );
    p.galley(pos, galley, color);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resources_stay_readable_at_every_clearing_edge_without_moving_true_aim() {
        for size in [vec2(160.0, 86.0), vec2(292.0, 86.0), vec2(729.0, 310.0)] {
            let rect = Rect::from_min_size(pos2(40.0, 160.0), size);
            for aim in [
                rect.left_top(),
                rect.right_top(),
                rect.left_bottom(),
                rect.right_bottom(),
                rect.center(),
            ] {
                let original = aim;
                let badge = badge_center(rect, aim);
                assert!(rect.contains_rect(Rect::from_center_size(badge, vec2(48.0, 48.0))));
                let text = vec2(72.0, 13.0);
                assert!(rect.contains_rect(
                    Rect::from_min_size(readout_origin(rect, badge, text), text).expand(2.0)
                ));
                assert_eq!(aim, original);
            }
        }
    }
}
