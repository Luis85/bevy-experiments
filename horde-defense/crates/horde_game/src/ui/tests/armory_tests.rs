//! Egui harness tests: armory, market and equipment comparison.
use super::*;

#[test]
fn item_sale_requires_review_and_confirmation_before_changing_the_wallet() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let id = f.game.campaign.add_item(4).unwrap();
    let price = content().items[4].sell_value;
    h.click(f, View::Armory, &format!("Review sale · {price}"));
    assert_eq!(f.armory.pending_sale, Some(id));
    assert!(f.game.campaign.item(id).is_some());
    assert_eq!(f.game.campaign.coins, 0);
    h.click(f, View::Armory, "Keep item");
    assert!(f.armory.pending_sale.is_none());
    assert!(f.game.campaign.item(id).is_some());
    h.click(f, View::Armory, &format!("Review sale · {price}"));
    h.click(f, View::Armory, "Confirm item sale");
    assert!(f.game.campaign.item(id).is_none());
    assert_eq!(f.game.campaign.coins, price);
    assert!(f.armory.pending_sale.is_none());
}

#[test]
fn preset_sale_warning_names_the_actual_reference_before_confirmation() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.coins = 65;
    f.game.campaign.purchase_node("quick_reload").unwrap();
    f.game.campaign.purchase_node("rifle_training").unwrap();
    let id = f.game.campaign.add_item(4).unwrap();
    f.game.campaign.equip(Mode::Personal, id).unwrap();
    f.game
        .campaign
        .save_preset("Patrol rifle".into(), Mode::Personal)
        .unwrap();
    let price = content().items[4].sell_value;
    h.click(f, View::Armory, &format!("Review sale · {price}"));
    let output = h.frame(f, View::Armory, vec![]);
    assert!(output.shapes.iter().any(|shape| shape_has_text(
        &shape.shape,
        "These presets will be incomplete: Patrol rifle"
    )));
    assert!(f.game.campaign.item(id).is_some());
    h.click(f, View::Armory, "Confirm item sale");
    assert_eq!(f.game.campaign.player.loadout[0], 5);
    assert!(f.game.campaign.apply_preset(1, Mode::Personal).is_err());
}

#[test]
fn large_armory_renders_only_visible_rows_and_filters_without_discarding_items() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    for _ in 0..1000 {
        f.game.campaign.add_item(4).unwrap();
    }
    let output = h.frame(f, View::Armory, vec![]);
    let rows = output
        .shapes
        .iter()
        .filter(|shape| shape_has_text(&shape.shape, " #"))
        .count();
    assert!(
        rows > 0 && rows < 20,
        "Expected a bounded visible row set, got {rows}"
    );
    assert!(
        output.shapes.iter().any(|shape| shape_has_text(
            &shape.shape,
            &format!("{} #1005", content().items[4].name)
        ))
    );
    f.armory.search = "no-matching-equipment-name".into();
    let output = h.frame(f, View::Armory, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| shape_has_text(&shape.shape, "Showing 0 of 1005 items"))
    );
    assert_eq!(f.game.campaign.items.len(), 1005);
}

#[test]
fn recommended_market_item_is_visible_and_purchased_only_after_explicit_input() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.first_clear(0, Mode::Personal);
    f.game.campaign.coins = 200;
    let index = content()
        .items
        .iter()
        .position(|i| i.tier == 1 && i.slot == Slot::Ranged)
        .unwrap();
    f.campaign.improvement_focus = Some(horde_core::campaign::RecommendationAction::BuyEquipment(
        index,
    ));
    let before = serde_json::to_value(&f.game).unwrap();
    let output = h.frame(f, View::Armory, vec![]);
    assert!(output.shapes.iter().any(|s| shape_has_text(
        &s.shape,
        &format!("Recommended: {}", content().items[index].name)
    )));
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    let count = f.game.campaign.items.len();
    let coins = f.game.campaign.coins;
    h.click(
        f,
        View::Armory,
        &format!("Buy · {} coins", content().items[index].buy_value),
    );
    assert_eq!(f.game.campaign.items.len(), count + 1);
    assert_eq!(
        f.game.campaign.coins,
        coins - content().items[index].buy_value
    );
    assert!(f.campaign.improvement_focus.is_none());
}

#[test]
fn leveled_equipment_ui_uses_the_same_price_and_comparison_as_the_transaction() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.choose_start_country("DEU").unwrap();
    f.game.campaign.coins = 1000;
    f.game.campaign.purchase_node("quick_reload").unwrap();
    f.game.campaign.purchase_node("rifle_training").unwrap();
    let rifle = f.game.campaign.buy_item(4).unwrap();
    f.game.campaign.equip(Mode::Personal, rifle).unwrap();
    // Real replay victories raise threat; merely marking country clears no
    // longer stands in for completed runs under the current progression rule.
    for _ in 0..6 {
        f.game.deploy_country_stage(0, Mode::Personal).unwrap();
        let mut attempt = f.game.attempt.take().unwrap();
        attempt.boss_spawned = true;
        let lane = attempt.lane_count / 2;
        f.game.spawn_enemy(
            &mut attempt,
            horde_core::content::EnemyKind::Brute,
            lane,
            true,
        );
        let target = [8.0, attempt.defender_position()[1]];
        let boss = attempt.enemies.last_mut().unwrap();
        boss.x = target[0];
        boss.hp = 1;
        f.game.attempt = Some(attempt);
        f.game.tick(Input {
            aim: target,
            fire: true,
            ..Input::default()
        });
        assert!(f.game.attempt.is_none());
        assert_eq!(
            f.game.last_result.as_ref().unwrap().outcome,
            Outcome::Victory
        );
    }
    assert_eq!(f.game.campaign.victories, 6);
    f.game.campaign.coins = 1000;
    let definition = content()
        .items
        .iter()
        .position(|i| i.slot == Slot::Ranged && i.tier == 1)
        .unwrap();
    let price = f.game.campaign.buy_price(definition).unwrap();
    let id = f.game.campaign.buy_item(definition).unwrap();
    let item_level = f.game.campaign.item(id).unwrap().level;
    assert!(item_level > 1);
    assert_eq!(f.game.campaign.coins, 1000 - price);
    let effective = f.game.campaign.effective_item(id).unwrap();
    assert!(effective.personal.damage > content().items[definition].personal.damage);
    f.armory.selected_item = Some(id);
    let output = h.frame(f, View::Armory, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| shape_has_text(&shape.shape, &format!("Lv {}", item_level)))
    );
    assert_eq!(
        f.game
            .campaign
            .compare_item(Mode::Personal, id)
            .unwrap()
            .after
            .damage,
        effective.personal.damage
    );
}

#[test]
fn quality_names_and_filter_sort_use_owned_quality_without_awarding_anything() {
    use horde_core::quality::Quality;
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let definition = content()
        .items
        .iter()
        .position(|i| i.slot == Slot::Ranged && i.tier == 1)
        .unwrap();
    let poor = f
        .game
        .campaign
        .add_item_with_quality(definition, 2, Some(Quality::Poor))
        .unwrap();
    let legendary = f
        .game
        .campaign
        .add_item_with_quality(definition, 2, Some(Quality::Legendary))
        .unwrap();
    assert!(
        f.game
            .campaign
            .effective_item(legendary)
            .unwrap()
            .personal
            .damage
            > f.game
                .campaign
                .effective_item(poor)
                .unwrap()
                .personal
                .damage
    );
    f.armory.sort = armory::ArmorySort::Quality;
    f.armory.quality = Some(Quality::Legendary);
    let before = serde_json::to_value(&f.game).unwrap();
    let output = h.frame(f, View::Armory, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, &format!("#{}", legendary)))
    );
    assert!(
        !output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, &format!("#{}", poor)))
    );
    assert!(
        output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, "Legendary"))
    );
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn visible_quality_drop_receipts_never_grant_the_item_or_coins_twice() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let quality = horde_core::quality::Quality::Rare;
    let id = f
        .game
        .campaign
        .add_item_with_quality(4, 2, Some(quality))
        .unwrap();
    let a = f.game.attempt.as_mut().unwrap();
    a.loot_events.push(horde_core::quality::LootEvent {
        item_id: id,
        definition: horde_core::content::ItemDefId::at(4),
        level: 2,
        quality,
        name: content().items[4].name.clone(),
        tick: a.tick,
        at: [20.0, 5.5],
    });
    let before = serde_json::to_value(&f.game).unwrap();
    for _ in 0..3 {
        let output = h.frame(f, View::Battle, vec![]);
        assert!(
            output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "Last loot · already banked"))
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "Rare"))
        );
    }
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    assert_eq!(
        f.game
            .campaign
            .items
            .iter()
            .filter(|item| item.id == id)
            .count(),
        1
    );
}
