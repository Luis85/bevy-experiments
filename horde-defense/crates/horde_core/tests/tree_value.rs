//! The analysis-only tree value model (W2-25, design progression-tree
//! section 2): the survival axis is the effective-health ratio, an ordnance
//! cooldown is worth nothing today, and the dominance groups pair each
//! dedicated node with its body-ladder peer.
use horde_core::campaign::Campaign;
use horde_core::campaign::tree_value::{
    BENCHMARK_GUNS, DominanceGroup, ValueAxis, ZERO_GAIN, benchmark_classes, node_value,
    node_value_at, reference_ranks, value_table,
};
use horde_core::content::Mode;
use horde_core::skill_catalog::{SkillNode, skills};
use horde_core::weapons::WeaponClass;

fn node(id: &str) -> &'static SkillNode {
    skills().node(id).expect("shipped skill node")
}

#[test]
fn field_fitness_ehp_gain_matches_ratio() {
    // Field fitness only adds health, so its survival gain is the ratio of
    // the starter's health with and without it (the mean hit is unchanged).
    let field_fitness = node("field_fitness");
    let health = Campaign::default().profile(Mode::Personal).personal.health as f32;
    let expected = ((health + 35.0) / health).ln();
    let value = node_value(field_fitness).expect("field fitness is valued");
    assert_eq!(value.axis, ValueAxis::Survival);
    assert_eq!(reference_ranks(field_fitness), [0, 4]);
    for (gain, rank) in value
        .ln_gain
        .into_iter()
        .zip(reference_ranks(field_fitness))
    {
        assert!(
            (gain - expected).abs() < 1e-5,
            "rank {rank}: gain {gain} != ln({}/{health}) = {expected}",
            health + 35.0
        );
        assert_eq!(node_value_at(field_fitness, rank), Some(gain));
    }
}

#[test]
fn quick_throw_is_zero_today() {
    // Ordnance value counts charges, damage and radius, not the cooldown,
    // so the shipped cooldown-only Quick throw is worth nothing (it is
    // retuned in wave 4).
    let value = node_value(node("quick_throw")).expect("quick throw is valued");
    assert_eq!(value.axis, ValueAxis::Ordnance);
    assert_eq!(value.group, DominanceGroup::Ordnance);
    for gain in value.ln_gain {
        assert!(gain.abs() <= ZERO_GAIN, "quick throw gain {gain}");
    }
    // A charge node on the same axis is worth something, so zero is not
    // the model's default.
    let bandolier = node_value(node("grenade_bandolier")).expect("valued");
    assert!(bandolier.ln_gain.iter().all(|gain| *gain > ZERO_GAIN));
}

#[test]
fn dominance_groups_pair_vitality_with_health_add() {
    let group = |id: &str| node_value(node(id)).map(|v| v.group);
    for id in ["field_fitness", "veteran_vitality", "body_vitality_1"] {
        assert_eq!(group(id), Some(DominanceGroup::Health), "{id}");
    }
    assert_eq!(group("quick_reload"), Some(DominanceGroup::Reload));
    assert_eq!(group("body_coordination_1"), Some(DominanceGroup::Reload));
    assert_eq!(group("heavy_strike"), Some(DominanceGroup::Melee));
    assert_eq!(group("body_strength_1"), Some(DominanceGroup::Melee));
    assert_eq!(group("endurance_drill"), Some(DominanceGroup::Stamina));
    assert_eq!(group("body_endurance_1"), Some(DominanceGroup::Stamina));
    assert_eq!(group("steady_hands"), Some(DominanceGroup::Damage));
    assert_eq!(group("stopping_power"), Some(DominanceGroup::Damage));
    assert_eq!(group("body_perception_1"), Some(DominanceGroup::Weak));
    assert_eq!(group("hardened"), Some(DominanceGroup::Armor));
    // Capability nodes carry no value.
    for id in [
        "pistol_training",
        "piercing",
        "engineering",
        "trap_blueprint",
    ] {
        assert_eq!(group(id), None, "{id}");
    }
}

#[test]
fn every_benchmark_gun_is_benched_once_per_weapon_class() {
    // A missing benchmark gun would leave its class perks measured on the
    // other classes' guns, worth nothing and so dominated.
    let classes = benchmark_classes();
    assert_eq!(classes.len(), BENCHMARK_GUNS.len(), "{classes:?}");
    assert_eq!(classes, WeaponClass::ALL);
}

#[test]
fn values_table_lists_every_node_once_in_catalogue_order() {
    let table = value_table();
    let ids: Vec<&str> = table
        .lines()
        .skip(1)
        .map(|line| line.split(',').next().unwrap_or_default())
        .collect();
    let expected: Vec<&str> = skills().nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, expected);
}
