//! Read-only campaign queries for presentation. Screens map these answers to
//! labels; the rules themselves (gates, prices, focus targets) live here.
use super::{Campaign, NodePayment, Recommendation, RecommendationAction};
use crate::content::{ItemDefId, ItemDefinition, Mode, content};
use crate::skill_catalog::{CampaignFeature, SkillEffect, SkillNode, skills};
use crate::weapons::WeaponClass;

/// Purchase state of one skill-graph node, consistent with `can_purchase_node`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeStatus {
    Owned,
    /// A catalogue gate (prerequisites, liberation or victories) is unmet.
    Locked {
        reason: String,
    },
    /// `can_purchase_node` accepts the purchase at this payment.
    Available(NodePayment),
    /// Gates are met but the preflight refuses (coins, active deployment).
    /// `payment` is `None` when no payment can be quoted.
    Blocked {
        payment: Option<NodePayment>,
        reason: String,
    },
}

/// Which upgrade a screen wants to spotlight in the skill graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpgradeFocus {
    /// The campaign recommendation when it is a skill, otherwise the first
    /// purchasable node, otherwise the first node whose prerequisites are owned.
    Next(Mode),
    /// The first node of the Fortification branch.
    Fortification,
    /// The first node granting proficiency in this weapon class.
    WeaponTraining(WeaponClass),
}

/// One quartermaster offer at the current shop item level.
#[derive(Clone, Debug, PartialEq)]
pub struct OfferPreview {
    /// Catalogue position of the offered definition.
    pub definition: usize,
    /// Stable ID of the offered definition; what `Command::BuyItem` takes.
    pub id: ItemDefId,
    pub level: u32,
    pub price: u64,
    /// The authored item scaled to the offered level.
    pub item: ItemDefinition,
    pub tier_unlocked: bool,
    pub affordable: bool,
}

/// The catalogue node that opens one campaign feature, for screens that
/// name the feature before it is bought ("Liberating it opens Mara (75
/// coins)"). A label accessor: buying the node is the unlock event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeatureUnlock {
    /// Stable ID of the node; what `Command::BuySkillNode` takes.
    pub node_id: String,
    /// The node's player-facing name (for example "Meet Mara").
    pub name: String,
    /// The node's coin price.
    pub cost: u64,
    /// Countries that must have been liberated before the node is buyable.
    pub liberations_required: u32,
}

impl Campaign {
    /// The first catalogue node granting `feature`, or `None` when no node
    /// does. Pure and read-only: the answer depends only on the skill
    /// catalogue, never on ownership, coins or liberations.
    pub fn feature_unlock(&self, feature: CampaignFeature) -> Option<FeatureUnlock> {
        let grant = SkillEffect::FeatureUnlock(feature);
        skills()
            .nodes
            .iter()
            .find(|node| node.effects.contains(&grant))
            .map(|node| FeatureUnlock {
                node_id: node.id.clone(),
                name: node.name.clone(),
                cost: node.price,
                liberations_required: node.minimum_liberated,
            })
    }

    pub fn node_status(&self, id: &str) -> NodeStatus {
        let Some(node) = skills().node(id) else {
            return NodeStatus::Locked {
                reason: "Unknown skill node".into(),
            };
        };
        if self.owns_node_definition(node) {
            return NodeStatus::Owned;
        }
        if let Some(reason) = self.node_requirement_error(node) {
            return NodeStatus::Locked { reason };
        }
        match (self.can_purchase_node(id), self.node_payment(id)) {
            (Ok(()), Ok(payment)) => NodeStatus::Available(payment),
            (Ok(()), Err(reason)) => NodeStatus::Blocked {
                payment: None,
                reason,
            },
            (Err(reason), payment) => NodeStatus::Blocked {
                payment: payment.ok(),
                reason,
            },
        }
    }

    /// The skill node a screen should focus, as a recommendation.
    pub fn recommendation_fallback(&self, focus: UpgradeFocus) -> Option<Recommendation> {
        let nodes = &skills().nodes;
        let node = match focus {
            UpgradeFocus::Next(mode) => {
                let suggested = self.recommendation(mode);
                if matches!(
                    suggested.action,
                    RecommendationAction::LearnNode(_) | RecommendationAction::LearnSkill(_)
                ) {
                    return Some(suggested);
                }
                nodes
                    .iter()
                    .find(|n| !self.has_node(&n.id) && self.can_purchase_node(&n.id).is_ok())
                    .or_else(|| {
                        nodes.iter().find(|n| {
                            !self.has_node(&n.id)
                                && n.prerequisites.iter().all(|id| self.has_node(id))
                        })
                    })
            }
            UpgradeFocus::Fortification => nodes.iter().find(|n| n.branch == "Fortification"),
            UpgradeFocus::WeaponTraining(class) => nodes.iter().find(|n| {
                n.effects
                    .iter()
                    .any(|e| matches!(e, SkillEffect::WeaponTraining(c) if *c == class))
            }),
        }?;
        Some(self.node_recommendation(node))
    }

    fn node_recommendation(&self, node: &SkillNode) -> Recommendation {
        let price = match self.node_payment(&node.id) {
            Ok(NodePayment::Coins(price)) => price,
            _ => 0,
        };
        Recommendation {
            title: node.name.clone(),
            detail: node.description.clone(),
            action: RecommendationAction::LearnNode(node.id.clone()),
            price,
            affordable: self.coins >= price,
        }
    }

    /// The threat rank at which the shop item level next rises, or `None`
    /// when the maximum reachable authored item level is already offered.
    pub fn next_gear_level_rank(&self) -> Option<u32> {
        let tuning = crate::progression::catalog();
        let level = self.shop_item_level();
        let next = level.saturating_mul(tuning.item_level_rank_step);
        let reachable_max = tuning.snapshot(tuning.threat_guard_rank).item_level;
        (level < reachable_max && next <= tuning.threat_guard_rank).then_some(next)
    }

    pub fn shop_offer(&self, definition: usize) -> Result<OfferPreview, String> {
        let authored = content().items.get(definition).ok_or("Unknown equipment")?;
        let price = self.buy_price(definition)?;
        let level = self.shop_item_level();
        Ok(OfferPreview {
            definition,
            id: authored.id.clone(),
            level,
            price,
            item: crate::progression::scale_item(authored.clone(), level),
            tier_unlocked: authored.tier <= self.unlocked(),
            affordable: self.coins >= price,
        })
    }

    /// Every purchasable (non-starter, unlocked tier) catalogue offer in order.
    pub fn shop_offers(&self) -> Vec<OfferPreview> {
        content()
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.tier > 0 && item.tier <= self.unlocked())
            .map(|(index, _)| self.shop_offer(index).expect("catalogue definition exists"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::survivors::SurvivorId;
    use crate::world::world;

    fn started() -> Campaign {
        let mut c = Campaign::default();
        c.choose_survivor(SurvivorId::Rook).unwrap();
        c.choose_start_country("DEU").unwrap();
        c
    }

    /// The pre-refactor UI rule from `ui/progression.rs`, kept as the oracle.
    fn legacy_ui_requirements(c: &Campaign, node: &SkillNode) -> bool {
        node.prerequisites.iter().all(|id| c.has_node(id))
            && c.liberated_count() >= node.minimum_liberated
            && c.victories >= node.minimum_victories
    }

    fn assert_status_agrees_with_preflight(c: &Campaign) {
        for node in &skills().nodes {
            let status = c.node_status(&node.id);
            assert_eq!(
                matches!(status, NodeStatus::Available(_)),
                c.can_purchase_node(&node.id).is_ok(),
                "{}: {status:?}",
                node.id
            );
            assert_eq!(status == NodeStatus::Owned, c.has_node(&node.id));
            assert_eq!(
                matches!(status, NodeStatus::Locked { .. }),
                !c.has_node(&node.id) && !legacy_ui_requirements(c, node),
                "{}: {status:?}",
                node.id
            );
            if let NodeStatus::Available(payment)
            | NodeStatus::Blocked {
                payment: Some(payment),
                ..
            } = status
            {
                assert_eq!(Ok(payment), c.node_payment(&node.id));
            }
        }
    }

    #[test]
    fn node_status_matches_the_purchase_preflight_across_campaign_states() {
        let mut c = started();
        assert_status_agrees_with_preflight(&c);
        c.coins = 100_000;
        assert_status_agrees_with_preflight(&c);
        c.victories = 100;
        assert_status_agrees_with_preflight(&c);
        for _ in 0..12 {
            let Some(next) = skills()
                .nodes
                .iter()
                .find(|n| c.can_purchase_node(&n.id).is_ok())
            else {
                break;
            };
            c.purchase_node(&next.id.clone()).unwrap();
            assert_status_agrees_with_preflight(&c);
        }
        c.coins = 0;
        assert_status_agrees_with_preflight(&c);
        c.reserve(Mode::Personal).unwrap();
        assert_status_agrees_with_preflight(&c);
        let legacy = Campaign {
            legacy_features: true,
            coins: 1000,
            ..Campaign::default()
        };
        assert_status_agrees_with_preflight(&legacy);
    }

    #[test]
    fn node_status_reports_lock_reasons_and_blocking_payments() {
        let mut c = started();
        c.coins = 0;
        let locked = skills()
            .nodes
            .iter()
            .find(|n| n.prerequisites.iter().any(|id| !c.has_node(id)))
            .unwrap();
        assert_eq!(
            c.node_status(&locked.id),
            NodeStatus::Locked {
                reason: "Learn the required prerequisite skills first".into()
            }
        );
        let open = skills()
            .nodes
            .iter()
            .find(|n| {
                !c.has_node(&n.id)
                    && n.price > 0
                    && c.node_requirement_error(n).is_none()
                    && n.price > c.coins
            })
            .unwrap();
        assert_eq!(
            c.node_status(&open.id),
            NodeStatus::Blocked {
                payment: Some(NodePayment::Coins(open.price)),
                reason: format!("Need {} more coins", open.price - c.coins),
            }
        );
        c.coins = open.price;
        assert_eq!(
            c.node_status(&open.id),
            NodeStatus::Available(NodePayment::Coins(open.price))
        );
        assert!(matches!(
            c.node_status("missing"),
            NodeStatus::Locked { .. }
        ));
    }

    #[test]
    fn reopened_country_uses_the_same_ever_liberated_gate_as_the_purchase_preflight() {
        let mut c = started();
        let count = world().country("DEU").unwrap().level_count();
        for stage in 0..count {
            c.record_country_stage_clear(stage).unwrap();
        }
        c.world.cleared.retain(|id| id != "DEU");
        c.coins = 1000;
        c.purchase_node("field_fitness").unwrap();
        assert_eq!(c.liberated_count(), 1);
        assert!(c.world.cleared.is_empty());
        assert!(c.can_purchase_node("companion_contact").is_ok());
        assert!(matches!(
            c.node_status("companion_contact"),
            NodeStatus::Available(_)
        ));
    }

    /// The pre-refactor `focus_next_upgrade` body, kept as the oracle.
    fn legacy_next_focus(c: &Campaign, mode: Mode) -> Option<RecommendationAction> {
        let suggested = c.recommendation(mode).action;
        match suggested {
            RecommendationAction::LearnNode(_) | RecommendationAction::LearnSkill(_) => {
                Some(suggested)
            }
            _ => skills()
                .nodes
                .iter()
                .find(|n| !c.has_node(&n.id) && c.can_purchase_node(&n.id).is_ok())
                .or_else(|| {
                    skills().nodes.iter().find(|n| {
                        !c.has_node(&n.id) && n.prerequisites.iter().all(|id| c.has_node(id))
                    })
                })
                .map(|n| RecommendationAction::LearnNode(n.id.clone())),
        }
    }

    #[test]
    fn next_upgrade_fallback_preserves_the_previous_focus_rule() {
        let mut states = vec![Campaign::default(), started()];
        let mut rich = started();
        rich.coins = 100_000;
        rich.victories = 100;
        states.push(rich.clone());
        for _ in 0..20 {
            let Some(next) = skills()
                .nodes
                .iter()
                .find(|n| rich.can_purchase_node(&n.id).is_ok())
            else {
                break;
            };
            rich.purchase_node(&next.id.clone()).unwrap();
            states.push(rich.clone());
        }
        let mut broke = rich.clone();
        broke.coins = 0;
        states.push(broke);
        states.push(Campaign {
            legacy_features: true,
            coins: 1000,
            ..started()
        });
        for c in &states {
            for mode in [Mode::Personal, Mode::Npc] {
                assert_eq!(
                    c.recommendation_fallback(UpgradeFocus::Next(mode))
                        .map(|r| r.action),
                    legacy_next_focus(c, mode)
                );
            }
        }
        // A fresh journey always has a focus to show.
        assert!(
            started()
                .recommendation_fallback(UpgradeFocus::Next(Mode::Personal))
                .is_some()
        );
    }

    #[test]
    fn fortification_and_weapon_training_fallbacks_pick_their_catalogue_nodes() {
        let c = started();
        let fortification = c
            .recommendation_fallback(UpgradeFocus::Fortification)
            .unwrap();
        let first = skills()
            .nodes
            .iter()
            .find(|n| n.branch == "Fortification")
            .unwrap();
        assert_eq!(
            fortification.action,
            RecommendationAction::LearnNode(first.id.clone())
        );
        assert_eq!(fortification.title, first.name);
        for class in WeaponClass::ALL {
            let expected = skills()
                .nodes
                .iter()
                .find(|n| n.effects.contains(&SkillEffect::WeaponTraining(class)));
            assert_eq!(
                c.recommendation_fallback(UpgradeFocus::WeaponTraining(class))
                    .map(|r| r.action),
                expected.map(|n| RecommendationAction::LearnNode(n.id.clone()))
            );
        }
    }

    #[test]
    fn next_gear_level_rank_follows_the_item_level_step_until_the_reachable_maximum() {
        let tuning = crate::progression::catalog();
        let mut c = started();
        let level = c.shop_item_level();
        assert_eq!(
            c.next_gear_level_rank(),
            Some(level * tuning.item_level_rank_step)
        );
        c.victories = u64::from(tuning.threat_guard_rank);
        assert_eq!(c.next_gear_level_rank(), None);
    }

    #[test]
    fn shop_offers_quote_the_buy_preflight_price_and_scaled_item() {
        let mut c = started();
        c.first_clears = content().levels[..3].iter().map(|l| l.id.clone()).collect();
        assert_eq!(c.unlocked(), 4);
        c.coins = 0;
        let offers = c.shop_offers();
        assert!(!offers.is_empty());
        for offer in &offers {
            let authored = &content().items[offer.definition];
            assert!(authored.tier > 0 && authored.tier <= c.unlocked());
            assert_eq!(Ok(offer.price), c.buy_price(offer.definition));
            assert_eq!(offer.level, c.shop_item_level());
            assert_eq!(
                offer.item,
                crate::progression::scale_item(authored.clone(), offer.level)
            );
            assert!(offer.tier_unlocked);
            assert!(!offer.affordable);
        }
        let first = offers[0].definition;
        c.coins = offers[0].price;
        assert!(c.shop_offer(first).unwrap().affordable);
        assert_eq!(offers[0].id, content().items[first].id);
        assert!(c.buy_item(first).is_ok());
        let later = offers
            .iter()
            .find(|offer| content().items[offer.definition].tier > 1)
            .expect("a tier-2+ offer at frontier 4")
            .definition;
        c.first_clears.clear();
        assert!(!c.shop_offer(later).unwrap().tier_unlocked);
        assert!(c.shop_offer(usize::MAX).is_err());
    }

    #[test]
    fn feature_unlock_companion_is_75_after_1_liberation() {
        let c = started();
        assert_eq!(
            c.feature_unlock(CampaignFeature::Companion),
            Some(FeatureUnlock {
                node_id: "companion_contact".into(),
                name: "Meet Mara".into(),
                cost: 75,
                liberations_required: 1,
            })
        );
        let recruitment = c.feature_unlock(CampaignFeature::Recruitment).unwrap();
        assert_eq!(
            (recruitment.node_id.as_str(), recruitment.cost),
            ("recruitment_network", 100)
        );
        assert_eq!(recruitment.liberations_required, 1);
        // Every feature has exactly the catalogue node that grants it, so
        // the answer always agrees with the node the purchase preflight gates.
        for feature in CampaignFeature::ALL {
            let unlock = c.feature_unlock(feature).unwrap();
            let node = skills().node(&unlock.node_id).unwrap();
            assert!(node.effects.contains(&SkillEffect::FeatureUnlock(feature)));
            assert_eq!(
                (
                    unlock.name.as_str(),
                    unlock.cost,
                    unlock.liberations_required
                ),
                (node.name.as_str(), node.price, node.minimum_liberated)
            );
        }
    }

    #[test]
    fn feature_unlock_is_pure() {
        let fresh = started();
        let mut liberated = started();
        let count = world().country("DEU").unwrap().level_count();
        for stage in 0..count {
            liberated.record_country_stage_clear(stage).unwrap();
        }
        liberated.coins = 1000;
        liberated.purchase_node("field_fitness").unwrap();
        liberated.purchase_node("companion_contact").unwrap();
        assert_eq!(liberated.liberated_count(), 1);
        assert!(liberated.has_feature(CampaignFeature::Companion));
        for c in [Campaign::default(), fresh.clone(), liberated] {
            let before = c.clone();
            let first: Vec<_> = CampaignFeature::ALL
                .iter()
                .map(|f| c.feature_unlock(*f))
                .collect();
            let again: Vec<_> = CampaignFeature::ALL
                .iter()
                .map(|f| c.feature_unlock(*f))
                .collect();
            assert_eq!(c, before, "feature_unlock changed the campaign");
            assert_eq!(first, again);
            // A label accessor, not the unlock event: ownership, liberations
            // and coins never change the answer.
            let fresh_answers: Vec<_> = CampaignFeature::ALL
                .iter()
                .map(|f| fresh.feature_unlock(*f))
                .collect();
            assert_eq!(first, fresh_answers);
        }
    }
}
