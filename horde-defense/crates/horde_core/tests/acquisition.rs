use horde_core::{
    campaign::{Campaign, RecommendationAction},
    content::{ItemDefinition, Mode, content},
};

fn trained_campaign() -> Campaign {
    let mut campaign = Campaign::default();
    campaign.reward(130, 0, Mode::Personal);
    campaign.purchase_node("quick_reload").unwrap();
    campaign.purchase_node("rifle_training").unwrap();
    campaign
}

#[test]
fn npc_issued_rifle_stays_unavailable_to_fresh_personal_loadouts_after_training() {
    let mut campaign = trained_campaign();
    assert!(content().items[0].npc_only);
    assert!(campaign.can_use_weapon(Mode::Npc, 0));
    assert!(!campaign.can_use_weapon(Mode::Personal, 0));
    let reason = campaign.weapon_access(Mode::Personal, 0).unwrap_err();
    assert!(reason.contains("NPC issued gear") && reason.contains("Buy or loot"));
    let before = campaign.clone();
    assert_eq!(campaign.equip(Mode::Personal, 1).unwrap_err(), reason);
    assert_eq!(campaign, before);

    campaign
        .save_preset("Mara's issued equipment".into(), Mode::Npc)
        .unwrap();
    let preset = campaign.presets.len() - 1;
    let before = campaign.clone();
    assert_eq!(
        campaign.apply_preset(preset, Mode::Personal).unwrap_err(),
        reason
    );
    assert_eq!(campaign, before);

    campaign.player.loadout[0] = 1;
    let before = campaign.clone();
    assert_eq!(campaign.reserve(Mode::Personal).unwrap_err(), reason);
    assert_eq!(campaign, before);
    assert_eq!(campaign.reserved, None);
}

#[test]
fn personal_rifles_require_a_real_purchase_or_loot_instance_as_well_as_training() {
    for purchased in [false, true] {
        let mut campaign = trained_campaign();
        let before = campaign.coins;
        let id = if purchased {
            campaign.buy_item(4).unwrap()
        } else {
            campaign.add_item(4).unwrap()
        };
        assert_ne!(id, 1);
        assert!(!content().items[campaign.item(id).unwrap().definition].npc_only);
        assert_eq!(
            campaign.coins,
            before
                - if purchased {
                    content().items[4].buy_value
                } else {
                    0
                }
        );
        campaign.equip(Mode::Personal, id).unwrap();
        assert_eq!(campaign.player.loadout[0], id);
        campaign.reserve(Mode::Personal).unwrap();
        campaign.validate().unwrap();
    }
}

#[test]
fn recommendations_do_not_offer_training_to_unlock_npc_issued_equipment() {
    let mut campaign = Campaign::default();
    campaign.reward(200, 0, Mode::Personal);
    campaign.purchase_node("quick_reload").unwrap();
    let recommendation = campaign.recommendation(Mode::Personal);
    assert!(
        !matches!(recommendation.action, RecommendationAction::LearnNode(ref id) if id == "rifle_training")
    );
    assert!(!recommendation.title.contains(&content().items[0].name));
}

#[test]
fn item_catalogs_without_npc_only_metadata_keep_their_previous_access_semantics() {
    let mut value = serde_json::to_value(&content().items[0]).unwrap();
    value.as_object_mut().unwrap().remove("npc_only");
    let restored: ItemDefinition = serde_json::from_value(value).unwrap();
    assert!(!restored.npc_only);
}
