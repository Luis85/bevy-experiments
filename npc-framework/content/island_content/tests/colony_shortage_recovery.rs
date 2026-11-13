//! A declared temporary upstream shortage, with ordinary replenishment and no rescue grants.
use bevy_app::App;
use island_content::village::{
    self, CoinPurse, ResourcePatch, Role, VillageState, Villager, Vitality, Wallet,
};
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use npc_framework::snapshot;

fn scene() -> App {
    island_content::new_sim_app(7, "island_colony")
}
fn money(app: &App) -> u64 {
    app.world()
        .resource::<SimIndex>()
        .map
        .values()
        .map(|entity| {
            u64::from(
                app.world()
                    .get::<Wallet>(*entity)
                    .map_or(0, |wallet| wallet.coins),
            ) + u64::from(
                app.world()
                    .get::<CoinPurse>(*entity)
                    .map_or(0, |purse| purse.coins),
            )
        })
        .sum::<u64>()
        + u64::from(app.world().resource::<VillageState>().fund)
}
fn made(app: &App, kind: &str) -> u64 {
    app.world()
        .resource::<VillageState>()
        .produced
        .get(kind)
        .copied()
        .unwrap_or(0)
}
fn healthy_residents(app: &App) {
    let residents: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .filter(|entity| {
            app.world()
                .get::<Villager>(**entity)
                .is_some_and(|person| person.role != Role::Visitor)
        })
        .collect();
    assert_eq!(residents.len(), 12);
    for entity in residents {
        let vitality = app.world().get::<Vitality>(*entity).unwrap();
        assert!(
            vitality.died_at.is_none() && vitality.health > 0,
            "{} died during the bounded shortage at {}",
            app.world().get::<Villager>(*entity).unwrap().name,
            app.world().resource::<SimClock>().tick
        );
    }
}

#[test]
fn a_temporary_timber_shortage_recovers_real_output_and_replays_across_a_mid_shortage_save() {
    let mut app = scene();
    let initial_money = money(&app);
    let day = app.world().resource::<SimConfig>().day_length;
    run_ticks(app.world_mut(), day);
    healthy_residents(&app);
    assert_eq!(money(&app), initial_money);
    let shortage_start = app.world().resource::<SimClock>().tick;
    let replenish_at = shortage_start + 2400;
    let patch_ids: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, entity)| {
            app.world()
                .get::<ResourcePatch>(*entity)
                .filter(|patch| patch.kind == "log")
                .map(|_| *id)
        })
        .collect();
    assert!(!patch_ids.is_empty());
    // Fixture boundary: exhaust only natural timber patches and delay their
    // existing replenishment. Carried goods, wallets, prices, brains, needs and
    // production targets remain untouched; no recovery command grants anything.
    for id in &patch_ids {
        let entity = app.world().resource::<SimIndex>().get(*id).unwrap();
        let mut patch = app.world_mut().get_mut::<ResourcePatch>(entity).unwrap();
        patch.remaining = 0;
        patch.replenish_at = replenish_at;
    }
    let timber_before = made(&app, "log");
    run_ticks(app.world_mut(), 1200);
    assert_eq!(
        made(&app, "log"),
        timber_before,
        "an exhausted patch cannot manufacture timber"
    );
    let saved = snapshot::save(app.world_mut()).unwrap();
    let mut resumed = scene();
    let report = snapshot::load(resumed.world_mut(), &saved, snapshot::LoadMode::Strict).unwrap();
    assert!(report.defaulted.is_empty() && report.skipped.is_empty());
    assert_eq!(state_hash(app.world_mut()), state_hash(resumed.world_mut()));

    let mut replenished = false;
    let mut recovery = None;
    while app.world().resource::<SimClock>().tick < replenish_at + day {
        run_ticks(app.world_mut(), 600);
        run_ticks(resumed.world_mut(), 600);
        assert_eq!(state_hash(app.world_mut()), state_hash(resumed.world_mut()));
        assert_eq!(money(&app), initial_money);
        assert_eq!(money(&resumed), initial_money);
        healthy_residents(&app);
        healthy_residents(&resumed);
        if app.world().resource::<SimClock>().tick <= replenish_at {
            assert_eq!(made(&app, "log"), timber_before);
        } else if recovery.is_none() {
            recovery = Some((
                made(&app, "plank"),
                made(&app, "bread") + made(&app, village::catalog::COOKED_MEAL),
                app.world().resource::<VillageState>().transactions,
            ));
        }
        replenished |= patch_ids.iter().all(|id| {
            let entity = app.world().resource::<SimIndex>().get(*id).unwrap();
            app.world()
                .get::<ResourcePatch>(entity)
                .unwrap()
                .replenish_at
                > replenish_at
        });
    }
    let (planks_at_recovery, food_at_recovery, trades_at_recovery) = recovery.unwrap();
    assert!(replenished, "ordinary patch replenishment never resumed");
    assert!(
        made(&app, "log") > timber_before,
        "timber gathering did not resume within one day"
    );
    assert!(
        made(&app, "plank") > planks_at_recovery,
        "downstream plank production did not resume within one day"
    );
    assert!(
        made(&app, "bread") + made(&app, village::catalog::COOKED_MEAL) > food_at_recovery,
        "finished food production did not continue after replenishment"
    );
    assert!(
        app.world().resource::<VillageState>().transactions > trades_at_recovery,
        "physical trading did not continue after replenishment"
    );
    println!(
        "COLONY_SHORTAGE_EVIDENCE {}",
        serde_json::json!({
            "seed":7, "shortage_start":shortage_start, "replenished_at":replenish_at,
            "final_tick":app.world().resource::<SimClock>().tick, "currency":initial_money,
            "new_timber":made(&app,"log")-timber_before, "recovery_planks":made(&app,"plank")-planks_at_recovery,
            "recovery_trades":app.world().resource::<VillageState>().transactions-trades_at_recovery,
            "residents_alive":12, "snapshot_continuation_equal":true, "recovery_grants":0
        })
    );
}
