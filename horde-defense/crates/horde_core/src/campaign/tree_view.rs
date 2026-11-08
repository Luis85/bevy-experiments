//! Read-only skill-tree presentation queries: node state with a requirement
//! checklist, the purchase path to a node, a typed stat preview and the node
//! a screen should focus. Every answer is derived from the same kernel as
//! `can_purchase_node`/`purchase_node`; screens only render. All iteration is
//! in catalogue order, so results are deterministic.
use super::loadout::coordination_reload_factor;
use super::{BodyAttribute, Campaign, NodePayment, RecommendationAction};
use crate::content::{Mode, StructureKind};
use crate::skill_catalog::{
    CampaignFeature, ClassStat, SkillEffect, SkillNode, skills, unlock_label,
};
use crate::weapons::WeaponClass;
use std::collections::{BTreeMap, BTreeSet};

/// Purchase state of one node for the tree canvas and detail panel.
///
/// `Buyable` holds exactly when `can_purchase_node` is `Ok`. Gates are
/// reported in this order: ownership, the free starter root, prerequisites,
/// milestones, payment, coins, then the preparation (active deployment) check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeState {
    Owned,
    /// Purchasable now at this payment.
    Buyable(NodePayment),
    /// Every gate is met except the price; `short` coins are missing.
    NeedsCoins {
        payment: NodePayment,
        short: u64,
    },
    /// Direct prerequisites not yet owned, as node ids in catalogue order.
    NeedsPrerequisites {
        missing: Vec<String>,
    },
    /// Prerequisites are owned but a milestone is unmet. Each field is
    /// `(have, need)` and is `Some` only when that milestone is unmet.
    NeedsMilestone {
        victories: Option<(u64, u64)>,
        liberated: Option<(u32, u32)>,
    },
    /// Cannot be bought for another reason (unknown id, the innate starter
    /// root, an exhausted legacy ledger, or an active deployment). `reason`
    /// is player wording.
    Unavailable {
        reason: String,
    },
}

impl NodeState {
    pub fn is_buyable(&self) -> bool {
        matches!(self, Self::Buyable(_))
    }
}

/// What a requirement checklist row is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequirementKind {
    /// Owning this direct prerequisite node id.
    Prerequisite(String),
    /// The node's `minimum_victories`.
    Victories,
    /// The node's `minimum_liberated`.
    Liberated,
    /// The coin price.
    Coins,
    /// The legacy body-point price (legacy profiles only).
    LegacyBodyPoints,
}

/// One checklist row: player wording plus whether it is met.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    pub kind: RequirementKind,
    /// Sentence case, no trailing period, for example "Learn Steady hands",
    /// "Win 3 defenses (1 / 3)", "Liberate 2 countries (0 / 2)" or
    /// "40 coins (have 25)".
    pub label: String,
    pub met: bool,
}

/// The unowned chain that leads to a node.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodePath {
    /// Unowned ancestors followed by the target, in a valid purchase order:
    /// each step's prerequisites are owned or appear earlier. Ties are broken
    /// by catalogue order. Empty when the target is owned or unknown.
    pub steps: Vec<String>,
    /// Sum of the catalogue prices of every step.
    pub total_coins: u64,
    /// Label of the first unmet milestone along `steps`, if any
    /// (for example "Win 2 defenses (0 / 2)").
    pub blocking: Option<String>,
}

/// A stat a node can change, with its label and value formatting.
///
/// Qualitative grants (weapon proficiency, actives, blueprints, features)
/// appear as `Unlock(name)` with `before` 0 and `after` 1.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PreviewStat {
    /// Personal max health, gear and body ranks included.
    MaxHealth,
    /// Personal max stamina, gear and body ranks included.
    Stamina,
    /// Personal armor, gear included.
    Armor,
    /// Personal melee damage per strike, gear and Strength included.
    MeleeDamage,
    /// Extra damage on personal weak-spot hits, as a fraction (0.15 = +15%).
    WeakSpotBonus,
    /// Flat personal firearm damage added per projectile.
    DamageBonus,
    /// Personal firearm damage multiplier.
    DamageMultiplier,
    /// Personal reload-duration multiplier from skills and Coordination
    /// (the 0.65 s minimum reload still applies).
    ReloadTime,
    /// Personal shot-interval multiplier (lower fires faster).
    ShotInterval,
    /// One weapon class's perk: a damage, reload or shot-interval
    /// multiplier, or an added weak-spot bonus fraction.
    ClassStat(WeaponClass, ClassStat),
    /// Enemy armor ignored by personal shots.
    ArmorPenetration,
    /// Multiplier on hazard (blast and spit) damage taken.
    HazardDamage,
    GrenadeCharges,
    MolotovCharges,
    OrdnanceDamage,
    OrdnanceRadius,
    BurnDuration,
    OrdnanceCooldown,
    /// Shared engineering multiplier on structure durability and damage.
    StructurePower,
    /// Mastery rank of one structure type (each rank adds 20%).
    StructureRank(StructureKind),
    /// Body-development rank of one attribute.
    BodyRank(BodyAttribute),
    /// A capability gained, named for the player.
    Unlock(String),
}

impl PreviewStat {
    /// Sentence-case stat name.
    pub fn label(&self) -> String {
        match self {
            Self::MaxHealth => "Max health".into(),
            Self::Stamina => "Max stamina".into(),
            Self::Armor => "Armor".into(),
            Self::MeleeDamage => "Melee damage".into(),
            Self::WeakSpotBonus => "Weak-spot damage".into(),
            Self::DamageBonus => "Firearm damage bonus".into(),
            Self::DamageMultiplier => "Firearm damage".into(),
            Self::ReloadTime => "Reload time".into(),
            Self::ShotInterval => "Time between shots".into(),
            Self::ClassStat(class, stat) => format!(
                "{} {}",
                class.name(),
                match stat {
                    ClassStat::Damage => "damage",
                    ClassStat::Reload => "reload time",
                    ClassStat::Cadence => "time between shots",
                    ClassStat::WeakBonus => "weak-spot damage",
                }
            ),
            Self::ArmorPenetration => "Armor penetration".into(),
            Self::HazardDamage => "Blast and spit damage taken".into(),
            Self::GrenadeCharges => "Grenade charges".into(),
            Self::MolotovCharges => "Molotov charges".into(),
            Self::OrdnanceDamage => "Ordnance damage".into(),
            Self::OrdnanceRadius => "Ordnance radius".into(),
            Self::BurnDuration => "Burn duration".into(),
            Self::OrdnanceCooldown => "Ordnance cooldown".into(),
            Self::StructurePower => "Structure strength".into(),
            Self::StructureRank(kind) => format!("{} mastery", kind.label()),
            Self::BodyRank(attribute) => format!("{} rank", attribute.label()),
            Self::Unlock(name) => name.clone(),
        }
    }
    /// The value as the player reads it: integers for counts and ranks,
    /// "+8" for flat bonuses, "×0.80" for multipliers, "+15%" for the
    /// weak-spot bonus and "Unlocked"/"Locked" for unlocks.
    pub fn format(&self, value: f32) -> String {
        match self {
            Self::DamageBonus => format!("+{}", value.round() as i64),
            Self::WeakSpotBonus | Self::ClassStat(_, ClassStat::WeakBonus) => {
                format!("+{}%", (value * 100.0).round() as i64)
            }
            Self::DamageMultiplier
            | Self::ReloadTime
            | Self::ShotInterval
            | Self::ClassStat(..)
            | Self::HazardDamage
            | Self::OrdnanceDamage
            | Self::OrdnanceRadius
            | Self::BurnDuration
            | Self::OrdnanceCooldown
            | Self::StructurePower => format!("×{value:.2}"),
            Self::Unlock(_) => if value >= 0.5 { "Unlocked" } else { "Locked" }.into(),
            Self::MaxHealth
            | Self::Stamina
            | Self::Armor
            | Self::MeleeDamage
            | Self::ArmorPenetration
            | Self::GrenadeCharges
            | Self::MolotovCharges
            | Self::StructureRank(_)
            | Self::BodyRank(_) => format!("{}", value.round() as i64),
        }
    }
}

/// One changed stat: `before` is the current campaign, `after` the same
/// campaign with the node owned.
#[derive(Clone, Debug, PartialEq)]
pub struct StatDelta {
    pub stat: PreviewStat,
    pub before: f32,
    pub after: f32,
}

impl StatDelta {
    /// One-line summary, for example "Max health 100 → 135" or
    /// "Unlocks Shotgun proficiency".
    pub fn summary(&self) -> String {
        match &self.stat {
            PreviewStat::Unlock(name) => format!("Unlocks {name}"),
            stat => format!(
                "{} {} → {}",
                stat.label(),
                stat.format(self.before),
                stat.format(self.after)
            ),
        }
    }
}

/// "1 defense" / "3 defenses".
fn count(n: u64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn victories_label(have: u64, need: u64) -> String {
    format!(
        "Win {} ({} / {need})",
        count(need, "defense", "defenses"),
        have.min(need)
    )
}

fn liberated_label(have: u32, need: u32) -> String {
    format!(
        "Liberate {} ({} / {need})",
        count(need.into(), "country", "countries"),
        have.min(need)
    )
}

/// Stat values of one campaign state, in [`PreviewStat`] order, plus the
/// names of everything it has unlocked.
fn stat_snapshot(c: &Campaign) -> (Vec<(PreviewStat, f32)>, BTreeSet<String>) {
    let modifiers = c.modifiers();
    let personal = c.profile(Mode::Personal).personal;
    let rank = |a: BodyAttribute| c.player.body[a].max(modifiers.body_development[a]);
    let mut stats = vec![
        (PreviewStat::MaxHealth, personal.health as f32),
        (PreviewStat::Stamina, personal.stamina as f32),
        (PreviewStat::Armor, personal.armor as f32),
        (PreviewStat::MeleeDamage, personal.melee_damage as f32),
        (PreviewStat::WeakSpotBonus, personal.weak_bonus),
        (PreviewStat::DamageBonus, modifiers.damage_add as f32),
        (PreviewStat::DamageMultiplier, modifiers.damage_multiplier),
        (
            PreviewStat::ReloadTime,
            modifiers.reload_multiplier
                * coordination_reload_factor(rank(BodyAttribute::Coordination)),
        ),
        (PreviewStat::ShotInterval, modifiers.cadence_multiplier),
    ];
    for class in WeaponClass::ALL {
        let perk = modifiers
            .class_modifiers
            .get(&class)
            .copied()
            .unwrap_or_default();
        stats.extend(
            ClassStat::ALL.map(|stat| (PreviewStat::ClassStat(class, stat), perk.get(stat))),
        );
    }
    stats.extend([
        (
            PreviewStat::ArmorPenetration,
            modifiers.armor_penetration as f32,
        ),
        (
            PreviewStat::HazardDamage,
            modifiers.hazard_damage_multiplier,
        ),
        (
            PreviewStat::GrenadeCharges,
            modifiers.grenade_charges as f32,
        ),
        (
            PreviewStat::MolotovCharges,
            modifiers.molotov_charges as f32,
        ),
        (
            PreviewStat::OrdnanceDamage,
            modifiers.ordnance_damage_multiplier,
        ),
        (
            PreviewStat::OrdnanceRadius,
            modifiers.ordnance_radius_multiplier,
        ),
        (
            PreviewStat::BurnDuration,
            modifiers.burn_duration_multiplier,
        ),
        (
            PreviewStat::OrdnanceCooldown,
            modifiers.ordnance_cooldown_multiplier,
        ),
        (
            PreviewStat::StructurePower,
            modifiers.engineering_multiplier,
        ),
    ]);
    stats.extend(StructureKind::ALL.map(|kind| {
        (
            PreviewStat::StructureRank(kind),
            c.structure_upgrades[kind] as f32,
        )
    }));
    stats.extend(BodyAttribute::ALL.map(|a| (PreviewStat::BodyRank(a), rank(a) as f32)));
    let mut unlocked = BTreeSet::new();
    for class in &modifiers.weapon_classes {
        unlocked.insert(format!("{} proficiency", class.name()));
    }
    for id in &modifiers.unlocks {
        unlocked.insert(unlock_label(id.as_str()).to_owned());
    }
    for feature in CampaignFeature::ALL {
        if c.has_feature(feature) {
            unlocked.insert(feature.label().to_owned());
        }
    }
    for kind in StructureKind::ALL {
        if c.blueprints[kind] {
            unlocked.insert(format!("{} blueprint", kind.label()));
        }
    }
    (stats, unlocked)
}

const LEGACY_BODY_NOTE: &str = "Body ranks you allocated before the skill tree count as owned, so you never pay for the same rank twice.";

impl Campaign {
    /// The node's purchase state; see [`NodeState`]. Unknown ids are
    /// `Unavailable`.
    pub fn node_state(&self, id: &str) -> NodeState {
        let Some(node) = skills().node(id) else {
            return NodeState::Unavailable {
                reason: "Unknown skill node".into(),
            };
        };
        if self.owns_node_definition(node) {
            return NodeState::Owned;
        }
        if node.price == 0 {
            // can_purchase_node's own wording for the innate root.
            return NodeState::Unavailable {
                reason: self.can_purchase_node(id).unwrap_err(),
            };
        }
        let missing = self.missing_prerequisites(node);
        if !missing.is_empty() {
            return NodeState::NeedsPrerequisites { missing };
        }
        let (victories, liberated) = self.milestone_gaps(node);
        if victories.is_some() || liberated.is_some() {
            return NodeState::NeedsMilestone {
                victories,
                liberated,
            };
        }
        let payment = match self.node_payment(id) {
            Ok(payment) => payment,
            Err(reason) => return NodeState::Unavailable { reason },
        };
        if let NodePayment::Coins(price) = payment
            && self.coins < price
        {
            return NodeState::NeedsCoins {
                payment,
                short: price - self.coins,
            };
        }
        match self.can_purchase_node(id) {
            Ok(()) => NodeState::Buyable(payment),
            Err(reason) => NodeState::Unavailable { reason },
        }
    }

    /// Direct unowned prerequisites of `node`, in catalogue order.
    fn missing_prerequisites(&self, node: &SkillNode) -> Vec<String> {
        skills()
            .nodes
            .iter()
            .filter(|n| node.prerequisites.contains(&n.id) && !self.owns_node_definition(n))
            .map(|n| n.id.clone())
            .collect()
    }

    /// `(have, need)` for each unmet milestone, using the same counters as
    /// `node_requirement_error`.
    #[allow(clippy::type_complexity)]
    fn milestone_gaps(&self, node: &SkillNode) -> (Option<(u64, u64)>, Option<(u32, u32)>) {
        let liberated = self.liberated_count();
        (
            (self.victories < node.minimum_victories)
                .then_some((self.victories, node.minimum_victories)),
            (liberated < node.minimum_liberated).then_some((liberated, node.minimum_liberated)),
        )
    }

    /// Label of the first unmet milestone of `node` (liberation first, as
    /// in the purchase preflight).
    fn milestone_block(&self, node: &SkillNode) -> Option<String> {
        match self.milestone_gaps(node) {
            (_, Some((have, need))) => Some(liberated_label(have, need)),
            (Some((have, need)), None) => Some(victories_label(have, need)),
            (None, None) => None,
        }
    }

    /// Checklist for the detail panel: every direct prerequisite (catalogue
    /// order), then the liberation and victory milestones when the node has
    /// them, then the price unless the node is owned or free. Unknown ids
    /// return an empty list.
    pub fn node_requirements(&self, id: &str) -> Vec<Requirement> {
        let Some(node) = skills().node(id) else {
            return Vec::new();
        };
        let mut rows: Vec<Requirement> = skills()
            .nodes
            .iter()
            .filter(|n| node.prerequisites.contains(&n.id))
            .map(|n| Requirement {
                kind: RequirementKind::Prerequisite(n.id.clone()),
                label: format!("Learn {}", n.name),
                met: self.owns_node_definition(n),
            })
            .collect();
        if node.minimum_liberated > 0 {
            let have = self.liberated_count();
            rows.push(Requirement {
                kind: RequirementKind::Liberated,
                label: liberated_label(have, node.minimum_liberated),
                met: have >= node.minimum_liberated,
            });
        }
        if node.minimum_victories > 0 {
            rows.push(Requirement {
                kind: RequirementKind::Victories,
                label: victories_label(self.victories, node.minimum_victories),
                met: self.victories >= node.minimum_victories,
            });
        }
        if node.price > 0 && !self.owns_node_definition(node) {
            match self.node_payment(id) {
                Ok(NodePayment::Coins(price)) => rows.push(Requirement {
                    kind: RequirementKind::Coins,
                    label: format!("{price} coins (have {})", self.coins),
                    met: self.coins >= price,
                }),
                Ok(NodePayment::LegacyBodyPoints(points)) => rows.push(Requirement {
                    kind: RequirementKind::LegacyBodyPoints,
                    label: format!(
                        "{} (have {})",
                        count(points.into(), "legacy body point", "legacy body points"),
                        self.player.points
                    ),
                    met: self.player.points >= points,
                }),
                Err(_) => {}
            }
        }
        rows
    }

    /// Unowned ancestors plus the target in purchase order; see [`NodePath`].
    pub fn path_to_node(&self, id: &str) -> NodePath {
        let nodes = &skills().nodes;
        let Some(target) = nodes.iter().position(|n| n.id == id) else {
            return NodePath::default();
        };
        let index: BTreeMap<&str, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        // Unowned ancestors, by catalogue index. Owned nodes end the walk.
        let mut wanted = BTreeSet::new();
        let mut stack = vec![target];
        while let Some(i) = stack.pop() {
            if self.owns_node_definition(&nodes[i]) || !wanted.insert(i) {
                continue;
            }
            stack.extend(nodes[i].prerequisites.iter().map(|p| index[p.as_str()]));
        }
        // Kahn's order over the wanted set, lowest catalogue index first.
        let mut pending: BTreeMap<usize, usize> = wanted
            .iter()
            .map(|&i| {
                let inside = nodes[i]
                    .prerequisites
                    .iter()
                    .filter(|p| wanted.contains(&index[p.as_str()]))
                    .count();
                (i, inside)
            })
            .collect();
        let mut ready: BTreeSet<usize> = pending
            .iter()
            .filter_map(|(&i, &n)| (n == 0).then_some(i))
            .collect();
        let mut path = NodePath::default();
        while let Some(i) = ready.pop_first() {
            pending.remove(&i);
            let node = &nodes[i];
            path.steps.push(node.id.clone());
            path.total_coins = path.total_coins.saturating_add(node.price);
            if path.blocking.is_none() {
                path.blocking = self.milestone_block(node);
            }
            for (&child, remaining) in pending.iter_mut() {
                if nodes[child].prerequisites.contains(&node.id) {
                    *remaining -= 1;
                    if *remaining == 0 {
                        ready.insert(child);
                    }
                }
            }
        }
        path
    }

    /// Personal stats that owning the node would change, in [`PreviewStat`]
    /// order, followed by its unlocks. Computed by deriving the real personal
    /// profile and skill modifiers with and without the node; owned and
    /// unknown nodes return an empty list.
    pub fn node_stat_preview(&self, id: &str) -> Vec<StatDelta> {
        let Some(node) = skills().node(id) else {
            return Vec::new();
        };
        if self.owns_node_definition(node) {
            return Vec::new();
        }
        let mut owned = self.clone();
        owned.grant_node(node);
        let (before, had) = stat_snapshot(self);
        let (after, has) = stat_snapshot(&owned);
        let mut deltas: Vec<StatDelta> = before
            .into_iter()
            .zip(after)
            .filter(|((_, b), (_, a))| (a - b).abs() > 1e-6)
            .map(|((stat, before), (_, after))| StatDelta {
                stat,
                before,
                after,
            })
            .collect();
        deltas.extend(has.difference(&had).map(|name| StatDelta {
            stat: PreviewStat::Unlock(name.clone()),
            before: 0.0,
            after: 1.0,
        }));
        deltas
    }

    /// Whether the survivor owns a non-npc_only Ranged item of `class`.
    /// Mara's starter guns are not the survivor's.
    fn survivor_owns_gun_of(&self, class: crate::weapons::WeaponClass) -> bool {
        self.items.iter().any(|item| {
            item.definition().slot == crate::content::Slot::Ranged
                && !item.definition().npc_only
                && !self.npc.loadout.contains(&item.id)
                && crate::weapons::WeaponSpec::from_item(item.definition()).class == class
        })
    }

    /// How much `node` would do for `mode` right now: 2 when an effect acts
    /// at once, 1 for an incremental body rank (or a marginal ordnance
    /// cooldown), 0 when nothing can act yet (a weapon class or class perk
    /// without its gun, construction or structure upgrades without a
    /// blueprint, a personal effect for Mara).
    fn node_value(&self, node: &SkillNode, mode: Mode) -> u8 {
        let personal = mode == Mode::Personal;
        let any_blueprint = self.blueprints.iter().any(|owned| *owned);
        let liberated = self.liberated_count();
        let blueprint_can_follow = skills().nodes.iter().any(|n| {
            n.effects
                .iter()
                .any(|e| matches!(e, SkillEffect::Blueprint(_)))
                && self.victories >= n.minimum_victories
                && liberated >= n.minimum_liberated
        });
        node.effects
            .iter()
            .map(|effect| match effect {
                SkillEffect::BodyDevelopment { .. }
                | SkillEffect::OrdnanceCooldownMultiplier(_) => u8::from(personal),
                SkillEffect::FeatureUnlock(CampaignFeature::Construction) => {
                    if any_blueprint || blueprint_can_follow {
                        2
                    } else {
                        0
                    }
                }
                SkillEffect::FeatureUnlock(_) | SkillEffect::Blueprint(_) => 2,
                SkillEffect::StructureRank { kind, .. } => {
                    if self.blueprints[*kind] {
                        2
                    } else {
                        0
                    }
                }
                SkillEffect::EngineeringMultiplier(_) => {
                    if any_blueprint {
                        2
                    } else {
                        0
                    }
                }
                SkillEffect::WeaponTraining(class) | SkillEffect::ClassModifier { class, .. } => {
                    if personal && self.survivor_owns_gun_of(*class) {
                        2
                    } else {
                        0
                    }
                }
                _ => {
                    if personal {
                        2
                    } else {
                        0
                    }
                }
            })
            .max()
            .unwrap_or(0)
    }

    /// Whether learning `id` would act at once for `mode` (see the
    /// recommendation). Unknown ids never help.
    pub fn node_helps_now(&self, id: &str, mode: Mode) -> bool {
        skills()
            .node(id)
            .is_some_and(|node| self.node_value(node, mode) == 2)
    }

    /// The buyable node worth learning now: the most useful class first
    /// (acts now, then body ranks), cheapest within it, catalogue order on
    /// ties. Never a node that cannot act yet.
    pub(super) fn best_buyable_node(&self, mode: Mode) -> Option<&'static SkillNode> {
        skills()
            .nodes
            .iter()
            .filter(|n| n.price > 0 && self.node_state(&n.id).is_buyable())
            .map(|n| (n, self.node_value(n, mode)))
            .filter(|(_, value)| *value > 0)
            .min_by_key(|(n, value)| (std::cmp::Reverse(*value), n.price))
            .map(|(n, _)| n)
    }

    /// The node a tree screen should focus: the campaign recommendation when
    /// it is an unowned node, else the best buyable node (see
    /// [`Self::best_buyable_node`]), else the useful node with the cheapest
    /// path free of milestone blocks, else the cheapest buyable node. Ties go
    /// to catalogue order. Never an owned node or the free starter root.
    pub fn recommended_node(&self, mode: Mode) -> Option<String> {
        let nodes = &skills().nodes;
        let unowned = |id: &str| {
            skills()
                .node(id)
                .is_some_and(|n| n.price > 0 && !self.owns_node_definition(n))
        };
        let suggested = match self.recommendation(mode).action {
            RecommendationAction::LearnNode(id) => Some(id),
            RecommendationAction::LearnSkill(skill) => Some(skill.id().to_owned()),
            _ => None,
        };
        if let Some(id) = suggested.filter(|id| unowned(id)) {
            return Some(id);
        }
        self.best_buyable_node(mode)
            .or_else(|| {
                nodes
                    .iter()
                    .filter(|n| unowned(&n.id))
                    .map(|n| (n, self.node_value(n, mode), self.path_to_node(&n.id)))
                    .filter(|(_, value, path)| *value > 0 && path.blocking.is_none())
                    .min_by_key(|(_, value, path)| (std::cmp::Reverse(*value), path.total_coins))
                    .map(|(n, ..)| n)
            })
            .or_else(|| {
                nodes
                    .iter()
                    .filter(|n| n.price > 0 && self.node_state(&n.id).is_buyable())
                    .min_by_key(|n| n.price)
            })
            .map(|n| n.id.clone())
    }

    /// Unowned nodes whose victory or liberation gate the current counts
    /// have just reached (the count equals the gate), in catalogue order. A
    /// result screen names them once, on the round that opened them. Rank
    /// ladders (body and structure ranks) are left out: they are steps of a
    /// ladder already shown, not new things to learn.
    pub fn milestone_opened_nodes(&self) -> Vec<&'static SkillNode> {
        let liberated = self.liberated_count();
        skills()
            .nodes
            .iter()
            .filter(|n| !self.owns_node_definition(n))
            .filter(|n| {
                !n.effects.iter().all(|e| {
                    matches!(
                        e,
                        SkillEffect::BodyDevelopment { .. } | SkillEffect::StructureRank { .. }
                    )
                })
            })
            .filter(|n| self.victories >= n.minimum_victories && liberated >= n.minimum_liberated)
            .filter(|n| {
                (n.minimum_victories > 0 && self.victories == n.minimum_victories)
                    || (n.minimum_liberated > 0 && liberated == n.minimum_liberated)
            })
            .collect()
    }

    /// The next missing step before defenses can be built, in player
    /// wording, or `None` when construction and a blueprint are owned.
    pub fn construction_blocker(&self) -> Option<String> {
        let nodes = &skills().nodes;
        if !self.has_feature(CampaignFeature::Construction) {
            let feature = nodes.iter().find(|n| {
                n.effects
                    .contains(&SkillEffect::FeatureUnlock(CampaignFeature::Construction))
            })?;
            return Some(format!(
                "Learn {} in Journey upgrades to build defenses.",
                feature.name
            ));
        }
        if self.blueprints.iter().any(|owned| *owned) {
            return None;
        }
        let blueprint = nodes
            .iter()
            .filter(|n| {
                n.effects
                    .iter()
                    .any(|e| matches!(e, SkillEffect::Blueprint(_)))
            })
            .min_by_key(|n| (n.minimum_liberated, n.minimum_victories, n.price))?;
        Some(match self.milestone_gaps(blueprint) {
            (_, Some((have, need))) => format!(
                "Liberate {} more {} to open the {}.",
                need - have,
                if need - have == 1 {
                    "country"
                } else {
                    "countries"
                },
                blueprint.name
            ),
            (Some((have, need)), None) => format!(
                "Win {} {} to open the {}.",
                need - have,
                if need - have == 1 {
                    "defense"
                } else {
                    "defenses"
                },
                blueprint.name
            ),
            (None, None) => format!(
                "Learn the {} in Journey upgrades to build defenses.",
                blueprint.name
            ),
        })
    }

    /// The compatibility note for body-development nodes on legacy profiles
    /// ("allocated ranks count as owned"). `None` for fresh profiles and
    /// other nodes, so fresh players never see legacy wording.
    pub fn legacy_node_note(&self, id: &str) -> Option<&'static str> {
        let node = skills().node(id)?;
        (self.legacy_features
            && node
                .effects
                .iter()
                .any(|e| matches!(e, SkillEffect::BodyDevelopment { .. })))
        .then_some(LEGACY_BODY_NOTE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::survivors::SurvivorId;

    #[test]
    fn perk_preview_stats_have_labels_formats_and_neutral_snapshot_values() {
        let shotgun = |stat| PreviewStat::ClassStat(WeaponClass::Shotgun, stat);
        assert_eq!(
            shotgun(ClassStat::Cadence).label(),
            "Shotgun time between shots"
        );
        assert_eq!(shotgun(ClassStat::Cadence).format(0.8), "×0.80");
        assert_eq!(shotgun(ClassStat::Damage).label(), "Shotgun damage");
        assert_eq!(shotgun(ClassStat::Reload).label(), "Shotgun reload time");
        assert_eq!(shotgun(ClassStat::WeakBonus).format(0.15), "+15%");
        assert_eq!(PreviewStat::ArmorPenetration.format(4.0), "4");
        assert_eq!(PreviewStat::HazardDamage.format(0.6), "×0.60");
        // Every perk stat is in the snapshot at its neutral value, so no
        // shipped node previews a change to it.
        let (stats, _) = stat_snapshot(&Campaign::default());
        let value = |stat: PreviewStat| stats.iter().find(|(s, _)| *s == stat).unwrap().1;
        for class in WeaponClass::ALL {
            for stat in ClassStat::ALL {
                let neutral = if stat == ClassStat::WeakBonus {
                    0.0
                } else {
                    1.0
                };
                assert_eq!(value(PreviewStat::ClassStat(class, stat)), neutral);
            }
        }
        assert_eq!(value(PreviewStat::ArmorPenetration), 0.0);
        assert_eq!(value(PreviewStat::HazardDamage), 1.0);
        let order: Vec<_> = stats.iter().map(|(s, _)| s.clone()).collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "PreviewStat order");
    }

    #[test]
    fn a_milestone_just_met_names_the_nodes_it_opened() {
        let mut c = started();
        assert!(c.milestone_opened_nodes().is_empty());
        c.victories = 1;
        let ids = |c: &Campaign| {
            c.milestone_opened_nodes()
                .iter()
                .map(|n| n.id.clone())
                .collect::<Vec<_>>()
        };
        assert!(ids(&c).contains(&"trap_blueprint".to_owned()));
        assert!(!ids(&c).contains(&"wall_blueprint".to_owned()));
        c.victories = 2;
        assert!(ids(&c).contains(&"wall_blueprint".to_owned()));
        assert!(
            !ids(&c).contains(&"trap_blueprint".to_owned()),
            "opened earlier"
        );
        c.coins = 1000;
        c.purchase_node("engineering").unwrap();
        c.purchase_node("trap_blueprint").unwrap();
        c.purchase_node("wall_blueprint").unwrap();
        assert!(!ids(&c).contains(&"wall_blueprint".to_owned()), "owned");
        c.victories = 3;
        assert!(
            !ids(&c)
                .iter()
                .any(|id| id.contains("_rank_") || id.starts_with("body_")),
            "rank ladders are not announced: {:?}",
            ids(&c)
        );
    }

    #[test]
    fn construction_blocker_names_the_next_missing_step() {
        let mut c = started();
        c.coins = 1000;
        assert_eq!(
            c.construction_blocker().as_deref(),
            Some("Learn Field engineering in Journey upgrades to build defenses.")
        );
        c.purchase_node("engineering").unwrap();
        assert_eq!(
            c.construction_blocker().as_deref(),
            Some("Win 1 defense to open the Trap blueprint.")
        );
        c.victories = 1;
        assert_eq!(
            c.construction_blocker().as_deref(),
            Some("Learn the Trap blueprint in Journey upgrades to build defenses.")
        );
        c.purchase_node("trap_blueprint").unwrap();
        assert_eq!(c.construction_blocker(), None);
    }

    fn started() -> Campaign {
        let mut c = Campaign::default();
        c.choose_survivor(SurvivorId::Rook).unwrap();
        c.choose_start_country("DEU").unwrap();
        c
    }

    fn price(id: &str) -> u64 {
        skills().node(id).unwrap().price
    }

    fn index(id: &str) -> usize {
        skills().nodes.iter().position(|n| n.id == id).unwrap()
    }

    /// A range of campaign states: fresh, rich, veteran, partly bought,
    /// broke, deployed and legacy.
    fn states() -> Vec<Campaign> {
        let mut states = vec![Campaign::default(), started()];
        let mut c = started();
        c.coins = 30;
        states.push(c.clone());
        c.coins = 100_000;
        states.push(c.clone());
        c.victories = 100;
        states.push(c.clone());
        for _ in 0..15 {
            let Some(next) = skills()
                .nodes
                .iter()
                .find(|n| c.can_purchase_node(&n.id).is_ok())
            else {
                break;
            };
            c.purchase_node(&next.id.clone()).unwrap();
            states.push(c.clone());
        }
        let mut broke = c.clone();
        broke.coins = 0;
        states.push(broke);
        let mut deployed = c.clone();
        deployed.reserve(Mode::Personal).unwrap();
        states.push(deployed);
        states.push(Campaign {
            legacy_features: true,
            coins: 1000,
            ..Campaign::default()
        });
        let mut legacy = Campaign {
            legacy_features: true,
            coins: 10,
            ..started()
        };
        legacy.player.points = 2;
        legacy.player.body[BodyAttribute::Vitality] = 3;
        states.push(legacy);
        states
    }

    #[test]
    fn node_state_is_buyable_exactly_when_the_purchase_preflight_accepts() {
        for c in states() {
            for node in &skills().nodes {
                let state = c.node_state(&node.id);
                assert_eq!(
                    state.is_buyable(),
                    c.can_purchase_node(&node.id).is_ok(),
                    "{}: {state:?}",
                    node.id
                );
                assert_eq!(state == NodeState::Owned, c.has_node(&node.id));
                match &state {
                    NodeState::Buyable(payment) => {
                        assert_eq!(Ok(*payment), c.node_payment(&node.id))
                    }
                    NodeState::NeedsCoins { payment, short } => {
                        let NodePayment::Coins(price) = *payment else {
                            panic!("legacy payments never lack coins")
                        };
                        assert_eq!(*short, price - c.coins);
                        assert!(*short > 0);
                    }
                    NodeState::NeedsPrerequisites { missing } => {
                        assert!(!missing.is_empty());
                        assert!(missing.iter().all(|id| !c.has_node(id)));
                        assert!(missing.iter().all(|id| node.prerequisites.contains(id)));
                        assert!(missing.windows(2).all(|w| index(&w[0]) < index(&w[1])));
                    }
                    NodeState::NeedsMilestone {
                        victories,
                        liberated,
                    } => {
                        assert!(victories.is_some() || liberated.is_some());
                        assert!(victories.is_none_or(|(have, need)| have < need));
                        assert!(liberated.is_none_or(|(have, need)| have < need));
                    }
                    NodeState::Owned | NodeState::Unavailable { .. } => {}
                }
                // The checklist agrees: a buyable node meets every row.
                let requirements = c.node_requirements(&node.id);
                if state.is_buyable() {
                    assert!(requirements.iter().all(|r| r.met), "{}", node.id);
                }
                for r in &requirements {
                    let first = r.label.chars().next().unwrap();
                    assert!(
                        first.is_uppercase() || first.is_ascii_digit(),
                        "{}",
                        r.label
                    );
                    assert!(!r.label.ends_with('.'), "{}", r.label);
                }
            }
        }
    }

    #[test]
    fn node_state_reports_each_gate_with_player_details() {
        let mut c = started();
        assert_eq!(c.node_state("pistol_training"), NodeState::Owned);
        assert_eq!(
            c.node_state("shotgun_training"),
            NodeState::NeedsPrerequisites {
                missing: vec!["steady_hands".into()]
            }
        );
        // Authored as [rifle, smg]; reported in catalogue order.
        assert_eq!(
            c.node_state("machine_gun_training"),
            NodeState::NeedsPrerequisites {
                missing: vec!["smg_training".into(), "rifle_training".into()]
            }
        );
        let fitness = price("field_fitness");
        c.coins = 5;
        assert_eq!(
            c.node_state("field_fitness"),
            NodeState::NeedsCoins {
                payment: NodePayment::Coins(fitness),
                short: fitness - 5
            }
        );
        c.coins = 1000;
        assert_eq!(
            c.node_state("field_fitness"),
            NodeState::Buyable(NodePayment::Coins(fitness))
        );
        c.purchase_node("field_fitness").unwrap();
        assert_eq!(
            c.node_state("companion_contact"),
            NodeState::NeedsMilestone {
                victories: None,
                liberated: Some((0, 1))
            }
        );
        c.purchase_node("engineering").unwrap();
        assert_eq!(
            c.node_state("wall_blueprint"),
            NodeState::NeedsMilestone {
                victories: Some((0, 2)),
                liberated: None
            }
        );
        assert!(matches!(
            c.node_state("missing"),
            NodeState::Unavailable { .. }
        ));
        c.reserve(Mode::Personal).unwrap();
        assert_eq!(
            c.node_state("quick_reload"),
            NodeState::Unavailable {
                reason: c.can_purchase_node("quick_reload").unwrap_err()
            }
        );
        let fresh_without_root = Campaign {
            skill_nodes: Default::default(),
            legacy_weapon_training: false,
            ..started()
        };
        assert!(matches!(
            fresh_without_root.node_state("pistol_training"),
            NodeState::Unavailable { .. }
        ));
    }

    #[test]
    fn requirement_checklists_use_player_wording_with_progress() {
        let mut c = started();
        c.coins = 25;
        let labels = |c: &Campaign, id: &str| -> Vec<(String, bool)> {
            c.node_requirements(id)
                .into_iter()
                .map(|r| (r.label, r.met))
                .collect()
        };
        assert_eq!(
            labels(&c, "shotgun_training"),
            [
                ("Learn Steady hands".to_owned(), false),
                (
                    format!("{} coins (have 25)", price("shotgun_training")),
                    false
                )
            ]
        );
        assert_eq!(
            labels(&c, "companion_contact"),
            [
                ("Learn Field fitness".to_owned(), false),
                ("Liberate 1 country (0 / 1)".to_owned(), false),
                ("75 coins (have 25)".to_owned(), false)
            ]
        );
        assert_eq!(
            labels(&c, "parallel_command")[1],
            ("Liberate 2 countries (0 / 2)".to_owned(), false)
        );
        assert_eq!(
            labels(&c, "body_vitality_7")[1],
            ("Win 1 defense (0 / 1)".to_owned(), false)
        );
        c.victories = 5;
        assert_eq!(
            labels(&c, "wall_blueprint")[1],
            ("Win 2 defenses (2 / 2)".to_owned(), true)
        );
        let kinds: Vec<_> = c
            .node_requirements("machine_gun_training")
            .into_iter()
            .map(|r| r.kind)
            .collect();
        assert_eq!(
            kinds,
            [
                RequirementKind::Prerequisite("smg_training".into()),
                RequirementKind::Prerequisite("rifle_training".into()),
                RequirementKind::Coins
            ]
        );
        // Owned nodes keep their prerequisites but drop the price row.
        assert!(
            c.node_requirements("pistol_training").is_empty(),
            "the root has no prerequisites and no price"
        );
        c.coins = 1000;
        c.purchase_node("steady_hands").unwrap();
        assert_eq!(
            labels(&c, "steady_hands"),
            [("Learn Survivor foundations".to_owned(), true)]
        );
        assert!(c.node_requirements("missing").is_empty());
        let mut legacy = Campaign {
            legacy_features: true,
            ..started()
        };
        legacy.player.points = 3;
        let rows = legacy.node_requirements("body_vitality_1");
        assert_eq!(rows.last().unwrap().kind, RequirementKind::LegacyBodyPoints);
        assert_eq!(rows.last().unwrap().label, "1 legacy body point (have 3)");
    }

    fn assert_valid_path(c: &Campaign, id: &str) {
        let path = c.path_to_node(id);
        if c.has_node(id) {
            assert_eq!(path, NodePath::default());
            return;
        }
        assert_eq!(path.steps.last().map(String::as_str), Some(id));
        let mut owned: BTreeSet<&str> = BTreeSet::new();
        for step in &path.steps {
            assert!(!c.has_node(step), "{id}: owned ancestor {step} in path");
            let node = skills().node(step).unwrap();
            for parent in &node.prerequisites {
                assert!(
                    c.has_node(parent) || owned.contains(parent.as_str()),
                    "{id}: {step} before its prerequisite {parent}"
                );
            }
            assert!(owned.insert(step), "{id}: duplicate step {step}");
        }
        let total: u64 = path
            .steps
            .iter()
            .map(|s| skills().node(s).unwrap().price)
            .sum();
        assert_eq!(path.total_coins, total);
        assert_eq!(path, c.path_to_node(id), "deterministic");
        let first_blocked = path.steps.iter().find(|s| {
            let n = skills().node(s).unwrap();
            c.victories < n.minimum_victories || c.liberated_count() < n.minimum_liberated
        });
        assert_eq!(path.blocking.is_some(), first_blocked.is_some(), "{id}");
    }

    #[test]
    fn paths_list_unowned_ancestors_in_a_valid_purchase_order() {
        let c = started();
        for node in &skills().nodes {
            assert_valid_path(&c, &node.id);
        }
        let mg = c.path_to_node("machine_gun_training");
        assert_eq!(
            mg.steps,
            [
                "quick_reload",
                "smg_training",
                "rifle_training",
                "machine_gun_training"
            ]
        );
        assert_eq!(
            mg.total_coins,
            price("quick_reload")
                + price("smg_training")
                + price("rifle_training")
                + price("machine_gun_training")
        );
        assert_eq!(mg.blocking, None);
        assert_eq!(
            c.path_to_node("companion_contact").blocking.as_deref(),
            Some("Liberate 1 country (0 / 1)")
        );
        assert_eq!(
            c.path_to_node("body_vitality_7").blocking.as_deref(),
            Some("Win 1 defense (0 / 1)")
        );
        let mut rich = started();
        rich.coins = 100_000;
        rich.victories = 3;
        rich.purchase_node("steady_hands").unwrap();
        rich.purchase_node("quick_reload").unwrap();
        assert_eq!(
            rich.path_to_node("shotgun_training").steps,
            ["shotgun_training"]
        );
        assert_eq!(
            rich.path_to_node("machine_gun_training").steps,
            ["smg_training", "rifle_training", "machine_gun_training"]
        );
        for node in &skills().nodes {
            assert_valid_path(&rich, &node.id);
        }
        assert_eq!(rich.path_to_node("steady_hands"), NodePath::default());
        assert_eq!(rich.path_to_node("missing"), NodePath::default());
    }

    fn delta(c: &Campaign, id: &str, stat: PreviewStat) -> (f32, f32) {
        let preview = c.node_stat_preview(id);
        let d = preview
            .iter()
            .find(|d| d.stat == stat)
            .unwrap_or_else(|| panic!("{id}: no {stat:?} in {preview:?}"));
        (d.before, d.after)
    }

    #[test]
    fn stat_previews_compare_real_stats_with_and_without_the_node() {
        let c = started();
        let health = c.profile(Mode::Personal).personal.health as f32;
        let preview = c.node_stat_preview("field_fitness");
        assert_eq!(
            preview,
            [StatDelta {
                stat: PreviewStat::MaxHealth,
                before: health,
                after: health + 35.0
            }]
        );
        assert_eq!(
            preview[0].summary(),
            format!("Max health {} → {}", health, health + 35.0)
        );
        let reload = c.node_stat_preview("quick_reload");
        assert_eq!(reload.len(), 1);
        assert_eq!(reload[0].stat, PreviewStat::ReloadTime);
        assert_eq!(reload[0].summary(), "Reload time ×1.00 → ×0.80");
        assert_eq!(
            delta(&c, "steady_hands", PreviewStat::DamageBonus),
            (0.0, 8.0)
        );
        assert_eq!(PreviewStat::DamageBonus.format(8.0), "+8");
        // Body ranks change the rank and the derived stat.
        assert_eq!(
            delta(
                &c,
                "body_vitality_1",
                PreviewStat::BodyRank(BodyAttribute::Vitality)
            ),
            (0.0, 1.0)
        );
        assert_eq!(
            delta(&c, "body_vitality_1", PreviewStat::MaxHealth),
            (health, health + 5.0)
        );
        assert_eq!(
            delta(&c, "body_coordination_1", PreviewStat::ReloadTime),
            (1.0, coordination_reload_factor(1))
        );
        let (before, after) = delta(&c, "body_perception_1", PreviewStat::WeakSpotBonus);
        assert!((after - before - 0.03).abs() < 1e-6, "{before} → {after}");
        // Weapon training and other capabilities are unlocks.
        assert_eq!(
            c.node_stat_preview("smg_training"),
            [StatDelta {
                stat: PreviewStat::Unlock("SMG proficiency".into()),
                before: 0.0,
                after: 1.0
            }]
        );
        assert_eq!(
            c.node_stat_preview("smg_training")[0].summary(),
            "Unlocks SMG proficiency"
        );
        assert_eq!(
            delta(
                &c,
                "grenade_training",
                PreviewStat::Unlock("Grenades".into())
            ),
            (0.0, 1.0)
        );
        assert_eq!(
            delta(&c, "companion_contact", PreviewStat::Unlock("Mara".into())),
            (0.0, 1.0)
        );
        assert_eq!(
            delta(&c, "engineering", PreviewStat::StructurePower),
            (1.0, 1.25)
        );
        assert_eq!(PreviewStat::StructurePower.format(1.25), "×1.25");
        assert_eq!(
            delta(
                &c,
                "engineering",
                PreviewStat::Unlock("Fortifications".into())
            ),
            (0.0, 1.0)
        );
        assert_eq!(
            delta(
                &c,
                "trap_blueprint",
                PreviewStat::Unlock("Trap blueprint".into())
            ),
            (0.0, 1.0)
        );
        assert_eq!(
            c.node_stat_preview("trap_rank_3"),
            [StatDelta {
                stat: PreviewStat::StructureRank(StructureKind::Trap),
                before: 0.0,
                after: 3.0
            }]
        );
        assert_eq!(
            delta(&c, "grenade_bandolier", PreviewStat::GrenadeCharges),
            (2.0, 4.0)
        );
        assert!(c.node_stat_preview("pistol_training").is_empty());
        assert!(c.node_stat_preview("missing").is_empty());
        // Every node with a stat effect previews at least one change.
        for node in &skills().nodes {
            if !c.has_node(&node.id) {
                assert!(!c.node_stat_preview(&node.id).is_empty(), "{}", node.id);
            }
        }
    }

    #[test]
    fn stat_previews_predict_the_purchase_outcome() {
        let mut c = started();
        c.coins = 100_000;
        c.victories = 100;
        for id in [
            "field_fitness",
            "quick_reload",
            "body_coordination_1",
            "engineering",
            "trap_blueprint",
            "trap_rank_1",
            "grenade_training",
        ] {
            let before = c.clone();
            let preview = c.node_stat_preview(id);
            assert_eq!(c, before, "previews never mutate");
            assert!(!preview.is_empty());
            c.purchase_node(id).unwrap();
            for d in &preview {
                if d.stat == PreviewStat::MaxHealth {
                    assert_eq!(c.profile(Mode::Personal).personal.health as f32, d.after);
                }
                if let PreviewStat::StructureRank(kind) = d.stat {
                    assert_eq!(c.structure_upgrades[kind] as f32, d.after);
                }
                if d.stat == PreviewStat::ReloadTime {
                    let m = c.modifiers();
                    let rank = c.body_rank(BodyAttribute::Coordination);
                    assert_eq!(
                        m.reload_multiplier * coordination_reload_factor(rank),
                        d.after
                    );
                }
            }
            // Owning the node leaves nothing further to preview.
            assert!(c.node_stat_preview(id).is_empty());
        }
    }

    #[test]
    fn preview_stat_formats_read_naturally() {
        assert_eq!(PreviewStat::MaxHealth.label(), "Max health");
        assert_eq!(PreviewStat::MaxHealth.format(135.0), "135");
        assert_eq!(PreviewStat::ReloadTime.format(0.8), "×0.80");
        assert_eq!(PreviewStat::WeakSpotBonus.format(0.15), "+15%");
        assert_eq!(
            PreviewStat::BodyRank(BodyAttribute::Vitality).label(),
            "Vitality rank"
        );
        assert_eq!(
            PreviewStat::StructureRank(StructureKind::Wall).label(),
            "Wall mastery"
        );
        let unlock = PreviewStat::Unlock("Grenades".into());
        assert_eq!(unlock.label(), "Grenades");
        assert_eq!(unlock.format(1.0), "Unlocked");
        assert_eq!(unlock.format(0.0), "Locked");
    }

    /// Buys the recommendation repeatedly with `coins`; returns the order.
    fn recommended_route(c: &mut Campaign, coins: u64) -> Vec<String> {
        c.coins = coins;
        let mut route = Vec::new();
        while let Some(id) = c.recommended_node(Mode::Personal) {
            if c.purchase_node(&id).is_err() {
                break;
            }
            route.push(id);
        }
        route
    }

    #[test]
    fn the_recommendation_buys_what_helps_now_before_body_ranks() {
        let mut c = started();
        let route = recommended_route(&mut c, 400);
        let first_body = route
            .iter()
            .position(|id| id.starts_with("body_"))
            .unwrap_or(route.len());
        // Every dedicated starter that acts now comes before any +5 body rank.
        for id in [
            "steady_hands",
            "field_fitness",
            "quick_reload",
            "endurance_drill",
        ] {
            let at = route.iter().position(|r| r == id).unwrap_or(usize::MAX);
            assert!(
                at < first_body,
                "{id} at {at}, first body rank at {first_body}: {route:?}"
            );
        }
        // Nothing that cannot act yet: no class without its gun, no
        // construction or frames before a blueprint can be learned.
        for useless in [
            "smg_training",
            "shotgun_training",
            "rifle_training",
            "engineering",
            "reinforced_frames",
        ] {
            assert!(!route.contains(&useless.to_owned()), "{useless}: {route:?}");
        }
    }

    #[test]
    fn the_first_recommendation_is_the_cheapest_node_that_helps_now() {
        let mut c = started();
        c.coins = 100_000;
        let cheapest = skills()
            .nodes
            .iter()
            .filter(|n| c.node_state(&n.id).is_buyable() && c.node_helps_now(&n.id, Mode::Personal))
            .map(|n| n.price)
            .min()
            .unwrap();
        c.coins = cheapest;
        let id = c.recommended_node(Mode::Personal).unwrap();
        assert!(c.node_state(&id).is_buyable(), "{id}");
        assert_eq!(price(&id), cheapest, "{id}");
        assert_ne!(id, "quick_reload", "affordable stronger starter skipped");
    }

    #[test]
    fn nodes_help_only_once_they_can_act() {
        let mut c = started();
        assert!(c.node_helps_now("steady_hands", Mode::Personal));
        assert!(
            !c.node_helps_now("engineering", Mode::Personal),
            "no blueprint yet"
        );
        assert!(
            !c.node_helps_now("rifle_training", Mode::Personal),
            "no rifle owned"
        );
        assert!(
            !c.node_helps_now("steady_hands", Mode::Npc),
            "personal-only effect"
        );
        c.victories = 1;
        assert!(
            c.node_helps_now("engineering", Mode::Personal),
            "trap blueprint can follow"
        );
        let rifle = crate::content::content()
            .items
            .iter()
            .position(|i| {
                i.slot == crate::content::Slot::Ranged
                    && !i.npc_only
                    && crate::weapons::WeaponSpec::from_item(i).class
                        == crate::weapons::WeaponClass::Rifle
            })
            .unwrap();
        c.add_item(rifle).unwrap();
        assert!(c.node_helps_now("rifle_training", Mode::Personal));
        assert!(
            !c.node_helps_now("body_vitality_1", Mode::Personal),
            "a rank, not a step"
        );
    }

    /// Design progression-tree §5 "Recommender": a class perk acts now only
    /// with an owned, non-npc_only Ranged item of its class that is not one
    /// of Mara's guns, and never for Mara. No shipped node authors a perk
    /// yet (W2-41), so the node is injected.
    #[test]
    fn class_modifier_node_acts_now_only_with_owned_class_weapon() {
        use crate::weapons::{WeaponClass, WeaponSpec};
        let perk_node = |class| SkillNode {
            effects: vec![SkillEffect::ClassModifier {
                class,
                stat: ClassStat::Damage,
                value: 1.2,
            }],
            ..skills().node("steady_hands").unwrap().clone()
        };
        let mut c = started();
        let class_of =
            |c: &Campaign, id: u64| WeaponSpec::from_item(c.item(id).unwrap().definition()).class;
        let survivor_gun = class_of(&c, c.player.loadout[crate::content::Slot::Ranged.index()]);
        let mara_gun = class_of(&c, c.npc.loadout[crate::content::Slot::Ranged.index()]);
        assert_eq!(c.node_value(&perk_node(survivor_gun), Mode::Personal), 2);
        assert_eq!(
            c.node_value(&perk_node(survivor_gun), Mode::Npc),
            0,
            "personal-only perk"
        );
        assert_ne!(mara_gun, survivor_gun, "the fixture separates the two guns");
        assert_eq!(
            c.node_value(&perk_node(mara_gun), Mode::Personal),
            0,
            "Mara's starter gun is not the survivor's"
        );
        assert_eq!(
            c.node_value(&perk_node(WeaponClass::Rifle), Mode::Personal),
            0
        );
        let rifle = crate::content::content()
            .items
            .iter()
            .position(|i| {
                i.slot == crate::content::Slot::Ranged
                    && !i.npc_only
                    && WeaponSpec::from_item(i).class == WeaponClass::Rifle
            })
            .unwrap();
        c.add_item(rifle).unwrap();
        assert_eq!(
            c.node_value(&perk_node(WeaponClass::Rifle), Mode::Personal),
            2
        );
        assert_eq!(c.node_value(&perk_node(WeaponClass::Rifle), Mode::Npc), 0);
    }

    #[test]
    fn recommended_node_is_never_owned_and_prefers_buyable_starters() {
        let mut c = started();
        c.coins = 40;
        let id = c.recommended_node(Mode::Personal).unwrap();
        assert!(c.node_state(&id).is_buyable(), "{id}");
        let node = skills().node(&id).unwrap();
        assert_eq!(node.prerequisites, ["pistol_training"]);
        // Broke: the cheapest reachable starter, still not the root.
        let broke = started();
        let id = broke.recommended_node(Mode::Personal).unwrap();
        assert_ne!(id, "pistol_training");
        assert_eq!(
            skills().node(&id).unwrap().prerequisites,
            ["pistol_training"]
        );
        assert_eq!(broke.path_to_node(&id).blocking, None);
        for c in states() {
            for mode in [Mode::Personal, Mode::Npc] {
                if let Some(id) = c.recommended_node(mode) {
                    assert!(!c.has_node(&id), "{id}");
                    assert!(skills().node(&id).unwrap().price > 0);
                }
            }
        }
        // A campaign that owns everything has nothing to recommend.
        let mut all = started();
        all.skill_nodes = skills().nodes.iter().map(|n| n.id.clone()).collect();
        assert_eq!(all.recommended_node(Mode::Personal), None);
    }

    #[test]
    fn legacy_note_only_appears_for_legacy_body_nodes() {
        let fresh = started();
        assert_eq!(fresh.legacy_node_note("body_vitality_1"), None);
        let legacy = Campaign {
            legacy_features: true,
            ..started()
        };
        assert!(legacy.legacy_node_note("body_vitality_1").is_some());
        assert_eq!(legacy.legacy_node_note("field_fitness"), None);
        // Content no longer carries the legacy sentence.
        for node in &skills().nodes {
            assert!(
                !node.description.contains("floor"),
                "{}: {}",
                node.id,
                node.description
            );
        }
    }
}
