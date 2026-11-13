//! Static tree rules (design progression-tree §3). W2-41 lands the structural
//! rows for the decision-19 nodes (append-only ids 170-177, §5), W4-03 the
//! effect retunes and W4-p the value rules 1-6 on the repriced catalogue.
mod support;
use horde_core::{
    campaign::tree_value::{
        CHEAP_NODE_LIMIT, CHEAP_NODE_PRICE, DEPTH_PEER_LIMIT, ZERO_GAIN, branch_value_per_coin,
        ladder_rank, node_value, node_value_at, reference_ranks,
    },
    content::{BossKind, ItemDefId, Mode, PersonalProfile, Slot, StructureKind, content},
    enemy_catalog::enemy_catalog,
    ordnance::OrdnanceKind,
    progression,
    sim::{node_depths, tier_cost},
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

// ---- Wave-4 value rules (W4-p, design progression-tree §3 rules 1-6) ----

fn node(id: &str) -> &'static SkillNode {
    skills().node(id).expect("shipped skill node")
}

/// `ancestor` is one of `node`'s prerequisite ancestors.
fn is_ancestor(ancestor: &SkillNode, node: &SkillNode) -> bool {
    let mut stack: Vec<&str> = node.prerequisites.iter().map(String::as_str).collect();
    let mut seen = BTreeSet::new();
    while let Some(id) = stack.pop() {
        if id == ancestor.id {
            return true;
        }
        if seen.insert(id)
            && let Some(parent) = skills().node(id)
        {
            stack.extend(parent.prerequisites.iter().map(String::as_str));
        }
    }
    false
}

/// The branch whose value per coin a node is measured against: its own,
/// except that a body rank reads its dedicated peer's branch (design §3
/// rule 2: Vitality, Strength and Endurance against Survival; Coordination
/// and Perception against Firearms).
fn value_branch(node: &SkillNode) -> &str {
    use horde_core::campaign::BodyAttribute as B;
    node.effects
        .iter()
        .find_map(|effect| match effect {
            SkillEffect::BodyDevelopment { attribute, .. } => Some(match attribute {
                B::Vitality | B::Strength | B::Endurance => "Survival",
                B::Coordination | B::Perception => "Firearms",
            }),
            _ => None,
        })
        .unwrap_or(node.branch.as_str())
}

/// Rule 1. A valued node is dominated when (i) it gains nothing at both
/// reference ranks, or (ii) a node of its dominance group that is neither
/// its ancestor nor its descendant costs no more, opens no later (victories
/// and liberations) and gains at least as much at both of its reference
/// ranks, strictly more at one. Ladder ranks of 2 or more neither dominate
/// nor are dominated under (ii).
///
/// The groups are the W2-25 model's (R48): the decision-19 kinds
/// `CadenceMultiplier`, `ArmorPenetration` and `HazardDamageMultiplier` each
/// have a group of their own, and a node with several effects sums its gain
/// over the axes it touches.
#[test]
fn no_valued_node_is_dominated_at_its_reference_states() {
    let valued: Vec<_> = skills()
        .nodes
        .iter()
        .filter(|n| n.price > 0)
        .filter_map(|n| Some((n, node_value(n)?)))
        .collect();
    assert!(valued.len() > 100, "{} valued nodes", valued.len());
    let pairwise = |n: &SkillNode| ladder_rank(n).is_none_or(|rank| rank < 2);
    let mut dominated = Vec::new();
    for (a, value) in &valued {
        if value.ln_gain.iter().all(|gain| *gain <= ZERO_GAIN) {
            dominated.push(format!("{} gains nothing ({:?})", a.id, value.ln_gain));
            continue;
        }
        if !pairwise(a) {
            continue;
        }
        let refs = reference_ranks(a);
        for (b, other) in &valued {
            if b.id == a.id
                || other.group != value.group
                || !pairwise(b)
                || is_ancestor(a, b)
                || is_ancestor(b, a)
                || b.price > a.price
                || b.minimum_victories > a.minimum_victories
                || b.minimum_liberated > a.minimum_liberated
            {
                continue;
            }
            let theirs = refs.map(|rank| node_value_at(b, rank).expect("valued"));
            let at_least = theirs.iter().zip(value.ln_gain).all(|(b, a)| *b >= a);
            let strictly = theirs.iter().zip(value.ln_gain).any(|(b, a)| *b > a);
            if at_least && strictly {
                dominated.push(format!(
                    "{} ({} coins, {:?}) is dominated by {} ({} coins, {theirs:?})",
                    a.id, a.price, value.ln_gain, b.id, b.price
                ));
            }
        }
    }
    assert!(dominated.is_empty(), "{}", dominated.join("; "));
}

/// Rule 2. A valued node under [`CHEAP_NODE_PRICE`] is worth at most
/// [`CHEAP_NODE_LIMIT`] times its branch's value per coin at each of its
/// reference ranks.
#[test]
fn cheap_nodes_stay_within_twice_branch_value_per_coin() {
    let mut over = Vec::new();
    for n in skills()
        .nodes
        .iter()
        .filter(|n| n.price > 0 && n.price < CHEAP_NODE_PRICE)
    {
        let Some(value) = node_value(n) else { continue };
        for (rank, gain) in reference_ranks(n).into_iter().zip(value.ln_gain) {
            let branch = branch_value_per_coin(value_branch(n), rank);
            let per_coin = gain / n.price as f32;
            if per_coin > CHEAP_NODE_LIMIT * branch {
                over.push(format!(
                    "{} ({} coins) at rank {rank}: {per_coin:.5} per coin > {CHEAP_NODE_LIMIT} x {branch:.5} ({})",
                    n.id,
                    n.price,
                    value_branch(n)
                ));
            }
        }
    }
    assert!(over.is_empty(), "{}", over.join("; "));
}

/// Value per coin of a valued, priced node at `rank`.
fn per_coin_at(n: &SkillNode, rank: u32) -> Option<f32> {
    (n.price > 0).then_some(())?;
    Some(node_value_at(n, rank)? / n.price as f32)
}

/// Rule 3. At any price, a non-ladder node's value per coin is at most
/// [`DEPTH_PEER_LIMIT`] times the median of the valued non-ladder nodes of
/// its branch one prerequisite depth either side that are open at that
/// rank. Ladders have their own rule (4).
#[test]
fn no_node_outvalues_its_depth_peers_by_more_than_two_and_a_half_x() {
    let depths = node_depths();
    let single = |n: &SkillNode| ladder_rank(n).is_none();
    let mut over = Vec::new();
    let mut checked = 0;
    for n in skills().nodes.iter().filter(|n| single(n)) {
        let Some(value) = node_value(n) else { continue };
        for (rank, gain) in reference_ranks(n).into_iter().zip(value.ln_gain) {
            let mut peers: Vec<f32> = skills()
                .nodes
                .iter()
                .filter(|p| {
                    p.id != n.id
                        && single(p)
                        && p.branch == n.branch
                        && depths[&p.id].abs_diff(depths[&n.id]) <= 1
                        && p.minimum_victories <= u64::from(rank)
                })
                .filter_map(|p| per_coin_at(p, rank))
                .collect();
            if peers.is_empty() {
                continue;
            }
            peers.sort_by(f32::total_cmp);
            // The lower median, as the pacing statistics use.
            let median = peers[(peers.len() - 1) / 2];
            let per_coin = gain / n.price as f32;
            checked += 1;
            if per_coin > DEPTH_PEER_LIMIT * median {
                over.push(format!(
                    "{} ({} coins) at rank {rank}: {per_coin:.5} per coin > {DEPTH_PEER_LIMIT} x median {median:.5} of {} peers",
                    n.id,
                    n.price,
                    peers.len()
                ));
            }
        }
    }
    assert!(checked > 40, "{checked} node states checked");
    assert!(over.is_empty(), "{}", over.join("; "));
}

/// The ladders: each body attribute and each structure kind, ranks in order.
fn ladders() -> Vec<Vec<&'static SkillNode>> {
    let mut ladders = Vec::new();
    for prefix in [
        "body_vitality_",
        "body_strength_",
        "body_coordination_",
        "body_endurance_",
        "body_perception_",
        "trap_rank_",
        "wall_rank_",
        "turret_rank_",
    ] {
        let ranks: Vec<_> = (1..)
            .map_while(|rank| skills().node(&format!("{prefix}{rank}")))
            .collect();
        assert!(ranks.len() >= 10, "{prefix}: {} ranks", ranks.len());
        ladders.push(ranks);
    }
    ladders
}

/// Rule 4. Along a ladder the value per coin never rises: at each of a
/// rank's reference ranks it is worth no more per coin than the rank below.
///
/// Perception is checked by halves instead. Its gain is a staircase: a
/// weak hit deals `round(damage x (1 + bonus))`, so a rank is worth
/// something only when its small bonus moves that whole number (at rank 0
/// every odd rank gains exactly nothing), and no rising price ladder makes
/// rank 4 worth less per coin than a rank 3 that gains 0. Over ten ranks the
/// steps average out: ranks 11-20 are worth no more per coin than ranks
/// 1-10 at the reference ranks of the first and the last rank.
#[test]
fn ladder_value_per_coin_never_rises() {
    let mut rises = Vec::new();
    for ladder in ladders() {
        if ladder[0].id == "body_perception_1" {
            assert_eq!(ladder.len(), 20);
            let last = ladder[ladder.len() - 1];
            let half = |nodes: &[&SkillNode], rank: u32| {
                let gain: f32 = nodes
                    .iter()
                    .map(|n| node_value_at(n, rank).expect("valued ladder rank"))
                    .sum();
                gain / nodes.iter().map(|n| n.price).sum::<u64>() as f32
            };
            for rank in reference_ranks(ladder[0])
                .into_iter()
                .chain(reference_ranks(last))
            {
                let (lower, upper) = (half(&ladder[..10], rank), half(&ladder[10..], rank));
                assert!(lower > 0.0, "Perception 1-10 gains nothing at rank {rank}");
                if upper > lower * 1.000_01 {
                    rises.push(format!(
                        "body_perception 11-20 {upper:.6} per coin > 1-10 {lower:.6} at rank {rank}"
                    ));
                }
            }
            continue;
        }
        for pair in ladder.windows(2) {
            let (lower, upper) = (pair[0], pair[1]);
            for rank in reference_ranks(upper) {
                let below = per_coin_at(lower, rank).expect("valued ladder rank");
                let above = per_coin_at(upper, rank).expect("valued ladder rank");
                if above > below * 1.000_01 {
                    rises.push(format!(
                        "{} {above:.6} per coin > {} {below:.6} at rank {rank}",
                        upper.id, lower.id
                    ));
                }
            }
        }
    }
    assert!(rises.is_empty(), "{}", rises.join("; "));
}

/// Rule 5. A body rank 1 is the cheap way into its stat: cheaper than the
/// dedicated node of the same dominance group.
#[test]
fn body_rank_one_is_cheaper_than_its_dedicated_peer() {
    for (body, peer) in [
        ("body_vitality_1", "field_fitness"),
        ("body_coordination_1", "quick_reload"),
        ("body_strength_1", "heavy_strike"),
        ("body_endurance_1", "endurance_drill"),
    ] {
        let (body, peer) = (node(body), node(peer));
        assert_eq!(
            node_value(body).expect("valued").group,
            node_value(peer).expect("valued").group,
            "{} and {} share a dominance group",
            body.id,
            peer.id
        );
        assert!(
            body.price < peer.price,
            "{} ({}) is not cheaper than {} ({})",
            body.id,
            body.price,
            peer.id,
            peer.price
        );
    }
}

/// Rule 6 and the design §4 tier definition: a tier is every node up to its
/// prerequisite depth that a fresh campaign can buy inside the tier window,
/// so the milestone-gated nodes (a victory gate of 5 or more) stay out.
#[test]
fn tier_costs_sit_in_design_bands() {
    let depths = node_depths();
    let gated = |n: &SkillNode| n.minimum_victories >= 5;
    assert!(
        skills()
            .nodes
            .iter()
            .any(|n| depths[&n.id] <= 2 && gated(n)),
        "a milestone-gated node sits inside tier 2, so the definition matters"
    );
    for (tier, band) in [(1, 1_700..=2_300), (2, 7_800..=9_800), (3, 18_500..=22_500)] {
        let expected: u64 = skills()
            .nodes
            .iter()
            .filter(|n| depths[&n.id] <= tier && !gated(n))
            .map(|n| n.price)
            .sum();
        assert_eq!(
            tier_cost(tier),
            expected,
            "tier {tier} counts a milestone-gated node"
        );
        assert!(
            band.contains(&expected),
            "tier {tier} costs {expected}, outside {band:?}"
        );
    }
}

// ---- The blueprint price (W4-p) ----

/// A blueprint is bought as its tree node, so every place that quotes or
/// checks its price reads the node's: the recommendation (text, price and
/// the affordable flag) and the smoke preparation. The content
/// `blueprint_price` is what a legacy-features campaign pays outside the
/// tree, and the two differ since the repricing.
#[test]
fn blueprint_offers_quote_the_tree_node_price() {
    use horde_core::campaign::{Campaign, RecommendationAction};
    let fresh = Campaign::default();
    for kind in StructureKind::ALL {
        let id = match kind {
            StructureKind::Trap => "trap_blueprint",
            StructureKind::Wall => "wall_blueprint",
            StructureKind::Turret => "turret_blueprint",
        };
        assert_eq!(fresh.blueprint_price(kind), support::node_price(id), "{id}");
        let legacy = Campaign {
            legacy_features: true,
            ..Campaign::default()
        };
        assert_eq!(
            legacy.blueprint_price(kind),
            content().structure(kind).blueprint_price,
            "{id} on a legacy-features campaign"
        );
    }
    let node = support::node_price("trap_blueprint");
    let legacy = content().structure(StructureKind::Trap).blueprint_price;
    assert!(
        legacy < node,
        "the trap node ({node}) costs more than the legacy blueprint ({legacy}), so the two are told apart"
    );
    // One victory and Field engineering open the trap blueprint; the wallet
    // covers the legacy price but not the node.
    let mut game = support::country_game("DEU");
    let campaign = game.campaign_mut();
    campaign.reward(
        support::node_price("engineering") + legacy,
        0,
        Mode::Personal,
    );
    campaign.purchase_node("engineering").unwrap();
    campaign.victories = 1;
    assert_eq!(campaign.coins, legacy);
    assert!(campaign.blueprint_available(StructureKind::Trap));
    let offer = campaign.recommendation(Mode::Personal);
    assert_eq!(
        offer.action,
        RecommendationAction::LearnNode("trap_blueprint".into())
    );
    assert_eq!(offer.price, node, "the offer quotes the node price");
    assert!(
        !offer.affordable,
        "{legacy} coins do not buy a {node}-coin node"
    );
    assert!(
        offer.detail.contains(&format!("{node} coins required")),
        "{:?}",
        offer.detail
    );
    // The smoke preparation buys the trap only when the node is affordable.
    horde_core::sim::smoke_prepare(&mut game, Mode::Npc)
        .expect("an unaffordable blueprint is skipped, not an error");
    assert_eq!(game.campaign().coins, legacy);
    assert!(!game.campaign().blueprints[StructureKind::Trap]);
}
