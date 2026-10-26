//! Table-driven trigonometry on binary angles (65536 units per turn).
//!
//! Everything here is pure integer arithmetic so the results are
//! bit-identical on every platform. No floating point is used, not even to
//! build the tables.

use std::sync::LazyLock;

/// π in Q62 fixed point (π · 2^62, rounded to nearest).
pub(crate) const PI_Q62: i128 = 14_488_038_916_154_245_685;

/// Entries in the quarter-wave table: angles 0..=QUARTER inclusive.
const QUARTER_LEN: usize = 16_385;

/// `sin` over the first quadrant, as raw `Fx` values (Q32), indexed by binary
/// angle 0..=16384. Built once, on first use.
pub(crate) static QUARTER_SINE: LazyLock<Vec<i64>> = LazyLock::new(build_quarter_sine);

/// `atan(2^-i)` in binary-angle units, rounded, for the CORDIC iterations.
pub(crate) const ATAN_TABLE: [i64; 16] = [
    8192, 4836, 2555, 1297, 651, 326, 163, 81, 41, 20, 10, 5, 3, 1, 1, 0,
];

/// Fixed-point fraction bits used while building the sine table.
const Q: u32 = 62;

/// sin(x) for x in Q62, 0 ≤ x ≤ π/2, by Taylor series in i128.
///
/// Terms are added until they vanish at Q62 resolution; the accumulated
/// truncation error is a few units of 2^-62, far below the Q32 output step.
fn sin_q62(x: i128) -> i128 {
    let x2 = (x * x) >> Q;
    let mut term = x;
    let mut sum = x;
    let mut k: i128 = 1;
    loop {
        // term_k = -term_{k-1} · x² / ((2k)(2k+1))
        term = -((term * x2) >> Q) / ((2 * k) * (2 * k + 1));
        if term == 0 {
            return sum;
        }
        sum += term;
        k += 1;
    }
}

fn build_quarter_sine() -> Vec<i64> {
    (0..QUARTER_LEN as i128)
        .map(|i| {
            // x = i · (π/2) / 16384 = i · π / 32768, rounded to nearest in Q62.
            let x = (i * PI_Q62 + (1 << 14)) >> 15;
            // Q62 → Q32, rounded to nearest.
            ((sin_q62(x) + (1 << 29)) >> 30) as i64
        })
        .collect()
}

/// sin of a binary angle, as a raw `Fx` value.
pub(crate) fn sin_raw(a: u16) -> i64 {
    let table = &*QUARTER_SINE;
    let i = (a & 0x3FFF) as usize;
    match a >> 14 {
        0 => table[i],
        1 => table[16_384 - i],
        2 => -table[i],
        _ => -table[16_384 - i],
    }
}

/// atan2 of raw `Fx` values, as a binary angle. `atan2(0, 0)` is 0.
///
/// CORDIC in vectoring mode on i64: the vector is scaled so its larger
/// component sits in [2^59, 2^60) (no overflow, full precision for tiny
/// inputs), reflected into the right half-plane, then rotated toward the
/// +x axis 16 times while the rotations are summed.
pub(crate) fn atan2_raw(y: i64, x: i64) -> u16 {
    // The axes are answered exactly.
    if y == 0 {
        return if x < 0 { 32_768 } else { 0 };
    }
    if x == 0 {
        return if y > 0 { 16_384 } else { 49_152 };
    }

    let m = x.unsigned_abs().max(y.unsigned_abs());
    // m's top bit is at 63 - lz; move it to bit 59.
    let shift = m.leading_zeros() as i32 - 4;
    let (mut x, mut y) = if shift >= 0 {
        (x << shift, y << shift)
    } else {
        (x >> -shift, y >> -shift)
    };

    let mut z: i64 = 0;
    if x < 0 {
        x = -x;
        y = -y;
        z = 32_768;
    }
    for (i, &step) in ATAN_TABLE.iter().enumerate() {
        let (dx, dy) = (x >> i, y >> i);
        if y > 0 {
            x += dy;
            y -= dx;
            z += step;
        } else {
            x -= dy;
            y += dx;
            z -= step;
        }
    }
    z.rem_euclid(65_536) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_constant_matches_f64() {
        let pi = PI_Q62 as f64 / (1u64 << 62) as f64;
        assert_eq!(pi, std::f64::consts::PI);
    }

    #[test]
    fn quarter_table_endpoints() {
        let t = &*QUARTER_SINE;
        assert_eq!(t.len(), 16_385);
        assert_eq!(t[0], 0);
        assert_eq!(t[16_384], 1 << 32);
        assert!(t.windows(2).all(|w| w[0] <= w[1]));
    }

    /// Replay-relevant bits: these are round(sin(i·π/32768) · 2^32), checked
    /// against an independent computation. A change here changes every replay.
    #[test]
    fn quarter_table_pinned_bits() {
        let t = &*QUARTER_SINE;
        assert_eq!(t[1], 411_775);
        assert_eq!(t[8_192], 3_037_000_500);
        assert_eq!(t[16_383], 4_294_967_276);
    }

    #[test]
    fn cordic_table_matches_f64() {
        for (i, &a) in ATAN_TABLE.iter().enumerate() {
            let e = (2f64.powi(-(i as i32))).atan() * 32768.0 / std::f64::consts::PI;
            assert_eq!(a, e.round() as i64, "{i}");
        }
    }
}
