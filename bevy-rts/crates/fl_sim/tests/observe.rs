//! Sim observability: the journal (core records, on and off, sorted, independent of the
//! system order, never hashed), the `fl_sim::phase` markers, the `sim_tick` span and the
//! runner's `JournalSince` query (spec §15 B, ADR-0007).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bevy_app::App;
use bevy_ecs::prelude::*;
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    CommandPayload, DebugSpawnCmd, MoveCmd, PlayerCommand, PlayerId, SimId, StopCmd, Tick,
};
use fl_sim::{
    CommandRegistry, DespawnRequest, Journal, JournalRecord, LifecycleSet, MatchSetup,
    PHASE_TARGET, Phase, RunConfig, SimBuilder, SimClock, SimFeature, SimRunner, TestDriver,
    ThinkSet, build_sim_app, load_content, setup_scenario, step_tick, system_order,
};
use fl_view::{JournalEntry, SimQuery, SimReply, TickDriver, TickInput, TickOutput};
use serde::Serialize;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;

fn core_mod() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_mod")
}

/// The duel (#1 player 0 at (100, 100), #2 player 1 at (900, 900)) with `features`, `run`
/// and cheats on.
fn duel(features: Vec<Arc<dyn SimFeature>>, run: RunConfig) -> App {
    let content = load_content(&features, &[core_mod()]).expect("core_mod loads");
    let setup = MatchSetup {
        seed: 1,
        cheats_allowed: true,
    };
    let mut app = build_sim_app(&features, content, setup, run).expect("the sim builds");
    setup_scenario(app.world_mut(), "core_test.duel").expect("duel sets up");
    app
}

fn journal_on() -> RunConfig {
    RunConfig {
        journal: Some(true),
        ..RunConfig::default()
    }
}

fn now(app: &App) -> Tick {
    app.world().resource::<SimClock>().tick
}

fn tick(app: &mut App, commands: Vec<PlayerCommand>) -> TickOutput {
    let input = TickInput {
        tick: now(app),
        commands,
    };
    step_tick(app.world_mut(), input)
}

fn v(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

/// `player`'s command `seq` for the current tick.
fn command<P: CommandPayload>(
    app: &App,
    player: u8,
    seq: u32,
    units: &[u32],
    payload: &P,
) -> PlayerCommand {
    PlayerCommand {
        exec_tick: now(app),
        player: PlayerId(player),
        seq,
        units: units.iter().map(|&id| SimId(id)).collect(),
        kind: app.world().resource::<CommandRegistry>().encode(payload),
        queue: false,
    }
}

fn kinds(journal: &[JournalEntry]) -> Vec<&str> {
    journal.iter().map(|e| e.kind.as_str()).collect()
}

#[test]
fn applied_and_rejected_commands_are_journaled() {
    let mut app = duel(Vec::new(), journal_on());
    let own = command(
        &app,
        0,
        1,
        &[1],
        &MoveCmd {
            target: v(500, 500),
        },
    );
    let foreign = command(
        &app,
        0,
        2,
        &[2],
        &MoveCmd {
            target: v(500, 500),
        },
    );
    let out = tick(&mut app, vec![own, foreign]);

    let applied: Vec<&JournalEntry> = out
        .journal
        .iter()
        .filter(|e| e.kind == "core.command_applied")
        .collect();
    let rejected: Vec<&JournalEntry> = out
        .journal
        .iter()
        .filter(|e| e.kind == "core.command_rejected")
        .collect();
    assert_eq!(applied.len(), 1, "{:?}", out.journal);
    assert_eq!(rejected.len(), 1, "{:?}", out.journal);

    let applied = applied[0];
    assert_eq!(applied.tick, Tick(0));
    assert_eq!(applied.phase, 0, "the Input phase");
    assert_eq!(applied.player, Some(PlayerId(0)));
    assert_eq!(applied.data["command"], "core.move");
    assert_eq!(applied.data["seq"], 1);
    assert_eq!(applied.data["units"], serde_json::json!([1]));
    assert_eq!(applied.data["queue"], false);

    let rejected = rejected[0];
    assert_eq!(rejected.data["command"], "core.move");
    assert_eq!(rejected.data["seq"], 2);
    assert_eq!(rejected.data["reason"], "core.no_valid_units");

    let order: Vec<&JournalEntry> = out
        .journal
        .iter()
        .filter(|e| e.kind == "core.order_changed")
        .collect();
    assert_eq!(order.len(), 1, "{:?}", out.journal);
    assert_eq!(order[0].sim_id, Some(SimId(1)));
    assert_eq!(order[0].data["orders"], 1);
    assert_eq!(order[0].data["current"], "move");
    assert_eq!(order[0].data["reason"], "command");

    let stop = command(&app, 0, 3, &[1], &StopCmd);
    let out = tick(&mut app, vec![stop]);
    let order = out
        .journal
        .iter()
        .find(|e| e.kind == "core.order_changed")
        .expect("the stop cleared #1's order");
    assert_eq!(order.data["orders"], 0);
    assert_eq!(order.data["current"], "none");
    assert_eq!(order.data["reason"], "stopped");
}

/// Despawns #1 at tick 2.
struct DespawnFeature;

impl SimFeature for DespawnFeature {
    fn id(&self) -> &'static str {
        "despawner"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.add_systems(
            LifecycleSet::Death,
            |clock: Res<SimClock>, mut despawns: MessageWriter<DespawnRequest>| {
                if clock.tick == Tick(2) {
                    despawns.write(DespawnRequest(SimId(1)));
                }
            },
        );
    }
}

#[test]
fn spawns_and_despawns_are_journaled() {
    let mut app = duel(vec![Arc::new(DespawnFeature)], journal_on());
    let spawn = command(
        &app,
        0,
        1,
        &[],
        &DebugSpawnCmd {
            unit: "core_test.tank".to_owned(),
            owner: PlayerId(1),
            pos: v(300, 400),
            heading: Angle::ZERO,
        },
    );
    let out = tick(&mut app, vec![spawn]);
    let spawned = out
        .journal
        .iter()
        .find(|e| e.kind == "core.unit_spawned")
        .unwrap_or_else(|| panic!("a spawn record in {:?}", out.journal));
    assert_eq!(spawned.sim_id, Some(SimId(3)));
    assert_eq!(spawned.player, Some(PlayerId(1)));
    assert_eq!(spawned.phase, 6, "the Lifecycle phase");
    assert_eq!(spawned.data["unit"], "core_test.tank");
    assert_eq!(spawned.data["owner"], 1);

    tick(&mut app, Vec::new());
    let out = tick(&mut app, Vec::new());
    assert_eq!(kinds(&out.journal), ["core.unit_despawned"]);
    assert_eq!(out.journal[0].sim_id, Some(SimId(1)));
    assert_eq!(out.journal[0].tick, Tick(2));
}

#[test]
fn journal_disabled_records_nothing() {
    let run = RunConfig {
        journal: Some(false),
        ..RunConfig::default()
    };
    let mut app = duel(Vec::new(), run);
    assert!(!app.world().resource::<Journal>().enabled());
    let own = command(
        &app,
        0,
        1,
        &[1],
        &MoveCmd {
            target: v(500, 500),
        },
    );
    let foreign = command(
        &app,
        0,
        2,
        &[2],
        &MoveCmd {
            target: v(500, 500),
        },
    );
    let out = tick(&mut app, vec![own, foreign]);
    assert!(out.journal.is_empty(), "{:?}", out.journal);
    assert_eq!(app.world().resource::<Journal>().len(), 0);
}

/// The duel's hash every tick for 40 ticks, with moves at ticks 0 and 20.
fn hash_stream(journal: bool) -> Vec<u64> {
    let run = RunConfig {
        hash_interval: Some(1),
        journal: Some(journal),
        ..RunConfig::default()
    };
    let mut app = duel(Vec::new(), run);
    let mut hashes = Vec::new();
    for t in 0..40 {
        let commands = match t {
            0 => vec![command(
                &app,
                0,
                1,
                &[1],
                &MoveCmd {
                    target: v(500, 500),
                },
            )],
            20 => vec![
                command(
                    &app,
                    0,
                    2,
                    &[1],
                    &MoveCmd {
                        target: v(200, 800),
                    },
                ),
                command(
                    &app,
                    1,
                    1,
                    &[2],
                    &MoveCmd {
                        target: v(100, 900),
                    },
                ),
            ],
            _ => Vec::new(),
        };
        let out = tick(&mut app, commands);
        assert_eq!(out.journal.is_empty(), !journal || t % 20 != 0);
        hashes.push(out.hash.expect("a hash every tick"));
    }
    hashes
}

#[test]
fn journal_does_not_change_state_hash() {
    assert_eq!(hash_stream(true), hash_stream(false));
}

/// Record `"test.ping"`: `n` is what the emitting system says.
#[derive(Debug, Serialize)]
struct Ping {
    n: u32,
}

impl JournalRecord for Ping {
    const KIND: &'static str = "test.ping";
}

#[derive(Debug, Serialize)]
struct Pong {}

impl JournalRecord for Pong {
    const KIND: &'static str = "test.pong";
}

/// Two unordered systems in `Think/Behaviour`, both recording every tick: `a` records
/// `ping {n: 1}` and `ping {n: 2}` for #2, then `ping {n: 3}` for #1; `b` records `pong` for
/// #2, then for #1. No (kind, sim id) gets records from both.
struct RecordingFeature;

fn record(journal: &mut Journal, tick: Tick, id: u32, rec: &impl JournalRecord) {
    journal.record(tick, Phase::Think, Some(SimId(id)), None, rec);
}

/// [`RecordingFeature`]'s system `a`.
fn record_pings(clock: Res<SimClock>, mut journal: ResMut<Journal>) {
    record(&mut journal, clock.tick, 2, &Ping { n: 1 });
    record(&mut journal, clock.tick, 2, &Ping { n: 2 });
    record(&mut journal, clock.tick, 1, &Ping { n: 3 });
}

/// [`RecordingFeature`]'s system `b`.
fn record_pongs(clock: Res<SimClock>, mut journal: ResMut<Journal>) {
    record(&mut journal, clock.tick, 2, &Pong {});
    record(&mut journal, clock.tick, 1, &Pong {});
}

impl SimFeature for RecordingFeature {
    fn id(&self) -> &'static str {
        "test"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.register_journal_record::<Ping>()
            .register_journal_record::<Pong>()
            .add_systems(ThinkSet::Behaviour, (record_pings, record_pongs));
    }
}

/// The journal of 10 ticks of the duel with [`RecordingFeature`], a move at tick 3 and
/// `shuffle_seed`.
fn recorded_journal(shuffle_seed: Option<u64>) -> Vec<JournalEntry> {
    let run = RunConfig {
        shuffle_seed,
        journal: Some(true),
        ..RunConfig::default()
    };
    let mut app = duel(vec![Arc::new(RecordingFeature)], run);
    let mut journal = Vec::new();
    for t in 0..10 {
        let commands = if t == 3 {
            vec![command(
                &app,
                0,
                1,
                &[1],
                &MoveCmd {
                    target: v(500, 500),
                },
            )]
        } else {
            Vec::new()
        };
        journal.extend(tick(&mut app, commands).journal);
    }
    journal
}

/// The order in which `shuffle_seed` runs [`RecordingFeature`]'s two systems.
fn recording_order(shuffle_seed: Option<u64>) -> Vec<&'static str> {
    let run = RunConfig {
        shuffle_seed,
        ..RunConfig::default()
    };
    let mut app = duel(vec![Arc::new(RecordingFeature)], run);
    system_order(&mut app)
        .lines()
        .filter_map(|line| {
            ["record_pings", "record_pongs"]
                .into_iter()
                .find(|name| line.ends_with(&format!("::{name}")))
        })
        .collect()
}

#[test]
fn journal_identical_across_shuffle_seeds() {
    // Not vacuous: some seed runs the two recording systems in another order than seed 1
    // (which seeds do depends on every system in the schedule, so it is searched for).
    let orders: Vec<(u64, Vec<&str>)> = (1..=8)
        .map(|seed| (seed, recording_order(Some(seed))))
        .collect();
    assert!(
        orders.iter().all(|(_, order)| order.len() == 2),
        "{orders:?}"
    );
    let other = orders
        .iter()
        .find(|(_, order)| order != &orders[0].1)
        .map(|&(seed, _)| seed)
        .unwrap_or_else(|| {
            panic!("seeds 1-8 must order the recording systems differently: {orders:?}")
        });

    let reference = recorded_journal(Some(1));
    // Five test records a tick, plus the move's four: applied, order changed, path request
    // and path result.
    assert_eq!(reference.len(), 10 * 5 + 4, "{reference:?}");
    for seed in [2, 3, other] {
        assert_eq!(
            recorded_journal(Some(seed)),
            reference,
            "shuffle seed {seed}"
        );
    }
}

#[test]
fn journal_entries_sorted() {
    let journal = recorded_journal(None);
    let mut per_tick: BTreeMap<Tick, Vec<&JournalEntry>> = BTreeMap::new();
    for entry in &journal {
        per_tick.entry(entry.tick).or_default().push(entry);
    }
    assert_eq!(per_tick.len(), 10);
    for (tick, entries) in &per_tick {
        let keys: Vec<_> = entries
            .iter()
            .map(|e| (e.phase, e.kind.as_str(), e.sim_id, e.emitted))
            .collect();
        assert!(keys.is_sorted(), "tick {}: {keys:?}", tick.0);
    }

    // Tick 3: the move's records (Input) first, then its path result and the test systems'
    // records (Think) by kind and sim id; #2's two pings keep their emission order.
    let tick3 = &per_tick[&Tick(3)];
    let keys: Vec<(u8, &str, Option<SimId>, u32)> = tick3
        .iter()
        .map(|e| (e.phase, e.kind.as_str(), e.sim_id, e.emitted))
        .collect();
    assert_eq!(
        keys,
        [
            (0, "core.command_applied", None, 0),
            (0, "core.order_changed", Some(SimId(1)), 0),
            (0, "core.path_request", Some(SimId(1)), 0),
            (1, "core.path_result", Some(SimId(1)), 0),
            (1, "test.ping", Some(SimId(1)), 0),
            (1, "test.ping", Some(SimId(2)), 0),
            (1, "test.ping", Some(SimId(2)), 1),
            (1, "test.pong", Some(SimId(1)), 0),
            (1, "test.pong", Some(SimId(2)), 0),
        ]
    );
    let pings: Vec<&serde_json::Value> = tick3[4..7].iter().map(|e| &e.data["n"]).collect();
    assert_eq!(pings, [3, 1, 2]);
}

/// The string and integer fields of a span or event, by name.
#[derive(Default)]
struct Fields(BTreeMap<String, String>);

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}

/// The `tick` of a `sim_tick` span.
struct SpanTick(String);

/// An event: its target, fields, and the `tick` of the `sim_tick` span entered when it was
/// emitted.
#[derive(Debug, Clone)]
struct Captured {
    target: String,
    fields: BTreeMap<String, String>,
    span_tick: Option<String>,
}

/// A layer that keeps every event, and the stack of entered `sim_tick` spans (the tests run
/// the sim on one thread). Entered, not parent: bevy_ecs makes each system's span when the
/// system is built, outside every tick, so a system's events have no `sim_tick` ancestor.
#[derive(Clone, Default)]
struct Capture {
    events: Arc<Mutex<Vec<Captured>>>,
    entered: Arc<Mutex<Vec<(Id, String)>>>,
}

impl Capture {
    fn events(&self) -> Vec<Captured> {
        self.events.lock().expect("not poisoned").clone()
    }
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Capture {
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if attrs.metadata().name() != "sim_tick" {
            return;
        }
        let mut fields = Fields::default();
        attrs.record(&mut fields);
        if let (Some(tick), Some(span)) = (fields.0.remove("tick"), ctx.span(id)) {
            span.extensions_mut().insert(SpanTick(tick));
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let tick = ctx
            .span(id)
            .and_then(|span| span.extensions().get::<SpanTick>().map(|t| t.0.clone()));
        if let Some(tick) = tick {
            self.entered
                .lock()
                .expect("not poisoned")
                .push((id.clone(), tick));
        }
    }

    fn on_exit(&self, id: &Id, _ctx: Context<'_, S>) {
        let mut entered = self.entered.lock().expect("not poisoned");
        if let Some(at) = entered.iter().rposition(|(entered, _)| entered == id) {
            entered.remove(at);
        }
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let span_tick = self
            .entered
            .lock()
            .expect("not poisoned")
            .last()
            .map(|(_, tick)| tick.clone());
        self.events.lock().expect("not poisoned").push(Captured {
            target: event.metadata().target().to_owned(),
            fields: fields.0,
            span_tick,
        });
    }
}

/// Run `f` with a subscriber that captures every event; returns the events.
fn capture(f: impl FnOnce()) -> Vec<Captured> {
    let layer = Capture::default();
    let subscriber = tracing_subscriber::registry().with(layer.clone());
    tracing::subscriber::with_default(subscriber, f);
    layer.events()
}

#[test]
fn phase_markers_emit_16_events_per_tick() {
    let events = capture(|| {
        let run = RunConfig {
            phase_markers: true,
            ..RunConfig::default()
        };
        let mut app = duel(Vec::new(), run);
        for _ in 0..3 {
            tick(&mut app, Vec::new());
        }
    });
    let phases = [
        "Input",
        "Think",
        "Act",
        "Resolve",
        "Economy",
        "Perceive",
        "Lifecycle",
        "Publish",
    ];
    let expected: Vec<(String, &str)> = phases
        .iter()
        .flat_map(|&phase| [(phase.to_owned(), "start"), (phase.to_owned(), "end")])
        .collect();
    for t in 0..3 {
        let markers: Vec<(String, &str)> = events
            .iter()
            .filter(|e| e.target == PHASE_TARGET && e.span_tick.as_deref() == Some(&t.to_string()))
            .map(|e| {
                let edge = match e.fields["edge"].as_str() {
                    "start" => "start",
                    "end" => "end",
                    other => panic!("edge {other}"),
                };
                (e.fields["phase"].clone(), edge)
            })
            .collect();
        assert_eq!(markers, expected, "tick {t}");
    }
    assert_eq!(
        events.iter().filter(|e| e.target == PHASE_TARGET).count(),
        3 * 16
    );

    let without = capture(|| {
        let mut app = duel(Vec::new(), RunConfig::default());
        tick(&mut app, Vec::new());
    });
    assert!(
        without.iter().all(|e| e.target != PHASE_TARGET),
        "no markers unless asked for"
    );
}

/// A system that logs the clock's tick every tick.
struct ProbeFeature;

impl SimFeature for ProbeFeature {
    fn id(&self) -> &'static str {
        "probe"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.add_systems(ThinkSet::Orders, |clock: Res<SimClock>| {
            tracing::info!(target: "observe_probe", clock = clock.tick.0, "probe");
        });
    }
}

#[test]
fn tick_span_carries_tick() {
    let events = capture(|| {
        let mut app = duel(vec![Arc::new(ProbeFeature)], RunConfig::default());
        for _ in 0..10 {
            tick(&mut app, Vec::new());
        }
    });
    let probes: Vec<(String, Option<String>)> = events
        .iter()
        .filter(|e| e.target == "observe_probe")
        .map(|e| (e.fields["clock"].clone(), e.span_tick.clone()))
        .collect();
    assert_eq!(probes.len(), 10);
    for (clock, span_tick) in &probes {
        assert_eq!(span_tick.as_ref(), Some(clock), "{probes:?}");
    }
    assert!(probes.iter().any(|(clock, _)| clock == "7"));
}

#[test]
fn journal_since_query() {
    let app = duel(vec![Arc::new(RecordingFeature)], journal_on());
    let mut runner = SimRunner::new(app);
    let mut driver = TestDriver::new();
    runner.run(&mut driver, 20);
    assert_eq!(driver.next_tick(), Tick(20));

    let SimReply::Journal(entries) = runner.query(SimQuery::JournalSince {
        tick: Tick(10),
        limit: 5,
    }) else {
        panic!("the journal answers");
    };
    assert_eq!(entries.len(), 5, "{entries:?}");
    assert!(entries.iter().all(|e| e.tick >= Tick(10)), "{entries:?}");
    assert_eq!(entries[0].tick, Tick(10), "oldest first");

    let SimReply::Journal(all) = runner.query(SimQuery::JournalSince {
        tick: Tick(0),
        limit: u32::MAX,
    }) else {
        panic!("the journal answers");
    };
    assert_eq!(all.len(), 20 * 5);
}
