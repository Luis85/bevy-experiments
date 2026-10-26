//! Determinism checks: twin runs and schedule-shuffle runs (spec §12.3, ADR-0005).

use fl_protocol::Tick;

use crate::builder::ScenarioBuilder;

/// The schedule-shuffle seeds, in the order [`shuffle_seeds`] takes them.
const SHUFFLE_SEEDS: [u64; 3] = [0x5EED_0001, 0x5EED_0002, 0x5EED_0003];

/// Build `builder` and return the state hashes of its first `ticks` ticks.
fn hash_stream(builder: ScenarioBuilder, ticks: u32) -> Vec<(Tick, u64)> {
    let mut scenario = builder.build();
    scenario.run_ticks(ticks);
    scenario.hashes()
}

/// Where hash stream `b` first departs from `a`: `"diverged at tick T: a=0x… b=0x…"`, or a
/// note that one stream hashed other ticks or ended early.
///
/// # Panics
/// If `a` is empty: with hashing off there is nothing to compare.
pub(crate) fn divergence(a: &[(Tick, u64)], b: &[(Tick, u64)]) -> Option<String> {
    assert!(
        !a.is_empty(),
        "no state hashes to compare: the hash interval is 0 or no tick ran"
    );
    for (&(tick_a, hash_a), &(tick_b, hash_b)) in a.iter().zip(b) {
        if tick_a != tick_b {
            return Some(format!(
                "hashed different ticks: a hashed tick {}, b tick {}",
                tick_a.0, tick_b.0
            ));
        }
        if hash_a != hash_b {
            return Some(format!(
                "diverged at tick {}: a=0x{hash_a:016x} b=0x{hash_b:016x}",
                tick_a.0
            ));
        }
    }
    (a.len() != b.len()).then(|| {
        format!(
            "hash streams differ in length: a has {} hashes, b {}",
            a.len(),
            b.len()
        )
    })
}

/// Build two scenarios with `make` and run each for `ticks` ticks: their hash streams must be
/// equal.
///
/// # Panics
/// `"diverged at tick T: a=0x… b=0x…"` at the first tick whose hashes differ; also if the
/// streams hash different ticks or no hash was taken.
pub fn assert_deterministic(make: impl Fn() -> ScenarioBuilder, ticks: u32) {
    let a = hash_stream(make(), ticks);
    let b = hash_stream(make(), ticks);
    if let Some(problem) = divergence(&a, &b) {
        panic!("twin run of {ticks} ticks: {problem}");
    }
}

/// The schedule-shuffle seeds to run: the first `k` of `[0x5EED_0001, 0x5EED_0002,
/// 0x5EED_0003]`, where `k` is the environment variable `FL_SHUFFLE_SEEDS` (default 1).
///
/// # Panics
/// If `FL_SHUFFLE_SEEDS` is not a number from 1 to 3: a shuffle check always runs at
/// least one shuffled schedule.
pub fn shuffle_seeds() -> Vec<u64> {
    let count = match std::env::var("FL_SHUFFLE_SEEDS") {
        Ok(text) => text
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|k| (1..=SHUFFLE_SEEDS.len()).contains(k))
            .unwrap_or_else(|| {
                panic!("FL_SHUFFLE_SEEDS must be 1, 2 or 3 (shuffled runs per check), not '{text}'")
            }),
        Err(_) => 1,
    };
    SHUFFLE_SEEDS[..count].to_vec()
}

/// Run `make` for `ticks` ticks unshuffled, then once per [`shuffle_seeds`] seed with the
/// order of unordered `SimTick` systems shuffled by it: every shuffled run must give the
/// unshuffled hash stream.
///
/// # Panics
/// `"shuffle seed 0x…: diverged at tick T: a=0x… b=0x…"` (a is the unshuffled run).
pub fn assert_shuffle_invariant(make: impl Fn() -> ScenarioBuilder, ticks: u32) {
    let plain = hash_stream(make().shuffle_seed(None), ticks);
    for seed in shuffle_seeds() {
        let shuffled = hash_stream(make().shuffle_seed(Some(seed)), ticks);
        if let Some(problem) = divergence(&plain, &shuffled) {
            panic!("shuffle seed 0x{seed:x}: {problem}");
        }
    }
}
