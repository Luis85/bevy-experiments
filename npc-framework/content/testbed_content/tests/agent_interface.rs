//! The agent interface on the Actor Testbed, after the first fresh-agent
//! play-test (`docs/_notes/agent-playtest-friction.md`):
//!
//! - the perception gate of affordances: nothing inside the closed store is
//!   offered until it is seen, a forgotten target disappears, and a far
//!   perceived target is offered with `why_not: "too far: .."` (no hidden
//!   radius); `world/state` stays omniscient;
//! - observation text: item kinds, positions, no memory of carried items,
//!   no perception rolls in recent events;
//! - the `interact` verb (walk into reach and act, one act) and
//!   `execute_orders` ending `done`;
//! - success odds on skill-checked affordances; tool targets' yield and
//!   uses; `list_units` owners.

use bevy_app::App;
use npc_framework::agent_io;
use npc_framework::prelude::*;
use serde_json::{Value, json};
use testbed_content::{TestbedIds, try_new_sim_app_with};

fn journal_plugin(app: &mut App) {
    app.add_plugins(JournalPlugin {
        verbose: false,
        capacity: 400_000,
    });
}

fn finish(mut app: App) -> App {
    app.finish();
    app.cleanup();
    app
}

fn app(name: &str) -> App {
    finish(try_new_sim_app_with(1, name, journal_plugin).unwrap_or_else(|e| panic!("{name}: {e}")))
}

fn inline(doc: Value) -> App {
    let doc = agent_io::ScenarioDoc::from_value(doc).unwrap();
    let loaded = testbed_content::scenario::load_with(&doc, 1, journal_plugin).unwrap();
    finish(loaded.app)
}

fn id(app: &App, name: &str) -> SimId {
    app.world().resource::<TestbedIds>().id(name)
}

fn call(app: &mut App, method: &str, params: Value) -> Value {
    agent_io::api::call(app.world_mut(), method, Some(params))
        .unwrap_or_else(|e| panic!("{method}: {e:?}"))
}

fn attach(app: &mut App, unit: SimId) {
    call(
        app,
        "agent/attach",
        json!({"unit": unit.0, "controller": "t", "mode": "Direct"}),
    );
    run_ticks(app.world_mut(), 2);
}

fn observe(app: &mut App, unit: SimId) -> Value {
    call(
        app,
        "agent/observe",
        json!({"unit": unit.0, "controller": "t"}),
    )
}

fn text(app: &mut App, unit: SimId) -> String {
    call(
        app,
        "agent/observe",
        json!({"unit": unit.0, "controller": "t", "format": "text"}),
    )["text"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn targets(obs: &Value) -> Vec<u64> {
    obs["affordances"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["target"].as_u64())
        .collect()
}

fn affordance(obs: &Value, id: &str) -> Option<Value> {
    obs["affordances"]
        .as_array()?
        .iter()
        .find(|a| a["id"] == json!(id))
        .cloned()
}

fn act(app: &mut App, unit: SimId, params: Value) -> Value {
    let mut p = json!({"unit": unit.0, "controller": "t"});
    for (k, v) in params.as_object().unwrap() {
        p[k] = v.clone();
    }
    let r = call(app, "agent/act", p);
    assert_eq!(r["accepted"], json!(true), "{r}");
    r
}

/// Runs until the unit is idle (its action queue, orders and interaction
/// done), at most `limit` ticks.
fn run_until_idle(app: &mut App, unit: SimId, limit: u64) {
    run_ticks(app.world_mut(), 1);
    for _ in 0..limit {
        if agent_io::api::busy_reason(app.world(), unit).is_none() {
            return;
        }
        run_ticks(app.world_mut(), 1);
    }
    panic!(
        "unit {unit:?} still busy after {limit} ticks: {:?}",
        agent_io::api::busy_reason(app.world(), unit)
    );
}

fn events(app: &mut App, unit: SimId, kinds: &str) -> Vec<Value> {
    call(
        app,
        "agent/events",
        json!({"unit": unit.0, "kinds": kinds, "limit": 10000}),
    )["events"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn nothing_inside_the_closed_store_is_offered_until_seen() {
    let mut app = app("smithy");
    let smith = id(&app, "smith");
    let (key, door, pickaxe, hammer) = (
        id(&app, "store_key"),
        id(&app, "store_door"),
        id(&app, "pickaxe"),
        id(&app, "hammer"),
    );
    attach(&mut app, smith);
    run_ticks(app.world_mut(), 8);
    let obs = observe(&mut app, smith);
    assert_eq!(obs["omniscient"], json!(false));
    let offered = targets(&obs);
    for hidden in [pickaxe, hammer] {
        assert!(
            !offered.contains(&u64::from(hidden.0)),
            "#{} is inside the closed store: {}",
            hidden.0,
            obs["affordances"]
        );
    }
    assert!(offered.contains(&u64::from(door.0)), "the door is seen");
    // world/state (debug tooling) still shows everything.
    let state = call(&mut app, "world/state", json!({}));
    let all: Vec<u64> = state["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["sim_id"].as_u64())
        .collect();
    assert!(all.contains(&u64::from(pickaxe.0)));

    // Fetch the key, unlock and open the store (one `interact` act each,
    // walking included), walk in: now the pickaxe is offered.
    for (verb, target) in [("pick_up", key), ("unlock", door), ("open", door)] {
        act(
            &mut app,
            smith,
            json!({"verb": "interact", "target": target.0, "args": {"verb": verb}}),
        );
        run_until_idle(&mut app, smith, 1500);
    }
    let done: Vec<String> = events(&mut app, smith, "OrderDone")
        .iter()
        .map(|e| e["event"]["order"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(done, ["Interact", "Interact", "Interact"]);
    // execute_orders (queued by `interact`) ended `done`, never `cancelled`.
    let statuses: Vec<String> = events(&mut app, smith, "ExternalAction")
        .iter()
        .map(|e| e["event"]["status"].as_str().unwrap().to_owned())
        .collect();
    assert!(!statuses.iter().any(|s| s == "cancelled"), "{statuses:?}");
    assert_eq!(statuses.iter().filter(|s| *s == "done").count(), 3);

    act(
        &mut app,
        smith,
        json!({"steps": [
            {"verb": "order", "args": {"order": {"type": "Move", "pos": [4500, 6500]}}},
            {"verb": "execute_orders"},
        ]}),
    );
    run_until_idle(&mut app, smith, 1500);
    run_ticks(app.world_mut(), 8);
    let obs = observe(&mut app, smith);
    assert!(
        affordance(&obs, &format!("pick_up@{}", pickaxe.0)).is_some_and(|a| a["ok"] == json!(true)),
        "inside the store the pickaxe is offered: {}",
        obs["affordances"]
    );
    // The carried key is not "remembered", and items are named by kind.
    let t = text(&mut app, smith);
    assert!(
        !t.contains(&format!("store_key #{} last seen", key.0)),
        "{t}"
    );
    assert!(t.contains(&format!("pickaxe #{} at (", pickaxe.0)), "{t}");
    assert!(!t.contains("perception.detect"), "{t}");
}

#[test]
fn a_far_perceived_target_is_offered_and_a_forgotten_one_disappears() {
    let mut app = inline(json!({
        "version": 1, "content": "testbed", "seed": 1,
        "world": {"layout": "open", "furnish": false, "detection": "automatic", "stagger": 1,
                  "items": [{"kind": "book", "pos": [0, 12000], "name": "book"}]},
        "actors": [{"kind": "unit", "name": "u", "template": "smith", "pos": [0, 0],
                    "facing": [0, 1], "fixed_facing": true}],
    }));
    let (u, book) = (id(&app, "u"), id(&app, "book"));
    attach(&mut app, u);
    run_ticks(app.world_mut(), 3);
    let obs = observe(&mut app, u);
    let sensed = &obs["sections"]["perception"]["sensed"];
    assert!(
        sensed
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == json!(book.0) && s["name"] == json!(format!("book #{}", book.0))),
        "{sensed}"
    );
    // 12 m is beyond the old 10 m affordance radius: offered all the same.
    let pick = affordance(&obs, &format!("pick_up@{}", book.0)).expect("the far book is offered");
    assert_eq!(pick["ok"], json!(false));
    assert!(
        pick["why_not"]
            .as_str()
            .unwrap()
            .starts_with("too far: 12.0 m"),
        "{pick}"
    );
    let t = text(&mut app, u);
    assert!(
        t.contains(&format!(
            "book #{} at (0.00 m, 12.00 m), 12.00 m away (vision)",
            book.0
        )),
        "{t}"
    );
    // Walk out of sight (vision 15 m) and wait until it is forgotten.
    act(
        &mut app,
        u,
        json!({"steps": [
            {"verb": "order", "args": {"order": {"type": "Move", "pos": [0, -9000]}}},
            {"verb": "execute_orders"},
        ]}),
    );
    run_until_idle(&mut app, u, 1000);
    let lost = events(&mut app, u, "Lost");
    assert!(
        lost.iter()
            .any(|e| e["event"]["target"] == json!(book.0)
                && e["event"]["forgotten"] == json!(false)),
        "{lost:?}"
    );
    // Remembered (with its last-known position): still offered.
    let obs = observe(&mut app, u);
    assert!(targets(&obs).contains(&u64::from(book.0)));
    run_ticks(app.world_mut(), 260);
    let lost = events(&mut app, u, "Lost");
    assert!(
        lost.iter().any(
            |e| e["event"]["target"] == json!(book.0) && e["event"]["forgotten"] == json!(true)
        ),
        "{lost:?}"
    );
    let obs = observe(&mut app, u);
    assert!(
        !targets(&obs).contains(&u64::from(book.0)),
        "forgotten: no longer offered: {}",
        obs["affordances"]
    );
}

#[test]
fn skill_checked_affordances_carry_their_odds_and_tool_targets_their_yield() {
    let mut app = inline(json!({
        "version": 1, "content": "testbed", "seed": 1,
        "world": {"layout": "compound", "furnish": true, "store_locked": false,
                  "detection": "automatic", "stagger": 1},
        "actors": [{"kind": "unit", "name": "smith", "template": "smith", "pos": [8500, -4500],
                    "facing": [0, -1], "fixed_facing": true}],
    }));
    let smith = id(&app, "smith");
    let ore = id(&app, "ore_rock");
    attach(&mut app, smith);
    run_ticks(app.world_mut(), 3);
    let obs = observe(&mut app, smith);
    let mine = affordance(&obs, &format!("use_tool_on@{}", ore.0))
        .unwrap_or_else(|| panic!("the ore rock is seen: {}", obs["affordances"]));
    let label = mine["label"].as_str().unwrap();
    assert!(label.contains("needs tool:pickaxe; yields"), "{label}");
    assert!(label.contains("uses left"), "{label}");
    let check = &mine["check"];
    assert_eq!(check["skill"], json!("Mining"), "{mine}");
    assert_eq!(check["dice"], json!("3d6"));
    let target = check["target_number"].as_i64().unwrap();
    let odds = check["odds_permille"].as_u64().unwrap();
    assert!((1..1000).contains(&odds), "{check}");
    assert!(
        label.contains(&format!("[Mining {target} vs 3d6: ~{}%", odds / 10)),
        "{label}"
    );
    // A unit with no Mining sees low odds (its default) or "cannot attempt".
    let units = call(&mut app, "agent/list_units", json!({"kind": "actor"}));
    assert_eq!(units["units"][0]["owner"], json!(1), "{units}");
}

#[test]
fn recent_events_keep_the_agents_own_outcomes_and_drop_perception_rolls() {
    let mut app = app("perception");
    let sharp = id(&app, "sharp");
    attach(&mut app, sharp);
    act(
        &mut app,
        sharp,
        json!({"verb": "wait", "args": {"ticks": 5}}),
    );
    run_ticks(app.world_mut(), 200);
    let obs = call(
        &mut app,
        "agent/observe",
        json!({"unit": sharp.0, "controller": "t", "events": 4}),
    );
    let recent = obs["recent_events"].as_array().unwrap();
    assert!(
        recent
            .iter()
            .all(|e| e["event"]["purpose"] != json!("perception.detect")),
        "{recent:?}"
    );
    assert!(
        recent
            .iter()
            .any(|e| e["event"]["type"] == json!("ExternalAction")
                && e["event"]["status"] == json!("done")),
        "{recent:?}"
    );
    // The rolls are still there when asked for.
    assert!(!events(&mut app, sharp, "Roll").is_empty());
}

/// An open field: actor `u` at the origin facing +z and a book 1 m ahead.
fn open_field() -> App {
    inline(json!({
        "version": 1, "content": "testbed", "seed": 1,
        "world": {"layout": "open", "furnish": false, "detection": "automatic", "stagger": 1,
                  "items": [{"kind": "book", "pos": [0, 1000], "name": "book"}]},
        "actors": [{"kind": "unit", "name": "u", "template": "smith", "pos": [0, 0],
                    "facing": [0, 1], "fixed_facing": true}],
    }))
}

/// `agent/act_and_wait` driven like a transport would: act now, then one
/// tick at a time until the wait is over (at most `limit` ticks).
fn act_and_wait(app: &mut App, unit: SimId, params: Value, limit: u64) -> Value {
    let mut p = json!({"unit": unit.0, "controller": "t"});
    for (k, v) in params.as_object().unwrap() {
        p[k] = v.clone();
    }
    let w = agent_io::api::act_and_wait_start(app.world_mut(), Some(p)).expect("starts");
    for _ in 0..limit {
        if agent_io::api::act_wait_ready(app.world_mut(), &w) {
            return agent_io::api::act_wait_result(app.world_mut(), &w, false).expect("result");
        }
        run_ticks(app.world_mut(), 1);
    }
    panic!("act_and_wait not done after {limit} ticks");
}

#[test]
fn act_and_wait_returns_events_and_observation() {
    let mut app = open_field();
    let mut twin = open_field();
    let u = id(&app, "u");
    attach(&mut app, u);
    attach(&mut twin, u);
    let move_to = json!({"steps": [
        {"verb": "order", "args": {"order": {"type": "Move", "pos": [0, -3000]}}},
        {"verb": "execute_orders"},
    ]});
    let start = app.world().resource::<SimClock>().tick;
    let r = act_and_wait(&mut app, u, move_to.clone(), 1500);
    assert_eq!(r["act"]["accepted"], json!(true), "{r}");
    assert_eq!(r["events"]["idle"], json!(true), "{r}");
    assert_eq!(r["events"]["timed_out"], json!(false), "{r}");
    let done: Vec<&str> = r["events"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == json!("OrderDone"))
        .map(|e| e["event"]["order"].as_str().unwrap())
        .collect();
    assert_eq!(done, ["Move"], "{}", r["events"]);
    let pos = &r["observation"]["sections"]["self"]["pos"];
    let (x, z) = (pos[0].as_i64().unwrap(), pos[1].as_i64().unwrap());
    assert!(
        x.abs() < 600 && (z + 3000).abs() < 600,
        "at the new position: {pos}"
    );
    assert_eq!(
        r["observation"]["changes"]["orders"],
        json!([{"order": "Move", "result": "done"}]),
        "the observation covers the act: {}",
        r["observation"]["changes"]
    );
    // The same act through plain agent/act, run for as long: the same
    // world (act_and_wait and since-last-look observations are host-side).
    let ran = app.world().resource::<SimClock>().tick - start;
    act(&mut twin, u, move_to);
    run_ticks(twin.world_mut(), ran);
    assert_eq!(
        npc_framework::kernel::hash::state_hash(app.world_mut()),
        npc_framework::kernel::hash::state_hash(twin.world_mut())
    );
}

#[test]
fn observe_since_reports_changes() {
    let mut app = open_field();
    let (u, book) = (id(&app, "u"), id(&app, "book"));
    attach(&mut app, u);
    run_ticks(app.world_mut(), 3);
    let seen = observe(&mut app, u)["seq"].as_u64().unwrap();
    act(
        &mut app,
        u,
        json!({"verb": "interact", "target": book.0, "args": {"verb": "pick_up"}}),
    );
    run_until_idle(&mut app, u, 1500);
    let obs = call(
        &mut app,
        "agent/observe",
        json!({"unit": u.0, "controller": "t", "since_seq": seen}),
    );
    let ch = &obs["changes"];
    assert_eq!(ch["truncated"], json!(false), "{ch}");
    let gained: Vec<u64> = ch["inventory_gained"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["item"].as_u64().unwrap())
        .collect();
    assert_eq!(gained, [u64::from(book.0)], "{ch}");
    assert_eq!(ch["inventory_gained"][0]["kind"], json!("book"), "{ch}");
    assert!(
        ch["interactions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["verb"] == json!("pick_up") && i["outcome"] == json!("success")),
        "{ch}"
    );
    assert!(
        ch["orders"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["order"] == json!("Interact") && o["result"] == json!("done")),
        "{ch}"
    );
    let t = call(
        &mut app,
        "agent/observe",
        json!({"unit": u.0, "controller": "t", "since_seq": seen, "format": "text"}),
    )["text"]
        .as_str()
        .unwrap()
        .to_owned();
    let first = t.lines().next().unwrap();
    assert!(first.starts_with("Since your last look: "), "{t}");
    assert!(
        first.contains(&format!("gained book #{}", book.0)),
        "{first}"
    );
    // Carried, so no longer "sensed": not news.
    assert!(!first.contains("no longer sense"), "{first}");
    assert_eq!(ch["lost"], json!([]), "{ch}");
}
