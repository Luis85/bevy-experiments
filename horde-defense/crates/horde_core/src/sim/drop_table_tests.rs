//! W2-09 (design loot-economy §3 "Drops", plan LE-5): the weighted
//! single-draw DropTable of the `drop_tier` Some path. The weights here are
//! written out by hand (drop-tier steps 6 / 3 / 1 / 1, a trained Ranged
//! class x3, neutral slots), so a change to the loot constants fails them.

use super::*;
use crate::balance::BalanceSettings;
use crate::content::Slot;
use crate::loot::DropTable;
use crate::weapons::{WeaponClass, WeaponSpec};
use std::collections::BTreeSet;

/// A fresh unpaused attempt in `mode` whose defender cannot fall.
fn deployed(mode: Mode) -> (Game, Attempt) {
    let mut g = Game::default();
    g.campaign.legacy_features = true;
    g.campaign.balance_settings = BalanceSettings::legacy();
    g.campaign.attempts = 1;
    g.deploy(0, mode).unwrap();
    g.set_paused(false);
    let mut a = g.attempt.take().unwrap();
    assert!(a.drop_tier.is_some(), "a wave-2 deploy is on the Some path");
    a.max_hp = 1_000_000;
    a.hp = a.max_hp;
    a.spawn_credit = -1.0e9;
    (g, a)
}

/// The weight of catalogue entry `definition` at `drop_tier` with the
/// `trained` weapon classes, by hand: 0 outside the pool (index below 4,
/// NPC issue, tier 0 or above the drop tier), else the drop-tier step
/// times 3 for a trained Ranged class.
fn weight_by_hand(definition: usize, drop_tier: usize, trained: &BTreeSet<WeaponClass>) -> u64 {
    let item = &content().items[definition];
    if definition < 4 || item.npc_only || item.tier == 0 || item.tier > drop_tier {
        return 0;
    }
    let step = [6, 3, 1, 1][drop_tier - item.tier];
    let class = if item.slot == Slot::Ranged && trained.contains(&WeaponSpec::from_item(item).class)
    {
        3
    } else {
        1
    };
    step * class
}

/// The definition `roll` lands on, walking the hand weights cumulatively in
/// catalogue index order, and the total.
fn pick_by_hand(roll: u64, drop_tier: usize, trained: &BTreeSet<WeaponClass>) -> (usize, u64) {
    let weights: Vec<u64> = (0..content().items.len())
        .map(|n| weight_by_hand(n, drop_tier, trained))
        .collect();
    let total = weights.iter().sum();
    let mut end = 0;
    for (n, weight) in weights.into_iter().enumerate() {
        end += weight;
        if roll < end {
            return (n, total);
        }
    }
    panic!("roll {roll} is not below the total {total}");
}

/// The trained classes the table reads: the Personal skill set, or the NPC's
/// own weapon class.
fn trained_of(a: &Attempt) -> BTreeSet<WeaponClass> {
    match a.mode {
        Mode::Personal => a.profile.modifiers.weapon_classes.clone(),
        Mode::Npc => BTreeSet::from([a.profile.weapon.class]),
    }
}

/// The `skip`-th seeded combat RNG whose next `below(100)` hits the 3% roll.
fn hit_rng(skip: usize) -> Rng {
    (1..)
        .map(Rng::new)
        .filter(|rng| rng.clone().below(100) < 3)
        .nth(skip)
        .unwrap()
}

/// Defeats a fresh shambler in lane 5 with the combat RNG set to `rng` and
/// returns the definition it dropped, if any (the attempt's earlier
/// receipts are dropped first).
fn kill_with(g: &mut Game, a: &mut Attempt, rng: Rng) -> Option<usize> {
    let shambler = crate::enemy_catalog::enemy_catalog()
        .resolve("shambler")
        .unwrap();
    a.enemies.clear();
    g.spawn_pick(a, shambler, 5);
    let mode = a.mode;
    let e = a.enemies.last_mut().expect("placed");
    if mode == Mode::Personal {
        e.hp = 0;
    } else {
        e.injury.unconscious = true;
    }
    a.kills = 1;
    a.rng = rng;
    // The receipts are capped; only this kill's matter.
    a.loot_events.clear();
    g.reap(a);
    assert!(a.enemies.is_empty(), "the hostile was reaped");
    a.loot_events
        .last()
        .map(|event| event.definition.index().unwrap())
}

#[test]
fn weighted_drops_favour_current_tier() {
    let (mut g, mut a) = deployed(Mode::Personal);
    a.profile.modifiers.weapon_classes.clear();
    let none = BTreeSet::new();
    for drop_tier in 1..=4usize {
        a.drop_tier = Some(drop_tier as u8);
        let table = DropTable::for_attempt(&a);
        let mut by_tier = [0u64; 5];
        for (n, item) in content().items.iter().enumerate() {
            let weight = weight_by_hand(n, drop_tier, &none);
            assert_eq!(
                table.weight(n),
                weight,
                "drop tier {drop_tier}: {} (tier {})",
                item.name,
                item.tier
            );
            by_tier[item.tier] += weight;
        }
        assert_eq!(table.total(), by_tier.iter().sum::<u64>());
        for tier in 1..drop_tier {
            assert!(
                by_tier[drop_tier] > by_tier[tier],
                "drop tier {drop_tier}: its own tier outweighs tier {tier}: {by_tier:?}"
            );
        }
    }
    // At drop tier 4 the current tier takes 90 of 168 (53.6%), tier 1 18
    // (10.7%), against 15 / 63 (23.8%) and 18 / 63 under the uniform pick.
    a.drop_tier = Some(4);
    let mut drops = [0u32; 5];
    for n in 0..600 {
        let definition = kill_with(&mut g, &mut a, hit_rng(n)).expect("a 3% hit drops");
        drops[content().items[definition].tier] += 1;
    }
    assert!(
        drops[4] > 270 && drops[4] > 3 * drops[1],
        "tier-4 drops lead: {drops:?}"
    );
}

#[test]
fn trained_class_weighted() {
    let (_, mut a) = deployed(Mode::Personal);
    a.drop_tier = Some(4);
    let trained = BTreeSet::from([WeaponClass::Pistol, WeaponClass::Shotgun]);
    a.profile.modifiers.weapon_classes = trained.clone();
    let table = DropTable::for_attempt(&a);
    let (mut trained_ranged, mut untrained_ranged) = (0, 0);
    for (n, item) in content().items.iter().enumerate() {
        assert_eq!(
            table.weight(n),
            weight_by_hand(n, 4, &trained),
            "{}",
            item.name
        );
        if item.slot != Slot::Ranged || item.tier != 4 || table.weight(n) == 0 {
            continue;
        }
        if trained.contains(&WeaponSpec::from_item(item).class) {
            assert_eq!(
                table.weight(n),
                18,
                "{}: a trained tier-4 weapon",
                item.name
            );
            trained_ranged += 1;
        } else {
            assert_eq!(
                table.weight(n),
                6,
                "{}: an untrained tier-4 weapon",
                item.name
            );
            untrained_ranged += 1;
        }
    }
    assert!(trained_ranged >= 2 && untrained_ranged >= 3);
    // Melee, Outfit and Accessory never take the class weight.
    assert!(content().items.iter().enumerate().all(|(n, item)| {
        item.slot == Slot::Ranged || table.weight(n) == weight_by_hand(n, 4, &BTreeSet::new())
    }));
    // The table follows the attempt's frozen profile.
    a.profile.modifiers.weapon_classes.clear();
    assert!(DropTable::for_attempt(&a).total() < table.total());
}

#[test]
fn npc_mode_weights_its_own_weapon_class() {
    let (_, mut a) = deployed(Mode::Npc);
    a.drop_tier = Some(4);
    // The player's skill classes do not reach an NPC attempt's drops.
    a.profile.modifiers.weapon_classes = WeaponClass::ALL.into_iter().collect();
    for class in [WeaponClass::Shotgun, WeaponClass::MachineGun] {
        a.profile.weapon.class = class;
        let table = DropTable::for_attempt(&a);
        let own = BTreeSet::from([class]);
        for (n, item) in content().items.iter().enumerate() {
            assert_eq!(
                table.weight(n),
                weight_by_hand(n, 4, &own),
                "{class:?}: {}",
                item.name
            );
        }
        let boosted: BTreeSet<WeaponClass> = content()
            .items
            .iter()
            .enumerate()
            .filter(|(n, item)| item.slot == Slot::Ranged && table.weight(*n) == 18)
            .map(|(_, item)| WeaponSpec::from_item(item).class)
            .collect();
        assert_eq!(boosted, own, "only the NPC's own class is weighted");
    }
}

#[test]
fn definition_pick_consumes_exactly_one_draw() {
    for mode in [Mode::Personal, Mode::Npc] {
        let (mut g, mut a) = deployed(mode);
        a.profile.modifiers.weapon_classes = BTreeSet::from([WeaponClass::Rifle]);
        for drop_tier in 1..=4usize {
            a.drop_tier = Some(drop_tier as u8);
            let trained = trained_of(&a);
            let table = DropTable::for_attempt(&a);
            // Every roll below the total lands where the hand walk does.
            let (_, total) = pick_by_hand(0, drop_tier, &trained);
            assert_eq!(table.total(), total, "{mode:?} drop tier {drop_tier}");
            for roll in 0..total {
                assert_eq!(
                    table.pick(roll),
                    pick_by_hand(roll, drop_tier, &trained).0,
                    "{mode:?} drop tier {drop_tier} roll {roll}"
                );
            }
            // A normal drop draws the 3% roll and one below(total), no more.
            for n in 0..40 {
                let rng = hit_rng(n);
                let mut expected = rng;
                assert!(expected.below(100) < 3);
                let (definition, _) = pick_by_hand(expected.below(total), drop_tier, &trained);
                let label = format!("{mode:?} drop tier {drop_tier} rng {n}");
                assert_eq!(kill_with(&mut g, &mut a, rng), Some(definition), "{label}");
                assert_eq!(a.rng, expected, "{label}: exactly one pick draw");
            }
        }
    }
}
