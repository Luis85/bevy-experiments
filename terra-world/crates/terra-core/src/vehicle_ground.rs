//! Shared, derived wheel support and reference surface policy. These queries
//! do not change terrain or own physics state. Hosts still admit swept bodies.
use crate::{FeatureKind, GeoPosition, LocalFrame, VehicleError, WorldFeature};

/// Semantic source material, never a claim about a surveyed friction value.
/// Coefficients below are explicitly authored reference-game tuning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VehicleSurface {
    #[default]
    Unknown,
    Paved,
    Gravel,
    Earth,
    Grass,
    Sand,
    Mud,
    Ice,
}
impl VehicleSurface {
    pub fn grip_scale(self) -> f64 {
        match self {
            Self::Unknown => 0.65,
            Self::Paved => 1.,
            Self::Gravel => 0.76,
            Self::Earth => 0.68,
            Self::Grass => 0.60,
            Self::Sand => 0.48,
            Self::Mud => 0.38,
            Self::Ice => 0.16,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Unclassified ground",
            Self::Paved => "Paved",
            Self::Gravel => "Gravel",
            Self::Earth => "Earth",
            Self::Grass => "Grass",
            Self::Sand => "Sand",
            Self::Mud => "Mud",
            Self::Ice => "Ice / snow",
        }
    }
    /// Interpret supported OSM surface tags. Missing road tags preserve the
    /// reference paved-road policy; tracks/paths and semantic land use differ.
    pub fn from_feature(feature: &WorldFeature) -> Self {
        if let Some(surface) = feature.properties.get("surface") {
            return match surface.trim().to_ascii_lowercase().as_str() {
                "asphalt" | "paved" | "concrete" | "concrete:plates" | "concrete:lanes"
                | "paving_stones" | "sett" | "cobblestone" | "unhewn_cobblestone" => Self::Paved,
                "gravel" | "fine_gravel" | "compacted" | "pebblestone" => Self::Gravel,
                "ground" | "dirt" | "earth" | "unpaved" | "clay" | "woodchips" => Self::Earth,
                "grass" | "grass_paver" => Self::Grass,
                "sand" => Self::Sand,
                "mud" => Self::Mud,
                "ice" | "snow" => Self::Ice,
                _ => Self::Unknown,
            };
        }
        match feature.kind {
            FeatureKind::Road => match feature.properties.get("highway").map(String::as_str) {
                Some("track" | "path" | "bridleway") => Self::Earth,
                _ => Self::Paved,
            },
            FeatureKind::Grass => Self::Grass,
            FeatureKind::Forest | FeatureKind::Farm => Self::Earth,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VehicleGroundSample {
    pub height_m: f64,
    pub surface: VehicleSurface,
}

/// Front-left, front-right, rear-left, rear-right, expressed as right/forward
/// offsets. Multi-axle reference models use an equivalent front/rear support.
pub fn vehicle_wheel_offsets(
    wheelbase_m: f64,
    track_m: f64,
) -> Result<[[f64; 2]; 4], VehicleError> {
    if !wheelbase_m.is_finite()
        || !(1.0..=20.).contains(&wheelbase_m)
        || !track_m.is_finite()
        || !(0.3..=8.).contains(&track_m)
    {
        return Err(VehicleError::Invalid);
    }
    Ok([
        [-track_m * 0.5, wheelbase_m * 0.5],
        [track_m * 0.5, wheelbase_m * 0.5],
        [-track_m * 0.5, -wheelbase_m * 0.5],
        [track_m * 0.5, -wheelbase_m * 0.5],
    ])
}

/// Terrain support derived once from the current four wheel contacts. No
/// fallback guesses a flat surface if any wheel leaves available coverage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VehicleGroundSupport {
    pub height_m: f64,
    /// Positive when the vehicle's forward axis points uphill.
    pub pitch_radians: f64,
    /// Positive when the right side is higher than the left.
    pub roll_radians: f64,
    pub wheel_heights_m: [f64; 4],
    /// Height above/below the fitted chassis plane, before visual travel limits.
    pub wheel_offsets_m: [f64; 4],
    pub surfaces: [VehicleSurface; 4],
    pub roughness_m: f64,
}
impl VehicleGroundSupport {
    /// Combined semantic material, cross-slope and rough-contact tuning. The
    /// longitudinal slope's normal-load factor is applied by step_vehicle.
    pub fn grip_scale(&self, weather_scale: f64) -> f64 {
        let material = self.surfaces.iter().map(|s| s.grip_scale()).sum::<f64>() * 0.25;
        let contact = 1. - 0.45 * (self.roughness_m / 0.30).clamp(0., 1.);
        (material * self.roll_radians.cos() * contact * weather_scale).clamp(0.05, 2.)
    }
    pub fn plane_height(&self, right_m: f64, forward_m: f64) -> f64 {
        self.height_m + right_m * self.roll_radians.tan() + forward_m * self.pitch_radians.tan()
    }
}

pub fn sample_vehicle_ground(
    position: GeoPosition,
    heading_deg: f64,
    wheelbase_m: f64,
    track_m: f64,
    mut sample: impl FnMut(GeoPosition) -> Option<VehicleGroundSample>,
) -> Result<VehicleGroundSupport, VehicleError> {
    position.validate().map_err(|_| VehicleError::Invalid)?;
    if !heading_deg.is_finite() {
        return Err(VehicleError::Invalid);
    }
    let offsets = vehicle_wheel_offsets(wheelbase_m, track_m)?;
    let frame = LocalFrame::new(position);
    let angle = heading_deg.to_radians();
    let mut heights = [0.; 4];
    let mut surfaces = [VehicleSurface::Unknown; 4];
    for (i, [right, forward]) in offsets.iter().copied().enumerate() {
        let p = frame
            .to_geo([
                forward * angle.sin() + right * angle.cos(),
                0.,
                -forward * angle.cos() + right * angle.sin(),
            ])
            .map_err(|_| VehicleError::Invalid)?;
        let value = sample(p).ok_or(VehicleError::Invalid)?;
        if !value.height_m.is_finite() || !(-12_000.0..=99_997.0).contains(&value.height_m) {
            return Err(VehicleError::Invalid);
        }
        heights[i] = value.height_m;
        surfaces[i] = value.surface;
    }
    let height = heights.iter().sum::<f64>() * 0.25;
    let pitch = ((heights[0] + heights[1] - heights[2] - heights[3]) / (2. * wheelbase_m)).atan();
    let roll = ((heights[1] + heights[3] - heights[0] - heights[2]) / (2. * track_m)).atan();
    if pitch.abs() > 1.2 || roll.abs() > 1.2 {
        return Err(VehicleError::Invalid);
    }
    let wheel_offsets_m = std::array::from_fn(|i| {
        heights[i] - (height + offsets[i][0] * roll.tan() + offsets[i][1] * pitch.tan())
    });
    Ok(VehicleGroundSupport {
        height_m: height,
        pitch_radians: pitch,
        roll_radians: roll,
        wheel_heights_m: heights,
        wheel_offsets_m,
        surfaces,
        roughness_m: wheel_offsets_m.iter().map(|v| v.abs()).fold(0., f64::max),
    })
}

/// Exact critically damped spring for bounded presentation suspension. Calling
/// it with different frame partitions gives the same constant-target result.
/// Invalid input preserves the old pair; this never modifies authority state.
pub fn suspension_spring(
    offset_m: f64,
    velocity_m_s: f64,
    target_m: f64,
    seconds: f64,
) -> (f64, f64) {
    if ![offset_m, velocity_m_s, target_m, seconds]
        .iter()
        .all(|v| v.is_finite())
        || !(0.0..=0.25).contains(&seconds)
        || seconds == 0.
    {
        return (offset_m, velocity_m_s);
    }
    let target = target_m.clamp(-0.35, 0.35);
    let omega = 14.;
    let difference = offset_m - target;
    let common = velocity_m_s + omega * difference;
    let decay = (-omega * seconds).exp();
    (
        (target + (difference + common * seconds) * decay).clamp(-0.35, 0.35),
        ((velocity_m_s - omega * common * seconds) * decay).clamp(-8., 8.),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn four_wheel_support_follows_rotated_slopes_across_date_line() {
        let p = GeoPosition::new(48., 179.999999, 100.).unwrap();
        let frame = LocalFrame::new(p);
        let support = sample_vehicle_ground(p, 90., 3.1, 1.8, |p| {
            let [east, _, south] = frame.to_local(p);
            Some(VehicleGroundSample {
                height_m: 100. + east * 0.20 + south * 0.10,
                surface: VehicleSurface::Paved,
            })
        })
        .unwrap();
        assert!((support.pitch_radians - 0.20_f64.atan()).abs() < 1e-5);
        assert!((support.roll_radians - 0.10_f64.atan()).abs() < 1e-5);
        assert!((support.height_m - 100.).abs() < 1e-5);
        assert!(support.roughness_m < 1e-5);
    }
    #[test]
    fn missing_invalid_and_rough_wheel_support_is_not_flat_pavement() {
        let p = GeoPosition::new(48., 11., 0.).unwrap();
        let mut n = 0;
        assert!(
            sample_vehicle_ground(p, 0., 3., 2., |_| {
                n += 1;
                (n != 4).then_some(VehicleGroundSample {
                    height_m: 0.,
                    surface: VehicleSurface::Paved,
                })
            })
            .is_err()
        );
        assert!(
            sample_vehicle_ground(p, 0., 3., 2., |_| Some(VehicleGroundSample {
                height_m: f64::NAN,
                surface: VehicleSurface::Paved
            }))
            .is_err()
        );
        let mut n = 0;
        let rough = sample_vehicle_ground(p, 0., 3., 2., |_| {
            let h = if n == 0 || n == 3 { 0.3 } else { 0. };
            n += 1;
            Some(VehicleGroundSample {
                height_m: h,
                surface: VehicleSurface::Gravel,
            })
        })
        .unwrap();
        assert!((rough.roughness_m - 0.15).abs() < 1e-8);
        assert!(rough.wheel_offsets_m[0] > 0. && rough.wheel_offsets_m[1] < 0.);
        assert!(rough.grip_scale(1.) < VehicleSurface::Gravel.grip_scale());
        assert!(rough.grip_scale(0.4) < rough.grip_scale(1.));
    }
    #[test]
    fn visual_spring_is_partition_stable_bounded_and_does_not_run_while_paused() {
        let one = suspension_spring(0., 0., 0.2, 0.1);
        let half = suspension_spring(0., 0., 0.2, 0.05);
        let two = suspension_spring(half.0, half.1, 0.2, 0.05);
        assert!((one.0 - two.0).abs() < 1e-10 && (one.1 - two.1).abs() < 1e-10);
        assert_eq!(suspension_spring(one.0, one.1, 0.2, 0.), one);
        assert_eq!(suspension_spring(one.0, one.1, f64::NAN, 0.05), one);
        let mut state = (0., 0.);
        for _ in 0..100 {
            state = suspension_spring(state.0, state.1, 10., 0.25);
        }
        assert!((state.0 - 0.35).abs() < 1e-6 && state.1.abs() < 1e-6);
    }
}
