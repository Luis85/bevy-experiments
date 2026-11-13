//! Cashless bodily-need recovery uses the same physical gathering as a player.
//! Operator setup isolates the decision at a reachable source; it is not route
//! discovery or unaided-play evidence, and creates neither items nor currency.
use bevy_app::App;
use island_content::village::{
    self, CoinPurse, Role, VillageState, Villager, Wallet, catalog, economy, market,
};
use npc_framework::interaction::{CarriedBy, items};
use npc_framework::kernel::journal::{Journal, set_journal};
use npc_framework::prelude::*;

fn money(app: &mut App) -> u64 {
    let wallets: u64 = app
        .world_mut()
        .query::<&Wallet>()
        .iter(app.world())
        .map(|wallet| u64::from(wallet.coins))
        .sum();
    let purses: u64 = app
        .world_mut()
        .query::<&CoinPurse>()
        .iter(app.world())
        .map(|purse| u64::from(purse.coins))
        .sum();
    wallets + purses + u64::from(app.world().resource::<VillageState>().fund)
}

#[test]
fn a_cashless_thirsty_farmer_physically_gathers_and_drinks_water_without_trade_or_a_stipend() {
    let mut app = island_content::new_sim_app(7, "living_village");
    let initial_money = money(&mut app);
    let ids = app.world().resource::<island_content::TestbedIds>();
    let ada_id = ids.id("Ada");
    let hero_id = app.world().resource::<VillageState>().played;
    let patch_id = ids.id(&format!("patch_{}", catalog::WATER));
    let ada = app.world().resource::<SimIndex>().get(ada_id).unwrap();
    let hero = app.world().resource::<SimIndex>().get(hero_id).unwrap();
    let patch = app.world().resource::<SimIndex>().get(patch_id).unwrap();
    assert!(
        app.world().get::<Brain>(ada).is_some(),
        "the farmer's real life tree must choose recovery"
    );
    let others: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(id, entity)| {
            **id != ada_id
                && app
                    .world()
                    .get::<Villager>(**entity)
                    .is_some_and(|person| person.role != Role::Visitor)
        })
        .map(|(_, entity)| *entity)
        .collect();
    for other in others {
        app.world_mut().entity_mut(other).remove::<Brain>();
    }

    // Transfer the original water entities, rather than consuming/replacing
    // them. The idle visitor does not advertise any of those goods for sale.
    let water: Vec<_> = items::carried_items(app.world(), ada)
        .into_iter()
        .filter(|id| {
            app.world()
                .resource::<SimIndex>()
                .get(*id)
                .and_then(|entity| app.world().get::<Item>(entity))
                .is_some_and(|item| item.kind == catalog::WATER)
        })
        .collect();
    assert!(
        !water.is_empty(),
        "fixture expects the constructor's real water reserve"
    );
    for id in &water {
        let item = app.world().resource::<SimIndex>().get(*id).unwrap();
        items::can_store(app.world(), hero, item).unwrap();
        items::put_in_inventory(app.world_mut(), hero, item);
        assert_eq!(app.world().get::<CarriedBy>(item).unwrap().holder, hero_id);
    }
    assert_eq!(economy::count(app.world(), ada, catalog::WATER), 0);
    assert!(
        !market::sells(app.world(), hero, catalog::WATER),
        "the transferred reserve must remain unlisted"
    );
    let cash = app.world().get::<Wallet>(ada).unwrap().coins;
    assert_eq!(
        cash, 200,
        "fixture starts with the normal finite resident endowment"
    );
    app.world_mut().get_mut::<Wallet>(ada).unwrap().coins = 0;
    let fund = app.world().resource::<VillageState>().fund;
    app.world_mut().resource_mut::<VillageState>().fund = fund.checked_add(cash).unwrap();
    assert_eq!(
        money(&mut app),
        initial_money,
        "the fixture only moves existing currency"
    );
    let pos = *app.world().get::<SimPos>(patch).unwrap();
    app.world_mut().entity_mut(ada).insert(pos);
    let source_before = app
        .world()
        .get::<village::ResourcePatch>(patch)
        .unwrap()
        .remaining;
    let produced_before = app
        .world()
        .resource::<VillageState>()
        .produced
        .get(catalog::WATER)
        .copied()
        .unwrap_or(0);
    let transactions_before = app.world().resource::<VillageState>().transactions;
    set_journal(app.world_mut(), true, false);
    submit(
        app.world_mut(),
        None,
        SimCommand::SetNeed {
            unit: ada_id.0,
            need: "Thirst".into(),
            value: 20_000,
        },
    )
    .unwrap();
    for _ in 0..1000 {
        run_ticks(app.world_mut(), 1);
        assert_eq!(
            app.world().get::<Wallet>(ada).unwrap().coins,
            0,
            "fixture isolation failed: another actor or a stipend funded recovery"
        );
        assert_eq!(
            money(&mut app),
            initial_money,
            "recovery must not create or destroy currency"
        );
        if economy::need(app.world(), ada, "Thirst") > 50_000 {
            break;
        }
    }
    let journal = app.world().resource::<Journal>();
    let count_effect = |name: &str| {
        journal
            .entries
            .iter()
            .filter(|entry| {
                entry.unit == Some(ada_id)
                    && matches!(&entry.kind, JournalKind::Effect { effect, applied: true }
            if effect.get("type").and_then(serde_json::Value::as_str) == Some(name) && effect.get("kind").and_then(serde_json::Value::as_str) == Some(catalog::WATER))
            })
            .count()
    };
    let consumed = count_effect("VillageConsumed");
    let gathered = count_effect("VillageGathered");
    let thirst = economy::need(app.world(), ada, "Thirst");
    assert!(
        thirst > 50_000,
        "a cashless thirsty farmer standing at a live water source must gather and drink; thirst={thirst}, consumed={consumed}, gathered={gathered}"
    );
    assert_eq!(
        consumed, 1,
        "recovery must use exactly one shared water-consumption effect"
    );
    assert_eq!(gathered, 1, "recovery must finish exactly one real gather");
    assert_eq!(
        app.world()
            .get::<village::ResourcePatch>(patch)
            .unwrap()
            .remaining,
        source_before - 1
    );
    assert_eq!(
        app.world()
            .resource::<VillageState>()
            .produced
            .get(catalog::WATER)
            .copied()
            .unwrap_or(0),
        produced_before + 1
    );
    assert_eq!(
        app.world().resource::<VillageState>().transactions,
        transactions_before,
        "cashless source recovery is not a hidden or free trade"
    );
    assert_eq!(economy::count(app.world(), ada, catalog::WATER), 0);
    for id in water {
        let item = app
            .world()
            .resource::<SimIndex>()
            .get(id)
            .expect("the original reserve still exists");
        assert_eq!(app.world().get::<CarriedBy>(item).unwrap().holder, hero_id);
    }
    assert!(!market::sells(app.world(), hero, catalog::WATER));
    assert_eq!(money(&mut app), initial_money);
}
