//! [`Angle`]: binary angles, 65536 units per full turn.

use core::ops::{Add, Neg, Sub};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Fx;
use crate::trig;

/// A binary angle: 65536 units make a full turn, and all arithmetic wraps.
///
/// Headings use 0 = +x, increasing counter-clockwise toward +y.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Angle(u16);

/// Denominator taking raw `Fx` degrees to angle units:
/// units = deg · 65536 / 360 = deg_raw / (360 · 2^16).
const DEG_RAW_PER_UNIT: i128 = 360 << 16;

impl Angle {
    /// 0°.
    pub const ZERO: Angle = Angle(0);
    /// 90°.
    pub const QUARTER: Angle = Angle(16_384);
    /// 180°.
    pub const HALF: Angle = Angle(32_768);

    /// The angle of `raw` units.
    #[inline]
    pub const fn from_raw(raw: u16) -> Angle {
        Angle(raw)
    }

    /// The raw units, 0..=65535.
    #[inline]
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// From degrees (any value; wraps), rounded to the nearest unit, ties
    /// toward +∞.
    pub const fn from_degrees(deg: Fx) -> Angle {
        let units = (deg.raw() as i128 * 2 + DEG_RAW_PER_UNIT).div_euclid(2 * DEG_RAW_PER_UNIT);
        Angle(units.rem_euclid(65_536) as u16)
    }

    /// In degrees, `[0, 360)`. Exact: one unit is 360/65536 degrees.
    #[inline]
    pub const fn to_degrees(self) -> Fx {
        Fx::from_raw(self.0 as i64 * (360 << 16))
    }

    /// In radians, `[0, 2π)`. Presentation only.
    #[expect(
        clippy::float_arithmetic,
        reason = "load-time or presentation conversion"
    )]
    pub fn to_radians_f32(self) -> f32 {
        (self.0 as f64 * (core::f64::consts::TAU / 65_536.0)) as f32
    }

    /// The shortest signed turn from `self` to `to`, in `[-32768, 32767]`
    /// units. Positive is counter-clockwise.
    #[inline]
    pub const fn signed_diff(self, to: Angle) -> i32 {
        to.0.wrapping_sub(self.0) as i16 as i32
    }

    /// Turns from `self` toward `target` the short way, by at most
    /// `max_step`; returns `target` once it is within reach.
    ///
    /// Tie: when `target` is exactly opposite (`signed_diff` = -32768) both
    /// ways are equally short, and it always turns clockwise (decreasing raw).
    pub const fn rotate_towards(self, target: Angle, max_step: Angle) -> Angle {
        let diff = self.signed_diff(target);
        let step = max_step.0 as i32;
        if diff.abs() <= step {
            target
        } else if diff > 0 {
            Angle(self.0.wrapping_add(max_step.0))
        } else {
            Angle(self.0.wrapping_sub(max_step.0))
        }
    }

    /// Sine, from a quarter-wave lookup table. Exact at the cardinal angles.
    #[inline]
    pub fn sin(self) -> Fx {
        Fx::from_raw(trig::sin_raw(self.0))
    }

    /// Cosine: `sin(self + QUARTER)`.
    #[inline]
    pub fn cos(self) -> Fx {
        Fx::from_raw(trig::sin_raw(self.0.wrapping_add(Self::QUARTER.0)))
    }

    /// The angle of the vector `(x, y)`; `atan2(0, 0)` is `ZERO`. Exact on the
    /// axes, otherwise within a few units.
    #[inline]
    pub fn atan2(y: Fx, x: Fx) -> Angle {
        Angle(trig::atan2_raw(y.raw(), x.raw()))
    }
}

impl Add for Angle {
    type Output = Angle;
    #[inline]
    fn add(self, rhs: Angle) -> Angle {
        Angle(self.0.wrapping_add(rhs.0))
    }
}

impl Sub for Angle {
    type Output = Angle;
    #[inline]
    fn sub(self, rhs: Angle) -> Angle {
        Angle(self.0.wrapping_sub(rhs.0))
    }
}

impl Neg for Angle {
    type Output = Angle;
    #[inline]
    fn neg(self) -> Angle {
        Angle(self.0.wrapping_neg())
    }
}

/// Human-readable formats get degrees (`f64`, going through [`Fx`]); binary
/// formats get the raw `u16`.
impl Serialize for Angle {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            self.to_degrees().serialize(serializer)
        } else {
            serializer.serialize_u16(self.0)
        }
    }
}

/// Human-readable formats read degrees (any number [`Fx`] accepts), wrapped
/// into a turn. Binary formats read the raw `u16`.
impl<'de> Deserialize<'de> for Angle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Angle, D::Error> {
        if deserializer.is_human_readable() {
            Fx::deserialize(deserializer).map(Angle::from_degrees)
        } else {
            u16::deserialize(deserializer).map(Angle)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    #[test]
    fn sin_table_exact_points() {
        assert_eq!(Angle::QUARTER.sin(), Fx::ONE);
        assert_eq!(Angle::HALF.sin(), Fx::ZERO);
        assert_eq!(Angle::ZERO.cos(), Fx::ONE);
    }

    #[test]
    fn cardinal_points() {
        let three_q = Angle::from_raw(49_152);
        assert_eq!(three_q.sin(), -Fx::ONE);
        assert_eq!(Angle::ZERO.sin(), Fx::ZERO);
        assert_eq!(Angle::QUARTER.cos(), Fx::ZERO);
        assert_eq!(Angle::HALF.cos(), -Fx::ONE);
        assert_eq!(three_q.cos(), Fx::ZERO);
    }

    #[test]
    fn sin_matches_f64_everywhere() {
        for r in 0..=u16::MAX {
            let a = Angle::from_raw(r);
            let e = (r as f64 / 65536.0 * TAU).sin();
            assert!((a.sin().to_f64() - e).abs() < 1e-6, "{r}");
            let c = (r as f64 / 65536.0 * TAU).cos();
            assert!((a.cos().to_f64() - c).abs() < 1e-6, "cos {r}");
        }
    }

    #[test]
    fn atan2_error_within_4_units() {
        for x in -50i32..=50 {
            for y in -50i32..=50 {
                if x == 0 && y == 0 {
                    continue;
                }
                let got = Angle::atan2(Fx::from_int(y), Fx::from_int(x));
                let e = ((y as f64).atan2(x as f64) * 65536.0 / TAU).round() as i64;
                let expected = Angle::from_raw(e.rem_euclid(65_536) as u16);
                let d = expected.signed_diff(got);
                assert!(
                    d.abs() <= 4,
                    "x={x} y={y} got={got:?} expected={expected:?}"
                );
            }
        }
    }

    #[test]
    fn atan2_axes_and_extremes() {
        assert_eq!(Angle::atan2(Fx::ZERO, Fx::ZERO), Angle::ZERO);
        assert_eq!(Angle::atan2(Fx::ZERO, Fx::ONE), Angle::ZERO);
        assert_eq!(Angle::atan2(Fx::ONE, Fx::ZERO), Angle::QUARTER);
        assert_eq!(Angle::atan2(Fx::ZERO, -Fx::ONE), Angle::HALF);
        assert_eq!(Angle::atan2(-Fx::ONE, Fx::ZERO), Angle::from_raw(49_152));
        // Extreme magnitudes must neither overflow nor lose the direction.
        let d = Angle::atan2(Fx::MIN, Fx::MIN).signed_diff(Angle::from_raw(40_960));
        assert!(d.abs() <= 4, "{d}");
        let d = Angle::atan2(Fx::MAX, Fx::MIN).signed_diff(Angle::from_raw(24_576));
        assert!(d.abs() <= 4, "{d}");
        let d = Angle::atan2(Fx::EPSILON, Fx::EPSILON).signed_diff(Angle::from_raw(8_192));
        assert!(d.abs() <= 4, "{d}");
    }

    #[test]
    fn rotate_towards_takes_short_way() {
        let a = Angle::from_degrees(Fx::from_int(350));
        let t = Angle::from_degrees(Fx::from_int(10));
        assert_eq!(
            a.rotate_towards(t, Angle::from_degrees(Fx::from_int(5))),
            Angle::from_degrees(Fx::from_int(355))
        );
    }

    #[test]
    fn rotate_towards_snaps_when_close() {
        let a = Angle::from_raw(100);
        let t = Angle::from_raw(65_500);
        assert_eq!(a.rotate_towards(t, Angle::from_raw(200)), t);
        assert_eq!(
            a.rotate_towards(t, Angle::from_raw(10)),
            Angle::from_raw(90)
        );
        assert_eq!(a.rotate_towards(a, Angle::ZERO), a);
    }

    #[test]
    fn rotate_towards_opposite_turns_clockwise() {
        let a = Angle::from_raw(1_000);
        let t = a + Angle::HALF;
        assert_eq!(
            a.rotate_towards(t, Angle::from_raw(10)),
            Angle::from_raw(990)
        );
        assert_eq!(
            Angle::ZERO.rotate_towards(Angle::HALF, Angle::from_raw(10)),
            Angle::from_raw(65_526)
        );
    }

    /// Replay-relevant bits of the CORDIC atan2. A change here changes every
    /// replay; update only deliberately.
    #[test]
    fn atan2_pinned_bits() {
        let cases = [
            ((1, 1), 8_194),
            ((1, 2), 4_834),
            ((3, 4), 6_712),
            ((-5, 12), 61_418),
            ((7, -3), 20_608),
            ((-1, -1), 40_962),
            ((50, -49), 24_468),
        ];
        for ((y, x), raw) in cases {
            let got = Angle::atan2(Fx::from_int(y), Fx::from_int(x)).raw();
            assert_eq!(got, raw, "atan2({y}, {x})");
        }
    }

    #[test]
    fn wrapping_ops_and_signed_diff() {
        let a = Angle::from_raw(65_000);
        let b = Angle::from_raw(1_000);
        assert_eq!((a + b).raw(), 464);
        assert_eq!((b - a).raw(), 1_536);
        assert_eq!((-b).raw(), 64_536);
        assert_eq!(-Angle::ZERO, Angle::ZERO);
        assert_eq!(a.signed_diff(b), 1_536);
        assert_eq!(b.signed_diff(a), -1_536);
        assert_eq!(Angle::ZERO.signed_diff(Angle::HALF), -32_768);
        assert_eq!(Angle::ZERO.signed_diff(Angle::from_raw(32_767)), 32_767);
    }

    #[test]
    fn degrees_roundtrip() {
        assert_eq!(Angle::from_degrees(Fx::from_int(90)), Angle::QUARTER);
        assert_eq!(Angle::from_degrees(Fx::from_int(180)), Angle::HALF);
        assert_eq!(Angle::from_degrees(Fx::from_int(360)), Angle::ZERO);
        assert_eq!(
            Angle::from_degrees(Fx::from_int(-90)),
            Angle::from_raw(49_152)
        );
        assert_eq!(Angle::from_degrees(Fx::from_int(450)), Angle::QUARTER);
        assert_eq!(Angle::QUARTER.to_degrees(), Fx::from_int(90));
        assert_eq!(Angle::from_raw(1).to_degrees(), Fx::from_ratio(360, 65_536));
        for r in (0..=u16::MAX).step_by(7) {
            let a = Angle::from_raw(r);
            assert_eq!(Angle::from_degrees(a.to_degrees()), a);
        }
        assert!((Angle::HALF.to_radians_f32() - std::f32::consts::PI).abs() < 1e-6);
    }

    #[test]
    fn serde_as_degrees() {
        assert_eq!(ron::to_string(&Angle::QUARTER).unwrap(), "90.0");
        assert_eq!(ron::from_str::<Angle>("90").unwrap(), Angle::QUARTER);
        assert_eq!(
            ron::from_str::<Angle>("-45.0").unwrap(),
            Angle::from_raw(57_344)
        );
        assert!(ron::from_str::<Angle>("1e40").is_err());
    }

    #[test]
    fn postcard_roundtrip_is_bit_exact() {
        for r in 0..=u16::MAX {
            let a = Angle::from_raw(r);
            let bytes = postcard::to_allocvec(&a).unwrap();
            assert_eq!(postcard::from_bytes::<Angle>(&bytes).unwrap(), a);
        }
        // Binary form is the raw u16, not degrees.
        let a = Angle::from_raw(12_345);
        assert_eq!(
            postcard::to_allocvec(&a).unwrap(),
            postcard::to_allocvec(&12_345u16).unwrap()
        );
    }
}
