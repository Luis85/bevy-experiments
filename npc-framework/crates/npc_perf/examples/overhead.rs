//! Rough per-span cost of `PerfLayer`. Run with
//! `cargo run -p npc_perf --release --example overhead`.
//!
//! Compares a bare `tracing_subscriber::registry()` against the registry plus
//! `PerfLayer`, for a cached span entered and exited (a Bevy system run) and
//! for a fresh `"mechanic"` span created, entered, exited and closed.

#![expect(clippy::print_stdout, reason = "example prints its measurements")]

use std::hint::black_box;
use std::time::Instant;

use npc_perf::PerfCollector;
use tracing::{Dispatch, info_span};
use tracing_subscriber::layer::SubscriberExt;

const N: u32 = 1_000_000;

fn ns_per_op(dispatch: &Dispatch, f: impl Fn()) -> f64 {
    tracing::dispatcher::with_default(dispatch, || {
        for _ in 0..N / 10 {
            f();
        }
        let start = Instant::now();
        for _ in 0..N {
            f();
        }
        start.elapsed().as_nanos() as f64 / f64::from(N)
    })
}

fn measure(label: &str, dispatch: &Dispatch) -> (f64, f64) {
    let cached = tracing::dispatcher::with_default(
        dispatch,
        || info_span!(parent: None, "system", name = "npc_needs::decay_needs_system"),
    );
    let system = ns_per_op(dispatch, || {
        let _g = cached.enter();
        black_box(());
    });
    let mechanic = ns_per_op(dispatch, || {
        let _g = info_span!("mechanic", name = "think.cond").entered();
        black_box(());
    });
    println!(
        "{label:<22} system enter+exit: {system:7.1} ns   mechanic span new+enter+exit+close: {mechanic:7.1} ns"
    );
    (system, mechanic)
}

fn main() {
    let bare = Dispatch::new(tracing_subscriber::registry());
    let collector = PerfCollector::new(600);
    let with_layer = Dispatch::new(tracing_subscriber::registry().with(collector.layer()));
    let (bs, bm) = measure("registry only", &bare);
    let (ls, lm) = measure("registry + PerfLayer", &with_layer);
    println!(
        "PerfLayer overhead: {:.1} ns per system run, {:.1} ns per mechanic span",
        ls - bs,
        lm - bm
    );
}
