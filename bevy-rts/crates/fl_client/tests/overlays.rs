//! Overlay tests: the decoders of the debug query answers, the polling of the overlays' queries
//! and the toggles.
//!
//! The runner is a bare `fl_view::runner_channel` end held by the test and read with
//! `try_recv`, so nothing is threaded and no reply arrives unless the test sends it; a reply
//! sent after a frame is read by the next one.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use fl_client::camera::RtsCamera;
use fl_client::debug::{
    ConsoleState, DebugCommandsPlugin, LoadRequest, LoadedMatch, OverlayRegistry, ScenarioLoader,
    replace_match,
};
use fl_client::driver::LocalDriver;
use fl_client::link::{PendingReplies, RunnerLink, send_query};
use fl_client::map_view::MapView;
use fl_client::overlays::{
    BlockedCells, ClusterView, OverlayData, OverlayState, OverlaysPlugin, POLL_FRAMES,
    blocked_near, decode_clusters, decode_passability, decode_paths, grid_lines,
};
use fl_client::selection::Selection;
use fl_client::view::SimViewPlugin;
use fl_math::Fx;
use fl_protocol::{MatchHeader, SimId, StringTables, Tick};
use fl_view::{FromRunner, RunnerEnd, SimQuery, SimReply, ToRunner, runner_channel};
use serde_json::{Value, json};

fn raw(metres: i32) -> i64 {
    Fx::from_int(metres).raw()
}

fn header() -> MatchHeader {
    MatchHeader {
        format_version: fl_protocol::FORMAT_VERSION,
        build_id: "test".to_owned(),
        sim_version: 1,
        features: Vec::new(),
        mods: Vec::new(),
        rules_hash: 0,
        cheats_allowed: true,
        seed: 1,
        string_tables: StringTables::default(),
    }
}

/// A headless client with a paused driver and the test holding the runner's end.
fn client() -> (App, RunnerEnd) {
    let (port, end) = runner_channel();
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        SimViewPlugin,
        DebugCommandsPlugin,
        OverlaysPlugin,
    ))
    .insert_resource(RunnerLink {
        port,
        header: header(),
    })
    .init_resource::<Selection>();
    let mut driver = LocalDriver::new(Tick(0));
    driver.pause();
    app.insert_resource(driver);
    (app, end)
}

fn select(app: &mut App, ids: &[u32]) {
    app.world_mut().resource_mut::<Selection>().0 = ids.iter().copied().map(SimId).collect();
}

/// The debug queries the runner has received, as `(name, args)`.
fn queries(end: &RunnerEnd) -> Vec<(String, Value)> {
    end.rx
        .try_iter()
        .filter_map(|message| match message {
            ToRunner::Query {
                query: SimQuery::Debug { name, args },
                ..
            } => Some((name, args)),
            _ => None,
        })
        .collect()
}

/// Updates until the runner has received a query; returns it.
fn first_query(app: &mut App, end: &RunnerEnd) -> (u64, String, Value) {
    for _ in 0..=POLL_FRAMES {
        app.update();
        if let Ok(ToRunner::Query {
            id,
            query: SimQuery::Debug { name, args },
        }) = end.rx.try_recv()
        {
            return (id, name, args);
        }
    }
    panic!("no debug query within {POLL_FRAMES} frames");
}

fn toggle(app: &mut App, name: &str) -> bool {
    OverlayRegistry::toggle(app.world_mut(), name).expect("a registered overlay")
}

#[test]
fn decode_paths_json() {
    let v = json!({
        "3": {
            "state": "following",
            "goal": [raw(10), raw(20)],
            "corners": [[raw(10), raw(20)], [raw(-5), raw(7)]],
            "next": 0,
        },
        "9": {"corners": []},
    });
    let paths = decode_paths(&v).unwrap();
    assert_eq!(
        paths,
        [
            (SimId(3), vec![Vec2::new(10.0, 20.0), Vec2::new(-5.0, 7.0)]),
            (SimId(9), Vec::new()),
        ]
        .into_iter()
        .collect()
    );
    assert!(decode_paths(&json!([1])).is_err());
    assert!(decode_paths(&json!({"x": {"corners": []}})).is_err());
    assert!(decode_paths(&json!({"3": {}})).is_err());
}

#[test]
fn decode_passability_runs() {
    let v = json!({"cells": [4, 2], "version": 7, "blocked": [[0, 2], [6, 1]]});
    assert_eq!(
        decode_passability(&v).unwrap(),
        BlockedCells {
            cells: UVec2::new(4, 2),
            version: 7,
            runs: vec![(0, 2), (6, 1)],
        }
    );
    assert!(decode_passability(&json!({"cells": [4, 2]})).is_err());
    assert!(decode_passability(&json!({"cells": [4, 2], "version": 1, "blocked": [[1]]})).is_err());
}

#[test]
fn decode_clusters_json() {
    let v = json!({
        "cluster_cells": 8,
        "clusters": [4, 4],
        "entrances": [[7, 3], [8, 3]],
        "nodes": 2,
    });
    assert_eq!(
        decode_clusters(&v).unwrap(),
        ClusterView {
            cluster_cells: 8,
            clusters: UVec2::new(4, 4),
            entrances: vec![UVec2::new(7, 3), UVec2::new(8, 3)],
        }
    );
    assert!(decode_clusters(&json!({"cluster_cells": 8})).is_err());
}

#[test]
fn paths_overlay_polls_selected_units() {
    let (mut app, end) = client();
    select(&mut app, &[2, 1]);
    assert!(toggle(&mut app, "paths"));
    assert!(app.world().resource::<OverlayState>().paths);
    let (_, name, args) = first_query(&mut app, &end);
    assert_eq!(
        (name.as_str(), args),
        ("core.paths", json!({"units": [1, 2]}))
    );
}

#[test]
fn paths_overlay_without_selection_sends_nothing() {
    let (mut app, end) = client();
    toggle(&mut app, "paths");
    for _ in 0..=2 * POLL_FRAMES {
        app.update();
    }
    assert!(queries(&end).is_empty());
}

#[test]
fn passability_follows_selected_unit() {
    let (mut app, end) = client();
    select(&mut app, &[1]);
    toggle(&mut app, "passability");
    let (_, name, args) = first_query(&mut app, &end);
    assert_eq!(
        (name.as_str(), args),
        ("core.passability", json!({"unit": 1}))
    );
}

#[test]
fn passability_without_selection_asks_for_tracked() {
    let (mut app, end) = client();
    toggle(&mut app, "passability");
    let (_, name, args) = first_query(&mut app, &end);
    assert_eq!(name, "core.passability");
    assert_eq!(args, json!({"class": "Tracked", "size": 1}));
}

#[test]
fn cells_overlay_asks_for_the_clusters_of_the_selected_unit() {
    let (mut app, end) = client();
    select(&mut app, &[4]);
    toggle(&mut app, "cells");
    let (_, name, args) = first_query(&mut app, &end);
    assert_eq!((name.as_str(), args), ("core.clusters", json!({"unit": 4})));
}

#[test]
fn overlay_toggles_through_registry() {
    let (mut app, _end) = client();
    assert!(!app.world().resource::<OverlayState>().cells);
    assert!(toggle(&mut app, "cells"));
    assert!(app.world().resource::<OverlayState>().cells);
    assert!(!toggle(&mut app, "cells"));
    assert!(!app.world().resource::<OverlayState>().cells);
    let names: Vec<&str> = app.world().resource::<OverlayRegistry>().names().collect();
    for name in ["cells", "passability", "paths"] {
        assert!(names.contains(&name), "{name} not in {names:?}");
    }
}

#[test]
fn polls_every_15_frames_and_never_with_one_in_flight() {
    let (mut app, end) = client();
    toggle(&mut app, "passability");
    let (id, _, _) = first_query(&mut app, &end);
    // No reply yet: the query stays in flight and nothing else is sent.
    for _ in 0..3 * POLL_FRAMES {
        app.update();
    }
    assert!(queries(&end).is_empty());
    // The reply, read by the next frame, frees the overlay to poll again within 15 frames.
    let answer = json!({"cells": [4, 2], "version": 1, "blocked": []});
    end.tx
        .send(FromRunner::Reply {
            id,
            reply: SimReply::Debug(answer),
        })
        .unwrap();
    let mut frames = 0;
    let found = loop {
        app.update();
        frames += 1;
        let q = queries(&end);
        if !q.is_empty() {
            break q;
        }
        assert!(frames <= POLL_FRAMES + 1, "no new poll after the reply");
    };
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, "core.passability");
}

#[test]
fn an_error_reply_does_not_stop_the_polling() {
    let (mut app, end) = client();
    toggle(&mut app, "cells");
    let (id, _, _) = first_query(&mut app, &end);
    end.tx
        .send(FromRunner::Reply {
            id,
            reply: SimReply::Error("no navigation graph".to_owned()),
        })
        .unwrap();
    for _ in 0..=POLL_FRAMES + 1 {
        app.update();
    }
    assert_eq!(queries(&end).len(), 1);
}

#[test]
fn a_toggled_off_overlay_stops_polling() {
    let (mut app, end) = client();
    toggle(&mut app, "passability");
    first_query(&mut app, &end);
    toggle(&mut app, "passability");
    for _ in 0..3 * POLL_FRAMES {
        app.update();
    }
    assert!(queries(&end).is_empty());
}

#[test]
fn grid_lines_count() {
    let lines = grid_lines(Vec2::new(50.0, 50.0), 10.0, 4.0);
    let vertical: Vec<f32> = lines
        .iter()
        .filter(|(a, b)| a.x == b.x)
        .map(|(a, _)| a.x)
        .collect();
    let horizontal: Vec<f32> = lines
        .iter()
        .filter(|(a, b)| a.y == b.y)
        .map(|(a, _)| a.y)
        .collect();
    assert_eq!(vertical, [40.0, 44.0, 48.0, 52.0, 56.0, 60.0]);
    assert_eq!(horizontal, [40.0, 44.0, 48.0, 52.0, 56.0, 60.0]);
    assert_eq!(lines.len(), 12);
    // Each line spans the square.
    assert_eq!(lines[0], (Vec2::new(40.0, 40.0), Vec2::new(40.0, 60.0)));
}

#[test]
fn grid_lines_rejects_bad_steps() {
    for step in [0.0, -4.0, f32::NAN, f32::INFINITY] {
        assert!(grid_lines(Vec2::ZERO, 10.0, step).is_empty(), "{step}");
    }
}

#[test]
fn blocked_near_keeps_cells_in_range() {
    // 4 x 2 cells of 10 m; cells 0, 1 and 6 are blocked.
    let blocked = BlockedCells {
        cells: UVec2::new(4, 2),
        version: 1,
        runs: vec![(0, 2), (6, 1)],
    };
    let near = blocked_near(&blocked, 10.0, Vec2::new(5.0, 5.0), 12.0);
    assert_eq!(near, [UVec2::new(0, 0), UVec2::new(1, 0)]);
    let all = blocked_near(&blocked, 10.0, Vec2::new(20.0, 10.0), 1000.0);
    assert_eq!(all, [UVec2::new(0, 0), UVec2::new(1, 0), UVec2::new(2, 1)]);
}

fn send_reply(end: &RunnerEnd, id: u64, value: Value) {
    end.tx
        .send(FromRunner::Reply {
            id,
            reply: SimReply::Debug(value),
        })
        .unwrap();
}

fn paths_answer(unit: u32) -> Value {
    json!({ unit.to_string(): {"corners": [[raw(10), raw(20)]], "next": 0} })
}

#[test]
fn overlay_keys_ignored_while_console_open() {
    let (mut app, _end) = client();
    app.init_resource::<ButtonInput<KeyCode>>();
    app.world_mut().resource_mut::<ConsoleState>().open = true;
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::F5);
    app.update();
    assert!(!app.world().resource::<OverlayState>().paths);
    // The same press with the console closed toggles.
    app.world_mut().resource_mut::<ConsoleState>().open = false;
    app.update();
    assert!(app.world().resource::<OverlayState>().paths);
}

#[test]
fn error_reply_clears_the_overlay_data() {
    let (mut app, end) = client();
    select(&mut app, &[1]);
    toggle(&mut app, "paths");
    let (id, _, _) = first_query(&mut app, &end);
    send_reply(&end, id, paths_answer(1));
    app.update();
    assert_eq!(app.world().resource::<OverlayData>().paths().len(), 1);
    // The next poll fails: nothing stale stays.
    let mut next = None;
    for _ in 0..=POLL_FRAMES + 1 {
        app.update();
        if let Ok(ToRunner::Query { id, .. }) = end.rx.try_recv() {
            next = Some(id);
            break;
        }
    }
    end.tx
        .send(FromRunner::Reply {
            id: next.expect("a second poll"),
            reply: SimReply::Error("no unit #1".to_owned()),
        })
        .unwrap();
    app.update();
    assert!(app.world().resource::<OverlayData>().paths().is_empty());
}

#[test]
fn reply_to_a_query_before_a_selection_change_is_dropped() {
    let (mut app, end) = client();
    select(&mut app, &[1]);
    toggle(&mut app, "paths");
    let (id, _, _) = first_query(&mut app, &end);
    select(&mut app, &[2]);
    send_reply(&end, id, paths_answer(1));
    app.update();
    assert!(app.world().resource::<OverlayData>().paths().is_empty());
    // The overlay asks again for the new selection, and that answer is kept.
    let (id, _, args) = first_query(&mut app, &end);
    assert_eq!(args, json!({"units": [2]}));
    send_reply(&end, id, paths_answer(2));
    app.update();
    assert_eq!(app.world().resource::<OverlayData>().paths().len(), 1);
}

#[test]
fn blocked_cells_in_view_are_cached() {
    let (mut app, end) = client();
    app.insert_resource(MapView {
        size_m: Vec2::new(400.0, 400.0),
        ground_color: Color::WHITE,
        terrain: None,
    });
    let camera = app
        .world_mut()
        .spawn(RtsCamera {
            focus: Vec3::new(10.0, 0.0, 10.0),
            ..default()
        })
        .id();
    toggle(&mut app, "passability");
    let (id, _, _) = first_query(&mut app, &end);
    // 100 x 100 cells of 4 m; the whole first and the whole last row are blocked.
    send_reply(
        &end,
        id,
        json!({"cells": [100, 100], "version": 1, "blocked": [[0, 100], [9900, 100]]}),
    );
    app.update();
    app.update();
    let data = app.world().resource::<OverlayData>();
    let near = data.blocked_in_view().to_vec();
    assert!(near.contains(&UVec2::new(0, 0)) && !near.contains(&UVec2::new(99, 99)));
    let rebuilds = data.view_rebuilds();
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<OverlayData>().view_rebuilds(),
        rebuilds
    );
    // Moving the camera across the map rebuilds the list.
    app.world_mut().get_mut::<RtsCamera>(camera).unwrap().focus = Vec3::new(390.0, 0.0, 390.0);
    app.update();
    let data = app.world().resource::<OverlayData>();
    assert!(data.view_rebuilds() > rebuilds);
    assert!(data.blocked_in_view().contains(&UVec2::new(99, 99)));
    assert!(!data.blocked_in_view().contains(&UVec2::new(0, 0)));
}

/// Inserts a [`ScenarioLoader`] that starts every match on a fresh `runner_channel`; the
/// runner ends land in the returned list, newest last.
fn swappable(app: &mut App) -> Arc<Mutex<Vec<RunnerEnd>>> {
    let ends = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&ends);
    app.insert_resource(ScenarioLoader(Box::new(move |_request| {
        let (port, end) = runner_channel();
        sink.lock().expect("not poisoned").push(end);
        Ok(LoadedMatch {
            link: RunnerLink {
                port,
                header: header(),
            },
            next: Tick(0),
            pending: Vec::new(),
            last_seq: BTreeMap::new(),
            map: None,
            ai: None,
            dropped: Vec::new(),
            hook: None,
        })
    })));
    ends
}

/// A client with the passability overlay on and its first query in flight, unanswered, then
/// a new match swapped in (final review I4); returns the stale query's id and the new
/// runner's end.
fn swap_with_a_poll_in_flight(app: &mut App, old: &RunnerEnd) -> (u64, RunnerEnd) {
    app.insert_resource(MapView {
        size_m: Vec2::new(400.0, 400.0),
        ground_color: Color::WHITE,
        terrain: None,
    });
    app.world_mut().spawn(RtsCamera {
        focus: Vec3::new(10.0, 0.0, 10.0),
        ..default()
    });
    let ends = swappable(app);
    toggle(app, "passability");
    let (stale, name, _) = first_query(app, old);
    assert_eq!(name, "core.passability");
    replace_match(app.world_mut(), LoadRequest::Scenario("other".to_owned())).expect("loads");
    let new = ends
        .lock()
        .expect("not poisoned")
        .pop()
        .expect("a new runner");
    (stale, new)
}

#[test]
fn overlay_polls_again_after_a_match_swap() {
    let (mut app, old) = client();
    let (_, new) = swap_with_a_poll_in_flight(&mut app, &old);
    // The old runner never answers; the overlay asks the new one, and its answer is drawn.
    let (id, name, _) = first_query(&mut app, &new);
    assert_eq!(name, "core.passability");
    send_reply(
        &new,
        id,
        json!({"cells": [100, 100], "version": 1, "blocked": [[0, 100]]}),
    );
    app.update();
    app.update();
    let data = app.world().resource::<OverlayData>();
    assert!(
        data.blocked_in_view().contains(&UVec2::new(0, 0)),
        "the new match's answer is applied"
    );
}

#[test]
fn a_query_after_a_match_swap_gets_its_own_reply() {
    let (mut app, old) = client();
    let (stale, new) = swap_with_a_poll_in_flight(&mut app, &old);
    // Another waiter's query, sent before the overlay polls again.
    let other = app
        .world_mut()
        .resource_scope(|world, mut replies: Mut<PendingReplies>| {
            send_query(
                world.resource::<RunnerLink>(),
                &mut replies,
                SimQuery::Entity(SimId(1)),
            )
        });
    match new.rx.try_recv() {
        Ok(ToRunner::Query { id, .. }) => assert_eq!(id, other),
        other => panic!("the new runner got {other:?}"),
    }
    new.tx
        .send(FromRunner::Reply {
            id: other,
            reply: SimReply::Entity(None),
        })
        .unwrap();
    app.update();
    let reply = app.world_mut().resource_mut::<PendingReplies>().take(other);
    assert_eq!(
        reply,
        Some(SimReply::Entity(None)),
        "query {other} kept its reply (the stale poll was {stale})"
    );
}
