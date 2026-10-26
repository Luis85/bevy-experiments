//! Plan Task C2: the stockpile claim of a meal is released in the tick the
//! meal completes (with its `TakeFood` effect), not one tick later when the
//! leaf stops. Before the fix the claim outlived the food it stood for, so
//! another agent could be refused food it was entitled to, and the property
//! tests had to tolerate the over-claim.

mod common;

use common::*;
use npc_framework::kernel::journal::{Journal, JournalKind, ReleaseReason, set_journal};
use village_content::actions::Eat;

/// A hungry villager at a stocked stockpile, one tick before its meal ends.
fn about_to_finish_meal() -> (App, VillageIds, Entity) {
    let mut app = village(1);
    let ids = ids(&app);
    set_journal(app.world_mut(), true, false);
    set_food(&mut app, 4);
    run_ticks(app.world_mut(), 1);
    set_hunger(&mut app, 30_000);
    let v = ent(&app, ids.villager);
    let found = run_until(&mut app, 3000, |app| {
        app.world().get::<Eat>(v).is_some_and(|e| e.remaining == 1)
    });
    assert!(found.is_some(), "the villager starts a meal");
    assert!(
        app.world()
            .resource::<Reservations>()
            .holds(ids.stockpile, ids.villager),
        "a meal holds its stockpile claim while it runs"
    );
    (app, ids, v)
}

#[test]
fn eat_releases_claim_on_completion() {
    let (mut app, ids, v) = about_to_finish_meal();
    let food_before = food(&app);
    run_ticks(app.world_mut(), 1);
    let done = tick(&app) - 1;
    // The meal completed this tick (its leaf stops at the next think).
    assert_eq!(app.world().get::<Eat>(v).map(|e| e.remaining), Some(0));
    assert_eq!(food(&app), food_before - 1, "the meal took its food");
    assert!(
        !app.world()
            .resource::<Reservations>()
            .holds(ids.stockpile, ids.villager),
        "the claim is released in the tick the meal completes"
    );
    // Journal: `Release { Finished }` in the same tick, before `TakeFood`.
    let entries: Vec<_> = app
        .world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|e| e.tick == done && e.unit == Some(ids.villager))
        .cloned()
        .collect();
    let release = entries.iter().position(|e| {
        e.kind
            == JournalKind::Release {
                target: ids.stockpile,
                reason: ReleaseReason::Finished,
            }
    });
    let take = entries.iter().position(|e| {
        matches!(&e.kind, JournalKind::Effect { effect, .. }
            if effect.get("type").and_then(|t| t.as_str()) == Some("TakeFood"))
    });
    let (Some(release), Some(take)) = (release, take) else {
        panic!("Release and TakeFood in tick {done}: {entries:?}");
    };
    assert!(release < take, "{entries:?}");

    // The leaf stops next tick: no second Release for the same claim.
    run_ticks(app.world_mut(), 1);
    assert!(app.world().get::<Eat>(v).is_none());
    let releases = app
        .world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|e| {
            e.tick > done
                && e.kind
                    == JournalKind::Release {
                        target: ids.stockpile,
                        reason: ReleaseReason::Finished,
                    }
        })
        .count();
    assert_eq!(releases, 0, "released once");
}
