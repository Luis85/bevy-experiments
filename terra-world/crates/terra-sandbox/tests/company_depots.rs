//! Warehouse commerce exercises actual stock, money, movement and saved contracts.
use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

struct Fixture {
    world: SandboxState,
    catalog: Catalog,
    garage: u64,
    depot: u64,
    terminal: u64,
    shop: u64,
    vehicle: u64,
    driver: u64,
}

fn command(
    world: &mut SandboxState,
    owner: u64,
    action: CompanyCommand,
    catalog: &Catalog,
) -> Result<Outcome, SandboxError> {
    world.apply(
        owner,
        Command::Transport {
            action: TransportCommand::Company { action },
        },
        catalog,
    )
}

fn fixture() -> Fixture {
    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let frame = LocalFrame::new(origin);
    let mut world = SandboxState::new();
    world.add_player(1, origin).unwrap();
    world.add_player(2, origin).unwrap();
    world.seed_living_demo(origin, &[], &catalog).unwrap();
    world.ensure_transport_demo(&catalog).unwrap();
    let garage = world
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Garage)
        .unwrap()
        .id;
    let depot = world
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Depot)
        .unwrap()
        .id;
    let shop = *world.living.shops.keys().next().unwrap();
    let terminal = world
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Terminal && f.linked_site == Some(shop))
        .unwrap()
        .id;
    let vehicle = world
        .transport
        .vehicles
        .values()
        .find(|v| v.owner == 1)
        .unwrap()
        .unit_id;
    // Isolate the corridor from seeded pedestrians and traffic, without disabling
    // the ordinary physical vehicle movement or contact path under test.
    for (facility, x) in [(garage, 500.), (depot, 600.), (terminal, 680.)] {
        world
            .transport
            .facilities
            .get_mut(&facility)
            .unwrap()
            .position = frame.to_geo([x, 0., -500.]).unwrap();
    }
    world.living.shops.get_mut(&shop).unwrap().position =
        world.transport.facilities[&terminal].position;
    for site in world.living.producers.values_mut() {
        site.enabled = false;
    }
    for unit in world.living.units.values_mut() {
        unit.needs.food = 1.;
        unit.needs.water = 1.;
    }
    world.living.roads.clear();
    world
        .set_player_position(1, world.transport.facilities[&garage].position)
        .unwrap();
    world.living.units.get_mut(&vehicle).unwrap().position =
        world.transport.facilities[&garage].position;
    command(
        &mut world,
        1,
        CompanyCommand::Create {
            name: "Warehouse Cooperative".into(),
        },
        &catalog,
    )
    .unwrap();
    command(
        &mut world,
        1,
        CompanyCommand::Deposit { amount: 20_000 },
        &catalog,
    )
    .unwrap();
    let driver = command(
        &mut world,
        1,
        CompanyCommand::HireDriver {
            name: "Robin".into(),
        },
        &catalog,
    )
    .unwrap()
    .created_id
    .unwrap();
    command(
        &mut world,
        1,
        CompanyCommand::OpenDepot {
            facility_id: garage,
        },
        &catalog,
    )
    .unwrap();
    world.validate_with_catalog(&catalog).unwrap();
    Fixture {
        world,
        catalog,
        garage,
        depot,
        terminal,
        shop,
        vehicle,
        driver,
    }
}

fn depot_stock(world: &SandboxState, facility: u64) -> &Inventory {
    &world.transport.companies[&1]
        .depots
        .iter()
        .find(|d| d.facility_id == facility)
        .unwrap()
        .stock
}

fn seed_depot(f: &mut Fixture, facility: u64, item: &str, amount: u32) {
    f.world
        .transport
        .companies
        .get_mut(&1)
        .unwrap()
        .depots
        .iter_mut()
        .find(|d| d.facility_id == facility)
        .unwrap()
        .stock
        .insert(item.into(), amount);
}

fn route(f: &mut Fixture, name: &str, source: u64, destination: u64, service: FleetService) -> u64 {
    command(
        &mut f.world,
        1,
        CompanyCommand::CreateRoute {
            name: name.into(),
            stops: vec![source, destination],
            service,
        },
        &f.catalog,
    )
    .unwrap()
    .created_id
    .unwrap()
}

fn assign(f: &mut Fixture, route: u64) {
    if f.world.transport.companies[&1].drivers[0]
        .vehicle_id
        .is_some()
    {
        command(
            &mut f.world,
            1,
            CompanyCommand::ReleaseVehicle {
                driver_id: f.driver,
            },
            &f.catalog,
        )
        .unwrap();
    }
    command(
        &mut f.world,
        1,
        CompanyCommand::Assign {
            driver_id: f.driver,
            vehicle_id: f.vehicle,
            route_id: route,
        },
        &f.catalog,
    )
    .unwrap();
    command(
        &mut f.world,
        1,
        CompanyCommand::SetDriverRunning {
            driver_id: f.driver,
            running: true,
        },
        &f.catalog,
    )
    .unwrap();
}

fn begin_sale(f: &mut Fixture, amount: u32) {
    let route = route(
        f,
        "Warehouse to market",
        f.garage,
        f.terminal,
        FleetService::DepotFreight {
            item: "wood".into(),
            amount,
        },
    );
    assign(f, route);
    f.world.advance_transport(0.1).unwrap();
}

fn money(world: &SandboxState) -> u64 {
    world.transport.treasury
        + world.living.accounts.values().map(|a| a.money).sum::<u64>()
        + world
            .living
            .units
            .values()
            .map(|u| u.pocket_money)
            .sum::<u64>()
        + world.living.shops.values().map(|s| s.money).sum::<u64>()
        + world
            .transport
            .jobs
            .values()
            .filter_map(|j| j.assignment.as_ref())
            .map(|a| a.reward_escrow)
            .sum::<u64>()
        + world
            .transport
            .companies
            .values()
            .map(|c| {
                c.cash
                    + c.departed_wages
                    + c.drivers
                        .iter()
                        .map(|d| {
                            d.wages_earned
                                + d.trip.as_ref().map_or(0, |t| {
                                    t.reward_escrow
                                        + t.goods_payment.as_ref().map_or(0, |p| p.total)
                                })
                        })
                        .sum::<u64>()
            })
            .sum::<u64>()
}

fn goods(world: &SandboxState, item: &str) -> u64 {
    let count = |stock: &Inventory| u64::from(stock.get(item).copied().unwrap_or(0));
    world
        .players
        .values()
        .map(|p| count(&p.inventory))
        .sum::<u64>()
        + world
            .living
            .shops
            .values()
            .map(|s| count(&s.stock))
            .sum::<u64>()
        + world
            .living
            .producers
            .values()
            .map(|s| count(&s.stock))
            .sum::<u64>()
        + world
            .transport
            .vehicles
            .values()
            .map(|v| count(&v.cargo))
            .sum::<u64>()
        + world
            .transport
            .jobs
            .values()
            .filter_map(|j| j.assignment.as_ref())
            .map(|a| count(&a.reserved_cargo))
            .sum::<u64>()
        + world
            .living
            .deliveries
            .iter()
            .filter(|d| d.item == item && d.phase != DeliveryPhase::ToPickup)
            .map(|d| u64::from(d.amount))
            .sum::<u64>()
        + world
            .transport
            .companies
            .values()
            .map(|c| {
                count(&c.household_consumed) + c.depots.iter().map(|d| count(&d.stock)).sum::<u64>()
            })
            .sum::<u64>()
}

fn trade_action(quote: CompanyDepotTradeQuote) -> CompanyCommand {
    CompanyCommand::TradeDepotStock {
        facility_id: quote.facility_id,
        shop_id: quote.shop_id,
        item: quote.item,
        amount: quote.amount,
        buy: quote.buy,
        quoted_total: quote.total_cost,
    }
}

fn assert_denied_unchanged(f: &mut Fixture, owner: u64, action: CompanyCommand) {
    let before = serde_json::to_value(&f.world).unwrap();
    assert!(command(&mut f.world, owner, action, &f.catalog).is_err());
    assert_eq!(serde_json::to_value(&f.world).unwrap(), before);
}

fn drive_until(f: &mut Fixture, test: impl Fn(&SandboxState) -> bool) {
    for _ in 0..2000 {
        if test(&f.world) {
            return;
        }
        f.world.advance_living(0.25, 0., &f.catalog).unwrap();
    }
    panic!(
        "condition not reached: {:?}, position {:?}",
        f.world.transport.companies[&1].drivers[0], f.world.living.units[&f.vehicle].position
    );
}

#[test]
fn nearby_market_trades_company_cash_and_private_stock_with_exact_quote_rollback() {
    let mut f = fixture();
    f.world.living.shops.get_mut(&f.shop).unwrap().position =
        f.world.transport.facilities[&f.garage].position;
    let total = money(&f.world);
    let wood = goods(&f.world, "wood");
    let wallet = f.world.living.accounts[&1].money;
    let quote = f
        .world
        .quote_company_depot_trade(1, f.garage, f.shop, "wood", 4, true)
        .unwrap();
    assert!(quote.total_cost > 0);
    let cash = f.world.transport.companies[&1].cash;
    command(&mut f.world, 1, trade_action(quote.clone()), &f.catalog).unwrap();
    assert_eq!(depot_stock(&f.world, f.garage)["wood"], 4);
    assert_eq!(
        f.world.transport.companies[&1].cash,
        cash - quote.total_cost
    );
    let sell = f
        .world
        .quote_company_depot_trade(1, f.garage, f.shop, "wood", 2, false)
        .unwrap();
    command(&mut f.world, 1, trade_action(sell.clone()), &f.catalog).unwrap();
    assert_eq!(f.world.transport.companies[&1].revenue, sell.total_cost);
    assert_eq!(f.world.living.accounts[&1].money, wallet);
    assert_eq!(money(&f.world), total);
    assert_eq!(goods(&f.world, "wood"), wood);
    let mut stale = quote.clone();
    stale.total_cost += 1;
    assert_denied_unchanged(&mut f, 1, trade_action(stale));
    assert_denied_unchanged(&mut f, 2, trade_action(quote.clone()));
    f.world
        .living
        .shops
        .get_mut(&f.shop)
        .unwrap()
        .prices
        .get_mut("wood")
        .unwrap()
        .buy += 1;
    assert_denied_unchanged(&mut f, 1, trade_action(quote.clone()));
    let distant = f.world.transport.facilities[&f.depot].position;
    f.world.set_player_position(1, distant).unwrap();
    assert_denied_unchanged(&mut f, 1, trade_action(quote));
    f.world.validate_with_catalog(&f.catalog).unwrap();
}

#[test]
fn failed_market_capacity_inventory_and_payer_checks_publish_no_partial_trade() {
    for case in 0..8 {
        let mut f = fixture();
        let garage = f.garage;
        f.world.living.shops.get_mut(&f.shop).unwrap().position =
            f.world.transport.facilities[&garage].position;
        seed_depot(&mut f, garage, "wood", 8);
        let buy = case < 4;
        match case {
            0 => {
                let amount = f.world.transport.companies[&1].cash;
                command(
                    &mut f.world,
                    1,
                    CompanyCommand::Withdraw { amount },
                    &f.catalog,
                )
                .unwrap();
            }
            1 => {
                f.world
                    .living
                    .shops
                    .get_mut(&f.shop)
                    .unwrap()
                    .stock
                    .remove("wood");
            }
            2 => seed_depot(&mut f, garage, "stone", 1992),
            3 => f.world.living.shops.get_mut(&f.shop).unwrap().money = MAX_MONEY,
            4 => {
                f.world.transport.companies.get_mut(&1).unwrap().depots[0]
                    .stock
                    .clear();
            }
            5 => f.world.living.shops.get_mut(&f.shop).unwrap().money = 0,
            6 => {
                // Fund this capacity fixture, then fill the company account
                // through authority so its recorded cash still reconciles.
                f.world.living.accounts.get_mut(&1).unwrap().money = MAX_MONEY;
                let amount = MAX_MONEY - f.world.transport.companies[&1].cash;
                command(
                    &mut f.world,
                    1,
                    CompanyCommand::Deposit { amount },
                    &f.catalog,
                )
                .unwrap();
            }
            _ => {
                f.world
                    .living
                    .shops
                    .get_mut(&f.shop)
                    .unwrap()
                    .stock
                    .insert("wood".into(), MAX_ITEM_COUNT);
            }
        }
        f.world.validate_with_catalog(&f.catalog).unwrap();
        let price = f.world.living.shops[&f.shop].prices["wood"];
        let total = 4 * if buy { price.buy } else { price.sell };
        assert!(
            f.world
                .quote_company_depot_trade(1, garage, f.shop, "wood", 4, buy)
                .is_err(),
            "case {case}"
        );
        let shop = f.shop;
        assert_denied_unchanged(
            &mut f,
            1,
            CompanyCommand::TradeDepotStock {
                facility_id: garage,
                shop_id: shop,
                item: "wood".into(),
                amount: 4,
                buy,
                quoted_total: total,
            },
        );
    }
}

#[test]
fn capped_buyer_refund_keeps_goods_and_escrow_until_a_real_sale_makes_money_space() {
    let mut f = fixture();
    let garage = f.garage;
    seed_depot(&mut f, garage, "wood", 8);
    f.world
        .living
        .shops
        .get_mut(&f.shop)
        .unwrap()
        .stock
        .remove("wood");
    begin_sale(&mut f, 8);
    f.world.living.shops.get_mut(&f.shop).unwrap().money = MAX_MONEY;
    let total = money(&f.world);
    let wood = goods(&f.world, "wood");
    let commitment = f.world.transport.companies[&1].drivers[0]
        .trip
        .clone()
        .unwrap();
    command(
        &mut f.world,
        1,
        CompanyCommand::ReturnLoad {
            driver_id: f.driver,
        },
        &f.catalog,
    )
    .unwrap();
    f.world.advance_transport(0.1).unwrap();
    assert_eq!(
        f.world.transport.companies[&1].drivers[0].status,
        FleetStatus::DestinationFull
    );
    assert_eq!(
        f.world.transport.companies[&1].drivers[0]
            .trip
            .as_ref()
            .unwrap()
            .goods_payment,
        commitment.goods_payment
    );
    assert_eq!(f.world.transport.vehicles[&f.vehicle].cargo["wood"], 8);
    assert_eq!(f.world.company_depot_reserved_units(1, garage), 8);
    assert_eq!(money(&f.world), total);
    // A real customer sale spends the buyer's capped wallet and permits the
    // previously blocked cancellation refund, with no host credit adjustment.
    f.world
        .set_player_position(2, f.world.living.shops[&f.shop].position)
        .unwrap();
    f.world
        .apply(
            2,
            Command::Living {
                action: LivingCommand::Sell {
                    shop_id: f.shop,
                    item: "wood".into(),
                    amount: 8,
                },
            },
            &f.catalog,
        )
        .unwrap();
    f.world.advance_transport(0.1).unwrap();
    assert!(f.world.transport.companies[&1].drivers[0].trip.is_none());
    assert_eq!(f.world.living.shops[&f.shop].money, MAX_MONEY);
    assert_eq!(depot_stock(&f.world, garage)["wood"], 8);
    assert_eq!(f.world.transport.companies[&1].revenue, 0);
    assert_eq!(money(&f.world), total);
    assert_eq!(goods(&f.world, "wood"), wood);
    f.world.validate_with_catalog(&f.catalog).unwrap();
}

#[test]
fn warehouse_handover_moves_buyer_commitment_and_space_reservation_exactly_once() {
    let mut f = fixture();
    let garage = f.garage;
    seed_depot(&mut f, garage, "wood", 8);
    f.world
        .living
        .shops
        .get_mut(&f.shop)
        .unwrap()
        .stock
        .remove("wood");
    let relief = command(
        &mut f.world,
        1,
        CompanyCommand::HireDriver { name: "Ari".into() },
        &f.catalog,
    )
    .unwrap()
    .created_id
    .unwrap();
    let replacement = f
        .world
        .transport
        .vehicles
        .values()
        .find(|v| v.owner == 2)
        .unwrap()
        .unit_id;
    f.world
        .transport
        .vehicles
        .get_mut(&replacement)
        .unwrap()
        .owner = 1;
    f.world.living.units.get_mut(&replacement).unwrap().owner = Some(1);
    let position = f.world.living.units[&f.vehicle].position;
    f.world.living.units.get_mut(&replacement).unwrap().position =
        LocalFrame::new(position).to_geo([0., 0., 10.]).unwrap();
    begin_sale(&mut f, 8);
    command(
        &mut f.world,
        1,
        CompanyCommand::HoldVehicle {
            driver_id: f.driver,
        },
        &f.catalog,
    )
    .unwrap();
    let commitment = f.world.transport.companies[&1].drivers[0]
        .trip
        .clone()
        .unwrap();
    let total = money(&f.world);
    let wood = goods(&f.world, "wood");
    command(
        &mut f.world,
        1,
        CompanyCommand::HandoverLoad {
            driver_id: f.driver,
            relief_driver_id: relief,
            vehicle_id: replacement,
        },
        &f.catalog,
    )
    .unwrap();
    let company = &f.world.transport.companies[&1];
    assert!(company.drivers[0].trip.is_none());
    assert_eq!(
        company.drivers[1].trip.as_ref().unwrap().goods_payment,
        commitment.goods_payment
    );
    assert_eq!(company.drivers[1].status, FleetStatus::Held);
    assert_eq!(f.world.company_depot_reserved_units(1, garage), 8);
    assert!(f.world.transport.vehicles[&f.vehicle].cargo.is_empty());
    assert_eq!(f.world.transport.vehicles[&replacement].cargo["wood"], 8);
    assert_eq!(money(&f.world), total);
    assert_eq!(goods(&f.world, "wood"), wood);
    f.world = serde_json::from_slice(&serde_json::to_vec(&f.world).unwrap()).unwrap();
    f.world.validate_with_catalog(&f.catalog).unwrap();
    command(
        &mut f.world,
        1,
        CompanyCommand::SetDriverRunning {
            driver_id: relief,
            running: false,
        },
        &f.catalog,
    )
    .unwrap();
    drive_until(&mut f, |w| {
        w.transport.companies[&1].drivers[1].trip.is_none()
    });
    assert_eq!(
        f.world.transport.companies[&1].revenue,
        commitment.goods_payment.unwrap().total
    );
    assert_eq!(f.world.company_depot_reserved_units(1, garage), 0);
    assert_eq!(money(&f.world), total);
    assert_eq!(goods(&f.world, "wood"), wood);
    f.world.validate_with_catalog(&f.catalog).unwrap();
}

#[test]
fn warehouse_sale_reserves_buyer_cash_refund_space_and_pays_only_after_physical_delivery() {
    let mut f = fixture();
    let garage = f.garage;
    seed_depot(&mut f, garage, "wood", 8);
    seed_depot(&mut f, garage, "stone", 1992);
    f.world
        .living
        .shops
        .get_mut(&f.shop)
        .unwrap()
        .stock
        .remove("wood");
    let total = money(&f.world);
    let wood = goods(&f.world, "wood");
    let issuer = f.world.transport.treasury;
    let buyer = f.world.living.shops[&f.shop].money;
    let company_cash = f.world.transport.companies[&1].cash;
    begin_sale(&mut f, 8);
    let trip = f.world.transport.companies[&1].drivers[0]
        .trip
        .clone()
        .unwrap();
    assert_eq!(trip.reward_escrow, 0);
    let payment = trip.goods_payment.unwrap();
    assert_eq!(f.world.living.shops[&f.shop].money, buyer - payment.total);
    assert_eq!(f.world.transport.companies[&1].cash, company_cash);
    assert_eq!(f.world.transport.treasury, issuer);
    assert_eq!(f.world.company_depot_reserved_units(1, garage), 8);
    assert_eq!(f.world.transport.vehicles[&f.vehicle].cargo["wood"], 8);
    f.world
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("stone".into(), 1);
    assert_denied_unchanged(
        &mut f,
        1,
        CompanyCommand::TransferDepotStock {
            facility_id: garage,
            item: "stone".into(),
            amount: 1,
            deposit: true,
        },
    );
    // A change to retail prices while the truck is loaded cannot rewrite its commitment.
    f.world
        .living
        .shops
        .get_mut(&f.shop)
        .unwrap()
        .prices
        .get_mut("wood")
        .unwrap()
        .sell += 17;
    f.world = serde_json::from_slice(&serde_json::to_vec(&f.world).unwrap()).unwrap();
    f.world.validate_with_catalog(&f.catalog).unwrap();
    // A positional jump cannot manufacture earned travel or payment.
    let original = f.world.living.units[&f.vehicle].position;
    f.world.living.units.get_mut(&f.vehicle).unwrap().position =
        f.world.transport.facilities[&f.terminal].position;
    f.world.advance_transport(0.1).unwrap();
    assert_eq!(f.world.transport.companies[&1].revenue, 0);
    assert!(f.world.transport.companies[&1].drivers[0].trip.is_some());
    f.world.living.units.get_mut(&f.vehicle).unwrap().position = original;
    command(
        &mut f.world,
        1,
        CompanyCommand::SetDriverRunning {
            driver_id: f.driver,
            running: false,
        },
        &f.catalog,
    )
    .unwrap();
    drive_until(&mut f, |w| {
        w.transport.companies[&1].drivers[0].trip.is_none()
    });
    assert_eq!(f.world.transport.companies[&1].revenue, payment.total);
    assert_eq!(
        f.world.transport.companies[&1].drivers[0].deliveries_completed,
        1
    );
    assert_eq!(f.world.living.shops[&f.shop].stock["wood"], 8);
    assert_eq!(f.world.company_depot_reserved_units(1, garage), 0);
    assert_eq!(f.world.transport.treasury, issuer);
    assert_eq!(money(&f.world), total);
    assert_eq!(goods(&f.world, "wood"), wood);
    f.world.validate_with_catalog(&f.catalog).unwrap();
}

#[test]
fn sale_waits_for_real_buyer_funds_and_returns_every_reserved_credit_and_item() {
    let mut f = fixture();
    let garage = f.garage;
    seed_depot(&mut f, garage, "wood", 8);
    f.world
        .living
        .shops
        .get_mut(&f.shop)
        .unwrap()
        .stock
        .remove("wood");
    f.world.living.shops.get_mut(&f.shop).unwrap().money = 0;
    begin_sale(&mut f, 8);
    assert_eq!(
        f.world.transport.companies[&1].drivers[0].status,
        FleetStatus::WaitingBuyerFunds
    );
    assert!(f.world.transport.vehicles[&f.vehicle].cargo.is_empty());
    // A real retail purchase recapitalizes the buyer; no periodic cash top-up.
    f.world
        .set_player_position(2, f.world.living.shops[&f.shop].position)
        .unwrap();
    f.world
        .apply(
            2,
            Command::Living {
                action: LivingCommand::Buy {
                    shop_id: f.shop,
                    item: "bread".into(),
                    amount: 1,
                },
            },
            &f.catalog,
        )
        .unwrap();
    let total = money(&f.world);
    let buyer = f.world.living.shops[&f.shop].money;
    let wood = goods(&f.world, "wood");
    f.world.advance_transport(0.1).unwrap();
    let loaded = f.world.transport.companies[&1].drivers[0]
        .trip
        .clone()
        .unwrap();
    assert!(
        loaded.service.cargo_load().unwrap().1 < 8,
        "finite buyer cash should limit this load"
    );
    assert_eq!(f.world.transport.companies[&1].revenue, 0);
    command(
        &mut f.world,
        1,
        CompanyCommand::ReturnLoad {
            driver_id: f.driver,
        },
        &f.catalog,
    )
    .unwrap();
    f.world.advance_transport(0.1).unwrap();
    assert!(f.world.transport.companies[&1].drivers[0].trip.is_none());
    assert_eq!(f.world.living.shops[&f.shop].money, buyer);
    assert_eq!(depot_stock(&f.world, garage)["wood"], 8);
    assert_eq!(f.world.transport.companies[&1].revenue, 0);
    assert_eq!(money(&f.world), total);
    assert_eq!(goods(&f.world, "wood"), wood);
    f.world.validate_with_catalog(&f.catalog).unwrap();
}

#[test]
fn private_transfer_reserves_mixed_capacity_uses_own_bays_and_has_no_public_fare() {
    let mut f = fixture();
    let (garage, depot) = (f.garage, f.depot);
    let source_public = f.world.living.producers.clone();
    f.world
        .set_player_position(1, f.world.transport.facilities[&depot].position)
        .unwrap();
    command(
        &mut f.world,
        1,
        CompanyCommand::OpenDepot { facility_id: depot },
        &f.catalog,
    )
    .unwrap();
    assert_eq!(f.world.living.producers, source_public);
    seed_depot(&mut f, garage, "wood", 8);
    seed_depot(&mut f, depot, "stone", 1996);
    let route = route(
        &mut f,
        "Private transfer",
        garage,
        depot,
        FleetService::DepotTransfer {
            item: "wood".into(),
            amount: 8,
        },
    );
    assign(&mut f, route);
    let money_before = money(&f.world);
    let wood = goods(&f.world, "wood");
    let issuer = f.world.transport.treasury;
    f.world.advance_transport(0.1).unwrap();
    let trip = f.world.transport.companies[&1].drivers[0]
        .trip
        .as_ref()
        .unwrap();
    assert_eq!(trip.service.cargo_load().unwrap().1, 4);
    assert_eq!(trip.reward_escrow, 0);
    assert!(trip.goods_payment.is_none());
    assert_eq!(f.world.company_depot_reserved_units(1, garage), 4);
    assert_eq!(f.world.company_depot_reserved_units(1, depot), 4);
    assert_denied_unchanged(
        &mut f,
        1,
        CompanyCommand::TransferDepotStock {
            facility_id: depot,
            item: "stone".into(),
            amount: 1,
            deposit: true,
        },
    );
    command(
        &mut f.world,
        1,
        CompanyCommand::SetDriverRunning {
            driver_id: f.driver,
            running: false,
        },
        &f.catalog,
    )
    .unwrap();
    drive_until(&mut f, |w| {
        w.transport.companies[&1].drivers[0].trip.is_none()
    });
    assert_eq!(depot_stock(&f.world, depot).values().sum::<u32>(), 2000);
    assert_eq!(depot_stock(&f.world, garage)["wood"], 4);
    assert_eq!(f.world.transport.companies[&1].revenue, 0);
    assert_eq!(f.world.transport.treasury, issuer);
    assert_eq!(money(&f.world), money_before);
    assert_eq!(goods(&f.world, "wood"), wood);
    assert!(
        f.world
            .snapshot_for(2, 2000.)
            .unwrap()
            .transport
            .company
            .is_none()
    );
    let mut invalid = f.world.clone();
    invalid
        .transport
        .companies
        .get_mut(&1)
        .unwrap()
        .depots
        .pop();
    assert!(invalid.validate_with_catalog(&f.catalog).is_err());
    f.world.validate_with_catalog(&f.catalog).unwrap();
}

#[test]
fn loaded_sale_survives_dormancy_partition_restart_and_rejects_forged_escrow_or_reserved_space() {
    let mut f = fixture();
    let garage = f.garage;
    seed_depot(&mut f, garage, "wood", 8);
    f.world
        .living
        .shops
        .get_mut(&f.shop)
        .unwrap()
        .stock
        .remove("wood");
    begin_sale(&mut f, 8);
    let money_before = money(&f.world);
    let wood = goods(&f.world, "wood");
    for mutation in 0..4 {
        let mut invalid = f.world.clone();
        let company = invalid.transport.companies.get_mut(&1).unwrap();
        match mutation {
            0 => {
                company.drivers[0]
                    .trip
                    .as_mut()
                    .unwrap()
                    .goods_payment
                    .as_mut()
                    .unwrap()
                    .total += 1
            }
            1 => {
                company.drivers[0]
                    .trip
                    .as_mut()
                    .unwrap()
                    .goods_payment
                    .as_mut()
                    .unwrap()
                    .shop_id = u64::MAX
            }
            2 => company.drivers[0].trip.as_mut().unwrap().goods_payment = None,
            _ => {
                company.depots[0].stock.insert("stone".into(), 2000);
            }
        }
        assert!(
            invalid.validate_with_catalog(&f.catalog).is_err(),
            "mutation {mutation}"
        );
    }
    f.world.set_player_dormant(1, true).unwrap();
    let mut before = f.world.transport.companies[&1].clone();
    let before_elapsed = f.world.transport.elapsed_seconds;
    let partition = RegionPartition::from_state(&f.world, 15, &f.catalog).unwrap();
    let mut working = partition
        .working_set(&Default::default(), &f.catalog)
        .unwrap();
    for _ in 0..120 {
        working.state.advance_living(1., 0., &f.catalog).unwrap();
    }
    let partition = partition
        .merge_active(&working.state, &working.active_regions, &f.catalog)
        .unwrap();
    f.world = partition.assemble_all(&f.catalog).unwrap();
    // The timestamp follows the global clock so dormant time grants no earned
    // travel. All commitments, goods, money and actual progress stay frozen.
    before.drivers[0]
        .trip
        .as_mut()
        .unwrap()
        .pickup_elapsed_seconds += f.world.transport.elapsed_seconds - before_elapsed;
    assert_eq!(f.world.transport.companies[&1], before);
    assert_eq!(money(&f.world), money_before);
    assert_eq!(goods(&f.world, "wood"), wood);
    f.world.set_player_dormant(1, false).unwrap();
    command(
        &mut f.world,
        1,
        CompanyCommand::SetDriverRunning {
            driver_id: f.driver,
            running: false,
        },
        &f.catalog,
    )
    .unwrap();
    drive_until(&mut f, |w| {
        w.transport.companies[&1].drivers[0].trip.is_none()
    });
    assert_eq!(
        f.world.transport.companies[&1].drivers[0].deliveries_completed,
        1
    );
    assert_eq!(money(&f.world), money_before);
    assert_eq!(goods(&f.world, "wood"), wood);
    f.world.validate_with_catalog(&f.catalog).unwrap();
}

fn career_step(
    f: &mut Fixture,
    conserved_money: u64,
    conserved_water: u64,
    conserved_bread: u64,
    recoveries: &mut u32,
) {
    f.world.advance_living(0.25, 0., &f.catalog).unwrap();
    if f.world.transport.companies[&1].drivers[0].status == FleetStatus::NeedsFuel {
        let position = f.world.living.units[&f.vehicle].position;
        f.world.set_player_position(1, position).unwrap();
        command(
            &mut f.world,
            1,
            CompanyCommand::HoldVehicle {
                driver_id: f.driver,
            },
            &f.catalog,
        )
        .unwrap();
        let quote = f
            .world
            .company_roadside_quote(1, f.vehicle, f.garage, 2, false)
            .unwrap();
        command(
            &mut f.world,
            1,
            CompanyCommand::RoadsideService {
                vehicle_id: quote.vehicle_id,
                facility_id: quote.facility_id,
                litres: quote.litres,
                repair: quote.repair,
                quoted_total: quote.total_cost,
            },
            &f.catalog,
        )
        .unwrap();
        // Finish a loaded commitment, or resume its physical journey to pickup.
        let running = f.world.transport.companies[&1].drivers[0].trip.is_none();
        command(
            &mut f.world,
            1,
            CompanyCommand::SetDriverRunning {
                driver_id: f.driver,
                running,
            },
            &f.catalog,
        )
        .unwrap();
        *recoveries += 1;
    }
    assert_eq!(
        money(&f.world),
        conserved_money,
        "career cash including escrow and earned household wallets"
    );
    assert_eq!(
        goods(&f.world, "water"),
        conserved_water,
        "career water including actual household consumption"
    );
    assert_eq!(
        goods(&f.world, "bread"),
        conserved_bread,
        "career bread including actual household consumption"
    );
}

fn career_until(
    f: &mut Fixture,
    totals: (u64, u64, u64),
    recoveries: &mut u32,
    test: impl Fn(&SandboxState) -> bool,
) {
    for _ in 0..2400 {
        if test(&f.world) {
            return;
        }
        career_step(f, totals.0, totals.1, totals.2, recoveries);
    }
    panic!(
        "career stopped: {:?}",
        f.world.transport.companies[&1].drivers[0]
    );
}

#[test]
fn six_session_warehouse_career_conserves_finite_goods_cash_and_recovers_scarcity() {
    measured_six_session_warehouse_career(false);
}

#[test]
fn purchased_stock_career_accounts_for_acquisition_setup_and_recovery_without_inventing_profit() {
    measured_six_session_warehouse_career(true);
}

fn measured_six_session_warehouse_career(purchased_stock: bool) {
    let mut f = fixture();
    let (garage, depot, terminal) = (f.garage, f.depot, f.terminal);
    f.world
        .set_player_position(1, f.world.transport.facilities[&depot].position)
        .unwrap();
    command(
        &mut f.world,
        1,
        CompanyCommand::OpenDepot { facility_id: depot },
        &f.catalog,
    )
    .unwrap();
    // Both scenarios start with the same finite distribution stock. One owner
    // contributes an authored harvest with unknown cost; the other purchases
    // that stock through the real company trade boundary at current retail.
    // Everything below the starting-budget capture is funded and physical.
    f.world
        .set_player_position(1, f.world.transport.facilities[&garage].position)
        .unwrap();
    let acquisition_cost = if purchased_stock {
        let destination = f.world.living.shops[&f.shop].position;
        f.world.living.shops.get_mut(&f.shop).unwrap().position =
            f.world.transport.facilities[&garage].position;
        f.world
            .living
            .shops
            .get_mut(&f.shop)
            .unwrap()
            .stock
            .insert("water".into(), 96);
        let quote = f
            .world
            .quote_company_depot_trade(1, garage, f.shop, "water", 96, true)
            .unwrap();
        let action = trade_action(quote.clone());
        command(&mut f.world, 1, action, &f.catalog).unwrap();
        f.world.living.shops.get_mut(&f.shop).unwrap().position = destination;
        quote.total_cost
    } else {
        let stock = f
            .world
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .entry("water".into())
            .or_default();
        *stock += 96;
        command(
            &mut f.world,
            1,
            CompanyCommand::TransferDepotStock {
                facility_id: garage,
                item: "water".into(),
                amount: 96,
                deposit: true,
            },
            &f.catalog,
        )
        .unwrap();
        0
    };
    f.world
        .set_player_position(1, f.world.transport.facilities[&depot].position)
        .unwrap();
    let cash = f.world.living.shops[&f.shop].money;
    f.world.living.accounts.get_mut(&2).unwrap().money += cash;
    f.world.living.shops.get_mut(&f.shop).unwrap().money = 0;
    f.world
        .living
        .shops
        .get_mut(&f.shop)
        .unwrap()
        .stock
        .insert("water".into(), 8);
    f.world
        .transport
        .vehicles
        .get_mut(&f.vehicle)
        .unwrap()
        .fuel_l = 0.04;
    let transfer = route(
        &mut f,
        "Harvest distribution",
        garage,
        depot,
        FleetService::DepotTransfer {
            item: "water".into(),
            amount: 8,
        },
    );
    let sale = route(
        &mut f,
        "Retail supply",
        depot,
        terminal,
        FleetService::DepotFreight {
            item: "water".into(),
            amount: 8,
        },
    );
    let totals = (
        money(&f.world),
        goods(&f.world, "water"),
        goods(&f.world, "bread"),
    );
    let initial_issuer = f.world.transport.treasury;
    let initial_company = f.world.transport.companies[&1].cash;
    let initial_accounting = f.world.transport.companies[&1].accounting_report();
    assert!(initial_accounting.reconciled);
    let initial_shop = f.world.living.shops[&f.shop].money;
    let initial_station_fuel = f.world.transport.facilities[&garage].fuel_l;
    let mut recoveries = 0;
    let mut restarted = 0;
    let mut buyer_purchases = 0;
    let mut additional_sales = 0;
    let mut returned = 0;
    let mut owner_withdrawals = 0;
    let mut owner_deposits = 0;
    for session in 0..6 {
        for batch in 0..2 {
            assign(&mut f, transfer);
            career_until(&mut f, totals, &mut recoveries, |w| {
                w.transport.companies[&1].drivers[0].trip.is_some()
            });
            command(
                &mut f.world,
                1,
                CompanyCommand::SetDriverRunning {
                    driver_id: f.driver,
                    running: false,
                },
                &f.catalog,
            )
            .unwrap();
            if session == 2 && batch == 0 {
                let cash = f.world.transport.companies[&1].cash;
                command(
                    &mut f.world,
                    1,
                    CompanyCommand::Withdraw { amount: cash },
                    &f.catalog,
                )
                .unwrap();
                owner_withdrawals += cash;
                career_step(&mut f, totals.0, totals.1, totals.2, &mut recoveries);
                assert_eq!(
                    f.world.transport.companies[&1].drivers[0].status,
                    FleetStatus::WaitingCompanyFunds
                );
                assert!(f.world.transport.companies[&1].drivers[0].trip.is_some());
                f.world = serde_json::from_slice(&serde_json::to_vec(&f.world).unwrap()).unwrap();
                f.world.validate_with_catalog(&f.catalog).unwrap();
                command(
                    &mut f.world,
                    1,
                    CompanyCommand::Deposit { amount: 500 },
                    &f.catalog,
                )
                .unwrap();
                owner_deposits += 500;
                restarted += 1;
            }
            if session == 3 && batch == 1 {
                for _ in 0..8 {
                    career_step(&mut f, totals.0, totals.1, totals.2, &mut recoveries);
                }
                command(
                    &mut f.world,
                    1,
                    CompanyCommand::ReturnLoad {
                        driver_id: f.driver,
                    },
                    &f.catalog,
                )
                .unwrap();
                career_until(&mut f, totals, &mut recoveries, |w| {
                    w.transport.companies[&1].drivers[0].trip.is_none()
                });
                assert_eq!(f.world.company_depot_reserved_units(1, depot), 0);
                command(
                    &mut f.world,
                    1,
                    CompanyCommand::SetDriverRunning {
                        driver_id: f.driver,
                        running: true,
                    },
                    &f.catalog,
                )
                .unwrap();
                career_until(&mut f, totals, &mut recoveries, |w| {
                    w.transport.companies[&1].drivers[0].trip.is_some()
                });
                command(
                    &mut f.world,
                    1,
                    CompanyCommand::SetDriverRunning {
                        driver_id: f.driver,
                        running: false,
                    },
                    &f.catalog,
                )
                .unwrap();
                returned += 1;
            }
            career_until(&mut f, totals, &mut recoveries, |w| {
                w.transport.companies[&1].drivers[0].trip.is_none()
            });
            assign(&mut f, sale);
            f.world.advance_transport(0.1).unwrap();
            if session == 0 && batch == 0 {
                assert_eq!(
                    f.world.transport.companies[&1].drivers[0].status,
                    FleetStatus::WaitingBuyerFunds
                );
            }
            // A finite customer actually buys the previous delivery. Those
            // credits, rather than a subsidy, fund the next purchase contract.
            f.world
                .set_player_position(2, f.world.living.shops[&f.shop].position)
                .unwrap();
            let amount = f.world.living.shops[&f.shop]
                .stock
                .get("water")
                .copied()
                .unwrap_or(0)
                .min(8);
            assert!(amount > 0, "stock must exist for the customer purchase");
            f.world
                .apply(
                    2,
                    Command::Living {
                        action: LivingCommand::Buy {
                            shop_id: f.shop,
                            item: "water".into(),
                            amount,
                        },
                    },
                    &f.catalog,
                )
                .unwrap();
            buyer_purchases += amount;
            career_until(&mut f, totals, &mut recoveries, |w| {
                w.transport.companies[&1].drivers[0].trip.is_some()
            });
            command(
                &mut f.world,
                1,
                CompanyCommand::SetDriverRunning {
                    driver_id: f.driver,
                    running: false,
                },
                &f.catalog,
            )
            .unwrap();
            if batch == 1 {
                let mut company_before = f.world.transport.companies[&1].clone();
                let before_elapsed = f.world.transport.elapsed_seconds;
                f.world.set_player_dormant(1, true).unwrap();
                for _ in 0..24 {
                    f.world.advance_living(5., 0., &f.catalog).unwrap();
                }
                company_before.drivers[0]
                    .trip
                    .as_mut()
                    .unwrap()
                    .pickup_elapsed_seconds += f.world.transport.elapsed_seconds - before_elapsed;
                assert_eq!(
                    f.world.transport.companies[&1], company_before,
                    "no offline payroll or sales"
                );
                let partition = RegionPartition::from_state(&f.world, 15, &f.catalog).unwrap();
                f.world = partition.assemble_all(&f.catalog).unwrap();
                f.world.set_player_dormant(1, false).unwrap();
                restarted += 1;
            }
            career_until(&mut f, totals, &mut recoveries, |w| {
                w.transport.companies[&1].drivers[0].trip.is_none()
            });
            // Finite buyer demand/funds may authorize a partial load. Finish
            // the remaining stock through real purchases and physical trips;
            // controller timing must not require subsidizing the buyer.
            for _ in 0..8 {
                if depot_stock(&f.world, depot)
                    .get("water")
                    .copied()
                    .unwrap_or(0)
                    == 0
                {
                    break;
                }
                let amount = f.world.living.shops[&f.shop]
                    .stock
                    .get("water")
                    .copied()
                    .unwrap_or(0)
                    .min(8);
                assert!(
                    amount > 0,
                    "a customer needs actual shelf stock to fund the next partial sale"
                );
                f.world
                    .set_player_position(2, f.world.living.shops[&f.shop].position)
                    .unwrap();
                f.world
                    .apply(
                        2,
                        Command::Living {
                            action: LivingCommand::Buy {
                                shop_id: f.shop,
                                item: "water".into(),
                                amount,
                            },
                        },
                        &f.catalog,
                    )
                    .unwrap();
                buyer_purchases += amount;
                command(
                    &mut f.world,
                    1,
                    CompanyCommand::SetDriverRunning {
                        driver_id: f.driver,
                        running: true,
                    },
                    &f.catalog,
                )
                .unwrap();
                career_until(&mut f, totals, &mut recoveries, |w| {
                    w.transport.companies[&1].drivers[0].trip.is_some()
                });
                command(
                    &mut f.world,
                    1,
                    CompanyCommand::SetDriverRunning {
                        driver_id: f.driver,
                        running: false,
                    },
                    &f.catalog,
                )
                .unwrap();
                career_until(&mut f, totals, &mut recoveries, |w| {
                    w.transport.companies[&1].drivers[0].trip.is_none()
                });
                additional_sales += 1;
            }
            assert_eq!(
                depot_stock(&f.world, depot)
                    .get("water")
                    .copied()
                    .unwrap_or(0),
                0
            );
            assert_eq!(money(&f.world), totals.0);
            assert_eq!(goods(&f.world, "water"), totals.1);
            f.world.validate_with_catalog(&f.catalog).unwrap();
        }
        command(
            &mut f.world,
            1,
            CompanyCommand::ReleaseVehicle {
                driver_id: f.driver,
            },
            &f.catalog,
        )
        .unwrap();
        // Off-duty consumption spends already-earned wages on real shelf stock.
        for _ in 0..48 {
            f.world.advance_transport(5.).unwrap();
        }
        assert_eq!(money(&f.world), totals.0);
        assert_eq!(goods(&f.world, "water"), totals.1);
        assert_eq!(goods(&f.world, "bread"), totals.2);
    }
    assign(&mut f, transfer);
    career_until(&mut f, totals, &mut recoveries, |w| {
        w.transport.companies[&1].drivers[0].status == FleetStatus::WaitingGoods
    });
    let revenue = f.world.transport.companies[&1].revenue;
    let cash = f.world.transport.companies[&1].cash;
    for _ in 0..120 {
        career_step(&mut f, totals.0, totals.1, totals.2, &mut recoveries);
    }
    assert_eq!(f.world.transport.companies[&1].revenue, revenue);
    assert_eq!(
        f.world.transport.companies[&1].cash, cash,
        "waiting at an empty depot accrues no phantom wages"
    );
    assert_eq!(
        f.world.transport.companies[&1].drivers[0].deliveries_completed,
        24 + additional_sales
    );
    assert_eq!(revenue, 96 * 30);
    assert_eq!(restarted, 7);
    assert_eq!(returned, 1);
    assert!(recoveries > 0);
    assert!(
        f.world.transport.companies[&1]
            .household_consumed
            .values()
            .sum::<u32>()
            > 0
    );
    assert!(f.world.transport.facilities[&garage].fuel_l < initial_station_fuel);
    assert!(f.world.transport.treasury > initial_issuer);
    f.world.validate_with_catalog(&f.catalog).unwrap();
    let accounting = f.world.transport.companies[&1].accounting_report();
    assert!(accounting.reconciled);
    assert_eq!(
        accounting.coverage,
        CompanyAccountingCoverage::SinceRegistration
    );
    assert_eq!(accounting.totals[&CompanyCashCategory::GoodsSales], revenue);
    assert_eq!(
        accounting.totals[&CompanyCashCategory::Payroll],
        f.world.transport.companies[&1].payroll_paid()
    );
    assert_eq!(
        accounting.totals[&CompanyCashCategory::Recruitment],
        COMPANY_HIRE_PRICE
    );
    assert_eq!(
        accounting.totals[&CompanyCashCategory::DepotAccess],
        2 * COMPANY_DEPOT_PRICE
    );
    assert_eq!(accounting.owner_paid_capital, COMPANY_REGISTRATION_PRICE);
    assert_eq!(
        accounting
            .totals
            .get(&CompanyCashCategory::InventoryPurchases)
            .copied()
            .unwrap_or(0),
        acquisition_cost
    );
    assert_eq!(
        accounting.unpriced_stock_in_units,
        if purchased_stock { 0 } else { 96 }
    );
    assert!(accounting.personal_vehicle_used);
    assert_eq!(accounting.financing.receipts, 20_000 + owner_deposits);
    assert_eq!(accounting.financing.payments, owner_withdrawals);
    assert_eq!(
        accounting.operating.net() - initial_accounting.operating.net(),
        cash as i64 + owner_withdrawals as i64 - initial_company as i64 - owner_deposits as i64
    );
    assert_eq!(
        accounting.cash_result_after_known_owner_costs,
        revenue as i64
            - f.world.transport.companies[&1].payroll_paid() as i64
            - accounting.totals[&CompanyCashCategory::Fuel] as i64
            - accounting.totals[&CompanyCashCategory::ServiceCallout] as i64
            - COMPANY_HIRE_PRICE as i64
            - (2 * COMPANY_DEPOT_PRICE) as i64
            - COMPANY_REGISTRATION_PRICE as i64
            - acquisition_cost as i64
    );
    assert!(accounting.recent_entries.len() <= MAX_COMPANY_RECENT_ENTRIES);
    let restored = RegionPartition::from_state(&f.world, 15, &f.catalog)
        .unwrap()
        .assemble_all(&f.catalog)
        .unwrap();
    assert_eq!(
        restored.transport.companies[&1].accounting_report(),
        accounting
    );
    println!(
        "{} {}",
        if purchased_stock {
            "PURCHASED_CAREER_REPORT"
        } else {
            "CAREER_REPORT"
        },
        serde_json::json!({
            "sessions": 6, "physical_seconds": f.world.transport.elapsed_seconds,
            "deliveries": 24 + additional_sales, "warehouse_sales": 12 + additional_sales, "transfers": 12,
            "source_water_units": 96, "customer_water_purchases": buyer_purchases,
            "save_resume_checkpoints": restarted, "cancelled_and_retried_transfer": returned,
            "fuel_recoveries": recoveries, "company_initial_minor_credits": initial_company,
            "company_final_minor_credits": cash, "goods_revenue_minor_credits": revenue,
            "owner_withdrawals_minor_credits": owner_withdrawals,
            "additional_owner_deposits_minor_credits": owner_deposits,
            "operating_cash_change_excluding_owner_transfers_minor_credits": cash as i64 + owner_withdrawals as i64 - initial_company as i64 - owner_deposits as i64,
            "payroll_minor_credits": f.world.transport.companies[&1].payroll_paid(),
            "shop_initial_minor_credits": initial_shop, "shop_final_minor_credits": f.world.living.shops[&f.shop].money,
            "household_consumed": f.world.transport.companies[&1].household_consumed,
            "cash_conserved_minor_credits": totals.0, "water_conserved_including_consumption": totals.1,
            "bread_conserved_including_consumption": totals.2,
            "final_status": f.world.transport.companies[&1].drivers[0].status,
            "source_acquisition": if purchased_stock { "purchased_at_retail" } else { "owner_contributed_unpriced_harvest" },
            "source_acquisition_minor_credits": purchased_stock.then_some(acquisition_cost),
            "persisted_accounting": accounting,
        })
    );
}
