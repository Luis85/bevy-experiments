mod support;
use horde_core::{
    campaign::Campaign,
    content::{EnemyKind, Mode, content},
    quality::{Quality, QualityCatalog, catalog},
    rules::Rng,
    sim::{Game, Input},
};
use support::legacy_game;

#[test]
fn six_quality_tiers_validate_palette_weights_and_strictly_increasing_multipliers() {
    let good = catalog().clone();
    good.validate().unwrap();
    assert_eq!(good.tiers.len(), 6);
    assert_eq!(
        QualityCatalog::from_json(include_str!("../quality.json")).unwrap(),
        good
    );
    let mut bad = good.clone();
    bad.tiers.pop();
    assert!(bad.validate().is_err());
    let mut bad = good.clone();
    bad.tiers[1].quality = Quality::Poor;
    assert!(bad.validate().is_err());
    let mut bad = good.clone();
    bad.tiers[0].color[0] = f32::NAN;
    assert!(bad.validate().is_err());
    let mut bad = good.clone();
    bad.tiers[0].weight = 1_000_001;
    assert!(bad.validate().is_err());
    let mut bad = good.clone();
    bad.tiers[4].stat_multiplier = 0.0;
    assert!(bad.validate().is_err());
    let mut bad = good.clone();
    bad.tiers[4].value_multiplier = 1.0;
    assert!(bad.validate().is_err());
    let mut bad = good.clone();
    for tier in &mut bad.tiers {
        tier.min_rank = 1;
    }
    assert!(bad.validate().is_err());
    for quality in Quality::ALL {
        assert!(!quality.name().is_empty());
        assert!(quality.color().iter().all(|v| v.is_finite()));
    }
}

#[test]
fn legacy_none_and_explicit_base_quality_preserve_every_levelled_definition_number() {
    let mut campaign = Campaign::default();
    for (definition, base) in content().items.iter().enumerate() {
        for level in [1, 7] {
            let id = campaign.add_item_at_level(definition, level).unwrap();
            let expected = horde_core::progression::scale_item(base.clone(), level);
            assert_eq!(campaign.effective_item(id).unwrap(), expected);
            let base_sale = campaign.item_sale_value(id).unwrap();
            let explicit = campaign
                .add_item_with_quality(definition, level, Some(base.base_quality))
                .unwrap();
            assert_eq!(campaign.effective_item(explicit).unwrap(), expected);
            assert_eq!(campaign.item_sale_value(explicit).unwrap(), base_sale);
            let mut old = serde_json::to_value(campaign.item(id).unwrap()).unwrap();
            old.as_object_mut().unwrap().remove("quality");
            assert_eq!(
                serde_json::from_value::<horde_core::campaign::Item>(old)
                    .unwrap()
                    .quality,
                None
            );
        }
    }
}

#[test]
fn rolled_quality_improves_actual_personal_npc_stats_and_sale_value_but_keeps_pistol_cap_and_roles()
{
    let mut g = legacy_game();
    let definition = content()
        .items
        .iter()
        .position(|item| item.name == "Compact service pistol")
        .unwrap();
    let common = g
        .campaign
        .add_item_with_quality(definition, 9, Some(Quality::Common))
        .unwrap();
    let legendary = g
        .campaign
        .add_item_with_quality(definition, 9, Some(Quality::Legendary))
        .unwrap();
    let before = g.campaign.effective_item(common).unwrap();
    let after = g.campaign.effective_item(legendary).unwrap();
    assert!(after.personal.damage > before.personal.damage);
    assert!(after.npc.damage_add > before.npc.damage_add);
    assert!(
        g.campaign.item_sale_value(legendary).unwrap()
            > g.campaign.item_sale_value(common).unwrap()
    );
    g.campaign.equip(Mode::Personal, legendary).unwrap();
    g.deploy(0, Mode::Personal).unwrap();
    let mut a = g.attempt.take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
    a.enemies[0].x = 10.0;
    let hp = a.enemies[0].hp;
    g.attempt = Some(a);
    g.tick(Input {
        fire: true,
        aim: [10.0, 5.5],
        ..Input::default()
    });
    let a = g.attempt.as_ref().unwrap();
    assert!(a.enemies[0].hp > 0);
    assert!(a.enemies[0].hp < hp);
    g.withdraw();
    let npc_issued = g
        .campaign
        .add_item_with_quality(0, 9, Some(Quality::Legendary))
        .unwrap();
    assert!(g.campaign.equip(Mode::Personal, npc_issued).is_err());
}

fn guaranteed_drop_game() -> Game {
    let mut g = legacy_game();
    g.deploy(0, Mode::Personal).unwrap();
    let mut a = g.attempt.take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
    a.enemies[0].x = 10.0;
    a.enemies[0].hp = 0;
    let seed = (1..10000)
        .find(|seed| Rng::new(*seed).below(100) < 3)
        .unwrap();
    a.rng = Rng::new(seed);
    g.attempt = Some(a);
    g
}

#[test]
fn drop_quality_uses_a_purpose_counter_without_extra_combat_rng_and_checkpoint_rewards_repeat_once()
{
    let mut g = guaranteed_drop_game();
    let a = g.attempt.as_ref().unwrap();
    let enemy = a.enemies[0].id;
    let expected_quality = catalog().roll(g.seed, a.id, enemy, a.progression.rank);
    let mut expected_rng = a.rng;
    assert!(expected_rng.below(100) < 3);
    let available = content()
        .items
        .iter()
        .enumerate()
        .filter(|(n, item)| *n >= 4 && item.tier > 0 && item.tier <= 1)
        .count();
    expected_rng.below(available as u64);
    g.tick(Input::default());
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.rng.state, expected_rng.state);
    assert_eq!(a.loot, 1);
    assert_eq!(a.loot_events.len(), 1);
    let receipt = &a.loot_events[0];
    assert_eq!(receipt.quality, expected_quality);
    assert_eq!(
        g.campaign.item_quality(receipt.item_id).unwrap(),
        expected_quality
    );
    let path = std::env::temp_dir().join(format!(
        "lastlight-quality-checkpoint-{}.json",
        std::process::id()
    ));
    g.save(&path).unwrap();
    let mut resumed = Game::load(&path).unwrap();
    let coins = g.campaign.coins;
    let items = g.campaign.items.len();
    for _ in 0..120 {
        g.tick(Input::default());
        resumed.tick(Input::default());
    }
    assert_eq!(
        serde_json::to_string(&g).unwrap(),
        serde_json::to_string(&resumed).unwrap()
    );
    assert_eq!(g.campaign.coins, coins);
    assert_eq!(g.campaign.items.len(), items);
    let mut bad = g.clone();
    bad.attempt.as_mut().unwrap().loot_events[0].level = 0;
    assert!(bad.save(&path).is_err());
    g.withdraw();
    assert_eq!(g.last_result.as_ref().unwrap().loot_events.len(), 1);
    let frozen = g.campaign.items.clone();
    g.withdraw();
    assert_eq!(g.campaign.items, frozen);
    std::fs::remove_file(&path).unwrap();
    if path.with_extension("bak").exists() {
        std::fs::remove_file(path.with_extension("bak")).unwrap();
    }
}

#[test]
fn counter_quality_is_reproducible_and_parallel_settlement_preserves_quality_level_and_identity_once()
 {
    for rank in [0, 5, 50] {
        for enemy in 1..1000 {
            assert_eq!(
                catalog().roll(2026, 7, enemy, rank),
                catalog().roll(2026, 7, enemy, rank)
            );
        }
    }
    let mut g = legacy_game();
    g.campaign.choose_start_country("USA").unwrap();
    g.campaign.attempts = 1;
    g.campaign.coins = 10_000;
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
    g.deploy_survivor(recruit, country, 0).unwrap();
    let count = g.campaign.items.len();
    g.battles[0]
        .game
        .campaign
        .add_item_with_quality(4, 9, Some(Quality::Epic))
        .unwrap();
    g.tick(Input::default());
    assert_eq!(g.campaign.items.len(), count + 1);
    let item = g.campaign.items.last().unwrap();
    assert_eq!(
        (item.definition_index(), item.level, item.quality),
        (4, 9, Some(Quality::Epic))
    );
    let id = item.id;
    for _ in 0..10 {
        g.tick(Input::default());
    }
    assert_eq!(g.campaign.items.len(), count + 1);
    assert_eq!(g.campaign.item(id).unwrap().quality, Some(Quality::Epic));
}
