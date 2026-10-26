//! Property tests for `fl_math`. f64 is used here only as a reference.

use fl_math::{Angle, Fx, FxVec2};
use proptest::prelude::*;

const ONE_RAW: i64 = 1 << 32;
const TWO_POW_32: f64 = 4_294_967_296.0;

proptest! {
    #[test]
    fn add_sub_roundtrip(a in -(1i64 << 61)..(1i64 << 61), b in -(1i64 << 61)..(1i64 << 61)) {
        let (a, b) = (Fx::from_raw(a), Fx::from_raw(b));
        prop_assert_eq!((a + b) - b, a);
        prop_assert_eq!((a - b) + b, a);
    }

    #[test]
    fn mul_matches_f64_within_2eps(
        a in -40_000 * ONE_RAW..=40_000 * ONE_RAW,
        b in -40_000 * ONE_RAW..=40_000 * ONE_RAW,
    ) {
        let (fa, fb) = (Fx::from_raw(a), Fx::from_raw(b));
        let got = fa * fb;
        // Operands this size convert to f64 exactly, but the f64 product of
        // two ~47-bit mantissas is itself rounded (by up to ~1e-7 near 1.6e9),
        // so the tolerance is 2 eps plus f64's own rounding of the reference.
        let expected = fa.to_f64() * fb.to_f64();
        let tol = 2.0 / TWO_POW_32 + expected.abs() * f64::EPSILON * 2.0;
        prop_assert!(
            (got.to_f64() - expected).abs() <= tol,
            "{} * {} = {} vs {}", fa, fb, got, expected
        );
        // Exact integer check: at most 2 eps from the true product.
        let exact = a as i128 * b as i128; // true product scaled by 2^64
        let err = got.raw() as i128 * ONE_RAW as i128 - exact;
        prop_assert!(err.abs() <= 2 * ONE_RAW as i128, "raw error {}", err);
    }

    #[test]
    fn vec_length_matches_f64(
        x in -2_000 * ONE_RAW..=2_000 * ONE_RAW,
        y in -2_000 * ONE_RAW..=2_000 * ONE_RAW,
    ) {
        let v = FxVec2::new(Fx::from_raw(x), Fx::from_raw(y));
        let expected = v.x.to_f64().hypot(v.y.to_f64());
        // Below ~1e-3 the 2^-32 resolution alone exceeds 1e-6 relative error.
        prop_assume!(expected > 1e-3);
        let rel = (v.length().to_f64() - expected).abs() / expected;
        prop_assert!(rel < 1e-6, "{:?}: rel error {}", v, rel);
    }

    #[test]
    fn angle_from_vec_roundtrip(r in any::<u16>()) {
        let a = Angle::from_raw(r);
        let back = FxVec2::from_angle(a).angle();
        prop_assert!(a.signed_diff(back).abs() <= 4, "{:?} -> {:?}", a, back);
    }
}
