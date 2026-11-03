//! Camera-space battle stance. The whole body shares one authored basis;
//! planted soles and leg joints are independent of upper-body action response.
use bevy_egui::egui::{Pos2, Vec2, emath::Rot2, vec2};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct BattleStance {
    pub hips: [f32; 2],
    pub neck: [f32; 2],
    pub near_shoulder: [f32; 2],
    pub far_shoulder: [f32; 2],
    pub soles: [[f32; 2]; 2],
    pub leg_hips: [[f32; 2]; 2],
    pub thigh: f32,
    pub shin: f32,
    pub boot_height: f32,
    pub forward_lean: f32,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct BodyResponse {
    pub lean: f32,
    pub recoil: f32,
    pub melee: f32,
    pub hit: f32,
    pub brace: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct LegPose {
    pub hip: Pos2,
    pub knee: Pos2,
    pub ankle: Pos2,
    pub sole: Pos2,
}
#[derive(Clone, Copy, Debug)]
pub struct StancePose {
    pub hips: Pos2,
    pub neck: Pos2,
    pub near_shoulder: Pos2,
    pub far_shoulder: Pos2,
    pub legs: [LegPose; 2],
    pub rotation: Rot2,
    pub mirror: bool,
    pub unit: f32,
}
impl StancePose {
    pub fn sample(
        def: BattleStance,
        center: Pos2,
        height: f32,
        logical_height: f32,
        target: Pos2,
        response: BodyResponse,
    ) -> Self {
        let unit = height / logical_height;
        let ground = center + vec2(0., height * 0.5);
        let hips = ground + vec2(def.hips[0], def.hips[1]) * unit;
        let mirror = target.x < hips.x;
        let angle = (def.forward_lean + response.lean - response.recoil * 0.035
            + response.melee * 0.10
            - response.hit * 0.06)
            .clamp(-0.22, 0.25);
        let rotation = Rot2::from_angle(if mirror { -angle } else { angle });
        let upper = |point: [f32; 2]| {
            let relative = vec2(
                (point[0] - def.hips[0]) * if mirror { -1. } else { 1. },
                point[1] - def.hips[1],
            ) * unit;
            hips + rotation * relative
        };
        let legs = std::array::from_fn(|index| {
            // Camera-facing depth ordering remains stable while aiming across
            // the body. Aiming/recoil never translate the feet or flip boots.
            let sole = ground + vec2(def.soles[index][0], def.soles[index][1]) * unit;
            let ankle = sole - vec2(0., def.boot_height * unit);
            let hip = hips + vec2(def.leg_hips[index][0], def.leg_hips[index][1]) * unit;
            let delta = ankle - hip;
            let distance = delta.length();
            let axis = delta / distance.max(0.001);
            let a = def.thigh * unit;
            let b = def.shin * unit;
            let along = (a * a + distance * distance - b * b) / (2. * distance.max(0.001));
            let bend = (a * a - along * along).max(0.).sqrt();
            let forward = vec2(axis.y, -axis.x);
            let knee = hip + axis * along + forward * bend;
            LegPose {
                hip,
                knee,
                ankle,
                sole,
            }
        });
        Self {
            hips,
            neck: upper(def.neck),
            near_shoulder: upper(def.near_shoulder) + vec2(0., response.brace * unit * 0.7),
            far_shoulder: upper(def.far_shoulder) + vec2(0., response.brace * unit * 0.7),
            legs,
            rotation,
            mirror,
            unit,
        }
    }
    pub fn upper(&self, local: Vec2) -> Pos2 {
        let relative = if self.mirror {
            vec2(-local.x, local.y)
        } else {
            local
        };
        self.hips + self.rotation * (relative * self.unit)
    }
}
