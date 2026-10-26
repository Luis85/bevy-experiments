//! Tests that need the *global* subscriber get their own test binary: the
//! global default can be set only once per process, so both tests share one
//! collector installed through [`global`]. They use disjoint span names.

use std::sync::OnceLock;

use npc_perf::{PerfCollector, install_global};
use tracing::info_span;

fn global() -> &'static PerfCollector {
    static GLOBAL: OnceLock<PerfCollector> = OnceLock::new();
    GLOBAL.get_or_init(|| {
        let collector = PerfCollector::new(600);
        assert!(install_global(&collector), "first install succeeds");
        collector
    })
}

#[test]
fn install_global_once_and_captures_worker_threads() {
    let collector = global();
    assert!(
        !install_global(&PerfCollector::new(600)),
        "second install fails"
    );

    let handles: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                let span =
                    info_span!(parent: None, "system", name = "npc_needs::decay_needs_system");
                for _ in 0..250 {
                    let _g = span.enter();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    collector.end_frame();
    let report = collector.report();
    let sys = report.system("npc_needs::decay_needs_system").unwrap();
    assert_eq!(sys.calls, 1000);
    assert_eq!(report.mechanic("needs").unwrap().calls, 1000);
}

mod bevy_mt {
    use std::collections::HashSet;
    use std::sync::Mutex;
    use std::thread::ThreadId;
    use std::time::Duration;

    use bevy_ecs::schedule::{MultiThreadedExecutor, Schedule};

    pub static THREADS: Mutex<Option<HashSet<ThreadId>>> = Mutex::new(None);

    fn note_thread() {
        THREADS
            .lock()
            .unwrap()
            .get_or_insert_with(HashSet::new)
            .insert(std::thread::current().id());
        // Long enough that the executor runs the systems in parallel.
        std::thread::sleep(Duration::from_micros(500));
    }

    pub fn sys_a() {
        note_thread();
    }
    pub fn sys_b() {
        note_thread();
    }
    pub fn sys_c() {
        note_thread();
    }
    pub fn sys_d() {
        note_thread();
    }

    /// Four systems with no data access, so they may all run in parallel.
    pub fn schedule() -> Schedule {
        let mut schedule = Schedule::default();
        schedule.set_executor(MultiThreadedExecutor::new());
        schedule.add_systems((sys_a, sys_b, sys_c, sys_d));
        schedule
    }
}

#[test]
fn multithreaded_executor_systems_are_captured() {
    let collector = global();
    // Systems are added after the global subscriber exists, so their cached
    // spans are enabled.
    let mut world = bevy_ecs::world::World::new();
    let mut schedule = bevy_mt::schedule();
    for _ in 0..20 {
        schedule.run(&mut world);
        collector.end_frame();
    }
    let report = collector.report();
    for name in ["sys_a", "sys_b", "sys_c", "sys_d"] {
        let full = format!("global::bevy_mt::{name}");
        let sys = report
            .system(&full)
            .unwrap_or_else(|| panic!("{full} missing: {:?}", report.systems));
        assert_eq!(sys.calls, 20, "{full}");
        assert!(sys.p50_us >= 400.0, "{full} timed: {sys:?}");
        assert_eq!(sys.mechanic, npc_perf::UNTAGGED);
    }
    let threads = bevy_mt::THREADS
        .lock()
        .unwrap()
        .as_ref()
        .map_or(0, |t| t.len());
    assert!(threads > 1, "systems ran on {threads} thread(s)");
}
