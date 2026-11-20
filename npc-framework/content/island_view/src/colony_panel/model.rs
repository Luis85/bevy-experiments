//! Read-only colony ledger projections, separate from widgets and control ownership.
use super::{Action, Ledger, Tab};
use crate::IslandMode;
use crate::game_ui::icon::Icon;
use crate::game_ui::tokens::Status;
use crate::game_ui::tooltip::Tooltip;
use bevy::prelude::*;
use island_content::village::{self, Role, VillageState, Villager, colony};
use npc_framework::prelude::{SimClock, SimId, SimIndex};
use std::collections::BTreeSet;

pub(crate) struct Entry {
    pub(crate) key: String,
    pub(crate) text: String,
    pub(crate) action: Option<Action>,
    pub(crate) heading: bool,
    /// A table row: one string per column instead of `text`.
    pub(crate) columns: Option<Vec<String>>,
    /// Trend drawn in a table row's last column.
    pub(crate) trend: Option<super::economy::Trend>,
    /// A wrapping row of small buttons instead of one full-width control.
    pub(crate) choices: Option<Vec<(String, Action)>>,
    /// A roster row: a shape for the resident's most pressing need.
    pub(crate) indicator: Option<crate::resident_inspector::model::NeedLevel>,
    /// A journal row: the shape of its severity.
    pub(crate) shape: Option<Status>,
    /// A table row: the picture of its good.
    pub(crate) icon: Option<Icon>,
    /// Stat chips in a grid: picture, label, value.
    pub(crate) stats: Option<Vec<(Icon, String, String)>>,
    /// Another kind of row, or a detail of this one.
    pub(crate) extra: Option<Extra>,
    /// What the row says about itself on hover.
    pub(crate) tip: Option<Tooltip>,
    /// A card's lines that say more on hover: line index and its tooltip.
    pub(crate) line_tips: Vec<(usize, Tooltip)>,
}

/// Rows beyond text, actions and tables. The row's own `text` is its label
/// or title and `icon` its picture.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Extra {
    /// A bar with a status: a need. Percent filled and the value in words.
    Meter {
        percent: u8,
        status: Status,
        value: String,
    },
    /// Small chips in a wrapping row.
    Chips(Vec<(Option<Icon>, String)>),
    /// A card of facts under the row's title.
    Card(Vec<String>),
    /// A caption band between the rows of a table.
    Group,
    /// A heading that folds what follows; `true` while open.
    Fold(bool),
    /// A badge after a table row's name.
    Badge(Status, String),
    /// A day strip: `(share in percent, colour)` segments and where now is.
    Strip(Vec<(f32, [f32; 3])>, f32),
    /// An open building site: progress, materials and what it waits for.
    Site {
        percent: u8,
        materials: Vec<(Option<Icon>, String)>,
        status: String,
    },
    /// An action that leads somewhere: quiet, with an arrow.
    Link,
    /// A line in full ink.
    Strong,
    /// A caption line.
    Caption,
    /// On a table header: every column sorts. The sorted column and
    /// whether it runs upwards.
    SortHeader(Option<(u8, bool)>),
    /// Bars of a good's last market prices, heights in pixels (2.3); the
    /// row's text says the same in words.
    Spark(Vec<u32>),
    /// A line of "Your orders" (2.7): its state and its button.
    Order(super::orders::OrderStatus, Option<(String, Action)>),
}
impl Entry {
    pub(crate) fn text(key: &str, value: impl Into<String>, heading: bool) -> Self {
        Self {
            key: key.into(),
            text: value.into(),
            action: None,
            heading,
            columns: None,
            trend: None,
            choices: None,
            indicator: None,
            shape: None,
            icon: None,
            stats: None,
            extra: None,
            tip: None,
            line_tips: Vec::new(),
        }
    }
    pub(crate) fn choices(key: &str, choices: Vec<(String, Action)>) -> Self {
        Self {
            choices: Some(choices),
            ..Self::text(key, "", false)
        }
    }
    pub(crate) fn action(key: impl Into<String>, label: impl Into<String>, action: Action) -> Self {
        Self {
            key: key.into(),
            text: label.into(),
            action: Some(action),
            heading: false,
            columns: None,
            trend: None,
            choices: None,
            indicator: None,
            shape: None,
            icon: None,
            stats: None,
            extra: None,
            tip: None,
            line_tips: Vec::new(),
        }
    }
    pub(crate) fn stats(key: &str, stats: Vec<(Icon, &str, String)>) -> Self {
        Self {
            stats: Some(
                stats
                    .into_iter()
                    .map(|(icon, label, value)| (icon, label.to_owned(), value))
                    .collect(),
            ),
            ..Self::text(key, "", false)
        }
    }
    /// A journal row: an action with the shape of its severity in front.
    pub(crate) fn marked(
        key: impl Into<String>,
        text: impl Into<String>,
        shape: Status,
        action: Action,
    ) -> Self {
        Self {
            shape: Some(shape),
            ..Self::action(key, text, action)
        }
    }
    pub(crate) fn with_icon(mut self, icon: Option<Icon>) -> Self {
        self.icon = icon;
        self
    }
    pub(crate) fn with(mut self, extra: Extra) -> Self {
        self.extra = Some(extra);
        self
    }
    pub(crate) fn with_action(mut self, action: Action) -> Self {
        self.action = Some(action);
        self
    }
    pub(crate) fn with_tip(mut self, tip: Tooltip) -> Self {
        self.tip = Some(tip);
        self
    }
    /// A line in full ink.
    pub(crate) fn strong(key: &str, value: impl Into<String>) -> Self {
        Self::text(key, value, false).with(Extra::Strong)
    }
    pub(crate) fn caption(key: &str, value: impl Into<String>) -> Self {
        Self::text(key, value, false).with(Extra::Caption)
    }
    /// A link: an action that leads to another place or page.
    pub(crate) fn link(
        key: impl Into<String>,
        label: impl Into<String>,
        icon: Icon,
        action: Action,
    ) -> Self {
        Self::action(key, label, action)
            .with_icon(Some(icon))
            .with(Extra::Link)
    }
    /// A roster row: an action with a need indicator in front of its text.
    pub(crate) fn resident(row: &super::roster::RosterRow) -> Self {
        Self {
            indicator: Some(row.need),
            ..Self::action(
                format!("resident{}", row.id.0),
                row.text(),
                Action::Resident(row.id),
            )
        }
    }
    pub(crate) fn table(
        key: impl Into<String>,
        columns: impl IntoIterator<Item = impl Into<String>>,
        trend: Option<super::economy::Trend>,
        heading: bool,
    ) -> Self {
        let columns: Vec<String> = columns.into_iter().map(Into::into).collect();
        Self {
            key: key.into(),
            text: columns.join(" "),
            action: None,
            heading,
            columns: Some(columns),
            trend,
            choices: None,
            indicator: None,
            shape: None,
            icon: None,
            stats: None,
            extra: None,
            tip: None,
            line_tips: Vec::new(),
        }
    }
}
pub(crate) fn entries(world: &World) -> Vec<Entry> {
    let ledger = world.resource::<Ledger>();
    // The season-end card stands in the column in place of the page.
    if let Some(card) = ledger
        .season_card
        .and_then(|season| season_card(world, season))
    {
        return super::season::season_entries(&card);
    }
    let summary = colony::summary(world);
    match ledger.tab {
        Tab::Overview => village_entries(world),
        Tab::Projects => {
            let mut rows = project_entries(world, ledger.confirm_site, &ledger.unfolded);
            rows.extend(commission_entries(world, &summary));
            rows
        }
        Tab::Journal => {
            let mut rows = Vec::new();
            if let Some(season) = super::last_season(world)
                && let Some(summary) = village::view::season_summary(world, season)
            {
                rows.push(Entry::link(
                    "journal_season",
                    format!("{} ended. See the season.", summary.name),
                    Icon::Hint,
                    Action::SeasonOpen(season),
                ));
            }
            rows.extend(journal_entries(world));
            rows
        }
        Tab::Residents => resident_entries(world),
        Tab::Economy => super::economy::entries(
            world,
            super::economy::View {
                scope: ledger.scope,
                village: ledger.village_scope,
                more: ledger.good_more,
                day_more: ledger.day_more,
                job_levels: ledger.job_levels,
                sent: &ledger.sent,
                page: ledger.economy,
                good: ledger.good.as_deref(),
                sort: ledger.goods_sort,
                compact: crate::game_ui::scale::layout(world).class
                    == crate::game_ui::scale::LayoutClass::Compact,
            },
        ),
        // Developer launches only: the palette on the left owns the tools.
        Tab::Landscape => {
            let tools = world.get_resource::<crate::shape_tools::ShapeTools>();
            let mut rows = vec![
                Entry::text("landscape_title", "Shape the island", true),
                Entry::text(
                    "landscape_intro",
                    if *world.resource::<IslandMode>() == IslandMode::Editor {
                        "Use the tool palette on the left: choose a tool, then drag across the land. A red outline means the edit is blocked."
                    } else {
                        "Choose Terraform above to shape the world. Inspection never changes terrain."
                    },
                    false,
                ),
                Entry::text(
                    "cursor",
                    tools.and_then(|tools| tools.hover).map_or_else(
                        || "Move the pointer over the island.".into(),
                        |(x, z)| format!("Cell {x}, {z}"),
                    ),
                    false,
                ),
            ];
            if let Some(error) = tools.and_then(|tools| tools.cursor_reason()) {
                rows.push(Entry::text(
                    "guard",
                    format!("Cannot edit here: {error}"),
                    false,
                ));
            }
            rows.push(Entry::text("protection", "Homes, workplaces and occupied cells are protected. Edits cannot sever the village's routes. Terrain is stored with the world.", false));
            rows
        }
    }
}
/// The Village page: the colony at a glance, its policies and its villages.
fn village_entries(world: &World) -> Vec<Entry> {
    let state = world.resource::<colony::ColonyState>();
    let village = world.resource::<VillageState>();
    let summary = colony::summary(world);
    let each = |units: u32| {
        if summary.residents == 0 {
            "0".to_owned()
        } else {
            format!("{:.1}", units as f32 / summary.residents as f32)
        }
    };
    let mut rows = vec![
        Entry::stats(
            "village_stats",
            vec![
                (Icon::Residents, "Residents", summary.residents.to_string()),
                (
                    Icon::Homes,
                    "Housed",
                    format!("{} of {}", summary.housed, summary.residents),
                ),
                (Icon::Rest, "Beds", summary.beds.to_string()),
                (
                    Icon::Morale,
                    "Morale",
                    format!("{}%", summary.average_morale / 1000),
                ),
                (Icon::Food, "Food per resident", each(summary.food_units)),
                (
                    Icon::CleanWater,
                    "Water per resident",
                    each(summary.water_units),
                ),
            ],
        ),
        Entry::text(
            "homes",
            format!(
                "{} households · {} meetings · {} sales",
                summary.households, summary.social_events, village.transactions
            ),
            false,
        ),
    ];
    rows.extend(census_entries(
        &village::view::census(world),
        summary.households,
    ));
    rows.extend(year_entry(village::view::yearly_census(world).last()));
    rows.extend(inn_entries(&village::view::inns(world)));
    rows.extend([
        Entry::text("policy_title", "Village rules", true),
        Entry::text(
            "policy",
            format!("Household food reserve: {} days", state.food_reserve_days),
            false,
        ),
        Entry::choices(
            "reserve",
            (1..=4)
                .map(|days| {
                    (
                        format!("{days} day{}", if days == 1 { "" } else { "s" }),
                        Action::Reserve(days),
                    )
                })
                .collect(),
        ),
        Entry::action(
            "gatherings",
            if state.gatherings {
                "Evening gatherings: enabled"
            } else {
                "Evening gatherings: disabled"
            },
            Action::Gatherings,
        ),
    ]);
    rows.push(Entry::text("regions_title", "Across the island", true));
    for (id, entity) in village::frontier::settlements::all(world) {
        let place = world
            .get::<village::frontier::Settlement>(entity)
            .expect("settlement");
        let residents = world
            .resource::<SimIndex>()
            .map
            .values()
            .filter(|actor| {
                village::frontier::settlements::member(world, **actor) == Some(id)
                    && world.get::<Villager>(**actor).is_some()
                    && village::living(world, **actor)
            })
            .count();
        rows.push(Entry::action(
            format!("region{}", id.0),
            format!("{} · {residents} residents", place.label),
            Action::Settlement(id),
        ));
    }
    rows
}

/// The inns and their guest beds; each opens the inn's card. Nothing on an
/// island without an inn.
pub(crate) fn inn_entries(inns: &[village::view::InnView]) -> Vec<Entry> {
    if inns.is_empty() {
        return Vec::new();
    }
    let mut rows = vec![Entry::text("inns_title", "Inns", true)];
    for inn in inns {
        let taken = inn.beds.iter().filter(|bed| bed.guest.is_some()).count();
        rows.push(Entry::link(
            format!("inn{}", inn.inn.0),
            format!(
                "{} \u{b7} {taken} of {} guest beds taken",
                crate::island_build::model::workplace_title(&inn.label, "innkeeper", None),
                inn.capacity
            ),
            Icon::Inn,
            Action::Site(inn.inn),
        ));
    }
    rows
}

/// Housing commissions of the older island: a named resident builds a
/// cottage on an open plot.
/// How many residents are building a home of their own. The village's own
/// sites are the rows above this section; this line never counts them.
pub(crate) fn commissions_line(count: u32) -> String {
    match count {
        0 => "No resident is building a home of their own.".to_owned(),
        1 => "1 resident is building a home of their own.".to_owned(),
        count => format!("{count} residents are building homes of their own."),
    }
}

/// What a resident is qualified for and where their job stands, a fact per
/// line. The line about reconsidering is left out when there is no wait.
pub(crate) fn employment_text(
    qualified: &str,
    reason: &str,
    reconsiders_in: Option<String>,
    requested: Option<&str>,
) -> String {
    let mut lines = vec![format!("Qualified for: {qualified}"), reason.to_owned()];
    if let Some(span) = reconsiders_in {
        lines.push(format!("They will think about their job again in {span}."));
    }
    if let Some(place) = requested {
        lines.push(format!(
            "They will move to {place} when their work is done."
        ));
    }
    lines.join("\n")
}

/// A span of game time in words: "about 2 hours", "about 20 minutes".
/// `None` for no time at all.
pub(crate) fn span_words(ticks: u64, day_length: u64) -> Option<String> {
    if ticks == 0 {
        return None;
    }
    let minutes = ticks * 24 * 60 / day_length.max(1);
    Some(if minutes >= 90 {
        let hours = (minutes + 30) / 60;
        format!("about {hours} hours")
    } else if minutes >= 55 {
        "about an hour".to_owned()
    } else if minutes >= 10 {
        format!("about {} minutes", (minutes + 2) / 5 * 5)
    } else {
        "a few minutes".to_owned()
    })
}

/// This year's births, deaths and arrivals, from the simulation's tally of
/// the current year; nothing before the tally has a year.
pub(crate) fn year_entry(year: Option<&village::demography::YearTally>) -> Option<Entry> {
    let year = year?;
    let plural = |count: u32, one: &str, many: &str| {
        format!("{count} {}", if count == 1 { one } else { many })
    };
    Some(Entry::text(
        "census_year",
        format!(
            "This year (year {}): {} \u{b7} {} \u{b7} {}.",
            year.year + 1,
            plural(year.births, "birth", "births"),
            plural(year.arrivals, "arrival", "arrivals"),
            plural(year.deaths, "death", "deaths"),
        ),
        false,
    ))
}

/// The families block of the Village page, from the simulation's census:
/// who lives here and what has changed. Empty in a world the census does
/// not count.
pub(crate) fn census_entries(census: &village::view::Census, households: u32) -> Vec<Entry> {
    let died: u32 = census.deaths.values().sum();
    if census.residents == 0 {
        return Vec::new();
    }
    let mut rows = vec![
        Entry::text("census_title", "Families", true),
        Entry::stats(
            "census",
            vec![
                (Icon::Homes, "Households", households.to_string()),
                (Icon::Morale, "Couples", census.couples.to_string()),
                (Icon::Residents, "Children", census.children.to_string()),
                (Icon::Rest, "Elders", census.elders.to_string()),
            ],
        ),
    ];
    let mut changes = Vec::new();
    for (count, one, many) in [
        (census.births, "birth", "births"),
        (census.arrivals, "arrival", "arrivals"),
        (census.came_of_age, "came of age", "came of age"),
        (died, "death", "deaths"),
    ] {
        if count > 0 {
            changes.push(format!("{count} {}", if count == 1 { one } else { many }));
        }
    }
    if changes.is_empty() {
        // A young island: say that nothing has changed yet, so the block
        // does not look unfinished.
        rows.push(Entry::text(
            "census_changes",
            "Nobody has been born, has arrived or has died since the island was founded.",
            false,
        ));
    } else {
        let mut entry = Entry::text(
            "census_changes",
            format!(
                "Since the island was founded: {}.",
                changes.join(" \u{b7} ")
            ),
            false,
        );
        if died > 0 {
            // Causes are the simulation's own words, most frequent first.
            let mut causes: Vec<(&String, &u32)> = census.deaths.iter().collect();
            causes.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            entry = entry.with_tip(Tooltip::new(
                "Deaths",
                causes
                    .iter()
                    .map(|(cause, count)| format!("{} {count}", super::roster::capitalised(cause)))
                    .collect::<Vec<_>>()
                    .join(" \u{b7} "),
            ));
        }
        rows.push(entry);
    }
    rows
}

fn commission_entries(world: &World, summary: &colony::ColonySummary) -> Vec<Entry> {
    let state = world.resource::<colony::ColonyState>();
    let mut rows = Vec::new();
    rows.push(Entry::text(
        "build_heading",
        "Homes residents build themselves",
        true,
    ));
    for (plot, project) in &state.projects {
        let Some(builder) = world.resource::<SimIndex>().get(project.builder) else {
            continue;
        };
        let name = world
            .get::<Villager>(builder)
            .map_or("Resident", |person| person.name.as_str());
        let status = if let Some(build) = world.get::<village::homestead::CottageBuild>(builder) {
            format!(
                "building · {} ticks remaining",
                build.ends.saturating_sub(world.resource::<SimClock>().tick)
            )
        } else {
            format!(
                "supplies: {}/8 planks · {}/4 logs",
                village::economy::count(world, builder, "plank"),
                village::economy::count(world, builder, "log")
            )
        };
        rows.push(Entry::text(
            &format!("project{}", plot.0),
            format!("{name} · {status}"),
            false,
        ));
        rows.push(Entry::action(
            format!("cancel{}", plot.0),
            "Cancel this housing commission",
            Action::CancelHome(*plot),
        ));
    }
    rows.push(Entry::text(
        "projects",
        commissions_line(summary.construction_projects),
        false,
    ));
    let mut builders: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, entity)| {
            world
                .get::<Villager>(*entity)
                .filter(|person| person.role != Role::Visitor && village::living(world, *entity))
                .map(|person| (person.role != Role::Carpenter, *id, person.name.as_str()))
        })
        .collect();
    builders.sort_by_key(|(priority, id, _)| (*priority, *id));
    for (plot_id, plot_entity) in village::homestead::plots(world) {
        let Some(plot) = world.get::<village::homestead::HomesteadPlot>(plot_entity) else {
            continue;
        };
        if plot.cottage_built {
            continue;
        }
        let Some(definition) = village::homestead::definition(world, plot_entity) else {
            continue;
        };
        if definition.use_kind != "housing" {
            continue;
        }
        if let Some((_, builder, name)) = builders.iter().find(|(_, builder, _)| {
            colony::ColonyAct {
                verb: "commission_home".into(),
                resident: Some(*builder),
                house: Some(plot_id),
                value: 0,
            }
            .check(world)
            .is_ok()
        }) {
            rows.push(Entry::action(
                format!("build{}", plot_id.0),
                format!("Ask {name} to build {}", definition.label.to_lowercase()),
                Action::Commission(*builder, plot_id),
            ));
        }
    }
    rows
}

/// The Journal page: the alert feed with its three filters, grouped by day.
/// Rows select and frame exactly as alert rows do.
/// The season-end card of `season`, beside the season before it when the
/// island keeps one.
fn season_card(world: &World, season: u32) -> Option<super::season::SeasonCard> {
    let ended = village::view::season_summary(world, season)?;
    let before = season
        .checked_sub(1)
        .and_then(|earlier| village::view::season_summary(world, earlier));
    // The island's age in days at the season's end.
    let days = u64::from(ended.season + 1) * village::DAYS_PER_SEASON;
    let good = |key: &str, count: u64| {
        let name = village::catalog::display_in(world, key).unwrap_or(key);
        crate::game_ui::words::noun(name, count)
    };
    Some(super::season::season_card(
        &ended,
        before.as_ref(),
        days,
        &good,
    ))
}

fn journal_entries(world: &World) -> Vec<Entry> {
    let (filter, total, lines) = crate::island_alerts::journal(world);
    let mut rows = vec![Entry::choices(
        "journal_filter",
        crate::island_alerts::model::Filter::ALL
            .iter()
            .map(|filter| (filter.label().to_owned(), Action::AlertFilter(*filter)))
            .collect(),
    )];
    let empty = crate::island_alerts::empty_text(filter, total);
    if lines.is_empty() && !empty.is_empty() {
        rows.push(Entry::text("journal_empty", empty, false));
    }
    let mut days = std::collections::BTreeSet::new();
    for line in lines {
        if days.insert(line.day) {
            rows.push(Entry::text(
                &format!("journal_day{}", line.day),
                // The fold's day as the top bar dates it: "Today",
                // "Autumn, day 4".
                crate::game_ui::words::capital(&crate::game_ui::words::when_day(
                    world,
                    line.day.saturating_sub(1),
                )),
                true,
            ));
        }
        let (key, action) = match line.fold {
            true => (
                format!("journal_fold{}", line.id),
                Action::AlertFold(line.id),
            ),
            false => (format!("journal_row{}", line.id), Action::AlertRow(line.id)),
        };
        rows.push(Entry::marked(key, line.text, line.status, action));
    }
    rows
}

/// Sites kept in the projects panel before "and N more".
const PROJECT_ROWS: usize = 8;
/// Planner decisions listed under the sites.
const DECISION_ROWS: usize = 5;

/// The projects panel of the Village tab: the civic fund, every open
/// construction site with its materials and a guarded Cancel, what each
/// settlement's planner saw last and its recent decisions with their reasons.
/// Empty in a world that cannot build.
fn project_entries(
    world: &World,
    confirm: Option<SimId>,
    unfolded: &BTreeSet<&'static str>,
) -> Vec<Entry> {
    if !crate::island_build::available(world) {
        return Vec::new();
    }
    let sites = crate::island_build::site_rows(world);
    let mut rows = vec![
        Entry::text("civic_title", "Building projects", true),
        // The civic fund as two numbers; what they mean is in the tooltip.
        Entry::stats(
            "civic_fund",
            vec![
                (
                    Icon::Coins,
                    "To spend",
                    village::civic::budget(world).to_string(),
                ),
                (
                    Icon::CivicFund,
                    "Held for open sites",
                    village::civic::fund_hold(world).to_string(),
                ),
            ],
        )
        .with_tip(
            Tooltip::new("Civic fund", crate::island_build::fund_line(world)).icon(Icon::CivicFund),
        ),
    ];
    // Upkeep at a glance, from the simulation's own tallies (no scan).
    if let Some(upkeep) = world.get_resource::<village::civic::upkeep::UpkeepState>() {
        let mut line = format!(
            "Upkeep: {} repaired so far; {} fire{} so far",
            crate::game_ui::words::count(upkeep.repairs, "building"),
            upkeep.fires,
            if upkeep.fires == 1 { "" } else { "s" }
        );
        if !upkeep.burning.is_empty() {
            line.push_str(&format!(", {} burning now", upkeep.burning.len()));
        }
        rows.push(Entry::text("civic_upkeep", line, false));
    }
    if sites.is_empty() {
        rows.push(Entry::text(
            "civic_none",
            "No building is under way. Choose Homes below to place one.",
            false,
        ));
    }
    for (site, _) in sites.iter().take(PROJECT_ROWS) {
        let id = SimId(site.id);
        rows.push(
            Entry::action(
                format!("civic_site{}", site.id),
                site.line(),
                Action::Site(id),
            )
            .with(Extra::Site {
                percent: (site.progress_permille() / 10).min(100) as u8,
                materials: site
                    .materials
                    .iter()
                    .map(|(good, delivered, needed)| {
                        (
                            Icon::of_good(good.trim_end_matches('s')),
                            format!("{good} {delivered}/{needed}"),
                        )
                    })
                    .collect(),
                status: super::roster::capitalised(&site.status()),
            }),
        );
        if confirm == Some(id) {
            rows.push(Entry::strong(
                &format!("civic_ask{}", site.id),
                format!("Abandon {}?", site.label),
            ));
            rows.push(Entry::text(
                &format!("civic_ask_detail{}", site.id),
                "Delivered materials go to the nearest store. Coins already paid stay paid.",
                false,
            ));
            rows.push(Entry::choices(
                &format!("civic_confirm{}", site.id),
                vec![
                    ("Keep building".to_owned(), Action::KeepSite),
                    ("Abandon it".to_owned(), Action::ConfirmCancelSite(id)),
                ],
            ));
        } else {
            rows.push(Entry::choices(
                &format!("civic_cancel{}", site.id),
                vec![(format!("Abandon {}", site.label), Action::CancelSite(id))],
            ));
        }
    }
    if sites.len() > PROJECT_ROWS {
        rows.push(Entry::text(
            "civic_more",
            format!("and {} more open sites", sites.len() - PROJECT_ROWS),
            false,
        ));
    }
    // The planner's view and its log are one step away, folded by default.
    let planners = crate::island_build::planner_lines(world);
    if !planners.is_empty() {
        let open = unfolded.contains("planner");
        rows.push(
            Entry::action(
                "civic_planner_title",
                "What the planner sees",
                Action::Fold("planner"),
            )
            .with(Extra::Fold(open)),
        );
        if open {
            for (id, line) in planners {
                rows.push(Entry::text(&format!("civic_planner{}", id.0), line, false));
            }
        }
    }
    let decisions = crate::island_build::recent_decisions(world, DECISION_ROWS);
    if !decisions.is_empty() {
        let open = unfolded.contains("decisions");
        rows.push(
            Entry::action(
                "civic_log_title",
                "Recent decisions",
                Action::Fold("decisions"),
            )
            .with(Extra::Fold(open)),
        );
        if open {
            let config = world
                .get_resource::<npc_framework::prelude::SimConfig>()
                .copied()
                .unwrap_or_default();
            for (index, decision) in decisions.iter().enumerate() {
                let day = village::calendar_at(decision.tick, &config, true).0 + 1;
                rows.push(Entry::text(
                    &format!("civic_log{index}"),
                    format!("Day {day} \u{b7} {}", decision.text),
                    false,
                ));
            }
        }
    }
    rows
}

/// Residents tab. The roster comes first: sort and settlement choices, then
/// one row per living resident. The selected resident's job and home (the
/// recorded management commands) are one step away, so they never push the
/// roster off the panel.
fn resident_entries(world: &World) -> Vec<Entry> {
    let ledger = world.resource::<Ledger>();
    let mut rows = Vec::new();
    rows.push(Entry::choices(
        "roster_sort",
        super::roster::SortKey::ALL
            .iter()
            .map(|key| (key.label().to_owned(), Action::Sort(*key)))
            .collect(),
    ));
    let scopes = super::economy::scopes(world);
    // A settlement from another world is not a valid selection here.
    let scope = scopes
        .iter()
        .find(|(candidate, _)| *candidate == ledger.scope)
        .and_then(|(candidate, _)| *candidate);
    if scopes.len() > 1 {
        rows.push(Entry::choices(
            "roster_scopes",
            scopes
                .iter()
                .map(|(candidate, label)| {
                    (
                        super::economy::scope_choice(*candidate, label),
                        Action::Scope(*candidate),
                    )
                })
                .collect(),
        ));
    }
    let mut roster = super::roster::arrange(super::roster::rows(world), ledger.sort, scope);
    // A link chose these residents ("Show them"): the page lists them only.
    if let Some((head, residents)) = &ledger.people_filter {
        roster.retain(|row| residents.contains(&row.id));
        rows.push(Entry::strong(
            "roster_filter",
            filter_heading(head, roster.len()),
        ));
        rows.push(Entry::choices(
            "roster_everyone",
            vec![(
                "Show everyone".to_owned(),
                Action::PeopleFilter(String::new(), Vec::new()),
            )],
        ));
    }
    let place = scopes
        .iter()
        .find(|(candidate, _)| *candidate == scope)
        .map_or("Whole island", |(_, label)| label.as_str());
    rows.push(Entry::text(
        "roster_note",
        format!(
            "{place}: {} resident{}, {}.",
            roster.len(),
            if roster.len() == 1 { "" } else { "s" },
            ledger.sort.note()
        ),
        false,
    ));
    rows.extend(roster.iter().map(Entry::resident));
    rows.extend(departed_entries(&village::view::departed(world), scope));
    rows
}

/// How many of the departed the People page lists before it counts the rest.
const DEPARTED_SHOWN: usize = 12;

/// "9 years" or "3 seasons": an age the simulation counts in seasons.
pub(crate) fn age_words(seasons: u64) -> String {
    match (seasons / 4, seasons) {
        (0, 1) => "1 season".to_owned(),
        (0, seasons) => format!("{seasons} seasons"),
        (1, _) => "1 year".to_owned(),
        (years, _) => format!("{years} years"),
    }
}

/// The People page's last section: who has died, newest first, each as a
/// plain sentence. A filter by settlement applies to them as to the roster.
/// Empty while nobody has died.
pub(crate) fn departed_entries(
    departed: &[village::view::Departed],
    scope: Option<SimId>,
) -> Vec<Entry> {
    let gone: Vec<&village::view::Departed> = departed
        .iter()
        .rev()
        .filter(|record| scope.is_none() || record.settlement == scope)
        .collect();
    if gone.is_empty() {
        return Vec::new();
    }
    let mut rows = vec![Entry::text("departed_title", "Departed", true)];
    for (index, record) in gone.iter().take(DEPARTED_SHOWN).enumerate() {
        let who = if record.role.is_empty() {
            format!("{}, a child", record.name)
        } else {
            format!("{}, {}", record.name, record.role)
        };
        let mut line = format!(
            "{who}. Died of {} on day {}, aged {}.",
            record.cause,
            record.day + 1,
            age_words(record.age_seasons)
        );
        if !record.laid_to_rest {
            line.push_str(" Not yet laid to rest.");
        }
        rows.push(Entry::text(&format!("departed{index}"), line, false));
    }
    if gone.len() > DEPARTED_SHOWN {
        rows.push(Entry::caption(
            "departed_more",
            format!("And {} before them.", gone.len() - DEPARTED_SHOWN),
        ));
    }
    rows
}

/// A resident's work and household: the recorded commands a player may give
/// them. The resident inspector shows these on its Home and job tab.
pub(crate) fn manage_entries(world: &World, id: SimId, entity: Entity) -> Vec<Entry> {
    let mut rows = Vec::new();
    if let Some(job) = village::frontier::labor::summary(world, entity) {
        rows.push(Entry::text(
            "employment_title",
            "Work & qualifications",
            true,
        ));
        rows.push(Entry::text(
            "employment",
            {
                let day_length = world
                    .get_resource::<npc_framework::prelude::SimConfig>()
                    .map_or(2400, |config| config.day_length);
                let requested = job.pending_workplace.map(|id| {
                    world
                        .get_resource::<SimIndex>()
                        .and_then(|index| index.get(id))
                        .and_then(|place| world.get::<village::VillagePlace>(place))
                        .map_or_else(
                            || "another workplace".to_owned(),
                            |place| place.label.clone(),
                        )
                });
                employment_text(
                    &job.allowed_roles.join(", "),
                    &job.reason,
                    span_words(job.cooldown_remaining, day_length),
                    requested.as_deref(),
                )
            },
            false,
        ));
        for vacancy in village::frontier::labor::vacancies(world) {
            let command = village::frontier::labor::AssignProfession {
                resident: id,
                workplace: vacancy.workplace,
            };
            if command.check(world).is_ok() {
                let settlement = world
                    .resource::<SimIndex>()
                    .get(vacancy.settlement)
                    .and_then(|e| world.get::<village::frontier::Settlement>(e))
                    .map_or("Village", |s| s.label.as_str());
                rows.push(Entry::action(
                    format!("job{}", vacancy.workplace.0),
                    format!(
                        "Ask for {} in {settlement} · {}/{} places taken",
                        vacancy.role, vacancy.occupied, vacancy.capacity
                    ),
                    Action::Profession(id, vacancy.workplace),
                ));
            }
        }
    }
    rows.push(Entry::text(
        "plan",
        crate::resident_inspector::plan_words(world, entity),
        false,
    ));
    rows.push(Entry::text("housing_heading", "Assign a home", true));
    for (house_id, house_entity) in &world.resource::<SimIndex>().map {
        let Some(house) = world.get::<colony::Household>(*house_entity) else {
            continue;
        };
        let own = house.residents.contains(&id);
        if own {
            rows.push(Entry::text(
                "current_home",
                format!(
                    "Lives in {} ({}/{})",
                    house.label,
                    house.residents.len(),
                    house.capacity
                ),
                false,
            ));
        } else if house.residents.len() < house.capacity as usize {
            rows.push(Entry::action(
                format!("house{}", house_id.0),
                format!(
                    "Move into {} ({}/{})",
                    house.label,
                    house.residents.len(),
                    house.capacity
                ),
                Action::Home(id, *house_id),
            ));
        }
    }
    rows
}

/// The heading of a People list a link chose: "Going without bread: 5
/// residents". A heading that already counts them ("3 residents in Harbour
/// are starving", a folded alert) is not counted twice.
pub(crate) fn filter_heading(head: &str, count: usize) -> String {
    let plural = if count == 1 { "" } else { "s" };
    if head.starts_with(&format!("{count} resident")) {
        format!("{head}.")
    } else {
        format!("{head}: {count} resident{plural}")
    }
}

/// How far to scroll the ledger (logical px, down positive) so an open
/// card, `card` = (top, bottom), shows in the visible `area` = (top,
/// bottom): the whole card when it fits, its bottom just in view; a card
/// taller than the area starts at the area's top, so its head is read
/// first; a card already in view stays.
pub(crate) fn card_scroll(area: (f32, f32), card: (f32, f32)) -> f32 {
    let (area_top, area_bottom) = area;
    let (top, bottom) = card;
    if bottom - top > area_bottom - area_top || top < area_top {
        return top - area_top;
    }
    (bottom - area_bottom).max(0.0)
}
