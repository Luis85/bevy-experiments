mod support;
use horde_core::{
    content::{BossKind, EnemyKind, Mode, StructureKind, content},
    enemy_catalog::enemy_catalog,
    sim::{Game, Input},
    weapons::WeaponClass,
};
use support::legacy_game;

#[test]
fn boss_history_is_bounded_and_rejects_duplicate_kinds() {
    let mut game = legacy_game();
    game.campaign_mut().defeated_bosses = vec![BossKind::Breacher; 4];
    assert!(game.campaign().validate().is_err());
    game.campaign_mut().defeated_bosses = vec![BossKind::Breacher; 2];
    assert!(game.campaign().validate().is_err());
    game.campaign_mut().defeated_bosses =
        vec![BossKind::Breacher, BossKind::Caller, BossKind::SiegeBrute];
    game.campaign().validate().unwrap();
}

#[test]
fn authored_contact_armor_preserves_integer_halving_for_odd_protection() {
    for mode in [Mode::Personal, Mode::Npc] {
        let mut game = legacy_game();
        game.campaign_mut().attempts = 1;
        game.deploy(0, mode).unwrap();
        let mut attempt = game.attempt_slot_mut().take().unwrap();
        game.spawn_enemy(&mut attempt, EnemyKind::Armored, 5, false);
        assert_eq!(
            attempt.enemies[0]
                .resolved
                .as_ref()
                .unwrap()
                .contact_armor_fraction,
            0.5
        );
        attempt.enemies[0].armor = 3;
        attempt.enemies[0].x = horde_core::sim::DEFENSE_LINE + 0.1;
        assert_eq!(attempt.enemy_protection(&attempt.enemies[0]), 3);
        attempt.enemies[0].x = horde_core::sim::DEFENSE_LINE;
        assert_eq!(attempt.enemy_protection(&attempt.enemies[0]), 1);
    }
}

fn decode(
    mut value: serde_json::Value,
    field: &str,
    replacement: serde_json::Value,
) -> Result<Game, String> {
    value["attempt"]["profile"][field] = replacement;
    let payload = serde_json::to_string(&value).unwrap();
    let checksum = support::fnv1a(payload.as_bytes());
    Ok(Game::decode_save_generation(
        &serde_json::to_vec(
            &serde_json::json!({"version":1,"checksum":checksum,"payload":payload}),
        )
        .unwrap(),
    )?)
}

#[test]
fn pistol_cap_is_applied_once_after_all_pellets_and_weakpoint_damage() {
    for kind in EnemyKind::ALL {
        let mut game = legacy_game();
        game.deploy(0, Mode::Personal).unwrap();
        let mut a = game.attempt_slot_mut().take().unwrap();
        assert_eq!(a.profile.weapon.class, WeaponClass::Pistol);
        a.profile.weapon.pellets = 16;
        a.profile.weapon.spread_radians = 0.001;
        a.profile.personal.damage = 1000;
        a.profile.personal.weak_bonus = 10.0;
        game.spawn_enemy(&mut a, kind, 5, false);
        a.enemies[0].x = 12.0;
        let initial = a.enemies[0].max_hp;
        *game.attempt_slot_mut() = Some(a);
        game.set_paused(false);
        game.tick(Input {
            aim: [12.0, 5.5],
            fire: true,
            ..Default::default()
        });
        let a = game.attempt().unwrap();
        assert_eq!(a.shots, 1);
        assert_eq!(a.ammo + 1, a.magazine);
        assert_eq!(
            a.enemies[0].hp,
            initial - (initial as f32 * 0.8).floor() as i32
        );
        assert!(
            a.enemies[0].hp > 0,
            "fresh {kind:?} survives even stacked damage"
        );
    }
}

#[test]
fn boss_history_unlocks_ordinary_profiles_and_survives_serialization() {
    let mut game = legacy_game();
    game.deploy(0, Mode::Personal).unwrap();
    let mut a = game.attempt_slot_mut().take().unwrap();
    assert!(game.spawn_archetype(&mut a, "breacher-pack", 3).is_err());
    assert!(a.enemies.is_empty());
    game.campaign_mut().first_clear(0, Mode::Npc);
    assert!(
        game.campaign()
            .unlocked_bosses()
            .contains(&BossKind::Breacher)
    );
    game.spawn_archetype(&mut a, "breacher-pack", 3).unwrap();
    let enemy = &a.enemies[0];
    assert!(!enemy.boss);
    assert_eq!(enemy.boss_kind, None);
    assert_eq!(
        enemy.max_hp,
        enemy_catalog().resolve("breacher-pack").unwrap().health
    );
    assert_ne!(enemy.max_hp, content().levels[0].boss_health);
    let restored: Game = serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();
    assert_eq!(
        restored.campaign().unlocked_bosses(),
        game.campaign().unlocked_bosses()
    );
}

#[test]
fn attack_components_carry_demolition_even_on_runner_bodies() {
    fn damaged_wall(multiplier: u8) -> i32 {
        let mut game = legacy_game();
        game.campaign_mut().first_clear(0, Mode::Personal);
        game.campaign_mut().first_clear(1, Mode::Personal);
        game.campaign_mut().reward(1000, 0, Mode::Personal);
        game.campaign_mut()
            .purchase_blueprint(StructureKind::Wall)
            .unwrap();
        game.deploy(0, Mode::Personal).unwrap();
        game.place(StructureKind::Wall, 5, 0).unwrap();
        let mut a = game.attempt_slot_mut().take().unwrap();
        let hp = a.structures[0].hp;
        game.spawn_enemy(&mut a, EnemyKind::Runner, 5, false);
        a.enemies[0].x = a.structures[0].x() + 0.65;
        a.enemies[0]
            .resolved
            .as_mut()
            .unwrap()
            .structure_damage_multiplier = multiplier;
        *game.attempt_slot_mut() = Some(a);
        game.set_paused(false);
        for _ in 0..120 {
            game.tick(Input::default());
        }
        hp - game.attempt().unwrap().structures[0].hp
    }
    let ordinary = damaged_wall(1);
    assert!(ordinary > 0);
    assert_eq!(damaged_wall(2), ordinary * 2);
}

#[test]
fn frozen_weapon_and_skill_snapshots_reject_unsafe_numbers() {
    let mut game = legacy_game();
    game.deploy(0, Mode::Personal).unwrap();
    let value = serde_json::to_value(&game).unwrap();
    let mut weapon = value["attempt"]["profile"]["weapon"].clone();
    weapon["pellets"] = serde_json::json!(u32::MAX);
    assert!(decode(value.clone(), "weapon", weapon).is_err());
    let mut mods = value["attempt"]["profile"]["modifiers"].clone();
    mods["ordnance_radius_multiplier"] = serde_json::json!(1e30);
    assert!(decode(value, "modifiers", mods).is_err());
}
