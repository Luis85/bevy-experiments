//! W3-23 (design-enemy-balance §1, §7, §8, §9; schema v7 §6): the pure core
//! reads the enemy UI hooks render. `Game::stage_preview` is the snapshot a
//! deploy of that stage would stamp (the front ramp and the boss rank
//! included), `Game::results_run` the settled report's run record (elite
//! kills and the boss time-to-kill), `Attempt::enemy_shot_protection` the
//! armor the defender's shots meet, and `Campaign::balance_rules_label` the
//! difficulty's name with a Custom's basis. None mutates, none draws.
use horde_core::balance::{BalancePreset, BalanceSettings};
use horde_core::command::Command;
use horde_core::content::{EnemyKind, Mode};
use horde_core::progression::BossRank;
use horde_core::sim::{DEFENSE_LINE, Game};
use horde_core::world::world;

const MODES: [Mode; 2] = [Mode::Personal, Mode::Npc];

/// A campaign started in `country` with its first `cleared` stages won.
fn started(country: &str, cleared: usize) -> Game {
    let mut g = Game::default();
    // Legacy campaigns field Mara without the companion skill path.
    g.campaign_mut().legacy_features = true;
    g.campaign_mut().attempts = 1;
    g.campaign_mut().choose_start_country(country).unwrap();
    for stage in 0..cleared {
        g.campaign_mut().record_country_stage_clear(stage).unwrap();
    }
    g
}

#[test]
fn stage_preview_is_the_snapshot_a_deploy_of_that_stage_stamps() {
    let deu = world().country("DEU").unwrap();
    let last = deu.level_count() - 1;
    assert!(last >= 2, "DEU has a front deep enough to ramp");
    for mode in MODES {
        for stage in [0, 1, last] {
            let mut g = started("DEU", stage);
            let before = serde_json::to_value(&g).unwrap();
            let preview = g.stage_preview("DEU", stage, mode).unwrap();
            assert_eq!(serde_json::to_value(&g).unwrap(), before, "pure read");
            assert_eq!(
                Some(preview.difficulty),
                g.effective_country_difficulty("DEU")
            );
            assert_eq!(Some(preview.boss_rank), g.stage_boss_rank("DEU", stage));
            assert_eq!(preview.boss_rank, preview.pressure.boss_rank);
            let rank_only = g.campaign().deployment_progression(mode).unwrap();
            if stage == 0 {
                assert_eq!(preview.pressure, rank_only, "stage 0 has no ramp");
            } else {
                assert!(
                    preview.pressure.front_ramp > 1.0
                        && preview.pressure.enemy_health > rank_only.enemy_health,
                    "{mode:?} stage {stage}: the front ramp is in the preview"
                );
            }
            // After the next victory: one rank higher, on the same stage.
            assert_eq!(preview.next.rank, preview.pressure.rank + 1);
            assert_eq!(preview.next.front_ramp, preview.pressure.front_ramp);
            assert_eq!(preview.next.boss_rank, preview.boss_rank);
            g.deploy_country_stage(stage, mode).unwrap();
            assert_eq!(
                g.attempt().unwrap().progression,
                preview.pressure,
                "{mode:?} stage {stage}: the preview is what deploy stamps"
            );
        }
    }
    let g = started("DEU", last);
    assert_eq!(
        g.stage_preview("DEU", last, Mode::Personal)
            .unwrap()
            .boss_rank,
        BossRank::Capstone
    );
    assert!(g.stage_preview("ZZZ", 0, Mode::Personal).is_err());
    assert!(
        g.stage_preview("DEU", deu.level_count(), Mode::Personal)
            .is_err()
    );
    // A country that is not the active front previews as well.
    assert_eq!(
        g.stage_preview("USA", 5, Mode::Personal).unwrap().boss_rank,
        BossRank::Regional
    );
}

#[test]
fn boss_rank_labels_are_the_player_facing_names() {
    assert_eq!(BossRank::Sector.label(), "Sector");
    assert_eq!(BossRank::Regional.label(), "Regional");
    assert_eq!(BossRank::Capstone.label(), "Capstone");
}

#[test]
fn results_run_is_the_settled_reports_run_record() {
    let mut g = started("DEU", 0);
    assert_eq!(g.results_run(), None, "no report, no run");
    g.deploy_country_stage(0, Mode::Personal).unwrap();
    assert_eq!(g.results_run(), None, "nothing settled yet");
    g.withdraw();
    // A second run, so the lookup is by attempt and not by position.
    g.deploy_country_stage(0, Mode::Personal).unwrap();
    g.withdraw();
    let before = serde_json::to_value(&g).unwrap();
    let report = g.last_result().unwrap().attempt_id;
    let run = g.results_run().unwrap();
    assert_eq!(run.attempt_id, report);
    assert_eq!(run.worker_id, None);
    assert_eq!(run.id, g.campaign().run_history.last().unwrap().id);
    assert_eq!(serde_json::to_value(&g).unwrap(), before, "pure read");
    // A report whose record is gone (a pruned history) has none.
    g.last_result_mut().as_mut().unwrap().attempt_id = u64::MAX;
    assert_eq!(g.results_run(), None);
}

#[test]
fn shot_protection_adds_ranged_armor_off_the_line() {
    for mode in MODES {
        let mut g = started("DEU", 0);
        g.deploy_country_stage(0, mode).unwrap();
        let mut a = g.attempt_slot_mut().take().unwrap();
        g.spawn_archetype(&mut a, "shield-bearer", 0).unwrap();
        g.spawn_enemy(&mut a, EnemyKind::Runner, 1, false);
        a.enemies[0].x = 7.0;
        a.enemies[1].x = 7.0;
        let profile = a.enemies[0].resolved.clone().unwrap();
        let ranged = if mode == Mode::Personal {
            profile.ranged_armor
        } else {
            profile.npc_ranged_armor
        };
        assert!(ranged > 0, "{mode:?}: the shield-bearer carries a shield");
        let shield = &a.enemies[0];
        assert_eq!(
            a.enemy_shot_protection(shield),
            a.enemy_protection(shield) + ranged,
            "{mode:?}: shots meet the shield"
        );
        let runner = &a.enemies[1];
        assert_eq!(
            a.enemy_shot_protection(runner),
            a.enemy_protection(runner),
            "{mode:?}: no shield, no difference"
        );
        // At the line the shield is down (`Enemy::contact`).
        a.enemies[0].x = DEFENSE_LINE;
        let shield = &a.enemies[0];
        assert_eq!(
            a.enemy_shot_protection(shield),
            a.enemy_protection(shield),
            "{mode:?}: in contact"
        );
    }
}

#[test]
fn balance_rules_label_names_a_customs_basis() {
    let mut g = Game::default();
    for preset in [
        BalancePreset::Easy,
        BalancePreset::Normal,
        BalancePreset::Hard,
    ] {
        g.apply(&Command::SetBalance(BalanceSettings::for_preset(preset)))
            .unwrap();
        assert_eq!(g.campaign().balance_rules_label(), preset.name());
        // An edit of a named preset is Custom on that preset's rules.
        let mut edited = BalanceSettings::for_preset(preset);
        edited.enemy_speed += 0.25;
        g.apply(&Command::SetBalance(edited)).unwrap();
        assert_eq!(g.campaign().balance_settings.preset, BalancePreset::Custom);
        assert_eq!(g.campaign().balance_extras_preset(), preset);
        assert_eq!(
            g.campaign().balance_rules_label(),
            format!("Custom ({} rules)", preset.name())
        );
    }
    assert_eq!(g.campaign().balance_rules_label(), "Custom (Hard rules)");
}
