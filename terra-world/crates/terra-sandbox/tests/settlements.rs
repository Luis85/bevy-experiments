use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

fn world() -> (SandboxState, Catalog, GeoPosition) {
    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let mut world = SandboxState::default();
    world.add_player(1, origin).unwrap();
    world.add_player(2, origin).unwrap();
    world.seed_living_demo(origin, &[], &catalog).unwrap();
    (world, catalog, origin)
}
fn action(
    world: &mut SandboxState,
    player: u64,
    action: SettlementCommand,
    catalog: &Catalog,
) -> Result<Outcome, SandboxError> {
    world.apply(player, Command::Settlement { action }, catalog)
}
fn found(world: &mut SandboxState, catalog: &Catalog, origin: GeoPosition) -> u64 {
    action(
        world,
        1,
        SettlementCommand::Found {
            name: "Greenhaven".into(),
            center: origin,
            faction_owner: None,
        },
        catalog,
    )
    .unwrap()
    .created_id
    .unwrap()
}
fn total_money(w: &SandboxState) -> u64 {
    w.living.accounts.values().map(|a| a.money).sum::<u64>()
        + w.living.shops.values().map(|s| s.money).sum::<u64>()
        + w.living.units.values().map(|u| u.pocket_money).sum::<u64>()
        + w.settlements
            .territories
            .values()
            .map(|t| t.treasury)
            .sum::<u64>()
        + w.settlements
            .contracts
            .values()
            .map(|c| c.reward)
            .sum::<u64>()
}
fn move_to(w: &mut SandboxState, p: u64, pos: GeoPosition) {
    w.set_player_position(p, pos).unwrap();
}
fn stock(w: &mut SandboxState, item: &str, n: u32) {
    w.players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert(item.into(), n);
}

fn clear_construction_area(w: &mut SandboxState, o: GeoPosition) {
    let active = w.living.accounts[&1].active_unit;
    let frame = LocalFrame::new(o);
    for (i, u) in w
        .living
        .units
        .values_mut()
        .filter(|u| u.id != active)
        .enumerate()
    {
        u.position = frame.to_geo([1000. + i as f64 * 4., 0., 0.]).unwrap();
    }
    for a in w.living.accounts.values() {
        w.players.get_mut(&a.player_id).unwrap().position = w.living.units[&a.active_unit].position;
    }
}
fn stand_near(w: &mut SandboxState, p: GeoPosition) {
    move_to(w, 1, LocalFrame::new(p).to_geo([8., 0., 0.]).unwrap());
}

#[test]
fn founding_costs_and_treasury_are_atomic_and_conserved() {
    let (mut w, c, o) = world();
    let money = total_money(&w);
    let id = found(&mut w, &c, o);
    assert_eq!(w.players[&1].inventory["wood"], 10);
    assert_eq!(w.players[&1].inventory["stone"], 10);
    assert_eq!(w.living.accounts[&1].money, 20_000);
    assert_eq!(w.settlements.territories[&id].treasury, 5000);
    assert_eq!(total_money(&w), money);
    let before = w.clone();
    assert!(
        action(
            &mut w,
            1,
            SettlementCommand::Found {
                name: "Overlap".into(),
                center: o,
                faction_owner: None
            },
            &c
        )
        .is_err()
    );
    assert_eq!(w, before);
    w.validate_with_catalog(&c).unwrap();
}
#[test]
fn territory_blocks_outsider_land_without_confiscating_existing_plots() {
    let (mut w, c, o) = world();
    let p = LocalFrame::new(o).to_geo([50., 0., 0.]).unwrap();
    move_to(&mut w, 2, p);
    w.apply(
        2,
        Command::ClaimPlot {
            center: p,
            radius_m: 10.,
        },
        &c,
    )
    .unwrap();
    let before = w.clone();
    assert!(
        action(
            &mut w,
            1,
            SettlementCommand::Found {
                name: "No confiscation".into(),
                center: o,
                faction_owner: None
            },
            &c
        )
        .is_err()
    );
    assert_eq!(w, before);
    let far = LocalFrame::new(o).to_geo([400., 0., 0.]).unwrap();
    move_to(&mut w, 1, far);
    found(&mut w, &c, far);
    move_to(&mut w, 2, far);
    let before = w.clone();
    assert!(
        w.apply(
            2,
            Command::ClaimPlot {
                center: far,
                radius_m: 10.
            },
            &c
        )
        .is_err()
    );
    assert_eq!(w, before);
}
#[test]
fn personal_territory_cannot_be_bypassed_by_faction_plot_transfer() {
    let (mut w, c, o) = world();
    found(&mut w, &c, o);
    let plot = w
        .apply(
            1,
            Command::ClaimPlot {
                center: o,
                radius_m: 30.,
            },
            &c,
        )
        .unwrap()
        .created_id
        .unwrap();
    let faction = w
        .apply(
            1,
            Command::CreateFaction {
                name: "Green guild".into(),
            },
            &c,
        )
        .unwrap()
        .created_id
        .unwrap();
    let before = w.clone();
    assert!(
        w.apply(
            1,
            Command::TransferPlotToFaction {
                plot_id: plot,
                faction_id: faction
            },
            &c
        )
        .is_err()
    );
    assert_eq!(w, before);
}
#[test]
fn faction_builder_can_operate_territory_but_ordinary_member_cannot() {
    let (mut w, c, o) = world();
    let faction = w
        .apply(
            1,
            Command::CreateFaction {
                name: "Builders".into(),
            },
            &c,
        )
        .unwrap()
        .created_id
        .unwrap();
    w.apply(
        1,
        Command::InviteToFaction {
            faction_id: faction,
            player_id: 2,
        },
        &c,
    )
    .unwrap();
    w.apply(
        2,
        Command::AcceptFactionInvite {
            faction_id: faction,
        },
        &c,
    )
    .unwrap();
    let id = action(
        &mut w,
        1,
        SettlementCommand::Found {
            name: "Shared ground".into(),
            center: o,
            faction_owner: Some(faction),
        },
        &c,
    )
    .unwrap()
    .created_id
    .unwrap();
    let before = w.clone();
    assert!(
        action(
            &mut w,
            2,
            SettlementCommand::Fund {
                settlement_id: id,
                amount: 100
            },
            &c
        )
        .is_err()
    );
    assert_eq!(w, before);
    w.apply(
        1,
        Command::SetFactionRole {
            faction_id: faction,
            player_id: 2,
            role: FactionRole::Builder,
        },
        &c,
    )
    .unwrap();
    let money = total_money(&w);
    action(
        &mut w,
        2,
        SettlementCommand::Fund {
            settlement_id: id,
            amount: 100,
        },
        &c,
    )
    .unwrap();
    assert_eq!(total_money(&w), money);
    w.validate_with_catalog(&c).unwrap();
}
#[test]
fn contributions_reject_unneeded_or_excess_stock_before_mutation() {
    let (mut w, c, o) = world();
    let id = found(&mut w, &c, o);
    stock(&mut w, "planks", 100);
    action(
        &mut w,
        1,
        SettlementCommand::Contribute {
            settlement_id: id,
            item: "planks".into(),
            amount: 30,
        },
        &c,
    )
    .unwrap();
    for (item, amount) in [("planks", 1), ("bread", 1), ("planks", 0)] {
        let before = w.clone();
        assert!(
            action(
                &mut w,
                1,
                SettlementCommand::Contribute {
                    settlement_id: id,
                    item: item.into(),
                    amount
                },
                &c
            )
            .is_err()
        );
        assert_eq!(w, before);
    }
    let far = LocalFrame::new(o).to_geo([0., 30., 0.]).unwrap();
    move_to(&mut w, 1, far);
    let before = w.clone();
    assert!(
        action(
            &mut w,
            1,
            SettlementCommand::Fund {
                settlement_id: id,
                amount: 100
            },
            &c
        )
        .is_err()
    );
    assert_eq!(w, before);
}
#[test]
fn neighborhood_to_city_uses_real_infrastructure_and_project_costs() {
    let (mut w, c, o) = world();
    let id = found(&mut w, &c, o);
    clear_construction_area(&mut w, o);
    let frame = LocalFrame::new(o);
    stock(&mut w, "planks", 500);
    stock(&mut w, "stone_blocks", 250);
    stock(&mut w, "tool", 12);
    w.settlements.progress.get_mut(&1).unwrap().experience = 250;
    let mut plots = Vec::new();
    for x in [-45., 45.] {
        let p = frame.to_geo([x, 0., 0.]).unwrap();
        move_to(&mut w, 1, p);
        let plot = w
            .apply(
                1,
                Command::ClaimPlot {
                    center: p,
                    radius_m: 40.,
                },
                &c,
            )
            .unwrap()
            .created_id
            .unwrap();
        plots.push((plot, p));
        for z in [-15., 0., 15.] {
            let house = LocalFrame::new(p).to_geo([0., 0., z]).unwrap();
            stand_near(&mut w, house);
            w.apply(
                1,
                Command::Build {
                    blueprint: "house".into(),
                    position: house,
                },
                &c,
            )
            .unwrap();
        }
    }
    for (i, z) in [-12., 0., 12.].into_iter().enumerate() {
        let p = LocalFrame::new(plots[0].1).to_geo([15., 0., z]).unwrap();
        move_to(&mut w, 1, p);
        w.apply(
            1,
            Command::Plant {
                plot_id: plots[0].0,
                position: p,
                crop: "wheat".into(),
            },
            &c,
        )
        .unwrap();
        assert_eq!(w.farms.len(), i + 1);
    }
    move_to(&mut w, 1, o);
    let money = total_money(&w);
    for tier in [SettlementTier::Village, SettlementTier::City] {
        for (item, amount) in tier.material_cost() {
            action(
                &mut w,
                1,
                SettlementCommand::Contribute {
                    settlement_id: id,
                    item,
                    amount,
                },
                &c,
            )
            .unwrap();
        }
        if w.settlements.territories[&id].treasury < tier.credit_cost() {
            action(
                &mut w,
                1,
                SettlementCommand::Fund {
                    settlement_id: id,
                    amount: 15_000,
                },
                &c,
            )
            .unwrap();
        }
        action(
            &mut w,
            1,
            SettlementCommand::Upgrade { settlement_id: id },
            &c,
        )
        .unwrap();
        assert_eq!(w.settlements.territories[&id].tier, tier);
    }
    assert_eq!(w.settlements.territories[&id].tier.radius_m(), 700.);
    assert_eq!(total_money(&w), money);
    assert!(w.settlements.territories[&id].stock.is_empty());
    w.validate_with_catalog(&c).unwrap();
}
#[test]
fn upgrade_rejects_foreign_property_in_expanded_region_atomically() {
    let (mut w, c, o) = world();
    let id = found(&mut w, &c, o);
    let p = LocalFrame::new(o).to_geo([200., 0., 0.]).unwrap();
    move_to(&mut w, 2, p);
    w.apply(
        2,
        Command::ClaimPlot {
            center: p,
            radius_m: 20.,
        },
        &c,
    )
    .unwrap();
    // Supply project prerequisites in a trusted fixture; failed expansion must
    // preserve them all, including the paid charter money.
    clear_construction_area(&mut w, o);
    stock(&mut w, "planks", 100);
    stock(&mut w, "stone_blocks", 100);
    w.settlements.progress.get_mut(&1).unwrap().experience = 100;
    let plot = w
        .apply(
            1,
            Command::ClaimPlot {
                center: o,
                radius_m: 40.,
            },
            &c,
        )
        .unwrap()
        .created_id
        .unwrap();
    let _ = plot;
    for x in [-10., 10.] {
        let p = LocalFrame::new(o).to_geo([x, 0., 0.]).unwrap();
        stand_near(&mut w, p);
        w.apply(
            1,
            Command::Build {
                blueprint: "house".into(),
                position: p,
            },
            &c,
        )
        .unwrap();
    }
    move_to(&mut w, 1, o);
    for (item, amount) in SettlementTier::Village.material_cost() {
        action(
            &mut w,
            1,
            SettlementCommand::Contribute {
                settlement_id: id,
                item,
                amount,
            },
            &c,
        )
        .unwrap();
    }
    let before = w.clone();
    assert!(
        action(
            &mut w,
            1,
            SettlementCommand::Upgrade { settlement_id: id },
            &c
        )
        .is_err()
    );
    assert_eq!(w, before);
}
#[test]
fn supply_contract_moves_goods_into_production_and_pays_escrow_once() {
    let (mut w, c, _) = world();
    let money = total_money(&w);
    w.advance_settlements(0., &c).unwrap();
    assert_eq!(total_money(&w), money);
    let job = w
        .settlements
        .contracts
        .values()
        .find(|j| w.living.producers.contains_key(&j.target_site))
        .unwrap()
        .clone();
    action(
        &mut w,
        1,
        SettlementCommand::AcceptContract {
            contract_id: job.id,
        },
        &c,
    )
    .unwrap();
    stock(&mut w, &job.item, job.amount);
    move_to(&mut w, 1, job.target_position);
    let original = w.living.producers[&job.target_site]
        .stock
        .get(&job.item)
        .copied()
        .unwrap_or(0);
    let wallet = w.living.accounts[&1].money;
    action(
        &mut w,
        1,
        SettlementCommand::FulfillContract {
            contract_id: job.id,
        },
        &c,
    )
    .unwrap();
    assert_eq!(
        w.living.producers[&job.target_site].stock[&job.item],
        original + job.amount
    );
    assert_eq!(w.living.accounts[&1].money, wallet + job.reward);
    assert_eq!(w.settlements.progress[&1].contracts_completed, 1);
    assert_eq!(total_money(&w), money);
    let before = w.clone();
    assert!(
        action(
            &mut w,
            1,
            SettlementCommand::FulfillContract {
                contract_id: job.id
            },
            &c
        )
        .is_err()
    );
    assert_eq!(w, before);
    w.validate_with_catalog(&c).unwrap();
}
#[test]
fn contract_failure_keeps_escrow_and_stock_and_respects_spatial_reach() {
    let (mut w, c, _) = world();
    w.advance_settlements(0., &c).unwrap();
    let job = w.settlements.contracts.values().next().unwrap().clone();
    action(
        &mut w,
        1,
        SettlementCommand::AcceptContract {
            contract_id: job.id,
        },
        &c,
    )
    .unwrap();
    for player in [1, 2] {
        let before = w.clone();
        assert!(
            action(
                &mut w,
                player,
                SettlementCommand::FulfillContract {
                    contract_id: job.id
                },
                &c
            )
            .is_err()
        );
        assert_eq!(w, before);
    }
    stock(&mut w, &job.item, job.amount);
    let high = LocalFrame::new(job.target_position)
        .to_geo([0., 30., 0.])
        .unwrap();
    move_to(&mut w, 1, high);
    let before = w.clone();
    assert!(
        action(
            &mut w,
            1,
            SettlementCommand::FulfillContract {
                contract_id: job.id
            },
            &c
        )
        .is_err()
    );
    assert_eq!(w, before);
    move_to(&mut w, 1, job.target_position);
    w.living.accounts.get_mut(&1).unwrap().money = MAX_MONEY;
    let before = w.clone();
    assert!(
        action(
            &mut w,
            1,
            SettlementCommand::FulfillContract {
                contract_id: job.id
            },
            &c
        )
        .is_err()
    );
    assert_eq!(w, before);
}
#[test]
fn contract_capacity_privacy_and_cancel_reopen_are_bounded() {
    let (mut w, c, o) = world();
    w.advance_settlements(0., &c).unwrap();
    let ids: Vec<_> = w.settlements.contracts.keys().copied().collect();
    assert!(ids.len() >= 4);
    for id in ids.iter().take(3) {
        action(
            &mut w,
            1,
            SettlementCommand::AcceptContract { contract_id: *id },
            &c,
        )
        .unwrap();
    }
    let before = w.clone();
    assert!(
        action(
            &mut w,
            1,
            SettlementCommand::AcceptContract {
                contract_id: ids[3]
            },
            &c
        )
        .is_err()
    );
    assert_eq!(w, before);
    let view = w.settlements.snapshot(2, o, 1000.);
    assert!(view.contracts.iter().all(|j| j.claimed_by.is_none()));
    assert!(view.progress.is_none());
    view.validate_for(2).unwrap();
    let money = total_money(&w);
    action(
        &mut w,
        1,
        SettlementCommand::CancelContract {
            contract_id: ids[0],
        },
        &c,
    )
    .unwrap();
    action(
        &mut w,
        2,
        SettlementCommand::AcceptContract {
            contract_id: ids[0],
        },
        &c,
    )
    .unwrap();
    assert_eq!(total_money(&w), money);
}
#[test]
fn expired_work_reopens_without_refunding_or_minting_escrow() {
    let (mut w, c, _) = world();
    w.settlements.contract_duration_seconds = 86400.;
    w.advance_settlements(0., &c).unwrap();
    let id = *w.settlements.contracts.keys().next().unwrap();
    action(
        &mut w,
        1,
        SettlementCommand::AcceptContract { contract_id: id },
        &c,
    )
    .unwrap();
    let reward = w.settlements.contracts[&id].reward;
    let money = total_money(&w);
    w.advance_settlements(86400., &c).unwrap();
    assert_eq!(w.settlements.contracts[&id].claimed_by, None);
    assert_eq!(w.settlements.contracts[&id].reward, reward);
    assert_eq!(total_money(&w), money);
    action(
        &mut w,
        2,
        SettlementCommand::AcceptContract { contract_id: id },
        &c,
    )
    .unwrap();
    w.validate_with_catalog(&c).unwrap();
}
#[test]
fn scarce_prices_are_anchored_and_no_resale_quote_exceeds_lowest_buy_quote() {
    let (mut w, c, _) = world();
    let shop = *w.living.shops.keys().next().unwrap();
    w.living
        .shops
        .get_mut(&shop)
        .unwrap()
        .stock
        .insert("bread".into(), 1);
    w.advance_settlements(0., &c).unwrap();
    let high = w.living.shops[&shop].prices["bread"];
    w.advance_settlements(60., &c).unwrap();
    assert_eq!(w.living.shops[&shop].prices["bread"], high);
    w.living
        .shops
        .get_mut(&shop)
        .unwrap()
        .stock
        .insert("bread".into(), 120);
    w.advance_settlements(60., &c).unwrap();
    let low = w.living.shops[&shop].prices["bread"];
    assert!(high.buy > low.buy);
    assert!(high.sell < low.buy);
    let pos = w.living.shops[&shop].position;
    move_to(&mut w, 1, pos);
    let before = w.living.accounts[&1].money;
    w.apply(
        1,
        Command::Living {
            action: LivingCommand::Buy {
                shop_id: shop,
                item: "bread".into(),
                amount: 2,
            },
        },
        &c,
    )
    .unwrap();
    assert_eq!(w.living.accounts[&1].money, before - 2 * low.buy);
}
#[test]
fn funded_resident_pay_requires_productive_activity_and_conserves_money() {
    let (mut w, c, _) = world();
    for u in w
        .living
        .units
        .values_mut()
        .filter(|u| u.role == UnitRole::Citizen)
    {
        u.pocket_money = 0;
    }
    let money = total_money(&w);
    w.advance_settlements(3600., &c).unwrap();
    assert_eq!(w.settlements.wages_paid, 0);
    w.living.metrics.production_cycles = 1;
    w.advance_settlements(3600., &c).unwrap();
    assert!(w.settlements.wages_paid > 0);
    assert_eq!(total_money(&w), money);
    let paid = w.settlements.wages_paid;
    w.advance_settlements(3600., &c).unwrap();
    assert_eq!(w.settlements.wages_paid, paid);
    assert_eq!(total_money(&w), money);
}
#[test]
fn settlement_state_and_private_views_survive_serialization_and_reject_corruption() {
    let (mut w, c, o) = world();
    found(&mut w, &c, o);
    w.advance_settlements(0., &c).unwrap();
    let job = *w.settlements.contracts.keys().next().unwrap();
    action(
        &mut w,
        1,
        SettlementCommand::AcceptContract { contract_id: job },
        &c,
    )
    .unwrap();
    let bytes = serde_json::to_vec(&w).unwrap();
    let restored: SandboxState = serde_json::from_slice(&bytes).unwrap();
    restored.validate_with_catalog(&c).unwrap();
    assert_eq!(restored, w);
    let view = w.settlements.snapshot(1, o, 1000.);
    view.validate_for(1).unwrap();
    assert!(view.validate_for(2).is_err());
    let mut broken = w.clone();
    broken
        .settlements
        .contracts
        .get_mut(&job)
        .unwrap()
        .target_position
        .latitude += 0.01;
    assert!(broken.validate_with_catalog(&c).is_err());
    let mut broken = w.clone();
    broken
        .settlements
        .reference_prices
        .values_mut()
        .next()
        .unwrap()
        .get_mut("bread")
        .unwrap()
        .buy = 0;
    assert!(broken.validate_with_catalog(&c).is_err());
}
#[test]
fn optional_resource_prices_follow_custom_catalog_without_refilling_stock() {
    let (mut w, mut c, _) = world();
    let shop = *w.living.shops.keys().next().unwrap();
    w.living
        .shops
        .get_mut(&shop)
        .unwrap()
        .stock
        .remove("berries");
    w.living
        .shops
        .get_mut(&shop)
        .unwrap()
        .prices
        .remove("berries");
    w.advance_settlements(0., &c).unwrap();
    assert!(w.living.shops[&shop].prices.contains_key("berries"));
    assert!(!w.living.shops[&shop].stock.contains_key("berries"));
    // Removing optional catalog content for an older small host does not
    // implicitly create that item or offer from the reference economy.
    for s in w.living.shops.values_mut() {
        s.stock.remove("iron_tool");
        s.prices.remove("iron_tool");
    }
    w.settlements.reference_prices.clear();
    w.settlements.contracts.clear();
    c.items.remove("iron_tool");
    c.recipes.remove("iron_tool");
    w.advance_settlements(60., &c).unwrap();
    assert!(!w.living.shops[&shop].prices.contains_key("iron_tool"));
    assert!(
        w.settlements
            .contracts
            .values()
            .all(|j| j.item != "iron_tool")
    );
}
#[test]
fn new_consumables_restore_needs_without_allowing_raw_fish() {
    let (mut w, c, _) = world();
    let id = w.living.accounts[&1].active_unit;
    w.living.units.get_mut(&id).unwrap().needs.food = 0.2;
    w.living.units.get_mut(&id).unwrap().needs.water = 0.2;
    stock(&mut w, "berries", 1);
    stock(&mut w, "cooked_fish", 1);
    stock(&mut w, "fish", 1);
    for item in ["berries", "cooked_fish"] {
        w.apply(
            1,
            Command::Living {
                action: LivingCommand::Consume { item: item.into() },
            },
            &c,
        )
        .unwrap();
    }
    assert!(w.living.units[&id].needs.food > 0.9);
    assert!(w.living.units[&id].needs.water > 0.25);
    let before = w.clone();
    assert!(
        w.apply(
            1,
            Command::Living {
                action: LivingCommand::Consume {
                    item: "fish".into()
                }
            },
            &c
        )
        .is_err()
    );
    assert_eq!(w, before);
}

#[test]
fn territory_plot_capacity_and_shared_plot_inheritance_are_enforced() {
    let (mut w, c, o) = world();
    found(&mut w, &c, o);
    let f = LocalFrame::new(o);
    for x in [-50., -30., -10., 10., 30., 50.] {
        let p = f.to_geo([x, 0., 0.]).unwrap();
        move_to(&mut w, 1, p);
        w.apply(
            1,
            Command::ClaimPlot {
                center: p,
                radius_m: 5.,
            },
            &c,
        )
        .unwrap();
    }
    let p = f.to_geo([70., 0., 0.]).unwrap();
    move_to(&mut w, 1, p);
    let before = w.clone();
    assert!(
        w.apply(
            1,
            Command::ClaimPlot {
                center: p,
                radius_m: 5.
            },
            &c
        )
        .is_err()
    );
    assert_eq!(w, before);
    let (mut w, c, o) = world();
    let faction = w
        .apply(
            1,
            Command::CreateFaction {
                name: "Shared builders".into(),
            },
            &c,
        )
        .unwrap()
        .created_id
        .unwrap();
    action(
        &mut w,
        1,
        SettlementCommand::Found {
            name: "Guild quarter".into(),
            center: o,
            faction_owner: Some(faction),
        },
        &c,
    )
    .unwrap();
    assert_eq!(w.settlement_plot_faction(1, o, 20.).unwrap(), Some(faction));
    let p = LocalFrame::new(o).to_geo([95., 0., 0.]).unwrap();
    assert!(w.settlement_plot_faction(1, p, 20.).is_err());
    let plot = w
        .apply(
            1,
            Command::ClaimPlot {
                center: o,
                radius_m: 20.,
            },
            &c,
        )
        .unwrap()
        .created_id
        .unwrap();
    assert_eq!(w.plots[&plot].faction_owner, Some(faction));
}

#[test]
fn territory_extraction_is_personal_or_shared_with_current_faction_members() {
    let (mut w, c, o) = world();
    found(&mut w, &c, o);
    let p = LocalFrame::new(o).to_geo([20., 0., 20.]).unwrap();
    w.seed_resource("authored:territory-wood".into(), p, "wood", 20, 30.)
        .unwrap();
    move_to(&mut w, 2, p);
    let before = w.clone();
    assert!(
        w.apply(
            2,
            Command::Gather {
                node_id: "authored:territory-wood".into(),
                amount: 1
            },
            &c
        )
        .is_err()
    );
    assert_eq!(w, before);
    move_to(&mut w, 1, p);
    w.apply(
        1,
        Command::Gather {
            node_id: "authored:territory-wood".into(),
            amount: 1,
        },
        &c,
    )
    .unwrap();
    let (mut w, c, o) = world();
    let faction = w
        .apply(
            1,
            Command::CreateFaction {
                name: "Foragers".into(),
            },
            &c,
        )
        .unwrap()
        .created_id
        .unwrap();
    w.apply(
        1,
        Command::InviteToFaction {
            faction_id: faction,
            player_id: 2,
        },
        &c,
    )
    .unwrap();
    w.apply(
        2,
        Command::AcceptFactionInvite {
            faction_id: faction,
        },
        &c,
    )
    .unwrap();
    action(
        &mut w,
        1,
        SettlementCommand::Found {
            name: "Forager grove".into(),
            center: o,
            faction_owner: Some(faction),
        },
        &c,
    )
    .unwrap();
    w.seed_resource("authored:shared-wood".into(), p, "wood", 20, 30.)
        .unwrap();
    move_to(&mut w, 2, p);
    w.check_settlement_interact(2, p).unwrap();
    w.apply(
        2,
        Command::Gather {
            node_id: "authored:shared-wood".into(),
            amount: 1,
        },
        &c,
    )
    .unwrap();
    assert!(w.check_settlement_land(2, p, 1.).is_err());
    w.validate_with_catalog(&c).unwrap();
}

#[test]
fn dormant_settlement_actions_reject_direct_and_wrapped_calls_without_mutation() {
    fn check(mut w: SandboxState, c: &Catalog, label: &str, command: SettlementCommand) {
        w.set_player_dormant(1, true).unwrap();
        w.validate_with_catalog(c).unwrap();
        let before = serde_json::to_vec(&w).unwrap();
        for direct in [true, false] {
            let mut candidate = w.clone();
            let result = if direct {
                candidate.apply_settlement(1, command.clone(), c)
            } else {
                action(&mut candidate, 1, command.clone(), c)
            };
            assert!(
                matches!(result, Err(SandboxError::Invalid(ref message)) if message.contains("resume")),
                "{label}, direct={direct}: {result:?}"
            );
            assert_eq!(serde_json::to_vec(&candidate).unwrap(), before, "{label}");
            candidate.validate_with_catalog(c).unwrap();

            // This is a feasible action, not merely an invalid request that
            // happened to fail. Explicit resume restores the same API path.
            candidate.set_player_dormant(1, false).unwrap();
            let result = if direct {
                candidate.apply_settlement(1, command.clone(), c)
            } else {
                action(&mut candidate, 1, command.clone(), c)
            };
            assert!(
                result.is_ok(),
                "resumed {label}, direct={direct}: {result:?}"
            );
            assert_ne!(serde_json::to_vec(&candidate).unwrap(), before, "{label}");
            candidate.validate_with_catalog(c).unwrap();
        }
    }

    let (mut w, c, o) = world();
    check(
        w.clone(),
        &c,
        "found",
        SettlementCommand::Found {
            name: "Active session required".into(),
            center: o,
            faction_owner: None,
        },
    );
    let settlement_id = found(&mut w, &c, o);
    stock(&mut w, "planks", 100);
    stock(&mut w, "stone_blocks", 100);
    check(
        w.clone(),
        &c,
        "contribute",
        SettlementCommand::Contribute {
            settlement_id,
            item: "planks".into(),
            amount: 1,
        },
    );
    check(
        w.clone(),
        &c,
        "fund",
        SettlementCommand::Fund {
            settlement_id,
            amount: 100,
        },
    );

    // Construct an actually eligible public-works upgrade through ordinary
    // plot/build/contribution commands, with trusted fixture experience.
    let mut upgrade = w.clone();
    clear_construction_area(&mut upgrade, o);
    upgrade.settlements.progress.get_mut(&1).unwrap().experience = 100;
    upgrade
        .apply(
            1,
            Command::ClaimPlot {
                center: o,
                radius_m: 40.,
            },
            &c,
        )
        .unwrap();
    for x in [-10., 10.] {
        let p = LocalFrame::new(o).to_geo([x, 0., 0.]).unwrap();
        stand_near(&mut upgrade, p);
        upgrade
            .apply(
                1,
                Command::Build {
                    blueprint: "house".into(),
                    position: p,
                },
                &c,
            )
            .unwrap();
    }
    move_to(&mut upgrade, 1, o);
    for (item, amount) in SettlementTier::Village.material_cost() {
        action(
            &mut upgrade,
            1,
            SettlementCommand::Contribute {
                settlement_id,
                item,
                amount,
            },
            &c,
        )
        .unwrap();
    }
    check(
        upgrade,
        &c,
        "upgrade",
        SettlementCommand::Upgrade { settlement_id },
    );

    w.advance_settlements(0., &c).unwrap();
    let contract = w
        .settlements
        .contracts
        .values()
        .find(|job| w.living.producers.contains_key(&job.target_site))
        .unwrap()
        .clone();
    let contract_id = contract.id;
    check(
        w.clone(),
        &c,
        "accept contract",
        SettlementCommand::AcceptContract { contract_id },
    );
    action(
        &mut w,
        1,
        SettlementCommand::AcceptContract { contract_id },
        &c,
    )
    .unwrap();
    check(
        w.clone(),
        &c,
        "cancel contract",
        SettlementCommand::CancelContract { contract_id },
    );
    stock(&mut w, &contract.item, contract.amount);
    move_to(&mut w, 1, contract.target_position);
    check(
        w,
        &c,
        "fulfill contract",
        SettlementCommand::FulfillContract { contract_id },
    );
}

#[test]
fn expired_unclaimed_offers_reject_direct_and_wrapped_acceptance_without_mutation() {
    let (mut original, catalog, _) = world();
    original.advance_settlements(0., &catalog).unwrap();
    let offer = original
        .settlements
        .contracts
        .values()
        .next()
        .unwrap()
        .clone();
    for direct in [false, true] {
        for offset in [0., 0.5] {
            let mut w = original.clone();
            w.settlements.elapsed_seconds = offer.expires_at + offset;
            w.validate_with_catalog(&catalog).unwrap();
            let before = serde_json::to_vec(&w).unwrap();
            let money = total_money(&w);
            let command = SettlementCommand::AcceptContract {
                contract_id: offer.id,
            };
            let result = if direct {
                w.apply_settlement(1, command, &catalog)
            } else {
                action(&mut w, 1, command, &catalog)
            };
            assert!(
                matches!(result, Err(SandboxError::Invalid(message)) if message.contains("expired"))
            );
            assert_eq!(serde_json::to_vec(&w).unwrap(), before);
            assert_eq!(total_money(&w), money);
            assert_eq!(w.settlements.contracts[&offer.id], offer);
            w.validate_with_catalog(&catalog).unwrap();
        }
        let mut w = original.clone();
        w.settlements.elapsed_seconds = offer.expires_at - 0.25;
        let money = total_money(&w);
        let command = SettlementCommand::AcceptContract {
            contract_id: offer.id,
        };
        if direct {
            w.apply_settlement(1, command, &catalog).unwrap();
        } else {
            action(&mut w, 1, command, &catalog).unwrap();
        }
        let accepted = &w.settlements.contracts[&offer.id];
        assert_eq!(accepted.claimed_by, Some(1));
        assert_eq!(accepted.reward, offer.reward);
        assert_eq!(
            accepted.expires_at,
            w.settlements.elapsed_seconds + w.settlements.contract_duration_seconds
        );
        assert_eq!(total_money(&w), money);
        w.validate_with_catalog(&catalog).unwrap();
    }
}

#[test]
fn supply_offer_clock_crossing_and_resume_require_maintenance_refresh_before_acceptance() {
    let (mut w, catalog, _) = world();
    w.advance_settlements(0., &catalog).unwrap();
    let offer = w.settlements.contracts.values().next().unwrap().clone();
    w.settlements.elapsed_seconds = offer.expires_at - 0.25;
    w.settlements.update_accumulator = 0.;
    w.settlements.wage_accumulator = 0.;
    w.advance_settlements(0.5, &catalog).unwrap();
    assert_eq!(w.settlements.contracts[&offer.id], offer);
    let encoded = serde_json::to_vec(&w).unwrap();
    let mut resumed: SandboxState = serde_json::from_slice(&encoded).unwrap();
    resumed.validate_with_catalog(&catalog).unwrap();
    let resumed_before = serde_json::to_vec(&resumed).unwrap();
    assert!(
        action(
            &mut resumed,
            2,
            SettlementCommand::AcceptContract {
                contract_id: offer.id
            },
            &catalog
        )
        .is_err()
    );
    assert_eq!(serde_json::to_vec(&resumed).unwrap(), resumed_before);
    let money = total_money(&resumed);
    resumed.advance_settlements(60., &catalog).unwrap();
    let renewed = &resumed.settlements.contracts[&offer.id];
    assert_eq!(renewed.claimed_by, None);
    assert_eq!(renewed.reward, offer.reward);
    assert_eq!(renewed.item, offer.item);
    assert_eq!(renewed.amount, offer.amount);
    assert!(renewed.expires_at > resumed.settlements.elapsed_seconds);
    assert_eq!(total_money(&resumed), money);
    action(
        &mut resumed,
        2,
        SettlementCommand::AcceptContract {
            contract_id: offer.id,
        },
        &catalog,
    )
    .unwrap();
    assert_eq!(resumed.settlements.contracts[&offer.id].claimed_by, Some(2));
    assert_eq!(
        resumed.settlements.contracts[&offer.id].reward,
        offer.reward
    );
    assert_eq!(total_money(&resumed), money);
    resumed.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn heterogeneous_merchants_skip_unpriced_requests_without_spending_offer_capacity() {
    let (mut w, catalog, _) = world();
    // Public work remains centrally funded. Other merchants can stock a
    // different catalog without imposing an undocumented shared-assortment rule.
    for site in w.living.producers.values_mut() {
        site.enabled = false;
    }
    let issuer_id = *w.living.shops.keys().next().unwrap();
    let supported = ["bread", "stone", "water", "wood"];
    let issuer = w.living.shops.get_mut(&issuer_id).unwrap();
    issuer.name = "Central market".into();
    issuer.prices = supported
        .into_iter()
        .map(|item| (item.into(), TradePrice { buy: 100, sell: 50 }))
        .collect();
    issuer.stock = supported
        .into_iter()
        .map(|item| (item.into(), 40))
        .collect();
    let mut retailer = issuer.clone();
    let retailer_id = w.living.next_id;
    w.living.next_id += 1;
    retailer.id = retailer_id;
    retailer.name = "Independent merchant".into();
    retailer.stock.clear();
    for item in ["berries", "fiber", "fish", "iron_ingot"] {
        retailer.prices.insert(
            item.into(),
            TradePrice {
                buy: 200,
                sell: 100,
            },
        );
    }
    w.living.shops.insert(retailer_id, retailer);
    w.validate_with_catalog(&catalog).unwrap();
    let money = total_money(&w);
    let first_contract = w.settlements.next_id;
    let stocks: Vec<_> = w
        .living
        .shops
        .values()
        .map(|shop| (shop.id, shop.stock.clone()))
        .collect();
    let retailer_money = w.living.shops[&retailer_id].money;

    // Unsupported keys sort before and between supported ones. They must not
    // panic or consume any of the four offers available in this batch.
    w.advance_settlements(60., &catalog).unwrap();
    assert_eq!(w.settlements.contracts.len(), 4);
    assert_eq!(w.settlements.next_id, first_contract + 4);
    let offered: std::collections::BTreeSet<_> = w
        .settlements
        .contracts
        .values()
        .map(|offer| {
            assert_eq!(offer.issuer_shop, issuer_id);
            assert_eq!(offer.target_site, retailer_id);
            offer.item.as_str()
        })
        .collect();
    assert_eq!(offered, supported.into_iter().collect());
    assert_eq!(total_money(&w), money);
    assert_eq!(w.living.shops[&retailer_id].money, retailer_money);
    assert_eq!(
        w.living
            .shops
            .values()
            .map(|shop| (shop.id, shop.stock.clone()))
            .collect::<Vec<_>>(),
        stocks
    );
    w.validate_with_catalog(&catalog).unwrap();

    // The supported offers already exist after resume, so a later batch sees
    // only unsupported shortages. It changes neither IDs nor reserved reward.
    let saved = serde_json::to_vec(&w).unwrap();
    let mut resumed: SandboxState = serde_json::from_slice(&saved).unwrap();
    resumed.validate_with_catalog(&catalog).unwrap();
    let offers = resumed.settlements.contracts.clone();
    let next_id = resumed.settlements.next_id;
    resumed.advance_settlements(60., &catalog).unwrap();
    assert_eq!(resumed.settlements.contracts, offers);
    assert_eq!(resumed.settlements.next_id, next_id);
    assert_eq!(total_money(&resumed), money);
    assert_eq!(
        resumed
            .living
            .shops
            .values()
            .map(|shop| (shop.id, shop.stock.clone()))
            .collect::<Vec<_>>(),
        stocks
    );
    resumed.validate_with_catalog(&catalog).unwrap();
}
