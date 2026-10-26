//! Thirst + the well: villagers have a `Thirst` need and drink at the well
//! (a smart object that `Provides` thirst through an `Approach` point, with a
//! few drinking slots handed out through `Reservations`) when thirsty.
//!
//! The tests go through stable names only (the `Thirst` need, the `well`
//! kind, the `DrinkWater` action, the `UrgentDrink` / `CasualDrink` guards),
//! so they say what the mechanic must do, not how it is wired.

mod common;

use common::*;
use npc_framework::kernel::inspect::{kind_name, sim_entities};
use npc_framework::kernel::journal::{Journal, JournalKind, ReleaseReason, set_journal};
use npc_framework::prelude::{NeedLevel, Provides};

const THIRST: &str = "Thirst";

/// The village's well (the only entity of kind `well`).
fn well(app: &App) -> SimId {
    let wells: Vec<SimId> = sim_entities(app.world())
        .into_iter()
        .filter(|&(_, e)| kind_name(app.world(), e) == "well")
        .map(|(id, _)| id)
        .collect();
    assert_eq!(wells.len(), 1, "the village has exactly one well");
    wells[0]
}

fn thirst(app: &App, id: SimId) -> (i32, NeedLevel) {
    let thirst = need_id(app.world(), THIRST);
    let n = app.world().get::<Needs>(ent(app, id)).unwrap().get(thirst);
    (n.value, n.level)
}

fn set_thirst(app: &mut App, value: i32) {
    let unit = ids(app).villager.0;
    send(
        app,
        None,
        SimCommand::SetNeed {
            unit,
            need: THIRST.into(),
            value,
        },
    );
}

/// `LeafStart`s of `action` by `unit` (tick, path).
fn leaf_starts(app: &App, unit: SimId, action: &str) -> Vec<(u64, String)> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|e| e.unit == Some(unit))
        .filter_map(|e| match &e.kind {
            JournalKind::LeafStart {
                action: a, path, ..
            } if a == action => Some((e.tick, path.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn village_has_thirst_and_a_well() {
    let app = village(1);
    let ids = ids(&app);
    // The layout keeps its SimIds (house 1 .. villager 5); the well comes after.
    assert_eq!(ids.villager, SimId(5));
    let well = well(&app);
    assert!(well > ids.villager, "the well is spawned after the plot");
    let thirst_id = need_id(app.world(), THIRST);
    let provides = app
        .world()
        .get::<Provides>(ent(&app, well))
        .expect("the well advertises what it provides");
    assert_eq!(provides.need, thirst_id);
    assert!(provides.amount_per_tick > 0);
    assert!(
        npc_framework::jobs::approach_of(app.world(), ent(&app, well)).is_some(),
        "the well has an approach point"
    );
    // A fresh villager starts with full thirst satisfaction.
    assert_eq!(thirst(&app, ids.villager).0, 100_000);
}

#[test]
fn urgent_thirst_sends_the_villager_to_the_well() {
    let mut app = village(1);
    let ids = ids(&app);
    set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 1);
    set_thirst(&mut app, 10_000);
    let well = well(&app);
    // He reserves a slot at the well, walks there and drinks.
    let drank = run_until(&mut app, 600, |app| {
        !leaf_starts(app, ids.villager, "DrinkWater").is_empty()
    });
    assert!(
        drank.is_some(),
        "the villager starts DrinkWater within 600 ticks"
    );
    let (_, path) = leaf_starts(&app, ids.villager, "DrinkWater")[0].clone();
    assert!(path.contains("UrgentDrink"), "urgent branch: {path}");
    assert!(
        app.world()
            .resource::<Reservations>()
            .holds(well, ids.villager),
        "drinking holds a slot at the well"
    );
    let at_well = npc_framework::jobs::approach_of(app.world(), ent(&app, well)).unwrap();
    assert_eq!(pos(&app, ids.villager), at_well, "he drinks at the well");
    // The drink restores thirst and frees the slot.
    let done = run_until(&mut app, 200, |app| {
        !app.world()
            .resource::<Reservations>()
            .holds(well, ids.villager)
    });
    assert!(done.is_some(), "the drink ends and releases the well");
    assert_eq!(
        thirst(&app, ids.villager).0,
        100_000,
        "one drink fills thirst"
    );
    // The level follows in the next tick's `NpcSet::Needs`.
    run_ticks(app.world_mut(), 1);
    assert_eq!(thirst(&app, ids.villager).1, NeedLevel::Ok);
    let released = app.world().resource::<Journal>().entries.iter().any(|e| {
        e.unit == Some(ids.villager)
            && e.kind
                == JournalKind::Release {
                    target: well,
                    reason: ReleaseReason::Finished,
                }
    });
    assert!(released, "the finished drink journals its Release");
}

#[test]
fn mildly_thirsty_villager_drinks_casually() {
    let mut app = village(1);
    let ids = ids(&app);
    set_journal(app.world_mut(), true, false);
    // Low, not urgent, before the first think (the command applies in tick
    // 0's inputs): the casual branch wins over the day's work.
    set_thirst(&mut app, 40_000);
    let drank = run_until(&mut app, 600, |app| {
        !leaf_starts(app, ids.villager, "DrinkWater").is_empty()
    });
    assert!(
        drank.is_some(),
        "a Low-thirst villager drinks within 600 ticks"
    );
    let (_, path) = leaf_starts(&app, ids.villager, "DrinkWater")[0].clone();
    assert!(path.contains("CasualDrink"), "casual branch: {path}");
}

#[test]
fn aborted_drink_releases_the_well() {
    let mut app = village(1);
    let ids = ids(&app);
    run_ticks(app.world_mut(), 1);
    set_thirst(&mut app, 10_000);
    let well = well(&app);
    let claimed = run_until(&mut app, 300, |app| {
        app.world()
            .resource::<Reservations>()
            .holds(well, ids.villager)
    });
    assert!(claimed.is_some(), "he claims a slot at the well");
    // Starving now outranks thirst: the drink branch is aborted.
    set_hunger(&mut app, 0);
    run_ticks(app.world_mut(), 2);
    assert!(
        !app.world()
            .resource::<Reservations>()
            .holds(well, ids.villager),
        "an aborted drink releases its well slot"
    );
}

#[test]
fn never_dehydrates_over_ten_days() {
    let mut app = village(1);
    let ids = ids(&app);
    let (mut min_thirst, mut min_hunger, mut min_energy) = (i32::MAX, i32::MAX, i32::MAX);
    for _ in 0..24_000 {
        run_ticks(app.world_mut(), 1);
        min_thirst = min_thirst.min(thirst(&app, ids.villager).0);
        let n = needs(&app, ids.villager);
        min_hunger = min_hunger.min(n.hunger.value);
        min_energy = min_energy.min(n.energy.value);
    }
    assert!(min_thirst > 0, "min thirst {min_thirst}");
    assert!(min_hunger > 0, "min hunger {min_hunger}");
    assert!(min_energy > 0, "min energy {min_energy}");
}
