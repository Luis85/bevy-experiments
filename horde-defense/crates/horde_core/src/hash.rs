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
