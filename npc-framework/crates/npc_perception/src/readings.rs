//! What a robot's sensors report ([`SensorReadings`], plan Task 12, spec
//! G10) and the integer angle helpers they share.
//!
//! **Angles** are millidegrees (mdeg). A world direction's angle is measured
//! from +x (east) towards +z (north), counter-clockwise on the map
//! ([`atan2_mdeg`], [`dir_ppm`]). Sensor angles (a LIDAR beam, a camera or
//! radar bearing) are relative to the agent's [`Facing`]: 0 = straight
//! ahead, **positive = to the left** (counter-clockwise, as in ROS), in
//! `(-180000, 180000]` ([`bearing_mdeg`]). No trigonometry at runtime: sines
//! and tangents come from offline tables ([`SIN_PPM`], [`TAN_PPM`]) with
//! linear interpolation between whole degrees.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use npc_kernel::SimId;
use serde::{Deserialize, Serialize};

use crate::camera::Detection;
use crate::lidar::LaserScan;
use crate::radar::{RadarEcho, RadarTrack};
use crate::sensors::Facing;

/// Snapshot name of [`SensorReadings`] (ADR 0013; it is not in the state
/// hash).
pub const SENSOR_READINGS: &str = "sensor_readings";

/// Parts per million (the scale of [`SIN_PPM`], [`TAN_PPM`], [`dir_ppm`]).
pub const PPM: i64 = 1_000_000;

/// `round(sin(d°) · 10⁶)` for `d = 0..=90`, generated offline.
pub const SIN_PPM: [i32; 91] = [
    0, 17452, 34899, 52336, 69756, 87156, 104528, 121869, 139173, 156434, 173648, 190809, 207912,
    224951, 241922, 258819, 275637, 292372, 309017, 325568, 342020, 358368, 374607, 390731, 406737,
    422618, 438371, 453990, 469472, 484810, 500000, 515038, 529919, 544639, 559193, 573576, 587785,
    601815, 615661, 629320, 642788, 656059, 669131, 681998, 694658, 707107, 719340, 731354, 743145,
    754710, 766044, 777146, 788011, 798636, 809017, 819152, 829038, 838671, 848048, 857167, 866025,
    874620, 882948, 891007, 898794, 906308, 913545, 920505, 927184, 933580, 939693, 945519, 951057,
    956305, 961262, 965926, 970296, 974370, 978148, 981627, 984808, 987688, 990268, 992546, 994522,
    996195, 997564, 998630, 999391, 999848, 1000000,
];

/// `round(tan(d°) · 10⁶)` for `d = 0..=45`, generated offline.
pub const TAN_PPM: [i32; 46] = [
    0, 17455, 34921, 52408, 69927, 87489, 105104, 122785, 140541, 158384, 176327, 194380, 212557,
    230868, 249328, 267949, 286745, 305731, 324920, 344328, 363970, 383864, 404026, 424475, 445229,
    466308, 487733, 509525, 531709, 554309, 577350, 600861, 624869, 649408, 674509, 700208, 726543,
    753554, 781286, 809784, 839100, 869287, 900404, 932515, 965689, 1000000,
];

/// sin of `x` in `0..=90000` mdeg, ppm (interpolated).
fn sin_quadrant(x: i64) -> i64 {
    let d = (x / 1000).clamp(0, 90) as usize;
    if d >= 90 {
        return i64::from(SIN_PPM[90]);
    }
    let f = x - d as i64 * 1000;
    let (lo, hi) = (i64::from(SIN_PPM[d]), i64::from(SIN_PPM[d + 1]));
    lo + (hi - lo) * f / 1000
}

/// sin of an angle in mdeg (any value), ppm.
pub fn sin_ppm(mdeg: i64) -> i64 {
    let a = mdeg.rem_euclid(360_000);
    let r = a % 90_000;
    match a / 90_000 {
        0 => sin_quadrant(r),
        1 => sin_quadrant(90_000 - r),
        2 => -sin_quadrant(r),
        _ => -sin_quadrant(90_000 - r),
    }
}

/// cos of an angle in mdeg (any value), ppm.
pub fn cos_ppm(mdeg: i64) -> i64 {
    sin_ppm(mdeg + 90_000)
}

/// The unit direction `(x, z)` in ppm of world angle `mdeg` (from +x
/// towards +z).
pub fn dir_ppm(mdeg: i64) -> (i64, i64) {
    (cos_ppm(mdeg), sin_ppm(mdeg))
}

/// `atan(n / d)` in mdeg for `0 ≤ n ≤ d`, `d > 0`: `0..=45000`.
fn atan_octant(n: i128, d: i128) -> i64 {
    let t = (n * i128::from(PPM) / d) as i64;
    let i = TAN_PPM.partition_point(|&v| i64::from(v) <= t);
    // `TAN_PPM[i - 1] <= t < TAN_PPM[i]`.
    if i >= TAN_PPM.len() {
        return 45_000;
    }
    let lo = i64::from(TAN_PPM[i - 1]);
    let hi = i64::from(TAN_PPM[i]);
    (i as i64 - 1) * 1000 + (t - lo) * 1000 / (hi - lo)
}

/// The angle of vector `(x, y)` in mdeg, `(-180000, 180000]`, measured from
/// +x towards +y (0 for the zero vector). Integer only.
pub fn atan2_mdeg(y: i64, x: i64) -> i32 {
    if x == 0 && y == 0 {
        return 0;
    }
    let (ax, ay) = (i128::from(x).abs(), i128::from(y).abs());
    let a0 = if ay <= ax {
        atan_octant(ay, ax)
    } else {
        90_000 - atan_octant(ax, ay)
    };
    let a = match (x >= 0, y >= 0) {
        (true, true) => a0,
        (false, true) => 180_000 - a0,
        (false, false) => -(180_000 - a0),
        (true, false) => -a0,
    };
    a as i32
}

/// The world angle (mdeg) of a facing (90000 = north for the default +z).
pub fn facing_angle_mdeg(facing: Facing) -> i32 {
    let (x, z) = facing.reduced();
    if (x, z) == (0, 0) {
        return 90_000;
    }
    atan2_mdeg(i64::from(z), i64::from(x))
}

/// The bearing (mdeg, + = left) of direction `d` (target − observer) seen
/// from an observer facing `facing`: 0 straight ahead, `(-180000, 180000]`.
pub fn bearing_mdeg(facing: Facing, d: (i64, i64)) -> i32 {
    let (fx, fz) = facing.reduced();
    let (fx, fz) = if (fx, fz) == (0, 0) {
        (0, 1)
    } else {
        (i64::from(fx), i64::from(fz))
    };
    let ahead = fx * d.0 + fz * d.1;
    let left = fx * d.1 - fz * d.0;
    atan2_mdeg(left, ahead)
}

/// `v` rounded to the nearest multiple of `res` (halves away from zero).
pub fn quantize(v: i32, res: i32) -> i32 {
    let res = i64::from(res.max(1));
    let v = i64::from(v);
    let q = (v.abs() + res / 2) / res * res;
    i32::try_from(v.signum() * q).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX })
}

/// What a robot's sensors last reported (plan Task 12, spec G10), written
/// on the agent's scan ticks by the perception pass: the LIDAR scan, the
/// camera's detections and the radar's tracks, ROS-shaped. A component the
/// pass adds to agents whose [`Sensors`](crate::Sensors) hold a LIDAR,
/// camera or radar. Snapshot-only ([`SENSOR_READINGS`]): it joins neither
/// the state nor the content hash, so worlds without robots keep every hash;
/// the save carries the camera tracker and the radar's previous echoes, so
/// a resumed world reports exactly what an uninterrupted one does.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensorReadings {
    /// The tick of the latest camera/radar scan (ROS `header.stamp`).
    pub tick: u64,
    /// The tick of the latest LIDAR scan (half a stagger after the
    /// camera/radar pass, [`is_lidar_due`](crate::is_lidar_due)).
    #[serde(default)]
    pub lidar_tick: u64,
    /// The first LIDAR's scan (`None` without a LIDAR).
    pub lidar: Option<LaserScan>,
    /// The first camera's detections, nearest first.
    pub camera: Vec<Detection>,
    /// The first radar's tracks, nearest first.
    pub radar: Vec<RadarTrack>,
    /// The camera tracker: the track id of each thing in view (a thing that
    /// leaves the view loses its id; back in view it gets a new one).
    pub tracks: BTreeMap<SimId, u32>,
    /// The next track id to hand out (ids start at 1).
    pub next_track: u32,
    /// The radar's echoes of its previous scan (for radial velocity and
    /// whether a target moves), with that scan's tick.
    pub radar_prev: BTreeMap<SimId, RadarEcho>,
    pub radar_tick: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // determinism: test-only cross-check of the offline tables, never sim state.
    #[allow(
        clippy::float_arithmetic,
        clippy::disallowed_types,
        clippy::disallowed_methods
    )]
    fn tables_and_angles_match_floats() {
        for (d, &v) in SIN_PPM.iter().enumerate() {
            let want = ((d as f64).to_radians().sin() * 1e6).round() as i64;
            assert!((i64::from(v) - want).abs() <= 1, "sin {d}");
        }
        for (d, &v) in TAN_PPM.iter().enumerate() {
            let want = ((d as f64).to_radians().tan() * 1e6).round() as i64;
            assert!((i64::from(v) - want).abs() <= 1, "tan {d}");
        }
        for mdeg in (-360_000..=360_000).step_by(1237) {
            let r = (f64::from(mdeg) / 1000.0).to_radians();
            assert!((sin_ppm(i64::from(mdeg)) as f64 - r.sin() * 1e6).abs() < 100.0);
            assert!((cos_ppm(i64::from(mdeg)) as f64 - r.cos() * 1e6).abs() < 100.0);
        }
        for (x, y) in [
            (1, 0),
            (0, 1),
            (-1, 0),
            (0, -1),
            (3, 4),
            (-7, 2),
            (-5, -5),
            (9, -1),
        ] {
            let want = (y as f64).atan2(x as f64).to_degrees() * 1000.0;
            let got = f64::from(atan2_mdeg(y, x));
            assert!((got - want).abs() < 20.0, "({x}, {y}): {got} vs {want}");
        }
        assert_eq!(atan2_mdeg(0, -5), 180_000);
        assert_eq!(atan2_mdeg(0, 0), 0);
    }

    #[test]
    fn bearings_are_left_positive() {
        let north = Facing::new(0, 1);
        assert_eq!(bearing_mdeg(north, (0, 10)), 0);
        assert_eq!(bearing_mdeg(north, (-10, 0)), 90_000);
        assert_eq!(bearing_mdeg(north, (10, 0)), -90_000);
        assert_eq!(bearing_mdeg(north, (0, -10)), 180_000);
        assert_eq!(facing_angle_mdeg(north), 90_000);
        assert_eq!(facing_angle_mdeg(Facing::new(1, 0)), 0);
    }

    #[test]
    fn quantize_rounds_to_nearest() {
        assert_eq!(quantize(7314, 500), 7500);
        assert_eq!(quantize(7249, 500), 7000);
        assert_eq!(quantize(-5763, 2000), -6000);
        assert_eq!(quantize(-999, 2000), 0);
        assert_eq!(quantize(0, 7), 0);
    }
}
