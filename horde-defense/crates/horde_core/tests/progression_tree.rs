//! Static tree rules (design progression-tree §3). W2-41 lands the structural
//! rows for the decision-19 nodes (append-only ids 170-177, §5); the wave-4
//! value rules (W4-p) join this file.
mod support;
use horde_core::{
    content::{BossKind, content},
    enemy_catalog::enemy_catalog,
    progression,
    skill_catalog::{SkillEffect, SkillNode, skills},
    weapons::WeaponClass,
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
