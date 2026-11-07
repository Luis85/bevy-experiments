mod support;
use horde_core::{
    campaign::Campaign,
    content::{EnemyKind, ItemDefId, Mode, content},
    loot::{ELITE_QUALITY_FLOOR, drop_quality},
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
        .campaign_mut()
        .add_item_with_quality(definition, 9, Some(Quality::Common))
        .unwrap();
    let legendary = g
        .campaign_mut()
        .add_item_with_quality(definition, 9, Some(Quality::Legendary))
        .unwrap();
    let before = g.campaign().effective_item(common).unwrap();
    let after = g.campaign().effective_item(legendary).unwrap();
    assert!(after.personal.damage > before.personal.damage);
    assert!(after.npc.damage_add > before.npc.damage_add);
    assert!(
        g.campaign().item_sale_value(legendary).unwrap()
            > g.campaign().item_sale_value(common).unwrap()
    );
    g.campaign_mut().equip(Mode::Personal, legendary).unwrap();
    g.deploy(0, Mode::Personal).unwrap();
    let mut a = g.attempt_slot_mut().take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
    a.enemies[0].x = 10.0;
    let hp = a.enemies[0].hp;
    *g.attempt_slot_mut() = Some(a);
    g.tick(Input {
        fire: true,
        aim: [10.0, 5.5],
        ..Input::default()
    });
    let a = g.attempt().unwrap();
    assert!(a.enemies[0].hp > 0);
    assert!(a.enemies[0].hp < hp);
    g.withdraw();
    let npc_issued = g
        .campaign_mut()
        .add_item_with_quality(0, 9, Some(Quality::Legendary))
        .unwrap();
    assert!(g.campaign_mut().equip(Mode::Personal, npc_issued).is_err());
}

fn guaranteed_drop_game() -> Game {
    let seed = (1..10000)
        .find(|seed| Rng::new(*seed).below(100) < 3)
        .unwrap();
    guaranteed_drop_game_with(seed)
}

/// A fresh level-0 Personal attempt (so `drop_tier` is Some) whose only
/// hostile dies on the next tick, with the combat RNG reseeded to `rng_seed`.
fn guaranteed_drop_game_with(rng_seed: u64) -> Game {
    let mut g = legacy_game();
    g.deploy(0, Mode::Personal).unwrap();
    let mut a = g.attempt_slot_mut().take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
    a.enemies[0].x = 10.0;
    a.enemies[0].hp = 0;
    a.rng = Rng::new(rng_seed);
    *g.attempt_slot_mut() = Some(a);
    g
}

/// Today's level-0 drop pool (catalogue index 4 on, tier 1), in index order.
fn tier_one_pool() -> Vec<usize> {
    content()
        .items
        .iter()
        .enumerate()
        .filter(|(n, item)| *n >= 4 && item.tier > 0 && item.tier <= 1)
        .map(|(n, _)| n)
        .collect()
}

/// The relative outcome by hand: the absolute tier read as a step from
/// Common, added to `base` and clamped to Poor..=Legendary.
fn shifted_by_hand(base: Quality, absolute: Quality) -> Quality {
    let index = (base as i32 + absolute as i32 - Quality::Common as i32).clamp(0, 5);
    Quality::ALL[index as usize]
}

#[test]
fn relative_roll_never_drops_a_tier4_below_epic() {
    // Decision 7 / LE-3: the absolute roll downgrades most tier-4 drops
    // (Legendary base) to Common; read relative to the base, the worst step
    // (Poor, -1) still leaves an Epic.
    let tier4: Vec<_> = content()
        .items
        .iter()
        .filter(|item| item.tier == 4)
        .collect();
    assert!(!tier4.is_empty());
    let mut absolute_below_epic = 0u32;
    for item in &tier4 {
        for rank in [0, 1, 2, 3, 5, 10, 23, 50, 1000] {
            for enemy in 1..=1500 {
                for elite in [false, true] {
                    let quality = drop_quality(item.base_quality, 2026, 7, enemy, rank, elite);
                    assert!(
                        quality >= Quality::Epic,
                        "{} rank {rank} enemy {enemy} elite {elite}: {quality:?}",
                        item.name
                    );
                }
                let relative =
                    catalog().roll_relative(item.base_quality, None, 0xA11CE, enemy, 3, rank);
                assert!(relative >= Quality::Epic, "{} rank {rank}", item.name);
                if catalog().roll(2026, 7, enemy, rank) < Quality::Epic {
                    absolute_below_epic += 1;
                }
            }
        }
    }
    assert!(
        absolute_below_epic > 0,
        "the absolute roll does downgrade tier-4 drops"
    );
}

#[test]
fn floor_zeroing_distribution_matches_table() {
    const SAMPLES: u64 = 200_000;
    let shares = |base: Quality, floor: Option<Quality>, rank: u32| {
        let mut counts = [0u64; 6];
        for enemy in 0..SAMPLES {
            counts[catalog().roll_relative(base, floor, 0x5EED_2026, 11, enemy, rank) as usize] +=
                1;
        }
        counts
    };
    let percent = |count: u64| count as f64 * 100.0 / SAMPLES as f64;
    // design-loot-economy §4, measured by B at rank 3 with the Uncommon
    // floor: tiers whose outcome lies below the floor weigh 0 and the rest
    // renormalise (no pile-up on the floor, as max() would give).
    let table: [(Quality, [f64; 6]); 2] = [
        (Quality::Common, [0.0, 0.0, 80.1, 17.0, 2.8, 0.0]),
        (Quality::Uncommon, [0.0, 0.0, 69.7, 24.2, 5.2, 0.9]),
    ];
    for (base, expected) in table {
        let counts = shares(base, Some(Quality::Uncommon), 3);
        assert_eq!(
            (counts[0], counts[1]),
            (0, 0),
            "{base:?}: nothing below the floor"
        );
        for (tier, want) in Quality::ALL.iter().zip(expected) {
            let got = percent(counts[*tier as usize]);
            assert!(
                (got - want).abs() < 0.5,
                "{base:?} base, {tier:?}: {got:.2}% against the table's {want}%"
            );
        }
    }
    // When every tier at or above the floor weighs 0 the result is
    // max(base, floor): at rank 0 no step reaches Legendary from Common
    // (Legendary needs rank 5) or from Poor.
    for (base, floor) in [
        (Quality::Common, Quality::Legendary),
        (Quality::Poor, Quality::Legendary),
    ] {
        for enemy in 0..64 {
            assert_eq!(
                catalog().roll_relative(base, Some(floor), 9, 9, enemy, 0),
                base.max(floor)
            );
        }
    }
    // The elite flag selects exactly this floor.
    assert_eq!(ELITE_QUALITY_FLOOR, Quality::Uncommon);
    for enemy in 0..2000 {
        for base in Quality::ALL {
            assert_eq!(
                drop_quality(base, 41, 5, enemy, 3, true),
                catalog().roll_relative(base, Some(ELITE_QUALITY_FLOOR), 41, 5, enemy, 3)
            );
            assert_eq!(
                drop_quality(base, 41, 5, enemy, 3, false),
                catalog().roll_relative(base, None, 41, 5, enemy, 3)
            );
        }
    }
}

#[test]
fn roll_relative_with_common_base_and_no_floor_equals_roll() {
    let mut tuples = Rng::new(0x5245_4C41_5449_5645);
    for n in 0..20_000u32 {
        let seed = tuples.next_u64();
        let attempt = tuples.next_u64() % 4096;
        let enemy = tuples.next_u64() % 4096;
        let rank = if n % 100 == 99 { 1000 + n % 7 } else { n % 41 };
        assert_eq!(
            catalog().roll_relative(Quality::Common, None, seed, attempt, enemy, rank),
            catalog().roll(seed, attempt, enemy, rank),
            "tuple {n}"
        );
    }
}

#[test]
fn drop_quality_consumes_no_combat_rng() {
    // A wave-2 (drop_tier Some) drop of an Uncommon-base tier-1 item, so the
    // relative outcome differs from the absolute roll.
    let pool = tier_one_pool();
    let rng_seed = (1..100_000u64)
        .find(|seed| {
            let mut rng = Rng::new(*seed);
            rng.below(100) < 3
                && content().items[pool[rng.below(pool.len() as u64) as usize]].base_quality
                    == Quality::Uncommon
        })
        .unwrap();
    let mut g = guaranteed_drop_game_with(rng_seed);
    let a = g.attempt().unwrap();
    assert!(a.drop_tier.is_some());
    let (seed, attempt, enemy, rank) = (g.seed(), a.id, a.enemies[0].id, a.progression.rank);
    let mut expected_rng = a.rng;
    assert!(expected_rng.below(100) < 3);
    let definition = pool[expected_rng.below(pool.len() as u64) as usize];
    let base = content().items[definition].base_quality;
    let expected = shifted_by_hand(base, catalog().roll(seed, attempt, enemy, rank));
    assert_eq!(
        drop_quality(base, seed, attempt, enemy, rank, false),
        expected
    );
    g.tick(Input::default());
    let a = g.attempt().unwrap();
    assert_eq!(
        a.rng.state, expected_rng.state,
        "the kill draws the drop chance and the pick, and nothing for its quality"
    );
    assert_eq!(a.loot_events.len(), 1);
    let receipt = &a.loot_events[0];
    assert_eq!(receipt.definition, ItemDefId::at(definition));
    assert_eq!(receipt.quality, expected, "{base:?} base");
    assert_eq!(
        g.campaign().item_quality(receipt.item_id).unwrap(),
        expected
    );
}

#[test]
fn drop_quality_uses_a_purpose_counter_without_extra_combat_rng_and_checkpoint_rewards_repeat_once()
{
    let mut g = guaranteed_drop_game();
    let a = g.attempt().unwrap();
    let enemy = a.enemies[0].id;
    let mut expected_rng = a.rng;
    assert!(expected_rng.below(100) < 3);
    let pool = tier_one_pool();
    let definition = pool[expected_rng.below(pool.len() as u64) as usize];
    // W2-07: a fresh deploy takes the drop_tier Some path, whose quality is
    // the relative roll on the picked item's base (decision 7), not the
    // absolute roll this test pinned before.
    let expected_quality = drop_quality(
        content().items[definition].base_quality,
        g.seed(),
        a.id,
        enemy,
        a.progression.rank,
        false,
    );
    g.tick(Input::default());
    let a = g.attempt().unwrap();
    assert_eq!(a.rng.state, expected_rng.state);
    assert_eq!(a.loot, 1);
    assert_eq!(a.loot_events.len(), 1);
    let receipt = &a.loot_events[0];
    assert_eq!(receipt.quality, expected_quality);
    assert_eq!(
        g.campaign().item_quality(receipt.item_id).unwrap(),
        expected_quality
    );
    let path = std::env::temp_dir().join(format!(
        "lastlight-quality-checkpoint-{}.json",
        std::process::id()
    ));
    g.save(&path).unwrap();
    let mut resumed = Game::load(&path).unwrap();
    let coins = g.campaign().coins;
    let items = g.campaign().items.len();
    for _ in 0..120 {
        g.tick(Input::default());
        resumed.tick(Input::default());
    }
    assert_eq!(
        serde_json::to_string(&g).unwrap(),
        serde_json::to_string(&resumed).unwrap()
    );
    assert_eq!(g.campaign().coins, coins);
    assert_eq!(g.campaign().items.len(), items);
    let mut bad = g.clone();
    bad.attempt_mut().unwrap().loot_events[0].level = 0;
    assert!(bad.save(&path).is_err());
    g.withdraw();
    assert_eq!(g.last_result().unwrap().loot_events.len(), 1);
    let frozen = g.campaign().items.clone();
    g.withdraw();
    assert_eq!(g.campaign().items, frozen);
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
    g.campaign_mut().choose_start_country("USA").unwrap();
    g.campaign_mut().attempts = 1;
    g.campaign_mut().coins = 10_000;
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
    let recruit = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, country))
        .unwrap();
    g.deploy_survivor(recruit, country, 0).unwrap();
    let count = g.campaign().items.len();
    g.battles_mut()[0]
        .game_mut()
        .campaign_mut()
        .add_item_with_quality(4, 9, Some(Quality::Epic))
        .unwrap();
    g.tick(Input::default());
    assert_eq!(g.campaign().items.len(), count + 1);
    let item = g.campaign().items.last().unwrap();
    assert_eq!(
        (item.definition_index(), item.level, item.quality),
        (4, 9, Some(Quality::Epic))
    );
    let id = item.id;
    for _ in 0..10 {
        g.tick(Input::default());
    }
    assert_eq!(g.campaign().items.len(), count + 1);
    assert_eq!(g.campaign().item(id).unwrap().quality, Some(Quality::Epic));
}
