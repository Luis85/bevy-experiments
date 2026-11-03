mod support;
use horde_core::campaign::Placement;
use horde_core::content::{EnemyKind, Mode, StructureKind, content};
use horde_core::sim::{Game, Input};
use support::legacy_game;

fn prepared_game() -> Game {
    let mut game = legacy_game();
    for level in 0..3 {
        game.campaign.first_clear(level, Mode::Personal);
    }
    game.campaign.reward(1000, 0, Mode::Personal);
    for kind in StructureKind::ALL {
        game.campaign.purchase_blueprint(kind).unwrap();
    }
    game
}

fn decode_value(value: serde_json::Value) -> Result<Game, String> {
    let payload = serde_json::to_string(&value).unwrap();
    let checksum = payload
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    Game::decode_save_generation(
        &serde_json::to_vec(&serde_json::json!({
            "version": 1, "checksum": checksum, "payload": payload,
        }))
        .unwrap(),
    )
}

#[test]
fn deployment_and_retry_start_immediately_and_live_building_preserves_actions() {
    let mut game = prepared_game();
    game.set_paused(true);
    game.deploy(0, Mode::Personal).unwrap();
    assert!(!game.global_paused);
    assert!(!game.attempt.as_ref().unwrap().paused);
    game.tick(Input::default());
    assert_eq!(game.attempt.as_ref().unwrap().tick, 1);
    let a = game.attempt.as_mut().unwrap();
    a.reload_left = 73;
    a.shot_cooldown = 19;
    a.melee_commit = 9;
    let supply = a.supply;
    game.preview_placement(StructureKind::Wall, 0, 0).unwrap();
    game.place(StructureKind::Wall, 0, 0).unwrap();
    assert!(game.place(StructureKind::Wall, 0, 0).is_err());
    assert!(game.place(StructureKind::Wall, 11, 0).is_err());
    let a = game.attempt.as_ref().unwrap();
    assert_eq!(
        (a.reload_left, a.shot_cooldown, a.melee_commit),
        (73, 19, 9)
    );
    let cost = content().structure(StructureKind::Wall).supply;
    assert_eq!(a.supply, supply - cost);
    assert!(!a.paused);
    let id = a.structures[0].id;
    assert_eq!(game.salvage(id).unwrap(), cost / 2);
    assert_eq!(
        game.attempt.as_ref().unwrap().supply,
        supply - cost + cost / 2
    );
    game.withdraw();
    game.set_paused(true);
    game.retry().unwrap();
    assert!(!game.global_paused);
    assert!(!game.attempt.as_ref().unwrap().paused);
}

#[test]
fn live_tick_zero_salvage_and_enemy_occupied_builds_cannot_generate_supply() {
    let mut game = prepared_game();
    game.deploy(0, Mode::Personal).unwrap();
    let starting = game.attempt.as_ref().unwrap().supply;
    game.place(StructureKind::Wall, 0, 0).unwrap();
    let a = game.attempt.as_ref().unwrap();
    assert_eq!(a.tick, 0);
    let id = a.structures[0].id;
    let cost = a.structures[0].cost;
    assert_eq!(game.salvage(id).unwrap(), cost / 2);
    assert_eq!(
        game.attempt.as_ref().unwrap().supply,
        starting - cost + cost / 2
    );
    let mut a = game.attempt.take().unwrap();
    game.spawn_enemy(&mut a, EnemyKind::Shambler, 0, false);
    a.enemies[0].x = 4.2;
    game.attempt = Some(a);
    let supply = game.attempt.as_ref().unwrap().supply;
    assert!(game.place(StructureKind::Wall, 0, 0).is_err());
    assert_eq!(game.attempt.as_ref().unwrap().supply, supply);
}

#[test]
fn country_layouts_are_isolated_preserved_and_apply_only_an_affordable_prefix() {
    let mut game = prepared_game();
    game.campaign.choose_start_country("DEU").unwrap();
    let plan: Vec<_> = (0..11)
        .map(|lane| Placement {
            kind: StructureKind::Turret,
            lane,
            column: 0,
        })
        .collect();
    game.campaign
        .save_country_layout("DEU", plan.clone())
        .unwrap();
    game.campaign
        .save_country_layout(
            "FRA",
            vec![Placement {
                kind: StructureKind::Wall,
                lane: 10,
                column: 4,
            }],
        )
        .unwrap();
    game.deploy_country_stage(0, Mode::Personal).unwrap();
    let a = game.attempt.as_ref().unwrap();
    let budget = (content().levels[0].supply as i32
        + a.modifiers.supply
        + a.progression.supply_bonus as i32)
        .max(0) as u32;
    let cost = content().structure(StructureKind::Turret).supply;
    let placed = (budget / cost) as usize;
    assert_eq!(a.structures.len(), placed);
    assert_eq!(a.layout_omissions, plan.len() - placed);
    assert_eq!(a.supply, budget - placed as u32 * cost);
    assert_eq!(game.campaign.layout_for("DEU"), plan);
    let resumed = decode_value(serde_json::to_value(&game).unwrap()).unwrap();
    assert_eq!(
        resumed.campaign.country_layouts,
        game.campaign.country_layouts
    );
    let supply = game.attempt.as_ref().unwrap().supply;
    game.save_country_layout().unwrap();
    assert_eq!(game.campaign.layout_for("DEU").len(), placed);
    assert_eq!(game.campaign.layout_for("FRA").len(), 1);
    assert_eq!(game.attempt.as_ref().unwrap().supply, supply);
    game.clear_country_layout().unwrap();
    assert!(game.campaign.layout_for("DEU").is_empty());
    assert_eq!(game.campaign.layout_for("FRA").len(), 1);
    game.withdraw();
    game.campaign.world.unlocked.push("FRA".into());
    game.campaign.enter_country("FRA").unwrap();
    game.deploy_country_stage(0, Mode::Personal).unwrap();
    let a = game.attempt.as_ref().unwrap();
    assert_eq!(a.structures.len(), 1);
    assert_eq!((a.structures[0].lane, a.structures[0].column), (10, 4));
}

#[test]
fn every_corpse_survives_effect_expiry_and_save_resume_without_an_arbitrary_cap() {
    let mut game = legacy_game();
    game.deploy(0, Mode::Personal).unwrap();
    for _ in 0..20 {
        let mut a = game.attempt.take().unwrap();
        for lane in 0..30 {
            game.spawn_enemy(&mut a, EnemyKind::Shambler, lane % 11, false);
            a.enemies.last_mut().unwrap().hp = 0;
        }
        game.attempt = Some(a);
        game.tick(Input::default());
    }
    let corpses = game.attempt.as_ref().unwrap().corpses.clone();
    assert_eq!(corpses.len(), 600);
    assert!(corpses.iter().any(|corpse| corpse.at[0] > 30.0));
    for _ in 0..180 {
        game.tick(Input::default());
    }
    assert_eq!(game.attempt.as_ref().unwrap().corpses, corpses);
    let mut resumed = decode_value(serde_json::to_value(&game).unwrap()).unwrap();
    assert_eq!(resumed.attempt.as_ref().unwrap().corpses, corpses);
    let coins = resumed.campaign.coins;
    resumed.tick(Input::default());
    assert_eq!(resumed.campaign.coins, coins);
    assert_eq!(resumed.attempt.as_ref().unwrap().corpses, corpses);
    let mut invalid = serde_json::to_value(&game).unwrap();
    invalid["attempt"]["corpses"][0]["at"][0] = serde_json::json!(-1.0);
    assert!(decode_value(invalid).is_err());
    let mut invalid = serde_json::to_value(&game).unwrap();
    invalid["attempt"]["corpses"][1]["id"] = invalid["attempt"]["corpses"][0]["id"].clone();
    assert!(decode_value(invalid).is_err());
}

#[test]
fn saves_without_corpse_or_country_plan_fields_keep_the_legacy_defaults() {
    let mut game = legacy_game();
    game.deploy(0, Mode::Personal).unwrap();
    let mut a = game.attempt.take().unwrap();
    game.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
    game.attempt = Some(a);
    let mut value = serde_json::to_value(&game).unwrap();
    value["campaign"]
        .as_object_mut()
        .unwrap()
        .remove("country_layouts");
    let attempt = value["attempt"].as_object_mut().unwrap();
    attempt.remove("corpses");
    attempt.remove("layout_omissions");
    attempt.get_mut("enemies").unwrap()[0]
        .as_object_mut()
        .unwrap()
        .remove("resolved");
    let resumed = decode_value(value).unwrap();
    assert!(resumed.campaign.country_layouts.is_empty());
    let a = resumed.attempt.as_ref().unwrap();
    assert!(a.corpses.is_empty());
    assert!(a.enemies[0].resolved.is_none());
}
