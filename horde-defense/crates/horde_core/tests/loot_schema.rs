//! Loot schema (W1-21): the saved auto-sell rule on `Campaign`, the drop
//! tracking on `Attempt` and `Report`, the auto-sold receipt on `Report` and
//! `RunRecord`, `ParallelBattle.pending_auto_sell`, and their save
//! validation. Every field is skipped at its default. Since W2-06 every
//! deploy writes `drop_tier` and the drop site and settlement fill the drop
//! tracking (`tests/loot_tracking.rs`); the other fields have no writer yet,
//! so these tests stage the values as a save would.
mod support;
use horde_core::{
    campaign::{AutoSellRule, AutoSoldLine, AutoSoldReceipt, MAX_AUTO_SOLD_LINES},
    command::Command,
    content::{ItemDefId, Mode},
    quality::Quality,
    sim::{Game, MAX_RUN_DROPS, SAVE_VERSION, SaveError},
};
use serde_json::{Value, json};
use support::strategic_game;

/// A strategic game with a live and a settled expedition, a withdrawn root
/// run (its report and run record) and a live root Personal attempt.
struct LootGame {
    game: Game,
    live: usize,
    settled: usize,
}

fn loot_game() -> LootGame {
    let mut g = strategic_game();
    let country = g.campaign().world.cleared[0].clone();
    let [first, second] = [(); 2].map(|()| {
        g.with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
            .unwrap()
    });
    let live = g.deploy_survivor(first, &country, 0).unwrap();
    let settled = g.deploy_survivor(second, &country, 0).unwrap();
    g.recall_survivor(settled).unwrap();
    g.deploy(0, Mode::Personal).unwrap();
    g.apply(&Command::Withdraw).unwrap();
    // Deploying clears the report; restore it as a save made later would hold.
    let report = g.last_result().cloned();
    g.deploy(0, Mode::Personal).unwrap();
    *g.last_result_mut() = report;
    let position = |id: u64| g.battles().iter().position(|b| b.id == id).unwrap();
    let (live, settled) = (position(live), position(settled));
    assert!(!g.battles()[live].settled && g.battles()[settled].settled);
    assert!(g.last_result().is_some() && g.attempt().is_some());
    LootGame {
        game: g,
        live,
        settled,
    }
}

fn counts(pairs: &[(&str, u32)]) -> Value {
    let mut map = serde_json::Map::new();
    for quality in ["Poor", "Common", "Uncommon", "Rare", "Epic", "Legendary"] {
        let n = pairs.iter().find(|(q, _)| *q == quality).map_or(0, |p| p.1);
        map.insert(quality.into(), json!(n));
    }
    Value::Object(map)
}

/// The staged save: every loot field at a valid non-default value. `ids`
/// are three parent item ids that no duty reserves: the root attempt drops
/// `ids[0]` and `ids[1]`, and the live expedition's pending sale holds `ids[2]`.
struct Staged {
    value: Value,
    ids: [u64; 3],
    live: usize,
    settled: usize,
}

fn staged() -> Staged {
    let LootGame {
        game,
        live,
        settled,
    } = loot_game();
    let mut value = serde_json::to_value(&game).unwrap();
    let reserved = game.campaign().reserved_items.clone();
    let free: Vec<u64> = (1..game.campaign().next_item_id)
        .filter(|id| !reserved.contains(id))
        .collect();
    let ids = [free[0], free[1], free[2]];
    value["campaign"]["auto_sell"] = json!({
        "enabled": true, "sell_below": "Rare", "slots": [true, false, true, true],
        "keep_upgrades": false, "apply_to_npc_runs": false
    });
    let attempt = &mut value["attempt"];
    attempt["loot"] = json!(2);
    attempt["drop_ids"] = json!([ids[0], ids[1]]);
    attempt["drop_tier"] = json!(2);
    let report = &mut value["last_result"];
    report["loot"] = json!(3);
    report["drop_ids"] = json!([ids[1], ids[0]]);
    report["loot_by_quality"] = counts(&[("Common", 1), ("Rare", 1)]);
    report["auto_sold"] = json!({
        "count": 1, "coins": 12, "by_quality": counts(&[("Poor", 1)]),
        "lines": [{"definition": "fire_axe", "level": 1, "quality": "Poor", "coins": 12}],
        "kept_on_overflow": 1
    });
    let record = value["campaign"]["run_history"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap();
    record["loot"] = json!(3);
    record["auto_sold_count"] = json!(1);
    record["auto_sold_coins"] = json!(12);
    value["battles"][live]["pending_auto_sell"] = json!([ids[2]]);
    value["battles"][live]["game"]["attempt"]["drop_tier"] = json!(1);
    Staged {
        value,
        ids,
        live,
        settled,
    }
}

fn load(value: Value) -> Result<Game, SaveError> {
    Game::from_payload(value, SAVE_VERSION)
}

/// Loads every row and fails with the names of the rows that loaded or
/// failed with anything but `SaveError::Invalid`.
fn assert_each_invalid(rows: Vec<(&str, Value)>) {
    let loaded: Vec<String> = rows
        .into_iter()
        .filter_map(|(name, value)| match load(value) {
            Err(SaveError::Invalid(_)) => None,
            Ok(_) => Some(format!("{name}: loaded")),
            Err(error) => Some(format!("{name}: {error}")),
        })
        .collect();
    assert_eq!(loaded, Vec::<String>::new(), "rows that did not fail");
}

/// `staged()` with `edit` applied.
fn edited(edit: impl FnOnce(&mut Staged)) -> Value {
    let mut s = staged();
    edit(&mut s);
    s.value
}

#[test]
fn drop_tracking_fields_round_trip_and_skip_when_empty() {
    let Staged {
        value, live, ids, ..
    } = staged();
    let loaded = load(value.clone()).unwrap_or_else(|e| panic!("staged loot save: {e}"));
    assert_eq!(serde_json::to_value(&loaded).unwrap(), value);
    let c = loaded.campaign();
    assert_eq!(
        c.auto_sell,
        AutoSellRule {
            enabled: true,
            sell_below: Quality::Rare,
            slots: [true, false, true, true],
            keep_upgrades: false,
            apply_to_npc_runs: false,
        }
    );
    let record = c.run_history.last().unwrap();
    assert_eq!((record.auto_sold_count, record.auto_sold_coins), (1, 12));
    let a = loaded.attempt().unwrap();
    assert_eq!(
        (a.drop_ids.clone(), a.drop_tier),
        (vec![ids[0], ids[1]], Some(2))
    );
    let r = loaded.last_result().unwrap();
    assert_eq!(r.drop_ids, [ids[1], ids[0]]);
    assert_eq!(
        (
            r.loot_by_quality[Quality::Common],
            r.loot_by_quality[Quality::Rare]
        ),
        (1, 1)
    );
    assert_eq!(
        r.auto_sold.lines,
        [AutoSoldLine {
            definition: ItemDefId("fire_axe".into()),
            level: 1,
            quality: Quality::Poor,
            coins: 12,
        }]
    );
    assert!(!r.auto_sold.is_empty());
    assert_eq!(loaded.battles()[live].pending_auto_sell, [ids[2]]);

    // Emptied again, every field leaves the save.
    let mut game = loaded;
    game.campaign_mut().auto_sell = AutoSellRule::default();
    let record = std::sync::Arc::make_mut(&mut game.campaign_mut().run_history)
        .last_mut()
        .unwrap();
    (record.auto_sold_count, record.auto_sold_coins) = (0, 0);
    let a = game.attempt_mut().unwrap();
    (a.drop_ids, a.drop_tier) = (Vec::new(), None);
    let r = game.last_result_mut().as_mut().unwrap();
    r.drop_ids.clear();
    r.loot_by_quality = Default::default();
    r.auto_sold = AutoSoldReceipt::default();
    for battle in game.battles_mut() {
        battle.pending_auto_sell.clear();
        if let Some(a) = battle.game_mut().attempt_mut() {
            a.drop_tier = None;
        }
    }
    assert_eq!(
        loot_keys(&serde_json::to_value(&game).unwrap()),
        Vec::<String>::new()
    );
}

/// Every saved location of a W1-21 key in `value` that is present, over the
/// root game and each worker game.
fn loot_keys(value: &Value) -> Vec<String> {
    fn check(object: &Value, path: &str, keys: &[&str], found: &mut Vec<String>) {
        for key in keys {
            if object.as_object().is_some_and(|o| o.contains_key(*key)) {
                found.push(format!("{path}.{key}"));
            }
        }
    }
    fn game(value: &Value, at: &str, found: &mut Vec<String>) {
        check(
            &value["campaign"],
            &format!("{at}campaign"),
            &["auto_sell"],
            found,
        );
        let history = value["campaign"]["run_history"]
            .as_array()
            .into_iter()
            .flatten();
        let pending = value["pending_runs"].as_array().into_iter().flatten();
        for (n, record) in history.chain(pending).enumerate() {
            let keys = ["auto_sold_count", "auto_sold_coins"];
            check(record, &format!("{at}record[{n}]"), &keys, found);
        }
        check(
            &value["attempt"],
            &format!("{at}attempt"),
            &["drop_ids", "drop_tier"],
            found,
        );
        let keys = ["drop_ids", "loot_by_quality", "auto_sold"];
        check(
            &value["last_result"],
            &format!("{at}last_result"),
            &keys,
            found,
        );
        let battles = value["battles"].as_array().into_iter().flatten();
        for (n, battle) in battles.enumerate() {
            check(
                battle,
                &format!("{at}battles[{n}]"),
                &["pending_auto_sell"],
                found,
            );
            game(&battle["game"], &format!("{at}battles[{n}].game."), found);
        }
    }
    let mut found = Vec::new();
    game(value, "", &mut found);
    found
}

#[test]
fn new_loot_fields_at_default_are_not_serialized() {
    assert_eq!(
        AutoSellRule::default(),
        AutoSellRule {
            enabled: false,
            sell_below: Quality::Uncommon,
            slots: [true; 4],
            keep_upgrades: true,
            apply_to_npc_runs: true,
        }
    );
    assert!(AutoSellRule::default().is_default());
    assert!(AutoSoldReceipt::default().is_empty());
    assert_eq!(MAX_AUTO_SOLD_LINES, 12);
    assert_eq!(MAX_RUN_DROPS, 4096);
    let LootGame {
        game,
        live,
        settled,
    } = loot_game();
    let mut value = serde_json::to_value(&game).unwrap();
    assert!(value["battles"][settled]["game"]["last_result"].is_object());
    assert!(
        !value["campaign"]["run_history"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // Since W2-06 every deploy freezes its drop tier, so the two live
    // attempts write one. The runs dropped nothing, so every other key stays
    // at its default and out of the save.
    assert_eq!(
        loot_keys(&value),
        [
            "attempt.drop_tier".to_string(),
            format!("battles[{live}].game.attempt.drop_tier"),
        ]
    );
    let attempt = value["attempt"].as_object_mut().unwrap();
    attempt.remove("drop_tier");
    let attempt = value["battles"][live]["game"]["attempt"]
        .as_object_mut()
        .unwrap();
    attempt.remove("drop_tier");
    assert_eq!(loot_keys(&value), Vec::<String>::new());
    // Defaults read back from a save with none of the keys.
    let loaded = load(value).unwrap();
    assert!(loaded.campaign().auto_sell.is_default());
    assert!(loaded.attempt().unwrap().drop_ids.is_empty());
    assert_eq!(loaded.attempt().unwrap().drop_tier, None);
    assert!(loaded.last_result().unwrap().auto_sold.is_empty());
}

#[test]
fn worker_campaign_cannot_carry_an_enabled_rule() {
    // A worker copy never inherits the parent's rule.
    let mut g = strategic_game();
    let country = g.campaign().world.cleared[0].clone();
    let id = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    g.campaign_mut().auto_sell.enabled = true;
    let battle = g.deploy_survivor(id, &country, 0).unwrap();
    let worker = g.battles().iter().find(|b| b.id == battle).unwrap();
    assert!(worker.game().campaign().auto_sell.is_default());
    assert!(g.campaign().auto_sell.enabled);
    let enabled = json!({"enabled": true});
    let disabled_but_set = json!({"sell_below": "Rare"});
    assert_each_invalid(vec![
        (
            "live worker enabled",
            edited(|s| {
                s.value["battles"][s.live]["game"]["campaign"]["auto_sell"] = enabled.clone()
            }),
        ),
        (
            "settled worker enabled",
            edited(|s| {
                s.value["battles"][s.settled]["game"]["campaign"]["auto_sell"] = enabled.clone()
            }),
        ),
        (
            "worker non-default",
            edited(|s| {
                s.value["battles"][s.live]["game"]["campaign"]["auto_sell"] =
                    disabled_but_set.clone()
            }),
        ),
        (
            "root sells below Poor",
            edited(|s| s.value["campaign"]["auto_sell"]["sell_below"] = json!("Poor")),
        ),
    ]);
}

#[test]
fn run_record_rejects_auto_sold_count_above_loot() {
    let LootGame { game, .. } = loot_game();
    let mut record = game.campaign().run_history.last().unwrap().clone();
    record.loot = 3;
    record.loot_by_quality = Default::default();
    for (count, coins, valid) in [
        (0, 0, true),
        (3, 40, true),
        (3, 0, true),
        (4, 40, false),
        (u32::MAX, 40, false),
        (0, 1, false),
    ] {
        record.auto_sold_count = count;
        record.auto_sold_coins = coins;
        assert_eq!(
            record.validate().is_ok(),
            valid,
            "count {count} coins {coins}"
        );
    }
    assert_each_invalid(vec![(
        "saved record",
        edited(|s| {
            let record = s.value["campaign"]["run_history"]
                .as_array_mut()
                .unwrap()
                .last_mut()
                .unwrap();
            record["auto_sold_count"] = json!(4);
        }),
    )]);
}

#[test]
fn drop_tracking_bounds_reject_out_of_range_saves() {
    let s = staged();
    let next = s.value["campaign"]["next_item_id"].as_u64().unwrap();
    let [a, b, _] = s.ids;
    let attempt = |field: &'static str, v: Value| edited(move |s| s.value["attempt"][field] = v);
    let report = |field: &'static str, v: Value| edited(move |s| s.value["last_result"][field] = v);
    let receipt = |field: &'static str, v: Value| {
        edited(move |s| s.value["last_result"]["auto_sold"][field] = v)
    };
    assert_each_invalid(vec![
        (
            "attempt drops descending",
            attempt("drop_ids", json!([b, a])),
        ),
        ("attempt drop duplicate", attempt("drop_ids", json!([a, a]))),
        ("attempt drops above loot", attempt("loot", json!(1))),
        (
            "attempt drop at next id",
            attempt("drop_ids", json!([a, next])),
        ),
        ("attempt drop id zero", attempt("drop_ids", json!([0, a]))),
        ("drop tier zero", attempt("drop_tier", json!(0))),
        ("drop tier above the pool", attempt("drop_tier", json!(5))),
        (
            "worker drop tier above the pool",
            edited(|s| s.value["battles"][s.live]["game"]["attempt"]["drop_tier"] = json!(9)),
        ),
        ("report drop duplicate", report("drop_ids", json!([a, a]))),
        ("report drop at next id", report("drop_ids", json!([next]))),
        (
            "report quality counts above loot",
            report("loot_by_quality", counts(&[("Rare", 4)])),
        ),
        (
            "receipt and kept above loot",
            receipt("kept_on_overflow", json!(3)),
        ),
        (
            "receipt quality sum",
            receipt("by_quality", counts(&[("Poor", 2)])),
        ),
        ("receipt line coins", receipt("coins", json!(11))),
        (
            "receipt unknown definition",
            edited(|s| {
                s.value["last_result"]["auto_sold"]["lines"][0]["definition"] =
                    json!("no_such_item")
            }),
        ),
    ]);
    // Rows that only one drop-tracking bound rejects, checked by message so
    // a row cannot pass on another rule.
    let worker_drops = |s: &mut Staged| {
        let worker = &mut s.value["battles"][s.live]["game"];
        worker["attempt"]["loot"] = json!(1);
        worker["attempt"]["drop_ids"] = json!([1]);
    };
    let only_drop_bound = vec![
        (
            // Quality counts and the receipt fit loot 1; the two ids do not.
            "report drops above loot",
            "Invalid saved report drop tracking",
            edited(|s| {
                let report = s.value["last_result"].as_object_mut().unwrap();
                report.insert("loot".into(), json!(1));
                report.insert("loot_by_quality".into(), counts(&[("Common", 1)]));
                report.remove("auto_sold");
            }),
        ),
        (
            "report drop id zero",
            "Invalid saved report drop tracking",
            report("drop_ids", json!([0, a])),
        ),
        (
            // Only the drop_tier Some path records drops (ruling E11).
            "legacy attempt with drops",
            "Invalid saved attempt drop tracking",
            edited(|s| {
                s.value["attempt"]
                    .as_object_mut()
                    .unwrap()
                    .remove("drop_tier");
            }),
        ),
        (
            // Drops are tracked at depth 0 only (ruling E5).
            "worker attempt with drops",
            "Invalid saved attempt drop tracking",
            edited(worker_drops),
        ),
        (
            // `finish` copies the counts into the report, whose bound
            // rejects a sum above loot: the live attempt must reject it too.
            "attempt quality counts above loot",
            "Invalid saved attempt drop tracking",
            attempt("loot_quality_counts", counts(&[("Common", 2), ("Rare", 1)])),
        ),
        (
            "worker attempt quality counts above loot",
            "Invalid saved attempt drop tracking",
            edited(|s| {
                let worker = &mut s.value["battles"][s.live]["game"]["attempt"];
                worker["loot"] = json!(1);
                worker["loot_quality_counts"] = counts(&[("Poor", 2)]);
            }),
        ),
    ];
    let wrong: Vec<String> = only_drop_bound
        .into_iter()
        .filter_map(|(name, expected, value)| match load(value) {
            Err(SaveError::Invalid(error)) if error == expected => None,
            Ok(_) => Some(format!("{name}: loaded")),
            Err(error) => Some(format!("{name}: {error}")),
        })
        .collect();
    assert_eq!(
        wrong,
        Vec::<String>::new(),
        "rows not rejected by their bound"
    );
    // The worker rows are valid without their drop ids.
    let mut worker = staged();
    worker_drops(&mut worker);
    worker.value["battles"][worker.live]["game"]["attempt"]["drop_ids"] = json!([]);
    if let Err(error) = load(worker.value) {
        panic!("worker loot 1 without drops: {error}");
    }
    // Quality counts that sum to exactly the attempt's loot load.
    let mut full = staged();
    full.value["attempt"]["loot_quality_counts"] = counts(&[("Common", 1), ("Rare", 1)]);
    if let Err(error) = load(full.value) {
        panic!("attempt quality counts equal to loot: {error}");
    }
    // A drop list of exactly MAX_RUN_DROPS ids is not rejected for its length.
    let mut many = staged();
    let ids: Vec<u64> = (1..=MAX_RUN_DROPS as u64).collect();
    many.value["campaign"]["next_item_id"] = json!(MAX_RUN_DROPS as u64 + 1);
    many.value["attempt"]["loot"] = json!(MAX_RUN_DROPS + 1);
    many.value["attempt"]["drop_ids"] = json!(ids);
    many.value["battles"][many.live]["pending_auto_sell"] = json!([]);
    if let Err(error) = load(many.value.clone()) {
        panic!("{MAX_RUN_DROPS} drops: {error}");
    }
    let mut too_many = many;
    let ids: Vec<u64> = (1..=MAX_RUN_DROPS as u64 + 1).collect();
    too_many.value["campaign"]["next_item_id"] = json!(MAX_RUN_DROPS as u64 + 2);
    too_many.value["attempt"]["drop_ids"] = json!(ids);
    too_many.value["attempt"]["loot"] = json!(MAX_RUN_DROPS + 1);
    match load(too_many.value) {
        Err(SaveError::Invalid(error)) => assert!(error.contains("drop"), "{error}"),
        other => panic!("{:?}", other.err()),
    }
}

#[test]
fn validate_rejects_settled_battle_with_non_empty_pending_auto_sell() {
    let Staged { value, ids, .. } = staged();
    load(value).unwrap();
    assert_each_invalid(vec![(
        "settled pending sale",
        edited(|s| s.value["battles"][s.settled]["pending_auto_sell"] = json!([ids[2]])),
    )]);
}

#[test]
fn validate_rejects_pending_auto_sell_overlapping_reserved_items_or_root_drop_ids() {
    let s = staged();
    let reserved = s.value["campaign"]["reserved_items"][0].as_u64().unwrap();
    let next = s.value["campaign"]["next_item_id"].as_u64().unwrap();
    let [drop, _, pending] = s.ids;
    let rows = [
        ("reserved item", json!([pending, reserved])),
        ("root drop id", json!([pending, drop])),
        ("duplicate", json!([pending, pending])),
        ("at the next item id", json!([next])),
        ("id zero", json!([0])),
    ];
    assert_each_invalid(
        rows.into_iter()
            .map(|(name, ids)| {
                (
                    name,
                    edited(move |s| s.value["battles"][s.live]["pending_auto_sell"] = ids),
                )
            })
            .collect(),
    );
}
