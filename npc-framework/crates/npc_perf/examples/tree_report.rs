//! Synthetic hierarchy snapshot cost: run with `cargo run --release --locked -p npc_perf --example tree_report`.
#![expect(
    clippy::print_stdout,
    reason = "benchmark prints measured diagnostic costs"
)]
use npc_perf::{MechanicTags, PerfCollector};
use std::time::{Duration, Instant};

fn best(mut f: impl FnMut()) -> Duration {
    (0..7)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed()
        })
        .min()
        .unwrap()
}
fn main() {
    for count in [300, 1000, 2500] {
        let c = PerfCollector::new(600);
        let mut tags = MechanicTags::default();
        for group in 0..20 {
            tags.add(format!("synthetic::g{group}::"), format!("work.g{group}"));
        }
        c.set_tags(tags);
        let names: Vec<_> = (0..count)
            .map(|i| format!("synthetic::g{}::system_{i}", i % 20))
            .collect();
        for frame in 0..600 {
            for (i, name) in names.iter().enumerate() {
                c.record_system(
                    name,
                    Duration::from_nanos(100 + ((frame * 7919 + i * 313) % 40000) as u64),
                );
            }
            c.end_frame();
        }
        let report = c.tree_report(None);
        assert_eq!(report.nodes.len(), count);
        let id = report.nodes[0].id;
        for sample in 0..240 {
            c.record_history(sample as f64 * 0.5, None);
        }
        let mut now = 120.0;
        let sample = best(|| {
            now += 0.5;
            c.record_history(now, None);
        });
        let history = best(|| {
            std::hint::black_box(c.history_report(&npc_perf::HistorySelection::Trace { id }, None));
        });
        let live = best(|| {
            std::hint::black_box(c.tree_report(Some(id)));
        });
        let capture = best(|| {
            std::hint::black_box(c.capture_tree());
        });
        let close = best(|| c.end_frame());
        println!(
            "{count} paths / 600 samples / 240 history points: live selected snapshot {live:?}; history sample {sample:?}; history projection {history:?}; complete freeze/export {capture:?}; idle interval close {close:?}"
        );
    }
}
