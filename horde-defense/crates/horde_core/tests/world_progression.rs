#![allow(clippy::print_stdout, clippy::print_stderr)] // CLI example / test diagnostics
mod support;
use horde_core::{
    campaign::Campaign,
    content::{Mode, Policy, Slot, StructureKind, content},
    progression::{ProgressionCatalog, ProgressionSnapshot, catalog},
    sim::{Game, Input, Outcome, smoke_input, smoke_prepare},
    survivors::{Appearance, SurvivorId},
};
use support::legacy_game;

#[test]
fn survivors_are_cosmetic_and_legacy_saves_keep_the_original_identity() {
    let mut rook = Campaign::default();
    let mut june = rook.clone();
    assert_eq!(rook.selected_survivor, None);
    rook.choose_survivor(SurvivorId::Rook).unwrap();
    june.choose_survivor(SurvivorId::June).unwrap();
    let a = rook.profile(Mode::Personal);
    let b = june.profile(Mode::Personal);
    assert_eq!(a.appearance, Appearance::Male);
    assert_eq!(b.appearance, Appearance::Female);
    assert_eq!(a.personal, b.personal);
    assert_eq!(a.npc, b.npc);
    assert_eq!(rook.player.loadout, june.player.loadout);
    june.choose_start_country("USA").unwrap();
    let before = june.clone();
    assert!(june.choose_survivor(SurvivorId::Rook).is_err());
    assert_eq!(june, before);
    let mut old = serde_json::to_value(rook).unwrap();
    old.as_object_mut().unwrap().remove("selected_survivor");
    assert_eq!(
        serde_json::from_value::<Campaign>(old)
            .unwrap()
            .selected_survivor,
        Some(SurvivorId::Rook)
    );
}

#[test]
fn victory_threat_is_independent_of_losses_currency_and_clear_ledger_edits() {
    let mut c = Campaign::default();
    c.choose_start_country("USA").unwrap();
    let baseline = c.progression_rank();
    c.attempts = 999;
    c.coins = 999_999;
    assert_eq!(c.progression_rank(), baseline);
    for stage in 0..6 {
        c.world.record_clear(stage).unwrap();
        c.first_clear(stage % 5, Mode::Personal);
        assert_eq!(c.progression_rank(), stage as u32 + 1);
    }
    let rank = c.progression_rank();
    c.world.record_clear(0).unwrap();
    assert_eq!(c.progression_rank(), rank);
    for stage in 6..horde_core::world::world()
        .country("USA")
        .unwrap()
        .level_count()
    {
        c.world.record_clear(stage).unwrap();
    }
    let liberated_rank = c.progression_rank();
    c.world.reopen("USA").unwrap();
    assert_eq!(c.progression_rank(), liberated_rank);
    c.first_clear(0, Mode::Npc);
    assert_eq!(c.progression_rank(), rank + 1);
    let mut last = ProgressionSnapshot::default();
    for rank in 0..1000 {
        let p = catalog().snapshot(rank);
        p.validate().unwrap();
        assert!(
            p.rank >= last.rank
                && p.batch_size >= last.batch_size
                && p.item_level >= last.item_level
                && p.enemy_health >= last.enemy_health
                && p.arrival_multiplier >= last.arrival_multiplier
                && p.boss_health_floor >= last.boss_health_floor
        );
        last = p;
    }
    assert_eq!(last.rank, 999);
    assert_eq!(last.batch_size, catalog().max_batch);
    let mut bad = catalog().clone();
    bad.batch_rank_step = 0;
    assert!(bad.validate().is_err());
    let mut bad = catalog().clone();
    bad.gear.damage_per_level = f32::NAN;
    assert!(bad.validate().is_err());
    let mut bad = catalog().clone();
    bad.pressure_floors[2].boss_health = 1;
    assert!(bad.validate().is_err());
    assert_eq!(
        ProgressionCatalog::from_json(include_str!("../progression.json")).unwrap(),
        *catalog()
    );
}

#[test]
fn older_checkpoint_fields_default_without_resetting_actions_or_creating_levelled_gear() {
    let mut g = legacy_game();
    g.deploy(0, Mode::Personal).unwrap();
    g.set_paused(false);
    g.tick(Input {
        fire: true,
        ..Input::default()
    });
    let mut value = serde_json::to_value(&g).unwrap();
    value["campaign"]
        .as_object_mut()
        .unwrap()
        .remove("selected_survivor");
    value["campaign"]["world"]
        .as_object_mut()
        .unwrap()
        .remove("clear_high_water");
    for item in value["campaign"]["items"].as_array_mut().unwrap() {
        item.as_object_mut().unwrap().remove("level");
    }
    value["attempt"]
        .as_object_mut()
        .unwrap()
        .remove("progression");
    value["attempt"]["profile"]
        .as_object_mut()
        .unwrap()
        .remove("appearance");
    let payload = serde_json::to_string(&value).unwrap();
    let checksum = support::fnv1a(payload.as_bytes());
    let bytes =
        serde_json::to_vec(&serde_json::json!({"version":1,"checksum":checksum,"payload":payload}))
            .unwrap();
    let resumed = Game::decode_save_generation(&bytes).unwrap();
    assert_eq!(resumed.campaign().selected_survivor, Some(SurvivorId::Rook));
    assert!(resumed.campaign().items.iter().all(|item| item.level == 1));
    let a = resumed.attempt().unwrap();
    assert_eq!(a.progression, ProgressionSnapshot::default());
    assert_eq!(a.profile.appearance, Appearance::Male);
    assert_eq!(a.ammo, g.attempt().unwrap().ammo);
    assert_eq!(a.shot_cooldown, g.attempt().unwrap().shot_cooldown);
}

#[test]
fn item_levels_change_both_profiles_without_transferring_personal_skills_or_breaking_legacy_items()
{
    let mut c = Campaign::default();
    c.reward(65, 0, Mode::Personal);
    c.purchase_node("quick_reload").unwrap();
    c.purchase_node("rifle_training").unwrap();
    let one = c.add_item_at_level(4, 1).unwrap();
    let late = c.add_item_at_level(4, 9).unwrap();
    assert_eq!(c.effective_item(one).unwrap(), content().items[4]);
    c.equip(Mode::Personal, one).unwrap();
    c.equip(Mode::Npc, one).unwrap();
    let p1 = c.profile(Mode::Personal);
    let n1 = c.profile(Mode::Npc);
    c.equip(Mode::Personal, late).unwrap();
    c.equip(Mode::Npc, late).unwrap();
    let p9 = c.profile(Mode::Personal);
    let n9 = c.profile(Mode::Npc);
    assert!(p9.personal.damage > p1.personal.damage);
    assert!(n9.npc.damage_add > n1.npc.damage_add);
    let frozen = n9.clone();
    c.reward(20, 0, Mode::Personal);
    c.purchase_node("steady_hands").unwrap();
    let after = c.profile(Mode::Npc);
    assert_eq!(after.npc, frozen.npc);
    assert_eq!(after.stats, frozen.stats);
    assert_eq!(
        (
            after.max_hp,
            after.max_fp,
            after.ranged_skill,
            after.melee_skill,
            after.npc_melee_damage_dice,
            after.npc_melee_damage_add
        ),
        (
            frozen.max_hp,
            frozen.max_fp,
            frozen.ranged_skill,
            frozen.melee_skill,
            frozen.npc_melee_damage_dice,
            frozen.npc_melee_damage_add
        )
    );
    let mut old = serde_json::to_value(c.item(one).unwrap()).unwrap();
    old.as_object_mut().unwrap().remove("level");
    assert_eq!(
        serde_json::from_value::<horde_core::campaign::Item>(old)
            .unwrap()
            .level,
        1
    );
    let json = serde_json::to_string(&c).unwrap();
    let restored: Campaign = serde_json::from_str(&json).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored, c);
    let mut invalid = restored;
    invalid.items.last_mut().unwrap().level = 65;
    assert!(invalid.validate().is_err());
}

#[test]
fn grouped_spawns_and_frozen_scaling_survive_pause_and_checkpoint_without_catch_up() {
    let mut g = legacy_game();
    g.campaign_mut().choose_start_country("USA").unwrap();
    for stage in 0..6 {
        g.campaign_mut().world.record_clear(stage).unwrap();
        g.campaign_mut().first_clear(stage % 5, Mode::Personal);
    }
    g.deploy_country_stage(6, Mode::Personal).unwrap();
    g.set_paused(false);
    let snapshot = g.attempt().unwrap().progression.clone();
    assert_eq!(snapshot.rank, 6);
    assert_eq!(snapshot.batch_size, 3);
    assert_eq!(snapshot.item_level, 4);
    for _ in 0..1200 {
        g.tick(Input::default());
        if !g.attempt().unwrap().enemies.is_empty() {
            break;
        }
    }
    let a = g.attempt().unwrap();
    assert_eq!(a.enemies.len(), 3);
    assert!(
        a.enemies
            .windows(2)
            .all(|e| e[0].id < e[1].id && e[0].born_tick == e[1].born_tick)
    );
    assert!(a.enemies.iter().all(|e| e.lane < 11));
    g.set_paused(true);
    let frozen = serde_json::to_string(&g).unwrap();
    for _ in 0..120 {
        g.tick(Input::default());
    }
    assert_eq!(serde_json::to_string(&g).unwrap(), frozen);
    let path = std::env::temp_dir().join(format!(
        "lastlight-batch-progress-{}.json",
        std::process::id()
    ));
    g.save(&path).unwrap();
    let mut resumed = Game::load(&path).unwrap();
    g.set_paused(false);
    resumed.set_paused(false);
    for _ in 0..120 {
        g.tick(Input::default());
        resumed.tick(Input::default());
    }
    assert_eq!(
        serde_json::to_string(&g).unwrap(),
        serde_json::to_string(&resumed).unwrap()
    );
    assert_eq!(g.attempt().unwrap().progression, snapshot);
    let mut invalid = g;
    invalid.attempt_mut().unwrap().progression.batch_size = 0;
    assert!(invalid.save(&path).is_err());
    std::fs::remove_file(&path).unwrap();
    if path.with_extension("bak").exists() {
        std::fs::remove_file(path.with_extension("bak")).unwrap();
    }
}

#[test]
fn staged_maximum_crowd_uses_the_frozen_population_cap_and_all_enemies_are_visible() {
    let mut g = horde_core::sim::maximum_progression_render_fixture().unwrap();
    let a = g.attempt().unwrap();
    assert_eq!(a.progression.rank, catalog().max_rank);
    assert_eq!(a.lane_count, catalog().max_lanes);
    assert!(g.campaign().player.is_max_level());
    assert_eq!(a.enemy_cap(), catalog().population_global_cap);
    assert_eq!(a.enemies.len(), a.enemy_cap() + 1);
    assert!(a.enemies.iter().all(|e| e.in_view()));
    assert_eq!(a.progression.batch_size, catalog().max_batch);
    let frozen = serde_json::to_string(&g).unwrap();
    for _ in 0..120 {
        g.tick(Input::default());
    }
    assert_eq!(serde_json::to_string(&g).unwrap(), frozen);
}

#[test]
fn eight_real_country_defenses_are_achievable_with_earned_levelled_shop_gear_in_both_modes() {
    for mode in [Mode::Personal, Mode::Npc] {
        let mut g = Game::default();
        g.set_seed(2026);
        g.campaign_mut().attempts = 1;
        g.campaign_mut().choose_start_country("USA").unwrap();
        if mode == Mode::Npc {
            g.campaign_mut().legacy_features = true;
            g.campaign_mut().npc.policy = Policy::BossFocus;
        }
        let mut last_rate = 0.0;
        let mut last_boss = 0;
        for stage in 0..8 {
            let mut cleared = false;
            for retry in 0..6 {
                smoke_prepare(&mut g, mode).unwrap();
                g.deploy_country_stage(stage, mode).unwrap();
                let a = g.attempt().unwrap();
                assert_eq!(a.progression.rank, stage as u32);
                assert!(a.arrival_rate() >= last_rate);
                last_rate = a.arrival_rate();
                let boss_floor = a.progression.boss_health_floor;
                assert!(boss_floor >= last_boss);
                last_boss = boss_floor;
                if mode == Mode::Npc && g.campaign().blueprints[StructureKind::Trap] {
                    for lane in
                        horde_core::sim::construction_lane_order(g.attempt().unwrap().lane_count)
                    {
                        if g.attempt().unwrap().supply
                            >= content().structure(StructureKind::Trap).supply
                        {
                            g.place(StructureKind::Trap, lane, 0).unwrap();
                        }
                    }
                }
                g.set_paused(false);
                for _ in 0..60 * 600 {
                    if g.attempt().is_none() {
                        break;
                    }
                    let input = smoke_input(&g);
                    g.tick(input);
                }
                let report = g.last_result().expect("bounded earned encounter");
                eprintln!(
                    "{mode:?} country stage{} retry{} {:?} {:.1}s {} coins",
                    stage + 1,
                    retry + 1,
                    report.outcome,
                    report.elapsed,
                    report.coins
                );
                if report.outcome == Outcome::Victory {
                    cleared = true;
                    break;
                }
            }
            assert!(
                cleared,
                "{mode:?} country stage{} has no earned clear",
                stage + 1
            );
        }
        assert_eq!(g.campaign().world.completed("USA"), 8);
        assert_eq!(g.campaign().progression_rank(), 8);
        assert!(g.campaign().items.iter().any(|item| item.level >= 4));
        let equipped = g
            .campaign()
            .item(if mode == Mode::Personal {
                g.campaign().player.loadout[Slot::Ranged.index()]
            } else {
                g.campaign().npc.loadout[Slot::Ranged.index()]
            })
            .unwrap();
        assert!(equipped.level >= 4);
    }
}
