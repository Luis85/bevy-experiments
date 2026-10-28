use npc_perf::{PerfCollector, PerfDomain, TraceKind};
use std::time::Duration;
use tracing::{Dispatch, info_span};
use tracing_subscriber::layer::SubscriberExt;

fn scoped<R>(collector: &PerfCollector, f: impl FnOnce() -> R) -> R {
    let dispatch = Dispatch::new(tracing_subscriber::registry().with(collector.layer()));
    tracing::dispatcher::with_default(&dispatch, f)
}

#[test]
fn cached_parentless_systems_get_execution_time_parents_and_self_cost() {
    let c = PerfCollector::new(16);
    scoped(&c, || {
        let cached = info_span!(parent:None, "system", name="npc_perception::scan");
        let outer = info_span!(parent:None, "system", name="npc_kernel::driver::drive_sim");
        {
            let _parent = outer.enter();
            let _system = cached.enter();
            let _child = info_span!("mechanic", name = "perception").entered();
        }
        {
            let _root = cached.enter();
        }
    });
    c.end_frame();
    let tree = c.tree_report(None);
    let scans: Vec<_> = tree
        .nodes
        .iter()
        .filter(|n| n.name == "npc_perception::scan")
        .collect();
    assert_eq!(scans.len(), 2, "one cached system has two execution paths");
    assert!(scans.iter().any(|n| n.parent.is_none()));
    assert!(scans.iter().any(|n| n.parent.is_some()));
    assert!(
        tree.nodes
            .iter()
            .all(|n| n.self_total_us <= n.inclusive_total_us)
    );
    assert!(
        tree.nodes
            .iter()
            .any(|n| n.kind == TraceKind::Mechanic && n.name == "perception")
    );
    assert!(
        tree.domains
            .iter()
            .any(|d| d.domain == PerfDomain::Simulation && d.work.mean_us >= 0.0)
    );
}

#[test]
fn existing_budget_report_stays_compatible_and_tree_reset_keeps_spans_usable() {
    let c = PerfCollector::new(8);
    scoped(&c, || {
        let span = info_span!(parent:None,"system",name="npc_needs::decay");
        {
            let _s = span.enter();
        }
        c.end_frame();
        assert_eq!(c.report().system("npc_needs::decay").unwrap().calls, 1);
        assert!(!c.tree_report(None).nodes.is_empty());
        c.reset();
        assert!(c.tree_report(None).nodes.is_empty());
        {
            let _s = span.enter();
        }
        c.end_frame();
        assert_eq!(c.tree_report(None).nodes.len(), 1);
    });
}

#[test]
fn domain_and_parent_mechanic_percentiles_use_raw_interval_sums() {
    let c = PerfCollector::new(8);
    // Child p95 values each equal 100; their parent's p95 must be 100, not 200.
    c.record_mechanic("think.agent", Duration::from_micros(100));
    c.end_frame();
    c.record_mechanic("think.cond", Duration::from_micros(100));
    c.end_frame();
    c.end_frame();
    let tree = c.tree_report(None);
    let think = tree.groups.iter().find(|g| g.mechanic == "think").unwrap();
    assert_eq!(think.work.p95_us, 100.0);
    assert!((think.work.mean_us - 200.0 / 3.0).abs() < 0.001);
    let sim = tree
        .domains
        .iter()
        .find(|g| g.domain == PerfDomain::Simulation)
        .unwrap();
    assert_eq!(sim.work.p95_us, 100.0);
    assert_eq!(sim.work.samples, 3);
}

#[test]
fn inactive_limits_and_warmup_are_not_false_green_measurements() {
    let c = PerfCollector::new(8);
    let mut budgets = npc_perf::PerfBudgets::default();
    budgets.set("needs", 5).set("not_seen", 10);
    c.set_budgets(budgets);
    c.set_min_samples(2);
    c.record_for_test("npc_needs::decay", 10);
    c.end_frame();
    let r = c.tree_report(None);
    let unseen = r.budgets.iter().find(|b| b.mechanic == "not_seen").unwrap();
    assert_eq!(unseen.observed_p95_us, None);
    assert!(!unseen.ready);
    let needs = r.budgets.iter().find(|b| b.mechanic == "needs").unwrap();
    assert_eq!(needs.observed_p95_us, Some(10.0));
    assert!(!needs.ready);
    assert!(!needs.over_budget);
    c.record_for_test("npc_needs::decay", 10);
    c.end_frame();
    assert!(
        c.tree_report(None)
            .budgets
            .iter()
            .find(|b| b.mechanic == "needs")
            .unwrap()
            .over_budget
    );
}

#[test]
fn shared_cached_span_is_safe_across_threads_and_reentrant_entries() {
    use std::sync::{Arc, Barrier};
    let c = PerfCollector::new(16);
    let d = Dispatch::new(tracing_subscriber::registry().with(c.layer()));
    let span = tracing::dispatcher::with_default(
        &d,
        || info_span!(parent:None,"system",name="npc_needs::shared"),
    );
    let barrier = Arc::new(Barrier::new(4));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let (d, span, b) = (d.clone(), span.clone(), barrier.clone());
            std::thread::spawn(move || {
                tracing::dispatcher::with_default(&d, || {
                    let _outer = span.enter();
                    b.wait();
                    let _inner = span.enter();
                })
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    c.end_frame();
    assert_eq!(c.report().system("npc_needs::shared").unwrap().calls, 8);
    let r = c.tree_report(None);
    assert_eq!(r.nodes.len(), 2);
    assert_eq!(r.nodes.iter().map(|n| n.calls).sum::<u64>(), 8);
    assert!(
        r.nodes
            .iter()
            .all(|n| n.self_total_us <= n.inclusive_total_us)
    );
}

#[test]
fn reset_discards_inflight_measurements_and_retagging_rebuilds_tree_ownership() {
    let c = PerfCollector::new(8);
    scoped(&c, || {
        let span = info_span!(parent:None,"system",name="game::work");
        let guard = span.enter();
        c.reset();
        drop(guard);
        assert!(c.tree_report(None).nodes.is_empty());
        assert!(c.report().systems.is_empty());
        let mut domains = npc_perf::DomainTags::default();
        domains.system("game", PerfDomain::UserInterface);
        c.set_domains(domains);
        {
            let _g = span.enter();
        }
        c.end_frame();
        assert_eq!(
            c.tree_report(None).nodes[0].domain,
            PerfDomain::UserInterface
        );
    });
}

#[test]
fn concurrent_reset_and_path_creation_never_reuse_a_stale_parent_id() {
    let c = PerfCollector::new(8);
    let dispatch = Dispatch::new(tracing_subscriber::registry().with(c.layer()));
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    std::thread::scope(|threads| {
        let barrier_worker = barrier.clone();
        threads.spawn(move || {
            tracing::dispatcher::with_default(&dispatch, || {
                let parent = info_span!(parent:None,"system",name="npc_needs::parent");
                let child = info_span!(parent:None,"system",name="npc_needs::child");
                barrier_worker.wait();
                for _ in 0..4000 {
                    let _parent = parent.enter();
                    let _child = child.enter();
                    std::thread::yield_now();
                }
            })
        });
        barrier.wait();
        for _ in 0..1000 {
            c.reset();
            let tree = c.tree_report(None);
            assert!(
                tree.nodes.iter().all(|n| n.parent.is_none_or(|p| p < n.id)),
                "a retained parent must have been created first"
            );
        }
    });
    let tree = c.tree_report(None);
    assert!(tree.nodes.iter().all(|n| n.parent.is_none_or(|p| p < n.id)));
}

#[test]
fn bounded_capture_and_explicit_domain_scopes_are_visible_in_metadata() {
    let c = PerfCollector::new(8);
    c.set_trace_limit(4);
    scoped(&c, || {
        let _parent = info_span!(parent:None,"system",name="game::parent").entered();
        for n in 0..20 {
            let name = format!("part{n}");
            let _child = info_span!("perf_scope", name = name.as_str(), domain = "ui").entered();
        }
    });
    c.end_frame();
    let tree = c.tree_report(None);
    assert_eq!(tree.nodes.len(), 4);
    assert!(tree.dropped_entries > 0);
    assert!(
        tree.nodes
            .iter()
            .any(|n| n.kind == TraceKind::Scope && n.domain == PerfDomain::UserInterface)
    );
    let selected = tree.nodes[1].id;
    let detail = c.tree_report(Some(selected)).detail.unwrap();
    assert!(detail.inclusive_per_call.p95_us >= detail.self_per_call.p95_us);
    let value = serde_json::to_value(&tree).unwrap();
    let back: npc_perf::PerfTreeReport = serde_json::from_value(value).unwrap();
    assert_eq!(tree, back);
}

#[test]
fn depth_limit_does_not_restart_descendants_as_duplicate_roots() {
    let c = PerfCollector::new(8);
    scoped(&c, || {
        let span = info_span!(parent:None,"system",name="npc_needs::recursive");
        let mut guards = Vec::new();
        for _ in 0..npc_perf::MAX_TRACE_DEPTH + 12 {
            guards.push(span.enter());
        }
        while let Some(guard) = guards.pop() {
            drop(guard);
        }
    });
    c.end_frame();
    let report = c.tree_report(None);
    assert!(report.dropped_entries > 0);
    assert_eq!(
        report
            .nodes
            .iter()
            .filter(|n| n.parent.is_none())
            .map(|n| n.calls)
            .sum::<u64>(),
        1
    );
}

#[test]
fn frozen_complete_capture_keeps_call_percentiles_for_every_path() {
    let c = PerfCollector::new(8);
    c.record_for_test("npc_needs::decay", 50);
    c.record_for_test("npc_perception::scan", 100);
    c.end_frame();
    let snapshot = c.capture_tree();
    assert_eq!(snapshot.details.len(), snapshot.nodes.len());
    c.record_for_test("npc_needs::decay", 500);
    c.end_frame();
    let needs = snapshot
        .nodes
        .iter()
        .find(|n| n.name == "npc_needs::decay")
        .unwrap();
    assert_eq!(
        snapshot
            .details
            .iter()
            .find(|d| d.id == needs.id)
            .unwrap()
            .inclusive_per_call
            .p95_us,
        50.0
    );
}

#[test]
fn tick_wall_samples_are_per_schedule_execution_not_per_render_interval() {
    use bevy_ecs::schedule::{Schedule, ScheduleLabel};
    #[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
    struct NpcSimTick;
    let c = PerfCollector::new(16);
    scoped(&c, || {
        let mut world = bevy_ecs::world::World::new();
        let mut schedule = Schedule::new(NpcSimTick);
        schedule.add_systems(|| std::hint::black_box(()));
        for _ in 0..3 {
            schedule.run(&mut world);
        }
    });
    c.end_frame();
    let report = c.tree_report(None);
    assert_eq!(report.frames, 1);
    assert_eq!(report.tick.unwrap().samples, 3);
}
