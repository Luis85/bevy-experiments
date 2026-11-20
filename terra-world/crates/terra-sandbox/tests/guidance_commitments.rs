use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

fn world() -> (SandboxState, Catalog) {
    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let mut state = SandboxState::default();
    state.add_player(1, origin).unwrap();
    state.add_player(2, origin).unwrap();
    state.seed_living_demo(origin, &[], &catalog).unwrap();
    state.ensure_transport_demo(&catalog).unwrap();
    state.advance_settlements(0., &catalog).unwrap();
    (state, catalog)
}

fn accept(state: &mut SandboxState, catalog: &Catalog, player: u64, id: u64) {
    state
        .apply(
            player,
            Command::Settlement {
                action: SettlementCommand::AcceptContract { contract_id: id },
            },
            catalog,
        )
        .unwrap();
}

// These comparison/intent fixtures author a different requested item. Keep the
// receiving recipe's actual demand aligned with that item before acceptance;
// 0.24 deliberately refuses a public request that its receiver does not need.
fn request_item(state: &mut SandboxState, id: u64, item: &str, amount: u32) {
    let contract = state.settlements.contracts.get_mut(&id).unwrap();
    contract.item = item.into();
    contract.amount = amount;
    let receiver = state
        .living
        .producers
        .get_mut(&contract.target_site)
        .unwrap();
    let current = receiver.stock.get(item).copied().unwrap_or(0);
    receiver
        .inputs
        .insert(item.into(), (current + amount).div_ceil(12));
}

#[test]
fn supply_deadlines_keep_the_last_minutes_and_expiry_distinct() {
    assert_eq!(supply_deadline_label(7200.), "2.0 world hours left");
    assert_eq!(supply_deadline_label(3600.), "1.0 world hour left");
    assert_eq!(supply_deadline_label(120.), "2 world minutes left");
    assert_eq!(supply_deadline_label(60.1), "2 world minutes left");
    assert_eq!(supply_deadline_label(60.), "1 world minute left");
    assert_eq!(supply_deadline_label(0.1), "1 world second left");
    assert_eq!(supply_deadline_label(0.), "deadline passed");
    assert_eq!(supply_deadline_label(-1.), "deadline passed");
}

#[test]
fn commitments_are_owner_scoped_bounded_read_only_and_rebuilt_after_save() {
    let (mut state, catalog) = world();
    let ids: Vec<_> = state
        .settlements
        .contracts
        .keys()
        .copied()
        .take(4)
        .collect();
    assert_eq!(ids.len(), 4);
    for id in &ids[..3] {
        accept(&mut state, &catalog, 1, *id);
    }
    accept(&mut state, &catalog, 2, ids[3]);
    state
        .settlements
        .contracts
        .get_mut(&ids[0])
        .unwrap()
        .expires_at = 8000.;
    state
        .settlements
        .contracts
        .get_mut(&ids[1])
        .unwrap()
        .expires_at = 4000.;
    state
        .settlements
        .contracts
        .get_mut(&ids[2])
        .unwrap()
        .expires_at = 4000.;

    let before = state.clone();
    let briefing = state.activity_commitments(1).unwrap();
    assert_eq!(state, before);
    assert_eq!(briefing.supply_requests.len(), 3);
    assert_eq!(
        briefing
            .supply_requests
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>(),
        vec![ids[1], ids[2], ids[0]]
    );
    assert!(!briefing.detail.contains(&format!("#{}", ids[3])));
    assert!(briefing.detail.contains("world hours left"));
    assert!(state.activity_commitments(999).is_none());
    let saved = serde_json::to_vec(&state).unwrap();
    let restored: SandboxState = serde_json::from_slice(&saved).unwrap();
    assert_eq!(briefing, restored.activity_commitments(1).unwrap());
    assert_eq!(saved, serde_json::to_vec(&state).unwrap());
}

#[test]
fn default_supply_prioritizes_deadline_but_explicit_accepted_choice_stays_exact() {
    let (mut state, catalog) = world();
    let ids: Vec<_> = state
        .settlements
        .contracts
        .keys()
        .copied()
        .take(3)
        .collect();
    accept(&mut state, &catalog, 1, ids[0]);
    accept(&mut state, &catalog, 1, ids[1]);
    state
        .settlements
        .contracts
        .get_mut(&ids[0])
        .unwrap()
        .expires_at = 9000.;
    state
        .settlements
        .contracts
        .get_mut(&ids[1])
        .unwrap()
        .expires_at = 2000.;
    let selected = |state: &SandboxState, preferred| {
        state
            .activity_guidance(1, &catalog, GuideTrack::Supply, preferred)
            .supply_contract
            .unwrap()
            .id
    };
    assert_eq!(selected(&state, None), ids[1]);
    assert_eq!(selected(&state, Some(ids[2])), ids[1]);
    assert_eq!(selected(&state, Some(ids[0])), ids[0]);
    state
        .settlements
        .contracts
        .get_mut(&ids[0])
        .unwrap()
        .expires_at = 2000.;
    assert_eq!(selected(&state, None), ids[0], "stable ID tie break");
}

#[test]
fn supply_intent_carries_request_even_when_gathering_command_matches() {
    let (mut state, catalog) = world();
    let ids: Vec<_> = state
        .settlements
        .contracts
        .keys()
        .copied()
        .take(2)
        .collect();
    for id in &ids {
        request_item(&mut state, *id, "wood", 10);
    }
    state.players.get_mut(&1).unwrap().inventory.remove("wood");
    accept(&mut state, &catalog, 1, ids[0]);
    accept(&mut state, &catalog, 1, ids[1]);
    let shown = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(ids[0]));
    let other = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(ids[1]));
    assert!(
        shown
            .action
            .as_ref()
            .unwrap()
            .same_intent(other.action.as_ref().unwrap())
    );
    assert!(!shown.same_intent(&other));
    state.settlements.elapsed_seconds += 1.;
    assert!(shown.same_intent(&state.activity_guidance(
        1,
        &catalog,
        GuideTrack::Supply,
        Some(ids[0]),
    )));
    for change in 0..4 {
        let mut changed = state.clone();
        let contract = changed.settlements.contracts.get_mut(&ids[0]).unwrap();
        match change {
            0 => contract.reward += 1,
            1 => contract.expires_at += 60.,
            2 => contract.amount += 1,
            _ => contract.target_position.altitude_m += 1.,
        }
        let current = changed.activity_guidance(1, &catalog, GuideTrack::Supply, Some(ids[0]));
        assert!(!shown.same_intent(&current), "change {change}");
    }
}

#[test]
fn supply_brief_compares_complete_missing_quantity_without_claiming_profit_or_reservation() {
    let (mut state, catalog) = world();
    let id = *state.settlements.contracts.keys().next().unwrap();
    let contract = state.settlements.contracts.get_mut(&id).unwrap();
    contract.item = "wood".into();
    contract.amount = 4;
    contract.reward = 1000;
    state.players.get_mut(&1).unwrap().inventory.remove("wood");
    state.living.accounts.get_mut(&1).unwrap().money = 2000;
    for shop in state.living.shops.values_mut() {
        shop.stock.insert("wood".into(), 0);
    }
    let mut other_seller = state.living.shops.values().next().unwrap().clone();
    other_seller.id = state.living.next_id;
    state.living.next_id += 1;
    state.living.shops.insert(other_seller.id, other_seller);
    let sellers: Vec<_> = state.living.shops.keys().copied().take(2).collect();
    let seller = state.living.shops.get_mut(&sellers[0]).unwrap();
    seller.name = "Complete stock".into();
    seller.stock.insert("wood".into(), 4);
    seller.prices.insert(
        "wood".into(),
        TradePrice {
            buy: 900,
            sell: 100,
        },
    );
    let short = state.living.shops.get_mut(&sellers[1]).unwrap();
    short.name = "Insufficient stock".into();
    short.stock.insert("wood".into(), 3);
    short
        .prices
        .insert("wood".into(), TradePrice { buy: 1, sell: 1 });
    let before = state.clone();
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert_eq!(state, before);
    assert!(guide.detail.contains("4 wood at Complete stock, 36.00 cr"));
    assert!(
        guide
            .detail
            .contains("exceeds the gross reward by 26.00 cr")
    );
    assert!(guide.detail.contains("Gross reserved reward: 10.00 cr"));
    assert!(
        guide
            .detail
            .contains("Wallet: 20.00 cr; shortfall: 16.00 cr")
    );
    assert!(guide.detail.contains("prices and stock can change"));
    assert!(!guide.detail.contains("Insufficient stock"));

    state
        .living
        .shops
        .get_mut(&sellers[0])
        .unwrap()
        .stock
        .insert("wood".into(), 3);
    let unavailable = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(
        unavailable
            .detail
            .contains("No full-quantity price comparison is available")
    );
    state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("wood".into(), 4);
    let ready = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(ready.detail.contains("acquisition cost is not recorded"));
    assert!(ready.detail.contains("not a profit figure"));
}

#[test]
fn accepted_supply_shows_one_complete_comparison_without_retargeting_its_market_action() {
    let (mut state, catalog) = world();
    let origin = state.players[&1].position;
    let id = *state.settlements.contracts.keys().next().unwrap();
    request_item(&mut state, id, "wheat", 6);
    let contract = state.settlements.contracts.get_mut(&id).unwrap();
    contract.reward = 900;
    contract.experience = 40;
    state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("wheat".into(), 2);
    state.resource_nodes.clear();
    for shop in state.living.shops.values_mut() {
        shop.stock.remove("wheat");
    }
    let nearest = *state.living.shops.keys().next().unwrap();
    let shop = state.living.shops.get_mut(&nearest).unwrap();
    shop.name = "Nearby partial stock".into();
    shop.position = origin;
    shop.money = 0;
    shop.stock.insert("wheat".into(), 2);
    shop.prices
        .insert("wheat".into(), TradePrice { buy: 100, sell: 50 });
    let mut complete = shop.clone();
    complete.id = state.living.next_id;
    state.living.next_id += 1;
    complete.name = "Full-quantity comparison".into();
    complete.position = LocalFrame::new(origin).to_geo([150., 0., 0.]).unwrap();
    complete.stock.insert("wheat".into(), 4);
    complete
        .prices
        .insert("wheat".into(), TradePrice { buy: 120, sell: 50 });
    let comparison_id = complete.id;
    let comparison_position = complete.position;
    state.living.shops.insert(complete.id, complete);
    accept(&mut state, &catalog, 1, id);

    let before = state.clone();
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert_eq!(
        state, before,
        "a comparison does not reserve or spend anything"
    );
    assert_eq!(guide.step, 2);
    assert!(
        matches!(guide.action, Some(GuideAction::OpenMarket { shop_id }) if shop_id == nearest)
    );
    assert!(
        guide
            .detail
            .contains("Nearby partial stock's live wheat quote")
    );
    assert!(
        guide
            .detail
            .contains("4 wheat at Full-quantity comparison, 4.80 cr")
    );
    assert!(
        guide
            .detail
            .contains("Gross reserved reward: 9.00 cr + 40 community XP")
    );
    assert_eq!(guide.detail.matches("4.80 cr").count(), 1);
    assert!(!guide.detail.contains("credits each"));
    assert!(
        !guide.detail.contains("Wallet:"),
        "sufficient cash need not repeat the commitments wallet"
    );
    assert!(guide.detail.contains("Deliver all 6 wheat to"));
    assert!(guide.detail.contains("within 20 m"));
    assert!(guide.detail.contains("world clock"));
    assert!(
        guide
            .detail
            .contains("Carried goods and travel are unpriced")
    );
    assert!(
        guide
            .detail
            .contains("Gathering or crafting remain alternatives")
    );

    // The full-quantity arithmetic agrees with the real quote after reaching
    // that seller; the next action still names the nearer, partially stocked one.
    let mut at_comparison_seller = state.clone();
    at_comparison_seller.players.get_mut(&1).unwrap().position = comparison_position;
    assert_eq!(
        at_comparison_seller
            .personal_trade_quote(1, comparison_id, "wheat", 4, true, &catalog)
            .unwrap()
            .total_cost,
        480
    );

    for shop in state.living.shops.values_mut() {
        shop.money = MAX_MONEY;
    }
    let blocked = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(
        matches!(blocked.action, Some(GuideAction::OpenMarket { shop_id }) if shop_id == nearest)
    );
    assert!(blocked.detail.contains("inventory or market cash capacity"));
    assert!(
        blocked
            .detail
            .contains("4 wheat at Full-quantity comparison, 4.80 cr")
    );
    assert!(matches!(
        state.personal_trade_quote(1, nearest, "wheat", 1, true, &catalog),
        Err(SandboxError::Capacity)
    ));
    state.living.accounts.get_mut(&1).unwrap().money = 1;
    let poor = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(poor.detail.contains("cannot afford one unit"));
    assert!(poor.detail.contains("Wallet: 0.01 cr; shortfall: 4.79 cr"));
}

#[test]
fn concise_supply_market_text_keeps_recipe_inputs_distinct_from_finished_goods() {
    let (mut state, catalog) = world();
    let origin = state.players[&1].position;
    let id = *state.settlements.contracts.keys().next().unwrap();
    request_item(&mut state, id, "planks", 5);
    let inventory = &mut state.players.get_mut(&1).unwrap().inventory;
    inventory.insert("planks".into(), 1);
    inventory.remove("wood");
    state.resource_nodes.clear();
    for shop in state.living.shops.values_mut() {
        shop.stock.remove("planks");
        shop.stock.remove("wood");
    }
    let ingredient_shop = *state.living.shops.keys().next().unwrap();
    let shop = state.living.shops.get_mut(&ingredient_shop).unwrap();
    // Equal display names must not collapse different items or seller IDs.
    shop.name = "Mixed exchange".into();
    shop.position = origin;
    shop.money = 0;
    shop.stock.insert("wood".into(), 20);
    shop.prices
        .insert("wood".into(), TradePrice { buy: 50, sell: 25 });
    let mut finished_shop = shop.clone();
    finished_shop.id = state.living.next_id;
    state.living.next_id += 1;
    finished_shop.position = LocalFrame::new(origin).to_geo([150., 0., 0.]).unwrap();
    finished_shop.stock.remove("wood");
    finished_shop.stock.insert("planks".into(), 4);
    finished_shop
        .prices
        .insert("planks".into(), TradePrice { buy: 100, sell: 50 });
    finished_shop.money = MAX_MONEY;
    state.living.shops.insert(finished_shop.id, finished_shop);
    accept(&mut state, &catalog, 1, id);

    let before = state.clone();
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert_eq!(state, before);
    assert!(
        matches!(guide.action, Some(GuideAction::OpenMarket { shop_id }) if shop_id == ingredient_shop)
    );
    assert!(
        guide
            .detail
            .contains("To make planks, first collect 2 wood")
    );
    assert!(guide.detail.contains("Mixed exchange's live wood quote"));
    assert!(guide.detail.contains("4 planks at Mixed exchange, 4.00 cr"));
    assert!(guide.detail.contains("Deliver all 5 planks to"));
    assert!(!guide.detail.contains("credits each"));
}

#[test]
fn expired_supply_offers_an_explicit_value_preserving_return() {
    let (mut state, catalog) = world();
    let id = *state.settlements.contracts.keys().next().unwrap();
    accept(&mut state, &catalog, 1, id);
    state.settlements.contracts.get_mut(&id).unwrap().expires_at =
        state.settlements.elapsed_seconds;
    let inventory = state.players[&1].inventory.clone();
    let wallet = state.living.accounts[&1].money;
    let reward = state.settlements.contracts[&id].reward;
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(guide.title.contains("deadline has passed"));
    let Some(GuideAction::Command(command)) = guide.action else {
        panic!("return action");
    };
    assert!(
        matches!(command, Command::Settlement { action: SettlementCommand::CancelContract { contract_id } } if contract_id == id)
    );
    state.apply(1, command, &catalog).unwrap();
    assert_eq!(state.players[&1].inventory, inventory);
    assert_eq!(state.living.accounts[&1].money, wallet);
    assert_eq!(state.settlements.contracts[&id].reward, reward);
    assert!(
        state
            .activity_commitments(1)
            .unwrap()
            .supply_requests
            .is_empty()
    );
}

/// Repeatable SDK continuity case: actual acceptance, clock expiry, explicit
/// return, physical delivery, exact save/resume and no duplicate payout. It is
/// not a measured human play session or prepared-terrain coverage claim.
#[test]
fn continuing_guidance_journey_preserves_obligations_and_rebuilds_after_resume() {
    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let mut state = SandboxState::default();
    state.add_player(1, origin).unwrap();
    state.seed_living_demo(origin, &[], &catalog).unwrap();
    state.advance_settlements(0., &catalog).unwrap();
    state.settlements.contract_duration_seconds = 86_400.;
    let deliver = state
        .settlements
        .contracts
        .values()
        .find(|c| c.item == "wood")
        .unwrap()
        .id;
    let other: Vec<_> = state
        .settlements
        .contracts
        .keys()
        .copied()
        .filter(|id| *id != deliver)
        .take(2)
        .collect();
    let missed = other[1];
    let returned = other[0];
    let starting_wallet = state.living.accounts[&1].money;
    accept(&mut state, &catalog, 1, missed);
    state.advance_settlements(60., &catalog).unwrap();
    accept(&mut state, &catalog, 1, returned);
    state.advance_settlements(60., &catalog).unwrap();
    accept(&mut state, &catalog, 1, deliver);
    let briefing = state.activity_commitments(1).unwrap();
    assert_eq!(
        briefing
            .supply_requests
            .iter()
            .map(|c| c.id)
            .collect::<Vec<_>>(),
        vec![missed, returned, deliver]
    );
    let shown = state.activity_guidance(1, &catalog, GuideTrack::Supply, None);
    assert_eq!(shown.supply_contract.as_ref().unwrap().id, missed);
    let saved = serde_json::to_vec(&state).unwrap();
    let mut resumed: SandboxState = serde_json::from_slice(&saved).unwrap();
    resumed.validate_with_catalog(&catalog).unwrap();
    assert_eq!(resumed, state);
    assert_eq!(resumed.activity_commitments(1).unwrap(), briefing);
    assert!(shown.same_intent(&resumed.activity_guidance(1, &catalog, GuideTrack::Supply, None)));

    let inventory_before_return = resumed.players[&1].inventory.clone();
    let return_command = Command::Settlement {
        action: SettlementCommand::CancelContract {
            contract_id: returned,
        },
    };
    resumed.apply(1, return_command.clone(), &catalog).unwrap();
    assert_eq!(resumed.players[&1].inventory, inventory_before_return);
    assert_eq!(resumed.living.accounts[&1].money, starting_wallet);
    let after_return = resumed.clone();
    assert!(resumed.apply(1, return_command, &catalog).is_err());
    assert_eq!(resumed, after_return);

    resumed.advance_settlements(86_281., &catalog).unwrap();
    assert_eq!(resumed.settlements.contracts[&missed].claimed_by, None);
    assert_eq!(resumed.settlements.contracts[&deliver].claimed_by, Some(1));
    let current = resumed.activity_guidance(1, &catalog, GuideTrack::Supply, None);
    assert_eq!(current.supply_contract.as_ref().unwrap().id, deliver);
    assert!(!shown.same_intent(&current));
    assert_eq!(
        resumed
            .activity_commitments(1)
            .unwrap()
            .supply_requests
            .len(),
        1
    );
    assert_eq!(resumed.players[&1].inventory, inventory_before_return);
    assert_eq!(resumed.living.accounts[&1].money, starting_wallet);

    let contract = resumed.settlements.contracts[&deliver].clone();
    assert!(resumed.players[&1].inventory[&contract.item] >= contract.amount);
    let destination = LocalFrame::new(contract.target_position)
        .to_geo([0., 0., 16.])
        .unwrap();
    let actor = resumed.living.accounts[&1].active_unit;
    resumed
        .apply(
            1,
            Command::Living {
                action: LivingCommand::MoveUnits {
                    unit_ids: vec![actor],
                    destination,
                },
            },
            &catalog,
        )
        .unwrap();
    let mut walked_seconds = 0.;
    let mut travel_m = 0.;
    let mut max_step_m: f64 = 0.;
    for _ in 0..2400 {
        let position = resumed.living.units[&actor].position;
        if position
            .surface_distance_m(contract.target_position)
            .hypot(position.altitude_m - contract.target_position.altitude_m)
            <= 19.
        {
            break;
        }
        resumed
            .advance_living_with_environment(0.1, 0., &catalog, |_, _, candidate| Some(candidate))
            .unwrap();
        let step = position.surface_distance_m(resumed.living.units[&actor].position);
        max_step_m = max_step_m.max(step);
        travel_m += step;
        walked_seconds += 0.1;
    }
    let before_deliver = resumed.clone();
    let command = Command::Settlement {
        action: SettlementCommand::FulfillContract {
            contract_id: deliver,
        },
    };
    resumed.apply(1, command.clone(), &catalog).unwrap();
    assert_eq!(
        resumed.living.accounts[&1].money,
        starting_wallet + contract.reward
    );
    assert_eq!(
        resumed.players[&1].inventory[&contract.item],
        before_deliver.players[&1].inventory[&contract.item] - contract.amount
    );
    assert_eq!(resumed.settlements.progress[&1].contracts_completed, 1);
    assert_eq!(
        resumed.settlements.progress[&1].experience,
        contract.experience
    );
    let completed = resumed.clone();
    assert!(resumed.apply(1, command, &catalog).is_err());
    assert_eq!(resumed, completed);
    assert!(
        resumed
            .activity_commitments(1)
            .unwrap()
            .supply_requests
            .is_empty()
    );
    resumed.validate_with_catalog(&catalog).unwrap();
    let final_saved = serde_json::to_vec(&resumed).unwrap();
    let final_resumed: SandboxState = serde_json::from_slice(&final_saved).unwrap();
    assert_eq!(final_resumed, resumed);
    assert_eq!(
        final_resumed.activity_commitments(1),
        resumed.activity_commitments(1)
    );
    println!(
        "GUIDANCE_CONTINUITY_REPORT {}",
        serde_json::json!({
            "case": "three_obligations_return_expiry_physical_delivery_resume",
            "accepted_contracts": 3,
            "explicit_returns": 1,
            "missed_deadlines": 1,
            "paid_deliveries": 1,
            "calendar_seconds": resumed.settlements.elapsed_seconds,
            "physical_walk_seconds": walked_seconds,
            "walk_distance_m": travel_m,
            "maximum_walk_step_m": max_step_m,
            "starting_personal_wallet_minor": starting_wallet,
            "final_personal_wallet_minor": resumed.living.accounts[&1].money,
            "gross_reward_minor": contract.reward,
            "delivered_item": contract.item,
            "delivered_quantity": contract.amount,
            "community_xp": contract.experience,
            "remaining_accepted_supply": 0,
        "scope": "Authored flat SDK support; a 24-world-hour fixture deadline with calendar and physical movement advanced separately. Uses ordinary starting inventory, paid escrow and commands. Not prepared-terrain, human-comprehension, career-profitability or hardware evidence.",
            "verification": {
                "earliest_deadline_order": true,
                "exact_open_obligations_after_resume": true,
                "return_preserves_goods_and_wallet": true,
                "duplicate_return_rejected_without_mutation": true,
                "expired_request_returns_to_board": true,
                "stale_obligation_requires_review": true,
                "remaining_obligation_preserved": true,
                "physical_delivery_without_position_override": true,
                "exact_inventory_and_reserved_payment": true,
                "duplicate_payment_rejected_without_mutation": true,
                "exact_completed_save_resume": true,
                "complete_catalog_validation": true
            }
        })
    );
}
