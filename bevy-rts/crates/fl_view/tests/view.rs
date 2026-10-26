//! Contract tests for the `fl_view` snapshot schema, runner protocol and command stamper.

use std::collections::BTreeMap;
use std::sync::Arc;

use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    CommandKind, CommandTypeId, PlayerCommand, PlayerId, SimId, StringTables, TeamId, Tick,
    UnitTypeId,
};
use fl_view::{
    AiController, CommandDraft, CommandStamper, EntityRecord, NullAi, OpaqueContent, PlayerView,
    RunnerControl, SimSnapshot, TickInput, ToRunner, ViewChannel, ai_controller, flags,
    runner_channel,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TestChan {
    n: u32,
}

impl ViewChannel for TestChan {
    const NAME: &'static str = "test.chan";
}

fn record(id: u32) -> EntityRecord {
    EntityRecord {
        sim_id: SimId(id),
        unit: UnitTypeId(2),
        owner: PlayerId(1),
        pos: FxVec2::new(Fx::from_int(10), Fx::from_int(-3)),
        heading: Angle::from_degrees(Fx::from_int(90)),
        turrets: vec![Angle::from_degrees(Fx::from_int(45))],
        hp_frac: EntityRecord::hp_frac(75, 100),
        flags: flags::MOVING,
        visible_to: 0b11,
    }
}

fn snapshot(ids: &[u32]) -> SimSnapshot {
    SimSnapshot {
        tick: Tick(12),
        entities: ids.iter().map(|&id| record(id)).collect(),
        channels: Vec::new(),
    }
}

fn draft(player: u8) -> CommandDraft {
    CommandDraft {
        player: PlayerId(player),
        units: vec![SimId(3)],
        kind: CommandKind {
            type_id: CommandTypeId(1),
            payload: vec![9],
        },
        queue: false,
    }
}

#[test]
fn snapshot_postcard_roundtrip() {
    let mut snap = snapshot(&[1, 4, 9]);
    snap.set_channel(&TestChan { n: 3 });
    let bytes = postcard::to_allocvec(&snap).unwrap();
    let back: SimSnapshot = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(snap, back);
}

#[test]
fn get_finds_by_sim_id() {
    let snap = snapshot(&[1, 4, 9]);
    assert_eq!(snap.get(SimId(4)).unwrap().sim_id, SimId(4));
    assert_eq!(snap.get(SimId(5)), None);
}

#[test]
fn channel_roundtrip() {
    let mut snap = snapshot(&[]);
    assert_eq!(snap.channel::<TestChan>(), None);
    snap.set_channel(&TestChan { n: 7 });
    assert_eq!(snap.channel::<TestChan>(), Some(Ok(TestChan { n: 7 })));
    snap.set_channel(&TestChan { n: 8 });
    assert_eq!(snap.channels.len(), 1);
    assert_eq!(snap.channel::<TestChan>(), Some(Ok(TestChan { n: 8 })));
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct FxChan {
    range: Fx,
}

impl ViewChannel for FxChan {
    const NAME: &'static str = "test.fx";
}

#[test]
fn channel_keeps_fx_exact() {
    // 2^30 m plus one raw unit (2^-32 m) needs 63 significant bits; an f64 (JSON) round trip
    // would lose the last unit.
    let range = Fx::from_raw((1_i64 << 62) + 1);
    let mut snap = snapshot(&[]);
    snap.set_channel(&FxChan { range });
    assert_eq!(snap.channel::<FxChan>(), Some(Ok(FxChan { range })));
}

#[test]
fn channel_decode_failure_is_err() {
    let mut snap = snapshot(&[]);
    snap.channels.push((TestChan::NAME.to_owned(), Vec::new()));
    assert!(matches!(snap.channel::<TestChan>(), Some(Err(_))));
}

#[test]
fn hp_frac_values() {
    assert_eq!(EntityRecord::hp_frac(50, 100), 32767);
    assert_eq!(EntityRecord::hp_frac(100, 100), 65535);
    assert_eq!(EntityRecord::hp_frac(0, 0), 0);
}

#[test]
fn stamper_assigns_increasing_seq_per_player() {
    let mut stamper = CommandStamper::default();
    let seqs: Vec<u32> = [0, 1, 0]
        .into_iter()
        .map(|p| stamper.stamp(Tick(5), draft(p)).seq)
        .collect();
    assert_eq!(seqs, vec![0, 0, 1]);
    assert_eq!(stamper.held().len(), 3);
}

#[test]
fn stamper_take_due_sorted_and_only_due() {
    let mut stamper = CommandStamper::default();
    stamper.stamp(Tick(6), draft(0));
    stamper.stamp(Tick(5), draft(1));
    stamper.stamp(Tick(5), draft(0));
    stamper.stamp(Tick(5), draft(1));

    let due = stamper.take_due(Tick(5));
    let keys: Vec<(Tick, PlayerId, u32)> =
        due.iter().map(|c| (c.exec_tick, c.player, c.seq)).collect();
    assert_eq!(
        keys,
        vec![
            (Tick(5), PlayerId(0), 1),
            (Tick(5), PlayerId(1), 0),
            (Tick(5), PlayerId(1), 1),
        ]
    );

    let held = stamper.held();
    assert_eq!(held.len(), 1);
    assert_eq!((held[0].exec_tick, held[0].seq), (Tick(6), 0));
    assert!(stamper.take_due(Tick(5)).is_empty());
}

#[test]
fn stamper_restore_continues_seq() {
    let pending = PlayerCommand {
        exec_tick: Tick(9),
        player: PlayerId(0),
        seq: 5,
        units: vec![SimId(3)],
        kind: CommandKind {
            type_id: CommandTypeId(1),
            payload: vec![],
        },
        queue: false,
    };
    let last_applied = BTreeMap::from([(PlayerId(0), 4)]);

    let mut stamper = CommandStamper::default();
    stamper.restore(vec![pending.clone()], &last_applied);
    assert_eq!(stamper.held(), vec![pending]);
    assert_eq!(stamper.stamp(Tick(10), draft(0)).seq, 6);
    assert_eq!(stamper.stamp(Tick(10), draft(1)).seq, 0);
}

#[test]
fn tick_input_postcard_roundtrip() {
    let mut stamper = CommandStamper::default();
    stamper.stamp(Tick(3), draft(0));
    stamper.stamp(Tick(3), draft(1));
    let input = TickInput {
        tick: Tick(3),
        commands: stamper.take_due(Tick(3)),
    };
    let bytes = postcard::to_allocvec(&input).unwrap();
    let back: TickInput = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(input, back);
}

#[test]
fn runner_channel_carries_messages() {
    let (port, end) = runner_channel();
    port.tx.send(ToRunner::Shutdown).unwrap();
    assert!(matches!(end.rx.try_recv(), Ok(ToRunner::Shutdown)));
}

#[test]
fn opaque_content_eq_is_identity() {
    let shared: Arc<dyn std::any::Any + Send + Sync> = Arc::new(7_u32);
    let a = OpaqueContent(Arc::clone(&shared));
    let b = OpaqueContent(shared);
    assert_eq!(a, b, "two handles on one value are equal");
    assert_eq!(a, a.clone());
    // An equal value in another allocation is a different content.
    let other = OpaqueContent(Arc::new(7_u32));
    assert_ne!(a, other);
    assert_eq!(format!("{a:?}"), "OpaqueContent");

    // It travels to the runner inside a control message, compared the same way.
    let (port, end) = runner_channel();
    let control = RunnerControl::ReloadContent(a.clone());
    port.tx
        .send(ToRunner::Control {
            id: 3,
            control: control.clone(),
        })
        .unwrap();
    assert_eq!(
        end.rx.try_recv().unwrap(),
        ToRunner::Control { id: 3, control }
    );
    assert_ne!(
        RunnerControl::ReloadContent(a),
        RunnerControl::ReloadContent(other)
    );
}

fn owned_record(id: u32, owner: u8, visible_to: u8) -> EntityRecord {
    EntityRecord {
        owner: PlayerId(owner),
        visible_to,
        ..record(id)
    }
}

#[test]
fn player_view_splits_own_and_visible() {
    let snap = SimSnapshot {
        tick: Tick(12),
        entities: vec![
            owned_record(1, 0, 0b00),
            owned_record(2, 1, 0b01),
            owned_record(3, 1, 0b10),
        ],
        channels: Vec::new(),
    };
    let view = PlayerView::from_snapshot(&snap, PlayerId(0), TeamId(0));
    assert_eq!(view.tick, Tick(12));
    assert_eq!((view.player, view.team), (PlayerId(0), TeamId(0)));
    let ids = |records: &[EntityRecord]| records.iter().map(|r| r.sim_id).collect::<Vec<_>>();
    assert_eq!(ids(&view.own), [SimId(1)]);
    assert_eq!(ids(&view.visible), [SimId(2)]);
    assert_eq!(view.visible[0], snap.entities[1]);

    // Team 1 sees #3, not #2; player 1 owns both.
    let view = PlayerView::from_snapshot(&snap, PlayerId(0), TeamId(1));
    assert_eq!(ids(&view.visible), [SimId(3)]);
    let view = PlayerView::from_snapshot(&snap, PlayerId(1), TeamId(1));
    assert_eq!(ids(&view.own), [SimId(2), SimId(3)]);
    assert!(view.visible.is_empty(), "#1 is visible to no team");
}

#[test]
fn null_ai_issues_nothing() {
    let mut ai = NullAi;
    assert_eq!(ai.id(), "null");
    let view = PlayerView::from_snapshot(&snapshot(&[1, 4, 9]), PlayerId(1), TeamId(0));
    assert_eq!(view.own.len(), 3);
    assert!(ai.think(&view, &StringTables::default()).is_empty());
}

#[test]
fn ai_controller_by_id() {
    let ai = ai_controller("null").expect("the null AI exists");
    assert_eq!(ai.id(), "null");
    assert!(ai_controller("nope").is_none());
}
