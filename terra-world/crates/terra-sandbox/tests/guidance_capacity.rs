use terra_core::GeoPosition;
use terra_sandbox::*;

fn world() -> (SandboxState, Catalog, u64) {
    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let mut state = SandboxState::default();
    state.add_player(1, origin).unwrap();
    state.seed_living_demo(origin, &[], &catalog).unwrap();
    state.advance_settlements(0., &catalog).unwrap();
    let id = *state.settlements.contracts.keys().next().unwrap();
    // Reuse a funded fixture's escrow; the selected item is authored policy.
    let contract = state.settlements.contracts.get_mut(&id).unwrap();
    contract.item = "planks".into();
    contract.amount = 8;
    contract.claimed_by = Some(1);
    for shop in state.living.shops.values_mut() {
        shop.stock.remove("planks");
    }
    state.resource_nodes.clear();
    (state, catalog, id)
}

fn guidance(state: &SandboxState, catalog: &Catalog, id: u64) -> ActivityGuidance {
    let before = state.clone();
    let result = state.activity_guidance(1, catalog, GuideTrack::Supply, Some(id));
    assert_eq!(*state, before, "capacity inspection must be read-only");
    result
}

fn execute(state: &mut SandboxState, catalog: &Catalog, guide: ActivityGuidance) {
    let Some(GuideAction::Command(command)) = guide.action else {
        panic!("expected an executable next acquisition");
    };
    state.apply(1, command, catalog).unwrap();
}

#[test]
fn multi_output_crafting_chooses_the_largest_complete_exchange_that_fits() {
    let (mut state, mut catalog, id) = world();
    catalog
        .recipes
        .get_mut("planks")
        .unwrap()
        .outputs
        .insert("stone".into(), 2);
    state.players.get_mut(&1).unwrap().inventory =
        Inventory::from([("wood".into(), 4), ("stone".into(), MAX_ITEM_COUNT - 2)]);
    catalog.validate().unwrap();
    state.validate_with_catalog(&catalog).unwrap();
    let first = guidance(&state, &catalog, id);
    assert!(
        matches!(&first.action, Some(GuideAction::Command(Command::Craft { recipe, count: 1 })) if recipe == "planks")
    );
    execute(&mut state, &catalog, first);
    assert_eq!(state.players[&1].inventory["planks"], 4);
    assert_eq!(state.players[&1].inventory["stone"], MAX_ITEM_COUNT);
    assert_eq!(state.players[&1].inventory["wood"], 2);
    let blocked = guidance(&state, &catalog, id);
    assert!(blocked.detail.contains("including every crafting output"));
    assert!(!matches!(
        blocked.action,
        Some(GuideAction::Command(Command::Craft { .. }))
    ));
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn full_byproduct_stack_prefers_a_real_finished_item_seller() {
    let (mut state, mut catalog, id) = world();
    catalog
        .recipes
        .get_mut("planks")
        .unwrap()
        .outputs
        .insert("stone".into(), 1);
    state.players.get_mut(&1).unwrap().inventory =
        Inventory::from([("wood".into(), 4), ("stone".into(), MAX_ITEM_COUNT)]);
    let shop = *state.living.shops.keys().next().unwrap();
    state
        .living
        .shops
        .get_mut(&shop)
        .unwrap()
        .stock
        .insert("planks".into(), 8);
    state
        .set_player_position(1, state.living.shops[&shop].position)
        .unwrap();
    let guide = guidance(&state, &catalog, id);
    assert!(matches!(guide.action, Some(GuideAction::OpenMarket { shop_id }) if shop_id == shop));
    let quote = state
        .personal_trade_quote(1, shop, "planks", 8, true, &catalog)
        .unwrap();
    state
        .apply(
            1,
            Command::Living {
                action: LivingCommand::TradeQuoted {
                    shop_id: shop,
                    item: "planks".into(),
                    amount: 8,
                    buy: true,
                    quoted_total: quote.total_cost,
                },
            },
            &catalog,
        )
        .unwrap();
    assert_eq!(state.players[&1].inventory["planks"], 8);
    assert_eq!(state.players[&1].inventory["stone"], MAX_ITEM_COUNT);
    assert_eq!(
        state.players[&1].inventory["wood"], 4,
        "blocked recipe spent no inputs"
    );
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn consuming_inputs_is_counted_before_crediting_all_recipe_outputs() {
    let (mut state, mut catalog, id) = world();
    let recipe = catalog.recipes.get_mut("planks").unwrap();
    recipe.inputs.insert("stone".into(), 2);
    recipe.outputs.insert("stone".into(), 2);
    state.players.get_mut(&1).unwrap().inventory =
        Inventory::from([("wood".into(), 4), ("stone".into(), MAX_ITEM_COUNT)]);
    catalog.validate().unwrap();
    let guide = guidance(&state, &catalog, id);
    assert!(
        matches!(&guide.action, Some(GuideAction::Command(Command::Craft { recipe, count: 2 })) if recipe == "planks")
    );
    execute(&mut state, &catalog, guide);
    assert_eq!(state.players[&1].inventory["stone"], MAX_ITEM_COUNT);
    assert_eq!(state.players[&1].inventory["planks"], 8);
    assert!(!state.players[&1].inventory.contains_key("wood"));
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn a_partial_affordable_recipe_batch_makes_progress_before_more_input_acquisition() {
    let (mut state, catalog, id) = world();
    state.players.get_mut(&1).unwrap().inventory = Inventory::from([("wood".into(), 2)]);
    let guide = guidance(&state, &catalog, id);
    assert!(
        matches!(&guide.action, Some(GuideAction::Command(Command::Craft { recipe, count: 1 })) if recipe == "planks")
    );
    execute(&mut state, &catalog, guide);
    assert_eq!(state.players[&1].inventory["planks"], 4);
    assert!(!state.players[&1].inventory.contains_key("wood"));
    let next = guidance(&state, &catalog, id);
    assert!(!matches!(
        next.action,
        Some(GuideAction::Command(Command::Craft { .. }))
    ));
}

#[test]
fn full_valid_catalog_inventory_still_allows_consumed_inputs_to_free_slots() {
    let (mut state, mut catalog, id) = world();
    let needed = MAX_INVENTORY_ITEMS - catalog.items.len();
    for index in 0..needed {
        catalog.items.insert(format!("filler_{index}"));
    }
    state.players.get_mut(&1).unwrap().inventory =
        catalog.items.iter().map(|item| (item.clone(), 1)).collect();
    let stock = &mut state.players.get_mut(&1).unwrap().inventory;
    stock.insert("planks".into(), 0);
    stock.insert("wood".into(), 2);
    state.settlements.contracts.get_mut(&id).unwrap().amount = 4;
    assert_eq!(state.players[&1].inventory.len(), MAX_INVENTORY_ITEMS);
    catalog.validate().unwrap();
    state.validate_with_catalog(&catalog).unwrap();
    let guide = guidance(&state, &catalog, id);
    assert!(matches!(
        &guide.action,
        Some(GuideAction::Command(Command::Craft { count: 1, .. }))
    ));
    execute(&mut state, &catalog, guide);
    assert_eq!(state.players[&1].inventory.len(), MAX_INVENTORY_ITEMS - 1);
    assert_eq!(state.players[&1].inventory["planks"], 4);
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn near_full_input_stack_gathers_only_the_needed_headroom_then_crafts() {
    let (mut state, mut catalog, id) = world();
    state.settlements.contracts.get_mut(&id).unwrap().amount = 1;
    let recipe = catalog.recipes.get_mut("planks").unwrap();
    recipe.inputs = Inventory::from([("wood".into(), MAX_ITEM_COUNT)]);
    recipe.outputs = Inventory::from([("planks".into(), 1)]);
    state.players.get_mut(&1).unwrap().inventory =
        Inventory::from([("wood".into(), MAX_ITEM_COUNT - 2)]);
    let position = state.players[&1].position;
    state.resource_nodes.insert(
        "last-wood".into(),
        ResourceNode {
            id: "last-wood".into(),
            position,
            item: "wood".into(),
            remaining: 5,
            capacity: 5,
            regrow_seconds: 0.,
            regrow_accumulator: 0.,
            source_feature: None,
        },
    );
    catalog.validate().unwrap();
    state.validate_with_catalog(&catalog).unwrap();
    let gather = guidance(&state, &catalog, id);
    assert!(
        matches!(&gather.action, Some(GuideAction::Command(Command::Gather { node_id, amount: 2 })) if node_id == "last-wood")
    );
    execute(&mut state, &catalog, gather);
    assert_eq!(state.players[&1].inventory["wood"], MAX_ITEM_COUNT);
    assert_eq!(state.resource_nodes["last-wood"].remaining, 3);
    let craft = guidance(&state, &catalog, id);
    assert!(matches!(
        &craft.action,
        Some(GuideAction::Command(Command::Craft { count: 1, .. }))
    ));
    execute(&mut state, &catalog, craft);
    assert_eq!(state.players[&1].inventory["planks"], 1);
    assert!(!state.players[&1].inventory.contains_key("wood"));
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn adversarial_unvalidated_sdk_inventory_gets_capacity_recovery_instead_of_gather() {
    let (mut state, catalog, id) = world();
    let contract = state.settlements.contracts.get_mut(&id).unwrap();
    contract.item = "wood".into();
    contract.amount = 5;
    // A fully catalog-valid world cannot have 64 *other* kinds: the catalog
    // itself has a 64-kind bound. This explicitly tests raw SDK input; it must
    // not be mislabeled a valid saved-world fixture.
    state.players.get_mut(&1).unwrap().inventory = (0..MAX_INVENTORY_ITEMS)
        .map(|index| (format!("unchecked_{index}"), 1))
        .collect();
    let position = state.players[&1].position;
    state.resource_nodes.insert(
        "blocked-wood".into(),
        ResourceNode {
            id: "blocked-wood".into(),
            position,
            item: "wood".into(),
            remaining: 5,
            capacity: 5,
            regrow_seconds: 0.,
            regrow_accumulator: 0.,
            source_feature: None,
        },
    );
    assert!(state.validate_with_catalog(&catalog).is_err());
    let guide = guidance(&state, &catalog, id);
    assert!(guide.detail.contains("inventory cannot fit"));
    assert!(!matches!(
        guide.action,
        Some(GuideAction::Command(Command::Gather { .. }))
    ));
    let before = state.clone();
    assert!(matches!(
        state.apply(
            1,
            Command::Gather {
                node_id: "blocked-wood".into(),
                amount: 1
            },
            &catalog
        ),
        Err(SandboxError::Capacity)
    ));
    assert_eq!(state, before);
}
