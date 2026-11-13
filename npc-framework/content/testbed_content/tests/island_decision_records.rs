//! Decision records: a `VillageDecision` journal effect whenever a resident's
//! running leaf changes. They explain the choice and never change the sim.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use serde_json::Value;
use std::collections::BTreeMap;
use testbed_content::village::{
    frontier::labor::ResidentProfile,
    resident::{self, DecisionTrace},
};

fn world() -> App {
    testbed_content::new_sim_app(7, "living_world")
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

/// Every `VillageDecision` in the journal as `(tick, unit, record)`.
fn decisions(world: &World) -> Vec<(u64, SimId, Value)> {
    world
        .resource::<Journal>()
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            JournalKind::Effect {
                effect,
                applied: true,
            } if effect.get("type").and_then(Value::as_str) == Some("VillageDecision") => Some((
                entry.tick,
                entry.unit?,
                serde_json::to_value(effect).expect("effect JSON"),
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn the_state_hash_is_identical_with_the_journal_on_and_off() {
    let mut quiet = world();
    let mut recorded = world();
    set_journal(quiet.world_mut(), false, false);
    // Verbose also journals every condition evaluation.
    set_journal(recorded.world_mut(), true, true);
    for checkpoint in 1..=5 {
        run_ticks(quiet.world_mut(), 300);
        run_ticks(recorded.world_mut(), 300);
        assert_eq!(
            state_hash(quiet.world_mut()),
            state_hash(recorded.world_mut()),
            "the journal changed the simulation by tick {}",
            checkpoint * 300
        );
    }
    let written = recorded.world().resource::<DecisionTrace>().recorded;
    assert!(
        written > 42,
        "only {written} decision records in 1500 ticks"
    );
    // The verbose journal is a ring buffer: older records have been evicted.
    let kept = decisions(recorded.world()).len() as u64;
    assert!(kept > 0 && kept <= written);
    assert_eq!(quiet.world().resource::<DecisionTrace>().recorded, 0);
    assert!(decisions(quiet.world()).is_empty());
}

#[test]
fn a_record_names_the_branch_and_why_each_higher_priority_was_passed_over() {
    let mut app = world();
    set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 600);
    let records = decisions(app.world());
    assert!(!records.is_empty());
    let mut failed_leaf = false;
    let mut by_unit: BTreeMap<SimId, Vec<&Value>> = BTreeMap::new();
    for (_, unit, record) in &records {
        by_unit.entry(*unit).or_default().push(record);
        assert_eq!(record["tree"], "island_life");
        let branch = record["branch"].as_str().expect("branch");
        assert!(branch.starts_with("Resident life > "), "{branch}");
        assert!(record["action"].as_str().is_some_and(|a| !a.is_empty()));
        assert!(record["activity"].is_string(), "{record}");
        let considered = record["considered"].as_array().expect("considered");
        // The first option of the tree (ADR 0041: freezing comes first) is
        // always considered first.
        if let Some(first) = considered.first()
            && !branch.starts_with("Resident life > Freezing")
        {
            assert_eq!(first["node"], "Freezing", "{record}");
            assert_eq!(first["rejected"], "life.freezing is false", "{record}");
        }
        for option in considered {
            assert!(option["node"].is_string() && option["rejected"].is_string());
        }
        failed_leaf |= considered.iter().any(|option| {
            option["rejected"]
                .as_str()
                .is_some_and(|why| why.contains("life.collect_found: nothing lost within reach"))
        });
    }
    assert!(
        failed_leaf,
        "no record explains a leaf that could not start"
    );
    // Rate limit: a resident is recorded only when its leaf changes.
    for (unit, records) in &by_unit {
        for pair in records.windows(2) {
            assert!(
                (&pair[0]["branch"], &pair[0]["param"]) != (&pair[1]["branch"], &pair[1]["param"]),
                "unit {} was recorded twice in a row for {}",
                unit.0,
                pair[0]["branch"]
            );
        }
    }
    let leaf_starts = app
        .world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|entry| matches!(entry.kind, JournalKind::LeafStart { .. }))
        .count();
    println!(
        "DECISION_RECORDS ticks=600 records={} leaf_starts={leaf_starts} residents={}",
        records.len(),
        by_unit.len()
    );
    assert!(
        records.len() * 2 < leaf_starts,
        "{} records for {leaf_starts} leaf starts is not rate-limited",
        records.len()
    );
}

#[test]
fn a_goap_record_carries_the_goal_and_the_plan_length() {
    let mut app = world();
    set_journal(app.world_mut(), true, false);
    let mut plan = None;
    while app.world().resource::<SimClock>().tick < 4_800 && plan.is_none() {
        run_ticks(app.world_mut(), 300);
        plan = decisions(app.world())
            .into_iter()
            .find(|(_, _, record)| record["action"] == "life.pursue_goal");
    }
    let (tick, unit, record) = plan.expect("no resident pursued a GOAP goal in 4800 ticks");
    println!("GOAP_RECORD tick={tick} unit={} {record}", unit.0);
    assert!(
        record["plan"]["goal"]
            .as_str()
            .is_some_and(|goal| !goal.is_empty()),
        "{record}"
    );
    assert!(
        record["plan"]["steps"]
            .as_u64()
            .is_some_and(|steps| steps >= 1)
    );
    assert!(record["plan"]["first"].is_string());
    assert!(
        record["branch"]
            .as_str()
            .is_some_and(|branch| branch.ends_with("Profession > Pursue production goal")),
        "{record}"
    );
}

#[test]
fn explain_reads_the_running_branch_without_the_journal() {
    let mut app = world();
    set_journal(app.world_mut(), false, false);
    run_ticks(app.world_mut(), 200);
    let before = state_hash(app.world_mut());
    let mut explained = 0;
    for (_, actor) in residents(app.world()) {
        let Some(record) = resident::explain(app.world(), actor) else {
            continue;
        };
        explained += 1;
        assert!(
            record["branch"]
                .as_str()
                .is_some_and(|branch| branch.starts_with("Resident life")),
            "{record}"
        );
        assert!(record["considered"].is_array());
    }
    assert!(
        explained > 30,
        "only {explained} residents had a running leaf"
    );
    assert_eq!(state_hash(app.world_mut()), before, "explain is read-only");
}
