//! The AI host: controllers think once per frame in which the newest snapshot's tick changed,
//! and their drafts go through `LocalDriver` as their own player only.
//!
//! The runner is faked without a thread: the test writes `TickOutput`s into the runner's end of
//! the channel before a frame, and a frozen clock means the driver never ticks by itself, so
//! every frame is deterministic.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use fl_client::ai::{AiHost, AiPlugin};
use fl_client::driver::LocalDriver;
use fl_client::link::{RunnerLink, encode_command};
use fl_client::view::SimViewPlugin;
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    MatchHeader, MoveCmd, PlayerId, SimId, StringTables, TeamId, Tick, UnitTypeId, tables,
};
use fl_view::{
    AiController, CommandDraft, EntityRecord, FromRunner, NullAi, PlayerView, RunnerEnd,
    SimSnapshot, TickDriver, TickOutput, runner_channel,
};

const ME: PlayerId = PlayerId(1);
const OTHER: PlayerId = PlayerId(0);

fn header() -> MatchHeader {
    let mut string_tables = StringTables::default();
    string_tables.insert(tables::COMMANDS, vec!["core.move".to_owned()]);
    MatchHeader {
        format_version: fl_protocol::FORMAT_VERSION,
        build_id: "test".to_owned(),
        sim_version: 1,
        features: Vec::new(),
        mods: Vec::new(),
        rules_hash: 0,
        cheats_allowed: true,
        seed: 1,
        string_tables,
    }
}

fn record(id: u32, owner: PlayerId, visible_to: u8) -> EntityRecord {
    EntityRecord {
        sim_id: SimId(id),
        unit: UnitTypeId(0),
        owner,
        pos: FxVec2::new(Fx::from_int(10), Fx::from_int(10)),
        heading: Angle::from_raw(0),
        turrets: Vec::new(),
        hp_frac: u16::MAX,
        flags: 0,
        visible_to,
    }
}

fn output(tick: u32) -> FromRunner {
    FromRunner::Tick(TickOutput {
        tick: Tick(tick),
        hash: None,
        snapshot: Arc::new(SimSnapshot {
            tick: Tick(tick),
            // #1 is player 0's and visible to every team; #2 is ours.
            entities: vec![record(1, OTHER, 0xFF), record(2, ME, 0xFF)],
            channels: Vec::new(),
        }),
        events: Vec::new(),
        journal: Vec::new(),
    })
}

fn move_draft(player: PlayerId) -> CommandDraft {
    let kind = encode_command(
        &header().string_tables,
        &MoveCmd {
            target: FxVec2::new(Fx::from_int(1), Fx::from_int(2)),
        },
    )
    .unwrap();
    CommandDraft {
        player,
        units: vec![SimId(2)],
        kind,
        queue: false,
    }
}

/// An AI that drafts one move for `drafts_for` on every think, and records its views.
struct Scripted {
    drafts_for: PlayerId,
    views: Arc<Mutex<Vec<PlayerView>>>,
}

impl AiController for Scripted {
    fn id(&self) -> &'static str {
        "scripted"
    }

    fn think(&mut self, view: &PlayerView, _tables: &StringTables) -> Vec<CommandDraft> {
        self.views.lock().unwrap().push(view.clone());
        vec![move_draft(self.drafts_for)]
    }
}

/// A headless client with a frozen clock, a driver at tick 100 and `ai` as player 1 on `team`;
/// the returned end is the fake runner's, to write tick outputs into.
fn app_with(team: TeamId, ai: Box<dyn AiController>) -> (App, RunnerEnd) {
    let (port, end) = runner_channel();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, SimViewPlugin, AiPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
        .insert_resource(RunnerLink {
            port,
            header: header(),
        })
        .insert_resource(LocalDriver::new(Tick(100)));
    app.world_mut()
        .resource_mut::<AiHost>()
        .players
        .insert(ME, (team, ai));
    (app, end)
}

fn thinks(app: &App) -> u64 {
    app.world().resource::<AiHost>().thinks
}

fn pending(app: &App) -> usize {
    app.world().resource::<LocalDriver>().pending().len()
}

#[test]
fn null_ai_issues_nothing() {
    let (mut app, end) = app_with(TeamId(1), Box::new(NullAi));
    for tick in 0..20 {
        end.tx.send(output(tick)).unwrap();
        app.update();
    }
    assert_eq!(pending(&app), 0);
    assert_eq!(thinks(&app), 20, "one think per observed tick");
}

#[test]
fn no_snapshot_no_think() {
    let (mut app, _end) = app_with(TeamId(1), Box::new(NullAi));
    app.update();
    app.update();
    assert_eq!(thinks(&app), 0);
}

#[test]
fn ai_thinks_once_per_new_tick() {
    let views = Arc::default();
    let ai = Scripted {
        drafts_for: ME,
        views: Arc::clone(&views),
    };
    let (mut app, end) = app_with(TeamId(1), Box::new(ai));
    for tick in 5..8 {
        end.tx.send(output(tick)).unwrap();
    }
    app.update();
    assert_eq!(thinks(&app), 1, "three ticks in one frame, one think");
    assert_eq!(
        views.lock().unwrap()[0].tick,
        Tick(7),
        "on the newest snapshot"
    );
    assert_eq!(pending(&app), 1);
    // A frame without a new snapshot doesn't think again.
    app.update();
    assert_eq!(thinks(&app), 1);
    assert_eq!(pending(&app), 1);
    end.tx.send(output(8)).unwrap();
    app.update();
    assert_eq!(thinks(&app), 2);
    assert_eq!(pending(&app), 2);
}

#[test]
fn drafts_are_stamped_for_the_ai_player() {
    let ai = Scripted {
        drafts_for: ME,
        views: Arc::default(),
    };
    let (mut app, end) = app_with(TeamId(1), Box::new(ai));
    end.tx.send(output(1)).unwrap();
    app.update();
    let held = app.world().resource::<LocalDriver>().pending();
    assert_eq!(held.len(), 1);
    assert_eq!(
        (held[0].player, held[0].exec_tick, held[0].seq),
        (ME, Tick(101), 0)
    );
}

#[test]
fn draft_for_another_player_is_dropped() {
    let ai = Scripted {
        drafts_for: OTHER,
        views: Arc::default(),
    };
    let (mut app, end) = app_with(TeamId(1), Box::new(ai));
    end.tx.send(output(1)).unwrap();
    app.update();
    assert_eq!(thinks(&app), 1, "it thought, and drafted");
    assert_eq!(pending(&app), 0, "nothing issued for another player");
}

#[test]
fn team_beyond_the_visibility_bits_sees_nothing() {
    let views = Arc::default();
    let ai = Scripted {
        drafts_for: ME,
        views: Arc::clone(&views),
    };
    // Team 8 has no bit in `visible_to`; player 0's record is visible to bits 0..8.
    let (mut app, end) = app_with(TeamId(8), Box::new(ai));
    end.tx.send(output(1)).unwrap();
    app.update();
    let views = views.lock().unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].own.len(), 1, "its own record is still its own");
    assert!(views[0].visible.is_empty(), "{:?}", views[0].visible);
}

#[test]
fn team_within_the_bits_sees_visible_records() {
    let views = Arc::default();
    let ai = Scripted {
        drafts_for: ME,
        views: Arc::clone(&views),
    };
    let (mut app, end) = app_with(TeamId(7), Box::new(ai));
    end.tx.send(output(1)).unwrap();
    app.update();
    let views = views.lock().unwrap();
    assert_eq!(views[0].visible.len(), 1);
}

#[test]
fn repeated_foreign_drafts_are_all_dropped_and_counted() {
    let ai = Scripted {
        drafts_for: OTHER,
        views: Arc::default(),
    };
    let (mut app, end) = app_with(TeamId(1), Box::new(ai));
    for tick in 1..=3 {
        end.tx.send(output(tick)).unwrap();
        app.update();
    }
    assert_eq!(pending(&app), 0);
    assert_eq!(app.world().resource::<AiHost>().dropped_drafts, 3);
}

#[test]
fn two_ai_players_each_think_once_per_tick_change() {
    let (mut app, end) = app_with(TeamId(1), Box::new(NullAi));
    app.world_mut()
        .resource_mut::<AiHost>()
        .players
        .insert(PlayerId(2), (TeamId(2), Box::new(NullAi)));
    end.tx.send(output(1)).unwrap();
    end.tx.send(output(2)).unwrap();
    app.update();
    assert_eq!(thinks(&app), 2, "one think per controller");
    app.update();
    assert_eq!(thinks(&app), 2, "no new tick, no think");
    end.tx.send(output(3)).unwrap();
    app.update();
    assert_eq!(thinks(&app), 4);
}

#[test]
fn tick_regression_triggers_a_rethink() {
    let views = Arc::default();
    let ai = Scripted {
        drafts_for: ME,
        views: Arc::clone(&views),
    };
    let (mut app, end) = app_with(TeamId(1), Box::new(ai));
    end.tx.send(output(50)).unwrap();
    app.update();
    end.tx.send(output(3)).unwrap();
    app.update();
    assert_eq!(thinks(&app), 2);
    assert_eq!(views.lock().unwrap()[1].tick, Tick(3));
}

#[test]
fn a_new_host_thinks_on_the_current_snapshot() {
    // Ruling R91: a load inserts a fresh host, which must think on the new match's first
    // snapshot even when its tick equals the one the old host last thought on.
    let (mut app, end) = app_with(TeamId(1), Box::new(NullAi));
    end.tx.send(output(5)).unwrap();
    app.update();
    assert_eq!(thinks(&app), 1);
    let mut host = AiHost::default();
    host.players.insert(ME, (TeamId(1), Box::new(NullAi)));
    app.insert_resource(host);
    app.update();
    assert_eq!(thinks(&app), 1, "the new host thought on tick 5 again");
    app.update();
    assert_eq!(thinks(&app), 1, "once");
}
