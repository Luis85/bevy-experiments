use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

fn command(state: &mut SandboxState, catalog: &Catalog, action: CompanyCommand) {
    state
        .apply(
            1,
            Command::Transport {
                action: TransportCommand::Company { action },
            },
            catalog,
        )
        .unwrap();
}

fn fixture(departed: bool) -> (SandboxState, Catalog, u64, u64) {
    let catalog = Catalog::builtin();
    let mut state = SandboxState::default();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    state.add_player(1, origin).unwrap();
    state.seed_living_demo(origin, &[], &catalog).unwrap();
    state.ensure_transport_demo(&catalog).unwrap();
    let garage = state
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Garage)
        .unwrap()
        .clone();
    state.set_player_position(1, garage.position).unwrap();
    command(
        &mut state,
        &catalog,
        CompanyCommand::Create {
            name: "Local shoppers".into(),
        },
    );
    command(
        &mut state,
        &catalog,
        CompanyCommand::Deposit { amount: 10_000 },
    );
    command(
        &mut state,
        &catalog,
        CompanyCommand::HireDriver {
            name: "Robin".into(),
        },
    );
    command(
        &mut state,
        &catalog,
        CompanyCommand::OpenDepot {
            facility_id: garage.id,
        },
    );
    let elapsed_seconds = state.transport.elapsed_seconds;
    let company = state.transport.companies.get_mut(&1).unwrap();
    company.cash -= 500;
    if departed {
        company.departed_wages = 500;
    } else {
        company.drivers[0].wages_earned = 500;
    }
    // This fixture authors a known five-credit payroll payment. Keep its
    // since-registration cash history exact instead of discarding the journal.
    let accounting = company.accounting.as_mut().unwrap();
    *accounting
        .totals
        .entry(CompanyCashCategory::Payroll)
        .or_default() += 500;
    accounting.monetary_entry_count += 1;
    accounting.recent_entries.push(CompanyCashEntry {
        sequence: accounting.monetary_entry_count,
        elapsed_seconds,
        category: CompanyCashCategory::Payroll,
        amount: 500,
        cash_after: company.cash,
    });
    let frame = LocalFrame::new(garage.position);
    for shop in state.living.shops.values_mut() {
        shop.position = frame.to_geo([1000., 0., 0.]).unwrap();
    }
    let near = *state.living.shops.keys().next().unwrap();
    let shop = state.living.shops.get_mut(&near).unwrap();
    shop.position = frame.to_geo([10., 0., 0.]).unwrap();
    shop.stock.insert("water".into(), 3);
    shop.prices
        .insert("water".into(), TradePrice { buy: 100, sell: 50 });
    let mut farther = shop.clone();
    farther.id = state.living.next_id;
    state.living.next_id += 1;
    farther.name = "Affordable neighbor".into();
    farther.position = frame.to_geo([30., 0., 0.]).unwrap();
    let far = farther.id;
    state.living.shops.insert(far, farther);
    state.validate_with_catalog(&catalog).unwrap();
    (state, catalog, near, far)
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
        + state.transport.treasury
        + state
            .transport
            .companies
            .values()
            .map(|c| {
                c.cash + c.departed_wages + c.drivers.iter().map(|d| d.wages_earned).sum::<u64>()
            })
            .sum::<u64>()
}

#[test]
fn households_skip_unaffordable_or_capped_shops_without_losing_money_or_material() {
    for departed in [false, true] {
        for capped in [false, true] {
            let (mut state, catalog, near, far) = fixture(departed);
            if capped {
                state.living.shops.get_mut(&near).unwrap().money = MAX_MONEY;
            } else {
                state
                    .living
                    .shops
                    .get_mut(&near)
                    .unwrap()
                    .prices
                    .get_mut("water")
                    .unwrap()
                    .buy = 1000;
            }
            let before = state.clone();
            let total_money = money(&state);
            let paid = state.transport.companies[&1].payroll_paid();
            for _ in 0..12 {
                state.advance_transport(5.).unwrap();
            }
            let company = &state.transport.companies[&1];
            assert_eq!(company.household_consumed.get("water"), Some(&1));
            assert_eq!(state.living.shops[&near], before.living.shops[&near]);
            assert_eq!(
                state.living.shops[&far].stock["water"],
                before.living.shops[&far].stock["water"] - 1
            );
            assert_eq!(
                state.living.shops[&far].money,
                before.living.shops[&far].money + 100
            );
            assert_eq!(state.transport.treasury, before.transport.treasury + 50);
            assert_eq!(money(&state), total_money);
            assert_eq!(company.payroll_paid(), paid);
            // Spending earned wages does not pay or record company payroll again.
            assert_eq!(
                company.accounting,
                before.transport.companies[&1].accounting
            );
            if departed {
                assert_eq!(company.departed_wages, 350);
                assert_eq!(company.departed_spending, 150);
            } else {
                assert_eq!(company.drivers[0].wages_earned, 350);
                assert_eq!(company.drivers[0].wages_spent, 150);
            }
            let restored: SandboxState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(restored, state);
            restored.validate_with_catalog(&catalog).unwrap();
        }
    }
}

#[test]
fn household_fallback_still_requires_a_local_stocked_shop_and_service_capacity() {
    for blocked_treasury in [false, true] {
        let (mut state, catalog, near, far) = fixture(false);
        state.living.shops.get_mut(&near).unwrap().money = MAX_MONEY;
        if blocked_treasury {
            state.transport.treasury = MAX_MONEY;
        } else {
            state
                .living
                .shops
                .get_mut(&far)
                .unwrap()
                .stock
                .remove("water");
        }
        let before = state.clone();
        for _ in 0..12 {
            state.advance_transport(5.).unwrap();
        }
        assert_eq!(
            state.transport.companies[&1],
            before.transport.companies[&1]
        );
        assert_eq!(state.living.shops, before.living.shops);
        assert_eq!(money(&state), money(&before));
        state.validate_with_catalog(&catalog).unwrap();
    }
}
