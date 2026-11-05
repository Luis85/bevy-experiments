//! The one FNV-1a implementation shared by save checksums, content
//! fingerprints and stable world hashes. Outputs are persisted in saves and
//! pinned by tests, so the constants and byte order must never change.

const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0100_0000_01b3;

/// The empty-input hash, for callers that stream bytes with [`fnv1a_step`].
pub(crate) const fn fnv1a_start() -> u64 {
    OFFSET_BASIS
}

/// One FNV-1a step: fold `byte` into `hash`.
#[inline]
pub(crate) fn fnv1a_step(hash: u64, byte: u8) -> u64 {
    (hash ^ u64::from(byte)).wrapping_mul(PRIME)
}

/// FNV-1a of `bytes`.
pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(OFFSET_BASIS, |hash, byte| fnv1a_step(hash, *byte))
}

/// Counter-based SplitMix avalanche, moved byte-identically from the quality
/// roll. Keyed rolls built on it are persisted through their outcomes, so the
/// constants must never change.
pub(crate) fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E3779B97F4A7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D049BB133111EB);
    value ^ (value >> 31)
}

/// A stateless keyed roll: never draws `Attempt.rng` or the strategy RNG.
/// Presentation must never call it (architecture_tests in horde_game).
pub(crate) fn keyed(seed: u64, a: u64, b: u64, tag: u64) -> u64 {
    mix(seed ^ mix(a) ^ mix(b) ^ tag)
}

/// Purpose tags for [`keyed`]: big-endian ASCII, append-only, never reused.
#[allow(
    dead_code,
    reason = "ELITE..MILESTONE gain their first users in wave 2"
)]
pub(crate) mod tags {
    pub(crate) const QUALITY: u64 = u64::from_be_bytes(*b"QUALITY\x01");
    pub(crate) const ELITE: u64 = u64::from_be_bytes(*b"ELITE\0\0\x01");
    pub(crate) const AFFIX: u64 = u64::from_be_bytes(*b"AFFIX\0\0\x01");
    pub(crate) const ELITE_PICK: u64 = u64::from_be_bytes(*b"ELPICK\0\x01");
    pub(crate) const RECRUIT_OFFER: u64 = u64::from_be_bytes(*b"OFFER\0\0\x01");
    pub(crate) const MILESTONE: u64 = u64::from_be_bytes(*b"MILEST\0\x01");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quality::Quality;

    /// The quality key exactly as quality.rs:112-125 computed it before the
    /// extraction, kept as the frozen reference for the move.
    fn legacy_quality_key(seed: u64, attempt: u64, enemy: u64) -> u64 {
        fn mix(mut value: u64) -> u64 {
            value = value.wrapping_add(0x9E3779B97F4A7C15);
            value = (value ^ (value >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94D049BB133111EB);
            value ^ (value >> 31)
        }
        mix(seed ^ mix(attempt) ^ mix(enemy) ^ 0x5155_414C_4954_5901)
    }

    /// `QUALITY_ROLL_TABLE_FNV` recorded by W1-01 (tests/save_migration.rs)
    /// on the v6 build, over the same 10k tuples.
    const W1_01_QUALITY_ROLL_TABLE_FNV: u64 = 0x1f6b_c282_6f97_e583;

    #[test]
    fn quality_roll_is_bit_identical_after_moving_mix_to_hash() {
        let catalog = crate::quality::catalog();
        let mut tuples = crate::rules::Rng::new(0x5155_414C_5441_424C);
        let mut table = String::new();
        for n in 0..10_000u32 {
            let seed = tuples.next_u64();
            let attempt = tuples.next_u64() % 4096;
            let enemy = tuples.next_u64() % 4096;
            let rank = if n % 100 == 99 { 1000 + n % 7 } else { n % 41 };
            assert_eq!(
                keyed(seed, attempt, enemy, tags::QUALITY),
                legacy_quality_key(seed, attempt, enemy),
                "tuple {n}"
            );
            let quality: Quality = catalog.roll(seed, attempt, enemy, rank);
            table.push_str(quality.name());
            table.push('\n');
        }
        assert_eq!(fnv1a(table.as_bytes()), W1_01_QUALITY_ROLL_TABLE_FNV);
    }

    #[test]
    fn keyed_tags_are_distinct_frozen_ascii() {
        // Characterisation pin: the quality tag is the literal quality.rs used.
        assert_eq!(tags::QUALITY, 0x5155_414C_4954_5901);
        let frozen: [(u64, &[u8; 8]); 6] = [
            (tags::QUALITY, b"QUALITY\x01"),
            (tags::ELITE, b"ELITE\0\0\x01"),
            (tags::AFFIX, b"AFFIX\0\0\x01"),
            (tags::ELITE_PICK, b"ELPICK\0\x01"),
            (tags::RECRUIT_OFFER, b"OFFER\0\0\x01"),
            (tags::MILESTONE, b"MILEST\0\x01"),
        ];
        for (tag, ascii) in frozen {
            assert_eq!(tag, u64::from_be_bytes(*ascii), "{ascii:?}");
        }
        let distinct: std::collections::BTreeSet<u64> = frozen.iter().map(|(t, _)| *t).collect();
        assert_eq!(distinct.len(), frozen.len());
        // Distinct tags give distinct streams for the same key.
        let rolls: std::collections::BTreeSet<u64> =
            frozen.iter().map(|(t, _)| keyed(7, 11, 13, *t)).collect();
        assert_eq!(rolls.len(), frozen.len());
    }
}
