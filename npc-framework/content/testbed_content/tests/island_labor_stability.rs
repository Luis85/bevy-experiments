//! Labour hysteresis on the regional island (`living_world`).
//!
//! A native capture showed hunters and tailors swapping trades in the same
//! pass and residents returning to a trade within hours. The rules against that:
//! a resident does not return to a profession left less than two days ago
//! unless a persistent pay gap of twice the usual threshold pulls them back, a
//! seat somebody just left stays open for 2400 ticks, and a worker whose own
//! goods are short moves only towards a larger shortage. An empty essential
//! workplace overrides them.
use bevy_ecs::prelude::*;
use npc_framework::kernel::journal::{Journal, JournalKind, set_journal};
use npc_framework::prelude::*;
use std::collections::BTreeMap;
use testbed_content::village::{
    self, Role, Villager, economy,
    frontier::labor::{
        self, Employment, PROFESSION_RETURN_TICKS, ResidentProfile, VACATED_SEAT_TICKS,
    },
};

/// Three quarters of a day: the unfixed world made its first quick return to a
/// trade well inside this stretch.
const STRETCH_TICKS: u64 = 18_000;
const SAMPLE: u64 = 300;
const ESSENTIAL: [Role; 5] = [
    Role::Farmer,
    Role::Miller,
    Role::Baker,
    Role::WaterKeeper,
    Role::Woodcutter,
];

#[derive(Clone, Debug)]
struct Change {
    tick: u64,
    resident: SimId,
    name: String,
    from: (Role, SimId),
    to: (Role, SimId),
    /// The destination was an essential workplace nobody staffed.
    floor: bool,
    /// A persistent realised-margin gap moved the resident.
    margin: bool,
}

fn residents(world: &World) -> Vec<(SimId, Entity)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| world.get::<ResidentProfile>(**entity).is_some())
        .map(|(id, entity)| (*id, *entity))
        .collect()
}

fn job_changes(world: &mut World, end: u64) -> Vec<Change> {
    let people = residents(world);
    let job = |world: &World, actor: Entity| {
        let person = world.get::<Villager>(actor).expect("resident");
        (person.role, person.workplace)
    };
    let mut current: BTreeMap<SimId, (Role, SimId)> = people
        .iter()
        .map(|(id, actor)| (*id, job(world, *actor)))
        .collect();
    let mut changes = Vec::new();
    while world.resource::<SimClock>().tick < end {
        run_ticks(world, SAMPLE);
        let before = current.clone();
        for (id, actor) in &people {
            let now = job(world, *actor);
            let was = before[id];
            if now == was {
                continue;
            }
            let staffed = before
                .iter()
                .any(|(other, held)| other != id && held.1 == now.1);
            changes.push(Change {
                tick: world
                    .get::<Employment>(*actor)
                    .expect("employment")
                    .changed_at,
                resident: *id,
                name: world
                    .get::<Villager>(*actor)
                    .expect("resident")
                    .name
                    .clone(),
                from: was,
                to: now,
                floor: ESSENTIAL.contains(&now.0) && !staffed,
                margin: world
                    .get::<Employment>(*actor)
                    .expect("employment")
                    .last_reason
                    .contains("margin"),
            });
            current.insert(*id, now);
        }
    }
    changes.sort_by_key(|change| (change.tick, change.resident));
    changes
}

#[test]
fn residents_do_not_flip_back_or_swap_trades_in_one_pass() {
    let mut app = testbed_content::new_sim_app(7, "living_world");
    let changes = job_changes(app.world_mut(), STRETCH_TICKS);
    println!(
        "LABOR_STABILITY ticks={STRETCH_TICKS} job_changes={}",
        changes.len()
    );
    assert!(!changes.is_empty(), "nobody changed trade at all");
    for (index, change) in changes.iter().enumerate() {
        for earlier in &changes[..index] {
            // A to B to A: back to a profession left less than two days ago.
            if earlier.resident == change.resident
                && earlier.from.0 == change.to.0
                && change.tick < earlier.tick + PROFESSION_RETURN_TICKS
            {
                assert!(
                    change.floor || change.margin,
                    "{} returned to {} at tick {} after leaving it at tick {}",
                    change.name,
                    change.to.0.name(),
                    change.tick,
                    earlier.tick
                );
            }
            // Opposite moves between the same two workplaces in one pass.
            if earlier.resident != change.resident
                && earlier.from.1 == change.to.1
                && change.tick < earlier.tick + VACATED_SEAT_TICKS
            {
                assert!(
                    change.floor,
                    "{} took the {} seat at tick {} that {} left at tick {}",
                    change.name,
                    change.to.0.name(),
                    change.tick,
                    earlier.name,
                    earlier.tick
                );
            }
        }
    }
    // The memory behind the rule is saved, hashed state and stays bounded.
    for (_, actor) in residents(app.world()) {
        let employment = app.world().get::<Employment>(actor).expect("employment");
        assert!(employment.departures.len() <= labor::MAX_DEPARTURES);
    }
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}

#[test]
fn hunger_and_thirst_onsets_are_journaled_once_for_the_alerts_feed() {
    let mut app = testbed_content::new_sim_app(7, "living_world");
    set_journal(app.world_mut(), true, false);
    let (id, actor) = residents(app.world())[0];
    // A resident with nothing to eat or drink: one carrying water drinks in the
    // very think that notices the thirst, before any onset.
    for kind in [
        village::catalog::WATER,
        "bread",
        village::catalog::COOKED_MEAL,
        village::catalog::BERRIES,
    ] {
        let carried = economy::count(app.world(), actor, kind);
        if carried > 0 {
            economy::consume(app.world_mut(), actor, kind, carried).expect("carried stock");
        }
    }
    for need in ["Hunger", "Thirst"] {
        let value = economy::need(app.world(), actor, need);
        economy::restore(app.world_mut(), actor, need, -value);
    }
    run_ticks(app.world_mut(), 3);
    let count = |app: &bevy_app::App, kind: &str| {
        app.world()
            .resource::<Journal>()
            .entries
            .iter()
            .filter(|entry| {
                entry.unit == Some(id)
                    && matches!(&entry.kind, JournalKind::Effect { effect, applied: true }
                        if effect.get("type").and_then(serde_json::Value::as_str) == Some(kind))
            })
            .count()
    };
    assert_eq!(count(&app, "VillageStarvationOnset"), 1);
    assert_eq!(count(&app, "VillageThirstOnset"), 1);
}
