//! W2-24 (design progression-tree §5 "Folding", ruling E12): the skill-perk
//! mechanisms at neutral authored values. Every test injects the value it
//! needs, so the mechanism is pinned apart from the shipped tree. Since W2-41
//! nodes 170-177 author class perks, `shield_breaker`'s penetration and
//! `blast_discipline`'s hazard multiplier, and since W4-03 the shrapnel trap
//! authors one charge per mastery rank.

use super::*;
use crate::content::StructureDefinition;
use crate::skill_catalog::{ClassModifiers, SkillModifiers};
use crate::weapons::WeaponClass;

fn legacy_game() -> Game {
    let mut game = Game::default();
    game.campaign.legacy_features = true;
    game.campaign.balance_settings = crate::balance::BalanceSettings::legacy();
    game
}

/// A legacy game that owns the trap blueprint.
fn trap_game() -> Game {
    let mut g = legacy_game();
    for level in 0..3 {
        g.campaign.first_clear(level, Mode::Personal);
    }
    g.campaign.reward(1000, 0, Mode::Personal);
    g.campaign.purchase_blueprint(StructureKind::Trap).unwrap();
    g
}

fn profile_with(campaign: &Campaign, mode: Mode, modifiers: SkillModifiers) -> DefenderProfile {
    let loadout = match mode {
        Mode::Personal => campaign.player.loadout,
        Mode::Npc => campaign.npc.loadout,
    };
    campaign.profile_with_modifiers(mode, loadout, &campaign.npc, modifiers)
}

#[test]
fn class_modifier_applies_only_to_matching_class() {
    let game = Game::default();
    let campaign = &game.campaign;
    let base = profile_with(campaign, Mode::Personal, SkillModifiers::default());
    assert_eq!(
        base.personal,
        campaign.profile(Mode::Personal).personal,
        "the seam is the shipped profile when nothing is injected"
    );
    let class = base.weapon.class;
    let perk = ClassModifiers {
        damage_multiplier: 1.5,
        reload_multiplier: 0.8,
        cadence_multiplier: 0.8,
        weak_bonus_add: 0.15,
    };
    let with_perk = |perk_class: WeaponClass| {
        let mut modifiers = SkillModifiers::default();
        modifiers.class_modifiers.insert(perk_class, perk);
        modifiers
    };
    for other in WeaponClass::ALL.into_iter().filter(|c| *c != class) {
        let p = profile_with(campaign, Mode::Personal, with_perk(other));
        assert_eq!(p.personal, base.personal, "{other:?} perk on a {class:?}");
        assert_eq!(p.npc, base.npc, "{other:?} perk on a {class:?}");
    }
    let p = profile_with(campaign, Mode::Personal, with_perk(class));
    assert_eq!(
        p.personal.damage,
        (base.personal.damage as f32 * 1.5).round() as i32
    );
    assert_eq!(p.personal.reload, (base.personal.reload * 0.8).max(0.65));
    assert_eq!(p.personal.cadence, (base.personal.cadence * 0.8).max(0.08));
    assert_eq!(
        p.personal.weak_bonus,
        (base.personal.weak_bonus + 0.15).min(1.0)
    );
    assert_ne!(p.personal, base.personal);
    // A neutral perk entry folds to the same numbers as no entry.
    let mut neutral = SkillModifiers::default();
    neutral
        .class_modifiers
        .insert(class, ClassModifiers::default());
    assert_eq!(
        profile_with(campaign, Mode::Personal, neutral).personal,
        base.personal
    );
    // Personal only: an NPC profile ignores a perk on its own class.
    let npc_base = profile_with(campaign, Mode::Npc, SkillModifiers::default());
    let npc = profile_with(campaign, Mode::Npc, with_perk(npc_base.weapon.class));
    assert_eq!(npc.personal, npc_base.personal);
    assert_eq!(npc.npc, npc_base.npc);
}

#[derive(Clone, Copy, Debug)]
enum Act {
    Shot,
    Melee,
    Turret,
    Ordnance,
}

/// Damage one armored, durable Shambler takes from `act` while the
/// defender's profile carries `penetration`.
fn damage_taken(penetration: i32, armor: i32, act: Act) -> i32 {
    let mut g = legacy_game();
    for level in 0..3 {
        g.campaign.first_clear(level, Mode::Personal);
    }
    g.campaign.reward(1000, 0, Mode::Personal);
    g.campaign
        .purchase_blueprint(StructureKind::Turret)
        .unwrap();
    g.campaign.purchase_node("grenade_training").unwrap();
    g.deploy(0, Mode::Personal).unwrap();
    g.set_paused(false);
    if matches!(act, Act::Turret) {
        g.place(StructureKind::Turret, 5, 0).unwrap();
    }
    let mut a = g.attempt.take().unwrap();
    a.profile.modifiers.armor_penetration = penetration;
    g.spawn_enemy(&mut a, EnemyKind::Shambler, 5, false);
    let e = &mut a.enemies[0];
    e.x = if matches!(act, Act::Melee) {
        DEFENSE_LINE
    } else {
        8.0
    };
    e.armor = armor;
    e.hp = 1000;
    e.max_hp = 1000;
    e.injury = InjuryState::new(1000, 10);
    g.attempt = Some(a);
    match act {
        Act::Shot => g.tick(Input {
            aim: [8.0, 5.5],
            fire: true,
            ..Input::default()
        }),
        Act::Melee => g.tick(Input {
            aim: [DEFENSE_LINE, 5.5],
            melee: true,
            ..Input::default()
        }),
        Act::Turret => {
            for _ in 0..40 {
                g.tick(Input::default());
            }
        }
        Act::Ordnance => {
            g.tick(Input {
                throw: Some(ThrowRequest {
                    kind: OrdnanceKind::Grenade,
                    cell: [8, 5],
                }),
                ..Input::default()
            });
            for _ in 0..40 {
                g.tick(Input::default());
            }
        }
    }
    let a = g.attempt.as_ref().unwrap();
    let e = &a.enemies[0];
    e.max_hp - e.hp
}

#[test]
fn armor_penetration_reduces_enemy_armor_for_personal_shots_only() {
    let armor = 6;
    let base = damage_taken(0, armor, Act::Shot);
    assert!(base > 0, "the starter shot beats armor {armor}");
    assert_eq!(damage_taken(4, armor, Act::Shot), base + 4);
    // Penetration past the armor never adds damage.
    assert_eq!(damage_taken(10, armor, Act::Shot), base + armor);
    assert_eq!(damage_taken(4, 0, Act::Shot), damage_taken(0, 0, Act::Shot));
    for act in [Act::Melee, Act::Turret, Act::Ordnance] {
        let without = damage_taken(0, armor, act);
        assert!(without > 0, "{act:?} reaches the enemy");
        assert_eq!(damage_taken(4, armor, act), without, "{act:?}");
    }
}

#[test]
fn hazard_scaled_is_identity_at_one_and_scales_at_point_six() {
    let mut g = Game::default();
    g.deploy(0, Mode::Personal).unwrap();
    let a = g.attempt.as_mut().unwrap();
    assert_eq!(a.profile.modifiers.hazard_damage_multiplier, 1.0);
    for raw in [0, 1, 5, 7, 30, 40, 1_000_000] {
        assert_eq!(combat::hazard_scaled(a, raw), raw, "identity at 1.0");
    }
    a.profile.modifiers.hazard_damage_multiplier = 0.6;
    for (raw, scaled) in [(0, 0), (1, 1), (5, 3), (7, 4), (10, 6), (30, 18), (40, 24)] {
        assert_eq!(combat::hazard_scaled(a, raw), scaled, "{raw} at 0.6");
    }
    // Personal only, like the class-perk and penetration folds: Mara and
    // recruits take raw hazard damage even when the profile carries the
    // player's multiplier.
    let mut g = legacy_game();
    g.campaign.attempts = 1;
    g.deploy(0, Mode::Npc).unwrap();
    let a = g.attempt.as_mut().unwrap();
    a.profile.modifiers.hazard_damage_multiplier = 0.6;
    for raw in [0, 1, 5, 7, 30, 40] {
        assert_eq!(combat::hazard_scaled(a, raw), raw, "NPC defender at 0.6");
    }
}

/// The class perk multiplies the profile the generic modifiers already
/// built, so a perk never scales a `damage_add` before its multiplier.
#[test]
fn class_modifier_folds_after_the_generic_modifiers() {
    let game = Game::default();
    let campaign = &game.campaign;
    let generic = SkillModifiers {
        damage_add: 3,
        damage_multiplier: 1.2,
        reload_multiplier: 0.9,
        ..SkillModifiers::default()
    };
    let generic_only = profile_with(campaign, Mode::Personal, generic.clone());
    let base = profile_with(campaign, Mode::Personal, SkillModifiers::default());
    assert_eq!(
        generic_only.personal.damage,
        ((base.personal.damage + 3) as f32 * 1.2).round() as i32
    );
    let mut both = generic;
    both.class_modifiers.insert(
        base.weapon.class,
        ClassModifiers {
            damage_multiplier: 1.5,
            reload_multiplier: 0.8,
            ..ClassModifiers::default()
        },
    );
    let p = profile_with(campaign, Mode::Personal, both);
    assert_eq!(
        p.personal.damage,
        (generic_only.personal.damage as f32 * 1.5).round() as i32
    );
    assert_eq!(
        p.personal.reload,
        (generic_only.personal.reload * 0.8).max(0.65)
    );
}

fn trap_charges_after_placement(definition: &StructureDefinition, rank: u32) -> u32 {
    let mut g = trap_game();
    g.campaign.structure_upgrades[StructureKind::Trap] = rank;
    g.deploy(0, Mode::Personal).unwrap();
    g.place_definition(definition, 5, 0).unwrap();
    let a = g.attempt.as_ref().unwrap();
    a.structures.last().unwrap().charges
}

#[test]
fn trap_rank_r_places_three_plus_r_charges() {
    // The value is set here so the mechanism is pinned apart from the
    // shipped trap, which authors the same 1 since W4-03.
    let mut trap = content().structure(StructureKind::Trap).clone();
    assert_eq!(trap.charges, 3);
    trap.charges_per_rank = 1;
    for rank in 0..=10 {
        assert_eq!(trap.charges_at_rank(rank), 3 + rank);
        assert_eq!(
            trap_charges_after_placement(&trap, rank),
            3 + rank,
            "rank {rank}"
        );
    }
}

#[test]
fn structures_without_charges_per_rank_keep_their_charges() {
    // Pin: a structure that authors no charges_per_rank keeps its catalogue
    // charges at every rank, as before the mechanism. Since W4-03 the
    // shipped shrapnel trap authors 1; walls and turrets author none.
    for d in &content().structures {
        if d.kind == StructureKind::Trap {
            assert_eq!(d.charges_per_rank, 1, "{}", d.id);
            continue;
        }
        for rank in 0..=10 {
            assert_eq!(d.charges_at_rank(rank), d.charges, "{} rank {rank}", d.id);
        }
    }
    let mut flat = content().structure(StructureKind::Trap).clone();
    flat.charges_per_rank = 0;
    for rank in 0..=10 {
        assert_eq!(trap_charges_after_placement(&flat, rank), flat.charges);
    }
}

/// The charges of the shipped trap placed by a campaign that learned `nodes`
/// through the tree (the tree's own gates, prices and rank mirror).
fn shipped_trap_charges_with(nodes: &[&str]) -> u32 {
    let mut g = Game::default();
    g.campaign.first_clear(0, Mode::Personal);
    g.campaign.reward(100_000, 0, Mode::Personal);
    for node in nodes {
        g.campaign.purchase_node(node).unwrap();
    }
    g.deploy(0, Mode::Personal).unwrap();
    g.place_definition(content().structure(StructureKind::Trap), 0, 0)
        .unwrap();
    let a = g.attempt.as_ref().unwrap();
    a.structures.last().unwrap().charges
}

#[test]
fn trap_mastery_1_adds_a_trap_charge() {
    // W4-03: the shipped shrapnel trap gains one charge per mastery rank
    // (design progression-tree §1, ruling E12), so Trap mastery 1 is worth a
    // kill and every later rank another.
    let blueprint = ["engineering", "trap_blueprint"];
    assert_eq!(shipped_trap_charges_with(&blueprint), 3);
    let mut owned = blueprint.to_vec();
    owned.push("trap_rank_1");
    assert_eq!(shipped_trap_charges_with(&owned), 4);
    owned.push("trap_rank_2");
    assert_eq!(shipped_trap_charges_with(&owned), 5);
}

#[test]
fn placed_trap_keeps_its_placement_charges_when_the_rank_rises() {
    // Ruling E12: charges are fixed at placement. The definition scales
    // (test-only authored value): a trap placed at rank 0 holds 3 and one
    // placed after the rank rises to 10 holds 13. A sim that re-derived
    // charges after placement (from the catalogue trap as placed at rank 0
    // or from the current rank) cannot keep both through a tick.
    let mut trap = content().structure(StructureKind::Trap).clone();
    trap.charges_per_rank = 1;
    let mut g = trap_game();
    g.deploy(0, Mode::Personal).unwrap();
    g.place_definition(&trap, 5, 0).unwrap();
    g.campaign.structure_upgrades[StructureKind::Trap] = 10;
    g.place_definition(&trap, 6, 0).unwrap();
    for _ in 0..2 {
        g.tick(Input::default());
    }
    let a = g.attempt.as_ref().unwrap();
    let charges: Vec<u32> = a.structures.iter().map(|s| s.charges).collect();
    assert_eq!(charges, [3, 13]);
}
