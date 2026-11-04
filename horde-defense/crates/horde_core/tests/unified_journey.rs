use horde_core::{
    campaign::{BodyAttribute, Campaign, NodePayment},
    content::{EnemyKind, ItemDefId, Mode, Slot, StructureKind, content},
    kind_map::KindMap,
    sim::{Game, Input, JourneyContinuation, Outcome},
    skill_catalog::{CampaignFeature, SkillCatalog, skills},
    survivors::SurvivorId,
};
mod support;
use support::legacy_game;

#[test]
fn entire_catalog_is_one_connected_globally_positioned_dag_and_preserves_stable_unlock_ids() {
    let catalog = skills();
    catalog.validate().unwrap();
    assert_eq!(catalog.nodes.len(), 169);
    assert_eq!(
        catalog
            .nodes
            .iter()
            .filter(|node| node.prerequisites.is_empty())
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        vec!["pistol_training"]
    );
    for id in [
        "quick_reload",
        "steady_hands",
        "smg_training",
        "shotgun_training",
        "rifle_training",
        "piercing",
        "engineering",
        "grenade_training",
        "molotov_training",
        "field_fitness",
        "body_vitality_20",
        "turret_rank_10",
        "country_operations",
    ] {
        assert!(catalog.node(id).is_some(), "missing {id}");
    }
    let mut bad = catalog.clone();
    bad.nodes[1].prerequisites.clear();
    assert!(bad.validate().is_err());
    let mut bad = catalog.clone();
    bad.nodes[1].position = bad.nodes[0].position;
    assert!(bad.validate().is_err());
    assert_eq!(
        SkillCatalog::from_json(include_str!("../skills.json")).unwrap(),
        *catalog
    );
}

#[test]
fn fresh_body_training_is_coin_graph_progression_and_never_double_applies_legacy_ranks() {
    let mut c = Campaign::default();
    c.reward(1000, 100, Mode::Personal);
    assert!(c.player.level > 1);
    assert_eq!(c.player.points, 0);
    assert!(c.allocate_body(BodyAttribute::Vitality).is_err());
    let personal = c.profile(Mode::Personal);
    let npc = c.profile(Mode::Npc);
    let price = skills().node("body_vitality_1").unwrap().price;
    assert_eq!(
        c.node_payment("body_vitality_1").unwrap(),
        NodePayment::Coins(price)
    );
    c.purchase_node("body_vitality_1").unwrap();
    c.purchase_node("body_vitality_2").unwrap();
    assert_eq!(c.body_rank(BodyAttribute::Vitality), 2);
    assert_eq!(
        c.profile(Mode::Personal).personal.health,
        personal.personal.health + 10
    );
    assert_eq!(c.profile(Mode::Npc).npc, npc.npc);
    assert_eq!(c.profile(Mode::Npc).max_hp, npc.max_hp);
    let before = c.clone();
    assert!(c.purchase_node("body_vitality_2").is_err());
    assert_eq!(c, before);
    c.validate().unwrap();
    let mut legacy = Campaign {
        legacy_features: true,
        ..Campaign::default()
    };
    legacy.player.level = 4;
    legacy.player.points = 0;
    legacy.player.body[BodyAttribute::Vitality] = 3;
    legacy.coins = 500;
    assert!(legacy.has_node("body_vitality_3"));
    let hp = legacy.profile(Mode::Personal).personal.health;
    let before = legacy.clone();
    assert!(legacy.purchase_node("body_vitality_2").is_err());
    assert_eq!(legacy, before);
    legacy.purchase_node("body_vitality_4").unwrap();
    assert_eq!(legacy.profile(Mode::Personal).personal.health, hp + 5);
    legacy.validate().unwrap();
}

#[test]
fn retained_legacy_points_pay_inside_the_same_graph_with_an_exact_nonrefundable_ledger() {
    let mut c = Campaign {
        legacy_features: true,
        ..Campaign::default()
    };
    c.player.level = 2;
    c.player.points = 1;
    c.coins = 0;
    assert_eq!(
        c.node_payment("body_vitality_1").unwrap(),
        NodePayment::LegacyBodyPoints(1)
    );
    c.can_purchase_node("body_vitality_1").unwrap();
    c.purchase_node("body_vitality_1").unwrap();
    assert_eq!(c.player.points, 0);
    assert_eq!(c.player.legacy_graph_points, 1);
    assert_eq!(c.player.body[BodyAttribute::Vitality], 0);
    assert_eq!(c.profile(Mode::Personal).personal.health, 105);
    c.validate().unwrap();
    c.respec_player().unwrap();
    assert_eq!(c.player.points, 0);
    assert!(c.has_node("body_vitality_1"));
    c.validate().unwrap();
    let mut value = serde_json::to_value(Campaign::default()).unwrap();
    value.as_object_mut().unwrap().remove("legacy_features");
    value["player"]
        .as_object_mut()
        .unwrap()
        .remove("legacy_graph_points");
    let mut old: Campaign = serde_json::from_value(value).unwrap();
    assert!(old.legacy_features);
    old.reward(0, 20, Mode::Personal);
    assert_eq!(old.player.points, 1);
    old.validate().unwrap();
}

#[test]
fn legacy_refund_cannot_orphan_coin_or_point_purchased_graph_nodes() {
    for use_legacy_point in [false, true] {
        let mut g = Game::default();
        let c = g.campaign_mut();
        c.legacy_features = true;
        c.player.level = if use_legacy_point { 5 } else { 4 };
        c.player.body[BodyAttribute::Vitality] = 3;
        c.player.points = u32::from(use_legacy_point);
        c.coins = 500;
        let price = skills().node("body_vitality_4").unwrap().price;
        assert_eq!(
            c.node_payment("body_vitality_4").unwrap(),
            if use_legacy_point {
                NodePayment::LegacyBodyPoints(1)
            } else {
                NodePayment::Coins(price)
            }
        );
        c.purchase_node("body_vitality_4").unwrap();
        assert_eq!(c.player.points, 0);
        assert_eq!(c.player.legacy_graph_points, u32::from(use_legacy_point));
        assert_eq!(c.coins, if use_legacy_point { 500 } else { 500 - price });
        c.validate().unwrap();
        let before = c.clone();
        for _ in 0..2 {
            let error = c.respec_player().unwrap_err();
            assert!(error.contains("body_vitality_4"), "{error}");
            assert_eq!(*c, before, "failed refunds must leave all ledgers intact");
        }
        assert!(!c.skill_nodes.contains("body_vitality_3"));
        assert!(c.has_node("body_vitality_3"));
        c.validate().unwrap();
        let path = std::env::temp_dir().join(format!(
            "journey-refund-{}-{use_legacy_point}.json",
            std::process::id()
        ));
        g.save(&path).unwrap();
        assert_eq!(*Game::load(&path).unwrap().campaign(), before);
        std::fs::remove_file(&path).unwrap();
        if path.with_extension("bak").exists() {
            std::fs::remove_file(path.with_extension("bak")).unwrap();
        }
    }
    let mut c = Campaign {
        legacy_features: true,
        ..Campaign::default()
    };
    c.player.level = 4;
    c.player.body[BodyAttribute::Vitality] = 3;
    c.respec_player().unwrap();
    assert_eq!(c.player.points, 3);
    assert_eq!(c.player.body, KindMap::default());
    c.respec_player().unwrap();
    assert_eq!(
        c.player.points, 3,
        "the same allocations cannot refund twice"
    );
    c.validate().unwrap();
}

#[test]
fn fortification_nodes_have_real_effects_and_existing_blueprints_and_ranks_cannot_be_bought_twice()
{
    let mut g = Game::default();
    g.campaign_mut().reward(5000, 0, Mode::Personal);
    for stage in 0..3 {
        g.campaign_mut().first_clear(stage, Mode::Personal);
    }
    assert!(!g.campaign().has_feature(CampaignFeature::Construction));
    g.campaign_mut().purchase_node("engineering").unwrap();
    assert!(g.campaign().has_feature(CampaignFeature::Construction));
    g.campaign_mut()
        .purchase_blueprint(StructureKind::Trap)
        .unwrap();
    assert!(g.campaign().has_node("trap_blueprint"));
    g.campaign_mut()
        .upgrade_structure(StructureKind::Trap)
        .unwrap();
    assert_eq!(g.campaign().structure_upgrades[StructureKind::Trap], 1);
    g.campaign().validate().unwrap();
    let before = g.campaign().clone();
    assert!(g.campaign_mut().purchase_node("trap_rank_1").is_err());
    assert_eq!(*g.campaign(), before);
    g.deploy(0, Mode::Personal).unwrap();
    g.place(StructureKind::Trap, 0, 0).unwrap();
    assert!(g.attempt().unwrap().structures[0].max_hp >= 1);
    let mut old = legacy_game().campaign().clone();
    old.coins = 1000;
    old.first_clear(0, Mode::Personal);
    old.blueprints[StructureKind::Trap] = true;
    old.structure_upgrades[StructureKind::Trap] = 3;
    let coins = old.coins;
    assert!(old.has_node("trap_rank_3"));
    assert!(old.purchase_node("trap_rank_2").is_err());
    assert_eq!(old.coins, coins);
    old.purchase_node("trap_rank_4").unwrap();
    assert_eq!(old.structure_upgrades[StructureKind::Trap], 4);
    old.validate().unwrap();
}

#[test]
fn companions_and_world_complexity_require_both_authored_unlock_nodes_and_country_milestones() {
    let mut c = Campaign {
        attempts: 1,
        ..Campaign::default()
    };
    c.reward(5000, 0, Mode::Personal);
    assert!(!c.npc_available());
    assert!(!c.has_feature(CampaignFeature::Recruitment));
    c.purchase_node("field_fitness").unwrap();
    let before = c.clone();
    assert!(c.purchase_node("companion_contact").is_err());
    assert_eq!(c, before);
    c.choose_start_country("VAT").unwrap();
    let count = horde_core::world::world()
        .country("VAT")
        .unwrap()
        .level_count();
    for stage in 0..count {
        c.world.record_clear(stage).unwrap();
        c.first_clear(stage % 5, Mode::Personal);
    }
    assert_eq!(c.liberated_count(), 1);
    assert!(!c.npc_available());
    c.purchase_node("companion_contact").unwrap();
    assert!(c.npc_available());
    c.purchase_node("recruitment_network").unwrap();
    assert!(horde_core::strategy::StrategyState::default().recruitment_unlocked(&c));
    assert!(!c.has_feature(CampaignFeature::Parallel));
    let before = c.clone();
    assert!(c.purchase_node("parallel_command").is_err());
    assert_eq!(c, before);
    let mut old = legacy_game().campaign().clone();
    old.attempts = 1;
    assert!(old.npc_available());
}

fn clear_active(g: &mut Game) {
    let mut a = g.attempt_slot_mut().take().unwrap();
    let lane = a.lane_count / 2;
    g.spawn_enemy(&mut a, EnemyKind::Brute, lane, true);
    a.boss_spawned = true;
    a.enemies.last_mut().unwrap().hp = 0;
    *g.attempt_slot_mut() = Some(a);
    g.tick(Input::default());
    assert_eq!(g.last_result().unwrap().outcome, Outcome::Victory);
}

#[test]
fn journey_every_outcome_stops_for_upgrades_and_continuation_uses_reported_country_safely() {
    let mut g = Game::default();
    g.campaign_mut().choose_survivor(SurvivorId::Rook).unwrap();
    g.campaign_mut().choose_start_country("VAT").unwrap();
    g.deploy_country_stage(0, Mode::Personal).unwrap();
    g.withdraw();
    g.set_paused(true);
    assert!(g.global_paused());
    assert_eq!(g.campaign().run_history.len(), 1);
    assert_eq!(g.continue_journey().unwrap(), JourneyContinuation::Deployed);
    assert!(!g.global_paused());
    let before = serde_json::to_string(&g).unwrap();
    assert!(g.continue_journey().is_err());
    assert_eq!(serde_json::to_string(&g).unwrap(), before);
    g.attempt_mut().unwrap().hp = 0;
    g.tick(Input::default());
    assert_eq!(g.continue_journey().unwrap(), JourneyContinuation::Deployed);
    g.set_repeat_remaining(10);
    clear_active(&mut g);
    assert_eq!(g.repeat_remaining(), 0);
    assert!(g.attempt().is_none());
    assert_eq!(g.continue_journey().unwrap(), JourneyContinuation::Deployed);
    assert_eq!(g.attempt().unwrap().stage, 1);
    clear_active(&mut g);
    g.continue_journey().unwrap();
    clear_active(&mut g);
    let history = g.campaign().run_history.len();
    assert_eq!(
        g.continue_journey().unwrap(),
        JourneyContinuation::CountryLiberated
    );
    assert!(g.global_paused());
    assert!(g.attempt().is_none());
    g.leave_journey();
    assert_eq!(g.campaign().run_history.len(), history);
}

#[test]
fn equipped_visuals_are_authored_actual_instances_and_frozen_for_the_attempt() {
    let mut g = Game::default();
    g.campaign_mut().coins = 1000;
    let outfit = content()
        .items
        .iter()
        .position(|item| item.name == "Padded vest")
        .unwrap();
    let id = g.campaign_mut().buy_item(outfit).unwrap();
    g.campaign_mut().equip(Mode::Personal, id).unwrap();
    let profile = g.campaign().profile(Mode::Personal);
    let visual = profile.equipment_visual.outfit.as_ref().unwrap();
    assert_eq!(visual.definition, ItemDefId::at(outfit));
    assert_eq!(visual.level, g.campaign().item(id).unwrap().level);
    assert_eq!(visual.style, "armor.leather");
    assert_eq!(visual.quality, g.campaign().item_quality(id).unwrap());
    assert_eq!(
        profile.equipment_visual.ranged.as_ref().unwrap().definition,
        g.campaign()
            .item(g.campaign().player.loadout[Slot::Ranged.index()])
            .unwrap()
            .definition
    );
    g.deploy(0, Mode::Personal).unwrap();
    assert_eq!(
        g.attempt().unwrap().profile.equipment_visual,
        profile.equipment_visual
    );
    assert!(g.campaign_mut().equip(Mode::Personal, 3).is_err());
    let mut old = serde_json::to_value(profile).unwrap();
    old.as_object_mut().unwrap().remove("equipment_visual");
    let restored: horde_core::campaign::DefenderProfile = serde_json::from_value(old).unwrap();
    assert!(restored.equipment_visual.outfit.is_none());
    assert!(restored.equipment_visual.weapon_class.is_none());
    let mut invalid = g.clone();
    invalid
        .attempt_mut()
        .unwrap()
        .profile
        .equipment_visual
        .weapon_class = Some(horde_core::weapons::WeaponClass::Smg);
    let path = std::env::temp_dir().join(format!("visual-class-check-{}.json", std::process::id()));
    assert!(invalid.save(&path).is_err());
    g.attempt_mut()
        .unwrap()
        .profile
        .equipment_visual
        .weapon_class = None;
    g.save(&path).unwrap();
    assert!(
        Game::load(&path)
            .unwrap()
            .attempt()
            .unwrap()
            .profile
            .equipment_visual
            .weapon_class
            .is_none()
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn leaving_journey_persists_without_erasing_report_and_old_resume_infers_only_once() {
    let mut g = Game::default();
    g.campaign_mut().choose_survivor(SurvivorId::Rook).unwrap();
    g.campaign_mut().choose_start_country("VAT").unwrap();
    assert!(!g.campaign().journey_active);
    g.deploy_country_stage(0, Mode::Personal).unwrap();
    assert!(g.campaign().journey_active);
    g.withdraw();
    assert!(g.campaign().journey_active);
    let path = std::env::temp_dir().join(format!("journey-state-{}.json", std::process::id()));
    g.save(&path).unwrap();
    let mut resumed = Game::load(&path).unwrap();
    assert!(resumed.campaign().journey_active);
    resumed.leave_journey();
    assert!(!resumed.campaign().journey_active);
    assert!(resumed.last_result().is_some());
    let count = resumed.campaign().run_history.len();
    resumed.save(&path).unwrap();
    let resumed = Game::load(&path).unwrap();
    assert!(!resumed.campaign().journey_active);
    assert_eq!(resumed.campaign().run_history.len(), count);
    let mut check = resumed.clone();
    assert!(check.continue_journey().is_err());
    let mut old = serde_json::to_value(resumed).unwrap();
    old["campaign"]
        .as_object_mut()
        .unwrap()
        .remove("journey_active");
    old["campaign"]
        .as_object_mut()
        .unwrap()
        .remove("journey_revision");
    let payload = serde_json::to_string(&old).unwrap();
    let checksum = support::fnv1a(payload.as_bytes());
    let migrated = Game::decode_save_generation(
        &serde_json::to_vec(
            &serde_json::json!({"version":1,"checksum":checksum,"payload":payload}),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(migrated.campaign().journey_active);
    assert_eq!(migrated.campaign().journey_revision, 1);
    std::fs::remove_file(&path).unwrap();
    if path.with_extension("bak").exists() {
        std::fs::remove_file(path.with_extension("bak")).unwrap();
    }
}
