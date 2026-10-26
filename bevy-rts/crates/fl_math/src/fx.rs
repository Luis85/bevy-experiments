//! [`Fx`]: the simulation's fixed-point scalar (signed 32.32).

use core::fmt;
use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use fixed::types::I32F32;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Number of fractional bits in [`Fx`].
const FRAC_BITS: u32 = 32;

/// A signed 32.32 fixed-point number.
///
/// All simulation arithmetic is integer arithmetic on the raw `i64`, so it is
/// bit-identical on every platform. Overflow follows Rust's integer rules
/// (panics in debug builds, wraps in release builds); keep magnitudes well
/// inside ±2^31.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fx(I32F32);

/// Returned by [`Fx::from_f64`] when the value is NaN, infinite, or does not
/// fit in the `Fx` range (|v| ≥ 2^31).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxRangeError {
    /// The rejected value.
    pub value: f64,
}

impl fmt::Display for FxRangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "value {} is not a finite number with magnitude below 2^31, so it cannot be an Fx",
            self.value
        )
    }
}

impl std::error::Error for FxRangeError {}

impl Fx {
    /// 0.
    pub const ZERO: Fx = Fx::from_raw(0);
    /// 1.
    pub const ONE: Fx = Fx::from_raw(1 << FRAC_BITS);
    /// 0.5.
    pub const HALF: Fx = Fx::from_raw(1 << (FRAC_BITS - 1));
    /// Largest value, just below 2^31.
    pub const MAX: Fx = Fx::from_raw(i64::MAX);
    /// Smallest value, -2^31.
    pub const MIN: Fx = Fx::from_raw(i64::MIN);
    /// Smallest positive value, 2^-32 (raw 1).
    pub const EPSILON: Fx = Fx::from_raw(1);

    /// The integer `v`, exactly.
    #[inline]
    pub const fn from_int(v: i32) -> Fx {
        Fx::from_raw((v as i64) << FRAC_BITS)
    }

    /// `num / den`, truncated toward zero to the nearest raw unit.
    ///
    /// Panics if `den` is zero or the result does not fit.
    pub const fn from_ratio(num: i64, den: i64) -> Fx {
        assert!(den != 0, "Fx::from_ratio: zero denominator");
        let q = ((num as i128) << FRAC_BITS) / den as i128;
        assert!(
            q >= i64::MIN as i128 && q <= i64::MAX as i128,
            "Fx::from_ratio: result out of range"
        );
        Fx::from_raw(q as i64)
    }

    /// The value whose raw bits (scaled by 2^32) are `raw`.
    #[inline]
    pub const fn from_raw(raw: i64) -> Fx {
        Fx(I32F32::from_bits(raw))
    }

    /// The raw bits: the value scaled by 2^32.
    #[inline]
    pub const fn raw(self) -> i64 {
        self.0.to_bits()
    }

    /// Converts a float, rounding to the nearest raw unit. **Load time only**:
    /// content files are converted once, never during the simulation.
    ///
    /// Rejects NaN, ±infinity, and any value with |v| ≥ 2^31.
    pub fn from_f64(v: f64) -> Result<Fx, FxRangeError> {
        const LIMIT: f64 = 2_147_483_648.0; // 2^31
        if !v.is_finite() || v.abs() >= LIMIT {
            return Err(FxRangeError { value: v });
        }
        // Cannot overflow after the check above (every f64 below 2^31 with a
        // fraction finer than 2^-32 is far from the limit); checked anyway.
        I32F32::checked_from_num(v)
            .map(Fx)
            .ok_or(FxRangeError { value: v })
    }

    /// Presentation only.
    #[inline]
    pub fn to_f32(self) -> f32 {
        self.0.to_num::<f32>()
    }

    /// Presentation only.
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0.to_num::<f64>()
    }

    /// The largest integer ≤ `self`.
    #[inline]
    pub const fn floor_to_i64(self) -> i64 {
        self.raw() >> FRAC_BITS
    }

    /// |self|. Overflows (like `i64::abs`) for `MIN`.
    #[inline]
    pub const fn abs(self) -> Fx {
        Fx::from_raw(self.raw().abs())
    }

    /// The smaller of `self` and `other`.
    #[inline]
    pub const fn min(self, other: Fx) -> Fx {
        if self.raw() <= other.raw() {
            self
        } else {
            other
        }
    }

    /// The larger of `self` and `other`.
    #[inline]
    pub const fn max(self, other: Fx) -> Fx {
        if self.raw() >= other.raw() {
            self
        } else {
            other
        }
    }

    /// `self` limited to `[lo, hi]`. Panics if `lo > hi`.
    #[inline]
    pub const fn clamp(self, lo: Fx, hi: Fx) -> Fx {
        assert!(lo.raw() <= hi.raw(), "Fx::clamp: lo > hi");
        self.max(lo).min(hi)
    }

    /// Square root, truncated to the raw unit below. Values ≤ 0 give `ZERO`.
    pub const fn sqrt(self) -> Fx {
        if self.raw() <= 0 {
            return Fx::ZERO;
        }
        // sqrt(raw / 2^32) * 2^32 = sqrt(raw * 2^32); at most ~2^47.5.
        Fx::from_raw(u128::isqrt((self.raw() as u128) << FRAC_BITS) as i64)
    }

    /// `self * n`, exactly.
    #[inline]
    pub const fn mul_int(self, n: i32) -> Fx {
        Fx::from_raw(self.raw() * n as i64)
    }
}

impl fmt::Display for Fx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl Add for Fx {
    type Output = Fx;
    #[inline]
    fn add(self, rhs: Fx) -> Fx {
        Fx(self.0 + rhs.0)
    }
}

impl Sub for Fx {
    type Output = Fx;
    #[inline]
    fn sub(self, rhs: Fx) -> Fx {
        Fx(self.0 - rhs.0)
    }
}

impl Mul for Fx {
    type Output = Fx;
    #[inline]
    fn mul(self, rhs: Fx) -> Fx {
        Fx(self.0 * rhs.0)
    }
}

impl Div for Fx {
    type Output = Fx;
    #[inline]
    fn div(self, rhs: Fx) -> Fx {
        Fx(self.0 / rhs.0)
    }
}

impl Neg for Fx {
    type Output = Fx;
    #[inline]
    fn neg(self) -> Fx {
        Fx(-self.0)
    }
}

impl AddAssign for Fx {
    #[inline]
    fn add_assign(&mut self, rhs: Fx) {
        *self = *self + rhs;
    }
}

impl SubAssign for Fx {
    #[inline]
    fn sub_assign(&mut self, rhs: Fx) {
        *self = *self - rhs;
    }
}

impl MulAssign for Fx {
    #[inline]
    fn mul_assign(&mut self, rhs: Fx) {
        *self = *self * rhs;
    }
}

impl DivAssign for Fx {
    #[inline]
    fn div_assign(&mut self, rhs: Fx) {
        *self = *self / rhs;
    }
}

/// Human-readable formats (RON, JSON) get an `f64`: exact while |v| < 2^21;
/// larger values with a fine fractional part lose their lowest bits (f64 has
/// a 53-bit mantissa). Binary formats (postcard) get the raw `i64`, which is
/// lossless; commands and snapshots rely on that.
impl Serialize for Fx {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_f64(self.to_f64())
        } else {
            serializer.serialize_i64(self.raw())
        }
    }
}

/// Human-readable formats accept integers and floats, and every value goes
/// through [`Fx::from_f64`]. Binary formats read the raw `i64`.
impl<'de> Deserialize<'de> for Fx {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Fx, D::Error> {
        if deserializer.is_human_readable() {
            deserializer.deserialize_f64(FxVisitor)
        } else {
            i64::deserialize(deserializer).map(Fx::from_raw)
        }
    }
}

struct FxVisitor;

impl FxVisitor {
    fn convert<E: de::Error>(v: f64) -> Result<Fx, E> {
        Fx::from_f64(v).map_err(E::custom)
    }
}

impl Visitor<'_> for FxVisitor {
    type Value = Fx;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a number with magnitude below 2^31")
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Fx, E> {
        Self::convert(v)
    }

    // Integers convert to f64 exactly for every in-range value; huge ones
    // round, but they are rejected anyway.
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Fx, E> {
        Self::convert(v as f64)
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Fx, E> {
        Self::convert(v as f64)
    }

    fn visit_i128<E: de::Error>(self, v: i128) -> Result<Fx, E> {
        Self::convert(v as f64)
    }

    fn visit_u128<E: de::Error>(self, v: u128) -> Result<Fx, E> {
        Self::convert(v as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fx_rejects_out_of_range() {
        assert!(Fx::from_f64(f64::NAN).is_err());
        assert!(Fx::from_f64(1e40).is_err());
        assert!(ron::from_str::<Fx>("1e40").is_err());
        assert_eq!(ron::from_str::<Fx>("40").unwrap(), Fx::from_int(40));
    }

    #[test]
    fn sqrt_exact_squares() {
        assert_eq!(Fx::from_int(144).sqrt(), Fx::from_int(12));
        assert_eq!(Fx::from_int(-4).sqrt(), Fx::ZERO);
    }

    #[test]
    fn from_f64_range_edges() {
        assert!(Fx::from_f64(f64::INFINITY).is_err());
        assert!(Fx::from_f64(f64::NEG_INFINITY).is_err());
        assert!(Fx::from_f64(2147483648.0).is_err());
        assert!(Fx::from_f64(-2147483648.0).is_err());
        // The largest f64 below 2^31 is 2^31 - 2^-22, which Q32 holds exactly.
        assert_eq!(
            Fx::from_f64(2147483647.9999998).unwrap().raw(),
            i64::MAX - 1023
        );
        assert_eq!(Fx::from_f64(2147483647.0).unwrap(), Fx::from_int(i32::MAX));
        assert_eq!(Fx::from_f64(-2.5).unwrap(), Fx::from_ratio(-5, 2));
        assert_eq!(Fx::from_f64(0.5).unwrap(), Fx::HALF);
    }

    #[test]
    fn range_error_names_value() {
        let e = ron::from_str::<Fx>("1e40").unwrap_err().to_string();
        assert!(
            e.contains("10000000000000000000000000000000000000000"),
            "{e}"
        );
        let e = ron::from_str::<Fx>("-3000000000").unwrap_err().to_string();
        assert!(e.contains("-3000000000"), "{e}");
        assert!(
            Fx::from_f64(f64::NAN)
                .unwrap_err()
                .to_string()
                .contains("NaN")
        );
    }

    #[test]
    fn ron_accepts_ints_and_floats() {
        assert_eq!(ron::from_str::<Fx>("-7").unwrap(), Fx::from_int(-7));
        assert_eq!(ron::from_str::<Fx>("1.25").unwrap(), Fx::from_ratio(5, 4));
        assert_eq!(ron::from_str::<Fx>("-0.5").unwrap(), -Fx::HALF);
        assert_eq!(ron::to_string(&Fx::from_ratio(5, 4)).unwrap(), "1.25");
        let x = Fx::from_ratio(-123_456, 1000);
        let back: Fx = ron::from_str(&ron::to_string(&x).unwrap()).unwrap();
        assert_eq!(back, x);
    }

    #[test]
    fn postcard_roundtrip_is_bit_exact() {
        for raw in [
            0x7FFF_FFFF_FFFF_FFF1,
            -0x1234_5678_9ABC_DEF1,
            i64::MIN,
            i64::MAX,
            1,
            -1,
            0,
        ] {
            let x = Fx::from_raw(raw);
            let bytes = postcard::to_allocvec(&x).unwrap();
            let back: Fx = postcard::from_bytes(&bytes).unwrap();
            assert_eq!(back.raw(), raw);
        }
        // Binary form is the raw i64 (zigzag varint), not an f64.
        let one = postcard::to_allocvec(&Fx::ONE).unwrap();
        assert_eq!(one, postcard::to_allocvec(&(1i64 << 32)).unwrap());
    }

    #[test]
    fn consts_and_raw() {
        assert_eq!(Fx::ONE.raw(), 1 << 32);
        assert_eq!(Fx::HALF.raw(), 1 << 31);
        assert_eq!(Fx::EPSILON.raw(), 1);
        assert_eq!(Fx::ZERO.raw(), 0);
        assert_eq!(Fx::MAX.raw(), i64::MAX);
        assert_eq!(Fx::MIN.raw(), i64::MIN);
        assert_eq!(Fx::from_raw(12345).raw(), 12345);
        assert_eq!(Fx::default(), Fx::ZERO);
    }

    #[test]
    fn from_ratio_and_floor() {
        assert_eq!(Fx::from_ratio(1, 2), Fx::HALF);
        assert_eq!(Fx::from_ratio(6, 3), Fx::from_int(2));
        assert_eq!(Fx::from_ratio(3, -2), Fx::from_ratio(-3, 2));
        assert_eq!(Fx::from_ratio(7, 2).floor_to_i64(), 3);
        assert_eq!(Fx::from_ratio(-7, 2).floor_to_i64(), -4);
        assert_eq!(Fx::from_int(-3).floor_to_i64(), -3);
    }

    #[test]
    fn arithmetic() {
        let a = Fx::from_int(6);
        let b = Fx::from_int(4);
        assert_eq!(a + b, Fx::from_int(10));
        assert_eq!(a - b, Fx::from_int(2));
        assert_eq!(a * b, Fx::from_int(24));
        assert_eq!(a / b, Fx::from_ratio(3, 2));
        assert_eq!(-a, Fx::from_int(-6));
        assert_eq!(a.mul_int(-3), Fx::from_int(-18));
        let mut c = a;
        c += b;
        c -= Fx::ONE;
        c *= Fx::HALF;
        c /= Fx::from_int(3);
        assert_eq!(c, Fx::from_int(3) / Fx::from_int(2));
    }

    #[test]
    fn helpers() {
        let a = Fx::from_int(-5);
        assert_eq!(a.abs(), Fx::from_int(5));
        assert_eq!(a.min(Fx::ONE), a);
        assert_eq!(a.max(Fx::ONE), Fx::ONE);
        assert_eq!(a.clamp(Fx::ZERO, Fx::ONE), Fx::ZERO);
        assert_eq!(Fx::from_int(9).clamp(Fx::ZERO, Fx::ONE), Fx::ONE);
        assert_eq!(Fx::HALF.clamp(Fx::ZERO, Fx::ONE), Fx::HALF);
        assert_eq!(Fx::from_int(2).sqrt().raw(), 6_074_000_999); // floor(sqrt(2) * 2^32)
        assert_eq!(Fx::ZERO.sqrt(), Fx::ZERO);
        assert_eq!(Fx::MAX.sqrt().raw(), 199_032_864_766_430); // floor(sqrt(2^63 - 1) * 2^16)
    }

    #[test]
    fn display_prints_decimal() {
        assert_eq!(Fx::from_ratio(5, 4).to_string(), "1.25");
        assert_eq!(Fx::from_int(-3).to_string(), "-3");
        assert_eq!(Fx::from_int(-3).to_f64(), -3.0);
        assert_eq!(Fx::HALF.to_f32(), 0.5);
    }
}
