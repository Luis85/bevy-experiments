//! Named debug queries (ADR-0015): `SimBuilder::register_debug_query`, answered between ticks
//! through `SimQuery::Debug`.

use std::path::PathBuf;
use std::sync::Arc;

use bevy_ecs::world::World;
use fl_protocol::{SimId, Tick};
use fl_sim::{
    MatchSetup, RunConfig, SimBuilder, SimFeature, SimRunner, build_sim_app, compute_state_hash,
    load_content, setup_scenario,
};
use fl_view::{SimQuery, SimReply, TickInput};
use serde_json::{Value, json};

fn core_mod() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_mod")
}

/// A feature that registers the debug query `query` (if any).
struct QueryFeature {
    id: &'static str,
    deps: &'static [&'static str],
    query: Option<(&'static str, fl_sim::DebugQueryFn)>,
}

impl SimFeature for QueryFeature {
    fn id(&self) -> &'static str {
        self.id
    }

    fn version(&self) -> u32 {
        1
    }

    fn deps(&self) -> &'static [&'static str] {
        self.deps
    }

    fn build(&self, sim: &mut SimBuilder) {
        if let Some((name, query)) = self.query {
            sim.register_debug_query(name, query);
        }
    }
}

fn feature(
    id: &'static str,
    deps: &'static [&'static str],
    query: Option<(&'static str, fl_sim::DebugQueryFn)>,
) -> Arc<dyn SimFeature> {
    Arc::new(QueryFeature { id, deps, query })
}

fn bad_args(_: &World, _: &Value) -> Result<Value, String> {
    Err("bad args".to_owned())
}

fn echo(_: &World, args: &Value) -> Result<Value, String> {
    Ok(json!({ "args": args }))
}

/// The duel of core_mod with `features`, hashing every tick.
fn duel_with(features: &[Arc<dyn SimFeature>]) -> SimRunner {
    let content = load_content(features, &[core_mod()]).expect("core_mod loads");
    let setup = MatchSetup {
        seed: 7,
        cheats_allowed: true,
    };
    let run = RunConfig {
        hash_interval: Some(1),
        ..RunConfig::default()
    };
    let mut app = build_sim_app(features, content, setup, run).expect("the sim builds");
    let ids = setup_scenario(app.world_mut(), "core_test.duel").expect("duel sets up");
    assert_eq!(ids, [SimId(1), SimId(2)]);
    SimRunner::new(app)
}

fn debug(name: &str, args: Value) -> SimQuery {
    SimQuery::Debug {
        name: name.to_owned(),
        args,
    }
}

fn idle_tick(runner: &mut SimRunner) {
    let tick = runner.next_tick();
    runner.step(TickInput {
        tick,
        commands: Vec::new(),
    });
}

#[test]
fn debug_queries_listed() {
    // "aaa.first" sorts before every "core." name, so an unsorted list would show.
    let features = [
        feature("test", &[], Some(("test.echo", echo))),
        feature("aaa", &[], Some(("aaa.first", echo))),
    ];
    let mut runner = duel_with(&features);
    let SimReply::Debug(reply) = runner.query(debug("core.debug_queries", Value::Null)) else {
        panic!("core.debug_queries answers SimReply::Debug");
    };
    let names: Vec<&str> = reply["queries"]
        .as_array()
        .expect("a \"queries\" array")
        .iter()
        .map(|name| name.as_str().expect("a name"))
        .collect();
    assert!(names.contains(&"core.debug_queries"), "{reply:#}");
    assert!(names.contains(&"test.echo"), "{reply:#}");
    assert_eq!(names.first(), Some(&"aaa.first"), "{names:?}");
    assert!(names.is_sorted(), "{names:?}");

    // A feature's query gets its args.
    let reply = runner.query(debug("test.echo", json!({ "x": 3 })));
    assert_eq!(reply, SimReply::Debug(json!({ "args": { "x": 3 } })));
}

#[test]
fn unknown_debug_query_is_error() {
    let mut runner = duel_with(&[]);
    assert_eq!(
        runner.query(debug("nope", Value::Null)),
        SimReply::Error("unknown debug query 'nope'".to_owned())
    );
}

#[test]
#[should_panic(expected = "registered by 'test' and 'other'")]
fn duplicate_debug_query_panics_naming_both() {
    // `other` depends on `test`, so `test` registers first.
    let features = [
        feature("test", &[], Some(("test.q", echo))),
        feature("other", &["test"], Some(("test.q", echo))),
    ];
    let _ = duel_with(&features);
}

#[test]
fn debug_query_does_not_change_state() {
    let features = [feature("test", &[], Some(("test.echo", echo)))];
    let mut runner = duel_with(&features);
    idle_tick(&mut runner);
    let before = compute_state_hash(runner.world());
    for n in 0..10 {
        let name = if n % 2 == 0 {
            "core.debug_queries"
        } else {
            "test.echo"
        };
        assert!(matches!(
            runner.query(debug(name, json!(n))),
            SimReply::Debug(_)
        ));
    }
    assert_eq!(compute_state_hash(runner.world()), before);
    assert_eq!(runner.next_tick(), Tick(1));
}

#[test]
fn failing_query_reports_its_message() {
    let features = [feature("test", &[], Some(("test.bad", bad_args)))];
    let mut runner = duel_with(&features);
    assert_eq!(
        runner.query(debug("test.bad", Value::Null)),
        SimReply::Error("bad args".to_owned())
    );
}
