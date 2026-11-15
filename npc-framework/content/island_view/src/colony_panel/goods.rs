//! The Goods table's market figures and the good card's words (design-ui
//! 2.2, 2.3). The figures come from the simulation's economy views
//! (`village::view::economy`) through [`EconomyCache`], refilled only when
//! the day or the latest market round changes; the words are pure functions
//! of those figures, so the glossary holds and each line is tested alone.
use super::Action;
use super::economy::Scope;
use bevy::prelude::*;
use island_content::village::view::{
    GoodFlow, HeldUpLine, MarketLine, PriceLine, Purpose, StockSplit, UseLine, WithoutCause,
    WithoutLine,
};
use island_content::village::{self, market};
use npc_framework::prelude::{SimClock, SimConfig, SimId};
use std::collections::BTreeMap;

/// When the cached figures were read: session epoch, game day and the
/// latest market round.
type CacheKey = (u64, u64, Option<u64>);

/// The market figures the Economy page shows, read from the simulation's
/// views when the day or the latest market round changes, never per frame.
/// View state: never saved, never hashed.
#[derive(Resource, Default)]
pub(crate) struct EconomyCache {
    key: Option<CacheKey>,
    /// Every traded good's price line, by good.
    pub(crate) prices: BTreeMap<String, PriceLine>,
    flow_key: Option<(CacheKey, Scope, String)>,
    /// The selected good's flow in the chosen scope (merged over the
    /// villages for the whole island).
    pub(crate) flow: Option<GoodFlow>,
    /// The chosen village's outlook, for the Priorities figures (3.9).
    outlook: Option<(CacheKey, SimId, village::view::Outlook)>,
    /// The selected good's stock split in the scope (read with its flow).
    pub(crate) split: Option<StockSplit>,
    /// Food and water in the scope: each good's split and how many go
    /// without it, for the table's Days and Low (read once a day).
    table: Option<(CacheKey, Scope, BTreeMap<String, (StockSplit, usize)>)>,
}

impl EconomyCache {
    /// The outlook of `village`, when it is the one read.
    pub(crate) fn outlook_of(&self, village: SimId) -> Option<&village::view::Outlook> {
        self.outlook
            .as_ref()
            .filter(|(_, held, _)| *held == village)
            .map(|(_, _, outlook)| outlook)
    }
    /// A food's or water's split in `scope` and how many go without it,
    /// when read.
    pub(crate) fn food_split(&self, scope: Scope, good: &str) -> Option<&(StockSplit, usize)> {
        self.table
            .as_ref()
            .filter(|(_, held, _)| *held == scope)
            .and_then(|(_, _, goods)| goods.get(good))
    }
    /// Whether the figures of `good` in `scope` are read.
    pub(crate) fn has_flow(&self, scope: Scope, good: &str) -> bool {
        self.flow_key
            .as_ref()
            .is_some_and(|(_, held, kind)| *held == scope && kind == good)
            && self.flow.is_some()
    }
}

fn cache_key(world: &World) -> CacheKey {
    let tick = world
        .get_resource::<SimClock>()
        .map_or(0, |clock| clock.tick);
    let day_length = world
        .get_resource::<SimConfig>()
        .map_or(1, |config| config.day_length.max(1));
    (
        npc_framework::agent_io::api::epoch(world),
        tick / day_length,
        world
            .get_resource::<market::MarketState>()
            .and_then(|state| state.last_round),
    )
}

/// Refill the cache when the day or the market moved, and read the selected
/// good's flow when it, the scope or the figures changed.
pub(crate) fn refresh_cache(
    world: &mut World,
    scope: Scope,
    good: Option<&str>,
    village: Option<SimId>,
) {
    let key = cache_key(world);
    world.init_resource::<EconomyCache>();
    if world.resource::<EconomyCache>().key != Some(key) {
        let prices = village::view::price_lines(world)
            .into_iter()
            .map(|line| (line.good.clone(), line))
            .collect();
        let mut cache = world.resource_mut::<EconomyCache>();
        cache.key = Some(key);
        cache.prices = prices;
    }
    if let Some(village) = village
        && world
            .resource::<EconomyCache>()
            .outlook
            .as_ref()
            .is_none_or(|(held_key, held, _)| (*held_key, *held) != (key, village))
    {
        let outlook = village::view::outlook(world, village);
        world.resource_mut::<EconomyCache>().outlook = Some((key, village, outlook));
    }
    if world
        .resource::<EconomyCache>()
        .table
        .as_ref()
        .is_none_or(|(held_key, held, _)| (*held_key, *held) != (key, scope))
    {
        let settlements: Vec<SimId> = match scope {
            Some(settlement) => vec![settlement],
            None => village::frontier::settlements::all(world)
                .into_iter()
                .map(|(id, _)| id)
                .collect(),
        };
        let table = village::catalog::trade_kinds(world)
            .iter()
            .filter(|kind| {
                village::catalog::food_restore_in(world, kind).is_some()
                    || **kind == village::catalog::WATER
            })
            .map(|kind| {
                let without = settlements
                    .iter()
                    .map(|settlement| {
                        village::view::good_flow(world, *settlement, kind)
                            .going_without
                            .len()
                    })
                    .sum();
                (
                    (*kind).to_owned(),
                    (village::view::stock_split(world, scope, kind), without),
                )
            })
            .collect();
        world.resource_mut::<EconomyCache>().table = Some((key, scope, table));
    }
    let Some(good) = good else {
        return;
    };
    let wanted = (key, scope, good.to_owned());
    if world.resource::<EconomyCache>().flow_key.as_ref() == Some(&wanted) {
        return;
    }
    let flow = match scope {
        Some(settlement) => village::view::good_flow(world, settlement, good),
        None => merge_flows(
            village::frontier::settlements::all(world)
                .into_iter()
                .map(|(settlement, _)| village::view::good_flow(world, settlement, good))
                .collect(),
        ),
    };
    let split = village::view::stock_split(world, scope, good);
    let mut cache = world.resource_mut::<EconomyCache>();
    cache.flow_key = Some(wanted);
    cache.flow = Some(flow);
    cache.split = Some(split);
}

/// What people can actually get of a good: what they hold for themselves,
/// what is on sale and what stands in the stores (not what lies at the
/// post for the boat).
pub(crate) fn reachable(split: &StockSplit) -> u32 {
    split
        .in_homes
        .saturating_add(split.on_sale)
        .saturating_add(split.in_stores)
}

/// Whether none of a good can be bought: nothing on sale, nothing in the
/// stores.
pub(crate) fn none_for_sale(split: &StockSplit) -> bool {
    split.on_sale == 0 && split.in_stores == 0
}

/// The card's stock line: where the stock is, only the parts that hold
/// some. "61 in Harbour: 58 in people's own packs, 3 on sale." / "... none
/// on sale." / "..., 4 at the post for the boat." `place` is "in Harbour"
/// or "on the island".
pub(crate) fn split_sentence(place: &str, split: &StockSplit) -> String {
    if split.total == 0 {
        return format!("None {place}.");
    }
    let mut parts = Vec::new();
    if split.in_homes > 0 {
        parts.push(format!("{} in people's own packs", split.in_homes));
    }
    if split.on_sale > 0 {
        parts.push(format!("{} on sale", split.on_sale));
    }
    if split.in_stores > 0 {
        parts.push(format!("{} in the stores", split.in_stores));
    }
    if none_for_sale(split) {
        parts.push("none on sale".to_owned());
    }
    if split.at_post > 0 {
        parts.push(format!("{} at the post for the boat", split.at_post));
    }
    format!("{} {place}: {}.", split.total, parts.join(", "))
}

/// The whole island's flow of one good: every village's, each line once.
pub(crate) fn merge_flows(flows: Vec<GoodFlow>) -> GoodFlow {
    let mut merged = GoodFlow::default();
    for flow in flows {
        if merged.good.is_empty() {
            merged.good = flow.good.clone();
        }
        for line in flow.used_by {
            if !merged.used_by.contains(&line) {
                merged.used_by.push(line);
            }
        }
        for line in flow.going_without {
            if !merged.going_without.iter().any(|old| old.who == line.who) {
                merged.going_without.push(line);
            }
        }
        for line in flow.held_up {
            if !merged.held_up.contains(&line) {
                merged.held_up.push(line);
            }
        }
    }
    merged
}

fn coins(count: u32) -> String {
    format!("{count} coin{}", if count == 1 { "" } else { "s" })
}

/// The Goods table's price cell: "4", or "4 +1" / "4 -1" when the price
/// moved since yesterday; "–" for a good that is not bought or sold.
pub(crate) fn price_cell(price: Option<u32>, day_ago: Option<u32>) -> String {
    let Some(price) = price else {
        return "\u{2013}".to_owned();
    };
    match day_ago {
        Some(old) if old < price => format!("{price} +{}", price - old),
        Some(old) if old > price => format!("{price} -{}", old - price),
        _ => price.to_string(),
    }
}

/// Card line 2: the price and how it moved since yesterday.
pub(crate) fn price_sentence(line: Option<&PriceLine>) -> String {
    let Some(line) = line else {
        return "No price: it is not bought or sold.".to_owned();
    };
    let price = coins(line.price);
    let mut text = match line.day_ago {
        Some(old) if old < line.price => {
            format!("Price {price}, up {} since yesterday.", line.price - old)
        }
        Some(old) if old > line.price => {
            format!("Price {price}, down {} since yesterday.", old - line.price)
        }
        Some(_) => format!("Price {price}, the same as yesterday."),
        None => format!("Price {price}."),
    };
    if line.at_ceiling {
        text.push_str(" It cannot go higher.");
    }
    text
}

/// Card line 3: the last market round.
pub(crate) fn market_sentence(market: Option<&MarketLine>) -> String {
    match market {
        None => "It has not been to market yet.".to_owned(),
        Some(round) if round.supply == 0 => {
            format!("Last market: none offered, {} wanted.", round.funded_demand)
        }
        Some(round) => format!(
            "Last market: {} offered, {} wanted, {} sold.",
            round.supply, round.funded_demand, round.sold
        ),
    }
}

/// Card line 3's tooltip: what "wanted" counts, and when the round was
/// ("Autumn, day 6", "14:20").
pub(crate) fn market_tip(date: &str, time: &str) -> String {
    format!("Wanted counts only buyers who had the coins. {date}, {time}.")
}

/// Card line 4: buyers who wanted it only for less than the price.
pub(crate) fn below_sentence(market: Option<&MarketLine>) -> Option<String> {
    market.filter(|round| round.below_price > 0).map(|round| {
        format!(
            "{} more were wanted, but only for less than the price.",
            round.below_price
        )
    })
}

/// Card line 5: the bars' heights in pixels (4 to 16, linear between the
/// lowest and the highest price; all 10 when they are equal) and their
/// words. `None` with fewer than two prices.
pub(crate) fn spark(history: &[u32]) -> Option<(Vec<u32>, String)> {
    if history.len() < 2 {
        return None;
    }
    let shown = &history[history.len().saturating_sub(16)..];
    let low = *shown.iter().min()?;
    let high = *shown.iter().max()?;
    let heights = shown
        .iter()
        .map(|price| {
            if high == low {
                10
            } else {
                4 + (price - low) * 12 / (high - low)
            }
        })
        .collect();
    let words = if high == low {
        format!("Last {} markets: steady at {}", shown.len(), coins(low))
    } else {
        format!("Last {} markets: {low} to {high} coins", shown.len())
    };
    Some((heights, words))
}

/// Who a use line belongs to, as card line 6 groups it. `job` is the
/// plural job of a resident ("bakers"), `made` the good a recipe makes.
pub(crate) fn user_group(
    line: &UseLine,
    job: &dyn Fn(Option<SimId>) -> Option<String>,
    made: &dyn Fn(&str) -> Option<String>,
) -> String {
    match &line.purpose {
        Purpose::Eat => "households (food)".to_owned(),
        Purpose::Drink => "households (drink)".to_owned(),
        Purpose::Fuel => "homes (fuel)".to_owned(),
        Purpose::Input { recipe } => {
            let makers = job(line.who).unwrap_or_else(|| "workshops".to_owned());
            match made(recipe) {
                Some(good) => format!("{makers} ({good})"),
                None => makers,
            }
        }
        Purpose::Build { .. } => "building sites".to_owned(),
        Purpose::Tool => format!(
            "{} (tool)",
            job(line.who).unwrap_or_else(|| "workers".to_owned())
        ),
        Purpose::Keep => "stores".to_owned(),
    }
}

/// Card line 6: at most four users, most units wanted first.
pub(crate) fn used_by_sentence(groups: &[(String, u32)]) -> String {
    let mut totals: Vec<(String, u32)> = Vec::new();
    for (group, wanted) in groups {
        match totals.iter_mut().find(|(name, _)| name == group) {
            Some((_, sum)) => *sum += wanted,
            None => totals.push((group.clone(), *wanted)),
        }
    }
    if totals.is_empty() {
        return "Used by: nobody on the island.".to_owned();
    }
    totals.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let more = totals.len().saturating_sub(4);
    let names: Vec<&str> = totals
        .iter()
        .take(4)
        .map(|(name, _)| name.as_str())
        .collect();
    if more > 0 {
        format!("Used by: {} and {more} more.", names.join(", "))
    } else {
        format!("Used by: {}.", names.join(", "))
    }
}

fn cause_rank(cause: &WithoutCause) -> u8 {
    match cause {
        WithoutCause::NoCoins => 0,
        WithoutCause::NoSeller => 1,
        WithoutCause::TooDear { .. } => 2,
        WithoutCause::TooFar => 3,
    }
}

/// A cause of going without for one resident ("has no coins") or for
/// several ("have no coins").
fn cause_words(rank: u8, one: bool) -> &'static str {
    match (rank, one) {
        (0, true) => "has no coins",
        (0, false) => "have no coins",
        (1, true) => "finds nobody selling it",
        (1, false) => "find nobody selling it",
        (2, true) => "finds it too dear",
        (2, false) => "find it too dear",
        (_, true) => "lives too far from a seller",
        (_, false) => "live too far from a seller",
    }
}

/// Card line 7: who has less of `good` than they like to keep (most still
/// hold some; it is not starving), with the two commonest causes or that
/// none is on sale, and the residents behind "Show them". `None` when
/// nobody does.
pub(crate) fn without_sentence(
    lines: &[WithoutLine],
    good: &str,
    none_on_sale: bool,
) -> Option<(String, Vec<SimId>)> {
    let first = lines.first()?;
    let who: Vec<SimId> = lines.iter().map(|line| line.who).collect();
    let short = if lines.len() == 1 {
        format!("{} has less {good} than they like to keep", first.label)
    } else {
        format!(
            "{} residents have less {good} than they like to keep",
            lines.len()
        )
    };
    if none_on_sale {
        return Some((format!("{short}; none is on sale."), who));
    }
    if lines.len() == 1 {
        return Some((
            format!(
                "{short}, and {}.",
                cause_words(cause_rank(&first.cause), true)
            ),
            who,
        ));
    }
    let mut counts: BTreeMap<u8, u32> = BTreeMap::new();
    for line in lines {
        *counts.entry(cause_rank(&line.cause)).or_default() += 1;
    }
    let mut causes: Vec<(u8, u32)> = counts.into_iter().collect();
    causes.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let parts: Vec<String> = causes
        .iter()
        .take(2)
        .map(|(rank, count)| format!("{count} {}", cause_words(*rank, *count == 1)))
        .collect();
    Some((
        format!(
            "{short}. {}.",
            crate::game_ui::words::capital(&parts.join(", "))
        ),
        who,
    ))
}

/// Card line 8: what waits for the good, at most two lines, the most
/// waiting first; "(2)" when more than one waits the same way.
pub(crate) fn held_up_lines(lines: &[HeldUpLine], good: &str) -> Vec<String> {
    let mut groups: Vec<((String, u32), u32)> = Vec::new();
    for line in lines {
        let key = (line.what.clone(), line.missing);
        match groups.iter_mut().find(|(held, _)| *held == key) {
            Some((_, count)) => *count += 1,
            None => groups.push((key, 1)),
        }
    }
    groups.sort_by_key(|group| std::cmp::Reverse(group.1));
    groups
        .into_iter()
        .take(2)
        .map(|((what, missing), count)| {
            let more = if count > 1 {
                format!(" ({count})")
            } else {
                String::new()
            };
            format!(
                "Held up: {what} waits for {}{more}.",
                crate::game_ui::words::count(u64::from(missing), good)
            )
        })
        .collect()
}

/// Card line 9 when nobody makes the good: the job that would, with its
/// open places, and the link to it. `job` is the job's name, its key and
/// its open places in the scope.
pub(crate) fn nobody_makes(
    job: Option<(&str, &str, u32)>,
    village: Scope,
) -> (String, Option<(String, Action)>) {
    match job {
        Some((name, key, open)) if open > 0 => (
            format!(
                "Nobody makes it now. {} has {open} open place{}.",
                super::capitalised(name),
                if open == 1 { "" } else { "s" }
            ),
            Some((
                format!("Show {} in Jobs", name.to_lowercase()),
                Action::JobsOf(village, Some(key.to_owned())),
            )),
        ),
        _ => (
            "Nobody makes it now, and no workplace for it has a free place.".to_owned(),
            None,
        ),
    }
}

/// The lines of a good card the narrow ledger keeps in view: price, last
/// market, going without and held up (2, 3, 7, 8); the rest fold.
pub(crate) const COMPACT_LINES: [u8; 4] = [2, 3, 7, 8];

#[cfg(test)]
mod tests {
    use super::*;

    fn price(price: u32, day_ago: Option<u32>) -> PriceLine {
        PriceLine {
            good: "bread".into(),
            display: "Bread".into(),
            price,
            previous: None,
            day_ago,
            change_permille: 0,
            trend: village::view::Trend::Steady,
            at_ceiling: false,
            last_market: None,
            history: Vec::new(),
        }
    }

    fn round(supply: u32, wanted: u32, sold: u32, below: u32) -> MarketLine {
        MarketLine {
            tick: 0,
            supply,
            funded_demand: wanted,
            sold,
            unmet: wanted.saturating_sub(sold),
            below_price: below,
        }
    }

    fn without(who: u32, cause: WithoutCause) -> WithoutLine {
        WithoutLine {
            who: SimId(who),
            label: format!("Resident {who}"),
            wanted: 1,
            cause,
            sentence: String::new(),
        }
    }

    #[test]
    fn a_price_that_rose_shows_its_change_beside_it() {
        assert_eq!(price_cell(Some(4), Some(3)), "4 +1");
        assert_eq!(price_cell(Some(4), Some(4)), "4");
        assert_eq!(price_cell(Some(4), None), "4");
        assert_eq!(price_cell(Some(3), Some(5)), "3 -2");
    }

    #[test]
    fn a_good_that_is_not_sold_has_a_dash_and_no_change() {
        assert_eq!(price_cell(None, Some(3)), "\u{2013}");
        assert_eq!(price_sentence(None), "No price: it is not bought or sold.");
    }

    #[test]
    fn the_good_card_says_price_last_market_and_who_goes_without() {
        assert_eq!(
            price_sentence(Some(&price(4, Some(3)))),
            "Price 4 coins, up 1 since yesterday."
        );
        assert_eq!(
            price_sentence(Some(&price(1, Some(2)))),
            "Price 1 coin, down 1 since yesterday."
        );
        assert_eq!(
            price_sentence(Some(&price(4, Some(4)))),
            "Price 4 coins, the same as yesterday."
        );
        let mut top = price(9, None);
        top.at_ceiling = true;
        assert_eq!(
            price_sentence(Some(&top)),
            "Price 9 coins. It cannot go higher."
        );
        let market = round(12, 18, 12, 6);
        assert_eq!(
            market_sentence(Some(&market)),
            "Last market: 12 offered, 18 wanted, 12 sold."
        );
        assert_eq!(
            below_sentence(Some(&market)).as_deref(),
            Some("6 more were wanted, but only for less than the price.")
        );
        assert_eq!(below_sentence(Some(&round(1, 1, 1, 0))), None);
        let (line, who) =
            without_sentence(&[without(4, WithoutCause::NoCoins)], "bread", false).unwrap();
        assert_eq!(
            line,
            "Resident 4 has less bread than they like to keep, and has no coins."
        );
        assert_eq!(who, vec![SimId(4)]);
    }

    #[test]
    fn the_last_market_says_what_wanted_counts_and_when() {
        assert_eq!(
            market_tip("Autumn, day 6", "14:20"),
            "Wanted counts only buyers who had the coins. Autumn, day 6, 14:20."
        );
    }

    #[test]
    fn a_market_with_nothing_offered_says_so() {
        assert_eq!(
            market_sentence(Some(&round(0, 5, 0, 0))),
            "Last market: none offered, 5 wanted."
        );
    }

    #[test]
    fn a_good_never_brought_to_market_says_so() {
        assert_eq!(market_sentence(None), "It has not been to market yet.");
    }

    #[test]
    fn the_card_names_what_holds_its_makers_up() {
        let held = |what: &str, who: u32| HeldUpLine {
            what: what.into(),
            who: Some(SimId(who)),
            site: None,
            missing: 2,
            since_tick: None,
        };
        assert_eq!(
            held_up_lines(&[held("Harbour cottage", 1)], "plank"),
            vec!["Held up: Harbour cottage waits for 2 planks.".to_owned()]
        );
        assert_eq!(
            held_up_lines(
                &[held("Bread", 1), held("Bread", 2), held("Inn", 3)],
                "flour"
            ),
            vec![
                "Held up: Bread waits for 2 flour (2).".to_owned(),
                "Held up: Inn waits for 2 flour.".to_owned(),
            ]
        );
        assert!(held_up_lines(&[], "flour").is_empty());
    }

    #[test]
    fn nobody_makes_it_offers_the_job_with_open_places() {
        let (line, link) = nobody_makes(Some(("baker", "baker", 2)), Some(SimId(3)));
        assert_eq!(line, "Nobody makes it now. Baker has 2 open places.");
        assert_eq!(
            link,
            Some((
                "Show baker in Jobs".to_owned(),
                Action::JobsOf(Some(SimId(3)), Some("baker".to_owned()))
            ))
        );
        let (line, link) = nobody_makes(Some(("baker", "baker", 0)), None);
        assert_eq!(
            line,
            "Nobody makes it now, and no workplace for it has a free place."
        );
        assert_eq!(link, None);
    }

    #[test]
    fn the_price_bars_span_lowest_to_highest_and_need_two_prices() {
        assert_eq!(spark(&[3]), None);
        assert_eq!(spark(&[]), None);
        let (heights, words) = spark(&[3, 4, 5]).unwrap();
        assert_eq!(heights, vec![4, 10, 16]);
        assert_eq!(words, "Last 3 markets: 3 to 5 coins");
        let (heights, words) = spark(&[2, 2]).unwrap();
        assert_eq!(heights, vec![10, 10]);
        assert_eq!(words, "Last 2 markets: steady at 2 coins");
        let long: Vec<u32> = (0..20).collect();
        assert_eq!(spark(&long).unwrap().0.len(), 16, "sixteen bars at most");
    }

    #[test]
    fn going_without_names_the_two_commonest_causes() {
        let dear = || WithoutCause::TooDear { quote: 5, limit: 3 };
        let lines = [
            without(1, WithoutCause::NoCoins),
            without(2, dear()),
            without(3, WithoutCause::NoCoins),
            without(4, dear()),
            without(5, WithoutCause::NoCoins),
        ];
        let (line, who) = without_sentence(&lines, "bread", false).unwrap();
        assert_eq!(
            line,
            "5 residents have less bread than they like to keep. 3 have no coins, 2 find it too dear."
        );
        assert_eq!(who.len(), 5);
        // Not starving: most still hold some; and when none is to be bought
        // the card says so.
        let (line, _) = without_sentence(&lines, "bread", true).unwrap();
        assert_eq!(
            line,
            "5 residents have less bread than they like to keep; none is on sale."
        );
        assert_eq!(without_sentence(&[], "bread", false), None);
    }

    fn split(on_sale: u32, in_homes: u32, in_stores: u32, at_post: u32) -> StockSplit {
        StockSplit {
            good: "bread".into(),
            on_sale,
            in_homes,
            in_stores,
            at_post,
            total: on_sale + in_homes + in_stores + at_post,
        }
    }

    #[test]
    fn the_stock_line_says_where_the_stock_is_in_plain_words() {
        // Every part.
        assert_eq!(
            split_sentence("in Harbour", &split(3, 50, 8, 4)),
            "65 in Harbour: 50 in people's own packs, 3 on sale, 8 in the stores, 4 at the post for the boat."
        );
        // Only some parts.
        assert_eq!(
            split_sentence("in Harbour", &split(3, 58, 0, 0)),
            "61 in Harbour: 58 in people's own packs, 3 on sale."
        );
        assert_eq!(
            split_sentence("on the island", &split(0, 0, 12, 0)),
            "12 on the island: 12 in the stores."
        );
        // None on sale.
        assert_eq!(
            split_sentence("in Harbour", &split(0, 61, 0, 0)),
            "61 in Harbour: 61 in people's own packs, none on sale."
        );
        // At the post only.
        assert_eq!(
            split_sentence("in Harbour", &split(0, 0, 0, 6)),
            "6 in Harbour: none on sale, 6 at the post for the boat."
        );
        assert_eq!(
            split_sentence("in Harbour", &split(0, 0, 0, 0)),
            "None in Harbour."
        );
        // What people can get, and whether any can be bought.
        assert_eq!(reachable(&split(3, 50, 8, 4)), 61);
        assert!(none_for_sale(&split(0, 61, 0, 2)));
        assert!(!none_for_sale(&split(0, 0, 1, 0)));
    }

    #[test]
    fn the_whole_island_card_merges_every_villages_flow() {
        let use_line = |who: u32| UseLine {
            who: Some(SimId(who)),
            label: format!("Resident {who}"),
            purpose: Purpose::Eat,
            wanted: 1,
            has: 0,
        };
        let a = GoodFlow {
            good: "bread".into(),
            used_by: vec![use_line(1), use_line(2)],
            going_without: vec![without(1, WithoutCause::NoCoins)],
            held_up: Vec::new(),
        };
        let b = GoodFlow {
            good: "bread".into(),
            used_by: vec![use_line(2), use_line(3)],
            going_without: vec![
                without(1, WithoutCause::NoCoins),
                without(7, WithoutCause::TooFar),
            ],
            held_up: Vec::new(),
        };
        let merged = merge_flows(vec![a, b]);
        assert_eq!(merged.good, "bread");
        assert_eq!(merged.used_by.len(), 3, "resident 2 once");
        assert_eq!(merged.going_without.len(), 2, "resident 1 once");
        let groups: Vec<(String, u32)> = merged
            .used_by
            .iter()
            .map(|line| (user_group(line, &|_| None, &|_| None), line.wanted))
            .collect();
        assert_eq!(used_by_sentence(&groups), "Used by: households (food).");
        assert_eq!(used_by_sentence(&[]), "Used by: nobody on the island.");
    }
}
