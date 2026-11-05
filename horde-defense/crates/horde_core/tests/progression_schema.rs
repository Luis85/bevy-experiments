//! Progression receipts (W1-25): `Report.progression_rewards`, the
//! `RunRecord.level_grant_coins` / `milestone_coins` totals and the
//! transient `SettlementIssue::MilestoneItemLost`, beside the W1-24 ledgers
//! `Player.level_grant_paid` and `Campaign.milestones_claimed`. Nothing
//! writes a non-default receipt yet (W2), so these tests stage the values as
//! a save would.
mod support;
use horde_core::{
    campaign::Player,
    command::Command,
    content::{ItemDefId, Mode},
    sim::{Game, MilestoneReward, ProgressionRewards, SAVE_VERSION, SaveError, SettlementIssue},
};
use serde_json::{Value, json};
use support::strategic_game;

/// A strategic game with a settled expedition (its own report and run
/// record), a withdrawn root run (report and run record) and a live root
/// Personal attempt. Returns the game and the settled battle's position.
fn receipt_game() -> (Game, usize) {
    let mut g = strategic_game();
    let country = g.campaign().world.cleared[0].clone();
    let recruit = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    let settled = g.deploy_survivor(recruit, &country, 0).unwrap();
    g.recall_survivor(settled).unwrap();
    g.deploy(0, Mode::Personal).unwrap();
    g.apply(&Command::Withdraw).unwrap();
    // Deploying clears the report; restore it as a save made later would hold.
    let report = g.last_result().cloned();
    g.deploy(0, Mode::Personal).unwrap();
    *g.last_result_mut() = report;
    let settled = g.battles().iter().position(|b| b.id == settled).unwrap();
    assert!(g.battles()[settled].settled);
    assert!(g.battles()[settled].game().last_result().is_some());
    assert!(g.last_result().is_some() && g.attempt().is_some());
    (g, settled)
}

/// The staged save: a root report receipt with two milestones (one with its
/// granted item, one whose item was lost), a settled expedition report
/// receipt with level coins only, run-record coin totals, and the ledgers.
struct Staged {
    value: Value,
    settled: usize,
    item: u64,
}

fn staged() -> Staged {
    let (game, settled) = receipt_game();
    let mut value = serde_json::to_value(&game).unwrap();
    let item = game.campaign().next_item_id - 1;
    let level = game.campaign().player.level;
    value["last_result"]["progression_rewards"] = json!({
        "level_from": 1, "level_to": 6, "level_coins": 77,
        "milestones": [
            {"rank": 5, "coins": 500, "item_id": item, "item_definition": "fire_axe"},
            {"rank": 10, "coins": 1000}
        ]
    });
    value["battles"][settled]["game"]["last_result"]["progression_rewards"] =
        json!({"level_from": 2, "level_to": 3, "level_coins": 9, "milestones": []});
    let record = value["campaign"]["run_history"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap();
    record["level_grant_coins"] = json!(77);
    record["milestone_coins"] = json!(1500);
    value["campaign"]["player"]["level_grant_paid"] = json!(level);
    Staged {
        value,
        settled,
        item,
    }
}

fn load(value: Value) -> Result<Game, SaveError> {
    Game::from_payload(value, SAVE_VERSION)
}

/// `staged()` with `edit` applied.
fn edited(edit: impl FnOnce(&mut Staged)) -> Value {
    let mut s = staged();
    edit(&mut s);
    s.value
}

/// Loads every row and fails with the names of the rows that loaded or
/// failed with anything but the receipt's own `SaveError::Invalid`.
fn assert_each_invalid(rows: Vec<(&str, Value)>) {
    let loaded: Vec<String> = rows
        .into_iter()
        .filter_map(|(name, value)| match load(value) {
            Err(SaveError::Invalid(message)) if message.contains("progression receipt") => None,
            Ok(_) => Some(format!("{name}: loaded")),
            Err(error) => Some(format!("{name}: {error}")),
        })
        .collect();
    assert_eq!(loaded, Vec::<String>::new(), "rows that did not fail");
}

fn milestone(rank: u32) -> Value {
    json!({"rank": rank, "coins": 100})
}

#[test]
fn progression_rewards_round_trip_and_reject_five_milestones() {
    let Staged {
        value,
        settled,
        item,
    } = staged();
    let loaded = load(value.clone()).unwrap_or_else(|e| panic!("staged receipt save: {e}"));
    assert_eq!(serde_json::to_value(&loaded).unwrap(), value);
    assert_eq!(
        loaded.last_result().unwrap().progression_rewards,
        ProgressionRewards {
            level_from: 1,
            level_to: 6,
            level_coins: 77,
            milestones: vec![
                MilestoneReward {
                    rank: 5,
                    coins: 500,
                    item_id: Some(item),
                    item_definition: Some(ItemDefId("fire_axe".into())),
                },
                MilestoneReward {
                    rank: 10,
                    coins: 1000,
                    item_id: None,
                    item_definition: None,
                },
            ],
        }
    );
    let worker = loaded.battles()[settled].game().last_result().unwrap();
    assert_eq!(worker.progression_rewards.level_coins, 9);

    // Four milestones at strictly increasing ranks and level_to at the cap
    // still load.
    let cap = Player::level_cap();
    load(edited(|s| {
        let receipt = &mut s.value["last_result"]["progression_rewards"];
        receipt["milestones"] = json!([5, 10, 15, 20].map(milestone));
        receipt["level_from"] = json!(cap);
        receipt["level_to"] = json!(cap);
    }))
    .unwrap_or_else(|e| panic!("four milestones at the cap: {e}"));

    let root = |field: &'static str, v: Value| {
        edited(move |s| s.value["last_result"]["progression_rewards"][field] = v)
    };
    assert_each_invalid(vec![
        (
            "five milestones",
            root("milestones", json!([5, 10, 15, 20, 25].map(milestone))),
        ),
        (
            "worker five milestones",
            edited(|s| {
                s.value["battles"][s.settled]["game"]["last_result"]["progression_rewards"]["milestones"] =
                    json!([1, 2, 3, 4, 5].map(milestone))
            }),
        ),
        (
            "ranks descending",
            root("milestones", json!([10, 5].map(milestone))),
        ),
        (
            "rank repeated",
            root("milestones", json!([5, 5].map(milestone))),
        ),
        ("level_from above level_to", root("level_from", json!(7))),
        ("level_to above the cap", root("level_to", json!(cap + 1))),
        (
            "unknown item definition",
            edited(|s| {
                s.value["last_result"]["progression_rewards"]["milestones"][0]["item_definition"] =
                    json!("no_such_item")
            }),
        ),
    ]);
}

/// Every saved location of a W1-24 or W1-25 key in `value` that is present,
/// over the root game and each worker game.
fn progression_keys(value: &Value) -> Vec<String> {
    fn check(object: &Value, path: &str, keys: &[&str], found: &mut Vec<String>) {
        for key in keys {
            if object.as_object().is_some_and(|o| o.contains_key(*key)) {
                found.push(format!("{path}.{key}"));
            }
        }
    }
    fn game(value: &Value, at: &str, found: &mut Vec<String>) {
        let campaign = &value["campaign"];
        check(
            campaign,
            &format!("{at}campaign"),
            &["milestones_claimed"],
            found,
        );
        check(
            &campaign["player"],
            &format!("{at}campaign.player"),
            &["level_grant_paid"],
            found,
        );
        let history = campaign["run_history"].as_array().into_iter().flatten();
        let pending = value["pending_runs"].as_array().into_iter().flatten();
        for (n, record) in history.chain(pending).enumerate() {
            let keys = ["level_grant_coins", "milestone_coins"];
            check(record, &format!("{at}record[{n}]"), &keys, found);
        }
        check(
            &value["last_result"],
            &format!("{at}last_result"),
            &["progression_rewards"],
            found,
        );
        let battles = value["battles"].as_array().into_iter().flatten();
        for (n, battle) in battles.enumerate() {
            game(&battle["game"], &format!("{at}battles[{n}].game."), found);
        }
    }
    let mut found = Vec::new();
    game(value, "", &mut found);
    found
}

#[test]
fn progression_ledgers_round_trip_and_skip_at_default() {
    assert!(ProgressionRewards::default().is_empty());
    let Staged { value, settled, .. } = staged();
    let loaded = load(value.clone()).unwrap_or_else(|e| panic!("staged receipt save: {e}"));
    assert_eq!(serde_json::to_value(&loaded).unwrap(), value);
    let record = loaded.campaign().run_history.last().unwrap();
    assert_eq!(
        (record.level_grant_coins, record.milestone_coins),
        (77, 1500)
    );
    assert_eq!(
        loaded.campaign().player.level_grant_paid,
        loaded.campaign().player.level
    );
    assert!(record.validate().is_ok());
    // The totals carry no bound of their own.
    let mut large = record.clone();
    (large.level_grant_coins, large.milestone_coins) = (u64::MAX, u64::MAX);
    assert!(large.validate().is_ok());

    // Emptied again, every key leaves the save.
    let mut game = loaded;
    game.campaign_mut().player.level_grant_paid = 0;
    let record = game.campaign_mut().run_history.last_mut().unwrap();
    (record.level_grant_coins, record.milestone_coins) = (0, 0);
    game.last_result_mut().as_mut().unwrap().progression_rewards = ProgressionRewards::default();
    game.battles_mut()[settled]
        .game_mut()
        .last_result_mut()
        .as_mut()
        .unwrap()
        .progression_rewards = ProgressionRewards::default();
    let emptied = serde_json::to_value(&game).unwrap();
    assert_eq!(progression_keys(&emptied), Vec::<String>::new());

    // A fresh game's save holds none of the keys, and defaults read back.
    let (fresh, settled) = receipt_game();
    let value = serde_json::to_value(&fresh).unwrap();
    assert_eq!(progression_keys(&value), Vec::<String>::new());
    let loaded = load(value).unwrap();
    assert!(loaded.last_result().unwrap().progression_rewards.is_empty());
    let worker = loaded.battles()[settled].game().last_result().unwrap();
    assert!(worker.progression_rewards.is_empty());
    let record = loaded.campaign().run_history.last().unwrap();
    assert_eq!((record.level_grant_coins, record.milestone_coins), (0, 0));
}

#[test]
fn milestone_item_lost_names_rank_and_item() {
    let issue = SettlementIssue::MilestoneItemLost {
        rank: 10,
        definition: ItemDefId("fire_axe".into()),
        error: "Inventory is full".into(),
    };
    assert_eq!(
        issue.to_string(),
        "Rank 10 milestone item fire_axe was not stored: Inventory is full"
    );
}
