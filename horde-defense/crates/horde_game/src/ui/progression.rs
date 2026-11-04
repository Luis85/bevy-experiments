//! One connected personal progression catalogue. All purchases use core preflight.
use super::*;
mod upgrade_graph;
use upgrade_graph::{Graph, Node, View};

pub(super) fn skills(ui: &mut egui::Ui, f: &mut Frontend) {
    ui.horizontal_wrapped(|ui| {
        ui.heading("Journey upgrades");
        ui.colored_label(AMBER, format!("{} coins", f.game.campaign.coins));
    });
    skill_tree(ui, f, visible_height(ui));
}
pub(super) fn engineering(ui: &mut egui::Ui, f: &mut Frontend) {
    skills(ui, f);
}
fn visible_height(ui: &egui::Ui) -> f32 {
    (ui.clip_rect().bottom() - ui.cursor().top())
        .min(ui.available_height())
        .max(0.0)
}
fn catalogue_nodes(f: &Frontend) -> Vec<Node> {
    catalogue_nodes_for(&f.game.campaign)
}
fn catalogue_nodes_for(campaign: &horde_core::campaign::Campaign) -> Vec<Node> {
    let catalog = horde_core::skill_catalog::skills();
    catalog
        .nodes
        .iter()
        .map(|node| {
            let learned = campaign.has_node(&node.id);
            let requirements = node.prerequisites.iter().all(|id| campaign.has_node(id))
                && campaign.liberated_count() >= node.minimum_liberated
                && campaign.victories >= node.minimum_victories;
            Node {
                position: vec2(node.position[0], node.position[1]),
                title: node.name.clone(),
                state: if learned {
                    "OWNED".into()
                } else if !requirements {
                    "LOCKED".into()
                } else {
                    payment_label(campaign, &node.id)
                },
                icon: upgrade_graph::skill_icon(&node.icon),
                acquired: learned,
                available: !learned && campaign.can_purchase_node(&node.id).is_ok(),
                prerequisites: node
                    .prerequisites
                    .iter()
                    .filter_map(|id| catalog.nodes.iter().position(|p| p.id == *id))
                    .collect(),
            }
        })
        .collect()
}
pub(super) fn skill_tree(ui: &mut egui::Ui, f: &mut Frontend, height: f32) {
    let catalog = horde_core::skill_catalog::skills();
    let id = ui.make_persistent_id("journey_complete_skill_graph");
    let mut view = ui
        .ctx()
        .data_mut(|d| d.get_temp::<View>(id).unwrap_or_default());
    let focus = match f.improvement_focus.as_ref() {
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
fn payment_label(campaign: &horde_core::campaign::Campaign, id: &str) -> String {
    match campaign.node_payment(id) {
        Ok(horde_core::campaign::NodePayment::LegacyBodyPoints(points)) => {
            format!("{points} legacy CP")
        }
        Ok(horde_core::campaign::NodePayment::Coins(price)) => format!("{price} coins"),
        Err(_) => "Unavailable".into(),
    }
}
fn skill_detail(
    ui: &mut egui::Ui,
    f: &mut Frontend,
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
    let owned = f.game.campaign.has_node(&node.id);
    let preflight = f.game.campaign.can_purchase_node(&node.id);
    if owned {
        ui.colored_label(
            AMBER,
            if f.game.campaign.skill_nodes.contains(&node.id)
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
                    payment_label(&f.game.campaign, &node.id)
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
        if node.effects.iter().any(|e|matches!(e,horde_core::skill_catalog::SkillEffect::BodyDevelopment{..})) && f.game.campaign.legacy_features && f.game.campaign.player.body.iter().any(|n|*n>0) && ui.button("Refund legacy body allocations").clicked() {f.dispatch(Command::RespecPlayer,"Legacy body points returned to the tree.");}
        ui.add_space(7.0);
        ui.strong("Requirements");
        if node.prerequisites.is_empty() {muted(ui,"Journey foundation");}
        for parent in &node.prerequisites {
            let name=horde_core::skill_catalog::skills().node(parent).map_or(parent.as_str(),|n|n.name.as_str());
            let learned=f.game.campaign.has_node(parent);
            ui.colored_label(if learned {AMBER} else {MUTED},format!("{} {name}",if learned {"Owned:"}else{"Requires:"}));
        }
        if node.minimum_victories>0 {ui.label(format!("Win {} rounds · {} won",node.minimum_victories,f.game.campaign.victories));}
        if node.minimum_liberated>0 {ui.label(format!("Liberate {} countries · {} achieved",node.minimum_liberated,f.game.campaign.liberated_count()));}
        if !owned {match f.game.campaign.node_payment(&node.id) {Ok(horde_core::campaign::NodePayment::LegacyBodyPoints(points))=>{muted(ui,format!("{points} legacy CP required · {} available",f.game.campaign.player.points));},_=>{muted(ui,format!("{} · {} coins available",payment_label(&f.game.campaign,&node.id),f.game.campaign.coins));}}}
        muted(ui,"Weapon training and equipment ownership are separate. Buy or loot trained weapons in Equipment.");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
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
            campaign.first_clears[..clears].fill(true);
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
    fn reopened_country_uses_the_same_ever_liberated_gate_as_the_purchase_preflight() {
        let mut f = crate::ui::test_support::test_frontend();
        f.game = Game::default();
        f.game.campaign.choose_survivor(SurvivorId::Rook).unwrap();
        f.game.campaign.choose_start_country("DEU").unwrap();
        let count = world().country("DEU").unwrap().level_count();
        for stage in 0..count {
            f.game.campaign.record_country_stage_clear(stage).unwrap();
        }
        f.game.campaign.world.cleared.retain(|id| id != "DEU");
        f.game.campaign.coins = 1000;
        f.game.campaign.purchase_node("field_fitness").unwrap();
        assert_eq!(f.game.campaign.liberated_count(), 1);
        assert!(f.game.campaign.world.cleared.is_empty());
        assert!(
            f.game
                .campaign
                .can_purchase_node("companion_contact")
                .is_ok()
        );
        let index = horde_core::skill_catalog::skills()
            .nodes
            .iter()
            .position(|n| n.id == "companion_contact")
            .unwrap();
        let nodes = catalogue_nodes(&f);
        assert!(nodes[index].available);
        assert_ne!(nodes[index].state, "LOCKED");
    }
}
