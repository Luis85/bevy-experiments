use super::model::{CANVAS, Icon};
use bevy_egui::egui::{self, Color32, Rect, Stroke, pos2, vec2};
use horde_core::{campaign::Skill, content::StructureKind};

/// A small padlock at the icon's top-right corner.
pub(super) fn lock_mark(painter: &egui::Painter, corner: egui::Pos2, scale: f32, color: Color32) {
    let unit = (scale * 1.0).max(0.7);
    let body = Rect::from_min_size(corner + vec2(-14.0, 7.0) * unit, vec2(11.0, 8.0) * unit);
    let stroke = Stroke::new(1.2, color);
    painter.rect_filled(body.expand(2.0), 0.0, CANVAS);
    painter.rect_filled(body, 0.0, color);
    let left = body.left() + 2.5 * unit;
    let right = body.right() - 2.5 * unit;
    let top = body.top() - 4.0 * unit;
    painter.line_segment([pos2(left, body.top()), pos2(left, top)], stroke);
    painter.line_segment([pos2(right, body.top()), pos2(right, top)], stroke);
    painter.line_segment([pos2(left, top), pos2(right, top)], stroke);
}

pub(in crate::ui::progression) fn skill_icon(name: &str) -> Icon {
    match name {
        "reload" => Icon::Skill(Skill::QuickReload),
        "pierce" => Icon::Skill(Skill::Piercing),
        "sweep" => Icon::Skill(Skill::Sweep),
        "health" => Icon::Skill(Skill::SecondWind),
        "brace" => Icon::Skill(Skill::Brace),
        "engineering" => Icon::Skill(Skill::Engineering),
        "trap" => Icon::Defense(StructureKind::Trap, 0),
        "wall" => Icon::Defense(StructureKind::Wall, 0),
        "turret" => Icon::Defense(StructureKind::Turret, 0),
        "grenade" => Icon::Grenade,
        "flame" => Icon::Flame,
        // Body development: each attribute reuses the pictogram of what it raises.
        "vitality" => Icon::Skill(Skill::SecondWind),
        "strength" => Icon::Skill(Skill::Sweep),
        "coordination" => Icon::Skill(Skill::QuickReload),
        "endurance" => Icon::Stamina,
        "perception" => Icon::Sight,
        "companion" => Icon::Companion,
        "pistol" | "firearm" => Icon::Firearm,
        _ => Icon::Firearm,
    }
}

/// The same small, square-ended strokes form every progression pictogram.
pub(in crate::ui::progression) fn pictogram(
    p: &egui::Painter,
    rect: Rect,
    icon: Icon,
    color: Color32,
) {
    let unit = rect.width().min(rect.height()) / 24.0;
    let origin = rect.center() - vec2(12.0, 12.0) * unit;
    let point = |x: f32, y: f32| origin + vec2(x, y) * unit;
    let stroke = Stroke::new((2.0 * unit).max(1.0), color);
    let segment = |a: [f32; 2], b: [f32; 2]| {
        p.line_segment([point(a[0], a[1]), point(b[0], b[1])], stroke);
    };
    let box_outline = |x: f32, y: f32, w: f32, h: f32| {
        p.rect_stroke(
            Rect::from_min_size(point(x, y), vec2(w, h) * unit),
            0.0,
            stroke,
            egui::StrokeKind::Inside,
        );
    };
    match icon {
        Icon::Firearm => {
            box_outline(3.0, 7.0, 17.0, 6.0);
            segment([20.0, 10.0], [23.0, 10.0]);
            segment([8.0, 13.0], [6.0, 20.0]);
            segment([12.0, 13.0], [12.0, 17.0]);
        }
        Icon::Grenade => {
            box_outline(6.0, 8.0, 12.0, 13.0);
            box_outline(9.0, 3.0, 6.0, 5.0);
            segment([15.0, 4.0], [21.0, 7.0]);
            segment([10.0, 11.0], [10.0, 18.0]);
            segment([14.0, 11.0], [14.0, 18.0]);
        }
        Icon::Flame => {
            for (a, b) in [
                ([12.0, 2.0], [6.0, 12.0]),
                ([6.0, 12.0], [5.0, 17.0]),
                ([5.0, 17.0], [12.0, 22.0]),
                ([12.0, 22.0], [19.0, 17.0]),
                ([19.0, 17.0], [17.0, 10.0]),
                ([17.0, 10.0], [13.0, 14.0]),
                ([13.0, 14.0], [12.0, 2.0]),
            ] {
                segment(a, b);
            }
        }
        Icon::Stamina => {
            for (a, b) in [
                ([15.0, 2.0], [6.0, 13.0]),
                ([6.0, 13.0], [12.0, 13.0]),
                ([12.0, 13.0], [9.0, 22.0]),
                ([9.0, 22.0], [18.0, 10.0]),
                ([18.0, 10.0], [12.0, 10.0]),
                ([12.0, 10.0], [15.0, 2.0]),
            ] {
                segment(a, b);
            }
        }
        Icon::Companion => {
            p.circle_stroke(point(8.0, 7.0), 3.0 * unit, stroke);
            p.circle_stroke(point(16.0, 7.0), 3.0 * unit, stroke);
            for (a, b) in [
                ([2.0, 21.0], [3.0, 14.0]),
                ([3.0, 14.0], [13.0, 14.0]),
                ([13.0, 14.0], [14.0, 21.0]),
                ([13.0, 14.0], [21.0, 14.0]),
                ([21.0, 14.0], [22.0, 21.0]),
            ] {
                segment(a, b);
            }
        }
        Icon::Sight => {
            p.circle_stroke(point(12.0, 12.0), 7.0 * unit, stroke);
            segment([12.0, 1.0], [12.0, 8.0]);
            segment([12.0, 16.0], [12.0, 23.0]);
            segment([1.0, 12.0], [8.0, 12.0]);
            segment([16.0, 12.0], [23.0, 12.0]);
        }
        Icon::Skill(Skill::Piercing) => {
            segment([2.0, 12.0], [22.0, 12.0]);
            segment([17.0, 7.0], [22.0, 12.0]);
            segment([17.0, 17.0], [22.0, 12.0]);
            segment([7.0, 3.0], [7.0, 8.0]);
            segment([7.0, 16.0], [7.0, 21.0]);
            segment([12.0, 3.0], [12.0, 8.0]);
            segment([12.0, 16.0], [12.0, 21.0]);
        }
        Icon::Skill(Skill::QuickReload) => {
            box_outline(8.0, 4.0, 8.0, 16.0);
            segment([11.0, 8.0], [13.0, 8.0]);
            segment([11.0, 12.0], [13.0, 12.0]);
            segment([3.0, 16.0], [3.0, 5.0]);
            segment([3.0, 5.0], [6.0, 8.0]);
            segment([21.0, 8.0], [21.0, 19.0]);
            segment([21.0, 19.0], [18.0, 16.0]);
        }
        Icon::Skill(Skill::Sweep) => {
            segment([4.0, 20.0], [20.0, 4.0]);
            segment([2.0, 15.0], [8.0, 21.0]);
            segment([12.0, 3.0], [19.0, 3.0]);
            segment([21.0, 5.0], [21.0, 12.0]);
            segment([4.0, 4.0], [7.0, 7.0]);
        }
        Icon::Skill(Skill::SecondWind) => {
            box_outline(3.0, 6.0, 18.0, 15.0);
            box_outline(8.0, 2.0, 8.0, 5.0);
            segment([12.0, 9.0], [12.0, 18.0]);
            segment([8.0, 13.5], [16.0, 13.5]);
        }
        Icon::Skill(Skill::Brace) => {
            for (a, b) in [
                ([3.0, 4.0], [21.0, 4.0]),
                ([3.0, 4.0], [3.0, 15.0]),
                ([21.0, 4.0], [21.0, 15.0]),
                ([3.0, 15.0], [12.0, 22.0]),
                ([21.0, 15.0], [12.0, 22.0]),
                ([12.0, 7.0], [12.0, 17.0]),
            ] {
                segment(a, b);
            }
        }
        Icon::Skill(Skill::Engineering) => {
            segment([4.0, 20.0], [17.0, 7.0]);
            segment([14.0, 3.0], [14.0, 8.0]);
            segment([14.0, 8.0], [19.0, 10.0]);
            segment([19.0, 10.0], [22.0, 6.0]);
            segment([22.0, 6.0], [19.0, 3.0]);
            box_outline(2.0, 17.0, 5.0, 5.0);
        }
        Icon::Defense(kind, rank) => {
            match kind {
                StructureKind::Trap => {
                    segment([2.0, 18.0], [22.0, 18.0]);
                    for x in [5.0, 12.0, 19.0] {
                        segment([x - 3.0, 18.0], [x, 7.0]);
                        segment([x, 7.0], [x + 3.0, 18.0]);
                    }
                }
                StructureKind::Wall => {
                    box_outline(2.0, 5.0, 20.0, 15.0);
                    segment([2.0, 12.0], [22.0, 12.0]);
                    segment([9.0, 5.0], [9.0, 12.0]);
                    segment([15.0, 12.0], [15.0, 20.0]);
                }
                StructureKind::Turret => {
                    box_outline(7.0, 6.0, 11.0, 7.0);
                    segment([1.0, 9.0], [7.0, 9.0]);
                    segment([12.0, 13.0], [12.0, 17.0]);
                    segment([12.0, 17.0], [5.0, 22.0]);
                    segment([12.0, 17.0], [19.0, 22.0]);
                }
            }
            for index in 0..rank {
                p.rect_filled(
                    Rect::from_min_size(
                        point(4.0 + index as f32 * 6.0, 0.0),
                        vec2(4.0, 2.0) * unit,
                    ),
                    0.0,
                    color,
                );
            }
        }
    }
}
