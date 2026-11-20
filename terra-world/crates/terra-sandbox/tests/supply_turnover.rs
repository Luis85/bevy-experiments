use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

fn world() -> (SandboxState, Catalog, u64, u64) {
    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let mut state = SandboxState::default();
    for player in 1..=3 {
        state.add_player(player, origin).unwrap();
    }
    state.seed_living_demo(origin, &[], &catalog).unwrap();
    for shop in state.living.shops.values_mut() {
        // Isolate demand from the authored exchange's automatic assortment.
        shop.name = "Test exchange".into();
        shop.prices = catalog
            .items
            .iter()
            .map(|item| (item.clone(), TradePrice { buy: 100, sell: 50 }))
            .collect();
        shop.stock = catalog
            .items
            .iter()
            .map(|item| (item.clone(), 40))
            .collect();
    }
    for site in state.living.producers.values_mut() {
        site.enabled = false;
    }
    let issuer = *state.living.shops.keys().next().unwrap();
    let producer = *state.living.producers.keys().next().unwrap();
    state.validate_with_catalog(&catalog).unwrap();
    (state, catalog, issuer, producer)
}

fn production(state: &mut SandboxState, id: u64, wood: u32, other: u32) {
    let site = state.living.producers.get_mut(&id).unwrap();
    site.enabled = true;
    site.inputs = BTreeMap::from([("wood".into(), 1)]);
    site.outputs = BTreeMap::from([("planks".into(), 1)]);
    site.stock.clear();
    if wood > 0 {
        site.stock.insert("wood".into(), wood);
    }
    if other > 0 {
        site.stock.insert("stone".into(), other);
    }
}

fn fund(
    state: &mut SandboxState,
    issuer: u64,
    target: u64,
    item: &str,
    amount: u32,
    reward: u64,
) -> u64 {
    let id = state.settlements.next_id;
    state.settlements.next_id += 1;
    let position = state
        .living
        .producers
        .get(&target)
        .map(|site| site.position)
        .or_else(|| state.living.shops.get(&target).map(|shop| shop.position))
        .unwrap();
    state.living.shops.get_mut(&issuer).unwrap().money -= reward;
    state.settlements.contracts.insert(
        id,
        SupplyContract {
            id,
            issuer_shop: issuer,
            target_site: target,
            target_position: position,
            item: item.into(),
            amount,
            reward,
            experience: 40,
            claimed_by: None,
            expires_at: state.settlements.elapsed_seconds
                + state.settlements.contract_duration_seconds,
        },
    );
    id
}

fn action(
    state: &mut SandboxState,
    catalog: &Catalog,
    player: u64,
    action: SettlementCommand,
) -> Result<Outcome, SandboxError> {
    state.apply(player, Command::Settlement { action }, catalog)
}

fn money(state: &SandboxState) -> u64 {
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

fn goods(state: &SandboxState) -> BTreeMap<String, u64> {
    let mut totals = BTreeMap::new();
    for inventory in state
        .players
        .values()
        .map(|p| &p.inventory)
        .chain(state.living.shops.values().map(|s| &s.stock))
        .chain(state.living.producers.values().map(|s| &s.stock))
        .chain(state.living.units.values().map(|u| &u.supplies))
    {
        for (item, amount) in inventory {
            *totals.entry(item.clone()).or_default() += u64::from(*amount);
        }
    }
    totals
}

fn trade(
    state: &mut SandboxState,
    catalog: &Catalog,
    shop: u64,
    item: &str,
    n: u32,
    buy: bool,
) -> u64 {
    state
        .set_player_position(1, state.living.shops[&shop].position)
        .unwrap();
    let quote = state
        .personal_trade_quote(1, shop, item, n, buy, catalog)
        .unwrap();
    state
        .apply(
            1,
            Command::Living {
                action: LivingCommand::TradeQuoted {
                    shop_id: shop,
                    item: item.into(),
                    amount: n,
                    buy,
                    quoted_total: quote.total_cost,
                },
            },
            catalog,
        )
        .unwrap();
    quote.total_cost
}

struct Save(PathBuf);
impl Save {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "terra-supply-turnover-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(!path.exists());
        Self(path)
    }
}
impl Drop for Save {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn posting_uses_exact_one_item_shortages_in_production_and_retail() {
    for production_request in [false, true] {
        let (mut state, catalog, issuer, producer) = world();
        let target = if production_request {
            production(&mut state, producer, 11, 0);
            producer
        } else {
            state
                .living
                .shops
                .get_mut(&issuer)
                .unwrap()
                .stock
                .insert("wood".into(), 39);
            issuer
        };
        let before_money = money(&state);
        let before_goods = goods(&state);
        state.advance_settlements(0., &catalog).unwrap();
        assert_eq!(state.settlements.contracts.len(), 1);
        let request = state.settlements.contracts.values().next().unwrap();
        assert_eq!(
            (request.target_site, request.item.as_str(), request.amount),
            (target, "wood", 1)
        );
        let price = state.living.shops[&issuer].prices["wood"];
        let expected = if production_request {
            price.buy * 115 / 100
                + (state.living.shops[&issuer]
                    .position
                    .surface_distance_m(request.target_position)
                    / 2.)
                    .clamp(0., 1000.) as u64
        } else {
            price.sell * 110 / 100
        };
        assert_eq!(request.reward, expected);
        assert_eq!(money(&state), before_money);
        assert_eq!(goods(&state), before_goods);
        state.validate_with_catalog(&catalog).unwrap();
    }
}

#[test]
fn posting_fits_the_entire_producer_inventory_or_reserves_no_money() {
    for available in [0, 1, 3, 12, 50] {
        let (mut state, catalog, issuer, producer) = world();
        production(&mut state, producer, 0, 2000 - available);
        let cash = state.living.shops[&issuer].money;
        let next = state.settlements.next_id;
        let total = money(&state);
        state.advance_settlements(0., &catalog).unwrap();
        if available == 0 {
            assert!(state.settlements.contracts.is_empty());
            assert_eq!(state.living.shops[&issuer].money, cash);
            assert_eq!(state.settlements.next_id, next);
        } else {
            let request = &state.settlements.contracts[&next];
            assert_eq!(request.amount, available.min(12));
            assert_eq!(request.target_site, producer);
        }
        assert_eq!(money(&state), total);
        state.validate_with_catalog(&catalog).unwrap();
    }
}

#[test]
fn obsolete_public_offers_block_readiness_and_real_acceptance_without_mutation() {
    for reason in 0..4 {
        let (mut state, catalog, issuer, producer) = world();
        production(&mut state, producer, 0, 0);
        let target = if reason == 3 { issuer } else { producer };
        let id = fund(&mut state, issuer, target, "wood", 4, 100);
        match reason {
            0 => {
                state
                    .living
                    .producers
                    .get_mut(&producer)
                    .unwrap()
                    .stock
                    .insert("wood".into(), 12);
            }
            1 => {
                state.living.producers.get_mut(&producer).unwrap().enabled = false;
            }
            2 => {
                state
                    .living
                    .producers
                    .get_mut(&producer)
                    .unwrap()
                    .inputs
                    .clear();
            }
            _ => {
                state
                    .living
                    .shops
                    .get_mut(&issuer)
                    .unwrap()
                    .prices
                    .remove("wood");
            }
        }
        let before = state.clone();
        assert_eq!(
            state.supply_contract_readiness(1, id, SupplyContractAction::Accept, &catalog),
            Err(SupplyContractBlocker::ObsoleteOffer)
        );
        assert_eq!(state, before);
        assert!(
            matches!(action(&mut state, &catalog, 1, SettlementCommand::AcceptContract { contract_id: id }), Err(SandboxError::Invalid(message)) if message.contains("no longer needed"))
        );
        assert_eq!(state, before);
        let guide = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
        assert!(!matches!(
            guide.action,
            Some(GuideAction::Command(Command::Settlement {
                action: SettlementCommand::AcceptContract { .. }
            }))
        ));
        assert_eq!(state, before);
        state.validate_with_catalog(&catalog).unwrap();
    }
}

#[test]
fn accepted_promises_survive_changed_demand_and_deliver_original_goods_and_reward() {
    for reason in 0..3 {
        let (mut state, catalog, issuer, producer) = world();
        production(&mut state, producer, 0, 0);
        let id = fund(&mut state, issuer, producer, "wood", 4, 100);
        action(
            &mut state,
            &catalog,
            1,
            SettlementCommand::AcceptContract { contract_id: id },
        )
        .unwrap();
        let accepted = state.settlements.contracts[&id].clone();
        match reason {
            0 => {
                state
                    .living
                    .producers
                    .get_mut(&producer)
                    .unwrap()
                    .stock
                    .insert("wood".into(), 12);
            }
            1 => {
                state.living.producers.get_mut(&producer).unwrap().enabled = false;
            }
            _ => {
                state
                    .living
                    .producers
                    .get_mut(&producer)
                    .unwrap()
                    .inputs
                    .clear();
            }
        }
        let total = money(&state);
        let stock = goods(&state);
        let wallet = state.living.accounts[&1].money;
        state.advance_settlements(60., &catalog).unwrap();
        assert_eq!(state.settlements.contracts[&id], accepted);
        state
            .set_player_position(1, accepted.target_position)
            .unwrap();
        assert_eq!(
            state.supply_contract_readiness(1, id, SupplyContractAction::Deliver, &catalog),
            Ok(())
        );
        action(
            &mut state,
            &catalog,
            1,
            SettlementCommand::FulfillContract { contract_id: id },
        )
        .unwrap();
        assert_eq!(state.living.accounts[&1].money, wallet + accepted.reward);
        assert_eq!(state.settlements.progress[&1].contracts_completed, 1);
        assert_eq!(money(&state), total);
        assert_eq!(goods(&state), stock);
        state.validate_with_catalog(&catalog).unwrap();
    }
}

#[test]
fn retirement_refunds_the_original_issuer_exactly_once_without_reusing_the_id() {
    let (mut state, catalog, central, _) = world();
    let issuer = state.living.next_id;
    state.living.next_id += 1;
    let mut secondary = state.living.shops[&central].clone();
    secondary.id = issuer;
    secondary.name = "Second exchange".into();
    state.living.shops.insert(issuer, secondary);
    let id = fund(&mut state, issuer, central, "wood", 4, 123);
    let next = state.settlements.next_id;
    let first_cash = state.living.shops[&central].money;
    let second_cash = state.living.shops[&issuer].money;
    let total = money(&state);
    let stock = goods(&state);
    state.advance_settlements(0., &catalog).unwrap();
    assert!(!state.settlements.contracts.contains_key(&id));
    assert_eq!(state.living.shops[&central].money, first_cash);
    assert_eq!(state.living.shops[&issuer].money, second_cash + 123);
    assert_eq!(state.settlements.next_id, next);
    for _ in 0..5 {
        state.advance_settlements(60., &catalog).unwrap();
    }
    assert_eq!(state.living.shops[&issuer].money, second_cash + 123);
    assert_eq!(money(&state), total);
    assert_eq!(goods(&state), stock);
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn cap_blocked_refund_survives_disk_and_recovers_after_actual_issuer_spending() {
    let (mut state, catalog, issuer, _) = world();
    let id = fund(&mut state, issuer, issuer, "wood", 4, 100);
    state.living.shops.get_mut(&issuer).unwrap().money = MAX_MONEY;
    let request = state.settlements.contracts[&id].clone();
    let total = money(&state);
    let stock = goods(&state);
    state.advance_settlements(0., &catalog).unwrap();
    assert_eq!(state.settlements.contracts[&id], request);
    assert_eq!(state.living.shops[&issuer].money, MAX_MONEY);
    let path = Save::new();
    state.save(&path.0).unwrap();
    let mut resumed = SandboxState::load_with_catalog(&path.0, &catalog).unwrap();
    assert_eq!(state, resumed);
    let spent = trade(&mut resumed, &catalog, issuer, "wood", 2, false);
    assert_eq!(spent, 100);
    resumed.advance_settlements(60., &catalog).unwrap();
    assert!(!resumed.settlements.contracts.contains_key(&id));
    assert_eq!(resumed.living.shops[&issuer].money, MAX_MONEY);
    assert_eq!(money(&resumed), total);
    assert_eq!(goods(&resumed), stock);
    resumed.advance_settlements(60., &catalog).unwrap();
    assert_eq!(money(&resumed), total);
    resumed.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn saturated_board_turns_over_after_real_trades_and_new_work_continues_after_disk_restore() {
    let (mut state, catalog, issuer, _) = world();
    let items: Vec<_> = catalog.items.iter().cloned().collect();
    assert!(items.len() > MAX_SUPPLY_CONTRACTS);
    for item in items.iter().take(MAX_SUPPLY_CONTRACTS) {
        state
            .living
            .shops
            .get_mut(&issuer)
            .unwrap()
            .stock
            .insert(item.clone(), 39);
        state
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .insert(item.clone(), 1);
    }
    let total = money(&state);
    let stock = goods(&state);
    for _ in 0..4 {
        state.advance_settlements(60., &catalog).unwrap();
    }
    assert_eq!(state.settlements.contracts.len(), MAX_SUPPLY_CONTRACTS);
    let old: Vec<_> = state.settlements.contracts.keys().copied().collect();
    let retired_escrow: u64 = state.settlements.contracts.values().map(|c| c.reward).sum();
    for item in items.iter().take(MAX_SUPPLY_CONTRACTS) {
        trade(&mut state, &catalog, issuer, item, 1, false);
    }
    let new_item = &items[MAX_SUPPLY_CONTRACTS];
    trade(&mut state, &catalog, issuer, new_item, 1, true);
    let before_cash = state.living.shops[&issuer].money;
    let next = state.settlements.next_id;
    state.advance_settlements(60., &catalog).unwrap();
    assert!(
        old.iter()
            .all(|id| !state.settlements.contracts.contains_key(id))
    );
    assert_eq!(state.settlements.contracts.len(), 1);
    let renewed = state.settlements.contracts[&next].clone();
    assert_eq!(
        (renewed.item.as_str(), renewed.amount),
        (new_item.as_str(), 1)
    );
    assert!(old.iter().all(|id| *id < renewed.id));
    assert_eq!(
        state.living.shops[&issuer].money,
        before_cash + retired_escrow - renewed.reward
    );
    assert_eq!(money(&state), total);
    assert_eq!(goods(&state), stock);
    action(
        &mut state,
        &catalog,
        1,
        SettlementCommand::AcceptContract { contract_id: next },
    )
    .unwrap();
    let path = Save::new();
    state.save(&path.0).unwrap();
    let mut resumed = SandboxState::load_with_catalog(&path.0, &catalog).unwrap();
    assert_eq!(state, resumed);
    action(
        &mut resumed,
        &catalog,
        1,
        SettlementCommand::FulfillContract { contract_id: next },
    )
    .unwrap();
    assert_eq!(resumed.settlements.progress[&1].contracts_completed, 1);
    assert_eq!(
        resumed.settlements.progress[&1].experience,
        renewed.experience
    );
    let cash = resumed.living.shops[&issuer].money;
    for _ in 0..4 {
        resumed.advance_settlements(60., &catalog).unwrap();
    }
    assert!(resumed.settlements.contracts.is_empty());
    assert_eq!(resumed.living.shops[&issuer].money, cash);
    assert_eq!(money(&resumed), total);
    assert_eq!(goods(&resumed), stock);
    resumed.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn expiry_or_explicit_return_releases_claim_before_obsolete_offer_retirement() {
    for expires in [false, true] {
        let (mut state, catalog, issuer, producer) = world();
        production(&mut state, producer, 0, 0);
        let id = fund(&mut state, issuer, producer, "wood", 4, 100);
        action(
            &mut state,
            &catalog,
            1,
            SettlementCommand::AcceptContract { contract_id: id },
        )
        .unwrap();
        state.living.producers.get_mut(&producer).unwrap().enabled = false;
        if expires {
            state.settlements.contracts.get_mut(&id).unwrap().expires_at =
                state.settlements.elapsed_seconds + 60.;
        } else {
            action(
                &mut state,
                &catalog,
                1,
                SettlementCommand::CancelContract { contract_id: id },
            )
            .unwrap();
            assert!(
                state.settlements.contracts.contains_key(&id),
                "return itself does not refund"
            );
        }
        let total = money(&state);
        let stock = goods(&state);
        let cash = state.living.shops[&issuer].money;
        let progress = state.settlements.progress.clone();
        state.advance_settlements(60., &catalog).unwrap();
        assert!(!state.settlements.contracts.contains_key(&id));
        assert_eq!(state.living.shops[&issuer].money, cash + 100);
        assert_eq!(state.settlements.progress, progress);
        assert_eq!(money(&state), total);
        assert_eq!(goods(&state), stock);
        state.validate_with_catalog(&catalog).unwrap();
    }
}

#[test]
fn needed_expiry_renews_same_id_without_resizing_or_repricing_existing_terms() {
    let (mut state, catalog, issuer, producer) = world();
    production(&mut state, producer, 11, 1989);
    let id = fund(&mut state, issuer, producer, "wood", 4, 123);
    state.settlements.contracts.get_mut(&id).unwrap().expires_at = 60.;
    let mut expected = state.settlements.contracts[&id].clone();
    let total = money(&state);
    let next = state.settlements.next_id;
    state.advance_settlements(60., &catalog).unwrap();
    expected.expires_at =
        state.settlements.elapsed_seconds + state.settlements.contract_duration_seconds;
    assert_eq!(state.settlements.contracts[&id], expected);
    assert_eq!(state.settlements.next_id, next);
    assert_eq!(money(&state), total);
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn incomplete_receiver_or_issuer_data_never_deletes_or_redirects_escrow() {
    for absent in 0..3 {
        let (mut state, catalog, issuer, producer) = world();
        let id = fund(&mut state, issuer, producer, "wood", 4, 123);
        state.settlements.contracts.get_mut(&id).unwrap().expires_at = 60.;
        match absent {
            0 => {
                state.living.producers.remove(&producer);
            }
            1 => {
                let site = state.living.producers.get_mut(&producer).unwrap();
                site.position = LocalFrame::new(site.position)
                    .to_geo([10., 0., 0.])
                    .unwrap();
            }
            _ => {
                state.living.shops.remove(&issuer);
            }
        }
        let request = state.settlements.contracts[&id].clone();
        let total = money(&state);
        state.advance_settlements(60., &catalog).unwrap();
        assert_eq!(state.settlements.contracts[&id], request);
        assert_eq!(money(&state), total);
        // These deliberately incomplete/replaced records are not asserted to
        // be a valid full save. Their absence cannot authorize deleting value.
    }
}

#[test]
fn filtered_recipient_view_does_not_invent_obsolescence_for_an_absent_receiver() {
    let (mut authority, catalog, issuer, producer) = world();
    production(&mut authority, producer, 0, 0);
    let id = fund(&mut authority, issuer, producer, "wood", 4, 100);
    let destination = authority.settlements.contracts[&id].target_position;
    let far = LocalFrame::new(destination)
        .to_geo([1000., 0., 0.])
        .unwrap();
    authority.set_player_position(1, far).unwrap();
    let view = authority.snapshot_for(1, 1.).unwrap();
    view.living.validate_for(1).unwrap();
    view.settlements.validate_for(1).unwrap();
    assert!(view.living.producers.is_empty());
    let mut replica = SandboxState::default();
    replica.players.insert(1, view.player);
    replica.living = LivingWorld::from_view(view.living);
    replica.settlements = SettlementWorld::from_view(view.settlements);
    let before = replica.clone();
    assert_eq!(
        replica.supply_contract_readiness(1, id, SupplyContractAction::Accept, &catalog),
        Ok(())
    );
    let guide = replica.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(
        matches!(guide.action, Some(GuideAction::Command(Command::Settlement { action: SettlementCommand::AcceptContract { contract_id } })) if contract_id == id)
    );
    assert_eq!(replica, before);
    authority
        .living
        .producers
        .get_mut(&producer)
        .unwrap()
        .enabled = false;
    let before = authority.clone();
    assert!(
        action(
            &mut authority,
            &catalog,
            1,
            SettlementCommand::AcceptContract { contract_id: id }
        )
        .is_err()
    );
    assert_eq!(authority, before);
}

#[test]
fn three_inspections_do_not_reserve_one_new_offer_and_competing_acceptance_is_atomic() {
    let (mut state, catalog, _, producer) = world();
    production(&mut state, producer, 11, 0);
    state.advance_settlements(0., &catalog).unwrap();
    let id = *state.settlements.contracts.keys().next().unwrap();
    let before = state.clone();
    for player in 1..=3 {
        assert_eq!(
            state.supply_contract_readiness(player, id, SupplyContractAction::Accept, &catalog),
            Ok(())
        );
    }
    assert_eq!(state, before);
    action(
        &mut state,
        &catalog,
        2,
        SettlementCommand::AcceptContract { contract_id: id },
    )
    .unwrap();
    let accepted = state.clone();
    for player in [1, 3] {
        assert_eq!(
            state.supply_contract_readiness(player, id, SupplyContractAction::Accept, &catalog),
            Err(SupplyContractBlocker::AlreadyAccepted)
        );
        assert!(
            action(
                &mut state,
                &catalog,
                player,
                SettlementCommand::AcceptContract { contract_id: id }
            )
            .is_err()
        );
        assert_eq!(state, accepted);
    }
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn competing_refunds_use_deterministic_ids_and_retain_every_blocked_escrow() {
    let (mut state, catalog, issuer, _) = world();
    let first = fund(&mut state, issuer, issuer, "wood", 4, 50);
    let second = fund(&mut state, issuer, issuer, "stone", 4, 50);
    state.living.shops.get_mut(&issuer).unwrap().money = MAX_MONEY - 75;
    let mut copy = state.clone();
    let total = money(&state);
    let held = state.settlements.contracts[&second].clone();
    state.advance_settlements(0., &catalog).unwrap();
    copy.advance_settlements(0., &catalog).unwrap();
    assert_eq!(state, copy);
    assert!(!state.settlements.contracts.contains_key(&first));
    assert_eq!(state.settlements.contracts[&second], held);
    assert_eq!(state.living.shops[&issuer].money, MAX_MONEY - 25);
    state.advance_settlements(60., &catalog).unwrap();
    assert_eq!(state.settlements.contracts[&second], held);
    assert_eq!(money(&state), total);
    state.validate_with_catalog(&catalog).unwrap();
}
