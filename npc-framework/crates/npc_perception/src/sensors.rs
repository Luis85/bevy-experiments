//! Sensor definitions, facing and the integer field-of-view test.

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_kernel::SimPos;
use npc_kernel::hash::{Fnv64, SimHash};
use serde::{Deserialize, Serialize};

use crate::memory::PerceptionMemory;

/// Every kind bit: matches anything perceivable.
pub const ALL_KINDS: u32 = u32::MAX;

/// Scale of [`Vision::cos2_ppm`]: cos² of the half angle in parts per million.
pub const COS2_SCALE: u32 = 1_000_000;

/// `round(cos²(d°) · 10⁶)` for `d = 0..=90`, generated offline (no
/// trigonometry at runtime). Exact at 0°, 30°, 45°, 60° and 90°.
/// Angles above 90° use `cos²(180° − d)`.
pub const COS2_PPM: [u32; 91] = [
    1000000, 999695, 998782, 997261, 995134, 992404, 989074, 985148, 980631, 975528, 969846,
    963592, 956773, 949397, 941474, 933013, 924024, 914519, 904508, 894005, 883022, 871572, 859670,
    847329, 834565, 821394, 807831, 793893, 779596, 764960, 750000, 734736, 719186, 703368, 687303,
    671010, 654508, 637819, 620961, 603956, 586824, 569587, 552264, 534878, 517450, 500000, 482550,
    465122, 447736, 430413, 413176, 396044, 379039, 362181, 345492, 328990, 312697, 296632, 280814,
    265264, 250000, 235040, 220404, 206107, 192169, 178606, 165435, 152671, 140330, 128428, 116978,
    105995, 95492, 85481, 75976, 66987, 58526, 50603, 43227, 36408, 30154, 24472, 19369, 14852,
    10926, 7596, 4866, 2739, 1218, 305, 0,
];

/// cos² (ppm) of a half angle in whole degrees (clamped to 0..=180).
pub const fn cos2_ppm_of(half_fov_deg: u16) -> u32 {
    let d = if half_fov_deg > 180 {
        180
    } else {
        half_fov_deg
    };
    let folded = if d > 90 { 180 - d } else { d };
    COS2_PPM[folded as usize]
}

/// A vision cone: `range_mm`, and a field of view of `2 · half_fov_deg`
/// around the agent's [`Facing`]. The angle test uses only `cos2_ppm`
/// (cos² of the half angle, [`COS2_SCALE`]) with integer dot products and
/// squared comparisons; `half_fov_deg` picks the branch (below, at or above
/// 90°) and is what gizmos draw. Edges are inclusive. Line of sight is
/// traced over occluding grid cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct Vision {
    pub range_mm: i32,
    /// Half the field of view, 0..=180 degrees (180 = all around).
    pub half_fov_deg: u16,
    /// `cos²(half_fov_deg) · 10⁶` (from [`COS2_PPM`]).
    pub cos2_ppm: u32,
}

impl Vision {
    /// A cone of `range_mm` and half angle `half_fov_deg` (clamped to 180).
    pub const fn new(range_mm: i32, half_fov_deg: u16) -> Self {
        let half = if half_fov_deg > 180 {
            180
        } else {
            half_fov_deg
        };
        Self {
            range_mm,
            half_fov_deg: half,
            cos2_ppm: cos2_ppm_of(half),
        }
    }

    /// Whether direction `d` (target − observer) lies in the cone around
    /// `facing`, ignoring range. See [`in_fov`].
    pub fn sees_direction(&self, facing: Facing, d: (i64, i64)) -> bool {
        in_fov(facing, d, self.half_fov_deg, self.cos2_ppm)
    }
}

/// Integer field-of-view test: is the angle between `facing` and `d` at most
/// the half angle? With `dot = f·d`, `S = 10⁶`:
/// - half < 90°: `dot ≥ 0` and `dot² · S ≥ cos2 · |f|² · |d|²`;
/// - half = 90°: `dot ≥ 0`;
/// - half > 90°: `dot ≥ 0` or `dot² · S ≤ cos2 · |f|² · |d|²`;
/// - half ≥ 180°, `d = 0` or a zero facing: always inside.
///
/// Edges are inclusive. All arithmetic in `i128` (the facing is reduced to
/// at most 2¹⁵ per axis first), so no input overflows.
pub fn in_fov(facing: Facing, d: (i64, i64), half_fov_deg: u16, cos2_ppm: u32) -> bool {
    if half_fov_deg >= 180 || d == (0, 0) {
        return true;
    }
    let (fx, fz) = facing.reduced();
    if (fx, fz) == (0, 0) {
        return true;
    }
    let (fx, fz) = (i128::from(fx), i128::from(fz));
    let (dx, dz) = (i128::from(d.0), i128::from(d.1));
    let dot = fx * dx + fz * dz;
    if half_fov_deg == 90 {
        return dot >= 0;
    }
    let lhs = dot * dot * i128::from(COS2_SCALE);
    let rhs = i128::from(cos2_ppm) * (fx * fx + fz * fz) * (dx * dx + dz * dz);
    if half_fov_deg < 90 {
        dot >= 0 && lhs >= rhs
    } else {
        dot >= 0 || lhs <= rhs
    }
}

/// Hearing: `Noise` stimuli within `radius_mm` (and within the noise's own
/// radius) are heard. No line of sight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct Hearing {
    pub radius_mm: i32,
}

/// Proximity: every perceivable thing within `radius_mm`, in any direction,
/// without line of sight (touch, smell, "someone right behind me").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct Proximity {
    pub radius_mm: i32,
}

/// Snapshot name of [`ReachSense`] (ADR 0013; it is not in the state hash).
pub const REACH_SENSE: &str = "reach_sense";

/// Reach (opt-in, plan Task 27): every perceivable thing within
/// `radius_mm`, in any direction, **with** line of sight (the same grid
/// line of sight as vision): what an agent could touch, never through a
/// wall. Sensed on the agent's scan ticks like vision and proximity, into
/// its [`PerceptionMemory`] (`SenseKind::Reach`). A separate component next
/// to [`Sensors`] (which the agent needs): it is registered for snapshots
/// only ([`REACH_SENSE`]), so worlds without it keep every state and content
/// hash. Humans use it as their reach, robots as a bump/proximity sensor.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct ReachSense {
    pub radius_mm: i32,
}

/// A planar LIDAR (plan Task 12, spec G10): `beams` beams over `fov_deg`
/// degrees centred on the agent's [`Facing`], each reporting the distance to
/// the first blocking or occluding cell or cell holding a perceivable thing
/// within `range_mm`, with deterministic noise of `±noise_ppm` of the range.
/// It reports ranges only ([`LaserScan`](crate::LaserScan)), never
/// identities: it feeds no [`PerceptionMemory`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct Lidar {
    pub range_mm: i32,
    /// The whole field of view, 1..=360 degrees.
    pub fov_deg: u16,
    pub beams: u16,
    /// Noise, parts per million of the measured range (10 000 = ±1 %).
    pub noise_ppm: u32,
}

/// A camera (plan Task 12): detections only, never images
/// ([`CameraDetection`](crate::CameraDetection)). Things in its cone (like
/// [`Vision`]: `range_mm`, `half_fov_deg` to each side of the [`Facing`],
/// `cos2_ppm`) with line of sight; feeds the memory as `camera`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct Camera {
    pub range_mm: i32,
    pub half_fov_deg: u16,
    /// `cos²(half_fov_deg) · 10⁶` (from [`COS2_PPM`]).
    pub cos2_ppm: u32,
}

impl Camera {
    /// A camera of `range_mm` and half angle `half_fov_deg` (clamped to 180).
    pub const fn new(range_mm: i32, half_fov_deg: u16) -> Self {
        let v = Vision::new(range_mm, half_fov_deg);
        Self {
            range_mm,
            half_fov_deg: v.half_fov_deg,
            cos2_ppm: v.cos2_ppm,
        }
    }

    /// Whether direction `d` lies in the cone (see [`in_fov`]).
    pub fn sees_direction(&self, facing: Facing, d: (i64, i64)) -> bool {
        in_fov(facing, d, self.half_fov_deg, self.cos2_ppm)
    }
}

/// A radar (plan Task 12): a wide cone with line of sight (walls block
/// it), tracks ([`RadarTrack`](crate::RadarTrack)) with bearing and range
/// quantized to `bearing_res_mdeg` / `range_res_mm`, a radial velocity and
/// an amplitude that is higher for things that move; feeds the memory as
/// `radar`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct Radar {
    pub range_mm: i32,
    pub half_fov_deg: u16,
    /// `cos²(half_fov_deg) · 10⁶` (from [`COS2_PPM`]).
    pub cos2_ppm: u32,
    pub range_res_mm: i32,
    pub bearing_res_mdeg: i32,
}

impl Radar {
    /// A radar of `range_mm`, half angle `half_fov_deg` (clamped to 180) and
    /// the given resolutions (at least 1).
    pub const fn new(
        range_mm: i32,
        half_fov_deg: u16,
        range_res_mm: i32,
        bearing_res_mdeg: i32,
    ) -> Self {
        let v = Vision::new(range_mm, half_fov_deg);
        Self {
            range_mm,
            half_fov_deg: v.half_fov_deg,
            cos2_ppm: v.cos2_ppm,
            range_res_mm: if range_res_mm < 1 { 1 } else { range_res_mm },
            bearing_res_mdeg: if bearing_res_mdeg < 1 {
                1
            } else {
                bearing_res_mdeg
            },
        }
    }

    /// Whether direction `d` lies in the cone (see [`in_fov`]).
    pub fn sees_direction(&self, facing: Facing, d: (i64, i64)) -> bool {
        in_fov(facing, d, self.half_fov_deg, self.cos2_ppm)
    }
}

/// One sensor definition. The robot sensors (`Lidar`, `Camera`, `Radar`,
/// plan Task 12) were appended, so the older variants keep their encodings
/// and hash tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum Sensor {
    Vision(Vision),
    Hearing(Hearing),
    Proximity(Proximity),
    Lidar(Lidar),
    Camera(Camera),
    Radar(Radar),
}

/// What an agent senses with. Put it on a sim entity (with `SimId` and
/// `SimPos`); it brings a [`Facing`] and an empty [`PerceptionMemory`].
///
/// `interest` is the kind mask the plugin's default conditions
/// (`sees_kind`, `remembers_kind`) test against ([`ALL_KINDS`] by default).
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[require(Facing, PerceptionMemory)]
#[reflect(Component)]
pub struct Sensors {
    pub sensors: Vec<Sensor>,
    pub interest: u32,
}

impl Default for Sensors {
    fn default() -> Self {
        Self {
            sensors: Vec::new(),
            interest: ALL_KINDS,
        }
    }
}

impl Sensors {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a vision cone.
    pub fn vision(mut self, range_mm: i32, half_fov_deg: u16) -> Self {
        self.sensors
            .push(Sensor::Vision(Vision::new(range_mm, half_fov_deg)));
        self
    }

    /// Adds hearing.
    pub fn hearing(mut self, radius_mm: i32) -> Self {
        self.sensors.push(Sensor::Hearing(Hearing { radius_mm }));
        self
    }

    /// Adds proximity.
    pub fn proximity(mut self, radius_mm: i32) -> Self {
        self.sensors
            .push(Sensor::Proximity(Proximity { radius_mm }));
        self
    }

    /// Adds a LIDAR (see [`Lidar`]).
    pub fn lidar(mut self, range_mm: i32, fov_deg: u16, beams: u16, noise_ppm: u32) -> Self {
        self.sensors.push(Sensor::Lidar(Lidar {
            range_mm,
            fov_deg,
            beams,
            noise_ppm,
        }));
        self
    }

    /// Adds a camera (see [`Camera`]).
    pub fn camera(mut self, range_mm: i32, half_fov_deg: u16) -> Self {
        self.sensors
            .push(Sensor::Camera(Camera::new(range_mm, half_fov_deg)));
        self
    }

    /// Adds a radar (see [`Radar`]).
    pub fn radar(
        mut self,
        range_mm: i32,
        half_fov_deg: u16,
        range_res_mm: i32,
        bearing_res_mdeg: i32,
    ) -> Self {
        self.sensors.push(Sensor::Radar(Radar::new(
            range_mm,
            half_fov_deg,
            range_res_mm,
            bearing_res_mdeg,
        )));
        self
    }

    /// The first LIDAR (the one [`SensorReadings`](crate::SensorReadings)
    /// reports).
    pub fn lidar_sensor(&self) -> Option<Lidar> {
        self.sensors.iter().find_map(|s| match s {
            Sensor::Lidar(l) => Some(*l),
            _ => None,
        })
    }

    /// The first camera (the one that reports and feeds the memory).
    pub fn camera_sensor(&self) -> Option<Camera> {
        self.sensors.iter().find_map(|s| match s {
            Sensor::Camera(c) => Some(*c),
            _ => None,
        })
    }

    /// The first radar (the one that reports and feeds the memory).
    pub fn radar_sensor(&self) -> Option<Radar> {
        self.sensors.iter().find_map(|s| match s {
            Sensor::Radar(r) => Some(*r),
            _ => None,
        })
    }

    /// Whether it has a LIDAR, camera or radar (it then keeps
    /// [`SensorReadings`](crate::SensorReadings)).
    pub fn has_robot_sensors(&self) -> bool {
        self.sensors
            .iter()
            .any(|s| matches!(s, Sensor::Lidar(_) | Sensor::Camera(_) | Sensor::Radar(_)))
    }

    /// Sets the interest mask of the default conditions.
    pub fn interest(mut self, kinds: u32) -> Self {
        self.interest = kinds;
        self
    }

    pub fn visions(&self) -> impl Iterator<Item = Vision> + '_ {
        self.sensors.iter().filter_map(|s| match s {
            Sensor::Vision(v) => Some(*v),
            _ => None,
        })
    }

    /// The largest hearing radius (`None` without hearing).
    pub fn hearing_radius(&self) -> Option<i32> {
        self.sensors
            .iter()
            .filter_map(|s| match s {
                Sensor::Hearing(h) => Some(h.radius_mm.max(0)),
                _ => None,
            })
            .max()
    }

    /// The largest proximity radius (`None` without proximity).
    pub fn proximity_radius(&self) -> Option<i32> {
        self.sensors
            .iter()
            .filter_map(|s| match s {
                Sensor::Proximity(p) => Some(p.radius_mm.max(0)),
                _ => None,
            })
            .max()
    }

    /// The radius a vision/proximity/camera/radar scan must query (0
    /// without any; the LIDAR traces its own beams).
    pub fn scan_radius(&self) -> i32 {
        self.sensors
            .iter()
            .filter_map(|s| match s {
                Sensor::Vision(v) => Some(v.range_mm.max(0)),
                Sensor::Proximity(p) => Some(p.radius_mm.max(0)),
                Sensor::Camera(c) => Some(c.range_mm.max(0)),
                Sensor::Radar(r) => Some(r.range_mm.max(0)),
                Sensor::Hearing(_) | Sensor::Lidar(_) => None,
            })
            .max()
            .unwrap_or(0)
    }
}

/// The direction an agent looks in: a ground-plane vector in mm (any
/// length; +z by default). Updated from movement at the end of every tick
/// (`facing_from_movement_system`, `NpcSet::Cleanup`) unless the entity has
/// [`FixedFacing`].
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Facing {
    pub x: i32,
    pub z: i32,
}

impl Default for Facing {
    fn default() -> Self {
        Self { x: 0, z: 1 }
    }
}

impl Facing {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// Facing from `from` towards `to` (unchanged default when equal).
    pub fn towards(from: SimPos, to: SimPos) -> Self {
        let dx = i64::from(to.x) - i64::from(from.x);
        let dz = i64::from(to.z) - i64::from(from.z);
        if (dx, dz) == (0, 0) {
            return Self::default();
        }
        let (x, z) = reduce(dx, dz);
        Self { x, z }
    }

    /// The vector with both axes reduced (by the same right shift) to at
    /// most 2¹⁵ in magnitude, keeping the direction up to rounding.
    pub fn reduced(self) -> (i32, i32) {
        reduce(i64::from(self.x), i64::from(self.z))
    }
}

fn reduce(mut x: i64, mut z: i64) -> (i32, i32) {
    const LIMIT: i64 = 1 << 15;
    while x.abs() > LIMIT || z.abs() > LIMIT {
        x /= 2;
        z /= 2;
    }
    (x as i32, z as i32)
}

/// Marker: movement does not turn this agent ([`Facing`] is set by the host
/// or content).
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct FixedFacing;

/// Things that can be seen (or felt by proximity): a bit set of content
/// kinds (e.g. bit 0 = "unit", bit 1 = "food"). Entities without it are
/// never sensed by vision, proximity or reach.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct Perceivable {
    pub kinds: u32,
}

impl Perceivable {
    pub const fn new(kinds: u32) -> Self {
        Self { kinds }
    }
}

// ---------------------------------------------------------------------------
// Hash encodings
// ---------------------------------------------------------------------------

impl SimHash for Sensors {
    const NAME: &'static str = "sensors";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(u32::try_from(self.sensors.len()).expect("too many sensors"));
        for s in &self.sensors {
            match *s {
                Sensor::Vision(v) => {
                    h.u8(0);
                    h.i32(v.range_mm);
                    h.u16(v.half_fov_deg);
                    h.u32(v.cos2_ppm);
                }
                Sensor::Hearing(x) => {
                    h.u8(1);
                    h.i32(x.radius_mm);
                }
                Sensor::Proximity(x) => {
                    h.u8(2);
                    h.i32(x.radius_mm);
                }
                Sensor::Lidar(x) => {
                    h.u8(3);
                    h.i32(x.range_mm);
                    h.u16(x.fov_deg);
                    h.u16(x.beams);
                    h.u32(x.noise_ppm);
                }
                Sensor::Camera(x) => {
                    h.u8(4);
                    h.i32(x.range_mm);
                    h.u16(x.half_fov_deg);
                    h.u32(x.cos2_ppm);
                }
                Sensor::Radar(x) => {
                    h.u8(5);
                    h.i32(x.range_mm);
                    h.u16(x.half_fov_deg);
                    h.u32(x.cos2_ppm);
                    h.i32(x.range_res_mm);
                    h.i32(x.bearing_res_mdeg);
                }
            }
        }
        h.u32(self.interest);
    }
}

impl SimHash for Facing {
    const NAME: &'static str = "facing";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.i32(self.x);
        h.i32(self.z);
    }
}

impl SimHash for FixedFacing {
    const NAME: &'static str = "fixed_facing";
    fn sim_hash(&self, _h: &mut Fnv64) {}
}

impl SimHash for Perceivable {
    const NAME: &'static str = "perceivable";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.kinds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_monotonic_and_exact_where_it_should_be() {
        for w in COS2_PPM.windows(2) {
            assert!(w[0] > w[1]);
        }
        assert_eq!(cos2_ppm_of(0), 1_000_000);
        assert_eq!(cos2_ppm_of(30), 750_000);
        assert_eq!(cos2_ppm_of(45), 500_000);
        assert_eq!(cos2_ppm_of(60), 250_000);
        assert_eq!(cos2_ppm_of(90), 0);
        assert_eq!(cos2_ppm_of(135), 500_000);
        assert_eq!(cos2_ppm_of(200), 1_000_000);
    }

    #[test]
    // determinism: test-only cross-check of the offline table, never sim state.
    #[allow(
        clippy::float_arithmetic,
        clippy::disallowed_types,
        clippy::disallowed_methods
    )]
    fn table_matches_cos_squared() {
        for (d, &v) in COS2_PPM.iter().enumerate() {
            let c = (d as f64).to_radians().cos();
            let want = (c * c * 1_000_000.0).round() as i64;
            assert!((i64::from(v) - want).abs() <= 1, "{d}: {v} vs {want}");
        }
    }

    #[test]
    fn reduce_keeps_direction() {
        assert_eq!(Facing::new(i32::MAX, 0).reduced().1, 0);
        let (x, z) = Facing::new(-2_000_000, 2_000_000).reduced();
        assert_eq!(x, -z);
        assert!(x.abs() <= 1 << 15);
    }
}
