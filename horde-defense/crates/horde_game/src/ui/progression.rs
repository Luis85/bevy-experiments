//! One connected personal progression catalogue. All purchases use core preflight.
use super::{UiCtx, muted};
use crate::art::{AMBER, MUTED};
use bevy_egui::egui::{self, Align, Layout, RichText, Sense, vec2};
use horde_core::campaign::{NodePayment, NodeStatus};
use horde_core::command::Command;
mod upgrade_graph;
use upgrade_graph::{Graph, Node, View};

fn catalogue_nodes(f: &UiCtx) -> Vec<Node> {
    catalogue_nodes_for(f.game.campaign())
}
fn catalogue_nodes_for(campaign: &horde_core::campaign::Campaign) -> Vec<Node> {
    let catalog = horde_core::skill_catalog::skills();
    catalog
        .nodes
        .iter()
        .map(|node| {
            let status = campaign.node_status(&node.id);
            Node {
                position: vec2(node.position[0], node.position[1]),
                title: node.name.clone(),
                state: status_label(&status),
                acquired: status == NodeStatus::Owned,
                available: matches!(status, NodeStatus::Available(_)),
                icon: upgrade_graph::skill_icon(&node.icon),
                prerequisites: node
                    .prerequisites
                    .iter()
                    .filter_map(|id| catalog.nodes.iter().position(|p| p.id == *id))
                    .collect(),
            }
        })
        .collect()
}
pub(super) fn skill_tree(ui: &mut egui::Ui, f: &mut UiCtx, height: f32) {
    let catalog = horde_core::skill_catalog::skills();
    let id = ui.make_persistent_id("journey_complete_skill_graph");
    let mut view = ui
        .ctx()
        .data_mut(|d| d.get_temp::<View>(id).unwrap_or_default());
    let focus = match f.campaign.improvement_focus.as_ref() {
        Some(horde_core::campaign::RecommendationAction::LearnSkill(s)) => Some(s.id()),
        Some(horde_core::campaign::RecommendationAction::LearnNode(id)) => Some(id.as_str()),
        _ => None,
    };
    view.recommend(focus.and_then(|id| catalog.nodes.iter().position(|n| n.id == id)));
    view.selected = view.selected.min(catalog.nodes.len() - 1);
    ui.horizontal_wrapped(|ui| {
        ui.menu_button(
            format!("Branch focus: {}", catalog.nodes[view.selected].branch),
            |ui| {
                let branches: std::collections::BTreeSet<_> =
                    catalog.nodes.iter().map(|n| n.branch.as_str()).collect();
                for branch in branches {
                    if ui.button(branch).clicked() {
                        if let Some(index) = catalog.nodes.iter().position(|n| n.branch == branch) {
                            view.focus(index);
                        }
                        ui.close();
                    }
                }
            },
        );
        muted(
            ui,
            format!("All {} upgrades · one connected tree", catalog.nodes.len()),
        );
    });
    let nodes = catalogue_nodes(f);
    let graph = Graph::skills(&nodes);
    let detail_width = if ui.available_width() >= 1000.0 {
        290.0
    } else {
        240.0
    };
    let graph_width = (ui.available_width() - detail_width - 18.0).max(180.0);
    let body_height = (height - 31.0).max(150.0);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            vec2(graph_width, body_height),
            Layout::top_down(Align::Min),
            |ui| {
                graph.show(ui, id, &mut view, (body_height - 35.0).max(100.0));
            },
        );
        ui.separator();
        ui.allocate_ui_with_layout(
            vec2(detail_width, body_height),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_width(detail_width);
                skill_detail(ui, f, &catalog.nodes[view.selected], body_height);
            },
        );
    });
    ui.ctx().data_mut(|d| d.insert_temp(id, view));
}
fn status_label(status: &NodeStatus) -> String {
    match status {
        NodeStatus::Owned => "OWNED".into(),
        NodeStatus::Locked { .. } => "LOCKED".into(),
        NodeStatus::Available(payment)
        | NodeStatus::Blocked {
            payment: Some(payment),
            ..
        } => price_label(*payment),
        NodeStatus::Blocked { payment: None, .. } => "Unavailable".into(),
    }
}
fn price_label(payment: NodePayment) -> String {
    match payment {
        NodePayment::LegacyBodyPoints(points) => format!("{points} legacy CP"),
        NodePayment::Coins(price) => format!("{price} coins"),
    }
}
fn payment_label(campaign: &horde_core::campaign::Campaign, id: &str) -> String {
    campaign
        .node_payment(id)
        .map_or_else(|_| "Unavailable".into(), price_label)
}
fn skill_detail(
    ui: &mut egui::Ui,
    f: &mut UiCtx,
    node: &horde_core::skill_catalog::SkillNode,
    height: f32,
) {
    ui.horizontal_wrapped(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
        upgrade_graph::pictogram(
            ui.painter(),
            r,
            upgrade_graph::skill_icon(&node.icon),
            AMBER,
        );
        ui.label(RichText::new(&node.name).size(20.0).strong());
    });
    let owned = f.game.campaign().has_node(&node.id);
    let preflight = f.game.campaign().can_purchase_node(&node.id);
    if owned {
        ui.colored_label(
            AMBER,
            if f.game.campaign().skill_nodes.contains(&node.id)
                || node.effects.iter().all(|e| {
                    !matches!(
                        e,
                        horde_core::skill_catalog::SkillEffect::BodyDevelopment { .. }
                    )
                })
            {
                "Owned permanently"
            } else {
                "Covered by legacy body allocation"
            },
        );
    } else {
        if ui
            .add_enabled(
                preflight.is_ok(),
                egui::Button::new(format!(
                    "Learn skill · {}",
                    payment_label(f.game.campaign(), &node.id)
                ))
                .min_size(vec2(ui.available_width(), 36.0)),
            )
            .clicked()
        {
            f.dispatch(Command::BuySkillNode(node.id.clone()), "Upgrade learned.");
        }
        if let Err(error) = &preflight {
            ui.label(RichText::new(error).color(MUTED).size(12.0));
        }
    }
    egui::ScrollArea::vertical().id_salt(("skill-requirements",&node.id)).max_height((height-105.0).max(55.0)).show(ui,|ui| {
        ui.label(&node.description);
        if node.effects.iter().any(|e|matches!(e,horde_core::skill_catalog::SkillEffect::BodyDevelopment{..})) && f.game.campaign().legacy_features && f.game.campaign().player.body.iter().any(|n|*n>0) && ui.button("Refund legacy body allocations").clicked() {f.dispatch(Command::RespecPlayer,"Legacy body points returned to the tree.");}
        ui.add_space(7.0);
        ui.strong("Requirements");
        if node.prerequisites.is_empty() {muted(ui,"Journey foundation");}
        for parent in &node.prerequisites {
            let name=horde_core::skill_catalog::skills().node(parent).map_or(parent.as_str(),|n|n.name.as_str());
            let learned=f.game.campaign().has_node(parent);
            ui.colored_label(if learned {AMBER} else {MUTED},format!("{} {name}",if learned {"Owned:"}else{"Requires:"}));
        }
        if node.minimum_victories>0 {ui.label(format!("Win {} rounds · {} won",node.minimum_victories,f.game.campaign().victories));}
        if node.minimum_liberated>0 {ui.label(format!("Liberate {} countries · {} achieved",node.minimum_liberated,f.game.campaign().liberated_count()));}
        if !owned {match f.game.campaign().node_payment(&node.id) {Ok(horde_core::campaign::NodePayment::LegacyBodyPoints(points))=>{muted(ui,format!("{points} legacy CP required · {} available",f.game.campaign().player.points));},_=>{muted(ui,format!("{} · {} coins available",payment_label(f.game.campaign(),&node.id),f.game.campaign().coins));}}}
        muted(ui,"Weapon training and equipment ownership are separate. Buy or loot trained weapons in Equipment.");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use horde_core::content::StructureKind;
    use horde_core::content::content;
    fn structure_node(
        kind: StructureKind,
        rank: u32,
    ) -> &'static horde_core::skill_catalog::SkillNode {
        horde_core::skill_catalog::skills()
            .nodes
            .iter()
            .find(|node| {
                node.effects.iter().any(|effect| match effect {
                    horde_core::skill_catalog::SkillEffect::Blueprint(value) => {
                        rank == 0 && *value == kind
                    }
                    horde_core::skill_catalog::SkillEffect::StructureRank {
                        kind: value,
                        rank: value_rank,
                    } => rank > 0 && *value == kind && *value_rank == rank,
                    _ => false,
                })
            })
            .unwrap()
    }
    fn actual_node(campaign: &horde_core::campaign::Campaign, id: &str) -> Node {
        let index = horde_core::skill_catalog::skills()
            .nodes
            .iter()
            .position(|n| n.id == id)
            .unwrap();
        catalogue_nodes_for(campaign).remove(index)
    }
    fn acquire_requirements(campaign: &mut horde_core::campaign::Campaign, id: &str) {
        let parents = horde_core::skill_catalog::skills()
            .node(id)
            .unwrap()
            .prerequisites
            .clone();
        for parent in parents {
            if !campaign.has_node(&parent) {
                acquire_requirements(campaign, &parent);
                campaign.purchase_node(&parent).unwrap();
            }
        }
    }
    #[test]
    fn catalogue_fortification_nodes_render_real_milestone_affordability_and_owned_states() {
        for kind in StructureKind::ALL {
            let mut campaign = horde_core::campaign::Campaign {
                coins: 100_000,
                ..Default::default()
            };
            let blueprint = structure_node(kind, 0);
            assert!(!actual_node(&campaign, &blueprint.id).available);
            assert!(campaign.can_purchase_node(&blueprint.id).is_err());
            campaign.victories = 100;
            acquire_requirements(&mut campaign, &blueprint.id);
            for rank in 0..=3 {
                let node = structure_node(kind, rank);
                acquire_requirements(&mut campaign, &node.id);
                let affordable = actual_node(&campaign, &node.id);
                assert!(!affordable.acquired);
                assert!(affordable.available);
                assert!(affordable.state.contains(&node.price.to_string()));
                campaign.coins = node.price - 1;
                assert!(!actual_node(&campaign, &node.id).available);
                assert!(campaign.can_purchase_node(&node.id).is_err());
                campaign.coins = 100_000;
                let before = campaign.coins;
                campaign.purchase_node(&node.id).unwrap();
                assert_eq!(before - campaign.coins, node.price);
                let owned = actual_node(&campaign, &node.id);
                assert!(owned.acquired);
                assert!(!owned.available);
                assert_eq!(owned.state, "OWNED");
                assert!(campaign.blueprints[kind]);
                assert_eq!(campaign.structure_upgrades[kind], rank);
            }
        }
    }
    #[test]
    fn legacy_fortification_transactions_keep_prices_and_are_reflected_by_production_catalogue_nodes()
     {
        for kind in StructureKind::ALL {
            let mut campaign = horde_core::campaign::Campaign {
                coins: 1000,
                legacy_features: true,
                ..Default::default()
            };
            let clears = content().structure(kind).unlock_clear;
            assert!(!campaign.blueprint_available(kind));
            assert!(campaign.purchase_blueprint(kind).is_err());
            campaign
                .first_clears
                .extend(content().levels[..clears].iter().map(|l| l.id.clone()));
            assert!(campaign.blueprint_available(kind));
            let before = campaign.coins;
            campaign.purchase_blueprint(kind).unwrap();
            assert_eq!(
                before - campaign.coins,
                content().structure(kind).blueprint_price
            );
            assert!(actual_node(&campaign, &structure_node(kind, 0).id).acquired);
            for rank in 1..=3 {
                let before = campaign.coins;
                campaign.upgrade_structure(kind).unwrap();
                assert_eq!(before - campaign.coins, u64::from(40 * rank));
                assert!(actual_node(&campaign, &structure_node(kind, rank).id).acquired);
            }
            assert!(campaign.upgrade_structure(kind).is_err());
        }
    }
    #[test]
    fn node_status_maps_to_catalogue_labels() {
        assert_eq!(status_label(&NodeStatus::Owned), "OWNED");
        assert_eq!(
            status_label(&NodeStatus::Locked {
                reason: "Win 3 defenses first".into()
            }),
            "LOCKED"
        );
        assert_eq!(
            status_label(&NodeStatus::Available(NodePayment::Coins(40))),
            "40 coins"
        );
        assert_eq!(
            status_label(&NodeStatus::Blocked {
                payment: Some(NodePayment::LegacyBodyPoints(1)),
                reason: "Need coins".into()
            }),
            "1 legacy CP"
        );
        assert_eq!(
            status_label(&NodeStatus::Blocked {
                payment: None,
                reason: "Ledger exhausted".into()
            }),
            "Unavailable"
        );
    }
}
