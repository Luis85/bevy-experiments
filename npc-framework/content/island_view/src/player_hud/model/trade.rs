//! The Trade workspace's world reads: the merchant boat's outlook and the
//! played character's goods on stalls. Read-only views of the simulation;
//! the rows are rebuilt on the HUD's beat and written only where they differ.
use super::*;
use crate::game_ui::words;
use crate::island_shell::model::BoatOutlook;
use island_content::village::{civic, merchant};
use npc_framework::prelude::SimIndex;

/// "The boat calls in 2 days." / "The boat is in until 14:20."
pub(crate) fn boat_when_line(when: &BoatOutlook) -> String {
    match when {
        BoatOutlook::In { until } => format!("The boat is in until {until}."),
        BoatOutlook::Due { days: 0 } => "The boat calls later today.".to_owned(),
        BoatOutlook::Due { days: 1 } => "The boat calls tomorrow.".to_owned(),
        BoatOutlook::Due { days } => format!("The boat calls in {days} days."),
    }
}

/// What the boat buys and brings: "It buys fish, cloth. It brings 6 salt."
/// `brings` is `(good, units)`; empty parts are left out.
pub(crate) fn boat_goods_line(buys: &[String], brings: &[(String, u32)]) -> String {
    let mut parts = Vec::new();
    if !buys.is_empty() {
        let names: Vec<String> = buys.iter().map(|good| words::noun(good, 2)).collect();
        parts.push(format!("It buys {}.", names.join(", ")));
    }
    if !brings.is_empty() {
        let goods: Vec<String> = brings
            .iter()
            .map(|(good, units)| words::count(u64::from(*units), good))
            .collect();
        parts.push(format!("It brings {}.", goods.join(", ")));
    }
    parts.join(" ")
}

/// The outside prices that moved: "Prices outside: fish up 20%, salt down
/// 10%." `None` while every price stands at its base.
pub(crate) fn boat_prices_line(factors: &[(String, u32)]) -> Option<String> {
    let moved: Vec<String> = factors
        .iter()
        .filter_map(|(good, permille)| {
            let percent = (i64::from(*permille) - 1000) / 10;
            let name = words::noun(good, 2);
            match percent {
                0 => None,
                up if up > 0 => Some(format!("{name} up {up}%")),
                down => Some(format!("{name} down {}%", -down)),
            }
        })
        .collect();
    (!moved.is_empty()).then(|| format!("Prices outside: {}.", moved.join(", ")))
}

/// One lot of yours on a shelf: "On the stall at Harbour market: 6 bread
/// at 4 coins each." A store reads "In the store at"; no quote yet says so.
pub(crate) fn consigned_line(
    stall: bool,
    place: &str,
    good: &str,
    units: u32,
    quote: Option<u32>,
) -> String {
    let at = if stall {
        "On the stall at"
    } else {
        "In the store at"
    };
    let goods = words::count(u64::from(units), good);
    match quote {
        Some(1) => format!("{at} {place}: {goods} at 1 coin each."),
        Some(price) => format!("{at} {place}: {goods} at {price} coins each."),
        None => format!("{at} {place}: {goods}, no market price yet."),
    }
}

/// The empty state of the goods-on-stalls list.
pub(crate) const NOTHING_CONSIGNED: &str = "Nothing of yours is on a stall.";

/// The boat's rows: when it calls, what it buys and brings, prices that
/// moved, then its announced orders. Nothing in a world without a boat.
pub(super) fn boat_rows(world: &World, rows: &mut Vec<Row>) {
    let boat = view::boat(world);
    if !boat.enabled {
        return;
    }
    let outlook = merchant::outlook(world);
    let buys: Vec<String> = outlook
        .iter()
        .filter(|good| good.buys)
        .map(|good| good.good.clone())
        .collect();
    // In port it sells what is aboard now; before it calls, what it will bring.
    let brings: Vec<(String, u32)> = if boat.docked {
        boat.aboard
            .iter()
            .filter(|(_, units)| *units > 0)
            .cloned()
            .collect()
    } else {
        outlook
            .iter()
            .filter(|good| good.units_next > 0)
            .map(|good| (good.good.clone(), good.units_next))
            .collect()
    };
    rows.push(Row::heading("boat_orders", "The merchant boat"));
    rows.push(Row::note(
        "boat_when",
        boat_when_line(&crate::island_shell::boat_when(world, &boat)),
        boat_goods_line(&buys, &brings),
    ));
    let factors: Vec<(String, u32)> = outlook
        .iter()
        .map(|good| (good.good.clone(), good.factor_permille))
        .collect();
    if let Some(line) = boat_prices_line(&factors) {
        rows.push(Row::note("boat_prices", line, ""));
    }
    let orders = crate::island_shell::model::boat_order_lines(
        &crate::island_shell::boat_orders(&boat),
        boat.docked,
    );
    for (index, line) in orders.into_iter().enumerate() {
        rows.push(Row::note(
            format!("boat_order_{index}"),
            line,
            "Sell it at the trading post while the boat is in.",
        ));
    }
}

/// The played character's lots on stalls and stores, by place then good.
pub(super) fn consigned_rows(world: &World, rows: &mut Vec<Row>) {
    let Some((unit, _)) = depth::played(world) else {
        return;
    };
    rows.push(Row::heading("consigned", "Your goods on stalls"));
    let lots = view::consigned(world, unit);
    if lots.is_empty() {
        rows.push(Row::note(
            "consigned_empty",
            NOTHING_CONSIGNED,
            "Sell goods at a market stall to lay them out for buyers.",
        ));
        return;
    }
    for lot in lots {
        let entity = world
            .get_resource::<SimIndex>()
            .and_then(|index| index.get(lot.depot));
        let stall = entity
            .and_then(|entity| world.get::<civic::Depot>(entity))
            .is_none_or(|depot| depot.public);
        let place = entity
            .and_then(|entity| world.get::<civic::Building>(entity))
            .map_or_else(
                || "the market".to_owned(),
                |building| building.label.clone(),
            );
        rows.push(Row::note(
            format!("consigned_{}_{}", lot.depot.0, lot.good),
            consigned_line(stall, &place, &lot.good, lot.units, lot.quote),
            "",
        ));
    }
}
