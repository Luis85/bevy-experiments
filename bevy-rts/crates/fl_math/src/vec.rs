//! [`FxVec2`] and [`FxVec3`]: fixed-point vectors.

use core::ops::{Add, Div, Mul, Neg, Sub};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Angle, Fx};

/// A 2D vector on the sim ground plane (`x` east, `y` north, metres).
///
/// Serialized as a tuple `(x, y)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FxVec2 {
    pub x: Fx,
    pub y: Fx,
}

impl FxVec2 {
    /// `(0, 0)`.
    pub const ZERO: FxVec2 = FxVec2::new(Fx::ZERO, Fx::ZERO);

    #[inline]
    pub const fn new(x: Fx, y: Fx) -> FxVec2 {
        FxVec2 { x, y }
    }

    #[inline]
    pub fn dot(self, rhs: FxVec2) -> Fx {
        self.x * rhs.x + self.y * rhs.y
    }

    /// The z component of the 3D cross product: positive when `rhs` is
    /// counter-clockwise from `self`.
    #[inline]
    pub fn perp_dot(self, rhs: FxVec2) -> Fx {
        self.x * rhs.y - self.y * rhs.x
    }

    /// `x² + y²`. Overflows for lengths ≥ 2^15.5 (~46 km); use
    /// [`length`](Self::length) for anything that might be large.
    #[inline]
    pub fn length_sq(self) -> Fx {
        self.dot(self)
    }

    /// The exact length, truncated to the raw unit below; saturates at
    /// `Fx::MAX` instead of overflowing.
    pub fn length(self) -> Fx {
        Fx::from_raw(self.length_raw().min(i64::MAX as u128) as i64)
    }

    /// `|self - rhs|²`.
    #[inline]
    pub fn distance_sq(self, rhs: FxVec2) -> Fx {
        (self - rhs).length_sq()
    }

    /// The unit vector in the same direction, or `ZERO` for the zero vector.
    /// Works for any magnitude, including `EPSILON`-sized vectors.
    pub fn normalize_or_zero(self) -> FxVec2 {
        let len = self.length_raw() as i128;
        if len == 0 {
            return FxVec2::ZERO;
        }
        // |component| ≤ len, so each quotient is within [-1, 1].
        let unit = |c: Fx| Fx::from_raw((((c.raw() as i128) << 32) / len) as i64);
        FxVec2::new(unit(self.x), unit(self.y))
    }

    /// The unit vector pointing at `angle`: `(cos, sin)`.
    #[inline]
    pub fn from_angle(angle: Angle) -> FxVec2 {
        FxVec2::new(angle.cos(), angle.sin())
    }

    /// Rotated counter-clockwise by `angle`.
    pub fn rotate(self, angle: Angle) -> FxVec2 {
        let (c, s) = (angle.cos(), angle.sin());
        FxVec2::new(self.x * c - self.y * s, self.x * s + self.y * c)
    }

    /// The direction of this vector; `ZERO` for the zero vector.
    #[inline]
    pub fn angle(self) -> Angle {
        Angle::atan2(self.y, self.x)
    }

    /// Each component clamped into `[min, max]`. Panics if `min > max` on
    /// either axis.
    pub fn clamp_to_rect(self, min: FxVec2, max: FxVec2) -> FxVec2 {
        FxVec2::new(self.x.clamp(min.x, max.x), self.y.clamp(min.y, max.y))
    }

    /// floor(sqrt(x_raw² + y_raw²)): the length in raw units, computed exactly
    /// in u128 (the sum is below 2^127, the root below 2^63.5).
    fn length_raw(self) -> u128 {
        let x = self.x.raw().unsigned_abs() as u128;
        let y = self.y.raw().unsigned_abs() as u128;
        (x * x + y * y).isqrt()
    }
}

impl Add for FxVec2 {
    type Output = FxVec2;
    #[inline]
    fn add(self, rhs: FxVec2) -> FxVec2 {
        FxVec2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for FxVec2 {
    type Output = FxVec2;
    #[inline]
    fn sub(self, rhs: FxVec2) -> FxVec2 {
        FxVec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Neg for FxVec2 {
    type Output = FxVec2;
    #[inline]
    fn neg(self) -> FxVec2 {
        FxVec2::new(-self.x, -self.y)
    }
}

impl Mul<Fx> for FxVec2 {
    type Output = FxVec2;
    #[inline]
    fn mul(self, rhs: Fx) -> FxVec2 {
        FxVec2::new(self.x * rhs, self.y * rhs)
    }
}

impl Div<Fx> for FxVec2 {
    type Output = FxVec2;
    #[inline]
    fn div(self, rhs: Fx) -> FxVec2 {
        FxVec2::new(self.x / rhs, self.y / rhs)
    }
}

impl Serialize for FxVec2 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (self.x, self.y).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FxVec2 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<FxVec2, D::Error> {
        <(Fx, Fx)>::deserialize(deserializer).map(|(x, y)| FxVec2::new(x, y))
    }
}

/// A 3D vector: the ground-plane `x`, `y` plus height `z`.
///
/// Serialized as a tuple `(x, y, z)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FxVec3 {
    pub x: Fx,
    pub y: Fx,
    pub z: Fx,
}

impl FxVec3 {
    /// `(0, 0, 0)`.
    pub const ZERO: FxVec3 = FxVec3::new(Fx::ZERO, Fx::ZERO, Fx::ZERO);

    #[inline]
    pub const fn new(x: Fx, y: Fx, z: Fx) -> FxVec3 {
        FxVec3 { x, y, z }
    }

    /// The ground-plane part, dropping `z`.
    #[inline]
    pub const fn xy(self) -> FxVec2 {
        FxVec2::new(self.x, self.y)
    }
}

impl Add for FxVec3 {
    type Output = FxVec3;
    #[inline]
    fn add(self, rhs: FxVec3) -> FxVec3 {
        FxVec3::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for FxVec3 {
    type Output = FxVec3;
    #[inline]
    fn sub(self, rhs: FxVec3) -> FxVec3 {
        FxVec3::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Mul<Fx> for FxVec3 {
    type Output = FxVec3;
    #[inline]
    fn mul(self, rhs: Fx) -> FxVec3 {
        FxVec3::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Serialize for FxVec3 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (self.x, self.y, self.z).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FxVec3 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<FxVec3, D::Error> {
        <(Fx, Fx, Fx)>::deserialize(deserializer).map(|(x, y, z)| FxVec3::new(x, y, z))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: i32, y: i32) -> FxVec2 {
        FxVec2::new(Fx::from_int(x), Fx::from_int(y))
    }

    #[test]
    fn basic_ops() {
        let a = v(3, 4);
        let b = v(1, -2);
        assert_eq!(a + b, v(4, 2));
        assert_eq!(a - b, v(2, 6));
        assert_eq!(-a, v(-3, -4));
        assert_eq!(a * Fx::from_int(2), v(6, 8));
        assert_eq!(
            a / Fx::from_int(2),
            FxVec2::new(Fx::from_ratio(3, 2), Fx::from_int(2))
        );
        assert_eq!(a.dot(b), Fx::from_int(-5));
        assert_eq!(a.perp_dot(b), Fx::from_int(-10));
        assert_eq!(a.length_sq(), Fx::from_int(25));
        assert_eq!(a.length(), Fx::from_int(5));
        assert_eq!(a.distance_sq(b), Fx::from_int(40));
        assert_eq!(FxVec2::default(), FxVec2::ZERO);
    }

    #[test]
    fn length_extremes() {
        let tiny = FxVec2::new(Fx::EPSILON, Fx::EPSILON);
        assert_eq!(tiny.length(), Fx::EPSILON); // floor(sqrt(2)) raw units
        let huge = FxVec2::new(Fx::MAX, Fx::MAX);
        assert_eq!(huge.length(), Fx::MAX); // saturates instead of wrapping
    }

    #[test]
    fn normalize() {
        assert_eq!(FxVec2::ZERO.normalize_or_zero(), FxVec2::ZERO);
        let n = v(3, 4).normalize_or_zero();
        assert_eq!(n, FxVec2::new(Fx::from_ratio(3, 5), Fx::from_ratio(4, 5)));
        assert_eq!(v(-7, 0).normalize_or_zero(), v(-1, 0));
        let tiny = FxVec2::new(Fx::EPSILON, Fx::ZERO).normalize_or_zero();
        assert_eq!(tiny, v(1, 0));
        let big = FxVec2::new(Fx::MIN, Fx::ZERO).normalize_or_zero();
        assert_eq!(big, v(-1, 0));
    }

    #[test]
    fn angles() {
        assert_eq!(FxVec2::from_angle(Angle::ZERO), v(1, 0));
        assert_eq!(FxVec2::from_angle(Angle::QUARTER), v(0, 1));
        assert_eq!(v(2, 3).rotate(Angle::QUARTER), v(-3, 2));
        assert_eq!(v(2, 3).rotate(Angle::HALF), v(-2, -3));
        assert_eq!(v(0, 5).angle(), Angle::QUARTER);
        assert_eq!(v(-5, 0).angle(), Angle::HALF);
    }

    #[test]
    fn postcard_roundtrip_is_bit_exact() {
        let p = FxVec2::new(
            Fx::from_raw(0x7FFF_FFFF_FFFF_FFF1),
            Fx::from_raw(-0x0123_4567_89AB_CDEF),
        );
        let bytes = postcard::to_allocvec(&p).unwrap();
        assert_eq!(postcard::from_bytes::<FxVec2>(&bytes).unwrap(), p);
        let q = FxVec3::new(Fx::from_raw(3), Fx::MIN, Fx::from_raw(0x1_0000_0001));
        let bytes = postcard::to_allocvec(&q).unwrap();
        assert_eq!(postcard::from_bytes::<FxVec3>(&bytes).unwrap(), q);
    }

    #[test]
    fn from_angle_roundtrip_exhaustive() {
        for r in 0..=u16::MAX {
            let a = Angle::from_raw(r);
            let back = FxVec2::from_angle(a).angle();
            assert!(a.signed_diff(back).abs() <= 4, "{a:?} -> {back:?}");
        }
    }

    #[test]
    fn clamp_rect() {
        let lo = v(0, 0);
        let hi = v(10, 20);
        assert_eq!(v(-5, 30).clamp_to_rect(lo, hi), v(0, 20));
        assert_eq!(v(5, 5).clamp_to_rect(lo, hi), v(5, 5));
    }

    #[test]
    fn serde_tuple() {
        let p = FxVec2::new(Fx::from_ratio(3, 2), Fx::from_int(-2));
        assert_eq!(ron::to_string(&p).unwrap(), "(1.5,-2.0)");
        assert_eq!(ron::from_str::<FxVec2>("(1.5, -2)").unwrap(), p);
        let q = FxVec3::new(Fx::ONE, Fx::HALF, Fx::from_int(3));
        assert_eq!(ron::from_str::<FxVec3>("(1, 0.5, 3)").unwrap(), q);
        assert_eq!(ron::to_string(&q).unwrap(), "(1.0,0.5,3.0)");
    }

    #[test]
    fn vec3() {
        let a = FxVec3::new(Fx::ONE, Fx::from_int(2), Fx::from_int(3));
        let b = FxVec3::new(Fx::ONE, Fx::ONE, Fx::ONE);
        assert_eq!(
            a + b,
            FxVec3::new(Fx::from_int(2), Fx::from_int(3), Fx::from_int(4))
        );
        assert_eq!(a - b, FxVec3::new(Fx::ZERO, Fx::ONE, Fx::from_int(2)));
        assert_eq!(
            a * Fx::from_int(2),
            FxVec3::new(Fx::from_int(2), Fx::from_int(4), Fx::from_int(6))
        );
        assert_eq!(a.xy(), v(1, 2));
        assert_eq!(FxVec3::default(), FxVec3::ZERO);
    }
}
