//! Integration tests for the `npc_perf` tracing layer and collector.
//!
//! Single-thread tests use a scoped subscriber (`tracing::subscriber::with_default`)
//! so tests in this binary never share a collector.

use std::thread;

use npc_perf::{
    DEBUG_BUDGET_SCALE, DEFAULT_MIN_SAMPLES, MechanicTags, PerfBudgets, PerfCollector, PerfReport,
    UNTAGGED,
};
use tracing::{Dispatch, info_span};
use tracing_subscriber::layer::SubscriberExt;

fn dispatch(collector: &PerfCollector) -> Dispatch {
    Dispatch::new(tracing_subscriber::registry().with(collector.layer()))
}

fn with_collector<R>(collector: &PerfCollector, f: impl FnOnce() -> R) -> R {
    tracing::dispatcher::with_default(&dispatch(collector), f)
}

#[test]
fn system_spans_timed_and_tagged() {
    let collector = PerfCollector::new(600);
    with_collector(&collector, || {
        // Bevy caches one span per system and enters it on every run.
        let span = info_span!(parent: None, "system", name = "npc_needs::decay_needs_system");
        for _ in 0..10 {
            let _g = span.enter();
            std::hint::black_box(0u64);
        }
    });
    let report = collector.report();
    let sys = report
        .system("npc_needs::decay_needs_system")
        .expect("system recorded");
    assert_eq!(sys.calls, 10);
    assert_eq!(sys.mechanic, "needs");
    assert!(sys.total_us >= 0.0);
    assert!(sys.max_us >= sys.p95_us && sys.p95_us >= sys.p50_us);
}

#[test]
fn mechanic_spans_use_name_directly() {
    let collector = PerfCollector::new(600);
    with_collector(&collector, || {
        for _ in 0..3 {
            let _g = info_span!("mechanic", name = "think.agent").entered();
        }
        // Spans with other names are ignored.
        let _g = info_span!("schedule", name = "SimTick").entered();
    });
    collector.end_frame();
    let report = collector.report();
    let m = report.mechanic("think.agent").expect("mechanic recorded");
    assert_eq!(m.calls, 3);
    assert_eq!(m.samples, 1);
    // Mechanic spans are not systems.
    assert!(report.systems.is_empty(), "{:?}", report.systems);
    assert!(report.mechanic("SimTick").is_none());
}

#[test]
fn percentiles_correct() {
    let collector = PerfCollector::new(600);
    for us in 1..=100 {
        collector.record_for_test("npc_needs::decay_needs_system", us);
    }
    let report = collector.report();
    let sys = report.system("npc_needs::decay_needs_system").unwrap();
    assert_eq!(sys.calls, 100);
    assert_eq!(sys.p50_us, 50.0);
    assert_eq!(sys.p95_us, 95.0);
    assert_eq!(sys.max_us, 100.0);
    assert_eq!(sys.total_us, 5050.0);
}

#[test]
fn rolling_window_drops_old_samples() {
    let collector = PerfCollector::new(10);
    for _ in 0..10 {
        collector.record_for_test("a::x", 1000);
    }
    for _ in 0..10 {
        collector.record_for_test("a::x", 1);
    }
    let report = collector.report();
    let sys = report.system("a::x").unwrap();
    // Percentiles cover the last 10 samples only; totals cover the whole run.
    assert_eq!(sys.max_us, 1.0);
    assert_eq!(sys.calls, 20);
    assert_eq!(sys.total_us, 10_010.0);
}

#[test]
fn end_frame_sums_per_mechanic() {
    let collector = PerfCollector::new(600);
    collector.set_tags(MechanicTags::new([("a::", "m")]));
    collector.record_for_test("a::x", 10);
    collector.record_for_test("a::y", 20);
    collector.end_frame();
    collector.record_for_test("a::x", 5);
    collector.end_frame();
    // A frame where the mechanic did not run adds no sample.
    collector.end_frame();

    let report = collector.report();
    assert_eq!(report.frames, 3);
    let m = report.mechanic("m").unwrap();
    assert_eq!(m.samples, 2);
    assert_eq!(m.calls, 3);
    assert_eq!(m.max_us, 30.0);
    assert_eq!(m.total_us, 35.0);
    assert_eq!(m.p50_us, 5.0);
    assert_eq!(m.p95_us, 30.0);
    assert_eq!(m.budget_us, None);
    assert!(!m.over_budget);
}

#[test]
fn budgets_wait_for_min_samples() {
    let collector = PerfCollector::new(600);
    assert_eq!(collector.min_samples(), DEFAULT_MIN_SAMPLES);
    assert_eq!(DEFAULT_MIN_SAMPLES, 60);
    let mut budgets = PerfBudgets::default();
    budgets.set("needs", 0);
    collector.set_budgets(budgets);
    collector.set_min_samples(5);
    // Samples 1..=4: over budget, but too few samples to judge a p95.
    for _ in 0..4 {
        collector.record_for_test("npc_needs::decay_needs_system", 10);
        collector.end_frame();
    }
    assert!(
        collector.take_breaches().is_empty(),
        "no check before 5 samples"
    );
    let m = collector.report().mechanic("needs").cloned().unwrap();
    assert_eq!(m.samples, 4);
    assert!(!m.over_budget, "the report waits for min_samples too");
    // The 5th sample makes the check count.
    collector.record_for_test("npc_needs::decay_needs_system", 10);
    collector.end_frame();
    let breaches = collector.take_breaches();
    assert_eq!(breaches.len(), 1, "{breaches:?}");
    assert_eq!(breaches[0].frame, 5);
    assert!(collector.report().mechanic("needs").unwrap().over_budget);

    // `reset` starts the count again: no spurious breach right after it.
    collector.reset();
    collector.record_for_test("npc_needs::decay_needs_system", 10);
    collector.end_frame();
    assert!(
        collector.take_breaches().is_empty(),
        "reset restarts the count"
    );
}

#[test]
fn budgets_wait_for_default_min_samples() {
    let collector = PerfCollector::new(600);
    let mut budgets = PerfBudgets::default();
    budgets.set("needs", 0);
    collector.set_budgets(budgets);
    for _ in 1..DEFAULT_MIN_SAMPLES {
        collector.record_for_test("npc_needs::decay_needs_system", 10);
        collector.end_frame();
    }
    assert!(collector.take_breaches().is_empty());
    collector.record_for_test("npc_needs::decay_needs_system", 10);
    collector.end_frame();
    assert_eq!(collector.take_breaches().len(), 1);
}

#[test]
fn budgets_scale_for_debug_profile() {
    let base = PerfBudgets::default_npc();
    let scaled = PerfBudgets::default_npc().scaled_for_profile();
    let factor = if cfg!(debug_assertions) {
        DEBUG_BUDGET_SCALE
    } else {
        1
    };
    assert_eq!(DEBUG_BUDGET_SCALE, 10);
    for (mechanic, us) in base.iter() {
        assert_eq!(scaled.get(mechanic), Some(us * factor), "{mechanic}");
    }
    assert_eq!(base.clone().scaled(3).get("think"), Some(60 * 3));
}

#[test]
fn budget_breach_rate_limited() {
    let collector = PerfCollector::new(600);
    collector.set_min_samples(1);
    let mut budgets = PerfBudgets::default();
    budgets.set("needs", 0);
    collector.set_budgets(budgets);
    for _ in 0..3 {
        collector.record_for_test("npc_needs::decay_needs_system", 10);
        collector.end_frame();
    }
    let breaches = collector.take_breaches();
    assert_eq!(breaches.len(), 1, "{breaches:?}");
    assert_eq!(breaches[0].mechanic, "needs");
    assert_eq!(breaches[0].budget_us, 0);
    assert_eq!(breaches[0].p95_us, 10.0);
    assert_eq!(breaches[0].frame, 1);
    assert!(collector.take_breaches().is_empty(), "take drains the list");

    let report = collector.report();
    let m = report.mechanic("needs").unwrap();
    assert_eq!(m.budget_us, Some(0));
    assert!(m.over_budget);
}

#[test]
fn within_budget_no_breach() {
    let collector = PerfCollector::new(600);
    collector.set_min_samples(1);
    let mut budgets = PerfBudgets::default();
    budgets.set("needs", 50);
    collector.set_budgets(budgets);
    collector.record_for_test("npc_needs::decay_needs_system", 10);
    collector.end_frame();
    assert!(collector.take_breaches().is_empty());
    assert!(!collector.report().mechanic("needs").unwrap().over_budget);
}

#[test]
fn layer_aggregates_across_threads() {
    let collector = PerfCollector::new(600);
    let d = dispatch(&collector);
    let handles: Vec<_> = (0..8)
        .map(|t| {
            let d = d.clone();
            thread::spawn(move || {
                tracing::dispatcher::with_default(&d, || {
                    // One cached span per thread (like a Bevy system) plus a
                    // shared name across threads.
                    let own = format!("worker::sys_{t}");
                    let cached = info_span!(parent: None, "system", name = own.as_str());
                    for _ in 0..1000 {
                        let _g = info_span!(parent: None, "system", name = "shared::sys").entered();
                        drop(_g);
                        let _c = cached.enter();
                    }
                });
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let report = collector.report();
    assert_eq!(report.system("shared::sys").unwrap().calls, 8000);
    for t in 0..8 {
        assert_eq!(
            report.system(&format!("worker::sys_{t}")).unwrap().calls,
            1000
        );
    }
    collector.end_frame();
    let untagged = collector.report().mechanic(UNTAGGED).unwrap().calls;
    assert_eq!(untagged, 16_000);
}

#[test]
fn bevy_schedule_systems_are_captured() {
    use bevy_ecs::prelude::*;
    use bevy_ecs::schedule::Schedule;

    #[derive(Resource, Default)]
    struct Counter(u32);

    fn first_system(mut c: ResMut<Counter>) {
        c.0 += 1;
    }
    fn second_system(c: Res<Counter>) {
        std::hint::black_box(c.0);
    }

    let collector = PerfCollector::new(600);
    with_collector(&collector, || {
        let mut world = World::new();
        world.init_resource::<Counter>();
        let mut schedule = Schedule::default();
        schedule.add_systems((first_system, second_system).chain());
        for _ in 0..5 {
            schedule.run(&mut world);
            collector.end_frame();
        }
        assert_eq!(world.resource::<Counter>().0, 5);
    });
    let report = collector.report();
    let names: Vec<&str> = report.systems.iter().map(|s| s.name.as_str()).collect();
    // Bevy names a fn system by its full type path: `<crate>::<fn path>`.
    // Systems defined inside a test fn include the enclosing fn in the path.
    let first = report
        .systems
        .iter()
        .find(|s| s.name.ends_with("first_system"))
        .unwrap_or_else(|| panic!("first_system missing in {names:?}"));
    let second = report
        .systems
        .iter()
        .find(|s| s.name.ends_with("second_system"))
        .unwrap_or_else(|| panic!("second_system missing in {names:?}"));
    assert_eq!(
        first.name,
        "perf::bevy_schedule_systems_are_captured::first_system"
    );
    assert_eq!(first.calls, 5);
    assert_eq!(second.calls, 5);
    assert_eq!(report.frames, 5);
}

#[test]
fn tags_longest_prefix_wins() {
    let tags = MechanicTags::new([("a::", "short"), ("a::b::", "long"), ("a::b::c", "longest")]);
    assert_eq!(tags.mechanic_for("a::x"), "short");
    assert_eq!(tags.mechanic_for("a::b::x"), "long");
    assert_eq!(tags.mechanic_for("a::b::c_system"), "longest");
    assert_eq!(tags.mechanic_for("z::x"), UNTAGGED);
}

#[test]
fn tags_tolerate_wrapped_type_paths() {
    let tags = MechanicTags::default_npc();
    // Closure systems, generic systems and `<T as Trait>` paths.
    assert_eq!(
        tags.mechanic_for("npc_view_helpers::select::build::{{closure}}"),
        "view.select"
    );
    assert_eq!(tags.mechanic_for("<npc_needs::Foo as Bar>::run"), "needs");
    assert_eq!(
        tags.mechanic_for("&npc_kernel::commands::apply_commands_system"),
        "inputs"
    );
    assert_eq!(
        tags.mechanic_for(
            "bevy_ecs::message::update::message_update_system<npc_sim::effects::SimEffect>"
        ),
        "engine"
    );
    // Bevy's placeholder name when its `debug` feature is off.
    assert_eq!(
        tags.mechanic_for("<Enable the debug feature to see the name>"),
        UNTAGGED
    );
}

#[test]
fn default_npc_tags_cover_existing_systems() {
    let tags = MechanicTags::default_npc();
    let cases = [
        // P3: the generic crates only; content tags its own systems (e.g.
        // `village_content::perf_tags`, tested there).
        ("npc_kernel::math::copy_prev_pos_system", "actions.move"),
        ("npc_needs::decay_needs_system", "needs"),
        ("npc_kernel::clock::day_night_system", "clock"),
        ("npc_kernel::clock::rethink_timer_system", "think"),
        ("npc_kernel::clock::advance_clock_system", "clock"),
        ("npc_kernel::schedule::think_system", "think"),
        ("npc_kernel::commands::apply_commands_system", "inputs"),
        ("npc_kernel::driver::apply_host_positions_system", "inputs"),
        ("npc_kernel::journal::flush_journal_system", "journal"),
        ("npc_kernel::driver::drive_sim", "driver"),
        ("npc_ai::runtime::think", "think"),
        ("npc_view_helpers::interp::interp_alpha", "view.interp"),
        ("npc_view_helpers::select::click_select", "view.select"),
        (
            "npc_view_helpers::screenshot::take_screenshot",
            "view.screenshot",
        ),
        (
            "npc_view_helpers::perf_panel::update_perf_panel",
            "view.perf_panel",
        ),
        ("bevy_render::view::prepare_view_targets", "render"),
        ("bevy_pbr::render::prepare_lights", "render"),
        (
            "bevy_transform::systems::propagate_parent_transforms",
            "engine",
        ),
        ("bevy_time::time_system", "engine"),
        // Inclusive of the whole frame: not part of `engine`.
        ("bevy_app::main_schedule::Main::run_main", "frame"),
    ];
    for (name, mechanic) in cases {
        assert_eq!(tags.mechanic_for(name), mechanic, "{name}");
    }
}

#[test]
fn set_tags_remaps_existing_systems() {
    let collector = PerfCollector::new(600);
    collector.record_for_test("game::foo_system", 3);
    assert_eq!(
        collector
            .report()
            .system("game::foo_system")
            .unwrap()
            .mechanic,
        UNTAGGED
    );
    collector.set_tags(MechanicTags::new([("game::", "game")]));
    assert_eq!(
        collector
            .report()
            .system("game::foo_system")
            .unwrap()
            .mechanic,
        "game"
    );
}

#[test]
fn reset_clears_stats_but_keeps_tracking() {
    let collector = PerfCollector::new(600);
    let d = dispatch(&collector);
    let span = tracing::dispatcher::with_default(
        &d,
        || info_span!(parent: None, "system", name = "npc_needs::decay_needs_system"),
    );
    tracing::dispatcher::with_default(&d, || {
        let _g = span.enter();
    });
    collector.end_frame();
    collector.reset();
    let report = collector.report();
    assert_eq!(report.frames, 0);
    assert!(report.systems.is_empty() && report.mechanics.is_empty());
    // The span cached before the reset is still timed afterwards.
    tracing::dispatcher::with_default(&d, || {
        let _g = span.enter();
    });
    assert_eq!(
        collector
            .report()
            .system("npc_needs::decay_needs_system")
            .unwrap()
            .calls,
        1
    );
}

#[test]
fn report_sorted_and_serde_roundtrip() {
    let collector = PerfCollector::new(600);
    collector.set_budgets(PerfBudgets::default_npc());
    collector.record_for_test("npc_needs::decay_needs_system", 5);
    collector.record_for_test("npc_kernel::schedule::think_system", 50);
    collector.record_for_test("npc_kernel::journal::flush_journal_system", 1);
    collector.end_frame();
    let report = collector.report();
    let totals: Vec<f64> = report.systems.iter().map(|s| s.total_us).collect();
    assert!(totals.windows(2).all(|w| w[0] >= w[1]), "{totals:?}");
    assert_eq!(report.systems[0].mechanic, "think");
    assert_eq!(report.mechanics[0].mechanic, "think");
    assert!(report.mechanics[0].budget_us.is_some());

    let json = serde_json::to_string(&report).unwrap();
    let back: PerfReport = serde_json::from_str(&json).unwrap();
    assert_eq!(back, report);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(v["systems"][0]["p95_us"].is_number());
    assert!(v["mechanics"][0]["over_budget"].is_boolean());
    assert_eq!(v["frames"], 1);

    let top = report.top_systems(2);
    assert_eq!(top.len(), 2);
    let table = report.to_table(10);
    assert!(table.contains("think"), "{table}");
    assert!(table.contains("decay_needs_system"), "{table}");
}

#[test]
fn default_budgets_cover_sim_mechanics() {
    let budgets = PerfBudgets::default_npc();
    for m in [
        "needs",
        "think",
        "actions.move",
        "effects",
        "clock",
        "inputs",
        "journal",
    ] {
        assert!(budgets.get(m).is_some(), "{m}");
    }
    // Per-agent mechanics scale with the agent count; fixed ones do not.
    let scaled = PerfBudgets::default_npc_for(100);
    assert!(scaled.get("think").unwrap() > budgets.get("think").unwrap());
    assert_eq!(scaled.get("clock"), budgets.get("clock"));
}

fn best_of(n: usize, mut f: impl FnMut()) -> std::time::Duration {
    (0..n)
        .map(|_| {
            let start = std::time::Instant::now();
            f();
            start.elapsed()
        })
        .min()
        .unwrap()
}

/// F4 panel follow-up: with 300 systems and full 600-sample windows,
/// `report_top(10)` (what the panel calls, every 0.5 s at most) costs under
/// 1 ms in any build, and is exactly `report()` truncated. `report()` itself
/// is O(window) per row (selection instead of a sort); measured ~1.0-1.3 ms
/// release / ~1.9 ms dev on the dev machine (3.7 / 7.4 ms with the sort).
/// `cargo test -p npc_perf --release --test perf report_ -- --nocapture`
/// prints the numbers.
#[test]
fn report_300_systems_under_1ms() {
    let collector = PerfCollector::new(600);
    let mut tags = MechanicTags::default_npc();
    for m in 0..20 {
        tags.add(format!("game::m{m}::"), format!("m{m}"));
    }
    collector.set_tags(tags);
    // Deterministic pseudo-random durations so selection does real work.
    let mut x: u64 = 0x2545_f491_4f6c_dd1d;
    for frame in 0..600 {
        for s in 0..300 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            collector.record_system(
                &format!("game::m{}::system_{s}", s % 20),
                std::time::Duration::from_nanos(200 + x % 50_000),
            );
        }
        if frame % 2 == 0 {
            collector.end_frame();
        }
    }
    let full = collector.report();
    assert_eq!(full.systems.len(), 300);
    let top = collector.report_top(10);
    assert_eq!(top.systems, full.systems[..10]);
    assert_eq!(top.mechanics, full.mechanics[..10]);
    assert_eq!(top.frames, full.frames);

    let top_cost = best_of(7, || {
        std::hint::black_box(collector.report_top(10));
    });
    let full_cost = best_of(7, || {
        std::hint::black_box(collector.report());
    });
    eprintln!("300 systems: report_top(10) {top_cost:?}, report() {full_cost:?}");
    assert!(
        top_cost < std::time::Duration::from_millis(1),
        "report_top(10) took {top_cost:?}"
    );
    let full_limit = std::time::Duration::from_millis(if cfg!(debug_assertions) { 8 } else { 3 });
    assert!(full_cost < full_limit, "report() took {full_cost:?}");
}

/// Levels of the `npc_perf` events logged while `f` runs.
fn breach_levels(collector: &PerfCollector, f: impl FnOnce()) -> Vec<tracing::Level> {
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::layer::{Context, Layer};

    #[derive(Clone, Default)]
    struct Levels(Arc<Mutex<Vec<tracing::Level>>>);
    impl<S: tracing::Subscriber> Layer<S> for Levels {
        fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
            if event.metadata().target() == "npc_perf" {
                self.0.lock().unwrap().push(*event.metadata().level());
            }
        }
    }
    let levels = Levels::default();
    let dispatch = Dispatch::new(
        tracing_subscriber::registry()
            .with(collector.layer())
            .with(levels.clone()),
    );
    tracing::dispatcher::with_default(&dispatch, f);
    levels.0.lock().unwrap().clone()
}

#[test]
fn breach_log_modes() {
    use npc_perf::BreachLog;
    use std::time::Duration;
    use tracing::Level;

    let run = |mode: Option<BreachLog>| {
        let collector = PerfCollector::new(600);
        collector.set_min_samples(1);
        collector.set_breach_interval(Duration::ZERO);
        if let Some(mode) = mode {
            collector.set_breach_log(mode);
        }
        let mut budgets = PerfBudgets::default();
        budgets.set("needs", 0);
        collector.set_budgets(budgets);
        let levels = breach_levels(&collector, || {
            for _ in 0..3 {
                collector.record_for_test("npc_needs::decay_needs_system", 10);
                collector.end_frame();
            }
        });
        let breaches = collector.take_breaches().len();
        (levels, breaches, collector.breach_log())
    };
    // Default: a warning per breach (rate-limited by the interval).
    let (levels, breaches, mode) = run(None);
    assert_eq!(mode, BreachLog::Warn);
    assert_eq!(levels, vec![Level::WARN; 3]);
    assert_eq!(breaches, 3, "every breach is recorded");
    // Once: the first breach of a mechanic warns, later ones are debug.
    let (levels, breaches, _) = run(Some(BreachLog::WarnOnce));
    assert_eq!(levels, vec![Level::WARN, Level::DEBUG, Level::DEBUG]);
    assert_eq!(breaches, 3, "logging never drops a recorded breach");
    // Debug: never a warning.
    let (levels, breaches, _) = run(Some(BreachLog::Debug));
    assert_eq!(levels, vec![Level::DEBUG; 3]);
    assert_eq!(breaches, 3);
}

#[test]
fn mechanic_span_inside_its_own_system_counts_once() {
    let collector = PerfCollector::new(600);
    with_collector(&collector, || {
        let system = info_span!(parent: None, "system", name = "npc_vehicles::systems::gate");
        let other = info_span!(parent: None, "system", name = "npc_kernel::schedule::think_system");
        for _ in 0..4 {
            {
                let _s = system.enter();
                // Same mechanic as the enclosing system: already counted.
                let _m = info_span!("mechanic", name = "vehicles").entered();
                // A different mechanic nested inside is its own breakdown.
                let _r = info_span!("mechanic", name = "rules").entered();
            }
            {
                // A nested breakdown span of another name is still timed.
                let _s = other.enter();
                let _m = info_span!("mechanic", name = "think.agent").entered();
            }
            // Outside any system span, the mechanic span is the only timing.
            let _m = info_span!("mechanic", name = "vehicles").entered();
        }
    });
    collector.end_frame();
    let report = collector.report();
    let vehicles = report.mechanic("vehicles").unwrap();
    assert_eq!(
        vehicles.calls, 8,
        "4 system runs + 4 standalone spans, not 12"
    );
    assert_eq!(report.mechanic("rules").unwrap().calls, 4);
    assert_eq!(report.mechanic("think.agent").unwrap().calls, 4);
    assert_eq!(report.mechanic("think").unwrap().calls, 4);
}
