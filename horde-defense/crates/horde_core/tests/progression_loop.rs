#![allow(clippy::print_stdout, clippy::print_stderr)] // CLI example / test diagnostics
//! Earned-reward progression proof. The controller reads only current enemies;
//! it receives no free purchases, loadout drops or authoritatively altered hits.
mod support;
use horde_core::{
    content::{EnemyKind, Mode, Slot, content},
    sim::{Game, Input, Outcome},
    weapons::WeaponClass,
};
use support::country_game;

fn fight(game: &mut Game) -> bool {
    game.deploy_country_stage(0, Mode::Personal).unwrap();
    game.set_paused(false);
    let mut injured_boss = false;
    for _ in 0..60 * 600 {
        let Some(attempt) = game.attempt() else {
            return injured_boss;
        };
        injured_boss |= attempt.enemies.iter().any(|e| e.boss && e.hp < e.max_hp);
        let target = attempt
            .enemies
            .iter()
            .filter(|e| e.in_view())
            .min_by(|a, b| a.x.total_cmp(&b.x).then(a.id.cmp(&b.id)));
        let input = target.map_or_else(Input::default, |target| Input {
            aim: [target.x, target.y()],
            fire: true,
            melee: target.contact() && attempt.stamina >= 25.0,
            ..Input::default()
        });
        game.tick(input);
    }
    panic!("First-level fight exceeded ten simulated minutes");
}

fn buy_weapon(game: &mut Game, name: &str) {
    let definition = content()
        .items
        .iter()
        .position(|item| item.name == name)
        .unwrap();
    let id = game.campaign_mut().buy_item(definition).unwrap();
    game.campaign_mut().equip(Mode::Personal, id).unwrap();
}

#[test]
fn normal_short_arena_earned_route_clears_within_five_runs_without_equipping_loot() {
    for seed in [1, 2026, 0x1A571167] {
        for country in ["DEU", "BRA", "EGY", "USA", "ISL"] {
            let mut game = country_game(country);
            game.set_seed(seed);
            assert_eq!(game.campaign().coins, 0);
            assert_eq!(
                game.campaign().profile(Mode::Personal).weapon.class,
                WeaponClass::Pistol
            );
            let mut clear_run = None;
            for run in 1..=5 {
                match run {
                    2 => {
                        buy_weapon(&mut game, "Compact service pistol");
                        game.campaign_mut().purchase_node("quick_reload").unwrap();
                    }
                    3 => {
                        game.campaign_mut().purchase_node("steady_hands").unwrap();
                        game.campaign_mut().purchase_node("smg_training").unwrap();
                        buy_weapon(&mut game, "Compact SMG");
                    }
                    4 => {
                        game.campaign_mut().purchase_node("stopping_power").unwrap();
                        game.campaign_mut().purchase_node("rapid_cycle").unwrap();
                    }
                    5 => {
                        game.campaign_mut().purchase_node("rifle_training").unwrap();
                        buy_weapon(&mut game, "Marksman rifle");
                    }
                    _ => {}
                }
                let banked_before = game.campaign().coins;
                let weapon_before = game.campaign().player.loadout[Slot::Ranged.index()];
                let injured_boss = fight(&mut game);
                let report = game.last_result().unwrap();
                eprintln!(
                    "country={country} seed={seed} run={run} {:?} elapsed={:.1} kills={} earned={} wallet={} profile={:?}",
                    report.outcome,
                    report.elapsed,
                    report.kills,
                    report.coins,
                    game.campaign().coins,
                    game.campaign().equipped(Mode::Personal, Slot::Ranged).name
                );
                if run == 1 {
                    assert_eq!(
                        report.outcome,
                        Outcome::Defeat,
                        "Normal fresh first run must require earned growth"
                    );
                }
                assert!(
                    report.kills >= 20,
                    "First-level earnings must come from real hostile defeats"
                );
                assert!(
                    report.coins >= 60,
                    "Every attempt must bank a meaningful guaranteed purchase budget"
                );
                assert_eq!(
                    game.campaign().coins,
                    banked_before + report.coins,
                    "Rewards bank once and survive defeat"
                );
                assert_eq!(
                    game.campaign().player.loadout[Slot::Ranged.index()],
                    weapon_before,
                    "Dropped weapons never enter this progression proof"
                );
                if report.outcome == Outcome::Victory {
                    assert!(
                        injured_boss,
                        "The purchased upgrade must make progress against the boss"
                    );
                }
                if run == 1 {
                    assert!(
                        (20.0..300.0).contains(&report.elapsed),
                        "Short opening should produce meaningful combat before retreat"
                    );
                }
                if report.outcome == Outcome::Victory {
                    clear_run = Some(run);
                    break;
                }
            }
            assert!(
                matches!(clear_run, Some(3..=5)),
                "country {country} seed {seed}: earned clear {clear_run:?}"
            );
        }
    }
}

#[test]
fn fresh_country_pistols_can_injure_the_boss_during_authored_exposure() {
    for country in ["DEU", "BRA", "EGY", "USA", "ISL"] {
        let mut game = country_game(country);
        game.deploy_country_stage(0, Mode::Personal).unwrap();
        let mut attempt = game.attempt_slot_mut().take().unwrap();
        let lane = attempt.lane_count / 2;
        game.spawn_enemy(&mut attempt, EnemyKind::Brute, lane, true);
        let boss = attempt.enemies.last_mut().unwrap();
        boss.x = attempt.width_yards - 2.0;
        attempt.tick = boss.born_tick + (boss.boss_timing.exposure_delay * 60.0).ceil() as u64;
        let health = boss.hp;
        let aim = [boss.x, boss.y()];
        attempt.boss_spawned = true;
        *game.attempt_slot_mut() = Some(attempt);
        game.set_paused(false);
        game.tick(Input {
            aim,
            fire: true,
            ..Input::default()
        });
        assert!(
            game.attempt().unwrap().enemies[0].hp < health,
            "country {country}: no mandatory rare-weapon dependency"
        );
    }
}
