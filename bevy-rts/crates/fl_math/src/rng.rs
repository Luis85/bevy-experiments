//! Keyed counter-based random numbers ([`roll`], [`Rolls`], [`stream_key`])
//! and the [`Pcg32`] generator for tools.

use serde::{Deserialize, Serialize};

use crate::Fx;

const PCG_MULT: u64 = 6_364_136_223_846_793_005;

/// PCG-XSH-RR 64/32 (O'Neill's `pcg32`), seeded exactly like
/// `pcg32_srandom_r`, so it reproduces the reference implementation's output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    /// Seeds like `pcg32_srandom_r(seed, stream)`. Streams that differ only
    /// in the top bit are the same stream (the reference behaves the same).
    pub fn new(seed: u64, stream: u64) -> Pcg32 {
        let mut rng = Pcg32 {
            state: 0,
            inc: (stream << 1) | 1,
        };
        rng.step();
        rng.state = rng.state.wrapping_add(seed);
        rng.step();
        rng
    }

    #[inline]
    fn step(&mut self) {
        self.state = self.state.wrapping_mul(PCG_MULT).wrapping_add(self.inc);
    }

    /// The next 32 random bits.
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.step();
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// The next 64 random bits: two draws, the first one in the high half.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let hi = self.next_u32() as u64;
        let lo = self.next_u32() as u64;
        (hi << 32) | lo
    }

    /// Uniform in `[lo, hi_excl)`, without bias (Lemire's multiply-and-reject).
    /// Panics if `lo >= hi_excl`.
    pub fn range_u32(&mut self, lo: u32, hi_excl: u32) -> u32 {
        assert!(
            lo < hi_excl,
            "Pcg32::range_u32: empty range {lo}..{hi_excl}"
        );
        let range = hi_excl - lo;
        let mut m = self.next_u32() as u64 * range as u64;
        if (m as u32) < range {
            // 2^32 mod range: the low products below this are the biased ones.
            let threshold = range.wrapping_neg() % range;
            while (m as u32) < threshold {
                m = self.next_u32() as u64 * range as u64;
            }
        }
        lo + (m >> 32) as u32
    }

    /// True with probability `num / den` (always true once `num >= den`).
    /// Consumes one draw (more on rare rejections). Panics if `den == 0`.
    #[inline]
    pub fn chance(&mut self, num: u32, den: u32) -> bool {
        self.range_u32(0, den) < num
    }

    /// Uniform in `[0, 1)`, with 32 random fractional bits.
    #[inline]
    pub fn unit_fx(&mut self) -> Fx {
        Fx::from_raw(self.next_u32() as i64)
    }

    /// In `[lo, hi)` (or exactly `lo` when `lo == hi`), from one draw:
    /// `lo + (hi - lo) · unit`, rounded down. Panics if `lo > hi`.
    pub fn range_fx(&mut self, lo: Fx, hi: Fx) -> Fx {
        assert!(lo <= hi, "Pcg32::range_fx: lo > hi");
        let span = hi.raw() as i128 - lo.raw() as i128; // up to 2^64 - 1
        let offset = (span * self.next_u32() as i128) >> 32; // < span
        Fx::from_raw((lo.raw() as i128 + offset) as i64)
    }

    /// The raw `(state, inc)` pair, for hashing and debugging.
    #[inline]
    pub fn state(&self) -> (u64, u64) {
        (self.state, self.inc)
    }
}

/// splitmix64's output function (Steele, Lea & Flood), applied to `z`.
#[inline]
const fn splitmix64(z: u64) -> u64 {
    let mut z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The key of a named random stream: `splitmix64(fnv1a64(name))`. The output
/// is part of the determinism contract (pinned by reference vectors).
pub const fn stream_key(name: &str) -> u64 {
    let bytes = name.as_bytes();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut i = 0;
    while i < bytes.len() {
        h ^= bytes[i] as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
        i += 1;
    }
    splitmix64(h)
}

/// The `n`-th random word of `stream` for entity `sim_id` at `tick` of the
/// match seeded with `seed` (spec 5.4). A pure function: no state is kept anywhere.
///
/// Every input is folded in with splitmix64 one word at a time, so each one
/// affects every output bit, and swapping `tick` and `sim_id` changes the
/// result. The output is part of the determinism contract.
pub fn roll(seed: u64, tick: u32, sim_id: u32, stream: u64, n: u32) -> u64 {
    let mut h = splitmix64(seed);
    h = splitmix64(h ^ (((tick as u64) << 32) | sim_id as u64));
    h = splitmix64(h ^ stream);
    splitmix64(h ^ n as u64)
}

/// A cursor over the draws `n = 0, 1, 2, ...` of one `(seed, tick, sim_id,
/// stream)` key. It is a short-lived local value, never stored in sim state.
#[derive(Clone, Debug)]
pub struct Rolls {
    seed: u64,
    tick: u32,
    sim_id: u32,
    stream: u64,
    n: u32,
}

impl Rolls {
    /// A cursor at draw 0.
    pub const fn new(seed: u64, tick: u32, sim_id: u32, stream: u64) -> Rolls {
        Rolls {
            seed,
            tick,
            sim_id,
            stream,
            n: 0,
        }
    }

    /// The next 64 random bits: `roll(.., n)`, then `n += 1`.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let v = roll(self.seed, self.tick, self.sim_id, self.stream, self.n);
        self.n += 1;
        v
    }

    /// The high 32 bits of [`Rolls::next_u64`].
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform in `[lo, hi_excl)`, without bias (Lemire's multiply-and-reject,
    /// as [`Pcg32::range_u32`]). Each rejection consumes a draw.
    /// Panics if `lo >= hi_excl`.
    pub fn range_u32(&mut self, lo: u32, hi_excl: u32) -> u32 {
        assert!(
            lo < hi_excl,
            "Rolls::range_u32: empty range {lo}..{hi_excl}"
        );
        let range = hi_excl - lo;
        let mut m = self.next_u32() as u64 * range as u64;
        if (m as u32) < range {
            let threshold = range.wrapping_neg() % range;
            while (m as u32) < threshold {
                m = self.next_u32() as u64 * range as u64;
            }
        }
        lo + (m >> 32) as u32
    }

    /// True with probability `num / den` (always true once `num >= den`).
    /// Panics if `den == 0`.
    #[inline]
    pub fn chance(&mut self, num: u32, den: u32) -> bool {
        self.range_u32(0, den) < num
    }

    /// Uniform in `[0, 1)`, with 32 random fractional bits.
    #[inline]
    pub fn unit_fx(&mut self) -> Fx {
        Fx::from_raw(self.next_u32() as i64)
    }

    /// In `[lo, hi)` (or exactly `lo` when `lo == hi`), from one draw:
    /// `lo + (hi - lo) · unit`, rounded down. Panics if `lo > hi`.
    pub fn range_fx(&mut self, lo: Fx, hi: Fx) -> Fx {
        assert!(lo <= hi, "Rolls::range_fx: lo > hi");
        let span = hi.raw() as i128 - lo.raw() as i128;
        let offset = (span * self.next_u32() as i128) >> 32;
        Fx::from_raw((lo.raw() as i128 + offset) as i64)
    }

    /// The number of draws made so far (the current `n`).
    #[inline]
    pub fn draws(&self) -> u32 {
        self.n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcg32_reference_vector() {
        // pcg32-demo, seed 42 stream 54
        let mut r = Pcg32::new(42, 54);
        let got: Vec<u32> = (0..6).map(|_| r.next_u32()).collect();
        assert_eq!(
            got,
            [
                0xa15c02b7, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e
            ]
        );
    }

    #[test]
    fn unit_fx_in_range() {
        let mut r = Pcg32::new(1, 1);
        for _ in 0..10_000 {
            let u = r.unit_fx();
            assert!(u >= Fx::ZERO && u < Fx::ONE);
        }
    }

    #[test]
    fn next_u64_is_two_draws_high_first() {
        let mut a = Pcg32::new(42, 54);
        let mut b = Pcg32::new(42, 54);
        let hi = b.next_u32() as u64;
        let lo = b.next_u32() as u64;
        assert_eq!(a.next_u64(), (hi << 32) | lo);
        assert_eq!(a, b);
    }

    #[test]
    fn range_u32_bounds_and_coverage() {
        let mut r = Pcg32::new(3, 7);
        let mut seen = [0u32; 7];
        for _ in 0..7_000 {
            let v = r.range_u32(10, 17);
            assert!((10..17).contains(&v));
            seen[(v - 10) as usize] += 1;
        }
        assert!(seen.iter().all(|&n| n > 800), "{seen:?}");
        assert_eq!(r.range_u32(5, 6), 5);
        // A near-full-width range must not hang or overflow.
        let v = r.range_u32(0, u32::MAX);
        assert!(v < u32::MAX);
    }

    #[test]
    fn chance_edges() {
        let mut r = Pcg32::new(11, 0);
        for _ in 0..1000 {
            assert!(!r.chance(0, 10));
            assert!(r.chance(10, 10));
        }
        let hits = (0..10_000).filter(|_| r.chance(1, 4)).count();
        assert!((2_200..2_800).contains(&hits), "{hits}");
    }

    #[test]
    fn range_fx_bounds() {
        let mut r = Pcg32::new(5, 5);
        let lo = Fx::from_int(-3);
        let hi = Fx::from_ratio(7, 2);
        for _ in 0..10_000 {
            let v = r.range_fx(lo, hi);
            assert!(v >= lo && v < hi, "{v}");
        }
        assert_eq!(r.range_fx(lo, lo), lo);
    }

    #[test]
    fn state_and_serde_roundtrip() {
        let mut r = Pcg32::new(42, 54);
        r.next_u32();
        let (s, inc) = r.state();
        assert_eq!(inc, (54 << 1) | 1);
        let text = ron::to_string(&r).unwrap();
        let mut back: Pcg32 = ron::from_str(&text).unwrap();
        assert_eq!(back.state(), (s, inc));
        assert_eq!(back.next_u32(), 0x7b47f409);
    }
}
