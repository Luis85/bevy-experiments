//! UV slices articulate unchanged trouser/boot pixels around shared leg joints.
use super::stance::LegPose;
use bevy_egui::egui::{Pos2, pos2, vec2};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct LegSource {
    pub hip: [f32; 2],
    pub knee: [f32; 2],
    pub ankle: [f32; 2],
    pub sole: [f32; 2],
}
#[derive(Clone, Copy, Debug)]
pub struct LimbSlice {
    pub clip: [f32; 4],
    pub source_start: Pos2,
    pub source_end: Pos2,
    pub start: Pos2,
    pub end: Pos2,
    pub width_scale: f32,
}
impl LimbSlice {
    pub fn point(&self, source: Pos2) -> Pos2 {
        let source_delta = self.source_end - self.source_start;
        let source_axis = source_delta.normalized();
        let source_side = vec2(-source_axis.y, source_axis.x);
        let world_delta = self.end - self.start;
        let world_axis = world_delta.normalized();
        let world_side = vec2(-world_axis.y, world_axis.x);
        let relative = source - self.source_start;
        self.start
            + world_axis
                * relative.dot(source_axis)
                * (world_delta.length() / source_delta.length())
            + world_side * relative.dot(source_side) * self.width_scale
    }
    #[cfg(test)]
    pub fn corners(&self) -> [Pos2; 4] {
        [
            pos2(self.clip[0], self.clip[1]),
            pos2(self.clip[2], self.clip[1]),
            pos2(self.clip[2], self.clip[3]),
            pos2(self.clip[0], self.clip[3]),
        ]
        .map(|point| self.point(point))
    }
}
pub fn leg_slices(
    bounds: [f32; 4],
    source: LegSource,
    pose: LegPose,
    width: f32,
) -> [LimbSlice; 3] {
    let point = |p: [f32; 2]| {
        pos2(
            bounds[0] + p[0] * (bounds[2] - bounds[0]),
            bounds[1] + p[1] * (bounds[3] - bounds[1]),
        )
    };
    let ys = [
        bounds[1],
        point(source.knee).y,
        point(source.ankle).y,
        bounds[3],
    ];
    let source_points = [
        point(source.hip),
        point(source.knee),
        point(source.ankle),
        point(source.sole),
    ];
    let world_points = [pose.hip, pose.knee, pose.ankle, pose.sole];
    std::array::from_fn(|index| LimbSlice {
        clip: [bounds[0], ys[index], bounds[2], ys[index + 1]],
        source_start: source_points[index],
        source_end: source_points[index + 1],
        start: world_points[index],
        end: world_points[index + 1],
        width_scale: width / (bounds[2] - bounds[0]),
    })
}
