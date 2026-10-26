//! The world snapshot invariant (ADR 0013): saving at tick T, loading into a
//! fresh app and running on gives the same state hashes as running without
//! a save. Content tests call [`assert_resume_equivalent`] with their app
//! factory; a mismatch names the first differing tick and entity diffs.

use bevy_app::App;
use npc_kernel::hash::state_hash;
use npc_kernel::replay::entity_diffs;
use npc_kernel::run_ticks;
use npc_kernel::snapshot::{LoadMode, WorldSnapshot, load, save};

/// The three runs [`check_resume_equivalent`] compares.
struct Runs {
    /// Never saved: runs straight through.
    never: App,
    /// Saved at `at` (proves saving does not change sim results).
    saved: App,
    /// A fresh app with the snapshot loaded.
    loaded: App,
}

fn hashes(runs: &mut Runs) -> (u64, u64, u64) {
    (
        state_hash(runs.never.world_mut()),
        state_hash(runs.saved.world_mut()),
        state_hash(runs.loaded.world_mut()),
    )
}

fn step(runs: &mut Runs) {
    run_ticks(runs.never.world_mut(), 1);
    run_ticks(runs.saved.world_mut(), 1);
    run_ticks(runs.loaded.world_mut(), 1);
}

/// Builds the three runs at tick `at`. The snapshot goes through a JSON
/// string, as a save file does.
fn start(make: &impl Fn() -> App, at: u64) -> Result<Runs, String> {
    let mut never = make();
    run_ticks(never.world_mut(), at);
    let mut saved = make();
    run_ticks(saved.world_mut(), at);
    let snap = save(saved.world_mut())?;
    let text = serde_json::to_string(&snap).map_err(|e| format!("snapshot to JSON: {e}"))?;
    let snap: WorldSnapshot =
        serde_json::from_str(&text).map_err(|e| format!("snapshot from JSON: {e}"))?;
    let mut loaded = make();
    // Finish the build like the other two runs (content frozen).
    run_ticks(loaded.world_mut(), 0);
    let report =
        load(loaded.world_mut(), &snap, LoadMode::Strict).map_err(|e| format!("load: {e}"))?;
    if !report.skipped.is_empty() {
        return Err(format!("load skipped {:?}", report.skipped));
    }
    Ok(Runs {
        never,
        saved,
        loaded,
    })
}

/// The entity diffs of `a` (named `label`) against the never-saved run.
fn diff_section(label: &str, a: &mut App, never: &mut App) -> String {
    let diffs = entity_diffs(a.world_mut(), never.world_mut());
    let json = serde_json::to_string_pretty(&diffs).unwrap_or_default();
    format!(
        "\n  {label} (a) vs never-saved (b): {} entities differ (hashed components only; \
         none means a resource or unhashed state differs)\n{json}",
        diffs.len()
    )
}

/// What differs at a mismatching checkpoint, for the failure message.
fn describe(runs: &mut Runs, tick: u64, (n, s, l): (u64, u64, u64)) -> String {
    let mut msg = format!(
        "first mismatch after tick {tick}: never-saved {n:016x}, saved {s:016x}, loaded {l:016x}"
    );
    if s != n {
        msg.push_str("\n  saving changed the sim (saved != never-saved)");
        msg.push_str(&diff_section("saved", &mut runs.saved, &mut runs.never));
    }
    if l != n {
        msg.push_str(&diff_section("loaded", &mut runs.loaded, &mut runs.never));
    }
    msg
}

/// [`assert_resume_equivalent`] returning the failure message.
///
/// # Errors
/// The first mismatch (with entity diffs), or a save/load failure.
pub fn check_resume_equivalent(
    make: impl Fn() -> App,
    at: u64,
    run: u64,
    every: u64,
) -> Result<(), String> {
    let every = every.max(1);
    let mut runs = start(&make, at)?;
    let mut last_good = at;
    let mut ticks = 0;
    loop {
        if ticks % every == 0 || ticks == run {
            let h = hashes(&mut runs);
            if h.0 != h.1 || h.0 != h.2 {
                // Narrow down to the first mismatching tick: rebuild and
                // compare every tick from the last matching checkpoint.
                let mut fine = start(&make, at)?;
                for _ in at..last_good {
                    step(&mut fine);
                }
                for t in last_good..=at + ticks {
                    let h = hashes(&mut fine);
                    if h.0 != h.1 || h.0 != h.2 {
                        return Err(format!(
                            "resume at {at} is not equivalent (checkpoint {}): {}",
                            at + ticks,
                            describe(&mut fine, t, h)
                        ));
                    }
                    step(&mut fine);
                }
                return Err(format!(
                    "resume at {at} is not equivalent: {} (not reproduced on a rebuilt run: \
                     the factory is not deterministic)",
                    describe(&mut runs, at + ticks, h)
                ));
            }
            last_good = at + ticks;
        }
        if ticks == run {
            return Ok(());
        }
        step(&mut runs);
        ticks += 1;
    }
}

/// Asserts the snapshot invariant: three apps from `make` (each at tick 0);
/// one runs to `at` and on (never saved), one runs to `at`, is saved and
/// runs on, and one gets the snapshot loaded. All three run `run` more ticks
/// and their state hashes must agree right after the load and every `every`
/// ticks (and at the end).
///
/// # Panics
/// On the first mismatch: the message names the first differing tick and the
/// entity diffs of the loaded vs the never-saved world.
pub fn assert_resume_equivalent(make: impl Fn() -> App, at: u64, run: u64, every: u64) {
    if let Err(msg) = check_resume_equivalent(make, at, run, every) {
        panic!("{msg}");
    }
}
