mod support;
use horde_core::{
    campaign::{Campaign, Player},
    content::{EnemyKind, Mode, StructureKind, content},
    ordnance::{OrdnanceKind, ThrowRequest},
    progression::catalog,
    sim::{Game, Input, MAX_LANES, Outcome},
};
use support::legacy_game;

fn arena(level: u32) -> Game {
    let mut g = legacy_game();
    g.campaign_mut().player.level = level;
    g.campaign_mut().player.points = level - 1;
    g.campaign_mut().player.xp = 0;
    g
}
fn total_xp(player: &Player) -> u64 {
    let mut p = player.clone();
    let mut total = player.xp;
    for level in 1..player.level {
        p.level = level;
        total += p.next_level_xp();
    }
    total
}
fn win(g: &mut Game, stage: usize, mode: Mode) {
    g.deploy_country_stage(stage, mode).unwrap();
    let mut a = g.attempt_slot_mut().take().unwrap();
    let center = a.lane_count / 2;
    g.spawn_enemy(&mut a, EnemyKind::Brute, center, true);
    a.boss_spawned = true;
    if mode == Mode::Npc {
        a.enemies[0].injury.unconscious = true;
    } else {
        a.enemies[0].hp = 0;
    }
    *g.attempt_slot_mut() = Some(a);
    g.tick(Input::default());
    assert_eq!(g.last_result().unwrap().outcome, Outcome::Victory);
}

#[test]
fn capped_level_and_lanes_are_frozen_until_next_deployment_and_legacy_overcap_is_preserved() {
    for (level, lanes) in [
        (1, 11),
        (10, 11),
        (11, 13),
        (20, 13),
        (21, 15),
        (30, 15),
        (31, 17),
        (50, 17),
        (90, 17),
    ] {
        assert_eq!(arena(level).campaign().lane_count(), lanes);
    }
    let mut g = arena(10);
    assert_eq!(g.campaign().next_lane_unlock_level(), Some(11));
    g.deploy(0, Mode::Personal).unwrap();
    assert_eq!(g.attempt().unwrap().lane_count, 11);
    g.campaign_mut().reward(0, u64::MAX, Mode::Personal);
    assert_eq!(g.campaign().player.level, 50);
    assert_eq!(g.campaign().player.xp, 0);
    assert_eq!(g.campaign().player.points, 49);
    assert!(g.campaign().player.is_max_level());
    assert_eq!(g.campaign().next_lane_unlock_level(), None);
    assert_eq!(g.attempt().unwrap().defender_position(), [0.5, 5.5]);
    g.withdraw();
    g.deploy(0, Mode::Personal).unwrap();
    assert_eq!(g.attempt().unwrap().lane_count, 17);
    assert_eq!(g.attempt().unwrap().defender_position(), [0.5, 8.5]);
    let mut legacy = arena(90).campaign().clone();
    legacy.player.xp = 42;
    legacy.migrate_threat_history();
    assert_eq!(legacy.player.level, 90);
    assert_eq!(legacy.player.points, 89);
    assert_eq!(legacy.player.xp, 0);
    legacy.validate().unwrap();
}

#[test]
fn seventeen_lane_targeting_defender_origin_contact_telemetry_and_throws_share_real_geometry() {
    for lane in 0..MAX_LANES {
        let mut g = arena(50);
        g.deploy(0, Mode::Personal).unwrap();
        let mut a = g.attempt_slot_mut().take().unwrap();
        g.spawn_enemy(&mut a, EnemyKind::Shambler, lane, false);
        a.enemies[0].x = 15.0;
        let hp = a.enemies[0].hp;
        let aim = [15.0, a.enemies[0].y()];
        *g.attempt_slot_mut() = Some(a);
        g.tick(Input {
            aim,
            fire: true,
            ..Input::default()
        });
        let a = g.attempt().unwrap();
        assert!(a.enemies[0].hp < hp);
        assert_eq!(
            a.events
                .iter()
                .find(|e| e.kind == horde_core::sim::EventKind::Shot)
                .unwrap()
                .from,
            a.defender_position()
        );
        assert_eq!(a.telemetry.damage_by_lane.len(), 17);
    }
    let mut g = arena(50);
    g.campaign_mut().reward(55, 0, Mode::Personal);
    g.campaign_mut().purchase_node("grenade_training").unwrap();
    g.campaign_mut().purchase_node("molotov_training").unwrap();
    g.deploy(0, Mode::Personal).unwrap();
    g.tick(Input {
        throw: Some(ThrowRequest {
            kind: OrdnanceKind::Grenade,
            cell: [15, 16],
        }),
        ..Input::default()
    });
    assert_eq!(g.attempt().unwrap().ordnance.projectiles[0].cell, [15, 16]);
    let before = g.attempt().unwrap().ordnance.charges.clone();
    g.tick(Input {
        throw: Some(ThrowRequest {
            kind: OrdnanceKind::Molotov,
            cell: [15, 17],
        }),
        ..Input::default()
    });
    assert_eq!(g.attempt().unwrap().ordnance.charges, before);
    let mut contact = arena(50);
    contact.deploy(0, Mode::Personal).unwrap();
    let mut a = contact.attempt_slot_mut().take().unwrap();
    contact.spawn_enemy(&mut a, EnemyKind::Shambler, 16, false);
    a.enemies[0].x = 1.5;
    *contact.attempt_slot_mut() = Some(a);
    for _ in 0..120 {
        contact.tick(Input::default());
    }
    assert!(contact.attempt().unwrap().telemetry.damage_by_lane[16] > 0);
}

#[test]
fn max_lane_builds_and_caller_warnings_are_legal_and_checkpoints_freeze_at_1_2_4x() {
    let mut g = arena(50);
    g.campaign_mut().first_clear(0, Mode::Personal);
    g.campaign_mut().first_clear(1, Mode::Personal);
    g.campaign_mut().reward(1000, 0, Mode::Personal);
    g.campaign_mut()
        .purchase_blueprint(StructureKind::Wall)
        .unwrap();
    g.deploy(1, Mode::Personal).unwrap();
    g.place(StructureKind::Wall, 16, 4).unwrap();
    let supply = g.attempt().unwrap().supply;
    assert!(g.place(StructureKind::Wall, 17, 0).is_err());
    assert_eq!(g.attempt().unwrap().supply, supply);
    let a = g.attempt_mut().unwrap();
    a.tick = (a.boss_time() * 60.0).ceil() as u64;
    g.tick(Input::default());
    let a = g.attempt().unwrap();
    let boss = a.enemies.iter().find(|e| e.boss).unwrap();
    assert_eq!(boss.lane, 8);
    let warning = (content().levels[1].boss_timing.initial_call
        - content().levels[1].boss_timing.call_warning)
        * 60.0;
    let tick = boss.born_tick + warning.ceil() as u64;
    g.attempt_mut().unwrap().tick = tick;
    g.tick(Input::default());
    assert!(
        g.attempt()
            .unwrap()
            .events
            .iter()
            .filter(|e| e.kind == horde_core::sim::EventKind::Reinforcements)
            .all(|e| e.lane.unwrap() < 17)
    );
    g.set_paused(true);
    let path = std::env::temp_dir().join(format!("lastlight-arena17-{}.json", std::process::id()));
    g.save(&path).unwrap();
    let frozen = serde_json::to_string(&g).unwrap();
    for _ in 0..4 {
        g.tick(Input::default());
    }
    assert_eq!(serde_json::to_string(&g).unwrap(), frozen);
    let mut reference = None;
    for batch in [1, 2, 4] {
        let mut restored = Game::load(&path).unwrap();
        restored.set_paused(false);
        for _ in 0..120 / batch {
            for _ in 0..batch {
                restored.tick(Input::default());
            }
        }
        let text = serde_json::to_string(&restored).unwrap();
        if let Some(expected) = &reference {
            assert_eq!(&text, expected);
        } else {
            reference = Some(text);
        }
    }
    let mut invalid = g;
    invalid.attempt_mut().unwrap().lane_count = 18;
    assert!(invalid.save(&path).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn every_replay_and_outbreak_victory_increases_future_threat_but_losses_do_not() {
    let mut g = legacy_game();
    g.campaign_mut().attempts = 1;
    g.campaign_mut().choose_start_country("VAT").unwrap();
    for stage in 0..3 {
        let before = g.campaign().victories;
        let xp = total_xp(&g.campaign().player);
        win(&mut g, stage, Mode::Npc);
        assert_eq!(g.campaign().victories, before + 1);
        assert_eq!(
            total_xp(&g.campaign().player) - xp,
            catalog().victory_player_xp
        );
        assert_eq!(
            g.last_result().unwrap().completion_xp,
            catalog().victory_player_xp
        );
    }
    let completed = g.campaign().world.completed("VAT");
    let rank = g.campaign().progression_rank();
    win(&mut g, 0, Mode::Npc);
    assert_eq!(g.campaign().world.completed("VAT"), completed);
    assert_eq!(g.campaign().progression_rank(), rank + 1);
    g.campaign_mut().world.reopen("VAT").unwrap();
    let before = g.campaign().progression_rank();
    win(&mut g, 2, Mode::Npc);
    assert_eq!(g.campaign().progression_rank(), before + 1);
    g.deploy_country_stage(0, Mode::Npc).unwrap();
    let snapshot = g.attempt().unwrap().progression.clone();
    g.withdraw();
    assert_eq!(g.last_result().unwrap().completion_xp, 0);
    g.deploy_country_stage(0, Mode::Npc).unwrap();
    assert_eq!(g.attempt().unwrap().progression, snapshot);
}

#[test]
fn capped_player_still_faces_strictly_increasing_health_and_damage_after_growth_rank_fifty() {
    assert_eq!(catalog().snapshot(52).item_level, 27);
    assert_eq!(catalog().snapshot(58).item_level, catalog().max_item_level);
    let mut g = arena(50);
    g.campaign_mut().attempts = 1;
    g.campaign_mut().victories = 50;
    g.campaign_mut().choose_start_country("USA").unwrap();
    g.deploy_country_stage(0, Mode::Npc).unwrap();
    let before = g.attempt().unwrap().progression.clone();
    g.withdraw();
    win(&mut g, 0, Mode::Npc);
    assert_eq!(g.campaign().player.level, 50);
    assert_eq!(g.campaign().player.xp, 0);
    assert_eq!(g.last_result().unwrap().completion_xp, 0);
    g.deploy_country_stage(0, Mode::Npc).unwrap();
    let next = &g.attempt().unwrap().progression;
    assert!(next.enemy_health > before.enemy_health && next.enemy_damage > before.enemy_damage);
    assert_eq!(next.batch_size, before.batch_size);
    assert_eq!(next.enemy_cap, before.enemy_cap);
    assert_eq!(next.item_level, before.item_level);
}

#[test]
fn future_preview_matches_actual_npc_completion_and_numerical_guards_reject_unsafe_metadata() {
    let mut g = arena(10);
    g.campaign_mut().attempts = 1;
    g.campaign_mut().player.xp = g.campaign_mut().player.next_level_xp() - 1;
    g.campaign_mut().choose_start_country("USA").unwrap();
    let predicted = g.campaign().next_victory_progression(Mode::Npc).unwrap();
    let level = g.campaign().level_after_victory();
    assert_eq!(g.campaign().lane_count_after_victory(), 13);
    win(&mut g, 0, Mode::Npc);
    assert_eq!(g.campaign().player.level, level);
    g.deploy_country_stage(0, Mode::Npc).unwrap();
    assert_eq!(g.attempt().unwrap().progression, predicted);
    assert_eq!(g.attempt().unwrap().lane_count, 13);
    g.withdraw();
    g.campaign_mut().victories = u64::from(catalog().threat_guard_rank) + 1;
    let before = g.clone();
    assert!(g.deploy_country_stage(0, Mode::Npc).is_err());
    assert_eq!(
        serde_json::to_string(&g).unwrap(),
        serde_json::to_string(&before).unwrap()
    );
    let mut bad = catalog().clone();
    bad.max_lanes = 18;
    assert!(bad.validate().is_err());
    let mut bad = catalog().clone();
    bad.population_global_cap = 225;
    assert!(bad.validate().is_err());
}

#[test]
fn parallel_worker_victories_award_completion_xp_and_difficulty_once_per_settlement() {
    let mut g = legacy_game();
    g.campaign_mut().attempts = 1;
    g.campaign_mut().coins = 10_000;
    g.campaign_mut().choose_start_country("USA").unwrap();
    let countries: Vec<_> = horde_core::world::world()
        .countries
        .iter()
        .take(2)
        .map(|c| (c.id.clone(), c.level_count()))
        .collect();
    for (id, count) in &countries {
        g.campaign_mut()
            .world
            .country_levels
            .insert(id.clone(), *count);
        g.campaign_mut().world.cleared.push(id.clone());
    }
    let country = &countries[0].0;
    for _ in 0..2 {
        let recruit = g
            .with_strategy(|strategy, campaign| strategy.recruit(campaign, country))
            .unwrap();
        g.deploy_survivor(recruit, country, 0).unwrap();
    }
    let victories = g.campaign().victories;
    let xp = total_xp(&g.campaign().player);
    for battle in g.battles_mut() {
        let mut a = battle.game_mut().attempt_slot_mut().take().unwrap();
        let lane = a.lane_count / 2;
        battle
            .game_mut()
            .spawn_enemy(&mut a, EnemyKind::Brute, lane, true);
        a.boss_spawned = true;
        a.enemies[0].injury.unconscious = true;
        *battle.game_mut().attempt_slot_mut() = Some(a);
    }
    g.tick(Input::default());
    assert_eq!(g.campaign().victories, victories + 2);
    assert_eq!(g.campaign().run_history.len(), 2);
    assert!(
        g.campaign()
            .run_history
            .iter()
            .all(|record| record.worker_id.is_some()
                && record.outcome == Outcome::Victory
                && record.kill_xp == 0
                && record.completion_xp == catalog().victory_player_xp)
    );
    assert_eq!(
        total_xp(&g.campaign().player) - xp,
        2 * catalog().victory_player_xp
    );
    let after = total_xp(&g.campaign().player);
    for _ in 0..4 {
        g.tick(Input::default());
    }
    assert_eq!(g.campaign().victories, victories + 2);
    assert_eq!(total_xp(&g.campaign().player), after);
}

#[test]
fn legacy_eleven_lane_checkpoint_remains_eleven_after_player_levels_up_and_old_history_migrates_once()
 {
    let mut g = arena(10);
    g.deploy(0, Mode::Personal).unwrap();
    g.campaign_mut().reward(0, u64::MAX, Mode::Personal);
    let mut value = serde_json::to_value(&g).unwrap();
    value["attempt"]
        .as_object_mut()
        .unwrap()
        .remove("lane_count");
    value["campaign"]
        .as_object_mut()
        .unwrap()
        .remove("threat_revision");
    let payload = serde_json::to_string(&value).unwrap();
    let checksum = support::fnv1a(payload.as_bytes());
    let restored = Game::decode_save_generation(
        &serde_json::to_vec(
            &serde_json::json!({"version":1,"checksum":checksum,"payload":payload}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(restored.attempt().unwrap().lane_count, 11);
    assert_eq!(restored.campaign().player.level, 50);
    let mut old = Campaign::default();
    old.choose_start_country("USA").unwrap();
    for stage in 0..6 {
        old.world.record_clear(stage).unwrap();
    }
    old.threat_revision = 0;
    let xp = total_xp(&old.player);
    old.migrate_threat_history();
    assert_eq!(old.progression_rank(), 6);
    assert_eq!(total_xp(&old.player), xp);
    old.first_clear(0, Mode::Npc);
    assert_eq!(old.progression_rank(), 7);
}
