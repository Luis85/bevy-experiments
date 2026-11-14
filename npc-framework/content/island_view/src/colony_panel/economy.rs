//! Stock, reference prices and professions per settlement, read from existing
//! simulation data. Trends come from a small view-side sample history; nothing
//! here is saved, hashed or consulted by the simulation.
use super::{Action, model::Entry};
use bevy::prelude::*;
use island_content::village::frontier::oversee;
use island_content::village::{self, Role, Villager, catalog, economy};
use npc_framework::prelude::{SimClock, SimId, SimIndex};
use std::collections::{BTreeMap, VecDeque};

/// Simulation ticks between two stock samples (ten seconds at 1x).
pub(crate) const SAMPLE_TICKS: u64 = 200;
/// Samples kept per good: the trend looks back at most one game day.
pub(crate) const HISTORY: usize = 13;

/// `None` is the whole island; `Some` one settlement.
pub(crate) type Scope = Option<SimId>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trend {
    /// Not enough samples yet.
    Unknown,
    Steady,
    Rising(u32),
    Falling(u32),
}

impl Trend {
    pub(crate) fn text(self) -> String {
        match self {
            Self::Unknown => "\u{2013}".into(),
            Self::Steady => "steady".into(),
            Self::Rising(by) => format!("up {by}"),
            Self::Falling(by) => format!("down {by}"),
        }
    }
}

/// Compare the newest sample with the oldest one kept. A change must exceed
/// both one unit and a tenth of the earlier stock to count as a trend.
pub(crate) fn trend(samples: &VecDeque<u32>) -> Trend {
    let (Some(first), Some(last)) = (samples.front(), samples.back()) else {
        return Trend::Unknown;
    };
    if samples.len() < 2 {
        return Trend::Unknown;
    }
    let change = last.abs_diff(*first);
    if change == 0 || change < (*first / 10).max(1) {
        Trend::Steady
    } else if last > first {
        Trend::Rising(change)
    } else {
        Trend::Falling(change)
    }
}

/// View-side stock samples per `(scope, good)`, scoped to the session epoch.
#[derive(Resource, Default)]
pub(crate) struct StockTrends {
    epoch: Option<u64>,
    last_tick: Option<u64>,
    samples: BTreeMap<(Scope, String), VecDeque<u32>>,
}

impl StockTrends {
    /// True when a new sample is due at `tick` for the world `epoch`.
    pub(crate) fn due(&mut self, epoch: u64, tick: u64) -> bool {
        if self.epoch != Some(epoch) || self.last_tick.is_some_and(|last| tick < last) {
            // Another world, or a clock that moved backwards: start over.
            *self = Self {
                epoch: Some(epoch),
                ..Self::default()
            };
        }
        self.last_tick
            .is_none_or(|last| tick >= last + SAMPLE_TICKS)
    }
    pub(crate) fn record(&mut self, tick: u64, totals: &BTreeMap<(Scope, String), u32>) {
        self.last_tick = Some(tick);
        // A good that disappeared entirely is sampled as zero, not forgotten.
        for (key, history) in &mut self.samples {
            if !totals.contains_key(key) {
                history.push_back(0);
            }
        }
        for (key, total) in totals {
            self.samples
                .entry(key.clone())
                .or_default()
                .push_back(*total);
        }
        for history in self.samples.values_mut() {
            while history.len() > HISTORY {
                history.pop_front();
            }
        }
    }
    /// The trend of several goods taken together (all food, say): their
    /// histories summed sample by sample, newest aligned with newest.
    pub(crate) fn trend_where(&self, scope: Scope, wanted: &dyn Fn(&str) -> bool) -> Trend {
        let mut sum: VecDeque<u32> = VecDeque::new();
        for ((held, kind), history) in &self.samples {
            if *held != scope || !wanted(kind) {
                continue;
            }
            for (back, units) in history.iter().rev().enumerate() {
                if sum.len() <= back {
                    sum.push_front(0);
                }
                let index = sum.len() - 1 - back;
                sum[index] = sum[index].saturating_add(*units);
            }
        }
        trend(&sum)
    }
    pub(crate) fn trend(&self, scope: Scope, kind: &str) -> Trend {
        self.samples
            .get(&(scope, kind.to_owned()))
            .map_or(Trend::Unknown, trend)
    }
}

/// Goods held by living residents, for the island and for each settlement.
pub(crate) fn stock_totals(world: &World) -> BTreeMap<(Scope, String), u32> {
    let mut totals = BTreeMap::new();
    let Some(index) = world.get_resource::<SimIndex>() else {
        return totals;
    };
    for entity in index.map.values() {
        let Some(person) = world.get::<Villager>(*entity) else {
            continue;
        };
        if person.role == Role::Visitor || !village::living(world, *entity) {
            continue;
        }
        let settlement = village::frontier::settlements::member(world, *entity);
        for (_, item) in economy::free_items(world, *entity) {
            // The island total, plus the resident's own settlement when it has one.
            for scope in std::iter::once(None).chain(settlement.map(Some)) {
                let held: &mut u32 = totals.entry((scope, item.kind.clone())).or_default();
                *held = held.saturating_add(item.stack);
            }
        }
    }
    totals
}

/// Sample at most once per [`SAMPLE_TICKS`]; cheap enough to keep running in
/// every mode so the table has a history when it is first opened.
pub(crate) fn sample(world: &mut World) {
    let tick = world
        .get_resource::<SimClock>()
        .map_or(0, |clock| clock.tick);
    let epoch = npc_framework::agent_io::api::epoch(world);
    if !world.resource_mut::<StockTrends>().due(epoch, tick) {
        return;
    }
    let totals = stock_totals(world);
    world.resource_mut::<StockTrends>().record(tick, &totals);
}

pub(crate) fn scopes(world: &World) -> Vec<(Scope, String)> {
    let mut scopes = vec![(None, "Whole island".to_owned())];
    for (id, entity) in village::frontier::settlements::all(world) {
        if let Some(place) = world.get::<village::frontier::Settlement>(entity) {
            scopes.push((Some(id), place.label.clone()));
        }
    }
    scopes
}

/// The three groups of the goods table, in table order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum GoodGroup {
    FoodAndWater,
    Materials,
    ToolsAndClothing,
}

impl GoodGroup {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::FoodAndWater => "Food and water",
            Self::Materials => "Materials",
            Self::ToolsAndClothing => "Tools and clothing",
        }
    }
    /// Food is what the world says restores hunger, and water; tools and
    /// clothing are the goods the icon table knows as such; the rest is
    /// material.
    pub(crate) fn of(kind: &str, edible: bool, water: bool) -> Self {
        use crate::game_ui::icon::Icon;
        if edible || water {
            return Self::FoodAndWater;
        }
        match Icon::of_good(kind) {
            Some(
                Icon::Axes
                | Icon::Pickaxes
                | Icon::Hammers
                | Icon::Hoes
                | Icon::HuntingSpears
                | Icon::FishingRods
                | Icon::WarmClothes
                | Icon::Bowls,
            ) => Self::ToolsAndClothing,
            _ => Self::Materials,
        }
    }
}

/// Whether a good is low, by the rules the top bar uses: food and water
/// taken together are below one per resident (so the water row is marked,
/// and every food row when all food is short), or open sites still need more
/// of it than the island holds.
pub(crate) fn good_low(stock: u32, needed_by_sites: u32, group_short: bool) -> bool {
    group_short || needed_by_sites > stock
}

/// Below this many days of use a good is low, whatever else holds.
pub(crate) const LOW_DAYS: u32 = 2;

/// Whether the days a stock lasts make it low: under [`LOW_DAYS`] of the
/// simulation's own estimate of a day's use. A good with no known use is
/// never low by this rule.
pub(crate) fn days_low(days: Option<u32>) -> bool {
    days.is_some_and(|days| days < LOW_DAYS)
}

/// "3", "99+" or a dash: the days a stock lasts.
pub(crate) fn days_words(days: Option<u32>) -> String {
    match days {
        Some(days) if days > 99 => "99+".to_owned(),
        Some(days) => days.to_string(),
        None => "\u{2013}".to_owned(),
    }
}

/// One day's use and the stock of every good the simulation estimates, in
/// a village or (summed) on the whole island: `kind -> (per day, stock)`.
pub(crate) fn day_use(world: &World, scope: Scope) -> BTreeMap<String, (u32, u32)> {
    let settlements: Vec<SimId> = match scope {
        Some(id) => vec![id],
        None => village::frontier::settlements::all(world)
            .into_iter()
            .map(|(id, _)| id)
            .collect(),
    };
    let mut out: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for settlement in settlements {
        for usage in village::view::daily_use(world, settlement) {
            let entry = out.entry(usage.good).or_default();
            entry.0 += usage.per_day;
            entry.1 += usage.stock;
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StockRow {
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) stock: u32,
    pub(crate) price: Option<u32>,
    pub(crate) trend: Trend,
    /// Days the stock lasts at the simulation's estimate of a day's use.
    pub(crate) days: Option<u32>,
    /// The price a day of market rounds ago (2.2), from the view cache.
    pub(crate) day_ago: Option<u32>,
}

fn good_label(world: &World, kind: &str) -> String {
    let name = catalog::display_in(world, kind).map_or_else(
        || {
            kind.strip_prefix("village_")
                .unwrap_or(kind)
                .replace('_', " ")
        },
        str::to_owned,
    );
    let mut letters = name.chars();
    letters.next().map_or(name.clone(), |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// Every tradeable good, including the ones nobody currently holds.
pub(crate) fn stock_rows(world: &World, scope: Scope) -> Vec<StockRow> {
    let totals = stock_totals(world);
    let trends = world.get_resource::<StockTrends>();
    let usage = day_use(world, scope);
    let prices = world.get_resource::<super::goods::EconomyCache>();
    // The catalog's goods only: a diary or a keepsake in a pack is not a
    // good of the island's market (closing phase, defect 18).
    let kinds: Vec<&str> = catalog::trade_kinds(world).to_vec();
    kinds
        .into_iter()
        .map(|kind| StockRow {
            kind: kind.to_owned(),
            label: good_label(world, kind),
            stock: totals.get(&(scope, kind.to_owned())).copied().unwrap_or(0),
            price: village::market::unit_price(world, kind),
            trend: trends.map_or(Trend::Unknown, |trends| trends.trend(scope, kind)),
            // Days left for food and water only: for other goods the
            // simulation's day of use is the units traded, not consumed.
            days: (catalog::food_restore_in(world, kind).is_some() || kind == catalog::WATER)
                .then(|| usage.get(kind))
                .flatten()
                .and_then(|(per_day, stock)| stock.checked_div(*per_day)),
            day_ago: prices
                .and_then(|cache| cache.prices.get(kind))
                .and_then(|line| line.day_ago),
        })
        .collect()
}

/// A resident's trade as players read it ("Well keeper", "Potter"): the
/// profession's `display` from the world's data, else the trade's own name.
pub(crate) fn trade_display(world: &World, role: Role) -> String {
    capitalised(&village::frontier::economy_spec::trade_label(world, role).replace('_', " "))
}

/// Which table the Economy page shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum EconomyPage {
    #[default]
    Goods,
    Jobs,
    Priorities,
}

/// One press of a trade's controls in a village's Jobs table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetStep {
    Fewer,
    More,
    /// Clear the target: the village decides again.
    VillageDecides,
}

/// The wanted headcount after a press: one from the overseer's target, or
/// from who works the trade now while the village decides; `None` hands the
/// trade back to the village.
pub(crate) fn next_wanted(current: u32, wanted: Option<u32>, step: TargetStep) -> Option<u32> {
    let base = wanted.unwrap_or(current);
    match step {
        TargetStep::Fewer => Some(base.saturating_sub(1)),
        TargetStep::More => Some((base + 1).min(oversee::MAX_TARGET)),
        TargetStep::VillageDecides => None,
    }
}

/// The recorded command for a press on a row of the profession table, or
/// why there is none, in one sentence.
pub(crate) fn target_from_row(
    world: &World,
    settlement: SimId,
    row: &oversee::ProfessionRowView,
    step: TargetStep,
) -> Result<oversee::SetProfessionTarget, String> {
    let wanted = next_wanted(row.current, row.wanted, step);
    if wanted == row.wanted {
        return Err(match step {
            TargetStep::VillageDecides => {
                format!("The village already decides its {}.", plural(&row.display))
            }
            TargetStep::More => format!(
                "A village can ask for at most {} of one job.",
                oversee::MAX_TARGET
            ),
            TargetStep::Fewer => format!("No {} are wanted already.", plural(&row.display)),
        });
    }
    let command = oversee::SetProfessionTarget {
        settlement,
        profession: Role::from_key(&row.key),
        wanted,
    };
    command
        .check(world)
        .map_err(|reason| crate::game_ui::words::sentence(&reason))?;
    Ok(command)
}

/// The recorded command for a press on trade `profession` of a village.
pub(crate) fn target_command(
    world: &World,
    settlement: SimId,
    profession: &str,
    step: TargetStep,
) -> Result<oversee::SetProfessionTarget, String> {
    let row = oversee::profession_table(world, settlement)
        .into_iter()
        .find(|row| row.key == profession)
        .ok_or_else(|| "That job is not in this village's table any more.".to_owned())?;
    target_from_row(world, settlement, &row, step)
}

/// What the ledger's foot says once a target was applied.
pub(crate) fn target_success(world: &World, command: &oversee::SetProfessionTarget) -> String {
    let name = trade_display(world, command.profession);
    match command.wanted {
        None => format!("{name}: the village decides again."),
        Some(count) => format!(
            "{}: {count} wanted. Residents change job one at a time, after the day's work.",
            capitalised(&plural(&name))
        ),
    }
}

/// "Bakers", "Well keepers": the trade's display name, plural, lower case.
fn plural(display: &str) -> String {
    crate::game_ui::words::noun(display, 2)
}

/// A topic key of the simulation (`housing`) as its topic.
fn topic_of(topic: &str) -> Option<oversee::PriorityTopic> {
    oversee::PriorityTopic::ALL
        .into_iter()
        .find(|candidate| candidate.key() == topic)
}

/// The level a priority row's control shows now.
pub(crate) fn priority_level(
    world: &World,
    settlement: SimId,
    topic: &str,
    profession: Option<&str>,
) -> oversee::PriorityLevel {
    topic_of(topic).map_or(oversee::PriorityLevel::Normal, |topic| {
        oversee::level(world, settlement, topic, profession.map(Role::from_key))
    })
}

/// The recorded command for a priority segment, or why there is none.
pub(crate) fn priority_command(
    world: &World,
    settlement: SimId,
    topic: &str,
    profession: Option<String>,
    level: oversee::PriorityLevel,
) -> Result<oversee::SetVillagePriority, String> {
    let topic = topic_of(topic).ok_or_else(|| "That priority is not known here.".to_owned())?;
    let command = oversee::SetVillagePriority {
        settlement,
        topic,
        profession: profession.as_deref().map(Role::from_key),
        level,
    };
    command
        .check(world)
        .map_err(|reason| crate::game_ui::words::sentence(&reason))?;
    Ok(command)
}

/// What the ledger's foot says once a priority was applied: the
/// simulation's own sentence of what the level changes.
pub(crate) fn priority_success(world: &World, command: &oversee::SetVillagePriority) -> String {
    crate::game_ui::words::sentence(&oversee::effect(
        world,
        command.topic,
        command.profession,
        command.level,
    ))
}

/// "Low", "Normal", "High".
pub(crate) fn level_label(level: oversee::PriorityLevel) -> &'static str {
    match level {
        oversee::PriorityLevel::Low => "Low",
        oversee::PriorityLevel::Normal => "Normal",
        oversee::PriorityLevel::High => "High",
    }
}

/// The tooltip of one of the Oversee controls of a row: what a press does.
pub(crate) fn choice_tip(action: &Action) -> Option<crate::game_ui::tooltip::Tooltip> {
    use crate::game_ui::tooltip::Tooltip;
    Some(match action {
        Action::BehaviourFile(crate::BehaviourFile::Save) => Tooltip::new(
            "Save behaviour to file",
            "Write the island's behaviour document to behaviour.json beside your save, to edit it.",
        ),
        Action::BehaviourFile(crate::BehaviourFile::Reload) => Tooltip::new(
            "Reload behaviour from file",
            "Read behaviour.json and change how every resident behaves, if the document passes the checks.",
        )
        .detail(["A refused document changes nothing and says why."]),
        Action::Target { step, .. } => match step {
            TargetStep::Fewer => Tooltip::new(
                "Fewer wanted",
                "Ask for one fewer in this job. Those no longer needed move to jobs that are short, one at a time.",
            ),
            TargetStep::More => Tooltip::new(
                "More wanted",
                "Ask for one more in this job. A resident whose own job can spare them joins after the day's work.",
            ),
            TargetStep::VillageDecides => Tooltip::new(
                "Village decides",
                "Clear your number: the village staffs the job from its own shortages again.",
            ),
        }
        .detail(["Nobody leaves in the middle of their work. The last one in a job the village cannot do without stays."]),
        Action::Priority { level, .. } => Tooltip::new(
            level_label(*level),
            match level {
                oversee::PriorityLevel::Low => {
                    "Seen to after everything else: fewer projects and smaller reserves."
                }
                oversee::PriorityLevel::Normal => "The village weighs it as it always has.",
                oversee::PriorityLevel::High => {
                    "Seen to first: sooner projects and larger reserves."
                }
            },
        )
        .detail(["The line under the row says what this level changes."]),
        _ => return None,
    })
}

fn capitalised(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// The selector's label for a scope: "Whole island" or the village's name.
pub(crate) fn scope_choice(scope: Scope, label: &str) -> String {
    if scope.is_none() {
        "Whole island".to_owned()
    } else {
        label.to_owned()
    }
}

/// How near the camera's centre a village's centre must be to be the one
/// Jobs and Priorities open on, in metres.
pub(crate) const NEAR_VILLAGE_M: f32 = 60.0;

/// The village Jobs and Priorities open on: the one whose centre is within
/// [`NEAR_VILLAGE_M`] of the camera's centre (the nearest), else the one with
/// the most residents (ties: the lower id). `None` on an island with none.
pub(crate) fn pick_village(world: &World, camera: Option<Vec3>) -> Option<SimId> {
    let villages: Vec<(SimId, Option<Vec3>)> = village::frontier::settlements::all(world)
        .into_iter()
        .map(|(id, entity)| {
            let centre = world
                .get::<npc_framework::prelude::SimPos>(entity)
                .map(|pos| npc_framework::view_helpers::sim_to_world(*pos));
            (id, centre)
        })
        .collect();
    let near = camera.and_then(|focus| {
        villages
            .iter()
            .filter_map(|(id, centre)| {
                let centre = (*centre)?;
                let gap = Vec2::new(centre.x - focus.x, centre.z - focus.z).length();
                (gap <= NEAR_VILLAGE_M).then_some((gap, *id))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, id)| id)
    });
    near.or_else(|| {
        let mut residents: BTreeMap<SimId, u32> = villages.iter().map(|(id, _)| (*id, 0)).collect();
        if let Some(index) = world.get_resource::<SimIndex>() {
            for entity in index.map.values() {
                if world.get::<Villager>(*entity).is_some()
                    && village::living(world, *entity)
                    && let Some(home) = village::frontier::settlements::member(world, *entity)
                    && let Some(count) = residents.get_mut(&home)
                {
                    *count += 1;
                }
            }
        }
        residents
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
            .map(|(id, _)| id)
    })
}

/// What open sites still wait for, by good.
fn site_needs(world: &World) -> BTreeMap<String, u32> {
    let mut needs = BTreeMap::new();
    if !crate::island_build::available(world) {
        return needs;
    }
    for site in village::civic::sites(world) {
        for (kind, wanted) in site.cost.iter() {
            let missing = wanted.saturating_sub(site.delivered.get(kind).copied().unwrap_or(0));
            if missing > 0 {
                *needs.entry(kind.clone()).or_default() += missing;
            }
        }
    }
    needs
}

/// What the Economy page shows: the village, Goods or Jobs, the chosen
/// good, the sorted column, and whether the column is the narrow one.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct View<'a> {
    /// The Goods table's scope; `None` is the whole island.
    pub(super) scope: Scope,
    /// The village Jobs and Priorities show; chosen again when it is gone.
    pub(super) village: Option<SimId>,
    /// The narrow ledger's good card has its fold open.
    pub(super) more: bool,
    /// The day's account shows all its lines.
    pub(super) day_more: bool,
    /// Priorities: the jobs that may get their own level are shown.
    pub(super) job_levels: bool,
    /// Orders sent from the ledger that wait or were refused.
    pub(super) sent: &'a [super::orders::SentOrder],
    pub(super) page: EconomyPage,
    pub(super) good: Option<&'a str>,
    pub(super) sort: Option<(u8, bool)>,
    pub(super) compact: bool,
}

/// A click on a header: sort by it, upwards for names and downwards for
/// numbers first; the same header again reverses.
pub(crate) fn next_sort(current: Option<(u8, bool)>, column: u8) -> Option<(u8, bool)> {
    match current {
        Some((sorted, ascending)) if sorted == column => Some((column, !ascending)),
        _ => Some((column, column == 0)),
    }
}

/// Order of two goods inside their group for a sorted column: 0 name,
/// 1 stock, 2 price, 3 days left, 4 change since yesterday. Ties go by name.
pub(crate) fn sort_goods(
    a: &StockRow,
    b: &StockRow,
    sort: Option<(u8, bool)>,
) -> std::cmp::Ordering {
    let change = |row: &StockRow| match row.trend {
        Trend::Rising(by) => i64::from(by),
        Trend::Falling(by) => -i64::from(by),
        Trend::Steady | Trend::Unknown => 0,
    };
    let by_name = a.label.cmp(&b.label);
    let Some((column, ascending)) = sort else {
        return by_name;
    };
    let order = match column {
        1 => a.stock.cmp(&b.stock),
        // By price, then by its change since yesterday.
        2 => a.price.cmp(&b.price).then_with(|| {
            let change = |row: &StockRow| {
                row.price
                    .zip(row.day_ago)
                    .map_or(0, |(now, then)| i64::from(now) - i64::from(then))
            };
            change(a).cmp(&change(b))
        }),
        3 => a.days.cmp(&b.days),
        4 => change(a).cmp(&change(b)),
        _ => by_name,
    };
    let order = if ascending { order } else { order.reverse() };
    order.then(by_name)
}

/// One call of the merchant boat, as the day's account says it:
/// `(goods bought, coins paid, goods sold, coins taken)`.
pub(crate) type VisitLine = (u32, u32, u32, u32);

/// The colony's day at a glance, for the Economy page: the civic fund, and
/// the merchant boat (where it is, what this or its last call traded, what
/// it wants next). Nothing about a boat in a world without one.
pub(crate) fn day_lines(
    fund: u32,
    boat: Option<(&str, Option<VisitLine>, Option<VisitLine>)>,
    orders: &[String],
) -> Vec<String> {
    let mut lines = vec![format!("The fund: {fund} coins.")];
    let Some((where_, this_visit, last_visit)) = boat else {
        return lines;
    };
    lines.push(where_.to_owned());
    let traded = |(bought, paid, sold, taken): VisitLine| {
        let goods = |count: u32| {
            if count == 1 {
                "1 good".to_owned()
            } else {
                format!("{count} goods")
            }
        };
        format!(
            "bought {} from the island for {paid} coins and sold {} for {taken} coins",
            goods(bought),
            goods(sold)
        )
    };
    if let Some(visit) = this_visit {
        lines.push(format!("This call so far: it {}.", traded(visit)));
    } else if let Some(visit) = last_visit {
        lines.push(format!("Its last call: it {}.", traded(visit)));
    }
    lines.extend(orders.iter().cloned());
    lines
}

/// The day's account of the colony for the Economy page, from the world:
/// its first two lines, the rest under a fold (closing phase, defect 11).
fn day_entries(world: &World, open: bool) -> Vec<Entry> {
    let fund = world
        .get_resource::<village::VillageState>()
        .map_or(0, |state| state.fund);
    let boat = village::view::boat(world);
    let visit = |totals: &Option<village::merchant::VisitTotals>| {
        totals.as_ref().map(|totals| {
            (
                totals.bought_units,
                totals.bought_coins,
                totals.sold_units,
                totals.sold_coins,
            )
        })
    };
    let where_ = if boat.docked {
        "The merchant boat is in at the trading post.".to_owned()
    } else {
        // In the top bar's calendar, never the running day count.
        format!(
            "The merchant boat's next call is {}.",
            crate::game_ui::words::when(world, boat.next_visit_tick)
        )
    };
    let orders = crate::island_shell::model::boat_order_lines(
        &crate::island_shell::boat_orders(&boat),
        boat.docked,
    );
    let lines = day_lines(
        fund,
        boat.enabled.then(|| {
            (
                where_.as_str(),
                visit(&boat.this_visit),
                visit(&boat.last_visit),
            )
        }),
        &orders,
    );
    let mut rows = vec![Entry::text("day_title", "Today on the island", true)];
    let more = lines.len().saturating_sub(DAY_SHOWN);
    rows.extend(
        lines
            .into_iter()
            .enumerate()
            .take(if open { usize::MAX } else { DAY_SHOWN })
            .map(|(index, line)| Entry::text(&format!("day_{index}"), line, false)),
    );
    if more > 0 {
        rows.push(
            Entry::action(
                "day_more",
                if open {
                    "Less of today".to_owned()
                } else {
                    format!("More of today ({more})")
                },
                Action::DayMore,
            )
            .with(super::model::Extra::Fold(open)),
        );
    }
    rows
}

/// Lines of the day's account shown before its fold.
const DAY_SHOWN: usize = 2;

/// Ledger rows for the Economy page: the day's account, the village filter,
/// the Goods or Jobs switch, then one table. Goods are grouped; a selected good has a detail
/// card under the table.
pub(super) fn entries(world: &World, view: View<'_>) -> Vec<Entry> {
    use super::model::Extra;
    let View {
        scope,
        village: chosen,
        more,
        day_more,
        job_levels,
        sent,
        page,
        good,
        sort,
        compact,
    } = view;
    use crate::game_ui::icon::Icon;
    use crate::game_ui::tokens::Status;
    let scopes = scopes(world);
    let goods = page == EconomyPage::Goods;
    // A settlement from another world is not a valid selection here.
    let scope = scopes
        .iter()
        .find(|(candidate, _)| *candidate == scope)
        .and_then(|(candidate, _)| *candidate);
    // Jobs and Priorities belong to one village, never the whole island.
    let chosen = chosen
        .filter(|id| scopes.iter().any(|(candidate, _)| *candidate == Some(*id)))
        .or_else(|| pick_village(world, None));
    let mut rows = day_entries(world, day_more);
    // Goods: the whole island and each village. Jobs and Priorities: the
    // villages only, and no row at all with one village.
    let chips: Vec<(String, Action)> = scopes
        .iter()
        .filter(|(candidate, _)| goods || candidate.is_some())
        .map(|(candidate, label)| (scope_choice(*candidate, label), Action::Scope(*candidate)))
        .collect();
    if chips.len() > 1 {
        rows.push(Entry::choices("stock_scopes", chips));
    }
    let mut pages = vec![
        ("Goods".to_owned(), Action::EconomyPage(EconomyPage::Goods)),
        ("Jobs".to_owned(), Action::EconomyPage(EconomyPage::Jobs)),
    ];
    pages.push((
        "Priorities".to_owned(),
        Action::EconomyPage(EconomyPage::Priorities),
    ));
    rows.push(Entry::choices("economy_view", pages));
    if !goods {
        if let Some(settlement) = chosen {
            // "Your orders" heads both pages of a village (2.7).
            let name = scopes
                .iter()
                .find(|(candidate, _)| *candidate == Some(settlement))
                .map_or("the village", |(_, label)| label.as_str());
            let table = oversee::profession_table(world, settlement);
            let priorities = oversee::priorities(world, settlement);
            rows.extend(super::orders::order_entries(
                settlement,
                name,
                super::orders::order_lines(
                    settlement,
                    &table,
                    &priorities,
                    sent,
                    crate::game_ui::words::paused(world),
                ),
            ));
            rows.extend(if page == EconomyPage::Jobs {
                target_entries(world, settlement)
            } else {
                priority_entries_with(world, settlement, &table, priorities, job_levels)
            });
        }
        return rows;
    }
    rows.push(
        Entry::table(
            "stock_header",
            // The narrow column has no room for the long word.
            [
                "Good",
                "Stock",
                "Price",
                "Days",
                if compact { "Change" } else { "Since yesterday" },
            ],
            None,
            true,
        )
        .with(Extra::SortHeader(sort)),
    );
    let needs = site_needs(world);
    let summary = village::colony::summary(world);
    let short_food = summary.residents > 0 && summary.food_units < summary.residents;
    let short_water = summary.residents > 0 && summary.water_units < summary.residents;
    let island = scope.is_none();
    let mut stock: Vec<(GoodGroup, StockRow)> = stock_rows(world, scope)
        .into_iter()
        .map(|row| {
            let edible = catalog::food_restore_in(world, &row.kind).is_some();
            let water = row.kind == catalog::WATER;
            (GoodGroup::of(&row.kind, edible, water), row)
        })
        .collect();
    // The three groups keep their order; a sorted column orders each inside.
    stock.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| sort_goods(&a.1, &b.1, sort)));
    let mut group = None;
    for (of, row) in &stock {
        if group != Some(*of) {
            group = Some(*of);
            rows.push(
                Entry::text(&format!("stock_group{}", *of as u8), of.label(), false)
                    .with(Extra::Group),
            );
        }
        let needed = needs.get(&row.kind).copied().unwrap_or(0);
        let water = row.kind == catalog::WATER;
        // The island's totals decide the shared rules; the days a stock
        // lasts mark a good low in a village too.
        let low = days_low(row.days)
            || island
                && good_low(
                    row.stock,
                    needed,
                    (water && short_water)
                        || (*of == GoodGroup::FoodAndWater && !water && short_food),
                );
        let mut entry = Entry::table(
            format!("stock_{}", row.kind),
            [
                row.label.clone(),
                row.stock.to_string(),
                super::goods::price_cell(row.price, row.day_ago),
                days_words(row.days),
                row.trend.text(),
            ],
            Some(row.trend),
            false,
        )
        .with_icon(Icon::of_good(&row.kind))
        .with_action(Action::Good(row.kind.clone()));
        if low {
            entry = entry.with(Extra::Badge(Status::Warn, "Low".to_owned()));
        }
        rows.push(entry);
        // The selected good's card sits right under its row (2.3).
        if good == Some(row.kind.as_str()) {
            rows.extend(good_card(world, row, scope, &scopes, &needs, compact, more));
        }
    }
    rows
}

/// The card of the selected good, under its row: the numbered lines of
/// design-ui 2.3, the price bars, and the links. The narrow ledger keeps
/// lines 2, 3, 7 and 8 in view and folds the rest.
fn good_card(
    world: &World,
    row: &StockRow,
    scope: Scope,
    scopes: &[(Scope, String)],
    needs: &BTreeMap<String, u32>,
    compact: bool,
    more: bool,
) -> Vec<Entry> {
    use super::goods;
    use super::model::Extra;
    use crate::game_ui::icon::Icon;
    let cache = world.get_resource::<goods::EconomyCache>();
    let price = cache.and_then(|cache| cache.prices.get(&row.kind));
    let flow = cache
        .filter(|cache| cache.has_flow(scope, &row.kind))
        .and_then(|cache| cache.flow.as_ref());
    // Lines by their number in the design; figures not read yet are left
    // out for one reconcile rather than shown as "loading".
    let mut lines: Vec<(u8, String)> = Vec::new();
    let mut links: Vec<(String, String, Action)> = Vec::new();
    let totals = stock_totals(world);
    let places: Vec<String> = scopes
        .iter()
        .filter(|(candidate, _)| candidate.is_some())
        .map(|(candidate, label)| {
            format!(
                "{label} {}",
                totals
                    .get(&(*candidate, row.kind.clone()))
                    .copied()
                    .unwrap_or(0)
            )
        })
        .collect();
    if places.len() > 1 {
        lines.push((1, places.join(" \u{b7} ")));
    }
    if cache.is_some() {
        match (price, row.price) {
            (Some(line), _) => {
                lines.push((2, goods::price_sentence(Some(line))));
                lines.push((3, goods::market_sentence(line.last_market.as_ref())));
                if let Some(below) = goods::below_sentence(line.last_market.as_ref()) {
                    lines.push((4, below));
                }
            }
            // Priced, but no market round of it yet: the price alone.
            (None, Some(coins)) => {
                lines.push((
                    2,
                    format!("Price {coins} coin{}.", if coins == 1 { "" } else { "s" }),
                ));
                lines.push((3, goods::market_sentence(None)));
            }
            (None, None) => lines.push((2, goods::price_sentence(None))),
        }
    }
    let spark = price.and_then(|price| goods::spark(&price.history));
    if let Some(flow) = flow {
        let job = |who: Option<SimId>| {
            let entity = world.resource::<SimIndex>().get(who?)?;
            let person = world.get::<Villager>(entity)?;
            Some(crate::game_ui::words::noun(
                &trade_display(world, person.role),
                2,
            ))
        };
        let made = |recipe: &str| {
            world
                .get_resource::<npc_framework::crafting::RecipeRegistry>()?
                .recipes
                .get(recipe)?
                .outputs
                .first()
                .map(|output| crate::game_ui::words::noun(&output.kind, 2))
        };
        let groups: Vec<(String, u32)> = flow
            .used_by
            .iter()
            .map(|line| (goods::user_group(line, &job, &made), line.wanted))
            .collect();
        lines.push((6, goods::used_by_sentence(&groups)));
        if let Some((line, who)) = goods::without_sentence(&flow.going_without) {
            lines.push((7, line));
            links.push((
                format!("stock_without_{}", row.kind),
                "Show them".to_owned(),
                Action::PeopleFilter(format!("Going without {}", row.label.to_lowercase()), who),
            ));
        }
        for line in goods::held_up_lines(&flow.held_up, &row.kind) {
            lines.push((8, line));
        }
    }
    // Who makes it: the simulation's producers, nearest workplaces first.
    let makers = village::view::producers(world, &row.kind);
    if makers.is_empty() {
        let job = scope
            .or_else(|| pick_village(world, None))
            .and_then(|village| {
                oversee::profession_table(world, village)
                    .into_iter()
                    .filter(|job| {
                        catalog::outputs_for(world, Role::from_key(&job.key))
                            .contains(&row.kind.as_str())
                    })
                    .max_by_key(|job| job.free_seats)
            });
        let (line, link) = goods::nobody_makes(
            job.as_ref()
                .map(|job| (job.display.as_str(), job.key.as_str(), job.free_seats)),
            scope,
        );
        lines.push((9, line));
        if let Some((label, action)) = link {
            links.push((format!("stock_maker_{}", row.kind), label, action));
        }
    } else {
        let names: Vec<String> = makers
            .iter()
            .take(4)
            .map(|maker| format!("{} ({})", maker.name, maker.role))
            .collect();
        let more = makers.len().saturating_sub(4);
        lines.push((
            9,
            format!(
                "Made by {}{}.",
                names.join(", "),
                if more > 0 {
                    format!(" and {more} more")
                } else {
                    String::new()
                }
            ),
        ));
    }
    if row.days.is_some()
        && let Some((per_day, _)) = day_use(world, scope).get(&row.kind).copied()
        && per_day > 0
    {
        lines.push((
            10,
            format!(
                "About {per_day} used a day: {} day{} left.",
                days_words(row.days),
                if row.days == Some(1) { "" } else { "s" }
            ),
        ));
    }
    match needs.get(&row.kind).copied().unwrap_or(0) {
        0 => {}
        needed => lines.push((11, format!("Open sites still need {needed}."))),
    }
    if let Some(by) = trend_words(row.trend) {
        lines.push((12, by));
    }
    let icon = Icon::of_good(&row.kind);
    // Lines 3 and 8 say more on hover (2.3).
    let mut tips: BTreeMap<u8, crate::game_ui::tooltip::Tooltip> = BTreeMap::new();
    if let Some(round) = price.and_then(|line| line.last_market.as_ref()) {
        tips.insert(
            3,
            crate::game_ui::tooltip::Tooltip::new(
                "Last market",
                goods::market_tip(&crate::game_ui::words::date(world, round.tick), &{
                    let permille = world
                        .get_resource::<npc_framework::prelude::SimConfig>()
                        .map_or(0, |config| config.time_of_day_permille(round.tick));
                    crate::game_ui::words::clock_face(permille)
                }),
            ),
        );
    }
    if let Some(since) =
        flow.and_then(|flow| flow.held_up.iter().filter_map(|line| line.since_tick).min())
    {
        tips.insert(
            8,
            crate::game_ui::tooltip::Tooltip::new(
                "Held up",
                format!("Since {}.", crate::game_ui::words::moment(world, since)),
            ),
        );
    }
    // The first card carries the good's name and picture; a card that
    // continues it carries neither.
    let card = |key: &str, lines: Vec<(u8, String)>| {
        let first = key == "stock_detail";
        let mut entry = Entry::text(
            key,
            if first {
                row.label.clone()
            } else {
                String::new()
            },
            false,
        )
        .with_icon(if first { icon } else { None });
        entry.line_tips = lines
            .iter()
            .enumerate()
            .filter_map(|(index, (number, _))| Some((index, tips.get(number)?.clone())))
            .collect();
        entry.with(Extra::Card(
            lines.into_iter().map(|(_, line)| line).collect(),
        ))
    };
    let spark_entry = spark.map(|(heights, words)| {
        Entry::text(&format!("stock_spark_{}", row.kind), words, false).with(Extra::Spark(heights))
    });
    let link_entries = links
        .into_iter()
        .map(|(key, label, action)| Entry::link(key, label, Icon::Locate, action));
    let mut out = Vec::new();
    if compact {
        let (kept, rest): (Vec<_>, Vec<_>) = lines
            .into_iter()
            .partition(|(number, _)| goods::COMPACT_LINES.contains(number));
        out.push(card("stock_detail", kept));
        out.extend(link_entries);
        out.push(
            Entry::action(
                format!("stock_more_{}", row.kind),
                format!("More about {}", row.label.to_lowercase()),
                Action::GoodMore,
            )
            .with(Extra::Fold(more)),
        );
        if more {
            out.push(card("stock_detail_more", rest));
            out.extend(spark_entry);
        }
    } else {
        // Lines 1 to 4, the bars (line 5), then the rest.
        let (head, tail): (Vec<_>, Vec<_>) = lines.into_iter().partition(|(number, _)| *number < 5);
        out.push(card("stock_detail", head));
        out.extend(spark_entry);
        if !tail.is_empty() {
            out.push(card("stock_detail_rest", tail));
        }
        out.extend(link_entries);
    }
    out
}

/// A dash for "the village decides", else the number.
pub(crate) fn wanted_words(wanted: Option<u32>) -> String {
    wanted.map_or_else(|| "\u{2013}".to_owned(), |wanted| wanted.to_string())
}

/// A village's Jobs table: each trade of the catalog with who works it now,
/// how many the overseer wants and its open places; under each adjustable
/// trade its controls (fewer, more, the village decides), and the
/// simulation's reason when now and wanted differ.
pub(crate) fn target_entries(world: &World, settlement: SimId) -> Vec<Entry> {
    use crate::game_ui::icon::Icon;
    let mut rows = vec![Entry::table(
        "targets_header",
        ["Job", "Now", "Wanted", "Open"],
        None,
        true,
    )];
    let table = oversee::profession_table(world, settlement);
    for row in &table {
        let key = format!("target_{}_{}", settlement.0, row.key);
        rows.push(
            Entry::table(
                &key,
                [
                    capitalised(&row.display),
                    row.current.to_string(),
                    wanted_words(row.wanted),
                    if row.free_seats == 0 {
                        "\u{2013}".into()
                    } else {
                        row.free_seats.to_string()
                    },
                ],
                None,
                false,
            )
            .with_icon(Some(
                Icon::of_trade(&row.key)
                    .or_else(|| Icon::of_trade(&row.display))
                    .unwrap_or(Icon::Work),
            )),
        );
        // How the job stands against the player's number is said once, in
        // "Your orders" above the table.
        if row.adjustable {
            rows.push(Entry::choices(
                &format!("{key}_set"),
                [
                    ("\u{2212}", TargetStep::Fewer),
                    ("+", TargetStep::More),
                    ("Village decides", TargetStep::VillageDecides),
                ]
                .into_iter()
                .map(|(label, step)| {
                    (
                        label.to_owned(),
                        Action::Target {
                            settlement,
                            profession: row.key.clone(),
                            step,
                        },
                    )
                })
                .collect(),
            ));
        } else {
            // Why there are no controls: the simulation's own refusal.
            let why = target_from_row(world, settlement, row, TargetStep::More)
                .err()
                .unwrap_or_else(|| "The village staffs this job itself.".to_owned());
            rows.push(Entry::caption(&format!("{key}_fixed"), why));
        }
    }
    let wanted: Vec<&oversee::ProfessionRowView> =
        table.iter().filter(|row| row.wanted.is_some()).collect();
    rows.push(Entry::text(
        "targets_total",
        if wanted.is_empty() {
            "The village decides every job. Use + and \u{2212} to ask for more or fewer of one."
                .to_owned()
        } else {
            format!(
                "You set {} of {} jobs; the village decides the rest.",
                wanted.len(),
                table.len()
            )
        },
        false,
    ));
    rows
}

/// The figure a priority's title carries (3.9): "4 days in store", "11
/// residents without". `None` when the figure is not known; Storage has none.
pub(crate) fn topic_figure(
    topic: &str,
    outlook: Option<&village::view::Outlook>,
    open_places: u32,
    boat_days: Option<u64>,
) -> Option<String> {
    let plural = |count: u32, one: &str, many: &str| {
        format!("{count} {}", if count == 1 { one } else { many })
    };
    match topic {
        "food_reserve" => outlook?
            .food_days
            .map(|days| format!("{} in store", plural(days, "day", "days"))),
        "fuel_reserve" => outlook?
            .fuel_days
            .map(|days| format!("{} in store", plural(days, "day", "days"))),
        "coats" => Some(format!(
            "{} without",
            plural(outlook?.without_coat, "resident", "residents")
        )),
        "housing" => Some(format!(
            "{} without a home",
            plural(outlook?.homeless, "resident", "residents")
        )),
        "workplace" => Some(plural(open_places, "open place", "open places")),
        "boat_orders" => boat_days.map(|days| match days {
            0 => "the boat is in".to_owned(),
            1 => "next call tomorrow".to_owned(),
            days => format!("next call in {days} days"),
        }),
        _ => None,
    }
}

fn level_choices(
    settlement: SimId,
    topic: &'static str,
    profession: Option<String>,
) -> Vec<(String, Action)> {
    [
        oversee::PriorityLevel::Low,
        oversee::PriorityLevel::Normal,
        oversee::PriorityLevel::High,
    ]
    .into_iter()
    .map(|level| {
        (
            level_label(level).to_owned(),
            Action::Priority {
                settlement,
                topic,
                profession: profession.clone(),
                level,
            },
        )
    })
    .collect()
}

/// A village's priorities, its tables already read: each topic with Low,
/// Normal and High, its figure in the title, and the simulation's sentence
/// of what the level changes; under Workplaces the way to give one job its
/// own level (3.9). The whole island has none of its own.
pub(crate) fn priority_entries_with(
    world: &World,
    settlement: SimId,
    table: &[oversee::ProfessionRowView],
    priorities: Vec<oversee::PriorityRowView>,
    job_levels: bool,
) -> Vec<Entry> {
    use super::model::Extra;
    let mut rows = vec![Entry::caption(
        "priorities_intro",
        "Normal is how the village decides by itself. High sees to a thing first. Low leaves it for last.",
    )];
    let outlook = world
        .get_resource::<super::goods::EconomyCache>()
        .and_then(|cache| cache.outlook_of(settlement));
    let open_places: u32 = table.iter().map(|row| row.free_seats).sum();
    let boat = village::view::boat(world);
    let boat_days = boat.enabled.then(|| {
        if boat.docked {
            return 0;
        }
        let now = world
            .get_resource::<SimClock>()
            .map_or(0, |clock| clock.tick);
        let day = world
            .get_resource::<npc_framework::prelude::SimConfig>()
            .map_or(1, |config| config.day_length.max(1));
        boat.next_visit_tick.saturating_sub(now).div_ceil(day)
    });
    let own: Vec<String> = priorities
        .iter()
        .filter_map(|row| row.profession.clone())
        .collect();
    for row in priorities {
        let key = format!(
            "priority_{}_{}_{}",
            settlement.0,
            row.topic,
            row.profession.as_deref().unwrap_or("all")
        );
        let figure = topic_figure(row.topic, outlook, open_places, boat_days);
        let title = match (&row.profession, figure) {
            (None, Some(figure)) => format!("{} \u{b7} {figure}", row.display),
            _ => row.display.clone(),
        };
        rows.push(Entry::strong(&format!("{key}_title"), title));
        let mut choices = level_choices(settlement, row.topic, row.profession.clone());
        if let Some(job) = &row.profession {
            // A job's own level goes back to the level of all workplaces.
            choices.push((
                "Same as the others".to_owned(),
                Action::Priority {
                    settlement,
                    topic: row.topic,
                    profession: Some(job.clone()),
                    level: oversee::PriorityLevel::Normal,
                },
            ));
        }
        rows.push(Entry::choices(&key, choices));
        rows.push(Entry::caption(
            &format!("{key}_effect"),
            crate::game_ui::words::sentence(&row.effect),
        ));
        if row.topic == "workplace" && row.profession.is_none() {
            rows.push(
                Entry::action(
                    format!("{key}_one_job"),
                    "Give one job its own level",
                    Action::JobLevels,
                )
                .with(Extra::Fold(job_levels)),
            );
            if job_levels {
                // A chip puts that job's workplaces first: its own level, High.
                let chips: Vec<(String, Action)> = table
                    .iter()
                    .filter(|job| !own.contains(&job.key))
                    .map(|job| {
                        (
                            super::orders::jobs(&job.display),
                            Action::Priority {
                                settlement,
                                topic: "workplace",
                                profession: Some(job.key.clone()),
                                level: oversee::PriorityLevel::High,
                            },
                        )
                    })
                    .collect();
                if !chips.is_empty() {
                    rows.push(Entry::choices(&format!("{key}_jobs"), chips));
                }
            }
        }
    }
    rows
}

/// "Up 7 since yesterday." for the detail card.
fn trend_words(trend: Trend) -> Option<String> {
    Some(match trend {
        Trend::Rising(by) => format!("Stock up {by} since yesterday."),
        Trend::Falling(by) => format!("Stock down {by} since yesterday."),
        Trend::Steady => "Stock the same as yesterday.".to_owned(),
        Trend::Unknown => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_steps_build_on_the_target_or_on_who_works_now() {
        // The village decides: the first press starts from who works it.
        assert_eq!(next_wanted(3, None, TargetStep::More), Some(4));
        assert_eq!(next_wanted(3, None, TargetStep::Fewer), Some(2));
        // A target set: the press moves the target, not the headcount.
        assert_eq!(next_wanted(3, Some(1), TargetStep::More), Some(2));
        assert_eq!(next_wanted(0, Some(0), TargetStep::Fewer), Some(0));
        assert_eq!(
            next_wanted(9, Some(oversee::MAX_TARGET), TargetStep::More),
            Some(oversee::MAX_TARGET)
        );
        assert_eq!(next_wanted(3, Some(5), TargetStep::VillageDecides), None);
        assert_eq!(wanted_words(None), "\u{2013}");
        assert_eq!(wanted_words(Some(2)), "2");
    }

    #[test]
    fn the_days_account_says_the_fund_and_the_boats_trade() {
        assert_eq!(day_lines(40, None, &[]), ["The fund: 40 coins."]);
        let orders = ["Next call wants 10 planks at 14 coins each.".to_owned()];
        assert_eq!(
            day_lines(
                40,
                Some((
                    "The merchant boat's next call is on day 9.",
                    None,
                    Some((12, 80, 1, 9))
                )),
                &orders
            ),
            [
                "The fund: 40 coins.",
                "The merchant boat's next call is on day 9.",
                "Its last call: it bought 12 goods from the island for 80 coins and sold 1 good for 9 coins.",
                "Next call wants 10 planks at 14 coins each."
            ]
        );
        // In port, this call's trade so far.
        let lines = day_lines(
            0,
            Some((
                "The merchant boat is in at the trading post.",
                Some((3, 20, 0, 0)),
                Some((12, 80, 1, 9)),
            )),
            &[],
        );
        assert_eq!(
            lines[2],
            "This call so far: it bought 3 goods from the island for 20 coins and sold 0 goods for 0 coins."
        );
    }

    fn history(values: &[u32]) -> VecDeque<u32> {
        values.iter().copied().collect()
    }

    #[test]
    fn a_header_sorts_its_column_inside_the_groups_and_again_reverses() {
        let row = |label: &str, stock, price, trend| StockRow {
            kind: label.to_lowercase(),
            label: label.to_owned(),
            stock,
            price,
            trend,
            days: None,
            day_ago: None,
        };
        let mut rows = vec![
            row("Planks", 11, Some(2), Trend::Rising(7)),
            row("Logs", 105, Some(1), Trend::Steady),
            row("Ore", 24, None, Trend::Falling(4)),
        ];
        let names =
            |rows: &[StockRow]| rows.iter().map(|row| row.label.clone()).collect::<Vec<_>>();
        rows.sort_by(|a, b| sort_goods(a, b, None));
        assert_eq!(
            names(&rows),
            ["Logs", "Ore", "Planks"],
            "by name until a header is chosen"
        );
        // Numbers start with the most; names start with A.
        assert_eq!(next_sort(None, 1), Some((1, false)));
        assert_eq!(next_sort(None, 0), Some((0, true)));
        assert_eq!(
            next_sort(Some((1, false)), 1),
            Some((1, true)),
            "again reverses"
        );
        assert_eq!(
            next_sort(Some((1, true)), 3),
            Some((3, false)),
            "another column starts over"
        );
        rows.sort_by(|a, b| sort_goods(a, b, Some((1, false))));
        assert_eq!(names(&rows), ["Logs", "Ore", "Planks"]);
        rows.sort_by(|a, b| sort_goods(a, b, Some((1, true))));
        assert_eq!(names(&rows), ["Planks", "Ore", "Logs"]);
        rows.sort_by(|a, b| sort_goods(a, b, Some((4, false))));
        assert_eq!(
            names(&rows),
            ["Planks", "Logs", "Ore"],
            "the biggest rise first"
        );
        rows.sort_by(|a, b| sort_goods(a, b, Some((2, true))));
        assert_eq!(
            names(&rows),
            ["Ore", "Logs", "Planks"],
            "no price sorts as the lowest"
        );
    }

    #[test]
    fn trend_needs_two_samples_and_ignores_noise() {
        assert_eq!(trend(&history(&[])), Trend::Unknown);
        assert_eq!(trend(&history(&[5])), Trend::Unknown);
        assert_eq!(trend(&history(&[40, 43])), Trend::Steady, "under a tenth");
        assert_eq!(trend(&history(&[40, 44])), Trend::Rising(4));
        assert_eq!(trend(&history(&[40, 60, 12])), Trend::Falling(28));
        assert_eq!(trend(&history(&[0, 1])), Trend::Rising(1));
        assert_eq!(trend(&history(&[3, 3, 3])), Trend::Steady);
        // In words beside the arrow: the font has no arrow glyphs.
        assert_eq!(Trend::Rising(4).text(), "up 4");
        assert_eq!(Trend::Falling(28).text(), "down 28");
        assert_eq!(Trend::Steady.text(), "steady");
        assert_eq!(
            trend_words(Trend::Falling(2)).as_deref(),
            Some("Stock down 2 since yesterday.")
        );
        assert_eq!(trend_words(Trend::Unknown), None);
    }

    #[test]
    fn goods_fall_into_three_groups_and_low_follows_the_bar_rules() {
        assert_eq!(GoodGroup::of("bread", true, false), GoodGroup::FoodAndWater);
        assert_eq!(GoodGroup::of("water", false, true), GoodGroup::FoodAndWater);
        assert_eq!(
            GoodGroup::of("village_plank", false, false),
            GoodGroup::Materials
        );
        assert_eq!(GoodGroup::of("ore", false, false), GoodGroup::Materials);
        assert_eq!(
            GoodGroup::of("pickaxe", false, false),
            GoodGroup::ToolsAndClothing
        );
        assert_eq!(
            GoodGroup::of("coat", false, false),
            GoodGroup::ToolsAndClothing
        );
        // An unknown good is a material, never dropped.
        assert_eq!(GoodGroup::of("mystery", false, false), GoodGroup::Materials);
        assert!(GoodGroup::FoodAndWater < GoodGroup::Materials);
        assert_eq!(GoodGroup::ToolsAndClothing.label(), "Tools and clothing");
        // Low: open sites need more than the island holds, or the group is short.
        assert!(good_low(11, 13, false));
        assert!(!good_low(13, 13, false));
        assert!(!good_low(0, 0, false), "nothing asks for it: not low");
        assert!(good_low(500, 0, true));
        // A day's use decides too: under two days left is low.
        assert!(days_low(Some(1)) && days_low(Some(0)));
        assert!(!days_low(Some(2)) && !days_low(None));
        assert_eq!(days_words(Some(3)), "3");
        assert_eq!(days_words(Some(140)), "99+");
        assert_eq!(days_words(None), "\u{2013}");
    }

    #[test]
    fn samples_are_rate_limited_bounded_and_scoped_to_the_epoch() {
        let mut trends = StockTrends::default();
        let key = (None, "village_flour".to_owned());
        let totals = |value: u32| BTreeMap::from([(key.clone(), value)]);
        assert!(trends.due(1, 0));
        trends.record(0, &totals(10));
        assert!(!trends.due(1, SAMPLE_TICKS - 1), "not before the interval");
        assert!(trends.due(1, SAMPLE_TICKS));
        trends.record(SAMPLE_TICKS, &totals(14));
        assert_eq!(trends.trend(None, "village_flour"), Trend::Rising(4));
        assert_eq!(
            trends.trend(Some(SimId(3)), "village_flour"),
            Trend::Unknown
        );

        // A good nobody holds any more is sampled as zero.
        trends.record(2 * SAMPLE_TICKS, &BTreeMap::new());
        assert_eq!(trends.trend(None, "village_flour"), Trend::Falling(10));

        for step in 3..40 {
            trends.record(step * SAMPLE_TICKS, &totals(7));
        }
        assert_eq!(trends.samples[&key].len(), HISTORY);
        assert_eq!(trends.trend(None, "village_flour"), Trend::Steady);

        // A new world, or a rewound clock, never inherits the old history.
        assert!(trends.due(2, 5));
        assert_eq!(trends.trend(None, "village_flour"), Trend::Unknown);
        trends.record(5000, &totals(1));
        assert!(trends.due(2, 100), "the clock moved backwards");
        assert_eq!(trends.trend(None, "village_flour"), Trend::Unknown);
    }
}
