//! Static tree rules (design progression-tree §3). W2-41 lands the structural
//! rows for the decision-19 nodes (append-only ids 170-177, §5); the wave-4
//! value rules (W4-p) join this file.
mod support;
use horde_core::{
    content::{BossKind, ItemDefId, Mode, PersonalProfile, Slot, StructureKind, content},
    enemy_catalog::enemy_catalog,
    ordnance::OrdnanceKind,
    progression,
    skill_catalog::{SkillEffect, SkillModifiers, SkillNode, skills},
    weapons::{WeaponClass, WeaponSpec},
};
use std::collections::{BTreeMap, BTreeSet};

/// The wave-0 catalogue length (before the production pass).
const WAVE_ZERO_NODES: usize = 169;
/// FNV-1a of the wave-0 ids, each followed by `\n`, in catalogue order
/// (computed on the merge base with main, 7bd5de8).
const WAVE_ZERO_IDS_FNV: u64 = 0xc4dc_1b88_0f01_a3a9;
/// The decision-19 nodes in append order (ids 170-177).
const DECISION_19: [&str; 8] = [
    "shotgun_pump_drill",
    "marksman_steady_aim",
    "shield_breaker",
    "blast_discipline",
    "pistol_double_tap",
    "smg_spray_control",
    "rifle_breath_control",
    "mg_belt_feeder",
];
/// The milestone victory ranks a decision-19 gate may take.
const MILESTONES: [u64; 4] = [5, 10, 15, 20];

/// The first threat rank at which `archetype` has spawn weight in any wave
/// pool the director can draw from (`max(level, snapshot(rank).wave_tier)`
/// for every authored level, every boss unlock granted), or None.
fn first_spawn_rank(archetype: &str) -> Option<u32> {
    let progression = progression::catalog();
    let bosses = [BossKind::Breacher, BossKind::Caller, BossKind::SiegeBrute];
    (0..=progression.max_rank).find(|&rank| {
        let tier = progression.snapshot(rank).wave_tier;
        (0..content().levels.len()).any(|level| {
            enemy_catalog()
                .wave_pool(level.max(tier), &bosses)
                .iter()
                .any(|entry| entry.archetype_id == archetype && entry.weight > 0)
        })
    })
}

fn counter_nodes() -> Vec<&'static SkillNode> {
    skills()
        .nodes
        .iter()
        .filter(|node| !node.counters.is_empty())
        .collect()
}

#[test]
fn skill_catalog_is_append_only_with_eight_weapon_mastery_nodes() {
    let nodes = &skills().nodes;
    let ids: Vec<&str> = nodes.iter().map(|node| node.id.as_str()).collect();
    let wave_zero: String = ids
        .iter()
        .take(WAVE_ZERO_NODES)
        .map(|id| format!("{id}\n"))
        .collect();
    assert_eq!(
        support::fnv1a(wave_zero.as_bytes()),
        WAVE_ZERO_IDS_FNV,
        "the first {WAVE_ZERO_NODES} ids changed or moved"
    );
    assert_eq!(nodes.len(), 177);
    assert_eq!(&ids[WAVE_ZERO_NODES..], DECISION_19);
    let mut perk_classes: BTreeMap<WeaponClass, &str> = BTreeMap::new();
    for node in &nodes[WAVE_ZERO_NODES..] {
        // A mastery node is milestone gated, extends the wave-0 tree and
        // carries only the decision-19 effects.
        assert!(
            MILESTONES.contains(&node.minimum_victories) && node.minimum_liberated == 0,
            "{}: gate V{} L{}",
            node.id,
            node.minimum_victories,
            node.minimum_liberated
        );
        assert!(
            node.prerequisites
                .iter()
                .all(|p| ids[..WAVE_ZERO_NODES].contains(&p.as_str())),
            "{}: prerequisites {:?}",
            node.id,
            node.prerequisites
        );
        assert!(!node.effects.is_empty(), "{}: no effect", node.id);
        let mut classes = BTreeSet::new();
        for effect in &node.effects {
            match effect {
                SkillEffect::ClassModifier { class, .. } => {
                    classes.insert(*class);
                }
                SkillEffect::ArmorPenetration(_) | SkillEffect::HazardDamageMultiplier(_) => {}
                other => panic!("{}: unexpected effect {other:?}", node.id),
            }
        }
        assert!(classes.len() <= 1, "{}: perks for {classes:?}", node.id);
        for class in classes {
            if let Some(previous) = perk_classes.insert(class, &node.id) {
                panic!("{class:?} has perks on {previous} and {}", node.id);
            }
        }
    }
    // Six class perks, one per weapon class, plus the two counters.
    assert_eq!(
        perk_classes.keys().copied().collect::<Vec<_>>(),
        WeaponClass::ALL.to_vec()
    );
}

#[test]
fn counters_name_catalog_archetypes() {
    let archetypes: BTreeSet<&str> = enemy_catalog().archetype_ids().collect();
    for node in &skills().nodes {
        for id in &node.counters {
            assert!(
                archetypes.contains(id.as_str()),
                "{} counters {id}, which is not a catalogue archetype",
                node.id
            );
        }
    }
    let table: Vec<(&str, Vec<&str>)> = counter_nodes()
        .into_iter()
        .map(|node| {
            (
                node.id.as_str(),
                node.counters.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    assert_eq!(
        table,
        vec![
            ("shotgun_pump_drill", vec!["sprinter"]),
            ("marksman_steady_aim", vec!["shield-bearer", "juggernaut"]),
            ("shield_breaker", vec!["shield-bearer", "juggernaut"]),
            ("blast_discipline", vec!["exploder", "spitter"]),
            ("smg_spray_control", vec!["splitter"]),
            ("mg_belt_feeder", vec!["sprinter", "splitter"]),
        ]
    );
}

#[test]
fn counter_nodes_open_no_later_than_their_archetypes() {
    let nodes = counter_nodes();
    assert!(!nodes.is_empty(), "no node lists counters");
    for node in nodes {
        let ranks: Vec<(&str, Option<u32>)> = node
            .counters
            .iter()
            .map(|id| (id.as_str(), first_spawn_rank(id)))
            .collect();
        assert!(
            ranks.iter().all(|(_, rank)| rank.is_some()),
            "{}: a countered archetype never spawns: {ranks:?}",
            node.id
        );
        let first = ranks.iter().filter_map(|(_, rank)| *rank).min().unwrap();
        assert!(
            node.minimum_victories <= u64::from(first),
            "{}: gate V{} opens after its first archetype spawns at rank {first} ({ranks:?})",
            node.id,
            node.minimum_victories
        );
        // Design §5 footnote: the gate is the largest milestone at or below
        // that rank, not merely any rank below it.
        let largest = MILESTONES
            .iter()
            .rev()
            .copied()
            .find(|milestone| *milestone <= u64::from(first));
        assert_eq!(
            Some(node.minimum_victories),
            largest,
            "{}: gate V{} is not the largest milestone at or below rank {first} ({ranks:?})",
            node.id,
            node.minimum_victories
        );
    }
}

/// A body attribute rank or a structure rank: ladders open no milestone.
fn is_ladder(id: &str) -> bool {
    id.starts_with("body_")
        || ["trap_rank_", "wall_rank_", "turret_rank_"]
            .iter()
            .any(|prefix| id.starts_with(prefix))
}

#[test]
fn every_milestone_rank_opens_a_new_non_ladder_node() {
    for milestone in MILESTONES {
        let opened: Vec<&str> = skills()
            .nodes
            .iter()
            .filter(|node| node.minimum_victories == milestone && !is_ladder(&node.id))
            .map(|node| node.id.as_str())
            .collect();
        assert!(
            !opened.is_empty(),
            "victory {milestone} opens no new non-ladder node"
        );
    }
}

// ---- Wave-4 effect retunes (W4-03, design progression-tree §1 and §3.7) ----

/// The personal profile of a fresh campaign that bought `nodes`, with the
/// starter loadout (worn service pistol, work jacket).
fn personal_with(nodes: &[&str]) -> PersonalProfile {
    support::trained_campaign(0, nodes)
        .profile(Mode::Personal)
        .personal
}

/// The personal profile of a campaign that bought `nodes` and carries the
/// catalogue weapon `weapon` (built without the equip gate, so the weapon is
/// judged with and without the nodes its training sits behind).
fn personal_with_weapon(nodes: &[&str], weapon: &str) -> (PersonalProfile, WeaponSpec) {
    let mut campaign = support::trained_campaign(0, nodes);
    let definition = ItemDefId::from(weapon).index().expect("catalogue weapon");
    let id = campaign.add_item(definition).unwrap();
    let mut loadout = campaign.player.loadout;
    loadout[Slot::Ranged.index()] = id;
    let profile = campaign.profile_for_loadout(Mode::Personal, loadout, &campaign.npc);
    (profile.personal, profile.weapon)
}

/// Shots a rank-0 `archetype` takes from `personal` with `weapon`: armor
/// comes off each projectile and the weapon's per-hit cap applies.
fn shots_to_kill(personal: &PersonalProfile, weapon: &WeaponSpec, archetype: &str) -> i32 {
    let enemy = enemy_catalog().resolve(archetype).unwrap();
    let hit = (personal.damage - enemy.armor)
        .max(1)
        .min(weapon.hit_cap(enemy.health));
    (enemy.health + hit - 1) / hit
}

#[test]
fn hardened_leaves_runner_contact_damaging() {
    let base = personal_with(&[]);
    let hardened = personal_with(&["field_fitness", "hardened"]);
    assert_eq!(base.armor, 1, "the starter work jacket");
    assert_eq!(hardened.armor, base.armor + 2, "Hardened adds 2 armor");
    // sim/combat.rs: a contact hit deals `(damage - armor).max(0)`.
    let runner = enemy_catalog().resolve("runner").unwrap();
    assert_eq!(runner.damage, 4, "the rank-0 Runner");
    assert!(
        runner.damage - hardened.armor >= 1,
        "a Runner hit ({}) no longer hurts through {} armor",
        runner.damage,
        hardened.armor
    );
}

#[test]
fn steady_hands_keeps_rank_zero_shambler_two_shot_and_adds_at_most_40_percent_per_shotgun_trigger()
{
    let (base, pistol) = personal_with_weapon(&[], "worn_service_pistol");
    let (steady, _) = personal_with_weapon(&["steady_hands"], "worn_service_pistol");
    assert_eq!((base.damage, steady.damage), (12, 16));
    assert_eq!(shots_to_kill(&base, &pistol, "shambler"), 3);
    assert_eq!(
        shots_to_kill(&steady, &pistol, "shambler"),
        2,
        "Steady hands buys the two-shot on a 32-health Shambler"
    );
    // The pump shotgun: damage is per pellet, so the flat add counts per pellet.
    let (pump_base, pump) = personal_with_weapon(&[], "pump_shotgun");
    let (pump_steady, _) = personal_with_weapon(&["steady_hands"], "pump_shotgun");
    let trigger = |p: &PersonalProfile| p.damage * pump.pellets as i32;
    assert!(trigger(&pump_steady) > trigger(&pump_base));
    assert!(
        trigger(&pump_steady) * 10 <= trigger(&pump_base) * 14,
        "pump trigger {} -> {} is more than +40%",
        trigger(&pump_base),
        trigger(&pump_steady)
    );
}

#[test]
fn quick_throw_adds_throws_per_deployment() {
    let chain = [
        "grenade_training",
        "grenade_bandolier",
        "molotov_training",
        "bottle_carrier",
    ];
    let before = support::trained_campaign(0, &chain).modifiers();
    let mut with = chain.to_vec();
    with.push("quick_throw");
    let after = support::trained_campaign(0, &with).modifiers();
    let issued = |m: &SkillModifiers| horde_core::ordnance::allowance(Mode::Personal, m);
    for kind in [OrdnanceKind::Grenade, OrdnanceKind::Molotov] {
        assert_eq!(
            issued(&after)[kind],
            issued(&before)[kind] + 1,
            "{kind:?}: Quick throw adds one throw per deployment"
        );
    }
    assert_eq!(before.ordnance_cooldown_multiplier, 1.0);
    assert_eq!(after.ordnance_cooldown_multiplier, 0.85);
}

#[test]
fn combat_reload_cuts_reload_by_a_quarter() {
    let before = personal_with(&["quick_reload", "rapid_cycle"]);
    let after = personal_with(&["quick_reload", "rapid_cycle", "combat_reload"]);
    assert!(
        after.reload > 0.65,
        "the starter pistol stays above the reload floor, so the ratio is the node's"
    );
    let ratio = after.reload / before.reload;
    assert!((ratio - 0.75).abs() < 1e-5, "reload ratio {ratio}");
    // Not dominated by Reload drill (x0.80) on the same stat.
    let drill = personal_with(&["quick_reload"]).reload / personal_with(&[]).reload;
    assert!(
        ratio < drill,
        "Combat reload {ratio} vs Reload drill {drill}"
    );
}

#[test]
fn trap_ranks_each_add_one_charge_and_say_so() {
    let trap = content().structure(StructureKind::Trap);
    assert_eq!((trap.id.as_str(), trap.charges), ("shrapnel_trap", 3));
    for rank in 0..=10u32 {
        assert_eq!(trap.charges_at_rank(rank), 3 + rank, "rank {rank}");
    }
    for other in [StructureKind::Wall, StructureKind::Turret] {
        let d = content().structure(other);
        assert_eq!(d.charges_at_rank(10), d.charges, "{}", d.id);
    }
    for rank in 1..=10u32 {
        let node = skills().node(&format!("trap_rank_{rank}")).unwrap();
        let charges = if rank == 1 {
            "+1 charge ".to_string()
        } else {
            format!("+{rank} charges ")
        };
        assert!(
            node.description.contains(&charges) && node.description.contains("+1 charge per rank"),
            "{}: {:?} does not name {charges:?}",
            node.id,
            node.description
        );
    }
}
