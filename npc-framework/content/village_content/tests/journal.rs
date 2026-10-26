//! Task G: the deterministic journal.

mod common;

use std::io::Write;
use std::sync::{Arc, Mutex};

use common::*;
use npc_framework::kernel::NpcSimTick;
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::{
    Journal, JournalEntry, JournalFilter, JournalKind, JournalPlugin, JournalSink, query,
};
use npc_framework::prelude::NeedLevel;

fn journaled(seed: u64, verbose: bool) -> App {
    let mut app = village(seed);
    app.add_plugins(JournalPlugin {
        verbose,
        ..Default::default()
    });
    app
}

fn entries(app: &App) -> Vec<JournalEntry> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .cloned()
        .collect()
}

#[test]
fn journal_off_by_default_records_nothing() {
    let mut app = village(1);
    run_ticks(app.world_mut(), 500);
    let journal = app.world().resource::<Journal>();
    assert!(!journal.enabled);
    assert!(journal.entries.is_empty());
}

#[test]
fn journal_records_abort_with_guard_name() {
    let mut app = journaled(1, false);
    let v = ids(&app).villager;
    set_tick(&mut app, 600);
    set_food(&mut app, 10);
    run_until(&mut app, 1000, |app| path(app, v).contains("Wander")).expect("wanders");
    set_hunger(&mut app, 10_000);
    run_until(&mut app, 3, |app| {
        path(app, v).starts_with("Villager > UrgentEat")
    })
    .unwrap_or_else(|| panic!("no UrgentEat; path {:?}", path(&app, v)));

    let aborts = query(
        app.world().resource::<Journal>(),
        &JournalFilter {
            unit: Some(v),
            kinds: vec!["Abort".into()],
            ..Default::default()
        },
    );
    assert!(
        aborts.iter().any(|e| matches!(
            &e.kind,
            JournalKind::Abort { guard, aborted }
                if guard == "UrgentEat" && aborted.iter().any(|n| n == "Wander")
        )),
        "{aborts:#?}"
    );
    let all = entries(&app);
    assert!(all.iter().any(|e| matches!(
        &e.kind,
        JournalKind::NeedLevel {
            need,
            to: NeedLevel::Urgent,
            ..
        } if need == HUNGER
    ) && e.unit == Some(v)));
    assert!(all.iter().any(|e| matches!(
        &e.kind,
        JournalKind::Command {
            cmd: SimCommand::SetNeed { .. }
        }
    )));
    assert!(
        all.iter()
            .any(|e| matches!(&e.kind, JournalKind::Reserve { target, ok: true } if *target == ids(&app).stockpile))
    );
    // The aborted leaf's stop follows the abort.
    let i = all
        .iter()
        .position(|e| matches!(&e.kind, JournalKind::Abort { guard, .. } if guard == "UrgentEat"))
        .unwrap();
    assert!(
        matches!(&all[i + 1].kind, JournalKind::LeafStop { node, .. } if node == "MoveToDest" || node == "IdleFor"),
        "{:#?}",
        &all[i..i + 3]
    );
}

#[test]
fn journal_filter_by_unit_kind_range() {
    let mut app = journaled(1, false);
    let v = ids(&app).villager;
    run_ticks(app.world_mut(), 3000);
    let journal = app.world().resource::<Journal>();

    let filter = JournalFilter {
        unit: Some(v),
        kinds: vec!["LeafStart".into(), "leafstop".into()],
        from: Some(100),
        to: Some(2000),
        ..Default::default()
    };
    let hits = query(journal, &filter);
    assert!(!hits.is_empty());
    for e in &hits {
        assert!(filter.matches(e));
        assert_eq!(e.unit, Some(v));
        assert!((100..=2000).contains(&e.tick), "{e:?}");
        assert!(matches!(
            e.kind,
            JournalKind::LeafStart { .. } | JournalKind::LeafStop { .. }
        ));
    }
    assert!(
        hits.windows(2)
            .all(|w| (w[0].tick, w[0].seq) < (w[1].tick, w[1].seq))
    );

    let night = query(
        journal,
        &JournalFilter {
            kinds: vec!["PhaseChange".into()],
            ..Default::default()
        },
    );
    assert_eq!(
        night
            .iter()
            .map(|e| (e.tick, e.kind.clone()))
            .collect::<Vec<_>>(),
        [
            (1680, JournalKind::PhaseChange { night: true }),
            (2400, JournalKind::PhaseChange { night: false }),
        ]
    );

    let grep = query(
        journal,
        &JournalFilter {
            grep: Some("MoveToDest".into()),
            ..Default::default()
        },
    );
    assert!(!grep.is_empty());
    assert!(
        grep.iter()
            .all(|e| serde_json::to_string(e).unwrap().contains("MoveToDest"))
    );

    let limited = query(
        journal,
        &JournalFilter {
            limit: Some(5),
            ..filter.clone()
        },
    );
    assert_eq!(
        limited,
        hits[hits.len() - 5..],
        "limit keeps the most recent"
    );
}

#[test]
fn journal_ring_buffer_caps() {
    let mut app = village(1);
    app.add_plugins(JournalPlugin {
        capacity: 100,
        ..Default::default()
    });
    run_ticks(app.world_mut(), 24_000);
    let journal = app.world().resource::<Journal>();
    assert_eq!(journal.entries.len(), 100);
    assert!(journal.entries.back().unwrap().tick > 20_000);
}

#[test]
fn journal_deterministic() {
    let run = || {
        let mut app = journaled(1, true);
        run_ticks(app.world_mut(), 5000);
        serde_json::to_string(&entries(&app)).unwrap()
    };
    let a = run();
    assert!(a.len() > 1000);
    assert!(a.contains("CondEval"));
    assert_eq!(a, run());
}

#[test]
fn journal_entries_roundtrip_json() {
    let mut app = journaled(1, true);
    set_food(&mut app, 3);
    run_ticks(app.world_mut(), 3000);
    let all = entries(&app);
    for e in &all {
        let json = serde_json::to_string(e).unwrap();
        let back: JournalEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, e, "{json}");
    }
}

#[test]
fn tooling_does_not_change_hash() {
    let mut plain = village(1);
    run_ticks(plain.world_mut(), 10_000);

    let mut tooled = journaled(1, true);
    tooled.world_mut().resource_mut::<Journal>().sink =
        Some(JournalSink::new(Box::new(std::io::sink())));
    run_ticks(tooled.world_mut(), 10_000);
    assert!(!tooled.world().resource::<Journal>().entries.is_empty());

    assert_eq!(
        state_hash(plain.world_mut()),
        state_hash(tooled.world_mut())
    );
}

/// Writer handle the test can read back.
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn journal_sink_streams_json_lines_per_tick() {
    let mut app = village(1);
    app.add_plugins(JournalPlugin {
        capacity: 10,
        ..Default::default()
    });
    let out = Shared::default();
    app.world_mut().resource_mut::<Journal>().sink = Some(JournalSink::new(Box::new(out.clone())));
    app.world_mut().run_schedule(NpcSimTick);
    // Flushed at the end of the tick: everything of tick 0 is already out.
    let text = String::from_utf8(out.0.lock().unwrap().clone()).unwrap();
    let lines: Vec<JournalEntry> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(!lines.is_empty());
    assert!(lines.iter().all(|e| e.tick == 0));
    run_ticks(app.world_mut(), 2000);
    let total = String::from_utf8(out.0.lock().unwrap().clone())
        .unwrap()
        .lines()
        .count();
    // The file keeps everything; the ring buffer only the last 10.
    assert!(total > 10, "{total}");
    assert_eq!(app.world().resource::<Journal>().entries.len(), 10);
}
