//! Anatomical joints and grasp contacts are different sockets. This pure
//! armature owns both IK endpoints and the displayed hand transform.
use bevy_egui::egui::{Pos2, Vec2, emath::Rot2, vec2};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct HandSockets {
    /// Normalized source coordinates, measured on the untouched hand component.
    pub wrist: [f32; 2],
    pub palm: [f32; 2],
}
#[derive(Clone, Copy, Debug)]
pub struct HandPose {
    pub palm: Pos2,
    pub wrist: Pos2,
    pub rotation: Rot2,
    pub mirror: bool,
}
impl HandPose {
    pub fn at_palm(palm: Pos2, size: Vec2, sockets: HandSockets, angle: f32, mirror: bool) -> Self {
        let rotation = Rot2::from_angle(angle);
        let mut offset = vec2(
            (sockets.wrist[0] - sockets.palm[0]) * size.x,
            (sockets.wrist[1] - sockets.palm[1]) * size.y,
        );
        if mirror {
            offset.x = -offset.x;
        }
        Self {
            palm,
            wrist: palm + rotation * offset,
            rotation,
            mirror,
        }
    }
    pub fn source_point(&self, point: [f32; 2], size: Vec2, sockets: HandSockets) -> Pos2 {
        let mut offset = vec2(
            (point[0] - sockets.palm[0]) * size.x,
            (point[1] - sockets.palm[1]) * size.y,
        );
        if self.mirror {
            offset.x = -offset.x;
        }
        self.palm + self.rotation * offset
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ArmPose {
    pub shoulder: Pos2,
    pub elbow: Pos2,
    pub wrist: Pos2,
}
/// Fixed-length two-bone IK. Positive bend selects the lower elbow in WORLD
/// coordinates, including mirrored left-facing poses. It never treats the
/// center of a fist as the anatomical wrist or stretches to meet a grasp.
pub fn solve_arm(shoulder: Pos2, requested: Pos2, upper: f32, lower: f32, bend: f32) -> ArmPose {
    let delta = requested - shoulder;
    let distance = delta
        .length()
        .clamp((upper - lower).abs() + 0.001, upper + lower - 0.001);
    let axis = if delta.length_sq() > 0.000001 {
        delta.normalized()
    } else {
        Vec2::X
    };
    let along = (upper * upper + distance * distance - lower * lower) / (2. * distance);
    let height = (upper * upper - along * along).max(0.).sqrt();
    let middle = shoulder + axis * along;
    let perpendicular = vec2(-axis.y, axis.x) * height;
    let a = middle + perpendicular;
    let b = middle - perpendicular;
    // The source-facing tie break is deterministic at a vertical singularity;
    // the ordinary two-handed firing arc always chooses the visible low elbow.
    let down = if a.y >= b.y { a } else { b };
    let up = if a.y >= b.y { b } else { a };
    ArmPose {
        shoulder,
        elbow: if bend >= 0. { down } else { up },
        wrist: shoulder + axis * distance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_egui::egui::pos2;
    #[test]
    fn palm_and_wrist_are_distinct_sockets_under_every_hand_transform() {
        let sockets = HandSockets {
            wrist: [0.2, 0.74],
            palm: [0.62, 0.48],
        };
        for mirror in [false, true] {
            for angle in [-2.5, -1.2, 0., 1.2, 2.5] {
                let hand =
                    HandPose::at_palm(pos2(20., 30.), vec2(2.8, 1.9), sockets, angle, mirror);
                assert!(hand.wrist.distance(hand.palm) > 1.);
                assert!(
                    hand.source_point(sockets.palm, vec2(2.8, 1.9), sockets)
                        .distance(hand.palm)
                        < 0.0001
                );
                assert!(
                    hand.source_point(sockets.wrist, vec2(2.8, 1.9), sockets)
                        .distance(hand.wrist)
                        < 0.0001
                );
            }
        }
    }
    #[test]
    fn elbows_use_the_lower_world_solution_on_both_facing_sides() {
        for x in [-10_f32, -8., 8., 10.] {
            for y in [-6_f32, 0., 6.] {
                let arm = solve_arm(Pos2::ZERO, pos2(x, y), 7.8, 7.5, 1.);
                let other = solve_arm(Pos2::ZERO, pos2(x, y), 7.8, 7.5, -1.);
                assert!(arm.elbow.y >= other.elbow.y);
                assert!((arm.shoulder.distance(arm.elbow) - 7.8).abs() < 0.001);
                assert!((arm.elbow.distance(arm.wrist) - 7.5).abs() < 0.001);
            }
        }
    }
}
