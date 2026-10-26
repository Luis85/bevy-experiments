//! The profiler layer, the JSON log layer and the journal sink, against synthetic tracing
//! data (scoped subscribers, fresh `Profiler`s).

use std::sync::{Arc, Mutex};

use fl_observe::{
    Budgets, DebugConfig, JournalSink, Profiler, feature_of, json_file_layer, new_run_dir,
    observe_layers,
};
use fl_protocol::{PlayerId, SimId, Tick};
use fl_view::JournalEntry;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::Registry;

const PHASES: [&str; 8] = [
    "Input",
    "Think",
    "Act",
    "Resolve",
    "Economy",
    "Perceive",
    "Lifecycle",
    "Publish",
];

/// A fresh profiler that lives for the rest of the test process.
fn profiler() -> &'static Profiler {
    Box::leak(Box::new(Profiler::new()))
}

/// A busy wait, so every timed region is measurably longer than zero.
fn spin() {
    let mut x = 0_u64;
    for i in 0..2_000 {
        x = std::hint::black_box(x.wrapping_add(i));
    }
    std::hint::black_box(x);
}

/// One synthetic sim tick: a `sim_tick` span, and inside it the eight phase marker pairs, each
/// around one `system` span.
fn synthetic_tick(tick: u32, system_spans: &[tracing::Span]) {
    let _tick = tracing::info_span!("sim_tick", tick).entered();
    for (phase, system) in PHASES.iter().zip(system_spans.iter().cycle()) {
        tracing::trace!(target: "fl_sim::phase", phase = *phase, edge = "start");
        {
            let _run = system.enter();
            spin();
        }
        tracing::trace!(target: "fl_sim::phase", phase = *phase, edge = "end");
    }
}

#[test]
fn feature_of_maps_paths() {
    assert_eq!(feature_of("fl_feat_artillery::fire::x"), "artillery");
    assert_eq!(
        feature_of("fl_feat_infantry_client::ui::y"),
        "infantry (client)"
    );
    assert_eq!(feature_of("fl_sim::movement::move_system"), "core.movement");
    assert_eq!(
        feature_of("fl_client::view::interpolate_views"),
        "client.view"
    );
    assert_eq!(feature_of("bevy_render::view::x"), "bevy");
    assert_eq!(feature_of("other::a"), "other");
}

#[test]
fn phase_times_from_marker_events() {
    let profiler = profiler();
    let subscriber = Registry::default().with(profiler.layer());
    tracing::subscriber::with_default(subscriber, || {
        let system =
            tracing::info_span!(parent: None, "system", name = "fl_sim::movement::move_system");
        for tick in 0..30 {
            synthetic_tick(tick, std::slice::from_ref(&system));
        }
    });

    let snapshot = profiler.snapshot();
    assert_eq!(snapshot.ticks, 30);
    assert_eq!(snapshot.tick.samples, 30);
    let names: Vec<&str> = snapshot
        .phases
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(names, PHASES, "phases in first-seen order");
    for (name, stat) in &snapshot.phases {
        assert_eq!(stat.samples, 30, "phase {name}");
        assert!(stat.max_ms > 0.0, "phase {name} was timed");
        assert!(stat.p50_ms <= stat.p95_ms && stat.p95_ms <= stat.max_ms);
    }
    assert_eq!(snapshot.history_ms.len(), 30);
}

#[test]
fn system_spans_on_another_thread() {
    let profiler = profiler();
    let dispatch = tracing::Dispatch::new(Registry::default().with(profiler.layer()));

    // The sim's thread: system spans inside sim ticks.
    let sim_dispatch = dispatch.clone();
    std::thread::spawn(move || {
        tracing::dispatcher::with_default(&sim_dispatch, || {
            let systems = [
                tracing::info_span!(parent: None, "system", name = "fl_sim::movement::move_system"),
                tracing::info_span!(parent: None, "system", name = "fl_feat_artillery::fire::shoot"),
            ];
            for tick in 0..30 {
                synthetic_tick(tick, &systems);
            }
        });
    })
    .join()
    .expect("the sim thread runs");

    // A client worker thread: a system span outside any tick, folded when a frame closes.
    let client_dispatch = dispatch.clone();
    std::thread::spawn(move || {
        tracing::dispatcher::with_default(&client_dispatch, || {
            let system = tracing::info_span!(parent: None, "system", name = "fl_client::view::interpolate_views");
            let _run = system.enter();
            spin();
        });
    })
    .join()
    .expect("the client thread runs");
    tracing::dispatcher::with_default(&dispatch, || {
        let _frame = tracing::info_span!("frame").entered();
    });

    let snapshot = profiler.snapshot();
    assert_eq!(snapshot.ticks, 30);
    let system = |name: &str| {
        snapshot
            .systems
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("system {name} in {:?}", snapshot.systems))
    };
    let movement = system("fl_sim::movement::move_system");
    assert_eq!(movement.feature, "core.movement");
    assert_eq!(movement.stat.samples, 30);
    assert_eq!(movement.history_ms.len(), 30);
    let artillery = system("fl_feat_artillery::fire::shoot");
    assert_eq!(artillery.feature, "artillery");
    assert_eq!(artillery.stat.samples, 30);
    let client = system("fl_client::view::interpolate_views");
    assert_eq!(client.feature, "client.view");
    assert_eq!(client.stat.samples, 1);

    let features: Vec<&str> = snapshot.features.iter().map(|(f, _)| f.as_str()).collect();
    assert_eq!(features, ["artillery", "client.view", "core.movement"]);
    assert_eq!(snapshot.ticks, 30, "a frame is not a sim tick");
}

/// Collects the `message` of every event on `target`.
struct Capture {
    target: &'static str,
    messages: Arc<Mutex<Vec<String>>>,
}

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        struct Message(String);
        impl Visit for Message {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        if event.metadata().target() == self.target {
            let mut message = Message(String::new());
            event.record(&mut message);
            self.messages.lock().expect("capture lock").push(message.0);
        }
    }
}

#[test]
fn budget_overrun_warns() {
    let profiler = profiler();
    profiler.set_budgets(Budgets {
        tick_ms: 0.0,
        ..Budgets::default()
    });
    let messages = Arc::new(Mutex::new(Vec::new()));
    let capture = Capture {
        target: "fl_observe::budget",
        messages: Arc::clone(&messages),
    };
    let subscriber = Registry::default().with(profiler.layer()).with(capture);
    tracing::subscriber::with_default(subscriber, || {
        let _tick = tracing::info_span!("sim_tick", tick = 0_u32).entered();
        spin();
    });

    let messages = messages.lock().expect("capture lock");
    assert!(
        messages.iter().any(|m| m.contains("tick")),
        "a tick budget warning, got {messages:?}"
    );
}

#[test]
fn json_layer_writes_tick_field() {
    let root = tempfile::tempdir().expect("temp dir");
    let run = new_run_dir(root.path()).expect("run dir");
    assert!(run.path.starts_with(root.path()));
    let latest = std::fs::read_to_string(root.path().join("latest.txt")).expect("latest.txt");
    assert_eq!(latest.trim(), run.id);

    let subscriber = Registry::default().with(json_file_layer(&run));
    tracing::subscriber::with_default(subscriber, || {
        let _tick = tracing::info_span!("sim_tick", tick = 3_u32).entered();
        // Like a bevy system span: entered on the thread, but with no parent.
        let system =
            tracing::info_span!(parent: None, "system", name = "fl_sim::movement::move_system");
        let _run = system.enter();
        tracing::info!(target: "fl_sim::movement", unit = 7_u32, "arrived");
    });

    let log = std::fs::read_to_string(run.path.join("log.jsonl")).expect("log.jsonl");
    let line = log.lines().next().expect("one line");
    assert!(line.contains("\"tick\":3"), "{line}");
    let value: serde_json::Value = serde_json::from_str(line).expect("valid JSON");
    assert_eq!(value["level"], "INFO");
    assert_eq!(value["target"], "fl_sim::movement");
    assert_eq!(value["fields"]["message"], "arrived");
    assert_eq!(value["fields"]["unit"], 7);
    assert!(value["timestamp"].is_string());
}

#[test]
fn journal_sink_writes_jsonl() {
    let root = tempfile::tempdir().expect("temp dir");
    let run = new_run_dir(root.path()).expect("run dir");
    let entry = |tick: u32, kind: &str| JournalEntry {
        tick: Tick(tick),
        phase: 2,
        kind: kind.to_owned(),
        sim_id: Some(SimId(4)),
        player: Some(PlayerId(1)),
        emitted: 0,
        data: serde_json::json!({ "x": 1 }),
    };
    let mut sink = JournalSink::create(&run).expect("sink");
    sink.write(&[entry(1, "movement.arrived"), entry(2, "movement.ordered")])
        .expect("write");
    sink.write(&[entry(3, "movement.arrived")]).expect("write");

    let text = std::fs::read_to_string(run.path.join("journal.jsonl")).expect("journal.jsonl");
    let lines: Vec<JournalEntry> = text
        .lines()
        .map(|l| serde_json::from_str(l).expect("one JSON object per line"))
        .collect();
    assert_eq!(
        lines,
        [
            entry(1, "movement.arrived"),
            entry(2, "movement.ordered"),
            entry(3, "movement.arrived")
        ]
    );
}

#[test]
fn logs_without_profiler() {
    // `profiler: false` must not turn the logs off (a missing profiler layer once hinted
    // `LevelFilter::OFF` to the whole stack).
    let root = tempfile::tempdir().expect("temp dir");
    let run = new_run_dir(root.path()).expect("run dir");
    let cfg = DebugConfig {
        log_filter: "info".to_owned(),
        json_log: true,
        journal_file: false,
        profiler: false,
        ..DebugConfig::default()
    };
    let profiler = profiler();
    let subscriber = Registry::default().with(observe_layers(&cfg, Some(&run), profiler));
    tracing::subscriber::with_default(subscriber, || {
        let _tick = tracing::info_span!("sim_tick", tick = 5_u32).entered();
        tracing::info!(target: "fl_sim::movement", "still logged");
        tracing::debug!(target: "fl_sim::movement", "filtered out");
    });

    let log = std::fs::read_to_string(run.path.join("log.jsonl")).expect("log.jsonl");
    let lines: Vec<serde_json::Value> = log
        .lines()
        .map(|l| serde_json::from_str(l).expect("valid JSON"))
        .collect();
    assert_eq!(lines.len(), 1, "{log}");
    assert_eq!(lines[0]["fields"]["message"], "still logged");
    assert_eq!(lines[0]["tick"], 5);
    assert_eq!(profiler.snapshot().ticks, 0, "the profiler saw nothing");
}
