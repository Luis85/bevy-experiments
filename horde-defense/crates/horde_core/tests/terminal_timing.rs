mod support;
use horde_core::{
    content::{EnemyKind, Mode},
    rules::{InjuryState, Rng},
    sim::{EnemyCondition, EventKind, Game, Input},
};
use support::legacy_game;

#[test]
fn personal_actions_and_npc_defenses_reap_before_followup_actions_at_all_batch_sizes() {
    use horde_core::content::StructureKind;
    for batch in [1, 2, 4] {
        for melee in [false, true] {
            let mut g = legacy_game();
            g.deploy(0, Mode::Personal).unwrap();
            let mut a = g.attempt.take().unwrap();
            g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
            let e = &mut a.enemies[0];
            e.x = if melee { 1.5 } else { 10.0 };
            e.hp = 1;
            e.armor = 0;
            let id = e.id;
            let point = [e.x, e.y()];
            g.attempt = Some(a);
            g.tick(Input {
                aim: point,
                fire: !melee,
                melee,
                ..Input::default()
            });
            let a = g.attempt.as_ref().unwrap();
            assert!(a.enemies.is_empty());
            assert_eq!(a.corpses[0].at, point);
            assert_eq!(a.corpses[0].tick, 1);
            assert!(a.reward_ledger.contains(&id));
            let hp = a.hp;
            advance(&mut g, 4, batch);
            assert_eq!(g.attempt.as_ref().unwrap().hp, hp);
            assert_eq!(g.attempt.as_ref().unwrap().corpses.len(), 1);
        }
        for mode in [Mode::Personal, Mode::Npc] {
            for kind in [StructureKind::Trap, StructureKind::Turret] {
                let mut g = legacy_game();
                g.campaign.attempts = 1;
                for level in 0..3 {
                    g.campaign.first_clear(level, Mode::Personal);
                }
                g.campaign.reward(1000, 0, Mode::Personal);
                g.campaign.purchase_blueprint(kind).unwrap();
                g.deploy(0, mode).unwrap();
                g.place(kind, 5, 0).unwrap();
                let mut a = g.attempt.take().unwrap();
                g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
                let e = &mut a.enemies[0];
                e.x = if kind == StructureKind::Trap {
                    4.8
                } else {
                    8.0
                };
                e.hp = 1;
                e.max_hp = 1;
                e.armor = 0;
                e.injury = InjuryState::new(1, 10);
                let id = e.id;
                if mode == Mode::Npc {
                    a.tick = 59;
                    a.ammo = 0;
                    a.reload_left = 120;
                    let seed = (0..100_000)
                        .find(|seed| Rng::new(*seed).roll3d6() == 3)
                        .unwrap();
                    a.rng = Rng::new(seed);
                }
                let expected_tick = a.tick + 1;
                g.attempt = Some(a);
                advance(&mut g, 4, batch);
                let a = g.attempt.as_ref().unwrap();
                assert!(!a.enemies.iter().any(|e| e.id == id));
                assert_eq!(
                    a.corpses.iter().find(|e| e.id == id).unwrap().tick,
                    expected_tick
                );
                assert_eq!(a.kills, 1);
            }
        }
    }
}

fn fixture(roll: i32) -> (Game, u64) {
    let mut g = legacy_game();
    g.campaign.attempts = 1;
    g.campaign.npc.prefer_aim = false;
    g.campaign.npc.allow_risk = true;
    g.deploy(0, Mode::Npc).unwrap();
    let mut a = g.attempt.take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
    let e = &mut a.enemies[0];
    e.x = 10.0;
    let id = e.id;
    let seed = (0..100_000)
        .find(|seed| Rng::new(*seed).roll3d6() == roll)
        .unwrap();
    a.rng = Rng::new(seed);
    a.profile.ranged_skill = 20;
    g.attempt = Some(a);
    (g, id)
}
fn advance(g: &mut Game, ticks: u32, batch: u32) {
    let mut left = ticks;
    while left > 0 {
        let count = left.min(batch);
        for _ in 0..count {
            g.tick(Input::default());
        }
        left -= count;
    }
}

#[test]
fn terminal_npc_hits_remove_enemy_and_commit_reward_and_corpse_in_the_exact_tick_at_every_observation_batch()
 {
    let mut reference = None;
    for batch in [1, 2, 4] {
        let (mut g, id) = fixture(3);
        let e = &mut g.attempt.as_mut().unwrap().enemies[0];
        e.hp = 1;
        e.max_hp = 1;
        e.injury = InjuryState::new(1, 10);
        advance(&mut g, 59, batch);
        assert!(
            g.attempt
                .as_ref()
                .unwrap()
                .enemies
                .iter()
                .any(|e| e.id == id)
        );
        advance(&mut g, 1, batch);
        let a = g.attempt.as_ref().unwrap();
        assert_eq!(a.tick, 60);
        assert!(!a.enemies.iter().any(|e| e.id == id));
        assert_eq!(a.kills, 1);
        assert!(a.reward_ledger.contains(&id));
        let corpse = a.corpses.iter().find(|e| e.id == id).unwrap();
        let hit = a
            .events
            .iter()
            .find(|e| e.target_id == Some(id) && e.kind == EventKind::Hit)
            .unwrap();
        assert_eq!(corpse.tick, 60);
        assert_eq!(corpse.at, hit.to);
        let corpse = corpse.clone();
        let coins = g.campaign.coins;
        advance(&mut g, 120, batch);
        let a = g.attempt.as_ref().unwrap();
        assert_eq!(g.campaign.coins, coins);
        assert_eq!(a.corpses.iter().filter(|e| e.id == id).count(), 1);
        assert_eq!(a.corpses.iter().find(|e| e.id == id).unwrap(), &corpse);
        let encoded = serde_json::to_string(&g).unwrap();
        if let Some(expected) = &reference {
            assert_eq!(&encoded, expected);
        } else {
            reference = Some(encoded);
        }
    }
}

#[test]
fn conscious_zero_hp_remains_explicitly_critical_until_its_legal_ht_turn_then_reaps_once_at_1_2_4x()
{
    for batch in [1, 2, 4] {
        let (mut g, id) = fixture(18);
        let a = g.attempt.as_mut().unwrap();
        a.enemies[0].hp = 0;
        a.enemies[0].injury.hp = 0;
        a.ammo = 0;
        a.reload_left = 120;
        assert_eq!(a.enemies[0].condition(Mode::Npc), EnemyCondition::Critical);
        assert!(a.enemies[0].critically_wounded(Mode::Npc));
        let start = a.enemies[0].x;
        advance(&mut g, 59, batch);
        let a = g.attempt.as_ref().unwrap();
        assert!(a.enemies[0].x < start);
        assert_eq!(a.kills, 0);
        assert!(a.corpses.is_empty());
        assert!(!a.enemies[0].injury.terminal());
        advance(&mut g, 1, batch);
        let a = g.attempt.as_ref().unwrap();
        assert!(!a.enemies.iter().any(|e| e.id == id));
        assert_eq!(a.kills, 1);
        assert_eq!(a.corpses.iter().find(|e| e.id == id).unwrap().tick, 60);
        assert!(
            a.log
                .iter()
                .any(|line| line.contains("unconscious; removed"))
        );
    }
}
