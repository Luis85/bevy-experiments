//! The market retains the seller and price that were actually presented.
//! Authority still checks the quote, stock, reach and accounts at acceptance.
use crate::interface::{Panel, TRADE_ITEMS, UiState};
use terra_bevy::{SandboxRuntime, rules::*};

#[derive(Default)]
pub struct MarketUi {
    pub selected_shop: Option<u64>,
    displayed: Option<DisplayedMarket>,
}

#[derive(Clone, PartialEq, Eq)]
struct DisplayedMarket {
    player_id: u64,
    restore_revision: u64,
    shop_id: u64,
    amount: u32,
    offers: Vec<Option<DisplayedOffer>>,
}

#[derive(Clone, PartialEq, Eq)]
struct DisplayedOffer {
    buy_total: Option<u64>,
    sell_total: Option<u64>,
    stock: u32,
    carried: u32,
    buy_problem: Option<String>,
    sell_problem: Option<String>,
}

pub fn shop<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a Shop> {
    match ui.market.selected_shop {
        Some(id) => s.state.living.shops.get(&id),
        None => s
            .state
            .living
            .active_unit(s.player_id)
            .and_then(|body| s.state.living.nearest_shop(body.position)),
    }
}

pub fn choose_next(s: &SandboxRuntime, ui: &mut UiState) {
    let Some(position) = s.state.players.get(&s.player_id).map(|p| p.position) else {
        return;
    };
    let current = shop(s, ui).map(|shop| shop.id);
    let mut shops: Vec<_> = s.state.living.shops.values().collect();
    shops.sort_by(|a, b| {
        a.position
            .surface_distance_m(position)
            .total_cmp(&b.position.surface_distance_m(position))
            .then(a.id.cmp(&b.id))
    });
    let next = shops
        .iter()
        .position(|shop| Some(shop.id) == current)
        .map_or(0, |index| (index + 1) % shops.len().max(1));
    ui.market.selected_shop = shops.get(next).map(|shop| shop.id);
    ui.market.displayed = None;
}

fn capture(s: &SandboxRuntime, ui: &UiState) -> Option<DisplayedMarket> {
    let player = s.state.players.get(&s.player_id)?;
    let shop = shop(s, ui)?;
    let amount = ui.trade_quantity;
    let total = |price: u64| {
        price
            .checked_mul(u64::from(amount))
            .filter(|total| *total <= MAX_MONEY)
    };
    let offers = TRADE_ITEMS
        .iter()
        .map(|(item, _)| {
            let price = shop.prices.get(*item)?;
            let problem = |buy| {
                s.state
                    .personal_trade_quote(s.player_id, shop.id, item, amount, buy, &s.catalog)
                    .err()
                    .map(|error| error.to_string())
            };
            Some(DisplayedOffer {
                buy_total: total(price.buy),
                sell_total: total(price.sell),
                stock: shop.stock.get(*item).copied().unwrap_or(0),
                carried: player.inventory.get(*item).copied().unwrap_or(0),
                buy_problem: problem(true),
                sell_problem: problem(false),
            })
        })
        .collect();
    Some(DisplayedMarket {
        player_id: s.player_id,
        restore_revision: s.restore_revision,
        shop_id: shop.id,
        amount,
        offers,
    })
}

/// Called in the same system and cadence as the labels. Updating a quote at
/// input time would silently replace the price on the screen.
pub fn refresh_displayed(s: &SandboxRuntime, ui: &mut UiState) {
    ui.market.displayed = if ui.context_panel && ui.panel == Panel::Market {
        capture(s, ui)
    } else {
        None
    };
}

fn money(amount: u64) -> String {
    format!("{}.{:02}", amount / 100, amount % 100)
}

pub fn offer_label(ui: &UiState, index: usize) -> String {
    let Some((market, offer)) = ui.market.displayed.as_ref().and_then(|market| {
        market
            .offers
            .get(index)
            .and_then(Option::as_ref)
            .map(|offer| (market, offer))
    }) else {
        return "No price at this market.".into();
    };
    let quote = |side: &str, total: Option<u64>, problem: &Option<String>| {
        let Some(total) = total else {
            return format!("{side}: amount exceeds the account limit");
        };
        let mut text = format!("{side} {} for {} cr", market.amount, money(total));
        if let Some(problem) = problem {
            text.push_str(&format!(" · {problem}"));
        }
        text
    };
    format!(
        "{}\n{}\nShop {} · you {}",
        quote("Buy", offer.buy_total, &offer.buy_problem),
        quote("Sell", offer.sell_total, &offer.sell_problem),
        offer.stock,
        offer.carried,
    )
}

pub fn trade_command(
    s: &SandboxRuntime,
    ui: &UiState,
    index: usize,
    buy: bool,
) -> Result<Command, String> {
    let displayed = ui
        .market
        .displayed
        .as_ref()
        .ok_or("Open Market and review a price before trading.")?;
    if !ui.context_panel
        || ui.panel != Panel::Market
        || displayed.player_id != s.player_id
        || displayed.restore_revision != s.restore_revision
        || displayed.amount != ui.trade_quantity
        || shop(s, ui).map(|shop| shop.id) != Some(displayed.shop_id)
    {
        return Err("Market or quantity changed. Review the updated quote and try again.".into());
    }
    let item = TRADE_ITEMS
        .get(index)
        .ok_or("Choose a listed market item.")?
        .0;
    let offer = displayed
        .offers
        .get(index)
        .and_then(Option::as_ref)
        .ok_or("This market has no price for that item.")?;
    let quoted_total = if buy {
        offer.buy_total
    } else {
        offer.sell_total
    }
    .ok_or("This amount exceeds the account limit. Choose a smaller quantity.")?;
    let current = s
        .state
        .personal_trade_quote(
            s.player_id,
            displayed.shop_id,
            item,
            displayed.amount,
            buy,
            &s.catalog,
        )
        .map_err(|error| error.to_string())?;
    if current.total_cost != quoted_total {
        return Err("Market price changed. Review the updated quote and try again.".into());
    }
    Ok(Command::Living {
        action: LivingCommand::TradeQuoted {
            shop_id: displayed.shop_id,
            item: item.into(),
            amount: displayed.amount,
            buy,
            quoted_total,
        },
    })
}

pub fn unavailable(s: &SandboxRuntime, ui: &UiState, index: usize, buy: bool) -> bool {
    trade_command(s, ui, index, buy).is_err()
}

#[cfg(test)]
mod tests {
    use super::*;
    use terra_core::{GeoPosition, LocalFrame};

    fn fixture() -> (SandboxRuntime, UiState, u64) {
        let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let mut s = SandboxRuntime::new(origin);
        s.state.seed_living_demo(origin, &[], &s.catalog).unwrap();
        let id = *s.state.living.shops.keys().next().unwrap();
        s.state
            .set_player_position(1, s.state.living.shops[&id].position)
            .unwrap();
        s.state
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .insert("bread".into(), 20);
        let mut ui = UiState {
            panel: Panel::Market,
            trade_quantity: 5,
            ..Default::default()
        };
        refresh_displayed(&s, &mut ui);
        (s, ui, id)
    }

    #[test]
    fn displayed_total_is_the_personal_trade_authority_expectation() {
        for buy in [true, false] {
            let (mut s, ui, shop) = fixture();
            let price = s.state.living.shops[&shop].prices["bread"];
            let total = (if buy { price.buy } else { price.sell }) * 5;
            assert!(offer_label(&ui, 0).contains(&format!("for {} cr", money(total))));
            let command = trade_command(&s, &ui, 0, buy).unwrap();
            assert!(
                matches!(&command, Command::Living { action: LivingCommand::TradeQuoted { quoted_total, amount: 5, .. } } if *quoted_total == total)
            );
            let before = s.state.living.accounts[&1].money;
            s.state.apply(1, command, &s.catalog).unwrap();
            assert_eq!(
                s.state.living.accounts[&1].money,
                if buy { before - total } else { before + total }
            );
        }
    }

    #[test]
    fn price_changed_after_render_requires_review_on_both_sides() {
        for buy in [true, false] {
            let (mut s, mut ui, shop) = fixture();
            let price = s
                .state
                .living
                .shops
                .get_mut(&shop)
                .unwrap()
                .prices
                .get_mut("bread")
                .unwrap();
            if buy {
                price.buy += 10;
            } else {
                price.sell -= 1;
            }
            let before = s.state.clone();
            assert!(
                trade_command(&s, &ui, 0, buy)
                    .unwrap_err()
                    .contains("price changed")
            );
            assert_eq!(s.state, before);
            refresh_displayed(&s, &mut ui);
            assert!(trade_command(&s, &ui, 0, buy).is_ok());
        }
    }

    #[test]
    fn authority_rejects_a_price_change_after_ui_enqueue() {
        let (mut s, ui, shop) = fixture();
        let command = trade_command(&s, &ui, 0, true).unwrap();
        s.state
            .living
            .shops
            .get_mut(&shop)
            .unwrap()
            .prices
            .get_mut("bread")
            .unwrap()
            .buy += 1;
        let before = s.state.clone();
        assert!(s.state.apply(1, command, &s.catalog).is_err());
        assert_eq!(s.state, before);
    }

    #[test]
    fn quantity_restore_and_seller_changes_cannot_reuse_a_displayed_quote() {
        for case in 0..3 {
            let (mut s, mut ui, shop) = fixture();
            match case {
                0 => ui.trade_quantity = 10,
                1 => s.restore_revision += 1,
                _ => ui.market.selected_shop = Some(shop + 1_000_000),
            }
            assert!(trade_command(&s, &ui, 0, true).is_err());
        }
    }

    #[test]
    fn selected_seller_is_retained_instead_of_falling_back_to_a_nearer_shop() {
        let (mut s, mut ui, id) = fixture();
        let mut other = s.state.living.shops[&id].clone();
        other.id += 1_000_000;
        other.name = "Chosen stocked seller".into();
        other.position = LocalFrame::new(other.position)
            .to_geo([8., 0., 0.])
            .unwrap();
        other.prices.get_mut("bread").unwrap().buy += 5;
        let selected = other.id;
        s.state.living.shops.insert(selected, other);
        ui.market.selected_shop = Some(selected);
        refresh_displayed(&s, &mut ui);
        assert!(
            matches!(trade_command(&s, &ui, 0, true).unwrap(), Command::Living { action: LivingCommand::TradeQuoted {shop_id, ..} } if shop_id == selected)
        );
        s.state.living.shops.remove(&selected);
        assert!(shop(&s, &ui).is_none());
        assert!(trade_command(&s, &ui, 0, true).is_err());
        choose_next(&s, &mut ui);
        assert_eq!(ui.market.selected_shop, Some(id));
    }

    #[test]
    fn unavailable_trade_explains_inventory_capacity_and_bounds_invalid_indices() {
        let (mut s, mut ui, _) = fixture();
        s.state
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .insert("bread".into(), MAX_ITEM_COUNT);
        refresh_displayed(&s, &mut ui);
        assert!(unavailable(&s, &ui, 0, true));
        assert!(offer_label(&ui, 0).contains("capacity"));
        assert!(trade_command(&s, &ui, usize::MAX, true).is_err());
        assert_eq!(offer_label(&ui, usize::MAX), "No price at this market.");
    }
}
