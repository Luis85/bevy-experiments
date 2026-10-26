//! Save fitness checks: the byte-identical round trip and the branch test (spec §5.8, §12.3,
//! ADR-0006).

use fl_sim::compute_state_hash;

use crate::builder::{Scenario, ScenarioBuilder};
use crate::determinism::divergence;

/// Where two saves first differ, for a failure message.
fn first_difference(a: &[u8], b: &[u8]) -> String {
    let at = a
        .iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()));
    format!(
        "{} vs {} bytes, first difference at byte {at}",
        a.len(),
        b.len()
    )
}

/// Save `s`, load the save into a fresh sim (same features, content and run config) and save
/// that: the two saves must be the same bytes, and the loaded sim must be at the same tick
/// with the same state hash.
///
/// # Panics
/// If the saves differ, or the save does not load.
pub fn assert_save_round_trip(s: &mut Scenario) {
    let first = s.save();
    let mut loaded = s.reload(&first);
    assert_eq!(
        loaded.tick(),
        s.tick(),
        "the loaded sim resumes at the saved tick"
    );
    let second = loaded.save();
    assert!(
        first == second,
        "save(load(save)) is not save at tick {}: {}",
        s.tick().0,
        first_difference(&first, &second)
    );
    assert_eq!(
        compute_state_hash(loaded.world()),
        compute_state_hash(s.world()),
        "the loaded sim's state hash differs at tick {}",
        s.tick().0
    );
}

/// The branch test: run A from `make` to tick `m`; run B from `make` to tick `n`, save it,
/// load the save and run the loaded sim on to `m` (issuing B's remaining builder commands as
/// their ticks come up). The hashes of ticks `n..m` must be A's, and A and the loaded sim
/// must save the same bytes at `m`.
///
/// # Panics
/// `"branch at tick n: diverged at tick T: a=0x… b=0x…"` (a is the unbroken run); also if
/// `n > m`, if A took no hash in `n..m`, or if the saves at `m` differ.
pub fn assert_branch(make: impl Fn() -> ScenarioBuilder, n: u32, m: u32) {
    assert!(n <= m, "the branch point {n} is after the end {m}");
    let mut a = make().build();
    a.run_ticks(m);
    let expected: Vec<_> = a
        .hashes()
        .into_iter()
        .filter(|(tick, _)| tick.0 >= n)
        .collect();

    let mut b = make().build();
    b.run_ticks(n);
    let bytes = b.save();
    let mut resumed = b.reload(&bytes);
    resumed.scheduled = std::mem::take(&mut b.scheduled);
    drop(b);
    resumed.run_ticks(m - n);

    if let Some(problem) = divergence(&expected, &resumed.hashes()) {
        panic!("branch at tick {n}: {problem}");
    }
    let (end_a, end_b) = (a.save(), resumed.save());
    assert!(
        end_a == end_b,
        "branch at tick {n}: the saves at tick {m} differ: {}",
        first_difference(&end_a, &end_b)
    );
}
