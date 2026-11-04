//! One unchanged living world across two configured garment lifetimes.
//! The idle visitor is not an unaided-player fixture. Only the operator clock
//! advances; resident needs, money, stocks, prices and jobs are never edited.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::interaction::items;
use npc_framework::prelude::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use testbed_content::village::{
    self, CoinPurse, Role, VillageState, Villager, Vitality, Wallet, catalog,
};

fn money(app: &mut App) -> u64 {
    let wallets = app
        .world_mut()
        .query::<&Wallet>()
        .iter(app.world())
        .map(|wallet| u64::from(wallet.coins))
        .sum::<u64>();
    let purses = app
        .world_mut()
        .query::<&CoinPurse>()
        .iter(app.world())
        .map(|purse| u64::from(purse.coins))
        .sum::<u64>();
    wallets + purses + u64::from(app.world().resource::<VillageState>().fund)
}
fn residents(app: &App) -> Vec<(SimId, Entity)> {
    app.world()
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| {
            app.world()
                .get::<Villager>(**entity)
                .is_some_and(|person| person.role != Role::Visitor)
        })
        .map(|(id, entity)| (*id, *entity))
        .collect()
}
fn traded(app: &App, kind: &str) -> u64 {
    app.world()
        .resource::<village::market::MarketState>()
        .commodities
        .get(kind)
        .map_or(0, |row| row.total_units)
}
fn food_traded(app: &App) -> u64 {
    ["bread", catalog::COOKED_MEAL, catalog::BERRIES]
        .into_iter()
        .map(|kind| traded(app, kind))
        .sum()
}
fn equipped(app: &App) -> BTreeMap<SimId, SimId> {
    residents(app)
        .into_iter()
        .filter_map(|(id, entity)| {
            items::equipped_matching(app.world(), entity, catalog::WARM_CLOTHES)
                .map(|item| (id, item))
        })
        .collect()
}

#[test]
fn mature_economy_replaces_worn_garments_with_new_production_and_real_sales() {
    let mut app = testbed_content::new_sim_app(7, "living_village");
    let initial_money = money(&mut app);
    let lifetime = u64::from(catalog::CLOTHING_DURABILITY);
    let end = lifetime * 2;
    let day = app.world().resource::<SimConfig>().day_length;
    let mut checkpoints: BTreeSet<_> = (day..=end).step_by(day as usize).collect();
    checkpoints.extend([lifetime, end]);
    let mut previous = 0;
    let mut baseline = None;
    let mut evidence = Vec::new();
    for checkpoint in checkpoints {
        run_ticks(app.world_mut(), checkpoint - previous);
        previous = checkpoint;
        let people = residents(&app);
        assert_eq!(people.len(), 12);
        let dead: Vec<_> = people
            .iter()
            .filter_map(|(id, entity)| {
                let vitality = app.world().get::<Vitality>(*entity).unwrap();
                (vitality.health <= 0 || vitality.died_at.is_some()).then_some((
                    *id,
                    app.world().get::<Villager>(*entity).unwrap().name.clone(),
                    vitality.died_at,
                ))
            })
            .collect();
        assert!(dead.is_empty(), "resident death at{checkpoint}: {dead:?}");
        assert_eq!(
            money(&mut app),
            initial_money,
            "currency changed at{checkpoint}"
        );
        let coats = equipped(&app);
        let made = app
            .world()
            .resource::<VillageState>()
            .produced
            .get(catalog::WARM_CLOTHES)
            .copied()
            .unwrap_or(0);
        let sold = traded(&app, catalog::WARM_CLOTHES);
        evidence.push(json!({"tick":checkpoint,"coats_equipped":coats.len(),"clothes_made":made,"clothes_sold":sold,"food_traded":food_traded(&app)}));
        if checkpoint == lifetime {
            baseline = Some((made, sold, food_traded(&app), coats));
        }
    }
    let (before_made, before_sold, before_food, old) = baseline.unwrap();
    let new = equipped(&app);
    let replaced = old
        .iter()
        .filter_map(|(actor, item)| {
            (app.world().resource::<SimIndex>().get(*item).is_none()
                && new
                    .get(actor)
                    .is_some_and(|replacement| replacement != item))
            .then_some(())
        })
        .count();
    let made = app
        .world()
        .resource::<VillageState>()
        .produced
        .get(catalog::WARM_CLOTHES)
        .copied()
        .unwrap_or(0);
    assert!(
        made > before_made,
        "new garment production stopped after the first lifetime: {evidence:?}"
    );
    assert!(
        traded(&app, catalog::WARM_CLOTHES) > before_sold,
        "replacement clothing sales stopped: {evidence:?}"
    );
    assert!(
        replaced > 0,
        "at least one resident must replace its destroyed original garment with another real item: {evidence:?}"
    );
    assert!(
        food_traded(&app) > before_food,
        "late food circulation stopped: {evidence:?}"
    );
}
