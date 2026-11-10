//! The skill-tree value model (design progression-tree section 2): an
//! analysis-only guard for the wave-4 static tree rules and the `pacing
//! --values` table. **No rule reads it**: it never touches a campaign, a
//! save or any RNG, and it changes no digest.
//!
//! A valued node's gain is the change in the natural log of one axis
//! metric when a synthetic campaign that owns the node's prerequisite
//! ancestors (which include its lower ladder ranks) also owns the node,
//! measured against the director's own wave pool
//! (`wave_pool(progression().snapshot(rank).wave_tier)`, no boss unlocks)
//! weighted by spawn weight times group size, since each pick spawns
//! `group_size` members. No balance preset is read: enemy health and damage
//! are the progression snapshot's alone, which matches Normal (its enemy
//! health and damage multipliers are 1). Capability nodes (an
//! `Unlock`, `FeatureUnlock`, `Blueprint` or `WeaponTraining` effect) carry
//! no value.
//!
//! The axes:
//! - **Firepower**: ln of the pool-weighted continuous kill rate. Per pellet
//!   `min(max(damage - armor_eff, 0), target_hp)` with
//!   `armor_eff = max(armor + ranged_armor - penetration, 0)`; a trigger is
//!   capped by the weapon's hit cap; a [`REFERENCE_WEAK_HIT_SHARE`] of
//!   triggers are weak-point hits; time to kill is the shots times the
//!   cadence plus the reload overhead `shots / magazine × reload`. Generic
//!   firearm nodes average the log over [`BENCHMARK_GUNS`]; a class perk
//!   uses its class's benchmark gun.
//! - **Survival**: `ln(health / max(mean hit after armor and the hazard
//!   multiplier, 0.25))`. A fused exploder's hit is its blast (it detonates
//!   in contact instead of attacking); the hazard multiplier scales that
//!   blast and reach attackers' spit. A burst without a fuse goes off where
//!   the body falls and is not counted.
//! - **Melee**: ln of the mean kill fraction per strike (contact armor)
//!   times the strikes in one stamina pool.
//! - **Ordnance**: ln of Σ charges × clipped damage (impact plus burn) ×
//!   radius² over the unlocked throws. Cooldown is not counted.
//! - **Fortification**: ln of the engineering multiplier plus, per owned
//!   blueprint, ln of its structure term: trap charges × kill fraction per
//!   charge, turret kill fraction per second, wall durability per mean hit.
//!   Damage and durability carry the sim's structure bonus (rank,
//!   fortification and engineering multiplier, `sim::npc::structure_bonus`).
use crate::campaign::{Campaign, DefenderProfile};
use crate::content::{Mode, Slot, StructureKind, content};
use crate::enemy_catalog::{DeathEffect, enemy_catalog};
use crate::ordnance::OrdnanceKind;
use crate::progression::ProgressionSnapshot;
use crate::skill_catalog::{SkillEffect, SkillModifiers, SkillNode, skills};
use crate::weapons::{WeaponClass, WeaponSpec};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::sync::OnceLock;

/// Share of personal triggers that strike a weak point.
pub const REFERENCE_WEAK_HIT_SHARE: f32 = 0.25;
/// A gain at or below this is no gain (dominance rule (i)).
pub const ZERO_GAIN: f32 = 0.001;
/// Nodes cheaper than this are checked against their branch's value per coin.
pub const CHEAP_NODE_PRICE: u64 = 100;
/// A cheap node may be worth at most this many times its branch's value per coin.
pub const CHEAP_NODE_LIMIT: f32 = 2.0;
/// A node may be worth at most this many times its depth peers' median value per coin.
pub const DEPTH_PEER_LIMIT: f32 = 2.5;
/// The guns generic firearm nodes are averaged over, one per class.
pub const BENCHMARK_GUNS: [&str; 6] = [
    "compact_service_pistol",
    "compact_smg",
    "pump_shotgun",
    "patrol_rifle_tier1",
    "scout_marksman_rifle",
    "belt_fed_machine_gun",
];

/// Floor for a metric before its log, so an axis with nothing to measure
/// stays finite.
const METRIC_FLOOR: f64 = 1e-6;
/// Stamina one melee strike spends (`sim/manual.rs`).
const STAMINA_PER_STRIKE: f64 = 25.0;

/// What a valued node improves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueAxis {
    Firepower,
    Survival,
    Melee,
    Ordnance,
    Fortification,
}
impl ValueAxis {
    pub fn label(self) -> &'static str {
        match self {
            Self::Firepower => "firepower",
            Self::Survival => "survival",
            Self::Melee => "melee",
            Self::Ordnance => "ordnance",
            Self::Fortification => "fortification",
        }
    }
}

/// Nodes that may dominate one another: only a node of the same group can
/// make another redundant (design A's grouping). Cadence, armor
/// penetration and hazard nodes have a group of their own, so no other
/// node can dominate them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DominanceGroup {
    /// HealthAdd and Vitality ranks.
    Health,
    /// ReloadMultiplier and Coordination ranks.
    Reload,
    /// MeleeAdd and Strength ranks.
    Melee,
    /// StaminaAdd and Endurance ranks.
    Stamina,
    /// DamageAdd and DamageMultiplier.
    Damage,
    /// Perception ranks.
    Weak,
    /// One weapon class's perks.
    Class(WeaponClass),
    Ordnance,
    /// Engineering multipliers and structure ranks.
    Fortification,
    /// ArmorAdd.
    Armor,
    /// CadenceMultiplier.
    Cadence,
    /// ArmorPenetration.
    Penetration,
    /// HazardDamageMultiplier.
    Hazard,
}
impl DominanceGroup {
    pub fn label(self) -> String {
        match self {
            Self::Health => "health".into(),
            Self::Reload => "reload".into(),
            Self::Melee => "melee".into(),
            Self::Stamina => "stamina".into(),
            Self::Damage => "damage".into(),
            Self::Weak => "weak".into(),
            Self::Class(class) => format!("class:{}", class.name().to_lowercase()),
            Self::Ordnance => "ordnance".into(),
            Self::Fortification => "fortification".into(),
            Self::Armor => "armor".into(),
            Self::Cadence => "cadence".into(),
            Self::Penetration => "penetration".into(),
            Self::Hazard => "hazard".into(),
        }
    }
}

/// A valued node's axis, group and ln gain at its two [`reference_ranks`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeValue {
    pub axis: ValueAxis,
    pub group: DominanceGroup,
    pub ln_gain: [f32; 2],
}

/// The axis and group of one effect; None for a capability.
fn classify(effect: &SkillEffect) -> Option<(ValueAxis, DominanceGroup)> {
    use crate::campaign::BodyAttribute as B;
    use DominanceGroup as G;
    use ValueAxis as A;
    Some(match effect {
        SkillEffect::BodyDevelopment { attribute, .. } => match attribute {
            B::Vitality => (A::Survival, G::Health),
            B::Strength => (A::Melee, G::Melee),
            B::Coordination => (A::Firepower, G::Reload),
            B::Endurance => (A::Melee, G::Stamina),
            B::Perception => (A::Firepower, G::Weak),
        },
        SkillEffect::StructureRank { .. } | SkillEffect::EngineeringMultiplier(_) => {
            (A::Fortification, G::Fortification)
        }
        SkillEffect::DamageAdd(_) | SkillEffect::DamageMultiplier(_) => (A::Firepower, G::Damage),
        SkillEffect::ReloadMultiplier(_) => (A::Firepower, G::Reload),
        SkillEffect::CadenceMultiplier(_) => (A::Firepower, G::Cadence),
        SkillEffect::ClassModifier { class, .. } => (A::Firepower, G::Class(*class)),
        SkillEffect::ArmorPenetration(_) => (A::Firepower, G::Penetration),
        SkillEffect::HealthAdd(_) => (A::Survival, G::Health),
        SkillEffect::ArmorAdd(_) => (A::Survival, G::Armor),
        SkillEffect::HazardDamageMultiplier(_) => (A::Survival, G::Hazard),
        SkillEffect::StaminaAdd(_) => (A::Melee, G::Stamina),
        SkillEffect::MeleeAdd(_) => (A::Melee, G::Melee),
        SkillEffect::GrenadeCharges(_)
        | SkillEffect::MolotovCharges(_)
        | SkillEffect::OrdnanceDamageMultiplier(_)
        | SkillEffect::OrdnanceRadiusMultiplier(_)
        | SkillEffect::BurnDurationMultiplier(_)
        | SkillEffect::OrdnanceCooldownMultiplier(_) => (A::Ordnance, G::Ordnance),
        SkillEffect::Blueprint(_)
        | SkillEffect::FeatureUnlock(_)
        | SkillEffect::WeaponTraining(_)
        | SkillEffect::Unlock(_) => return None,
    })
}

/// Each effect's axis and group, or None when the node carries a capability.
fn classified(node: &SkillNode) -> Option<Vec<(ValueAxis, DominanceGroup)>> {
    let classes: Option<Vec<_>> = node.effects.iter().map(classify).collect();
    classes.filter(|c| !c.is_empty())
}

/// The ladder rank a body-development or structure-rank node grants; None
/// for every other node. Ranks of 2 or more stay out of pairwise dominance.
pub fn ladder_rank(node: &SkillNode) -> Option<u32> {
    node.effects.iter().find_map(|effect| match effect {
        SkillEffect::BodyDevelopment { rank, .. } | SkillEffect::StructureRank { rank, .. } => {
            Some(*rank)
        }
        _ => None,
    })
}

/// The two progression ranks a node is valued at: its victory gate `g`
/// and `g + 4`.
pub fn reference_ranks(node: &SkillNode) -> [u32; 2] {
    let g = u32::try_from(node.minimum_victories).unwrap_or(u32::MAX);
    [g, g.saturating_add(4)]
}

/// The node's ln gain at progression rank `rank`; None for a capability node.
pub fn node_value_at(node: &SkillNode, rank: u32) -> Option<f32> {
    let classes = classified(node)?;
    let before = fold(node, false);
    let after = fold(node, true);
    let context = Context::at(rank);
    let guns = guns_for(node);
    let axes: BTreeSet<ValueAxis> = classes.iter().map(|(axis, _)| *axis).collect();
    let gain: f64 = axes
        .into_iter()
        .map(|axis| context.metric(axis, &after, &guns) - context.metric(axis, &before, &guns))
        .sum();
    Some(gain as f32)
}

/// The node's axis and group (its first effect's) and its gain at both
/// reference ranks; None for a capability node.
pub fn node_value(node: &SkillNode) -> Option<NodeValue> {
    let (axis, group) = *classified(node)?.first()?;
    let [low, high] = reference_ranks(node);
    Some(NodeValue {
        axis,
        group,
        ln_gain: [node_value_at(node, low)?, node_value_at(node, high)?],
    })
}

/// Σ ln gain / Σ price at `rank` over the valued, priced, non-ladder nodes
/// of `branch` whose victory gate is at most `rank`; 0 when there are none.
pub fn branch_value_per_coin(branch: &str, rank: u32) -> f32 {
    let (gain, price) = skills()
        .nodes
        .iter()
        .filter(|n| {
            n.branch == branch
                && n.price > 0
                && ladder_rank(n).is_none()
                && n.minimum_victories <= u64::from(rank)
        })
        .filter_map(|n| Some((node_value_at(n, rank)?, n.price)))
        .fold((0.0f64, 0u64), |(g, p), (gain, price)| {
            (g + f64::from(gain), p.saturating_add(price))
        });
    if price == 0 {
        0.0
    } else {
        (gain / price as f64) as f32
    }
}

/// The `pacing --values` table: a CSV header, then one row per catalogue
/// node in catalogue order with its axis, group and gain (and gain per
/// coin) at both reference ranks. Capability rows leave the value columns
/// empty.
pub fn value_table() -> String {
    let mut s = String::from(
        "id,branch,price,gate,axis,group,rank_lo,gain_lo,per_coin_lo,rank_hi,gain_hi,per_coin_hi\n",
    );
    for node in &skills().nodes {
        let [low, high] = reference_ranks(node);
        let _ = write!(
            s,
            "{},{},{},{}",
            node.id, node.branch, node.price, node.minimum_victories
        );
        match node_value(node) {
            Some(value) => {
                let _ = write!(s, ",{},{}", value.axis.label(), value.group.label());
                for (rank, gain) in [low, high].into_iter().zip(value.ln_gain) {
                    let per_coin = if node.price > 0 {
                        format!("{:.6}", gain / node.price as f32)
                    } else {
                        String::new()
                    };
                    let _ = write!(s, ",{rank},{gain:.4},{per_coin}");
                }
            }
            None => s.push_str(",capability,,,,,,,"),
        }
        s.push('\n');
    }
    s
}

/// The modifiers of a synthetic campaign owning `node`'s prerequisite
/// ancestors and, when `with_node`, the node itself, folded in catalogue
/// order like [`Campaign::modifiers`]. A node outside the catalogue folds
/// last.
fn fold(node: &SkillNode, with_node: bool) -> SkillModifiers {
    let catalog = skills();
    let mut owned = BTreeSet::new();
    let mut stack: Vec<&str> = node.prerequisites.iter().map(String::as_str).collect();
    while let Some(id) = stack.pop() {
        if owned.insert(id)
            && let Some(parent) = catalog.node(id)
        {
            stack.extend(parent.prerequisites.iter().map(String::as_str));
        }
    }
    let mut modifiers = SkillModifiers::default();
    let mut folded = false;
    for n in &catalog.nodes {
        if n.id == node.id {
            folded = true;
            if with_node {
                node.effects.iter().for_each(|e| modifiers.apply(e));
            }
        } else if owned.contains(n.id.as_str()) {
            n.effects.iter().for_each(|e| modifiers.apply(e));
        }
    }
    if with_node && !folded {
        node.effects.iter().for_each(|e| modifiers.apply(e));
    }
    modifiers
}

/// The fresh campaign every profile is derived from, with each benchmark
/// gun owned: the starter loadout and one loadout per benchmark gun.
struct Bench {
    campaign: Campaign,
    starter: [u64; 4],
    guns: Vec<(WeaponClass, [u64; 4])>,
}
fn bench() -> &'static Bench {
    static BENCH: OnceLock<Bench> = OnceLock::new();
    BENCH.get_or_init(|| {
        let mut campaign = Campaign::default();
        let starter = campaign.player.loadout;
        let mut guns = Vec::new();
        for id in BENCHMARK_GUNS {
            let Some(index) = content().items.iter().position(|i| i.id.as_str() == id) else {
                continue;
            };
            let definition = &content().items[index];
            if definition.slot != Slot::Ranged {
                continue;
            }
            let class = WeaponSpec::from_item(definition).class;
            if let Ok(item) = campaign.add_item(index) {
                let mut loadout = starter;
                loadout[Slot::Ranged.index()] = item;
                guns.push((class, loadout));
            }
        }
        Bench {
            campaign,
            starter,
            guns,
        }
    })
}
impl Bench {
    fn profile(&self, modifiers: &SkillModifiers, loadout: [u64; 4]) -> DefenderProfile {
        self.campaign.profile_with_modifiers(
            Mode::Personal,
            loadout,
            &self.campaign.npc,
            modifiers.clone(),
        )
    }
}

/// The loadouts a node's firepower is averaged over: its perk classes'
/// benchmark guns, otherwise every benchmark gun (the starter loadout when
/// the content has none).
fn guns_for(node: &SkillNode) -> Vec<[u64; 4]> {
    let bench = bench();
    let classes: BTreeSet<WeaponClass> = node
        .effects
        .iter()
        .filter_map(|e| match e {
            SkillEffect::ClassModifier { class, .. } => Some(*class),
            _ => None,
        })
        .collect();
    let perk: Vec<_> = bench
        .guns
        .iter()
        .filter(|(class, _)| classes.contains(class))
        .map(|(_, loadout)| *loadout)
        .collect();
    if !perk.is_empty() {
        return perk;
    }
    let all: Vec<_> = bench.guns.iter().map(|(_, loadout)| *loadout).collect();
    if all.is_empty() {
        vec![bench.starter]
    } else {
        all
    }
}

/// One wave-pool archetype at a reference rank, already scaled by the
/// rank's enemy health and damage.
struct Target {
    weight: f64,
    hp: i32,
    armor: i32,
    ranged_armor: i32,
    contact_armor: i32,
    /// One hit on the survivor before armor: the contact strike or spit,
    /// or a fused exploder's blast.
    survivor_hit: i32,
    structure_hit: i32,
    /// The survivor's hit is hazard damage (a spit or a blast), which the
    /// hazard multiplier scales.
    hazard: bool,
}

/// The progression snapshot and wave pool at one rank, each archetype
/// weighted by spawn weight times group size.
struct Context {
    snapshot: ProgressionSnapshot,
    pool: Vec<Target>,
}
impl Context {
    fn at(rank: u32) -> Self {
        let snapshot = crate::progression::catalog().snapshot(rank);
        let catalog = enemy_catalog();
        let pool = catalog
            .wave_pool(snapshot.wave_tier, &[])
            .into_iter()
            .filter_map(|entry| {
                let e = catalog.resolve(&entry.archetype_id).ok()?;
                let scale = |base: i32| (base as f32 * snapshot.enemy_damage).round() as i32;
                let hit = scale(e.damage);
                let fused = match &e.death_effect {
                    Some(DeathEffect::Burst {
                        damage, fuse: true, ..
                    }) => Some(*damage),
                    _ => None,
                };
                Some(Target {
                    // Each weighted pick spawns `group_size` members
                    // (`Director::spawn_group`).
                    weight: f64::from(entry.weight) * f64::from(e.group_size.max(1)),
                    hp: (e.health as f32 * snapshot.enemy_health).round().max(1.0) as i32,
                    armor: e.armor,
                    ranged_armor: e.ranged_armor,
                    contact_armor: (e.armor as f32 * e.contact_armor_fraction).floor() as i32,
                    // A fused exploder detonates in contact instead of
                    // attacking (sim/combat.rs `detonate_fuse`).
                    survivor_hit: fused.map_or(hit, scale),
                    structure_hit: hit.saturating_mul(i32::from(e.structure_damage_multiplier)),
                    hazard: fused.is_some() || e.reach > 0.0,
                })
            })
            .collect();
        Self { snapshot, pool }
    }
    /// The pool-weighted mean of `f`; 0 for an empty pool.
    fn mean(&self, f: impl Fn(&Target) -> f64) -> f64 {
        let weight: f64 = self.pool.iter().map(|t| t.weight).sum();
        if weight <= 0.0 {
            return 0.0;
        }
        self.pool.iter().map(|t| t.weight * f(t)).sum::<f64>() / weight
    }
    /// The ln metric of `axis` under `m`.
    fn metric(&self, axis: ValueAxis, m: &SkillModifiers, guns: &[[u64; 4]]) -> f64 {
        match axis {
            ValueAxis::Firepower => {
                let bench = bench();
                guns.iter()
                    .map(|loadout| self.kill_rate(&bench.profile(m, *loadout)).ln())
                    .sum::<f64>()
                    / guns.len().max(1) as f64
            }
            ValueAxis::Survival => self.survival(m),
            ValueAxis::Melee => self.melee(m),
            ValueAxis::Ordnance => self.ordnance(m),
            ValueAxis::Fortification => self.fortification(m),
        }
    }
    /// Pool-weighted continuous kills per second (≥ the floor).
    fn kill_rate(&self, profile: &DefenderProfile) -> f64 {
        let p = &profile.personal;
        let weapon = &profile.weapon;
        let pellets = f64::from(weapon.pellets.max(1));
        let weak_share = f64::from(REFERENCE_WEAK_HIT_SHARE);
        let weak_raw = (p.damage as f32 * (1.0 + p.weak_bonus)).round() as i32;
        let cadence = f64::from(p.cadence.max(0.08));
        let reload = f64::from(p.reload.max(0.5));
        let magazine = f64::from(p.magazine.max(1));
        let time = self.mean(|t| {
            let armor = (t.armor + t.ranged_armor - profile.modifiers.armor_penetration).max(0);
            let trigger = |raw: i32| {
                let pellet = f64::from((raw - armor).max(0).min(t.hp));
                (pellet * pellets).min(f64::from(weapon.hit_cap(t.hp)))
            };
            let expected =
                ((1.0 - weak_share) * trigger(p.damage) + weak_share * trigger(weak_raw)).max(1e-3);
            let shots = f64::from(t.hp) / expected;
            shots * cadence + shots / magazine * reload
        });
        if time > 0.0 {
            (1.0 / time).max(METRIC_FLOOR)
        } else {
            METRIC_FLOOR
        }
    }
    fn survival(&self, m: &SkillModifiers) -> f64 {
        let p = bench().profile(m, bench().starter).personal;
        let hazard = m.hazard_damage_multiplier;
        let mean_hit = self.mean(|t| {
            let raw = if t.hazard && hazard != 1.0 {
                (t.survivor_hit as f32 * hazard).round() as i32
            } else {
                t.survivor_hit
            };
            f64::from((raw - p.armor).max(0))
        });
        (f64::from(p.health.max(1)) / mean_hit.max(0.25)).ln()
    }
    fn melee(&self, m: &SkillModifiers) -> f64 {
        let p = bench().profile(m, bench().starter).personal;
        let kill_fraction = self.mean(|t| {
            (f64::from((p.melee_damage - t.contact_armor).max(0)) / f64::from(t.hp)).min(1.0)
        });
        let strikes = f64::from(p.stamina.max(0)) / STAMINA_PER_STRIKE;
        (kill_fraction.max(METRIC_FLOOR) * strikes.max(METRIC_FLOOR)).ln()
    }
    fn ordnance(&self, m: &SkillModifiers) -> f64 {
        let fortification = self.snapshot.fortification_multiplier;
        let mut total = 0.0;
        for kind in [OrdnanceKind::Grenade, OrdnanceKind::Molotov] {
            if !m.unlocked(kind.unlock()) {
                continue;
            }
            let d = crate::ordnance::catalog().definition(kind);
            let multiplier = m.ordnance_damage_multiplier * fortification;
            let scaled = |base: i32| {
                if base > 0 {
                    (base as f32 * multiplier).round().clamp(1.0, 100_000.0) as i32
                } else {
                    0
                }
            };
            let impact = scaled(d.damage);
            let burn = scaled(d.burn_damage);
            let burn_ticks = if d.burn_ticks > 0 {
                (d.burn_ticks as f32 * m.burn_duration_multiplier)
                    .round()
                    .clamp(1.0, 3600.0)
            } else {
                0.0
            };
            let pulses = f64::from(burn_ticks) / f64::from(d.burn_interval.max(1));
            let radius = f64::from((d.radius * m.ordnance_radius_multiplier).clamp(0.1, 15.0));
            let charges = f64::from(match kind {
                OrdnanceKind::Grenade => m.grenade_charges,
                OrdnanceKind::Molotov => m.molotov_charges,
            });
            let damage = self.mean(|t| {
                (f64::from((impact - t.armor).max(0)) + pulses * f64::from((burn - t.armor).max(0)))
                    .min(f64::from(t.hp))
            });
            total += charges * damage * radius * radius;
        }
        total.max(METRIC_FLOOR).ln()
    }
    fn fortification(&self, m: &SkillModifiers) -> f64 {
        let engineering = if m.engineering {
            m.engineering_multiplier
        } else {
            1.0
        };
        let mut value = f64::from(engineering).ln();
        for kind in StructureKind::ALL {
            if !m.blueprints[kind] {
                continue;
            }
            let d = content().structure(kind);
            let rank = m.structure_ranks[kind];
            // `sim::npc::structure_bonus`: the skill engineering multiplier
            // scales damage and durability with the rank and fortification.
            let bonus = (1.0 + rank as f32 * 0.2)
                * self.snapshot.fortification_multiplier
                * m.engineering_multiplier;
            let damage = (d.damage as f32 * bonus).round() as i32;
            let kill_fraction = |ranged: bool| {
                self.mean(|t| {
                    let armor = t.armor + if ranged { t.ranged_armor } else { 0 };
                    (f64::from((damage - armor).max(0)) / f64::from(t.hp)).min(1.0)
                })
            };
            let term = match kind {
                StructureKind::Trap => f64::from(d.charges_at_rank(rank)) * kill_fraction(false),
                StructureKind::Turret => {
                    kill_fraction(true) / f64::from(d.cooldown.max(1.0 / 60.0))
                }
                StructureKind::Wall => {
                    let durability = f64::from((d.health as f32 * bonus).round());
                    durability / self.mean(|t| f64::from(t.structure_hit.max(0))).max(0.25)
                }
            };
            value += term.max(METRIC_FLOOR).ln();
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_weights_count_every_member_of_a_group_pick() {
        // The director spawns `group_size` members per weighted pick, so a
        // sprinter pick (group 4) puts four bodies on the field.
        let catalog = enemy_catalog();
        let tier = |rank: u32| crate::progression::catalog().snapshot(rank).wave_tier;
        let rank = (0..=40)
            .find(|&r| {
                catalog.wave_pool(tier(r), &[]).iter().any(|e| {
                    catalog
                        .resolve(&e.archetype_id)
                        .is_ok_and(|e| e.group_size > 1)
                })
            })
            .expect("a grouped archetype spawns at some rank");
        let context = Context::at(rank);
        let pool = catalog.wave_pool(context.snapshot.wave_tier, &[]);
        assert_eq!(pool.len(), context.pool.len());
        for (entry, target) in pool.iter().zip(&context.pool) {
            let group = catalog.resolve(&entry.archetype_id).unwrap().group_size;
            assert_eq!(
                target.weight,
                f64::from(entry.weight) * f64::from(group),
                "rank {rank}: {}",
                entry.archetype_id
            );
        }
    }

    #[test]
    fn structure_rank_gains_use_the_engineered_damage_and_durability() {
        // The sim scales structure damage and durability by
        // `sim::npc::structure_bonus`: rank, progression fortification and
        // the skill engineering multiplier. Each rank 1 owns Engineering
        // (x1.25) through its blueprint.
        for (id, kind) in [
            ("trap_rank_1", StructureKind::Trap),
            ("turret_rank_1", StructureKind::Turret),
            ("wall_rank_1", StructureKind::Wall),
        ] {
            let node = skills().node(id).expect("shipped skill node");
            let before = fold(node, false);
            assert!(before.engineering && before.engineering_multiplier > 1.0);
            assert_eq!(before.structure_ranks[kind], 0, "{id}");
            let d = content().structure(kind);
            for rank in reference_ranks(node) {
                let c = Context::at(rank);
                let term = |r: u32| {
                    let bonus = (1.0 + r as f32 * 0.2)
                        * c.snapshot.fortification_multiplier
                        * before.engineering_multiplier;
                    let damage = (d.damage as f32 * bonus).round() as i32;
                    let kill = |ranged: bool| {
                        c.mean(|t| {
                            let armor = t.armor + if ranged { t.ranged_armor } else { 0 };
                            (f64::from((damage - armor).max(0)) / f64::from(t.hp)).min(1.0)
                        })
                    };
                    let term = match kind {
                        StructureKind::Trap => f64::from(d.charges_at_rank(r)) * kill(false),
                        StructureKind::Turret => kill(true) / f64::from(d.cooldown.max(1.0 / 60.0)),
                        StructureKind::Wall => {
                            f64::from((d.health as f32 * bonus).round())
                                / c.mean(|t| f64::from(t.structure_hit.max(0))).max(0.25)
                        }
                    };
                    term.max(METRIC_FLOOR).ln()
                };
                let expected = (term(1) - term(0)) as f32;
                let gain = node_value_at(node, rank).expect("valued");
                assert!(
                    (gain - expected).abs() < 1e-5,
                    "{id} at rank {rank}: gain {gain} != engineered {expected}"
                );
            }
        }
    }

    #[test]
    fn survival_counts_a_fused_burst_as_a_hazard_hit() {
        // A fused exploder detonates in contact instead of attacking, and
        // the survivor's hazard multiplier scales the blast (sim/combat.rs
        // `detonate_fuse`, `hazard_scaled`), as it scales a spit.
        let node = skills()
            .node("blast_discipline")
            .expect("shipped skill node");
        let before = fold(node, false);
        let after = fold(node, true);
        let hazard = after.hazard_damage_multiplier;
        assert!(hazard < 1.0);
        let armor = bench().profile(&before, bench().starter).personal.armor;
        let catalog = enemy_catalog();
        let mut fused_seen = false;
        for rank in reference_ranks(node) {
            let snapshot = crate::progression::catalog().snapshot(rank);
            let (mut weight, mut sum_before, mut sum_after) = (0.0, 0.0, 0.0);
            for entry in catalog.wave_pool(snapshot.wave_tier, &[]) {
                let e = catalog.resolve(&entry.archetype_id).expect("resolves");
                let scale = |base: i32| (base as f32 * snapshot.enemy_damage).round() as i32;
                let (raw, is_hazard) = match &e.death_effect {
                    Some(DeathEffect::Burst {
                        damage, fuse: true, ..
                    }) => {
                        fused_seen = true;
                        (scale(*damage), true)
                    }
                    _ => (scale(e.damage), e.reach > 0.0),
                };
                let hit = |m: f32| {
                    let raw = if is_hazard && m != 1.0 {
                        (raw as f32 * m).round() as i32
                    } else {
                        raw
                    };
                    f64::from((raw - armor).max(0))
                };
                let w = f64::from(entry.weight) * f64::from(e.group_size);
                weight += w;
                sum_before += w * hit(1.0);
                sum_after += w * hit(hazard);
            }
            let expected =
                ((sum_before / weight).max(0.25) / (sum_after / weight).max(0.25)).ln() as f32;
            let gain = node_value_at(node, rank).expect("valued");
            assert!(
                (gain - expected).abs() < 1e-5,
                "rank {rank}: gain {gain} != {expected}"
            );
        }
        assert!(fused_seen, "a fused exploder spawns at a reference rank");
    }
}
