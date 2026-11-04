mod support;
use horde_core::{
    content::{EnemyKind, Mode},
    ordnance::{self, OrdnanceCatalog, OrdnanceKind, OrdnanceState, ThrowRequest},
    sim::{ENEMY_HIT_STOP_TICKS, EventKind, Game, Input, Outcome},
};
use std::sync::atomic::{AtomicU64, Ordering};
use support::{advance, legacy_game};

fn game(mode: Mode, unlock: bool) -> Game {
    let mut game = legacy_game();
    if mode == Mode::Npc {
        game.campaign.attempts = 1;
    }
    if unlock {
        game.campaign.coins = 100;
        game.campaign.purchase_node("grenade_training").unwrap();
        game.campaign.purchase_node("molotov_training").unwrap();
    }
    game.deploy(0, mode).unwrap();
    game.set_paused(false);
    game
}
fn throw(game: &mut Game, kind: OrdnanceKind, cell: [u8; 2]) {
    game.tick(Input {
        throw: Some(ThrowRequest { kind, cell }),
        ..Input::default()
    });
}
fn enemy(game: &mut Game, lane: u8, x: f32, hp: i32, boss: bool) -> u64 {
    let mut attempt = game.attempt.take().unwrap();
    game.spawn_enemy(&mut attempt, EnemyKind::Shambler, lane, boss);
    let enemy = attempt.enemies.last_mut().unwrap();
    enemy.x = x;
    enemy.hp = hp;
    let id = enemy.id;
    game.attempt = Some(attempt);
    id
}
fn save_path() -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "lastlight-ordnance-{}-{}.json",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn authored_catalog_rejects_impossible_flight_radius_and_fire() {
    let catalog = ordnance::catalog().clone();
    catalog.validate().unwrap();
    let mut invalid = catalog.clone();
    invalid.grenade.flight_ticks = 0;
    assert!(invalid.validate().is_err());
    let mut invalid = catalog.clone();
    invalid.molotov.radius = f32::NAN;
    assert!(invalid.validate().is_err());
    let mut invalid = catalog.clone();
    invalid.molotov.burn_interval = 0;
    assert!(invalid.validate().is_err());
    let mut changed = catalog;
    changed.grenade.damage += 7;
    assert_eq!(
        OrdnanceCatalog::from_json(&serde_json::to_string(&changed).unwrap())
            .unwrap()
            .grenade
            .damage,
        changed.grenade.damage
    );
}

#[test]
fn skill_unlock_cells_cooldowns_and_finite_attempt_charges_are_authoritative() {
    let mut locked = game(Mode::Personal, false);
    throw(&mut locked, OrdnanceKind::Grenade, [15, 5]);
    assert_eq!(
        locked.attempt.as_ref().unwrap().ordnance,
        OrdnanceState::default()
    );
    let mut game = game(Mode::Personal, true);
    for cell in [[30, 5], [15, 11], [255, 255]] {
        throw(&mut game, OrdnanceKind::Grenade, cell);
    }
    assert_eq!(game.attempt.as_ref().unwrap().ordnance.charges, [2, 2]);
    for _ in 0..3 {
        throw(&mut game, OrdnanceKind::Grenade, [15, 5]);
        let charges = game.attempt.as_ref().unwrap().ordnance.charges[0];
        throw(&mut game, OrdnanceKind::Grenade, [15, 5]);
        assert_eq!(game.attempt.as_ref().unwrap().ordnance.charges[0], charges);
        advance(&mut game, ordnance::catalog().grenade.cooldown_ticks);
    }
    assert_eq!(game.attempt.as_ref().unwrap().ordnance.charges[0], 0);
    assert_eq!(game.attempt.as_ref().unwrap().ordnance.spent[0], 2);
    game.withdraw();
    game.retry().unwrap();
    assert_eq!(game.attempt.as_ref().unwrap().ordnance.charges, [2, 2]);
}

#[test]
fn npc_manual_throw_is_inert_and_preserves_rules_and_rng() {
    let mut requested = game(Mode::Npc, true);
    let mut control = requested.clone();
    for _ in 0..120 {
        throw(&mut requested, OrdnanceKind::Molotov, [15, 5]);
        control.tick(Input::default());
    }
    assert_eq!(
        serde_json::to_string(&requested).unwrap(),
        serde_json::to_string(&control).unwrap()
    );
}

#[test]
fn flight_delays_enemy_only_aoe_and_resolves_hit_stop_and_rewards_once() {
    let mut game = game(Mode::Personal, true);
    let victim = enemy(&mut game, 5, 15.5, 1, false);
    let survivor = enemy(&mut game, 6, 16.5, 25, false);
    let distant = enemy(&mut game, 0, 15.5, 25, false);
    let hp = game.attempt.as_ref().unwrap().hp;
    throw(&mut game, OrdnanceKind::Grenade, [15, 5]);
    let a = game.attempt.as_ref().unwrap();
    assert_eq!(a.enemies.iter().find(|e| e.id == victim).unwrap().hp, 1);
    assert_eq!(
        a.ordnance.projectiles[0].flight_left,
        ordnance::catalog().grenade.flight_ticks
    );
    advance(&mut game, ordnance::catalog().grenade.flight_ticks - 1);
    assert!(game.attempt.as_ref().unwrap().reward_ledger.is_empty());
    advance(&mut game, 1);
    let a = game.attempt.as_ref().unwrap();
    assert!(a.reward_ledger.contains(&victim));
    assert!(a.reward_ledger.contains(&survivor));
    assert_eq!(a.enemies.iter().find(|e| e.id == distant).unwrap().hp, 25);
    assert_eq!(a.hp, hp);
    assert!(a.events.iter().any(|e| e.kind == EventKind::Explosion));
    let coins = a.coins;
    advance(&mut game, 90);
    assert_eq!(game.attempt.as_ref().unwrap().coins, coins);
}

#[test]
fn molotov_burns_over_time_and_paused_flight_and_fire_are_immutable() {
    let mut game = game(Mode::Personal, true);
    let target = enemy(&mut game, 5, 15.5, 25, false);
    throw(&mut game, OrdnanceKind::Molotov, [15, 5]);
    game.set_paused(true);
    let paused = serde_json::to_string(&game).unwrap();
    advance(&mut game, 120);
    assert_eq!(serde_json::to_string(&game).unwrap(), paused);
    game.set_paused(false);
    advance(&mut game, ordnance::catalog().molotov.flight_ticks);
    let a = game.attempt.as_ref().unwrap();
    let hostile = a.enemies.iter().find(|e| e.id == target).unwrap();
    assert!(hostile.hp < 25 && hostile.hp > 0);
    assert!(
        hostile.hit_stop_remaining() > 0 && hostile.hit_stop_remaining() <= ENEMY_HIT_STOP_TICKS
    );
    assert_eq!(a.ordnance.fire_zones.len(), 1);
    game.set_paused(true);
    let paused = serde_json::to_string(&game).unwrap();
    advance(&mut game, 120);
    assert_eq!(serde_json::to_string(&game).unwrap(), paused);
    game.set_paused(false);
    advance(&mut game, ordnance::catalog().molotov.burn_interval);
    assert!(
        game.attempt
            .as_ref()
            .unwrap()
            .reward_ledger
            .contains(&target)
    );
}

#[test]
fn flight_and_fire_save_resume_are_identical_and_forged_ammo_is_rejected() {
    let mut original = game(Mode::Personal, true);
    enemy(&mut original, 5, 15.5, 25, false);
    throw(&mut original, OrdnanceKind::Molotov, [15, 5]);
    let path = save_path();
    for count in [12, 60] {
        advance(&mut original, count);
        original.save(&path).unwrap();
        let mut restored = Game::load(&path).unwrap();
        advance(&mut restored, 40);
        advance(&mut original, 40);
        assert_eq!(
            serde_json::to_string(&original).unwrap(),
            serde_json::to_string(&restored).unwrap()
        );
    }
    let mut forged = original.clone();
    forged.attempt.as_mut().unwrap().ordnance.charges[1] += 1;
    assert!(forged.save(&path).is_err());
    let mut forged = original;
    forged.attempt.as_mut().unwrap().ordnance.fire_zones[0].radius = f32::INFINITY;
    assert!(forged.save(&path).is_err());
    std::fs::remove_file(&path).unwrap();
    if path.with_extension("bak").exists() {
        std::fs::remove_file(path.with_extension("bak")).unwrap();
    }
}

#[test]
fn grenade_boss_defeat_uses_the_single_completion_and_reward_path() {
    let mut game = game(Mode::Personal, true);
    enemy(&mut game, 5, 15.5, 1, true);
    game.attempt.as_mut().unwrap().boss_spawned = true;
    throw(&mut game, OrdnanceKind::Grenade, [15, 5]);
    advance(&mut game, ordnance::catalog().grenade.flight_ticks);
    assert!(game.attempt.is_none());
    let report = game.last_result.as_ref().unwrap();
    assert_eq!(report.outcome, Outcome::Victory);
    assert_eq!(report.kills, 1);
    let coins = game.campaign.coins;
    advance(&mut game, 120);
    assert_eq!(game.campaign.coins, coins);
}

#[test]
fn old_attempts_default_to_empty_ordnance_without_refilling_owned_unlocks() {
    let game = game(Mode::Personal, true);
    let mut value = serde_json::to_value(&game).unwrap();
    value["attempt"].as_object_mut().unwrap().remove("ordnance");
    let restored: Game = serde_json::from_value(value).unwrap();
    assert_eq!(
        restored.attempt.as_ref().unwrap().ordnance,
        OrdnanceState::default()
    );
    let path = save_path();
    restored.save(&path).unwrap();
    std::fs::remove_file(path).unwrap();
}
