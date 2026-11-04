#![allow(clippy::print_stdout, clippy::print_stderr)] // CLI example / test diagnostics
mod support;
use horde_core::content::{EnemyKind, Mode, StructureKind, content};
use horde_core::sim::{DEFENSE_LINE, Game, Input, Outcome};
use horde_core::status::StatusId;
use support::legacy_game;

fn game(mode: Mode) -> Game {
    let mut g = legacy_game();
    g.campaign.attempts = 1;
    g.deploy(0, mode).unwrap();
    g.set_paused(false);
    g
}
fn enemy(g: &mut Game, lane: u8, x: f32, boss: bool) {
    let mut a = g.attempt.take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Shambler, lane, boss);
    a.enemies.last_mut().unwrap().x = x;
    g.attempt = Some(a);
}
fn ticks(g: &mut Game, n: u32) {
    for _ in 0..n {
        g.tick(Input::default());
    }
}
#[test]
fn pause_freezes_every_authoritative_clock() {
    let mut g = game(Mode::Personal);
    enemy(&mut g, 0, 8.0, false);
    g.set_paused(true);
    let before = serde_json::to_string(&g).unwrap();
    ticks(&mut g, 300);
    assert_eq!(serde_json::to_string(&g).unwrap(), before);
}
#[test]
fn enemies_keep_lanes_and_contact_has_a_windup() {
    let mut g = game(Mode::Personal);
    enemy(&mut g, 0, DEFENSE_LINE + 0.001, false);
    let hp = g.attempt.as_ref().unwrap().hp;
    ticks(&mut g, 1);
    assert_eq!(g.attempt.as_ref().unwrap().hp, hp);
    ticks(&mut g, 30);
    assert_eq!(g.attempt.as_ref().unwrap().hp, hp);
    ticks(&mut g, 120);
    let a = g.attempt.as_ref().unwrap();
    assert!(a.hp < hp);
    assert_eq!(a.enemies[0].lane, 0);
    assert_eq!(a.last_damage_lane, Some(0));
}
#[test]
fn starter_can_hit_all_eleven_contact_and_approach_lanes() {
    for lane in 0..11 {
        for x in [1.5, 14.0, 29.0] {
            let mut g = game(Mode::Personal);
            enemy(&mut g, lane, x, false);
            let hp = g.attempt.as_ref().unwrap().enemies[0].hp;
            g.tick(Input {
                aim: [x, lane as f32 + 0.5],
                fire: true,
                ..Input::default()
            });
            let a = g.attempt.as_ref().unwrap();
            assert!(
                a.enemies.is_empty() || a.enemies[0].hp < hp,
                "lane {lane}, x {x}"
            );
        }
    }
}
#[test]
fn melee_contact_eligibility_and_exclusive_recovery() {
    let mut g = game(Mode::Personal);
    enemy(&mut g, 10, 2.5, false);
    g.tick(Input {
        aim: [2.5, 10.5],
        melee: true,
        ..Input::default()
    });
    assert_eq!(
        g.attempt.as_ref().unwrap().stamina,
        g.attempt.as_ref().unwrap().max_stamina
    );
    g.attempt.as_mut().unwrap().enemies[0].x = DEFENSE_LINE;
    g.attempt.as_mut().unwrap().ammo = 5;
    g.tick(Input {
        aim: [1.5, 10.5],
        melee: true,
        ..Input::default()
    });
    let a = g.attempt.as_ref().unwrap();
    assert!(a.melee_commit > 0);
    assert!(a.stamina < a.max_stamina);
    for _ in 0..20 {
        g.tick(Input {
            aim: [15.0, 5.5],
            fire: true,
            reload: true,
            ..Input::default()
        });
    }
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.ammo, 5);
    assert_eq!(a.reload_left, 0);
    assert_eq!(a.shots, 0);
}
#[test]
fn reload_cancelled_by_melee_never_grants_ammunition() {
    let mut g = game(Mode::Personal);
    enemy(&mut g, 0, 1.5, false);
    g.attempt.as_mut().unwrap().ammo = 2;
    g.tick(Input {
        reload: true,
        ..Input::default()
    });
    assert!(g.attempt.as_ref().unwrap().reload_left > 0);
    g.tick(Input {
        melee: true,
        aim: [1.5, 0.5],
        ..Input::default()
    });
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.reload_left, 0);
    assert_eq!(a.ammo, 2);
}
#[test]
fn kills_pay_once_and_results_never_pay_again() {
    let mut g = game(Mode::Personal);
    enemy(&mut g, 5, 8.0, false);
    g.attempt.as_mut().unwrap().enemies[0].hp = 0;
    ticks(&mut g, 1);
    let coins = g.campaign.coins;
    assert!(coins > 0);
    ticks(&mut g, 10);
    assert_eq!(g.campaign.coins, coins);
    g.withdraw();
    assert_eq!(g.campaign.coins, coins);
    assert_eq!(g.last_result.as_ref().unwrap().coins, coins);
    g.withdraw();
    assert_eq!(g.campaign.coins, coins);
}
#[test]
fn boss_timer_is_not_victory_and_boss_transaction_is_atomic() {
    let mut g = game(Mode::Personal);
    g.attempt.as_mut().unwrap().tick = (content().levels[0].boss_time * 60.0) as u64;
    ticks(&mut g, 1);
    assert!(g.last_result.is_none());
    assert!(g.attempt.as_ref().unwrap().boss_spawned);
    let a = g.attempt.as_mut().unwrap();
    a.enemies.iter_mut().find(|e| e.boss).unwrap().hp = 0;
    let before = g.campaign.coins;
    ticks(&mut g, 1);
    assert!(g.attempt.is_none());
    let r = g.last_result.as_ref().unwrap();
    assert_eq!(r.outcome, Outcome::Victory);
    assert!(r.first_clear);
    assert_eq!(
        g.campaign.coins - before,
        content().levels[0].boss_reward + content().levels[0].completion_bonus
    );
    ticks(&mut g, 10);
    assert_eq!(
        g.campaign.coins - before,
        content().levels[0].boss_reward + content().levels[0].completion_bonus
    );
}
#[test]
fn construction_bounds_invalid_spend_salvage_and_blocking() {
    let mut g = game(Mode::Personal);
    g.campaign.blueprints = horde_core::kind_map::KindMap::filled(true);
    g.set_paused(true);
    let supply = g.attempt.as_ref().unwrap().supply;
    assert!(g.place(StructureKind::Wall, 11, 0).is_err());
    assert!(g.place(StructureKind::Wall, 0, 5).is_err());
    assert_eq!(g.attempt.as_ref().unwrap().supply, supply);
    g.place(StructureKind::Wall, 0, 0).unwrap();
    assert!(g.place(StructureKind::Trap, 0, 0).is_err());
    let id = g.attempt.as_ref().unwrap().structures[0].id;
    let cost = content().structure(StructureKind::Wall).supply;
    assert_eq!(g.salvage(id).unwrap(), cost / 2);
    assert_eq!(g.attempt.as_ref().unwrap().supply, supply - cost + cost / 2);
    g.place(StructureKind::Wall, 0, 0).unwrap();
    enemy(&mut g, 0, 5.0, false);
    g.set_paused(false);
    ticks(&mut g, 40);
    let a = g.attempt.as_ref().unwrap();
    assert!(a.enemies[0].x >= a.structures[0].x() + 0.69);
    let id = a.structures[0].id;
    g.set_paused(true);
    let refund = g.salvage(id).unwrap();
    assert!(refund < content().structure(StructureKind::Wall).supply);
    g.set_paused(false);
    ticks(&mut g, 60);
    assert!(g.attempt.as_ref().unwrap().enemies[0].x < 4.8);
}
#[test]
fn spawn_capacity_skips_without_a_deferred_burst() {
    let mut g = game(Mode::Personal);
    let cap = content().levels[0].enemy_cap;
    for n in 0..cap {
        enemy(&mut g, (n % 11) as u8, 25.0, false);
    }
    ticks(&mut g, 300);
    assert_eq!(g.attempt.as_ref().unwrap().enemies.len(), cap);
    g.attempt.as_mut().unwrap().enemies.clear();
    ticks(&mut g, 1);
    assert!(g.attempt.as_ref().unwrap().enemies.len() <= 1);
}
#[test]
fn save_resume_preserves_rng_actions_and_rewards_exactly() {
    let mut original = game(Mode::Npc);
    ticks(&mut original, 400);
    let json = serde_json::to_string(&original).unwrap();
    let mut resumed: Game = serde_json::from_str(&json).unwrap();
    ticks(&mut original, 900);
    ticks(&mut resumed, 900);
    assert_eq!(
        serde_json::to_string(&original).unwrap(),
        serde_json::to_string(&resumed).unwrap()
    );
}
#[test]
fn wounded_npc_at_zero_is_not_automatically_dead() {
    let mut g = game(Mode::Npc);
    let a = g.attempt.as_mut().unwrap();
    a.injury.hp = 0;
    a.hp = 0;
    ticks(&mut g, 1);
    assert!(g.attempt.is_some());
    assert!(!g.attempt.as_ref().unwrap().injury.dead);
}
#[test]
fn unconscious_hostile_and_boss_release_slot_and_pay_once() {
    let mut g = game(Mode::Npc);
    enemy(&mut g, 0, 1.5, false);
    g.attempt.as_mut().unwrap().enemies[0].injury.unconscious = true;
    ticks(&mut g, 1);
    assert!(g.attempt.as_ref().unwrap().enemies.is_empty());
    let coins = g.campaign.coins;
    ticks(&mut g, 2);
    assert_eq!(g.campaign.coins, coins);
    enemy(&mut g, 5, 20.0, true);
    let a = g.attempt.as_mut().unwrap();
    a.boss_spawned = true;
    a.enemies
        .iter_mut()
        .find(|e| e.boss)
        .unwrap()
        .injury
        .unconscious = true;
    ticks(&mut g, 1);
    assert_eq!(g.last_result.as_ref().unwrap().outcome, Outcome::Victory);
}
#[test]
fn npc_manual_inputs_do_not_fire_or_reset_action_timers() {
    let mut first = game(Mode::Npc);
    enemy(&mut first, 4, 20.0, false);
    let mut second: Game = serde_json::from_str(&serde_json::to_string(&first).unwrap()).unwrap();
    for _ in 0..180 {
        first.tick(Input::default());
        second.tick(Input {
            fire: true,
            melee: true,
            reload: true,
            heal: true,
            brace: true,
            ..Input::default()
        });
    }
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap()
    );
}
fn strategic_game() -> Game {
    let mut g = legacy_game();
    let countries: Vec<_> = horde_core::world::world()
        .countries
        .iter()
        .take(4)
        .map(|c| (c.id.clone(), c.level_count()))
        .collect();
    g.campaign.choose_start_country(&countries[0].0).unwrap();
    for (id, count) in countries {
        g.campaign.world.country_levels.insert(id.clone(), count);
        g.campaign.world.cleared.push(id);
    }
    g.campaign.coins = 10000;
    g.campaign.attempts = 1;
    g
}
#[test]
fn parallel_battles_pause_and_resume_all_authoritative_state() {
    let mut g = strategic_game();
    let country = g.campaign.world.cleared[0].clone();
    let r1 = g.strategy.recruit(&mut g.campaign, &country).unwrap();
    let r2 = g.strategy.recruit(&mut g.campaign, &country).unwrap();
    g.deploy_survivor(r1, &country, 0).unwrap();
    g.deploy_survivor(r2, &country, 0).unwrap();
    ticks(&mut g, 420);
    assert_eq!(g.battles[0].game.attempt.as_ref().unwrap().tick, 420);
    assert_eq!(g.battles[1].game.attempt.as_ref().unwrap().tick, 420);
    let mut other: Game = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
    ticks(&mut g, 500);
    ticks(&mut other, 500);
    assert_eq!(
        serde_json::to_string(&g).unwrap(),
        serde_json::to_string(&other).unwrap()
    );
    g.set_paused(true);
    let paused = serde_json::to_string(&g).unwrap();
    ticks(&mut g, 120);
    assert_eq!(serde_json::to_string(&g).unwrap(), paused);
}
#[test]
fn parallel_rewards_and_unique_loot_settle_exactly_once() {
    let mut g = strategic_game();
    let country = g.campaign.world.cleared[0].clone();
    let r1 = g.strategy.recruit(&mut g.campaign, &country).unwrap();
    let r2 = g.strategy.recruit(&mut g.campaign, &country).unwrap();
    let b1 = g.deploy_survivor(r1, &country, 0).unwrap();
    g.deploy_survivor(r2, &country, 0).unwrap();
    assert!(g.deploy_survivor(r1, &country, 0).is_err());
    for b in &mut g.battles {
        enemy(&mut b.game, 5, 20.0, true);
        let a = b.game.attempt.as_mut().unwrap();
        a.boss_spawned = true;
        a.enemies[0].injury.unconscious = true;
    }
    let coins = g.campaign.coins;
    let items = g.campaign.items.len();
    let payout = content().levels[0].boss_reward + content().levels[0].completion_bonus;
    let expected_payout: u64 = g
        .battles
        .iter()
        .map(|b| (payout as f32 * b.game.attempt.as_ref().unwrap().modifiers.reward).round() as u64)
        .sum();
    ticks(&mut g, 1);
    assert_eq!(g.campaign.coins - coins, expected_payout);
    assert_eq!(g.campaign.items.len(), items + 2);
    let ids: std::collections::BTreeSet<_> = g.campaign.items.iter().map(|i| i.id).collect();
    assert_eq!(ids.len(), g.campaign.items.len());
    let paid = g.campaign.coins;
    ticks(&mut g, 2);
    assert_eq!(g.campaign.coins, paid);
    assert!(g.campaign.reserved_items.is_empty());
    assert!(g.battles.iter().all(|b| b.settled));
    assert!(g.take_settlement_issues().is_empty());
    assert!(g.recall_survivor(b1).is_err());
}
#[test]
fn failed_parallel_loot_insert_is_reported_once_instead_of_silently_lost() {
    use horde_core::sim::SettlementIssue;
    let mut g = strategic_game();
    let country = g.campaign.world.cleared[0].clone();
    let recruit = g.strategy.recruit(&mut g.campaign, &country).unwrap();
    let battle = g.deploy_survivor(recruit, &country, 0).unwrap();
    // Exhaust the shared stash's identity space so the transfer must fail.
    g.campaign.next_item_id = u64::MAX;
    let items = g.campaign.items.len();
    g.battles[0]
        .game
        .campaign
        .add_item_with_quality(4, 9, None)
        .unwrap();
    g.tick(Input::default());
    assert_eq!(g.campaign.items.len(), items);
    let issues = g.take_settlement_issues();
    assert_eq!(
        issues,
        vec![SettlementIssue::LootLost {
            battle,
            definition: horde_core::content::ItemDefId::at(4),
            level: 9,
            error: "Persistent item identity space exhausted".into(),
        }]
    );
    assert!(issues[0].to_string().contains("was not stored"));
    ticks(&mut g, 3);
    assert!(g.take_settlement_issues().is_empty());
}
#[test]
fn country_modifiers_and_replayed_clears_preserve_local_progress() {
    let mut g = legacy_game();
    let id = horde_core::world::world()
        .countries
        .iter()
        .find(|c| c.level_count() > 1)
        .unwrap()
        .id
        .clone();
    g.campaign.choose_start_country(&id).unwrap();
    g.deploy_country_stage(0, Mode::Personal).unwrap();
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.country_id.as_deref(), Some(id.as_str()));
    let country = horde_core::world::world().country(&id).unwrap();
    assert_eq!(
        a.modifiers.enemy_health,
        country.modifiers.enemy_health * country.difficulty
    );
    g.set_paused(false);
    enemy(&mut g, 5, 25.0, true);
    let a = g.attempt.as_mut().unwrap();
    a.boss_spawned = true;
    a.enemies[0].hp = 0;
    ticks(&mut g, 1);
    assert_eq!(g.campaign.world.completed(&id), 1);
    g.retry().unwrap();
    g.set_paused(false);
    enemy(&mut g, 5, 25.0, true);
    let a = g.attempt.as_mut().unwrap();
    a.boss_spawned = true;
    a.enemies[0].hp = 0;
    ticks(&mut g, 1);
    assert_eq!(g.campaign.world.completed(&id), 1);
}
#[test]
fn active_parallel_save_recovers_a_complete_previous_generation() {
    let mut g = strategic_game();
    let country = g.campaign.world.cleared[0].clone();
    let id = g.strategy.recruit(&mut g.campaign, &country).unwrap();
    g.deploy_survivor(id, &country, 0).unwrap();
    ticks(&mut g, 120);
    let path = std::env::temp_dir().join(format!("horde-save-{}.json", std::process::id()));
    g.save(&path).unwrap();
    let first = serde_json::to_string(&g).unwrap();
    ticks(&mut g, 60);
    g.save(&path).unwrap();
    let loaded = Game::load(&path).unwrap();
    assert_eq!(
        serde_json::to_string(&loaded).unwrap(),
        serde_json::to_string(&g).unwrap()
    );
    std::fs::write(&path, "interrupted generation").unwrap();
    let recovered = Game::load(&path).unwrap();
    assert_eq!(serde_json::to_string(&recovered).unwrap(), first);
    for p in [
        path.clone(),
        path.with_extension("bak"),
        path.with_extension("tmp"),
    ] {
        let _ = std::fs::remove_file(p);
    }
}
#[test]
fn grand_strategy_country_operation_advances_until_country_is_free() {
    let mut g = strategic_game();
    let origin = g.campaign.world.cleared[0].clone();
    let country = horde_core::world::world()
        .countries
        .iter()
        .find(|c| {
            g.campaign.world.can_enter(&c.id)
                && !g.campaign.world.cleared.contains(&c.id)
                && c.level_count() > 1
        })
        .unwrap()
        .clone();
    let id = g.strategy.recruit(&mut g.campaign, &origin).unwrap();
    let operation = g.deploy_country_operation(id, &country.id).unwrap();
    for stage in 0..country.level_count() {
        let b = g.battles.iter_mut().find(|b| b.id == operation).unwrap();
        assert_eq!(b.stage, stage);
        assert!(!b.settled);
        enemy(&mut b.game, 5, 20.0, true);
        let a = b.game.attempt.as_mut().unwrap();
        a.boss_spawned = true;
        a.enemies[0].injury.unconscious = true;
        ticks(&mut g, 1);
        assert_eq!(g.campaign.world.completed(&country.id), stage + 1);
        assert!(g.take_settlement_issues().is_empty());
    }
    assert!(
        g.battles
            .iter()
            .find(|b| b.id == operation)
            .unwrap()
            .settled
    );
    assert!(g.campaign.world.cleared.contains(&country.id));
}
#[test]
fn retry_keeps_reported_country_but_every_main_round_stops_for_upgrades() {
    let mut g = strategic_game();
    let original = g.campaign.world.cleared[0].clone();
    let other = g.campaign.world.cleared[1].clone();
    g.campaign.enter_country(&original).unwrap();
    g.deploy_country_stage(0, Mode::Npc).unwrap();
    g.set_paused(false);
    enemy(&mut g, 5, 20.0, true);
    let a = g.attempt.as_mut().unwrap();
    a.boss_spawned = true;
    a.enemies[0].injury.unconscious = true;
    ticks(&mut g, 1);
    g.campaign.enter_country(&other).unwrap();
    g.retry().unwrap();
    assert_eq!(
        g.attempt.as_ref().unwrap().country_id.as_deref(),
        Some(original.as_str())
    );
    g.set_paused(false);
    enemy(&mut g, 5, 20.0, true);
    let a = g.attempt.as_mut().unwrap();
    a.boss_spawned = true;
    a.enemies[0].injury.unconscious = true;
    ticks(&mut g, 1);
    g.campaign.enter_country(&other).unwrap();
    g.start_repeat(2).unwrap();
    assert_eq!(
        g.attempt.as_ref().unwrap().country_id.as_deref(),
        Some(original.as_str())
    );
    assert!(!g.global_paused);
    enemy(&mut g, 5, 20.0, true);
    let a = g.attempt.as_mut().unwrap();
    a.boss_spawned = true;
    a.enemies[0].injury.unconscious = true;
    ticks(&mut g, 1);
    assert!(!g.continue_repeat().unwrap());
    assert!(g.attempt.is_none());
    assert_eq!(g.repeat_remaining, 0);
}
#[test]
fn campaign_gates_are_achievable_in_both_modes_without_rare_loot() {
    for seed in [1, 2026, 0x1A571167] {
        let report = horde_core::sim::smoke_seed(seed)
            .unwrap_or_else(|error| panic!("seed {seed}: {error}"));
        eprintln!("Campaign proof seed {seed}:\n{report}");
        assert!(
            report
                .lines()
                .any(|line| line.starts_with("Personal stage5") && line.contains("Victory"))
        );
        assert!(
            report
                .lines()
                .any(|line| line.starts_with("Npc stage5") && line.contains("Victory"))
        );
    }
}
#[test]
fn invalid_worker_reservation_cannot_replace_a_valid_save_generation() {
    let mut g = strategic_game();
    let country = g.campaign.world.cleared[0].clone();
    let id = g.strategy.recruit(&mut g.campaign, &country).unwrap();
    g.deploy_survivor(id, &country, 0).unwrap();
    ticks(&mut g, 120);
    let path = std::env::temp_dir().join(format!("horde-invalid-save-{}.json", std::process::id()));
    g.save(&path).unwrap();
    let valid = serde_json::to_string(&g).unwrap();
    g.battles[0].loadout[0] = u64::MAX;
    assert!(g.save(&path).is_err());
    assert_eq!(
        serde_json::to_string(&Game::load(&path).unwrap()).unwrap(),
        valid
    );
    for p in [
        path.clone(),
        path.with_extension("bak"),
        path.with_extension("tmp"),
    ] {
        let _ = std::fs::remove_file(p);
    }
}
#[test]
fn npc_draws_a_different_weapon_with_a_ready_turn_before_attacking() {
    let mut g = game(Mode::Npc);
    g.campaign.npc.policy = horde_core::content::Policy::Guardian;
    enemy(&mut g, 0, 1.5, false);
    g.attempt.as_mut().unwrap().tick = 59;
    ticks(&mut g, 1);
    let a = g.attempt.as_ref().unwrap();
    assert!(a.decision.starts_with("Ready"));
    assert_eq!(a.active_damage, 0);
    ticks(&mut g, 60);
    assert!(
        g.attempt
            .as_ref()
            .unwrap()
            .decision
            .starts_with("Attack: strike")
    );
}
#[test]
fn cautious_policy_aims_when_useful_even_if_optional_aiming_is_disabled() {
    let mut g = game(Mode::Npc);
    g.campaign.npc.prefer_aim = false;
    g.campaign.npc.allow_risk = false;
    enemy(&mut g, 5, 29.5, false);
    let a = g.attempt.as_mut().unwrap();
    a.profile.ranged_skill = 12;
    a.profile.npc.skill_bonus = 0;
    a.profile.npc.accuracy = 5;
    let magazine = a.ammo;
    ticks(&mut g, 59);
    assert_eq!(g.attempt.as_ref().unwrap().shots, 0);
    ticks(&mut g, 1);
    let a = g.attempt.as_ref().unwrap();
    assert!(a.decision.starts_with("Aim at"));
    assert_eq!(a.ammo, magazine);
    assert_eq!(a.shots, 0);
    ticks(&mut g, 60);
    let a = g.attempt.as_ref().unwrap();
    assert!(a.decision.starts_with("Attack: shoot"));
    assert_eq!(a.shots, 1);
    assert_eq!(a.ammo, magazine - 1);
}
#[test]
fn risk_preference_changes_low_odds_actions_only_at_the_next_legal_turn() {
    let mut g = game(Mode::Npc);
    g.campaign.npc.prefer_aim = false;
    g.campaign.npc.allow_risk = false;
    enemy(&mut g, 5, 29.5, false);
    let a = g.attempt.as_mut().unwrap();
    a.profile.ranged_skill = 12;
    a.profile.npc.skill_bonus = 0;
    a.profile.npc.accuracy = 0;
    let magazine = a.ammo;
    let initial_random_state = a.rng.state;
    ticks(&mut g, 60);
    let a = g.attempt.as_ref().unwrap();
    assert!(a.decision.starts_with("Do Nothing"));
    assert_eq!(a.ammo, magazine);
    assert_eq!(a.shots, 0);
    assert_eq!(
        a.rng.state, initial_random_state,
        "Declining a shot must not inspect or consume future dice"
    );
    ticks(&mut g, 30);
    let before = serde_json::to_string(g.attempt.as_ref().unwrap()).unwrap();
    g.campaign.npc.allow_risk = true;
    assert_eq!(
        serde_json::to_string(g.attempt.as_ref().unwrap()).unwrap(),
        before,
        "Changing policy cannot reset action state or clocks"
    );
    ticks(&mut g, 29);
    assert_eq!(g.attempt.as_ref().unwrap().shots, 0);
    ticks(&mut g, 1);
    let a = g.attempt.as_ref().unwrap();
    assert!(a.decision.starts_with("Attack: shoot"));
    assert_eq!(a.shots, 1);
    assert_eq!(a.ammo, magazine - 1);
    g.campaign.npc.allow_risk = false;
    ticks(&mut g, 60);
    assert_eq!(
        g.attempt.as_ref().unwrap().shots,
        1,
        "Returning to cautious policy cannot fire another low-odds shot"
    );
}

#[test]
fn crowded_lane_arrivals_never_push_existing_hostiles_rightward() {
    let mut g = game(Mode::Personal);
    let mut a = g.attempt.take().unwrap();
    for _ in 0..24 {
        g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
    }
    g.attempt = Some(a);
    for _ in 0..120 {
        let positions: std::collections::BTreeMap<_, _> = g
            .attempt
            .as_ref()
            .unwrap()
            .enemies
            .iter()
            .map(|e| (e.id, e.x))
            .collect();
        g.tick(Input::default());
        for e in &g.attempt.as_ref().unwrap().enemies {
            if let Some(previous) = positions.get(&e.id) {
                assert!(
                    e.x <= *previous,
                    "Hostile {} moved right: {} -> {}",
                    e.id,
                    previous,
                    e.x
                );
            }
        }
    }
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.telemetry.spawned, 24);
    assert_eq!(a.telemetry.peak_enemies, 24);
}

#[test]
fn every_contact_lane_publishes_a_traceable_windup_before_damage() {
    use horde_core::sim::EventKind;
    let mut g = game(Mode::Personal);
    for lane in 0..11 {
        enemy(&mut g, lane, 1.5, false);
    }
    ticks(&mut g, 1);
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.hp, a.max_hp);
    for lane in 0..11 {
        let e = a.enemies.iter().find(|e| e.lane == lane).unwrap();
        assert!(a.events.iter().any(|event| event.kind == EventKind::Windup
            && event.actor_id == Some(e.id)
            && event.lane == Some(lane)
            && event.duration_ticks >= 30));
    }
    ticks(&mut g, 100);
    let telemetry = g
        .attempt
        .as_ref()
        .map(|a| &a.telemetry)
        .or_else(|| g.last_result.as_ref().map(|r| &r.telemetry))
        .unwrap();
    assert!(telemetry.damage_by_lane.iter().all(|damage| *damage > 0));
}

#[test]
fn boss_exposure_is_relative_to_arrival_and_uses_authored_timing() {
    let mut g = game(Mode::Personal);
    g.attempt.as_mut().unwrap().tick = 12_345;
    enemy(&mut g, 5, 29.0, true);
    let boss = g
        .attempt
        .as_mut()
        .unwrap()
        .enemies
        .iter_mut()
        .find(|e| e.boss)
        .unwrap();
    boss.boss_timing.exposure_delay = 2.0;
    boss.boss_timing.exposure_duration = 3.0;
    boss.boss_timing.cycle = 10.0;
    let born = boss.born_tick;
    assert!(!boss.exposed_at(born + 119));
    assert!(boss.exposed_at(born + 120));
    assert!(boss.exposed_at(born + 299));
    assert!(!boss.exposed_at(born + 300));
    assert!(boss.exposed_at(born + 720));
}

#[test]
fn caller_announces_specific_lanes_before_reinforcements_and_resume_preserves_them() {
    use horde_core::sim::EventKind;
    let mut g = legacy_game();
    g.campaign.first_clear(0, Mode::Personal);
    g.deploy(1, Mode::Personal).unwrap();
    g.set_paused(false);
    let boss_tick = (content().levels[1].boss_time * 60.0) as u64;
    g.attempt.as_mut().unwrap().tick = boss_tick - 301;
    ticks(&mut g, 1);
    assert!(
        g.attempt
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|e| e.kind == EventKind::BossWarning)
    );
    ticks(&mut g, 300);
    let born = g
        .attempt
        .as_ref()
        .unwrap()
        .enemies
        .iter()
        .find(|e| e.boss)
        .unwrap()
        .born_tick;
    let timing = &content().levels[1].boss_timing;
    let warning_tick = born + ((timing.initial_call - timing.call_warning) * 60.0).round() as u64;
    let call_tick = born + (timing.initial_call * 60.0).round() as u64;
    while g.attempt.as_ref().unwrap().tick < warning_tick {
        g.tick(Input::default());
    }
    let lanes: Vec<_> = g
        .attempt
        .as_ref()
        .unwrap()
        .events
        .iter()
        .filter(|e| e.kind == EventKind::Reinforcements && e.tick == warning_tick)
        .map(|e| e.lane.unwrap())
        .collect();
    assert_eq!(lanes.len(), 2);
    assert_ne!(lanes[0], lanes[1]);
    let mut resumed: Game = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
    while g.attempt.as_ref().unwrap().tick < call_tick {
        g.tick(Input::default());
        resumed.tick(Input::default());
    }
    for lane in lanes {
        assert!(
            g.attempt
                .as_ref()
                .unwrap()
                .enemies
                .iter()
                .any(|e| e.kind == EnemyKind::Runner && e.born_tick == call_tick && e.lane == lane)
        );
    }
    assert_eq!(
        serde_json::to_string(&g).unwrap(),
        serde_json::to_string(&resumed).unwrap()
    );
}

fn npc_attack_fixture(attack_roll: i32, defense_roll: Option<i32>, armor: i32) -> (Game, u64) {
    use horde_core::rules::Rng;
    let mut g = game(Mode::Npc);
    g.campaign.npc.prefer_aim = false;
    enemy(&mut g, 5, 10.0, false);
    let seed = (0..1_000_000)
        .find(|seed| {
            let mut rng = Rng::new(*seed);
            rng.roll3d6() == attack_roll && defense_roll.is_none_or(|roll| rng.roll3d6() == roll)
        })
        .unwrap();
    let a = g.attempt.as_mut().unwrap();
    a.profile.ranged_skill = 20;
    a.enemies[0].armor = armor;
    a.rng = Rng::new(seed);
    (g, seed)
}
#[test]
fn npc_impacts_distinguish_misses_defenses_protection_and_injury() {
    use horde_core::sim::EventKind;
    for (roll, defense, armor, kind) in [
        (17, None, 0, EventKind::Miss),
        (10, Some(7), 0, EventKind::Defended),
        (3, None, 100, EventKind::Armor),
        (3, None, 0, EventKind::Hit),
    ] {
        let (mut g, _) = npc_attack_fixture(roll, defense, armor);
        let target = g.attempt.as_ref().unwrap().enemies[0].id;
        ticks(&mut g, 60);
        let a = g.attempt.as_ref().unwrap();
        assert_eq!(a.shots, 1);
        assert!(
            a.events.iter().any(|event| event.kind == kind
                && event.target_id == Some(target)
                && event.actor_id.is_none()
                && event.lane == Some(5)),
            "Missing {kind:?} authoritative impact"
        );
        assert_eq!(a.telemetry.npc_decisions.values().sum::<u32>(), 1);
        if let Some(enemy) = a.enemies.iter().find(|enemy| enemy.id == target) {
            assert_eq!(
                enemy.hit_stop_remaining(),
                if kind == EventKind::Hit { 6 } else { 0 }
            );
        }
        if kind != EventKind::Hit {
            assert_eq!(a.active_damage, 0);
        }
    }
}

#[test]
fn positive_personal_injury_holds_six_movement_steps_then_resumes_for_enemies_and_bosses() {
    use horde_core::sim::ENEMY_HIT_STOP_TICKS;
    for boss in [false, true] {
        let mut g = game(Mode::Personal);
        enemy(&mut g, 5, 10.0, boss);
        let a = g.attempt.as_mut().unwrap();
        a.boss_spawned = boss;
        a.profile.personal.damage = 1;
        a.profile.personal.weak_bonus = 0.0;
        a.enemies[0].armor = 0;
        let hp = a.enemies[0].hp;
        g.tick(Input {
            fire: true,
            aim: [10.0, 5.5],
            ..Input::default()
        });
        assert_eq!(g.attempt.as_ref().unwrap().enemies[0].hp, hp - 1);
        assert_eq!(g.attempt.as_ref().unwrap().enemies[0].x, 10.0);
        ticks(&mut g, ENEMY_HIT_STOP_TICKS - 1);
        let a = g.attempt.as_ref().unwrap();
        assert_eq!(a.enemies[0].x, 10.0);
        assert_eq!(a.enemies[0].hit_stop_remaining(), 0);
        assert_eq!(a.shots, 1);
        assert_eq!(a.enemies[0].hp, hp - 1);
        ticks(&mut g, 1);
        assert!(g.attempt.as_ref().unwrap().enemies[0].x < 10.0);
    }
}

#[test]
fn repeated_injury_refreshes_without_accumulating_and_misses_or_armor_do_not_hold_movement() {
    let mut g = game(Mode::Personal);
    enemy(&mut g, 5, 10.0, false);
    let a = g.attempt.as_mut().unwrap();
    a.profile.personal.damage = 1;
    a.profile.personal.weak_bonus = 0.0;
    for _ in 0..3 {
        g.attempt.as_mut().unwrap().shot_cooldown = 0;
        g.tick(Input {
            fire: true,
            aim: [10.0, 5.5],
            ..Input::default()
        });
        assert_eq!(
            g.attempt.as_ref().unwrap().enemies[0].hit_stop_remaining(),
            5
        );
        assert_eq!(g.attempt.as_ref().unwrap().enemies[0].x, 10.0);
    }
    ticks(&mut g, 5);
    ticks(&mut g, 1);
    assert!(g.attempt.as_ref().unwrap().enemies[0].x < 10.0);
    for armor in [0, 100] {
        let mut g = game(Mode::Personal);
        enemy(&mut g, 5, 10.0, false);
        g.attempt.as_mut().unwrap().enemies[0].armor = armor;
        g.tick(Input {
            fire: true,
            aim: if armor == 0 { [10.0, 0.5] } else { [10.0, 5.5] },
            ..Input::default()
        });
        let a = g.attempt.as_ref().unwrap();
        assert_eq!(a.active_damage, 0);
        assert_eq!(a.enemies[0].hit_stop_remaining(), 0);
        assert!(a.enemies[0].x < 10.0);
    }
}

#[test]
fn melee_and_sweep_hold_each_injured_enemy_without_extending_attack_windup() {
    let mut g = game(Mode::Personal);
    for lane in [4, 5, 6] {
        enemy(&mut g, lane, DEFENSE_LINE, false);
    }
    ticks(&mut g, 1);
    let a = g.attempt.as_mut().unwrap();
    a.profile.sweep = true;
    a.profile.personal.melee_damage = 2;
    let windup = a.enemies[0].attack_left;
    g.tick(Input {
        melee: true,
        aim: [DEFENSE_LINE, 5.5],
        ..Input::default()
    });
    let a = g.attempt.as_mut().unwrap();
    assert_eq!(a.melee_commit, 30);
    for e in &mut a.enemies {
        assert_eq!(e.hit_stop_remaining(), 5);
        assert_eq!(e.attack_left, windup - 1);
        e.x = 10.0;
    }
    ticks(&mut g, 5);
    assert!(
        g.attempt
            .as_ref()
            .unwrap()
            .enemies
            .iter()
            .all(|e| e.x == 10.0)
    );
    ticks(&mut g, 1);
    assert!(
        g.attempt
            .as_ref()
            .unwrap()
            .enemies
            .iter()
            .all(|e| e.x < 10.0)
    );
}

#[test]
fn traps_and_turrets_hold_only_positive_injuries_in_both_modes() {
    use horde_core::{
        rules::{InjuryState, Rng},
        sim::EventKind,
    };
    for mode in [Mode::Personal, Mode::Npc] {
        for kind in [StructureKind::Trap, StructureKind::Turret] {
            for armor in [0, 1000] {
                let mut g = game(mode);
                g.campaign.blueprints = horde_core::kind_map::KindMap::filled(true);
                g.set_paused(true);
                g.place(kind, 5, 0).unwrap();
                g.set_paused(false);
                let x = g.attempt.as_ref().unwrap().structures[0].x() + 0.4;
                enemy(&mut g, 5, x, false);
                let seed = (0..100_000)
                    .find(|seed| Rng::new(*seed).roll3d6() == 3)
                    .unwrap();
                let a = g.attempt.as_mut().unwrap();
                // Keep the target alive and free of major-wound immobility.
                a.enemies[0].max_hp = 1000;
                a.enemies[0].hp = 1000;
                a.enemies[0].injury = InjuryState::new(1000, 10);
                a.enemies[0].armor = armor;
                a.rng = Rng::new(seed);
                a.reload_left = 180;
                if mode == Mode::Npc {
                    a.tick = 59;
                }
                g.tick(Input::default());
                let a = g.attempt.as_ref().unwrap();
                let damage = 1000 - a.enemies[0].hp;
                assert_eq!(damage > 0, armor == 0, "{mode:?} {kind:?}");
                assert_eq!(
                    a.enemies[0].hit_stop_remaining(),
                    if damage > 0 { 6 } else { 0 }
                );
                assert!(
                    a.events
                        .iter()
                        .any(|e| e.actor_id == Some(a.structures[0].id)
                            && e.kind
                                == if damage > 0 {
                                    EventKind::Hit
                                } else {
                                    EventKind::Armor
                                })
                );
            }
        }
    }
}

#[test]
fn npc_hit_stop_holds_translation_without_extra_decisions_or_rerolls() {
    let (mut g, _) = npc_attack_fixture(3, None, 0);
    {
        let e = &mut g.attempt.as_mut().unwrap().enemies[0];
        e.max_hp = 1000;
        e.hp = 1000;
        e.injury = horde_core::rules::InjuryState::new(1000, 10);
    }
    ticks(&mut g, 60);
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.enemies[0].hit_stop_remaining(), 6);
    let x = a.enemies[0].x;
    let hp = a.enemies[0].hp;
    let rng = serde_json::to_string(&a.rng).unwrap();
    ticks(&mut g, 6);
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.enemies[0].x, x);
    assert_eq!(a.enemies[0].hp, hp);
    assert_eq!(serde_json::to_string(&a.rng).unwrap(), rng);
    assert_eq!(a.shots, 1);
    assert_eq!(a.telemetry.npc_decisions.values().sum::<u32>(), 1);
    ticks(&mut g, 1);
    assert!(g.attempt.as_ref().unwrap().enemies[0].x < x);
    ticks(&mut g, 53);
    assert_eq!(
        g.attempt
            .as_ref()
            .unwrap()
            .telemetry
            .npc_decisions
            .values()
            .sum::<u32>(),
        2
    );
}

#[test]
fn hit_stop_expires_in_fixed_ticks_while_contacted_or_queued_in_either_mode() {
    use horde_core::sim::EnemyPhase;
    for mode in [Mode::Personal, Mode::Npc] {
        let mut g = game(mode);
        enemy(&mut g, 5, DEFENSE_LINE, false);
        enemy(&mut g, 5, DEFENSE_LINE + 0.65, false);
        ticks(&mut g, 1);
        let a = g.attempt.as_mut().unwrap();
        assert_eq!(a.enemies[0].attack_phase, EnemyPhase::Windup);
        assert_eq!(a.enemies[1].attack_phase, EnemyPhase::Queued);
        let windup = a.enemies[0].attack_left;
        for e in &mut a.enemies {
            e.statuses.refresh(StatusId::HitStop, 6);
        }
        ticks(&mut g, 6);
        let a = g.attempt.as_ref().unwrap();
        assert!(a.enemies.iter().all(|e| e.hit_stop_remaining() == 0));
        assert_eq!(a.enemies[0].attack_left, windup - 6);
        assert_eq!(a.enemies[1].attack_phase, EnemyPhase::Queued);
    }
}

#[test]
fn hit_stop_survives_pause_and_save_resume_and_rejects_unbounded_saved_duration() {
    let mut g = game(Mode::Personal);
    enemy(&mut g, 5, 10.0, false);
    g.tick(Input {
        fire: true,
        aim: [10.0, 5.5],
        ..Input::default()
    });
    ticks(&mut g, 2);
    assert_eq!(
        g.attempt.as_ref().unwrap().enemies[0].hit_stop_remaining(),
        3
    );
    g.set_paused(true);
    let frozen = serde_json::to_string(&g).unwrap();
    ticks(&mut g, 120);
    assert_eq!(serde_json::to_string(&g).unwrap(), frozen);
    let path = std::env::temp_dir().join(format!("horde-hit-stop-{}.json", std::process::id()));
    g.save(&path).unwrap();
    let mut resumed = Game::load(&path).unwrap();
    assert_eq!(serde_json::to_string(&resumed).unwrap(), frozen);
    g.attempt.as_mut().unwrap().enemies[0]
        .statuses
        .refresh(StatusId::HitStop, 7);
    assert!(g.save(&path).is_err());
    let restored = &mut g.attempt.as_mut().unwrap().enemies[0].statuses;
    restored.remove(StatusId::HitStop);
    restored.refresh(StatusId::HitStop, 3);
    g.set_paused(false);
    resumed.set_paused(false);
    ticks(&mut g, 3);
    ticks(&mut resumed, 3);
    assert_eq!(resumed.attempt.as_ref().unwrap().enemies[0].x, 10.0);
    ticks(&mut g, 1);
    ticks(&mut resumed, 1);
    assert!(resumed.attempt.as_ref().unwrap().enemies[0].x < 10.0);
    assert_eq!(
        serde_json::to_string(&g).unwrap(),
        serde_json::to_string(&resumed).unwrap()
    );
    let mut legacy = serde_json::to_value(&g).unwrap();
    legacy["attempt"]["enemies"][0]
        .as_object_mut()
        .unwrap()
        .remove("statuses");
    let legacy: Game = serde_json::from_value(legacy).unwrap();
    assert_eq!(
        legacy.attempt.as_ref().unwrap().enemies[0].hit_stop_remaining(),
        0
    );
    for path in [
        path.clone(),
        path.with_extension("bak"),
        path.with_extension("tmp"),
    ] {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn a_boss_defeat_stops_piercing_followups_and_later_turrets() {
    for use_turrets in [false, true] {
        // This terminal-action fixture needs a shot that penetrates the new
        // boss armor. Purchase a rifle and prepare it through real training;
        // starter-pistol damage and caps are covered independently.
        let mut g = legacy_game();
        g.campaign.reward(130, 0, Mode::Personal);
        g.campaign.purchase_node("quick_reload").unwrap();
        g.campaign.purchase_node("rifle_training").unwrap();
        let rifle = g.campaign.buy_item(4).unwrap();
        g.campaign.equip(Mode::Personal, rifle).unwrap();
        g.deploy(0, Mode::Personal).unwrap();
        g.set_paused(false);
        if use_turrets {
            g.campaign.blueprints = horde_core::kind_map::KindMap::filled(true);
            g.set_paused(true);
            g.place(StructureKind::Turret, 5, 0).unwrap();
            g.place(StructureKind::Turret, 5, 1).unwrap();
            g.set_paused(false);
        }
        enemy(&mut g, 5, 9.0, false);
        // The nearer boss has the newer ID: spatial impact priority must win
        // over BTreeMap identity order before canceling terminal follow-ups.
        enemy(&mut g, 5, 8.0, true);
        let a = g.attempt.as_mut().unwrap();
        a.boss_spawned = true;
        a.enemies.iter_mut().for_each(|e| e.hp = 1);
        a.profile.pierce = true;
        g.tick(Input {
            fire: !use_turrets,
            aim: [8.0, 5.5],
            ..Input::default()
        });
        let report = g.last_result.as_ref().unwrap();
        assert_eq!(report.outcome, Outcome::Victory);
        assert_eq!(report.kills, 1);
        assert_eq!(
            report.coins,
            content().levels[0].boss_reward + content().levels[0].completion_bonus
        );
    }
}

#[test]
fn destroyed_blockers_release_references_before_the_same_tick_can_be_saved() {
    let mut g = legacy_game();
    g.campaign.first_clear(0, Mode::Personal);
    g.campaign.first_clear(1, Mode::Personal);
    g.campaign.reward(1000, 0, Mode::Personal);
    g.campaign.purchase_blueprint(StructureKind::Wall).unwrap();
    g.deploy(0, Mode::Personal).unwrap();
    g.place(StructureKind::Wall, 0, 0).unwrap();
    g.attempt.as_mut().unwrap().structures[0].hp = 1;
    enemy(&mut g, 0, 4.9, false);
    g.set_paused(false);
    for _ in 0..180 {
        g.tick(Input::default());
        if g.attempt.as_ref().unwrap().structures.is_empty() {
            break;
        }
    }
    let a = g.attempt.as_ref().unwrap();
    assert!(a.structures.is_empty());
    assert!(a.enemies[0].target_structure.is_none());
    assert_eq!(a.telemetry.structure_losses, 1);
    let path = std::env::temp_dir().join(format!(
        "horde-destroyed-blocker-{}.json",
        std::process::id()
    ));
    g.save(&path).unwrap();
    let loaded = Game::load(&path).unwrap();
    assert_eq!(
        serde_json::to_string(&g).unwrap(),
        serde_json::to_string(&loaded).unwrap()
    );
    for path in [
        path.clone(),
        path.with_extension("bak"),
        path.with_extension("tmp"),
    ] {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn cautious_contact_defense_is_a_whole_maneuver_with_a_real_dodge_bonus() {
    use horde_core::{
        content::Policy,
        rules::Rng,
        sim::{EventKind, NpcManeuver},
    };
    let mut g = game(Mode::Npc);
    g.campaign.npc.prefer_aim = false;
    g.campaign.npc.policy = Policy::Balanced;
    enemy(&mut g, 0, 1.5, false);
    let attack_roll = content().enemy(EnemyKind::Shambler).npc_skill.clamp(7, 10);
    let seed = (0..1_000_000)
        .find(|seed| {
            let mut rng = Rng::new(*seed);
            rng.roll3d6() == attack_roll && rng.roll3d6() == 10
        })
        .unwrap();
    let a = g.attempt.as_mut().unwrap();
    a.profile.ranged_skill = 5;
    a.profile.npc.skill_bonus = 0;
    a.profile.npc.accuracy = 0;
    a.rng = Rng::new(seed);
    ticks(&mut g, 120);
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.maneuver, NpcManeuver::AllOutDefense);
    assert_eq!(a.defense_bonus, 2);
    assert_eq!(a.shots, 0);
    assert_eq!(a.hp, a.max_hp);
    assert!(a.events.iter().any(|e| e.kind == EventKind::Defended
        && e.actor_id.is_some()
        && e.target_id.is_none()
        && e.lane == Some(0)));
    assert!(
        a.log
            .iter()
            .any(|line| line.contains("defense roll 10 vs 11"))
    );
    g.campaign.npc.allow_risk = true;
    ticks(&mut g, 60);
    assert_eq!(
        g.attempt.as_ref().unwrap().defense_bonus,
        0,
        "A different maneuver cannot retain last turn's defensive bonus"
    );
}

#[test]
fn siege_brute_recovers_then_telegraphs_before_another_attack() {
    use horde_core::sim::EnemyPhase;
    let mut g = legacy_game();
    g.campaign.first_clear(0, Mode::Personal);
    g.campaign.first_clear(1, Mode::Personal);
    g.deploy(2, Mode::Personal).unwrap();
    g.set_paused(false);
    let mut a = g.attempt.take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Brute, 5, true);
    a.enemies.last_mut().unwrap().x = 1.5;
    g.attempt = Some(a);
    g.attempt.as_mut().unwrap().boss_spawned = true;
    while g.attempt.as_ref().unwrap().telemetry.contact_attacks == 0 {
        g.tick(Input::default());
    }
    let first = g.attempt.as_ref().unwrap().tick;
    let recovery = (content().levels[2].boss_timing.siege_recovery * 60.0).round() as u32;
    assert_eq!(
        g.attempt
            .as_ref()
            .unwrap()
            .enemies
            .iter()
            .find(|e| e.boss)
            .unwrap()
            .attack_phase,
        EnemyPhase::Recovery
    );
    ticks(&mut g, recovery);
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.telemetry.contact_attacks, 1);
    assert_eq!(
        a.enemies.iter().find(|e| e.boss).unwrap().attack_phase,
        EnemyPhase::Windup
    );
    while g.attempt.as_ref().unwrap().telemetry.contact_attacks == 1 {
        g.tick(Input::default());
    }
    let a = g.attempt.as_ref().unwrap();
    assert!(
        a.tick - first
            >= u64::from(
                recovery + (content().enemy(EnemyKind::Brute).windup * 60.0).ceil() as u32
            )
    );
}

#[test]
fn fully_absorbed_personal_hits_and_invalid_aim_do_not_create_hidden_damage() {
    use horde_core::sim::EventKind;
    let mut g = game(Mode::Personal);
    enemy(&mut g, 5, 10.0, false);
    g.attempt.as_mut().unwrap().enemies[0].armor = 100;
    let hp = g.attempt.as_ref().unwrap().enemies[0].hp;
    g.tick(Input {
        fire: true,
        aim: [10.0, 5.5],
        ..Input::default()
    });
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.enemies[0].hp, hp);
    assert_eq!(a.active_damage, 0);
    assert_eq!(a.telemetry.absorbed, 1);
    assert!(
        a.events
            .iter()
            .any(|e| e.kind == EventKind::Armor && e.value == 0)
    );
    let ammo = a.ammo;
    ticks(&mut g, 60);
    g.tick(Input {
        fire: true,
        aim: [f32::NAN, 5.5],
        ..Input::default()
    });
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.ammo, ammo);
    assert!(
        a.events
            .iter()
            .all(|e| e.from.iter().chain(e.to.iter()).all(|v| v.is_finite()))
    );
}

#[test]
fn invalid_numeric_state_cannot_overwrite_the_last_valid_generation() {
    let mut g = game(Mode::Personal);
    let path = std::env::temp_dir().join(format!("horde-numeric-save-{}.json", std::process::id()));
    g.save(&path).unwrap();
    let valid = serde_json::to_string(&g).unwrap();
    g.attempt.as_mut().unwrap().stamina = f32::NAN;
    assert!(g.save(&path).is_err());
    assert_eq!(
        serde_json::to_string(&Game::load(&path).unwrap()).unwrap(),
        valid
    );
    for path in [
        path.clone(),
        path.with_extension("bak"),
        path.with_extension("tmp"),
    ] {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn paused_peak_review_fixture_uses_the_real_cap_and_an_affordable_defense_plan() {
    let mut g = horde_core::sim::peak_render_fixture().unwrap();
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.enemies.len(), a.enemy_cap() + 1);
    assert_eq!(a.enemies.iter().filter(|e| e.boss).count(), 1);
    assert!(
        StructureKind::ALL
            .iter()
            .all(|kind| a.structures.iter().any(|s| s.kind == *kind))
    );
    let spent: u32 = a.structures.iter().map(|s| s.cost).sum();
    assert_eq!(
        spent + a.supply,
        (content().levels[a.level].supply as i32
            + a.modifiers.supply
            + a.progression.supply_bonus as i32)
            .max(0) as u32
    );
    let frozen = serde_json::to_string(&g).unwrap();
    ticks(&mut g, 240);
    assert_eq!(serde_json::to_string(&g).unwrap(), frozen);
}

#[test]
fn parallel_loot_cursors_and_reports_do_not_duplicate_an_unused_shared_stash() {
    let mut g = strategic_game();
    let country = g.campaign.world.cleared[0].clone();
    for _ in 0..1000 {
        g.campaign.add_item(4).unwrap();
    }
    let recruit = g.strategy.recruit(&mut g.campaign, &country).unwrap();
    let original_items = g.campaign.items.len();
    g.deploy_survivor(recruit, &country, 0).unwrap();
    assert!(g.battles[0].game.campaign.items.len() <= 8);
    assert_eq!(g.campaign.items.len(), original_items);
    // A legacy save has no append cursor. Its first settlement must scan once
    // without paying for the original equipment snapshot again.
    g.battles[0].synced_item_index = 0;
    ticks(&mut g, 1);
    assert_eq!(g.campaign.items.len(), original_items);
    let battle = &mut g.battles[0];
    enemy(&mut battle.game, 5, 20.0, true);
    let a = battle.game.attempt.as_mut().unwrap();
    a.boss_spawned = true;
    a.enemies
        .iter_mut()
        .find(|e| e.boss)
        .unwrap()
        .injury
        .unconscious = true;
    ticks(&mut g, 1);
    assert!(g.battles[0].settled);
    assert!(g.battles[0].game.campaign.items.len() <= 8);
    assert_eq!(g.campaign.items.len(), original_items + 1);
}

#[test]
fn npc_turret_uses_its_authored_skill_and_a_critical_failure_requires_ready() {
    use horde_core::{rules::Rng, sim::EventKind};
    let mut g = game(Mode::Npc);
    g.campaign.blueprints = horde_core::kind_map::KindMap::filled(true);
    g.set_paused(true);
    g.place(StructureKind::Turret, 5, 4).unwrap();
    g.set_paused(false);
    enemy(&mut g, 5, 14.5, false);
    let seed = (0..100_000)
        .find(|seed| Rng::new(*seed).roll3d6() == 17)
        .unwrap();
    g.attempt.as_mut().unwrap().rng = Rng::new(seed);
    ticks(&mut g, 60);
    let a = g.attempt.as_ref().unwrap();
    let turret = &a.structures[0];
    let id = turret.id;
    assert!(
        a.log.iter().any(|line| line.contains("skill 15:")),
        "{:?}",
        a.log
    );
    assert!(
        a.events
            .iter()
            .any(|e| e.kind == EventKind::Miss && e.actor_id == Some(id))
    );
    assert_eq!(turret.ready_turns, 1);
    let shots_before = a
        .events
        .iter()
        .filter(|e| e.kind == EventKind::Shot && e.actor_id == Some(id))
        .count();
    assert_eq!(shots_before, 1);
    ticks(&mut g, 60);
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.structures[0].ready_turns, 0);
    assert!(
        !a.events
            .iter()
            .any(|e| e.kind == EventKind::Shot && e.actor_id == Some(id) && e.tick == 120)
    );
}

#[test]
fn a_single_generation_decoder_uses_the_same_schema_and_full_state_validation() {
    let mut g = game(Mode::Npc);
    ticks(&mut g, 121);
    let path = std::env::temp_dir().join(format!("horde-decode-save-{}.json", std::process::id()));
    g.save(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let decoded = Game::decode_save_generation(&bytes).unwrap();
    assert_eq!(
        serde_json::to_string(&g).unwrap(),
        serde_json::to_string(&decoded).unwrap()
    );
    assert!(Game::decode_save_generation(b"interrupted generation").is_err());
    for path in [
        path.clone(),
        path.with_extension("bak"),
        path.with_extension("tmp"),
    ] {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn npc_and_personal_attacks_cannot_target_unseen_entry_queue_positions() {
    let mut npc = game(Mode::Npc);
    enemy(&mut npc, 5, 40.0, false);
    ticks(&mut npc, 60);
    let a = npc.attempt.as_ref().unwrap();
    assert_eq!(a.decision_target, None);
    assert_eq!(a.shots, 0);
    assert!(a.decision.starts_with("Do Nothing"));
    npc.attempt.as_mut().unwrap().enemies[0].x = 29.5;
    ticks(&mut npc, 60);
    assert!(npc.attempt.as_ref().unwrap().decision_target.is_some());
    let mut personal = game(Mode::Personal);
    enemy(&mut personal, 5, 40.0, false);
    let hp = personal.attempt.as_ref().unwrap().enemies[0].hp;
    personal.tick(Input {
        fire: true,
        aim: [30.0, 5.5],
        ..Input::default()
    });
    assert_eq!(personal.attempt.as_ref().unwrap().enemies[0].hp, hp);
    assert_eq!(personal.attempt.as_ref().unwrap().telemetry.misses, 1);
}

#[test]
fn an_unaffordable_country_plan_starts_with_an_affordable_prefix_without_losing_the_plan() {
    use horde_core::campaign::Placement;
    let mut g = legacy_game();
    g.campaign.first_clear(0, Mode::Personal);
    g.campaign.first_clear(1, Mode::Personal);
    g.campaign.first_clear(2, Mode::Personal);
    g.campaign.reward(1000, 0, Mode::Personal);
    g.campaign
        .purchase_blueprint(StructureKind::Turret)
        .unwrap();
    g.campaign.choose_start_country("DEU").unwrap();
    let layout = (0..4)
        .map(|lane| Placement {
            kind: StructureKind::Turret,
            lane,
            column: 0,
        })
        .collect();
    g.campaign.save_country_layout("DEU", layout).unwrap();
    let coins = g.campaign.coins;
    let attempts = g.campaign.attempts;
    g.deploy_country_stage(0, Mode::Personal).unwrap();
    let a = g.attempt.as_ref().unwrap();
    let cost = content().structure(StructureKind::Turret).supply;
    let placed = a.structures.len();
    assert_eq!(placed + a.layout_omissions, 4);
    assert!(a.layout_omissions > 0);
    let budget = (content().levels[0].supply as i32
        + a.modifiers.supply
        + a.progression.supply_bonus as i32)
        .max(0) as u32;
    assert_eq!(a.supply + placed as u32 * cost, budget);
    assert_eq!(g.campaign.layout_for("DEU").len(), 4);
    assert_eq!(g.campaign.reserved, Some(Mode::Personal));
    assert_eq!(g.campaign.coins, coins);
    assert_eq!(g.campaign.attempts, attempts);
}
