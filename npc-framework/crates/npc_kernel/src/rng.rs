//! Counter-based randomness: a draw depends only on (seed, who, when, why),
//! never on how many draws happened before or in what order systems ran.

use bevy_ecs::prelude::*;

use crate::ids::SimId;

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorldSeed(pub u64);

/// The SplitMix64 finaliser (Steele, Lea, Flood 2014). Pure, platform independent.
pub const fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// SplitMix64 finaliser over a combination of the inputs:
/// `splitmix64(seed ^ splitmix64(id ^ splitmix64(tick ^ splitmix64(purpose))))`.
/// This exact nesting is part of the determinism contract; changing it changes every draw.
pub fn sim_rand(seed: u64, id: SimId, tick: u64, purpose: u32) -> u64 {
    splitmix64(seed ^ splitmix64(id.0 as u64 ^ splitmix64(tick ^ splitmix64(purpose as u64))))
}

/// Uniform-ish integer in `lo..hi` (hi exclusive, hi > lo) from `sim_rand`:
/// `lo + (r % (hi - lo))`. Modulo bias is negligible for small spans.
///
/// # Panics
/// When `hi <= lo`.
pub fn sim_range(seed: u64, id: SimId, tick: u64, purpose: u32, lo: i32, hi: i32) -> i32 {
    assert!(hi > lo, "sim_range: empty range {lo}..{hi}");
    let span = (hi as i64 - lo as i64) as u64;
    let r = sim_rand(seed, id, tick, purpose);
    (lo as i64 + (r % span) as i64) as i32
}
