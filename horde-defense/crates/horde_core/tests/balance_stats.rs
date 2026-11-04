use horde_core::{
    balance::{BalancePreset, BalanceSettings, catalog},
    content::{EnemyKind, Mode},
    sim::{Game, Input, Outcome},
    stats::{DamageBucket, DamageSource, DamageTotals, DpsTracker},
};
mod support;
use support::legacy_game;

#[test]
fn presets_validate_and_atomic_edits_freeze_active_arena_and_rules() {
    catalog().validate().unwrap();
    let mut g = Game::default();
    assert_eq!(g.campaign.arena_geometry(), (3, 12.0));
    assert!(g.campaign.balance_settings.enemy_speed >= 1.15);
    for preset in [
        BalancePreset::Easy,
        BalancePreset::Normal,
        BalancePreset::Hard,
    ] {
        BalanceSettings::for_preset(preset).validate().unwrap();
    }
    g.deploy(0, Mode::Personal).unwrap();
    let before = serde_json::to_string(g.attempt.as_ref().unwrap()).unwrap();
    let mut bad = BalanceSettings::for_preset(BalancePreset::Hard);
    bad.enemy_speed = f32::NAN;
    let campaign = g.campaign.clone();
    assert!(g.campaign.set_balance_settings(bad).is_err());
    assert_eq!(g.campaign, campaign);
    g.campaign
        .set_balance_settings(BalanceSettings::for_preset(BalancePreset::Hard))
        .unwrap();
    assert_eq!(
        serde_json::to_string(g.attempt.as_ref().unwrap()).unwrap(),
        before
    );
    g.withdraw();
    assert_eq!(
        g.campaign.run_history[0].balance.preset,
        BalancePreset::Normal
    );
    g.deploy(0, Mode::Personal).unwrap();
    assert_eq!(
        g.attempt.as_ref().unwrap().balance.preset,
        BalancePreset::Hard
    );
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.construction_bounds(), [2.0, 6.0]);
    for column in 0..5 {
        let p = a.build_cell_center(2, column).unwrap();
        assert_eq!(a.build_cell_at(p), Some([2, column]));
        assert!(p[0] < a.width_yards / 2.0);
    }
}

#[test]
fn five_simulation_second_dps_is_source_separated_and_rejects_corrupt_aggregate_windows() {
    let mut d = DpsTracker::new();
    d.record(60, DamageSource::Defender, 100);
    d.record(120, DamageSource::Structures, 50);
    d.record(180, DamageSource::Ordnance, 25);
    assert_eq!(
        d.totals,
        DamageTotals {
            defender: 100,
            structures: 50,
            ordnance: 25
        }
    );
    assert_eq!(d.live(180), 35.0);
    assert_eq!(d.peak, 35.0);
    assert!((d.average(180) - 175.0 / 3.0).abs() < 0.001);
    assert_eq!(d.live(361), 15.0);
    d.validate(400).unwrap();
    let mut bad = DpsTracker::new();
    bad.totals.defender = 100;
    bad.buckets.push_back(DamageBucket {
        tick: 1,
        damage: DamageTotals {
            defender: 60,
            ..Default::default()
        },
    });
    bad.buckets.push_back(DamageBucket {
        tick: 2,
        damage: DamageTotals {
            defender: 60,
            ..Default::default()
        },
    });
    assert!(bad.validate(2).is_err());
    bad.totals.defender = u64::MAX;
    bad.buckets[0].damage.defender = u64::MAX;
    bad.buckets[1].damage.defender = 1;
    assert!(bad.validate(2).is_err());
    let mut bad = d.clone();
    bad.buckets[0].tick = 500;
    assert!(bad.validate(400).is_err());
}

#[test]
fn authoritative_dps_and_width_roundtrip_freeze_on_pause_and_do_not_use_wall_time() {
    let mut g = legacy_game();
    g.deploy(0, Mode::Personal).unwrap();
    let mut a = g.attempt.take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
    let e = &mut a.enemies[0];
    e.x = 10.0;
    e.hp = 1000;
    e.max_hp = 1000;
    e.injury = horde_core::rules::InjuryState::new(1000, 10);
    g.attempt = Some(a);
    g.tick(Input {
        aim: [10.0, 5.5],
        fire: true,
        ..Input::default()
    });
    let a = g.attempt.as_ref().unwrap();
    assert!(a.dps.totals.defender > 0);
    assert_eq!(a.live_dps(), a.dps.totals.defender as f32 / 5.0);
    g.set_paused(true);
    let before = serde_json::to_string(&g).unwrap();
    for _ in 0..60 {
        g.tick(Input::default());
    }
    assert_eq!(serde_json::to_string(&g).unwrap(), before);
    let path = std::env::temp_dir().join(format!("lastlight-dps-{}.json", std::process::id()));
    g.save(&path).unwrap();
    let mut restored = Game::load(&path).unwrap();
    assert_eq!(restored.attempt.as_ref().unwrap().width_yards, 30.0);
    g.set_paused(false);
    restored.set_paused(false);
    for _ in 0..320 {
        g.tick(Input::default());
        restored.tick(Input::default());
    }
    assert_eq!(
        serde_json::to_string(&g).unwrap(),
        serde_json::to_string(&restored).unwrap()
    );
    assert_eq!(g.attempt.as_ref().unwrap().live_dps(), 0.0);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn resolved_defender_structure_and_ordnance_injury_feed_distinct_real_damage_sources() {
    use horde_core::{
        content::StructureKind,
        ordnance::{OrdnanceKind, ThrowRequest},
    };
    let mut g = legacy_game();
    for level in 0..3 {
        g.campaign.first_clear(level, Mode::Personal);
    }
    g.campaign.reward(1000, 0, Mode::Personal);
    g.campaign
        .purchase_blueprint(StructureKind::Turret)
        .unwrap();
    g.campaign.purchase_node("grenade_training").unwrap();
    g.deploy(0, Mode::Personal).unwrap();
    g.place(StructureKind::Turret, 5, 0).unwrap();
    let mut a = g.attempt.take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
    let e = &mut a.enemies[0];
    e.x = 8.0;
    e.hp = 1000;
    e.max_hp = 1000;
    e.injury = horde_core::rules::InjuryState::new(1000, 10);
    g.attempt = Some(a);
    g.tick(Input {
        aim: [8.0, 5.5],
        fire: true,
        ..Input::default()
    });
    g.tick(Input {
        throw: Some(ThrowRequest {
            kind: OrdnanceKind::Grenade,
            cell: [8, 5],
        }),
        ..Input::default()
    });
    for _ in 0..40 {
        g.tick(Input::default());
    }
    let a = g.attempt.as_ref().unwrap();
    let totals = a.dps.totals;
    assert!(totals.defender > 0 && totals.structures > 0 && totals.ordnance > 0);
    assert_eq!(a.active_damage, totals.defender + totals.ordnance);
    assert_eq!(a.structure_damage, totals.structures);
    g.withdraw();
    assert_eq!(
        g.campaign.run_history.last().unwrap().damage_by_source,
        totals
    );
}

#[test]
fn all_finished_outcomes_have_one_untruncated_persistent_record_and_reopening_pays_nothing() {
    let mut g = legacy_game();
    for _ in 0..300 {
        g.deploy(0, Mode::Personal).unwrap();
        g.withdraw();
    }
    assert_eq!(g.campaign.run_history.len(), 300);
    assert_eq!(g.campaign.next_run_id, 301);
    assert!(
        g.campaign
            .run_history
            .iter()
            .enumerate()
            .all(|(i, r)| r.id == i as u64 + 1 && r.outcome == Outcome::Withdrawal)
    );
    g.withdraw();
    assert_eq!(g.campaign.run_history.len(), 300);
    g.deploy(0, Mode::Personal).unwrap();
    g.attempt.as_mut().unwrap().hp = 0;
    g.tick(Input::default());
    assert_eq!(
        g.campaign.run_history.last().unwrap().outcome,
        Outcome::Defeat
    );
    g.deploy(0, Mode::Personal).unwrap();
    let mut a = g.attempt.take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Brute, 5, true);
    a.boss_spawned = true;
    a.enemies[0].hp = 0;
    g.attempt = Some(a);
    g.tick(Input::default());
    assert_eq!(
        g.campaign.run_history.last().unwrap().outcome,
        Outcome::Victory
    );
    assert_eq!(g.campaign.run_history.len(), 302);
    let path =
        std::env::temp_dir().join(format!("lastlight-run-history-{}.json", std::process::id()));
    g.save(&path).unwrap();
    let loaded = Game::load(&path).unwrap();
    assert_eq!(loaded.campaign.run_history, g.campaign.run_history);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn worker_history_uses_one_parent_owner_and_checkpoint_outbox_settles_once() {
    let mut g = legacy_game();
    g.deploy(0, Mode::Personal).unwrap();
    g.withdraw();
    let existing = g.campaign.run_history.len();
    g.campaign.attempts = 1;
    g.campaign.coins = 10_000;
    g.campaign.choose_start_country("USA").unwrap();
    let countries: Vec<_> = horde_core::world::world()
        .countries
        .iter()
        .take(2)
        .map(|c| (c.id.clone(), c.level_count()))
        .collect();
    for (id, count) in &countries {
        g.campaign.world.country_levels.insert(id.clone(), *count);
        g.campaign.world.cleared.push(id.clone());
    }
    let country = &countries[0].0;
    let recruit = g.strategy.recruit(&mut g.campaign, country).unwrap();
    let battle = g.deploy_survivor(recruit, country, 0).unwrap();
    assert!(g.battles[0].game.campaign.run_history.is_empty());
    g.battles[0].game.withdraw();
    assert_eq!(g.campaign.run_history.len(), existing);
    let path =
        std::env::temp_dir().join(format!("lastlight-run-outbox-{}.json", std::process::id()));
    g.save(&path).unwrap();
    let mut g = Game::load(&path).unwrap();
    g.tick(Input::default());
    assert_eq!(g.campaign.run_history.len(), existing + 1);
    let record = g.campaign.run_history.last().unwrap();
    assert_eq!(record.worker_id, Some(battle));
    assert_eq!(record.recruit_id, Some(recruit));
    assert_eq!(record.outcome, Outcome::Withdrawal);
    assert!(g.battles[0].game.campaign.run_history.is_empty());
    for _ in 0..4 {
        g.tick(Input::default());
    }
    assert_eq!(g.campaign.run_history.len(), existing + 1);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn older_checkpoint_geometry_is_eleven_by_thirty_with_empty_history_and_unknown_old_dps_window() {
    let mut g = legacy_game();
    g.deploy(0, Mode::Personal).unwrap();
    let mut value = serde_json::to_value(&g).unwrap();
    value["campaign"]
        .as_object_mut()
        .unwrap()
        .remove("run_history");
    value["campaign"]
        .as_object_mut()
        .unwrap()
        .remove("next_run_id");
    value["campaign"]
        .as_object_mut()
        .unwrap()
        .remove("balance_settings");
    for key in [
        "width_yards",
        "balance",
        "dps",
        "lane_count",
        "gear_snapshot",
    ] {
        value["attempt"].as_object_mut().unwrap().remove(key);
    }
    let payload = serde_json::to_string(&value).unwrap();
    let checksum = support::fnv1a(payload.as_bytes());
    let loaded = Game::decode_save_generation(
        &serde_json::to_vec(
            &serde_json::json!({"version":1,"checksum":checksum,"payload":payload}),
        )
        .unwrap(),
    )
    .unwrap();
    let a = loaded.attempt.unwrap();
    assert_eq!(a.lane_count, 11);
    assert_eq!(a.width_yards, 30.0);
    assert!(a.dps.buckets.is_empty());
    assert!(loaded.campaign.run_history.is_empty());
    assert_eq!(a.balance, BalanceSettings::legacy());
}
