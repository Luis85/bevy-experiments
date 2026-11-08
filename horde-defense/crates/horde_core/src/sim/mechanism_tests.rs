//! W2-24 (design progression-tree §5 "Folding", ruling E12): the skill-perk
//! mechanisms at neutral authored values. No shipped node authors a class
//! perk, penetration, a hazard multiplier or trap charges per rank yet, so
//! every test injects the value it needs.

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
    // Test-only authored value: the shipped trap gains charges in wave 4.
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
fn existing_structures_keep_charges_without_charges_per_rank() {
    // Pin: with no authored charges_per_rank every structure keeps its
    // catalogue charges at every rank, as before the mechanism.
    for d in &content().structures {
        for rank in 0..=10 {
            assert_eq!(d.charges_at_rank(rank), d.charges, "{} rank {rank}", d.id);
        }
    }
    let trap = content().structure(StructureKind::Trap);
    for rank in 0..=10 {
        assert_eq!(trap_charges_after_placement(trap, rank), trap.charges);
    }
    // A trap already on the field keeps its placed charges when the rank
    // rises later; charges are fixed at placement.
    let mut g = trap_game();
    g.deploy(0, Mode::Personal).unwrap();
    g.place(StructureKind::Trap, 5, 0).unwrap();
    g.campaign.structure_upgrades[StructureKind::Trap] = 10;
    g.tick(Input::default());
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.structures[0].charges, trap.charges);
}
