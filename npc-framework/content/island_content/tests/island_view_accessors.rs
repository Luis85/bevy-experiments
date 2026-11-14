//! The read accessors views use (`village::view`) and the stability of the
//! civic fund.
mod life_support;

use bevy_ecs::prelude::*;
use island_content::village::{
    Villager,
    resident::{self, BlockKind, ResidentTraits},
    view,
};
use life_support::*;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use serde_json::json;

#[test]
fn a_resident_at_home_at_night_is_not_labelled_as_going_home() {
    let mut app = world_with(|world| {
        short_days(world, 2_400);
        only_weather(world, "clear");
    });
    // Night falls at 70% of the day.
    run_ticks(app.world_mut(), 1_900);
    let mut at_home = 0;
    let mut labels = std::collections::BTreeSet::new();
    for _ in 0..6 {
        run_ticks(app.world_mut(), 60);
        for (id, actor) in all_residents(app.world()) {
            let Some(label) = view::activity_label(app.world(), actor) else {
                continue;
            };
            labels.insert(label);
            let person = app.world().get::<Villager>(actor).expect("villager");
            let here = *app.world().get::<SimPos>(actor).expect("position");
            let home = here.dist2(person.home) <= 1_500_i64.pow(2);
            if home {
                assert_ne!(label, "going home", "resident {} is home already", id.0);
            }
            if label == "asleep at home" || label == "at home" {
                assert!(home);
                at_home += 1;
            }
        }
    }
    assert!(at_home > 0, "nobody was at home at night: {labels:?}");
    assert!(labels.contains("asleep at home"), "{labels:?}");
    assert_eq!(view::activity_label(app.world(), Entity::PLACEHOLDER), None);
}

#[test]
fn the_latest_decision_is_structured_with_a_does_not_apply_flag() {
    let mut app = world_with(|_| {});
    let (id, actor) = all_residents(app.world())[4];
    assert_eq!(
        view::latest_decision(app.world(), actor),
        None,
        "nothing runs yet"
    );
    // Without the journal the answer is read from the live tree.
    set_journal(app.world_mut(), false, false);
    run_ticks(app.world_mut(), 60);
    let live = view::latest_decision(app.world(), actor).expect("live decision");
    assert!(!live.recorded && live.tick.is_none());
    assert_eq!(live.tree, "island_life");
    assert_eq!(live.branch[0], "Resident life");
    assert!(live.action.starts_with("life."));
    assert!(!live.considered.is_empty());
    assert!(live.considered.iter().all(|option| option.reason.is_none()));
    // With the journal the record of the moment of choice is kept.
    set_journal(app.world_mut(), true, false);
    let mut recorded = None;
    for _ in 0..40 {
        run_ticks(app.world_mut(), 20);
        if let Some(decision) = view::latest_decision(app.world(), actor)
            && decision.recorded
        {
            recorded = Some(decision);
            break;
        }
    }
    let decision = recorded.expect("a recorded decision");
    assert!(decision.tick.is_some());
    assert_eq!(decision.activity, view::activity_label(app.world(), actor));
    let record = effects(app.world(), "VillageDecision")
        .into_iter()
        .rfind(|record| record["node"] == json!(decision.node))
        .expect("the journal has the same decision");
    assert_eq!(record["action"], json!(decision.action));
    assert_eq!(record["branch"], json!(decision.branch.join(" > ")));
    assert_eq!(
        record["considered"].as_array().map(Vec::len),
        Some(decision.considered.len())
    );
    // A false guard is a flag, not a sentence.
    let skipped: Vec<_> = decision
        .considered
        .iter()
        .filter(|option| option.does_not_apply)
        .collect();
    assert!(!skipped.is_empty());
    for option in &skipped {
        assert!(option.condition.is_some() && option.reason.is_none());
    }
    let first = &decision.considered[0];
    // The tree's first option (ADR 0041: freezing comes first).
    assert_eq!(first.node, "Freezing");
    assert_eq!(first.condition.as_deref(), Some("life.freezing"));
    assert!(first.does_not_apply);
    // An option that applied and could not start says why.
    for option in decision
        .considered
        .iter()
        .filter(|option| !option.does_not_apply)
    {
        assert!(
            option
                .reason
                .as_ref()
                .is_some_and(|reason| !reason.is_empty())
        );
    }
    let data = serde_json::to_value(&decision).expect("plain data");
    assert!(data["considered"][0].get("rejected").is_none());
    assert_eq!(data["considered"][0]["does_not_apply"], true);
    let _ = id;
}

#[test]
fn effective_schedule_blocks_follow_chronotype_and_diligence() {
    let mut app = world_with(|_| {});
    let (_, actor) = all_residents(app.world())[0];
    let set = |app: &mut bevy_app::App, chronotype: i32, diligence: i32| {
        *app.world_mut()
            .get_mut::<ResidentTraits>(actor)
            .expect("traits") = ResidentTraits {
            diligence,
            sociability: 50,
            chronotype,
            thrift: 50,
        };
    };
    set(&mut app, 0, 50);
    let plain = view::schedule_blocks(app.world(), actor);
    assert_eq!(plain.len(), 5);
    let work = plain
        .iter()
        .find(|block| block.kind == BlockKind::Work)
        .expect("work");
    assert_eq!((work.from_permille, work.to_permille), (0, 700));
    assert_eq!((work.from_tick, work.to_tick), (0, 16_800));
    assert!(work.active, "dawn is inside the work block");
    let leisure = plain
        .iter()
        .find(|block| block.kind == BlockKind::Leisure)
        .expect("leisure");
    // The evening starts at 620 since the review fix pass (npc-ai#3).
    assert_eq!((leisure.from_permille, leisure.to_permille), (620, 700));
    // An owl's whole day is later; a lark's wraps before dawn.
    set(&mut app, 100, 50);
    let owl = view::schedule_blocks(app.world(), actor);
    let work = owl
        .iter()
        .find(|block| block.kind == BlockKind::Work)
        .expect("work");
    assert_eq!((work.from_permille, work.to_permille), (25, 725));
    assert!(!work.active, "an owl is not at work at dawn");
    set(&mut app, -100, 50);
    let lark = view::schedule_blocks(app.world(), actor);
    let sleep = lark
        .iter()
        .find(|block| block.kind == BlockKind::Sleep)
        .expect("sleep");
    assert_eq!((sleep.from_permille, sleep.to_permille), (675, 975));
    // Diligence delays leisure; at 100 the delay swallows half of it, and a
    // longer delay than the block removes it.
    set(&mut app, 0, 100);
    let diligent = view::schedule_blocks(app.world(), actor);
    let leisure = diligent
        .iter()
        .find(|block| block.kind == BlockKind::Leisure)
        .expect("leisure");
    assert_eq!(leisure.from_permille, 660);
    // Every view agrees with the condition the tree reads, all day long.
    set(&mut app, 37, 81);
    for _ in 0..24 {
        run_ticks(app.world_mut(), 1_000);
        for block in view::schedule_blocks(app.world(), actor) {
            assert_eq!(
                block.active,
                resident::in_block(app.world(), actor, block.kind)
            );
            let at = resident::day_permille(app.world());
            let inside = if block.from_permille <= block.to_permille {
                (block.from_permille..block.to_permille).contains(&at)
            } else {
                at >= block.from_permille || at < block.to_permille
            };
            if inside {
                assert!(block.active, "{block:?} at {at}");
            }
        }
    }
    let delayed_away = world_with(|world| {
        world["living_world"]["behavior"] = json!({"schedule": {"diligence_shift": 200}});
    });
    let (_, somebody) = all_residents(delayed_away.world())
        .into_iter()
        .find(|(_, actor)| resident::traits(delayed_away.world(), *actor).diligence >= 70)
        .expect("a diligent founder");
    assert!(
        view::schedule_blocks(delayed_away.world(), somebody)
            .iter()
            .all(|block| block.kind != BlockKind::Leisure),
        "a delay longer than the block removes it"
    );
}
