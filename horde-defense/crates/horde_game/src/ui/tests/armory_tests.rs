//! Egui harness tests: armory, market and equipment comparison.
use super::*;

#[test]
fn item_sale_requires_review_and_confirmation_before_changing_the_wallet() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let id = f.game.campaign_mut().add_item(4).unwrap();
    let price = content().items[4].sell_value;
    h.click(f, View::Armory, &format!("Review sale · {price}"));
    assert_eq!(f.armory.pending_sale, Some(id));
    assert!(f.game.campaign().item(id).is_some());
    assert_eq!(f.game.campaign().coins, 0);
    h.click(f, View::Armory, "Keep item");
    assert!(f.armory.pending_sale.is_none());
    assert!(f.game.campaign().item(id).is_some());
    h.click(f, View::Armory, &format!("Review sale · {price}"));
    h.click(f, View::Armory, "Confirm item sale");
    assert!(f.game.campaign().item(id).is_none());
    assert_eq!(f.game.campaign().coins, price);
    assert!(f.armory.pending_sale.is_none());
}

#[test]
fn preset_sale_warning_names_the_actual_reference_before_confirmation() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().coins = 65;
    f.game.campaign_mut().purchase_node("quick_reload").unwrap();
    f.game
        .campaign_mut()
        .purchase_node("rifle_training")
        .unwrap();
    let id = f.game.campaign_mut().add_item(4).unwrap();
    f.game.campaign_mut().equip(Mode::Personal, id).unwrap();
    f.game
        .campaign_mut()
        .save_preset("Patrol rifle".into(), Mode::Personal)
        .unwrap();
    let price = content().items[4].sell_value;
    h.click(f, View::Armory, &format!("Review sale · {price}"));
    let output = h.frame(f, View::Armory, vec![]);
    assert!(output.shapes.iter().any(|shape| shape_has_text(
        &shape.shape,
        "These presets will be incomplete: Patrol rifle"
    )));
    assert!(f.game.campaign().item(id).is_some());
    h.click(f, View::Armory, "Confirm item sale");
    assert_eq!(f.game.campaign().player.loadout[0], 5);
    assert!(
        f.game
            .campaign_mut()
            .apply_preset(1, Mode::Personal)
            .is_err()
    );
}

#[test]
fn large_armory_renders_only_visible_rows_and_filters_without_discarding_items() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    for _ in 0..1000 {
        f.game.campaign_mut().add_item(4).unwrap();
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
            .any(|shape| shape_has_text(&shape.shape, "0 of 1005 items"))
    );
    assert_eq!(f.game.campaign().items.len(), 1005);
}

#[test]
fn recommended_market_item_is_visible_and_purchased_only_after_explicit_input() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().first_clear(0, Mode::Personal);
    f.game.campaign_mut().coins = 200;
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
    let count = f.game.campaign().items.len();
    let coins = f.game.campaign().coins;
    h.click(
        f,
        View::Armory,
        &format!("Buy · {} coins", content().items[index].buy_value),
    );
    assert_eq!(f.game.campaign().items.len(), count + 1);
    assert_eq!(
        f.game.campaign().coins,
        coins - content().items[index].buy_value
    );
    assert!(f.campaign.improvement_focus.is_none());
}

#[test]
fn leveled_equipment_ui_uses_the_same_price_and_comparison_as_the_transaction() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().coins = 1000;
    f.game.campaign_mut().purchase_node("quick_reload").unwrap();
    f.game
        .campaign_mut()
        .purchase_node("rifle_training")
        .unwrap();
    let rifle = f.game.campaign_mut().buy_item(4).unwrap();
    f.game.campaign_mut().equip(Mode::Personal, rifle).unwrap();
    // Real replay victories raise threat; merely marking country clears no
    // longer stands in for completed runs under the current progression rule.
    for _ in 0..6 {
        f.game.deploy_country_stage(0, Mode::Personal).unwrap();
        let mut attempt = f.game.attempt_slot_mut().take().unwrap();
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
        *f.game.attempt_slot_mut() = Some(attempt);
        f.game.tick(Input {
            aim: target,
            fire: true,
            ..Input::default()
        });
        assert!(f.game.attempt().is_none());
        assert_eq!(f.game.last_result().unwrap().outcome, Outcome::Victory);
    }
    assert_eq!(f.game.campaign().victories, 6);
    f.game.campaign_mut().coins = 1000;
    let definition = content()
        .items
        .iter()
        .position(|i| i.slot == Slot::Ranged && i.tier == 1)
        .unwrap();
    let price = f.game.campaign().buy_price(definition).unwrap();
    let id = f.game.campaign_mut().buy_item(definition).unwrap();
    let item_level = f.game.campaign().item(id).unwrap().level;
    assert!(item_level > 1);
    assert_eq!(f.game.campaign().coins, 1000 - price);
    let effective = f.game.campaign().effective_item(id).unwrap();
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
            .campaign()
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
        .campaign_mut()
        .add_item_with_quality(definition, 2, Some(Quality::Poor))
        .unwrap();
    let legendary = f
        .game
        .campaign_mut()
        .add_item_with_quality(definition, 2, Some(Quality::Legendary))
        .unwrap();
    assert!(
        f.game
            .campaign()
            .effective_item(legendary)
            .unwrap()
            .personal
            .damage
            > f.game
                .campaign()
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
        .campaign_mut()
        .add_item_with_quality(4, 2, Some(quality))
        .unwrap();
    let a = f.game.attempt_mut().unwrap();
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
            .campaign()
            .items
            .iter()
            .filter(|item| item.id == id)
            .count(),
        1
    );
}

fn text_rect(output: &FullOutput, exact: &str) -> Vec<Rect> {
    fn collect(shape: &egui::epaint::Shape, exact: &str, out: &mut Vec<Rect>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == exact => {
                out.push(text.galley.rect.translate(text.pos.to_vec2()));
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().for_each(|shape| collect(shape, exact, out));
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, exact, &mut out);
    }
    out
}

#[test]
fn equip_buttons_and_equipped_labels_share_one_status_column() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().coins = 65;
    f.game.campaign_mut().purchase_node("quick_reload").unwrap();
    f.game
        .campaign_mut()
        .purchase_node("rifle_training")
        .unwrap();
    // Names of different lengths must not move the status column.
    for definition in 0..content().items.len().min(4) {
        f.game.campaign_mut().add_item(definition).unwrap();
    }
    // Newest first: the equipped rifle is the top visible row.
    let rifle = f.game.campaign_mut().add_item(4).unwrap();
    f.game.campaign_mut().equip(Mode::Personal, rifle).unwrap();
    // An equipped selection keeps the comparison header out of this check.
    f.armory.selected_item = Some(rifle);
    let output = h.frame(f, View::Armory, vec![]);
    let equipped = text_rect(&output, "Equipped");
    let equip = text_rect(&output, "Equip");
    assert!(
        !equipped.is_empty() && equip.len() >= 2,
        "{equipped:?} {equip:?}"
    );
    let centre = equipped[0].center().x;
    for rect in equipped.iter().chain(&equip) {
        assert!(
            (rect.center().x - centre).abs() < 1.0,
            "status column is ragged: {centre} vs {rect:?}"
        );
    }
}

#[test]
fn equipped_selection_says_so_instead_of_repeating_identical_columns() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().coins = 65;
    f.game.campaign_mut().purchase_node("quick_reload").unwrap();
    f.game
        .campaign_mut()
        .purchase_node("rifle_training")
        .unwrap();
    let rifle = f.game.campaign_mut().add_item(4).unwrap();
    let spare = f.game.campaign_mut().add_item(4).unwrap();
    f.game.campaign_mut().equip(Mode::Personal, rifle).unwrap();
    let has = |output: &FullOutput, text: &str| {
        output.shapes.iter().any(|s| shape_has_text(&s.shape, text))
    };
    f.armory.selected_item = Some(rifle);
    let output = h.frame(f, View::Armory, vec![]);
    assert!(has(&output, crate::ui::armory::CURRENTLY_EQUIPPED));
    assert!(has(&output, "Current loadout"));
    assert!(!has(&output, "With this item"));
    let before = f.game.campaign().player.loadout;
    h.click(f, View::Armory, "Equip selected");
    assert_eq!(f.game.campaign().player.loadout, before);
    f.armory.selected_item = Some(spare);
    let output = h.frame(f, View::Armory, vec![]);
    assert!(!has(&output, crate::ui::armory::CURRENTLY_EQUIPPED));
    assert!(has(&output, "With this item"));
}

#[test]
fn armory_filter_row_shares_one_centre_line() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let output = h.frame(f, View::Armory, vec![]);
    let centre = |text: &str| {
        text_rect(&output, text)
            .first()
            .unwrap_or_else(|| panic!("{text} not painted"))
            .center()
            .y
    };
    let row = centre("Search equipment");
    for sibling in ["All slots", "All qualities", "Newest first"] {
        let y = centre(sibling);
        assert!((y - row).abs() < 1.0, "{sibling} at {y}, search at {row}");
    }
}

/// The newest owned item's row label, the first row under "Newest first".
fn newest_row_label(f: &FrontendParts) -> String {
    let item = f
        .game
        .campaign()
        .items
        .iter()
        .max_by_key(|item| item.id)
        .unwrap();
    format!(
        "{} #{} · Lv {}",
        item.definition().name,
        item.id,
        item.level
    )
}

#[test]
fn equipment_shows_inventory_above_the_fold_at_both_review_sizes() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    for definition in 0..6 {
        f.game.campaign_mut().add_item(definition).unwrap();
    }
    f.act(|c| c.open_preparation(Tab::Armory));
    let first = newest_row_label(f);
    for (size, scale) in [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)] {
        settings::configure(&h.ctx, scale);
        f.settings.scale = scale;
        let output = h.frame_size(f, View::WorldWithChrome, vec![], size);
        assert!(
            fully_visible(&output, &first),
            "{size:?}: first inventory row {first:?} is not visible: {:?}",
            clipped_bounds(&output, &first)
        );
        // The row's status column is on screen with it.
        let (row, clip) = clipped_bounds(&output, &first).unwrap();
        let status = text_rect(&output, "Equip")
            .into_iter()
            .chain(text_rect(&output, "Equipped"))
            .find(|rect| (rect.center().y - row.center().y).abs() < 40.0)
            .expect("status cell beside the first row");
        assert!(
            clip.contains_rect(status),
            "{size:?}: status {status:?} clipped"
        );
        // The legend no longer repeats the quality filter.
        assert_eq!(text_count(&output, "Poor"), 0, "{size:?}");
    }
}

#[test]
fn equipment_keeps_its_loadout_comparison_on_screen_at_desktop_size() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.act(|c| c.open_preparation(Tab::Armory));
    let output = h.frame_size(f, View::WorldWithChrome, vec![], vec2(1440.0, 900.0));
    for label in ["Equip selected", "Maximum health"] {
        assert!(
            fully_visible(&output, label),
            "{label}: {:?}",
            clipped_bounds(&output, label)
        );
    }
}
