use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

fn world() -> (SandboxState, Catalog) {
    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let mut state = SandboxState::default();
    state.add_player(1, origin).unwrap();
    state.add_player(2, origin).unwrap();
    state.seed_living_demo(origin, &[], &catalog).unwrap();
    (state, catalog)
}

/// A finite authored request: reserve existing issuer cash just as posting
/// does. The fixture does not add money or reset any accepted request.
fn fund_request(state: &mut SandboxState, target: u64, item: &str, amount: u32) -> u64 {
    // The authored input request must represent actual receiver demand before
    // acceptance. This does not transfer goods or change accepted terms.
    if let Some(site) = state.living.producers.get_mut(&target) {
        let current = site.stock.get(item).copied().unwrap_or(0);
        site.inputs
            .insert(item.into(), (current + amount).div_ceil(12));
    }
    let issuer = *state.living.shops.keys().next().unwrap();
    let position = state
        .living
        .producers
        .get(&target)
        .map(|site| site.position)
        .or_else(|| state.living.shops.get(&target).map(|shop| shop.position))
        .unwrap();
    let id = state.settlements.next_id;
    state.settlements.next_id += 1;
    state.living.shops.get_mut(&issuer).unwrap().money -= 500;
    state.settlements.contracts.insert(
        id,
        SupplyContract {
            id,
            issuer_shop: issuer,
            target_site: target,
            target_position: position,
            item: item.into(),
            amount,
            reward: 500,
            experience: 40,
            claimed_by: None,
            expires_at: state.settlements.contract_duration_seconds,
        },
    );
    id
}

fn command(action: SupplyContractAction, id: u64) -> Command {
    Command::Settlement {
        action: match action {
            SupplyContractAction::Accept => SettlementCommand::AcceptContract { contract_id: id },
            SupplyContractAction::Deliver => SettlementCommand::FulfillContract { contract_id: id },
            SupplyContractAction::Return => SettlementCommand::CancelContract { contract_id: id },
        },
    }
}

fn ready_world() -> (SandboxState, Catalog, u64) {
    let (mut state, catalog) = world();
    let target = *state.living.producers.keys().next().unwrap();
    let id = fund_request(&mut state, target, "wood", 5);
    state
        .apply(1, command(SupplyContractAction::Accept, id), &catalog)
        .unwrap();
    let destination = state.settlements.contracts[&id].target_position;
    state.set_player_position(1, destination).unwrap();
    state.validate_with_catalog(&catalog).unwrap();
    assert_eq!(
        state.supply_contract_readiness(1, id, SupplyContractAction::Deliver, &catalog),
        Ok(())
    );
    (state, catalog, id)
}

fn total_money(state: &SandboxState) -> u64 {
    state.living.accounts.values().map(|a| a.money).sum::<u64>()
        + state.living.shops.values().map(|s| s.money).sum::<u64>()
        + state
            .living
            .units
            .values()
            .map(|u| u.pocket_money)
            .sum::<u64>()
        + state
            .settlements
            .contracts
            .values()
            .map(|c| c.reward)
            .sum::<u64>()
        + state
            .settlements
            .territories
            .values()
            .map(|t| t.treasury)
            .sum::<u64>()
}

fn total_wood(state: &SandboxState) -> u64 {
    let amount = |stock: &Inventory| u64::from(stock.get("wood").copied().unwrap_or(0));
    state
        .players
        .values()
        .map(|p| amount(&p.inventory))
        .sum::<u64>()
        + state
            .living
            .shops
            .values()
            .map(|s| amount(&s.stock))
            .sum::<u64>()
        + state
            .living
            .producers
            .values()
            .map(|s| amount(&s.stock))
            .sum::<u64>()
        + state
            .living
            .units
            .values()
            .map(|u| amount(&u.supplies))
            .sum::<u64>()
        + state
            .resource_nodes
            .values()
            .filter(|r| r.item == "wood")
            .map(|r| u64::from(r.remaining))
            .sum::<u64>()
}

fn assert_blocked(
    state: &mut SandboxState,
    catalog: &Catalog,
    player: u64,
    id: u64,
    action: SupplyContractAction,
    expected: SupplyContractBlocker,
) {
    let before = state.clone();
    assert_eq!(
        state.supply_contract_readiness(player, id, action, catalog),
        Err(expected)
    );
    assert_eq!(*state, before, "inspection mutated authority");
    assert!(state.apply(player, command(action, id), catalog).is_err());
    assert_eq!(
        *state, before,
        "rejection moved goods, escrow or another field"
    );
}

#[test]
fn readiness_is_read_only_and_actual_success_transfers_exact_funded_value_once() {
    let (mut state, catalog, id) = ready_world();
    let contract = state.settlements.contracts[&id].clone();
    let before = state.clone();
    assert_eq!(
        state.supply_contract_readiness(1, id, SupplyContractAction::Deliver, &catalog),
        Ok(())
    );
    assert_eq!(
        state.supply_contract_readiness(1, id, SupplyContractAction::Return, &catalog),
        Ok(())
    );
    assert_eq!(
        state.supply_contract_readiness(1, id, SupplyContractAction::Accept, &catalog),
        Err(SupplyContractBlocker::AlreadyAccepted)
    );
    assert_eq!(state, before);
    let money = total_money(&state);
    let wood = total_wood(&state);
    state
        .apply(1, command(SupplyContractAction::Deliver, id), &catalog)
        .unwrap();
    assert_eq!(total_money(&state), money);
    assert_eq!(total_wood(&state), wood);
    assert_eq!(
        state.living.accounts[&1].money,
        before.living.accounts[&1].money + contract.reward
    );
    assert_eq!(state.settlements.progress[&1].contracts_completed, 1);
    assert_eq!(
        state.settlements.progress[&1].experience,
        u64::from(contract.experience)
    );
    assert_blocked(
        &mut state,
        &catalog,
        1,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::ContractMissing,
    );
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn dormant_guidance_never_advertises_accept_acquire_deliver_or_return() {
    let (mut state, catalog, id) = ready_world();
    state.set_player_dormant(1, true).unwrap();
    for context in 0..4 {
        match context {
            1 => {
                state.players.get_mut(&1).unwrap().inventory.remove("wood");
            }
            2 => {
                state.settlements.elapsed_seconds = state.settlements.contracts[&id].expires_at;
            }
            3 => {
                state.settlements.contracts.get_mut(&id).unwrap().claimed_by = None;
            }
            _ => {}
        }
        let before = state.clone();
        for track in [
            GuideTrack::Suggested,
            GuideTrack::Supply,
            GuideTrack::Home,
            GuideTrack::Community,
            GuideTrack::Road,
            GuideTrack::Company,
        ] {
            let guide = state.activity_guidance(1, &catalog, track, Some(id));
            assert!(guide.action.is_none());
            assert!(guide.detail.contains("Resume this owner"));
        }
        assert_eq!(state, before);
    }
}

#[test]
fn delivery_reports_identity_expiry_goods_reach_and_capacity_without_mutation() {
    let (base, catalog, id) = ready_world();
    let mut state = base.clone();
    assert_blocked(
        &mut state,
        &catalog,
        999,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::PlayerMissing,
    );
    let mut state = base.clone();
    state.set_player_dormant(1, true).unwrap();
    assert_blocked(
        &mut state,
        &catalog,
        1,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::PlayerDormant,
    );
    let mut state = base.clone();
    state.living.enabled = false;
    assert_blocked(
        &mut state,
        &catalog,
        1,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::AccountUnavailable,
    );
    let mut state = base.clone();
    assert_blocked(
        &mut state,
        &catalog,
        2,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::NotAccepted,
    );
    let mut state = base.clone();
    state.settlements.elapsed_seconds = state.settlements.contracts[&id].expires_at;
    assert_blocked(
        &mut state,
        &catalog,
        1,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::Expired,
    );
    assert_eq!(
        state.supply_contract_readiness(1, id, SupplyContractAction::Return, &catalog),
        Ok(())
    );
    let mut state = base.clone();
    state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("wood".into(), 4);
    assert_blocked(
        &mut state,
        &catalog,
        1,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::MissingGoods {
            item: "wood".into(),
            needed: 5,
            carried: 4,
        },
    );
    let mut state = base.clone();
    let far = LocalFrame::new(state.players[&1].position)
        .to_geo([100., 0., 0.])
        .unwrap();
    state.set_player_position(1, far).unwrap();
    assert_blocked(
        &mut state,
        &catalog,
        1,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::OutOfReach,
    );
    let mut state = base.clone();
    state.living.accounts.get_mut(&1).unwrap().money = MAX_MONEY;
    assert_blocked(
        &mut state,
        &catalog,
        1,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::WalletCapacity,
    );
    let mut state = base.clone();
    let target = state.settlements.contracts[&id].target_site;
    state.living.producers.get_mut(&target).unwrap().stock =
        Inventory::from([("wood".into(), 2000)]);
    state.validate_with_catalog(&catalog).unwrap();
    assert_blocked(
        &mut state,
        &catalog,
        1,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::DestinationCapacity,
    );
    let mut state = base.clone();
    state.living.producers.remove(&target);
    assert_blocked(
        &mut state,
        &catalog,
        1,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::DestinationUnavailable,
    );
    let mut state = base.clone();
    state
        .settlements
        .contracts
        .get_mut(&id)
        .unwrap()
        .target_position = far;
    assert_blocked(
        &mut state,
        &catalog,
        1,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::InvalidState,
    );
    let mut state = base;
    let mut absent_catalog = catalog;
    absent_catalog.items.remove("wood");
    assert_blocked(
        &mut state,
        &absent_catalog,
        1,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::CatalogItemUnavailable,
    );
}

#[test]
fn shared_acceptance_and_return_checks_preserve_limits_and_expired_offer_escrow() {
    let (mut state, catalog) = world();
    state.advance_settlements(0., &catalog).unwrap();
    let ids: Vec<_> = state.settlements.contracts.keys().copied().collect();
    assert_eq!(ids.len(), 4);
    let money = total_money(&state);
    for id in &ids[..3] {
        assert_eq!(
            state.supply_contract_readiness(1, *id, SupplyContractAction::Accept, &catalog),
            Ok(())
        );
        state
            .apply(1, command(SupplyContractAction::Accept, *id), &catalog)
            .unwrap();
    }
    assert_blocked(
        &mut state,
        &catalog,
        1,
        ids[3],
        SupplyContractAction::Accept,
        SupplyContractBlocker::ActiveLimit,
    );
    assert_blocked(
        &mut state,
        &catalog,
        2,
        ids[0],
        SupplyContractAction::Return,
        SupplyContractBlocker::NotAccepted,
    );
    state
        .apply(1, command(SupplyContractAction::Return, ids[0]), &catalog)
        .unwrap();
    assert_eq!(total_money(&state), money);
    state.settlements.elapsed_seconds = state.settlements.contracts[&ids[0]].expires_at;
    assert_blocked(
        &mut state,
        &catalog,
        1,
        ids[0],
        SupplyContractAction::Accept,
        SupplyContractBlocker::Expired,
    );
    assert_eq!(total_money(&state), money);
}

#[test]
fn expired_public_offers_are_never_advertised_as_accept_until_real_maintenance() {
    let (mut state, catalog) = world();
    state.advance_settlements(0., &catalog).unwrap();
    let id = *state.settlements.contracts.keys().next().unwrap();
    let deadline = state.settlements.contracts[&id].expires_at;
    state.settlements.elapsed_seconds = deadline - 0.1;
    let shown = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(
        matches!(shown.action, Some(GuideAction::Command(Command::Settlement { action: SettlementCommand::AcceptContract { contract_id } })) if contract_id == id)
    );
    state.settlements.elapsed_seconds = deadline;
    let before = state.clone();
    let expired = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert_eq!(state, before);
    assert!(
        expired
            .detail
            .contains("Expired public offers cannot be accepted")
    );
    assert!(!matches!(
        expired.action,
        Some(GuideAction::Command(Command::Settlement {
            action: SettlementCommand::AcceptContract { .. }
        }))
    ));
    assert!(!shown.same_intent(&expired));
    let money = total_money(&state);
    state.advance_settlements(60., &catalog).unwrap();
    assert_eq!(total_money(&state), money);
    let refreshed = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(
        !shown.same_intent(&refreshed),
        "a renewed deadline needs review"
    );
    assert!(
        matches!(refreshed.action, Some(GuideAction::Command(Command::Settlement { action: SettlementCommand::AcceptContract { contract_id } })) if contract_id == id)
    );
}

#[test]
fn recipient_view_absence_is_explicit_and_does_not_claim_whole_world_failure() {
    let (mut authority, catalog, id) = ready_world();
    let destination = authority.settlements.contracts[&id].target_position;
    let far = LocalFrame::new(destination)
        .to_geo([1000., 0., 0.])
        .unwrap();
    authority.set_player_position(1, far).unwrap();
    let view = authority.snapshot_for(1, 1.).unwrap();
    assert!(view.living.producers.is_empty());
    let mut replica = SandboxState::default();
    replica.players.insert(1, view.player);
    replica.living = LivingWorld::from_view(view.living);
    replica.settlements.contracts = view
        .settlements
        .contracts
        .into_iter()
        .map(|c| (c.id, c))
        .collect();
    replica.settlements.elapsed_seconds = view.settlements.elapsed_seconds;
    let before = replica.clone();
    let blocker = replica
        .supply_contract_readiness(1, id, SupplyContractAction::Deliver, &catalog)
        .unwrap_err();
    assert_eq!(blocker, SupplyContractBlocker::DestinationUnavailable);
    assert!(blocker.to_string().contains("current view"));
    let guide = replica.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(guide.detail.contains("outside the current view"));
    assert!(
        matches!(guide.action, Some(GuideAction::Travel { position, .. }) if position == destination)
    );
    assert_eq!(replica, before);
    authority.set_player_position(1, destination).unwrap();
    assert_eq!(
        authority.supply_contract_readiness(1, id, SupplyContractAction::Deliver, &catalog),
        Ok(())
    );
}

#[test]
fn two_inspections_cannot_reserve_the_same_final_producer_capacity() {
    let (mut state, catalog) = world();
    let target = *state.living.producers.keys().next().unwrap();
    state.living.producers.get_mut(&target).unwrap().stock =
        Inventory::from([("wood".into(), 1000), ("stone".into(), 995)]);
    let wood = fund_request(&mut state, target, "wood", 5);
    let stone = fund_request(&mut state, target, "stone", 5);
    let destination = state.living.producers[&target].position;
    for (player, id) in [(1, wood), (2, stone)] {
        state.set_player_position(player, destination).unwrap();
        state
            .apply(player, command(SupplyContractAction::Accept, id), &catalog)
            .unwrap();
        assert_eq!(
            state.supply_contract_readiness(player, id, SupplyContractAction::Deliver, &catalog),
            Ok(())
        );
    }
    state.validate_with_catalog(&catalog).unwrap();
    let money = total_money(&state);
    state
        .apply(1, command(SupplyContractAction::Deliver, wood), &catalog)
        .unwrap();
    assert_blocked(
        &mut state,
        &catalog,
        2,
        stone,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::DestinationCapacity,
    );
    assert_eq!(
        state.living.producers[&target]
            .stock
            .values()
            .copied()
            .sum::<u32>(),
        2000
    );
    assert_eq!(total_money(&state), money);
    assert_eq!(state.settlements.contracts[&stone].claimed_by, Some(2));
    state.validate_with_catalog(&catalog).unwrap();
}

struct SavedWorld(PathBuf);
impl SavedWorld {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        Self(std::env::temp_dir().join(format!(
            "terra-supply-readiness-{}-{}.json",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for SavedWorld {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn finite_supply_journey_gathers_recovers_full_receiver_after_disk_restore_and_pays_once() {
    let (mut state, catalog) = world();
    let target = *state.living.shops.keys().next().unwrap();
    let destination = state.living.shops[&target].position;
    state
        .living
        .shops
        .get_mut(&target)
        .unwrap()
        .stock
        .insert("wood".into(), 35);
    let id = fund_request(&mut state, target, "wood", 5);
    state.players.get_mut(&1).unwrap().inventory.remove("wood");
    // Accept while retail demand exists. A subsequent arrival fills the
    // receiving stack before gathering; the accepted promise must survive.
    let accept = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert_eq!(accept.step, 1);
    let Some(GuideAction::Command(command)) = accept.action else {
        panic!("journey should offer an executable acceptance");
    };
    state.apply(1, command, &catalog).unwrap();
    state
        .living
        .shops
        .get_mut(&target)
        .unwrap()
        .stock
        .insert("wood".into(), MAX_ITEM_COUNT);
    state.resource_nodes.insert(
        "finite-supply-wood".into(),
        ResourceNode {
            id: "finite-supply-wood".into(),
            position: destination,
            item: "wood".into(),
            remaining: 5,
            capacity: 5,
            regrow_seconds: 0.,
            regrow_accumulator: 0.,
            source_feature: None,
        },
    );
    state.set_player_position(1, destination).unwrap();
    state.validate_with_catalog(&catalog).unwrap();
    let money = total_money(&state);
    let wood = total_wood(&state);
    let wallet = state.living.accounts[&1].money;
    let reward = state.settlements.contracts[&id].reward;
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert_eq!(guide.step, 2);
    let Some(GuideAction::Command(command)) = guide.action else {
        panic!("journey should offer an executable gathering command");
    };
    state.apply(1, command, &catalog).unwrap();
    assert_eq!(state.resource_nodes["finite-supply-wood"].remaining, 0);
    assert_eq!(total_money(&state), money);
    assert_eq!(total_wood(&state), wood);
    let blocked = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert_eq!(blocked.title, "Delivery needs attention");
    assert!(blocked.detail.contains("receiving service cannot fit"));
    assert!(
        matches!(blocked.action, Some(GuideAction::Command(Command::Settlement { action: SettlementCommand::CancelContract { contract_id } })) if contract_id == id)
    );
    assert_blocked(
        &mut state,
        &catalog,
        1,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::DestinationCapacity,
    );
    let path = SavedWorld::new();
    state.save(&path.0).unwrap();
    let mut resumed = SandboxState::load_with_catalog(&path.0, &catalog).unwrap();
    assert_eq!(state, resumed);
    // An ordinary paid purchase creates space; no stock is deleted, refilled
    // or teleported and the funded request remains accepted throughout.
    let quote = resumed
        .personal_trade_quote(1, target, "wood", 5, true, &catalog)
        .unwrap();
    resumed
        .apply(
            1,
            Command::Living {
                action: LivingCommand::TradeQuoted {
                    shop_id: target,
                    item: "wood".into(),
                    amount: 5,
                    buy: true,
                    quoted_total: quote.total_cost,
                },
            },
            &catalog,
        )
        .unwrap();
    assert_eq!(resumed.settlements.contracts[&id].claimed_by, Some(1));
    let ready = resumed.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert_eq!(ready.action_label, "Deliver and collect reward");
    assert!(!blocked.same_intent(&ready));
    let Some(GuideAction::Command(delivery)) = ready.action else {
        unreachable!()
    };
    resumed.apply(1, delivery, &catalog).unwrap();
    assert_eq!(
        resumed.living.accounts[&1].money,
        wallet - quote.total_cost + reward
    );
    assert_eq!(resumed.players[&1].inventory["wood"], 5);
    assert_eq!(total_money(&resumed), money);
    assert_eq!(total_wood(&resumed), wood);
    assert_blocked(
        &mut resumed,
        &catalog,
        1,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::ContractMissing,
    );
    resumed.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn wallet_capacity_recovers_through_real_spending_without_erasing_or_minting_money() {
    let (mut state, catalog, id) = ready_world();
    state.living.accounts.get_mut(&1).unwrap().money = MAX_MONEY;
    let money = total_money(&state);
    assert_blocked(
        &mut state,
        &catalog,
        1,
        id,
        SupplyContractAction::Deliver,
        SupplyContractBlocker::WalletCapacity,
    );
    let target = *state.living.shops.keys().next().unwrap();
    state
        .set_player_position(1, state.living.shops[&target].position)
        .unwrap();
    let quote = state
        .personal_trade_quote(1, target, "bread", 10, true, &catalog)
        .unwrap();
    assert!(quote.total_cost >= state.settlements.contracts[&id].reward);
    state
        .apply(
            1,
            Command::Living {
                action: LivingCommand::TradeQuoted {
                    shop_id: target,
                    item: "bread".into(),
                    amount: 10,
                    buy: true,
                    quoted_total: quote.total_cost,
                },
            },
            &catalog,
        )
        .unwrap();
    state
        .set_player_position(1, state.settlements.contracts[&id].target_position)
        .unwrap();
    assert_eq!(
        state.supply_contract_readiness(1, id, SupplyContractAction::Deliver, &catalog),
        Ok(())
    );
    state
        .apply(1, command(SupplyContractAction::Deliver, id), &catalog)
        .unwrap();
    assert_eq!(total_money(&state), money);
    state.validate_with_catalog(&catalog).unwrap();
}
