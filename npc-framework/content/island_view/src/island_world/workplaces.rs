//! Workplace models fitted to the bodies the simulation blocks.
//!
//! The KayKit buildings are tabletop miniatures. At the scale they were
//! placed with (2.4) a smithy stood 2.4 m high with a 0.75 m door on a body
//! the simulation blocks 5 m square. Each solid workplace is now scaled
//! across the ground until the box of its ground-level walls fills that
//! body along its longer side, lifted upwards by at most 1.25 times that to
//! bring its door towards 0.85 of a resident, capped at 7 m (the mill at
//! 9 m), and turned so its door or working side faces the entrance. The body stays the
//! simulation's: what the model leaves of it is the yard its own barrows,
//! logs and crates stand in. Open workplaces (the well, the fishing hut)
//! keep their scale: a well's wall is waist high at it.
use bevy::prelude::*;
use island_content::village::Role;

use crate::island_homes::model::{BodyFrame, HouseModel};

/// The scale every workplace was placed with, and open ones keep.
pub(crate) const OPEN_SCALE: f32 = 2.4;
/// No workshop but the mill stands taller than this: the two-storey inn is
/// 7.5 m to its chimney's mouth, a home 6.5 m.
pub(crate) const WORKSHOP_HEIGHT_M: f32 = 7.0;
/// The mill may stand this tall to the tip of its sails.
pub(crate) const WINDMILL_HEIGHT_M: f32 = 9.0;
/// A door this high (0.85 of a 1.8 m resident) reads as one a resident
/// walks through: a model is lifted towards it.
pub(crate) const DOOR_LEAST_M: f32 = 0.85 * 1.8;
/// How much taller than wide a model may be stretched to bring its door
/// towards [`DOOR_LEAST_M`].
pub(crate) const MAX_LIFT: f32 = 1.25;

/// The workplace meshes that are fitted, measured in their own units (the
/// kit's grid is 0.07). A model's door or working side is on its +Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkModel {
    Smithy,
    Windmill,
    Sawmill,
    Mine,
    Tavern,
    Market,
    /// The two-storey house, fitted like the inn.
    Lodge,
}

impl WorkModel {
    #[cfg(test)]
    pub(crate) const ALL: [WorkModel; 7] = [
        WorkModel::Smithy,
        WorkModel::Windmill,
        WorkModel::Sawmill,
        WorkModel::Mine,
        WorkModel::Tavern,
        WorkModel::Market,
        WorkModel::Lodge,
    ];

    /// The model a solid workplace of this trade wears, as
    /// `living_scene::building` chooses it; `None` for the open ones.
    pub(crate) fn of(role: Role) -> Option<Self> {
        Some(match role {
            Role::Smith => Self::Smithy,
            Role::Miller => Self::Windmill,
            Role::Woodcutter | Role::Carpenter => Self::Sawmill,
            Role::Miner => Self::Mine,
            Role::Innkeeper | Role::Baker => Self::Tavern,
            Role::Farmer | Role::Tailor | Role::Tanner => Self::Market,
            // A data-only trade works in the generic workshop.
            Role::Trade(_) => Self::Market,
            Role::WaterKeeper | Role::Fisher => return None,
            _ => Self::Lodge,
        })
    }

    /// The box of the ground-level walls: x range, z range.
    fn walls(self) -> ((f32, f32), (f32, f32)) {
        match self {
            // The shed and the forge; the anvil's yard lies in front.
            Self::Smithy => ((-0.63, 0.66), (-0.56, 0.44)),
            // The tower's stone base; barrows and crates stand around it.
            Self::Windmill => ((-0.39, 0.39), (-0.39, 0.39)),
            Self::Sawmill => ((-0.45, 0.60), (-0.46, 0.57)),
            // The whole outcrop.
            Self::Mine => ((-0.82, 0.79), (-1.02, 0.90)),
            // The stone base under the house and its barrel.
            Self::Tavern => ((-0.50, 0.50), (-0.55, 0.55)),
            Self::Market => ((-0.90, 0.90), (-0.715, 0.60)),
            Self::Lodge => ((-0.35, 0.35), (-0.35, 0.245)),
        }
    }

    /// How high the model stands: the smithy to its chimney's mouth, the
    /// mill to the top of its sails, the timber yard to its hoist.
    pub(crate) fn height_units(self) -> f32 {
        match self {
            Self::Smithy => 0.992,
            Self::Windmill => 1.47,
            Self::Sawmill => 1.70,
            Self::Mine => 1.125,
            Self::Tavern => 1.40,
            Self::Market => 0.98,
            Self::Lodge => 1.28,
        }
    }

    /// Height of the door, or of the opening somebody works at: the mine's
    /// adit, the market's doors.
    pub(crate) fn door_units(self) -> f32 {
        match self {
            Self::Mine => 0.42,
            Self::Market => 0.36,
            _ => 0.315,
        }
    }

    /// Scale and place the model on a body, in the body's own frame.
    pub(crate) fn fit(self, frame: &BodyFrame) -> WorkFit {
        if self == Self::Lodge {
            let fit = HouseModel::Hall.fit(frame);
            return WorkFit {
                scale: fit.scale,
                translation: fit.translation,
            };
        }
        let ((x0, x1), (z0, z1)) = self.walls();
        // Across the ground the wall box fills the body along its longer
        // side, unless the model would then stand taller than the cap: then
        // it shrinks evenly, never squashed.
        let tallest = self.tallest_m() / self.height_units();
        let level = (frame.width / (x1 - x0))
            .min(frame.depth / (z1 - z0))
            .min(tallest)
            * self.across_trim();
        // Upwards it may be stretched by at most `MAX_LIFT`, only as far as
        // a door 0.85 of a resident, and never past the cap. A door that
        // still falls short is left short (the sawmill, the market, the
        // mine: `doors_are_measured_against_a_resident`).
        let lift = (DOOR_LEAST_M / self.door_units())
            .clamp(level, level * MAX_LIFT)
            .min(tallest.max(level));
        WorkFit {
            scale: Vec3::new(level, lift, level),
            // Turned half round, like a home: the model's +Z faces the
            // entrance, which is the body's -Z.
            translation: Vec3::new((x0 + x1) * 0.5 * level, 0.0, (z0 + z1) * 0.5 * level),
        }
    }

    /// The tallest the model may stand: the mill's sails may reach higher
    /// than any workshop.
    fn tallest_m(self) -> f32 {
        if self == Self::Windmill {
            WINDMILL_HEIGHT_M
        } else {
            WORKSHOP_HEIGHT_M
        }
    }

    /// Per-model trims judged from the front captures beside a resident
    /// (`target/npc-out/world2/scale-*`): 1.0 keeps the rule's fit.
    fn across_trim(self) -> f32 {
        1.0
    }
}

/// A workplace model fitted to one body, in the body's own frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WorkFit {
    pub(crate) scale: Vec3,
    pub(crate) translation: Vec3,
}

impl WorkFit {
    /// A point of the model, in the body's frame.
    #[cfg(test)]
    pub(crate) fn point(&self, model: Vec3) -> Vec3 {
        self.translation + Vec3::new(-model.x, model.y, -model.z) * self.scale
    }

    /// The model's transform under a visual root, wherever the root stands
    /// and however it is turned.
    pub(crate) fn under(&self, root: &Transform, frame: &BodyFrame) -> Transform {
        let turn = root.rotation.inverse();
        let origin = turn
            * (Vec3::new(frame.centre.x, root.translation.y, frame.centre.y) - root.translation);
        let facing = turn * frame.rotation();
        Transform::from_translation(origin + facing * self.translation)
            .with_rotation(facing * Quat::from_rotation_y(std::f32::consts::PI))
            .with_scale(self.scale)
    }
}

/// On a workplace's visual root: where its fitted model stands under it.
/// The forge's chimney and fire are found through it.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct WorkplaceFit(pub(crate) Transform);

/// The box a fitted workplace fills, for taking it out of the way of the
/// camera: the body, as high as the model stands on it.
pub(crate) fn occluder(
    role: Role,
    frame: BodyFrame,
    placed: &Transform,
    drawn: Entity,
) -> crate::island_homes::Occluder {
    let height = match WorkModel::of(role) {
        Some(WorkModel::Lodge) => HouseModel::Hall.fit(&frame).chimney().y,
        Some(model) => model.height_units() * placed.scale.y,
        None => 3.0,
    };
    crate::island_homes::Occluder {
        frame,
        height,
        drawn,
    }
}

/// The transform of a workplace's model under its root: fitted to `frame`
/// for a solid workplace, the old scale and offset for an open one.
pub(crate) fn model_transform(
    role: Role,
    solid: bool,
    root: &Transform,
    frame: &BodyFrame,
    offset_z: f32,
) -> (Transform, bool) {
    match WorkModel::of(role).filter(|_| solid) {
        Some(model) => (model.fit(frame).under(root, frame), true),
        None => (
            Transform::from_xyz(0.0, 0.0, offset_z).with_scale(Vec3::splat(OPEN_SCALE)),
            false,
        ),
    }
}

/// The mouth of the smithy's forge chimney and the middle of its fire, in
/// the model's own units.
pub(crate) const FORGE_CHIMNEY_UNITS: Vec3 = Vec3::new(0.208, 0.992, -0.142);
pub(crate) const FORGE_FIRE_UNITS: Vec3 = Vec3::new(0.21, 0.30, 0.36);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::island_homes::model::{DOOR_HEIGHT_M, RESIDENT_HEIGHT_M};
    use npc_framework::prelude::SimPos;

    fn body() -> BodyFrame {
        BodyFrame::of_founding_workplace(SimPos::new(10_000, -22_000))
    }

    #[test]
    fn every_trade_with_a_solid_workplace_has_a_fitted_model() {
        for role in [
            Role::Smith,
            Role::Miller,
            Role::Woodcutter,
            Role::Carpenter,
            Role::Miner,
            Role::Baker,
            Role::Farmer,
            Role::Tailor,
            Role::Tanner,
            Role::Hunter,
        ] {
            assert!(WorkModel::of(role).is_some(), "{role:?}");
        }
        // The open ones keep their scale and their place.
        for role in [Role::WaterKeeper, Role::Fisher] {
            assert_eq!(WorkModel::of(role), None);
            let (placed, fitted) = model_transform(role, false, &Transform::IDENTITY, &body(), 3.5);
            assert!(!fitted);
            assert_eq!(placed.scale, Vec3::splat(OPEN_SCALE));
            assert_eq!(placed.translation, Vec3::new(0.0, 0.0, 3.5));
        }
        // A well's wall is waist high at that scale.
        assert!((0.384 * OPEN_SCALE - 0.92).abs() < 0.01);
    }

    #[test]
    fn a_fitted_workplace_stands_on_its_body_and_faces_the_entrance() {
        let frame = body();
        let (half_w, half_d) = (frame.width * 0.5, frame.depth * 0.5);
        assert_eq!((frame.width, frame.depth), (5.0, 5.0));
        for model in WorkModel::ALL {
            let fit = model.fit(&frame);
            let ((x0, x1), (z0, z1)) = model.walls();
            // The wall box lies on the body; a model nothing holds back
            // touches it on its longer side.
            let corners = [
                fit.point(Vec3::new(x0, 0.0, z0)),
                fit.point(Vec3::new(x1, 0.0, z1)),
            ];
            for corner in corners {
                assert!(
                    corner.x.abs() <= half_w + 1e-3 && corner.z.abs() <= half_d + 1e-3,
                    "{model:?}: {corner}"
                );
            }
            let reach = (corners[0] - corners[1]).abs();
            let held_back = matches!(model, WorkModel::Windmill | WorkModel::Sawmill);
            assert_eq!(
                (reach.x - frame.width).abs() < 1e-3 || (reach.z - frame.depth).abs() < 1e-3,
                !held_back,
                "{model:?}: {reach}"
            );
            // Even those cover most of it.
            assert!(reach.x > 4.0 || reach.z > 4.0, "{model:?}: {reach}");
            // Its front, the model's +Z, is on the entrance side.
            assert!(fit.point(Vec3::new(0.0, 0.0, z1)).z < fit.point(Vec3::new(0.0, 0.0, z0)).z);
            // Never squashed; lifted by at most `MAX_LIFT`.
            if model != WorkModel::Lodge {
                assert_eq!(fit.scale.x, fit.scale.z, "{model:?}");
                assert!(
                    fit.scale.y >= fit.scale.x && fit.scale.y <= fit.scale.x * MAX_LIFT + 1e-4,
                    "{model:?}: {}",
                    fit.scale
                );
            }
            // No workshop towers over the inn; the mill may. The lodge is
            // the inn's own house.
            let inn = HouseModel::Hall.fit(&frame).chimney().y;
            assert!((7.4..7.6).contains(&inn), "{inn}");
            let tall = model.height_units() * fit.scale.y;
            if model == WorkModel::Windmill {
                assert!((8.5..=WINDMILL_HEIGHT_M + 1e-3).contains(&tall), "{tall}");
            } else if model != WorkModel::Lodge {
                assert!(
                    tall <= WORKSHOP_HEIGHT_M + 1e-3 && tall < inn,
                    "{model:?}: {tall}"
                );
            }
            // Bigger than the old placement.
            assert!(fit.scale.x > OPEN_SCALE, "{model:?}");
        }
    }

    #[test]
    fn doors_are_measured_against_a_resident() {
        // Door, or opening worked at, over a 1.8 m resident. The models are
        // miniatures of buildings wider than the 5 m body: lifted by at
        // most 1.25 and capped at 7 m, the bakery and the smithy reach
        // 0.85; the timber yard is held by its height, the mine and the
        // market by the lift, and stay short.
        let frame = body();
        let ratio =
            |model: WorkModel| model.door_units() * model.fit(&frame).scale.y / RESIDENT_HEIGHT_M;
        let old = |model: WorkModel| model.door_units() * OPEN_SCALE / RESIDENT_HEIGHT_M;
        for (model, least, most) in [
            (WorkModel::Windmill, 1.05, 1.09),
            (WorkModel::Lodge, 1.1, 1.3),
            (WorkModel::Tavern, 0.84, 0.86),
            (WorkModel::Sawmill, 0.7, 0.74),
            (WorkModel::Smithy, 0.84, 0.86),
            (WorkModel::Mine, 0.74, 0.78),
            (WorkModel::Market, 0.68, 0.71),
        ] {
            let now = ratio(model);
            assert!((least..=most).contains(&now), "{model:?}: {now}");
            assert!(
                now > old(model) * 1.05,
                "{model:?}: {now} against {}",
                old(model)
            );
            assert!(old(model) < 0.6);
        }
        assert!((ratio(WorkModel::Lodge) - DOOR_HEIGHT_M / RESIDENT_HEIGHT_M).abs() < 1e-4);
        // The smith's anvil is knee to hip high.
        let anvil = 0.17 * WorkModel::Smithy.fit(&frame).scale.y;
        assert!((0.6..=0.9).contains(&anvil), "{anvil}");
    }

    #[test]
    fn a_fitted_model_follows_its_root_and_the_forge_is_found_through_it() {
        let frame = body();
        // A founding root: at the site, not turned.
        let root = Transform::from_xyz(10.0, 1.5, -22.0);
        let (placed, fitted) = model_transform(Role::Smith, true, &root, &frame, 3.5);
        assert!(fitted);
        let fit = WorkModel::Smithy.fit(&frame);
        // The same point through the root and through the frame.
        for model in [
            FORGE_CHIMNEY_UNITS,
            FORGE_FIRE_UNITS,
            Vec3::new(-0.63, 0.0, 0.44),
        ] {
            let through_root = root.transform_point(placed.transform_point(model));
            let through_frame = frame.to_world(fit.point(model), 1.5);
            assert!(
                (through_root - through_frame).length() < 1e-3,
                "{through_root} against {through_frame}"
            );
        }
        // A built root is turned and stands elsewhere: the model still
        // lands on the body.
        let turned = Transform::from_xyz(3.0, 1.5, -30.0).with_rotation(Quat::from_rotation_y(1.2));
        let (placed, _) = model_transform(Role::Smith, true, &turned, &frame, 3.5);
        let through_root = turned.transform_point(placed.transform_point(FORGE_CHIMNEY_UNITS));
        let through_frame = frame.to_world(fit.point(FORGE_CHIMNEY_UNITS), 1.5);
        assert!((through_root - through_frame).length() < 1e-3);
        // The chimney's mouth is above the fire and high over a resident.
        let chimney = fit.point(FORGE_CHIMNEY_UNITS);
        assert!(chimney.y > 3.5 && chimney.y > fit.point(FORGE_FIRE_UNITS).y + 2.0);
    }
}
