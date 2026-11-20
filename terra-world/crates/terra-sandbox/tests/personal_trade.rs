use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

fn world() -> (SandboxState, Catalog, u64) {
    let catalog = Catalog::builtin();
    let mut state = SandboxState::default();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    state.add_player(1, origin).unwrap();
    state.seed_living_demo(origin, &[], &catalog).unwrap();
    let shop = *state.living.shops.keys().next().unwrap();
    state
        .set_player_position(1, state.living.shops[&shop].position)
        .unwrap();
    state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("bread".into(), 10);
    (state, catalog, shop)
}

fn quote(state: &SandboxState, shop_id: u64, amount: u32, buy: bool) -> LivingCommand {
    let price = state.living.shops[&shop_id].prices["bread"];
    LivingCommand::TradeQuoted {
        shop_id,
        item: "bread".into(),
        amount,
        buy,
        quoted_total: (if buy { price.buy } else { price.sell }) * u64::from(amount),
    }
}

fn apply(
    state: &mut SandboxState,
    catalog: &Catalog,
    action: LivingCommand,
) -> Result<Outcome, SandboxError> {
    state.apply(1, Command::Living { action }, catalog)
}

#[test]
fn personal_trade_rejects_changed_buy_and_sell_quotes_then_accepts_reviewed_total() {
    for buy in [true, false] {
        let (mut state, catalog, shop_id) = world();
        let before_quote = state.clone();
        let visible = state
            .personal_trade_quote(1, shop_id, "bread", 2, buy, &catalog)
            .unwrap();
        assert_eq!(state, before_quote);
        assert_eq!(visible.shop_id, shop_id);
        assert_eq!(visible.item, "bread");
        assert_eq!(visible.amount, 2);
        assert_eq!(visible.buy, buy);
        let stale = quote(&state, shop_id, 2, buy);
        let price = state
            .living
            .shops
            .get_mut(&shop_id)
            .unwrap()
            .prices
            .get_mut("bread")
            .unwrap();
        if buy {
            price.buy += 10;
        } else {
            price.sell -= 1;
        }
        let before = state.clone();
        let error = apply(&mut state, &catalog, stale).unwrap_err();
        assert!(error.to_string().contains("quote changed"));
        assert_eq!(state, before, "stale quote changed stock, cash or metrics");

        let reviewed = quote(&state, shop_id, 2, buy);
        let total_cash = state.living.accounts[&1].money + state.living.shops[&shop_id].money;
        let total_goods =
            state.players[&1].inventory["bread"] + state.living.shops[&shop_id].stock["bread"];
        apply(&mut state, &catalog, reviewed).unwrap();
        assert_eq!(
            state.living.accounts[&1].money + state.living.shops[&shop_id].money,
            total_cash
        );
        assert_eq!(
            state.players[&1].inventory["bread"] + state.living.shops[&shop_id].stock["bread"],
            total_goods
        );
        assert_eq!(
            state.players[&1].inventory["bread"],
            if buy { 12 } else { 8 }
        );
        assert_eq!(
            state.living.metrics.units_traded,
            before.living.metrics.units_traded + 2
        );
        state.validate_with_catalog(&catalog).unwrap();
    }
}

#[test]
fn personal_quote_does_not_bypass_stock_payer_funds_or_receiving_caps() {
    for buy in [true, false] {
        for failure in 0..4 {
            let (mut state, catalog, shop) = world();
            match (buy, failure) {
                (true, 0) => {
                    state
                        .living
                        .shops
                        .get_mut(&shop)
                        .unwrap()
                        .stock
                        .remove("bread");
                }
                (false, 0) => {
                    state.players.get_mut(&1).unwrap().inventory.remove("bread");
                }
                (true, 1) => state.living.accounts.get_mut(&1).unwrap().money = 0,
                (false, 1) => state.living.shops.get_mut(&shop).unwrap().money = 0,
                (true, 2) => state.living.shops.get_mut(&shop).unwrap().money = MAX_MONEY,
                (false, 2) => state.living.accounts.get_mut(&1).unwrap().money = MAX_MONEY,
                (true, 3) => {
                    state
                        .players
                        .get_mut(&1)
                        .unwrap()
                        .inventory
                        .insert("bread".into(), MAX_ITEM_COUNT);
                }
                (false, 3) => {
                    state
                        .living
                        .shops
                        .get_mut(&shop)
                        .unwrap()
                        .stock
                        .insert("bread".into(), MAX_ITEM_COUNT);
                }
                _ => unreachable!(),
            }
            let action = quote(&state, shop, 1, buy);
            let before = state.clone();
            assert!(
                state
                    .personal_trade_quote(1, shop, "bread", 1, buy, &catalog)
                    .is_err()
            );
            assert_eq!(state, before, "preflight changed a blocked exchange");
            assert!(apply(&mut state, &catalog, action).is_err());
            assert_eq!(state, before, "buy={buy}, failure={failure}");
        }
    }
}

#[test]
fn quoted_personal_trade_keeps_three_dimensional_reach_and_checked_bounds() {
    let (mut state, catalog, shop) = world();
    let action = quote(&state, shop, 1, true);
    let above = LocalFrame::new(state.players[&1].position)
        .to_geo([0., 30., 0.])
        .unwrap();
    state.set_player_position(1, above).unwrap();
    let before = state.clone();
    assert!(matches!(
        apply(&mut state, &catalog, action),
        Err(SandboxError::OutOfReach)
    ));
    assert_eq!(state, before);

    state
        .set_player_position(1, state.living.shops[&shop].position)
        .unwrap();
    for buy in [true, false] {
        for (amount, quoted_total, price) in [
            (0, 0, 100),
            (101, 10_100, 100),
            (1, u64::MAX, 100),
            // Keep the supplied total valid so the real quote multiplication
            // must reject overflow rather than the expectation's own bound.
            (2, 100, u64::MAX),
        ] {
            let prices = state
                .living
                .shops
                .get_mut(&shop)
                .unwrap()
                .prices
                .get_mut("bread")
                .unwrap();
            if buy {
                prices.buy = price;
            } else {
                prices.sell = price;
            }
            let before = state.clone();
            if quoted_total != u64::MAX {
                assert!(
                    state
                        .personal_trade_quote(1, shop, "bread", amount, buy, &catalog)
                        .is_err()
                );
                assert_eq!(state, before);
            }
            assert!(
                apply(
                    &mut state,
                    &catalog,
                    LivingCommand::TradeQuoted {
                        shop_id: shop,
                        item: "bread".into(),
                        amount,
                        buy,
                        quoted_total,
                    }
                )
                .is_err()
            );
            assert_eq!(state, before);
        }
    }
}

#[test]
fn quoted_trade_wire_shape_is_explicit_and_legacy_commands_remain_available() {
    let (mut state, catalog, shop) = world();
    let quoted = quote(&state, shop, 1, true);
    let mut wire = serde_json::to_value(&quoted).unwrap();
    assert_eq!(wire["type"], "trade_quoted");
    let restored: LivingCommand = serde_json::from_value(wire.clone()).unwrap();
    apply(&mut state, &catalog, restored).unwrap();
    wire.as_object_mut().unwrap().remove("quoted_total");
    assert!(serde_json::from_value::<LivingCommand>(wire).is_err());
    let legacy = serde_json::json!({"type":"buy","shop_id":shop,"item":"bread","amount":1});
    apply(
        &mut state,
        &catalog,
        serde_json::from_value(legacy).unwrap(),
    )
    .unwrap();
    let legacy = serde_json::json!({"type":"sell","shop_id":shop,"item":"bread","amount":1});
    apply(
        &mut state,
        &catalog,
        serde_json::from_value(legacy).unwrap(),
    )
    .unwrap();
    state.validate_with_catalog(&catalog).unwrap();
}
