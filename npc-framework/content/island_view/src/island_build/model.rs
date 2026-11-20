//! The build menu, the placement ghost, the projects panel and the building
//! card as plain data. No `World` access and no simulation state: the widget
//! layer supplies definitions, plots and counts it read from the simulation.
use crate::game_ui::icon::Icon;
use island_content::village::Role;
use island_content::village::civic::Plot;
use island_content::village::frontier::building_spec::{BuildingDef, BuildingKind};
use serde_json::Value;

/// The palette's sections, in the order they are offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Group {
    Homes,
    Workplaces,
    Storage,
    Market,
    /// Farm buildings, when the catalogue has any: a building with crop
    /// plots, and then the farmers' workplace with it.
    Farms,
}

impl Group {
    pub(crate) const ALL: [Self; 5] = [
        Self::Homes,
        Self::Workplaces,
        Self::Storage,
        Self::Market,
        Self::Farms,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Homes => "Homes",
            Self::Workplaces => "Workplaces",
            Self::Storage => "Storage",
            Self::Market => "Market",
            Self::Farms => "Farms",
        }
    }
    /// The short name on the build dock.
    pub(crate) fn dock_label(self) -> &'static str {
        match self {
            Self::Workplaces => "Work",
            other => other.label(),
        }
    }
    pub(crate) fn icon(self) -> Icon {
        match self {
            Self::Homes => Icon::Homes,
            Self::Workplaces => Icon::Work,
            Self::Storage => Icon::Storage,
            Self::Market => Icon::Market,
            Self::Farms => Icon::Farms,
        }
    }
    /// The group comes from the data. A building with crop plots is a farm
    /// building; where the catalogue has one (`farms`), the farmers'
    /// workplace joins it. Without one there is no Farms group at all.
    pub(crate) fn of(def: &BuildingDef, farms: bool) -> Self {
        if def.field.is_some() {
            return Self::Farms;
        }
        match &def.kind {
            BuildingKind::Home { .. } => Self::Homes,
            BuildingKind::Workplace {
                profession: Role::Farmer,
                ..
            } if farms => Self::Farms,
            BuildingKind::Workplace { .. } => Self::Workplaces,
            BuildingKind::Storage { .. } => Self::Storage,
            BuildingKind::Stall { .. } => Self::Market,
        }
    }
}

/// One building of the palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaletteItem {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) group: Group,
    /// "8 planks, 4 logs", or "no materials".
    pub(crate) cost: String,
    /// One line on what the finished building is for.
    pub(crate) purpose: String,
    /// Body in metres: across the entrance, then behind it.
    pub(crate) size: (u32, u32),
    /// The cost by catalogue key, for icons and the village stock.
    pub(crate) materials: Vec<(String, u32)>,
    pub(crate) icon: Icon,
    /// Why the building cannot be chosen now, as a sentence. The catalogue
    /// has no such rule today, so nothing sets it: the card is ready for one.
    pub(crate) unavailable: Option<String>,
}

impl PaletteItem {
    /// The line under the cost: the reason while unavailable, else the purpose.
    pub(crate) fn card_note(&self) -> &str {
        self.unavailable.as_deref().unwrap_or(&self.purpose)
    }
}

/// The picture of a building: its trade's for a workplace.
pub(crate) fn building_icon(kind: &BuildingKind) -> Icon {
    match kind {
        BuildingKind::Home { beds } if *beds > 2 => Icon::House,
        BuildingKind::Home { .. } => Icon::Cottage,
        BuildingKind::Workplace { profession, .. } => {
            Icon::of_trade(profession.name()).unwrap_or(Icon::Work)
        }
        BuildingKind::Storage { .. } => Icon::Stockpile,
        BuildingKind::Stall { .. } => Icon::MarketStall,
    }
}

/// "8 planks, 4 logs" as a sentence for a tooltip.
pub(crate) fn capitalised_cost(cost: &str) -> String {
    if cost == "no materials" {
        "Costs no materials.".to_owned()
    } else {
        format!("Costs {cost}.")
    }
}

/// What the tag beside the cursor says about the ghost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Tag {
    pub(crate) valid: bool,
    /// "You can build here" or "Cannot build here".
    pub(crate) verdict: &'static str,
    /// The simulation's reason for a refusal, as a sentence.
    pub(crate) reason: Option<String>,
    /// "Cottage · 8 planks · 4 logs · 4 × 5 m".
    pub(crate) facts: String,
}

/// The verdict is a tick or a cross and a sentence, never a colour alone.
/// `refused` is the simulation's reason: the placement check's, or the one
/// it gave for a recorded placement.
pub(crate) fn tag(item: &PaletteItem, refused: Option<&str>) -> Tag {
    let reason = refused.map(|reason| {
        let mut sentence = crate::colony_panel::capitalised(reason.trim());
        if !sentence.ends_with('.') {
            sentence.push('.');
        }
        sentence
    });
    Tag {
        valid: reason.is_none(),
        verdict: if reason.is_none() {
            "You can build here"
        } else {
            "Cannot build here"
        },
        reason,
        facts: format!(
            "{} \u{b7} {} \u{b7} {} \u{d7} {} m",
            item.name,
            item.cost.replace(", ", " \u{b7} "),
            item.size.0,
            item.size.1
        ),
    }
}

/// "8 planks, 4 logs" from counted, already named goods.
pub(crate) fn cost_text(cost: &[(String, u32)]) -> String {
    if cost.is_empty() {
        return "no materials".into();
    }
    cost.iter()
        .map(|(good, count)| crate::game_ui::words::count(u64::from(*count), good))
        .collect::<Vec<_>>()
        .join(", ")
}

fn people(count: u32, one: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {one}s")
    }
}

/// What a finished building is for, in one line.
pub(crate) fn purpose(kind: &BuildingKind) -> String {
    match kind {
        BuildingKind::Home { beds } => format!(
            "A home with {beds} bed{}",
            if *beds == 1 { "" } else { "s" }
        ),
        BuildingKind::Workplace {
            profession,
            capacity,
            ..
        } => format!("Work for {}", people(*capacity, profession.name())),
        BuildingKind::Storage { capacity } => {
            format!("Stores {capacity} units; residents buy from it")
        }
        BuildingKind::Stall { capacity } => {
            format!("Sells {capacity} units to anyone, visitors too")
        }
    }
}

/// The palette in authored order. `good` names a good for display.
pub(crate) fn palette(defs: &[BuildingDef], good: &dyn Fn(&str) -> String) -> Vec<PaletteItem> {
    let farms = defs.iter().any(|def| def.field.is_some());
    defs.iter()
        .map(|def| {
            let cost: Vec<(String, u32)> = def
                .cost
                .iter()
                .map(|stack| (good(&stack.good), stack.count))
                .collect();
            PaletteItem {
                id: def.id.clone(),
                name: def.display.clone(),
                group: Group::of(def, farms),
                cost: cost_text(&cost),
                // A field is a store too, but what it is for is its crop.
                purpose: def.field.as_ref().map_or_else(
                    || purpose(&def.kind),
                    |field| {
                        format!(
                            "{} plots of {}, with a store for the harvest",
                            field.plots,
                            good(&field.crop)
                        )
                    },
                ),
                size: (def.footprint.width, def.footprint.depth),
                materials: def
                    .cost
                    .iter()
                    .map(|stack| (stack.good.clone(), stack.count))
                    .collect(),
                icon: if def.field.is_some() {
                    Icon::Field
                } else {
                    building_icon(&def.kind)
                },
                unavailable: None,
            }
        })
        .collect()
}

/// The groups that have something to offer, with their buildings.
pub(crate) fn grouped(items: &[PaletteItem]) -> Vec<(Group, Vec<&PaletteItem>)> {
    Group::ALL
        .into_iter()
        .map(|group| {
            (
                group,
                items.iter().filter(|item| item.group == group).collect(),
            )
        })
        .filter(|(_, items): &(Group, Vec<&PaletteItem>)| !items.is_empty())
        .collect()
}

/// The group the palette shows: the wanted one if it has buildings, else the
/// first that has.
pub(crate) fn shown_group(items: &[PaletteItem], wanted: Group) -> Option<Group> {
    let groups = grouped(items);
    groups
        .iter()
        .find(|(group, _)| *group == wanted)
        .or(groups.first())
        .map(|(group, _)| *group)
}

/// The next quarter turn.
pub(crate) fn rotated(rotation: u8) -> u8 {
    (rotation + 1) % 4
}

/// The placement ghost under the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ghost {
    pub(crate) building: String,
    /// Entrance cell.
    pub(crate) cell: (i32, i32),
    pub(crate) rotation: u8,
    /// Where the body would stand, when the building is known.
    pub(crate) plot: Option<Plot>,
    /// `None`: the simulation would accept it. Otherwise its own reason.
    pub(crate) blocked: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tint {
    /// The simulation would open a site here.
    Valid,
    Blocked,
}

impl Ghost {
    /// The ghost for one answer of the simulation's placement check.
    pub(crate) fn new(
        building: &str,
        cell: (i32, i32),
        rotation: u8,
        plot: Option<Plot>,
        check: Result<(), String>,
    ) -> Self {
        Self {
            building: building.into(),
            cell,
            rotation: rotation % 4,
            plot,
            blocked: match (check, plot) {
                (Err(reason), _) => Some(reason),
                // No plot means the building is not in the catalogue.
                (Ok(()), None) => Some("this building is not available".into()),
                (Ok(()), Some(_)) => None,
            },
        }
    }
    pub(crate) fn tint(&self) -> Tint {
        if self.blocked.is_none() {
            Tint::Valid
        } else {
            Tint::Blocked
        }
    }
    /// Body cells and the entrance cell, for the outline.
    pub(crate) fn cells(&self) -> (Vec<(i32, i32)>, (i32, i32)) {
        let body = self.plot.map_or_else(Vec::new, |plot| {
            plot.cells().map(|cell| (cell.x, cell.z)).collect()
        });
        (body, self.cell)
    }
}

/// Materials of one site: `(good, delivered, needed)`.
pub(crate) type Materials = Vec<(String, u32, u32)>;

/// How far along an open site is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    /// Still short of materials.
    Materials,
    /// Everything delivered; nobody has worked on it yet.
    Ready,
    Building,
}

/// One open construction site, for the projects panel and the site card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiteRow {
    pub(crate) id: u32,
    pub(crate) label: String,
    pub(crate) materials: Materials,
    pub(crate) work_done: u32,
    pub(crate) work_ticks: u32,
    /// Commissioned by the planner, not placed by the overseer.
    pub(crate) civic: bool,
    pub(crate) reason: String,
    pub(crate) builder: Option<String>,
    /// Whole days without a delivery while materials are missing.
    pub(crate) waiting_days: u32,
    /// Materials are missing and the civic fund has nothing to pay for them:
    /// no resident will bring any until it has.
    pub(crate) unfunded: bool,
}

impl SiteRow {
    pub(crate) fn supplied(&self) -> bool {
        self.materials
            .iter()
            .all(|(_, delivered, needed)| delivered >= needed)
    }
    pub(crate) fn stage(&self) -> Stage {
        if !self.supplied() {
            Stage::Materials
        } else if self.work_done == 0 {
            Stage::Ready
        } else {
            Stage::Building
        }
    }
    /// 0..=1000: materials are the first half of a site, work the second.
    pub(crate) fn progress_permille(&self) -> u32 {
        let (delivered, needed) =
            self.materials
                .iter()
                .fold((0_u64, 0_u64), |(have, want), (_, delivered, needed)| {
                    (
                        have + u64::from((*delivered).min(*needed)),
                        want + u64::from(*needed),
                    )
                });
        // Nothing to deliver, or nothing to work, counts as done.
        let materials = (delivered * 1000).checked_div(needed).unwrap_or(1000);
        let work = (u64::from(self.work_done.min(self.work_ticks)) * 1000)
            .checked_div(u64::from(self.work_ticks))
            .unwrap_or(1000);
        ((materials + work) / 2) as u32
    }
    /// "planks 3/8 · logs 4/4", or "no materials needed".
    pub(crate) fn materials_line(&self) -> String {
        if self.materials.is_empty() {
            return "no materials needed".into();
        }
        self.materials
            .iter()
            .map(|(good, delivered, needed)| format!("{good} {delivered}/{needed}"))
            .collect::<Vec<_>>()
            .join(" · ")
    }
    /// What the site is waiting for, in a few words.
    pub(crate) fn status(&self) -> String {
        match self.stage() {
            Stage::Materials => {
                let waiting = if self.waiting_days > 0 {
                    format!("waiting for materials for {}", days(self.waiting_days))
                } else {
                    "waiting for materials".to_owned()
                };
                if self.unfunded {
                    format!("{waiting}; the fund cannot pay for them yet")
                } else {
                    waiting
                }
            }
            Stage::Ready => match &self.builder {
                Some(name) => format!("materials in · {name} is on the way"),
                None => "materials in · waiting for a builder".into(),
            },
            Stage::Building => {
                let work = format!("work {}/{}", self.work_done, self.work_ticks);
                match &self.builder {
                    Some(name) => format!("{work} · {name} is building"),
                    None => work,
                }
            }
        }
    }
    /// The two lines of its row in the projects panel.
    pub(crate) fn line(&self) -> String {
        format!(
            "{} · {}%\n{} · {}",
            self.label,
            self.progress_permille() / 10,
            self.materials_line(),
            self.status()
        )
    }
}

pub(crate) fn days(count: u32) -> String {
    if count == 1 {
        "1 day".into()
    } else {
        format!("{count} days")
    }
}

/// "Civic fund: 420 coins to spend · 96 held for open sites".
pub(crate) fn fund_line(fund: u32, budget: u32, held: u32, sites: usize) -> String {
    let coins = |count: u32| format!("{count} coin{}", if count == 1 { "" } else { "s" });
    let reserve = fund.saturating_sub(budget);
    let spend = if fund == 0 {
        "Civic fund: empty. It fills on levy day and from what the village's stores sell."
            .to_owned()
    } else if budget == 0 {
        format!(
            "Civic fund: {}, all of it the reserve for grants. Nothing can be spent on materials yet.",
            coins(fund)
        )
    } else {
        format!(
            "Civic fund: {}. {} can be spent on materials; {reserve} stay in reserve for grants.",
            coins(fund),
            coins(budget)
        )
    };
    if sites == 0 {
        spend
    } else {
        format!(
            "{spend}\n{held} held back from the dividend for {sites} open site{}.",
            if sites == 1 { "" } else { "s" }
        )
    }
}

/// Whole days a site has gone without a delivery, while it still needs one.
pub(crate) fn waiting_days(now: u64, since: u64, day_ticks: u64, supplied: bool) -> u32 {
    if supplied || day_ticks == 0 {
        return 0;
    }
    u32::try_from(now.saturating_sub(since) / day_ticks).unwrap_or(u32::MAX)
}

/// One planner decision of the log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Decision {
    pub(crate) tick: u64,
    /// The site the decision is about, when it names one.
    pub(crate) site: Option<u32>,
    pub(crate) text: String,
}

fn text<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload.get(key).and_then(Value::as_str)
}

fn number(payload: &Value, key: &str) -> Option<u64> {
    payload.get(key).and_then(Value::as_u64)
}

/// A planner decision (`VillageCivicProject`) in words: who decided what,
/// and the planner's own reason. `settlement` and `building` are display
/// names the caller looked up.
pub(crate) fn decision_text(settlement: &str, building: Option<&str>, payload: &Value) -> String {
    let what = building.map_or_else(|| "a building".to_owned(), article);
    let reason = text(payload, "reason").unwrap_or_default();
    let because = if reason.is_empty() {
        String::new()
    } else {
        format!(": {reason}")
    };
    match text(payload, "decision").unwrap_or_default() {
        "commissioned" => {
            let cost = match (number(payload, "estimated_cost"), number(payload, "fund")) {
                (Some(cost), Some(fund)) => format!(" (about {cost} coins; fund {fund})"),
                _ => String::new(),
            };
            format!("{settlement} ordered {what}{because}{cost}")
        }
        "completed" => match number(payload, "paid") {
            Some(paid) => format!("{settlement} finished {what}; the fund paid {paid} coins"),
            None => format!("{settlement} finished {what}"),
        },
        "cancelled" => format!("{settlement} gave up {what}{because}"),
        other => {
            // "deferred: the civic fund cannot pay for the materials yet"
            let (verb, why) = other.split_once(": ").unwrap_or((other, ""));
            let cost = match (number(payload, "estimated_cost"), number(payload, "fund")) {
                (Some(cost), Some(fund)) if verb == "deferred" => {
                    format!(" (about {cost} coins; fund {fund})")
                }
                _ => String::new(),
            };
            let wanted = if reason.is_empty() {
                String::new()
            } else {
                format!(" It wants {what}{because}.")
            };
            if why.is_empty() {
                format!("{settlement}: {verb}.{wanted}")
            } else {
                format!("{settlement} {verb} {what}: {why}{cost}.{wanted}")
            }
        }
    }
}

/// "a cottage", "an inn": a building's display name as a lowercase noun.
pub(crate) fn article(name: &str) -> String {
    let lower = name.to_lowercase();
    let an = lower.starts_with(['a', 'e', 'i', 'o', 'u']);
    format!("{} {lower}", if an { "an" } else { "a" })
}

/// The newest `limit` decisions, newest first.
pub(crate) fn recent(log: &[Decision], limit: usize) -> Vec<&Decision> {
    log.iter().rev().take(limit).collect()
}

/// A planner's remembered decision in display words: "commissioned
/// tailor_shop" names the building by its catalogue key, which `display`
/// turns into its name.
pub(crate) fn last_decision(decision: &str, display: &dyn Fn(&str) -> Option<String>) -> String {
    match decision.strip_prefix("commissioned ") {
        Some(key) => match display(key) {
            Some(name) => format!("ordered {}", article(&name)),
            None => format!("ordered {}", article(&key.replace('_', " "))),
        },
        None => decision.to_owned(),
    }
}

/// What a planner saw at its last review.
pub(crate) fn planner_line(
    settlement: &str,
    homeless: u32,
    free_beds: u32,
    surplus: u32,
    decision: &str,
) -> String {
    let seen = format!(
        "{settlement}: {homeless} without a bed · {free_beds} free bed{} · {surplus} goods with no room in store",
        if free_beds == 1 { "" } else { "s" },
    );
    if decision.is_empty() {
        format!("{seen}\nNo decision yet.")
    } else {
        format!("{seen}\nLast decision: {decision}")
    }
}

/// The stock of a store or a stall: how full it is and what lies there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shelf {
    pub(crate) used: u32,
    pub(crate) capacity: u32,
    /// `(good, units)`, already named.
    pub(crate) stock: Vec<(String, u32)>,
    pub(crate) sold_units: u64,
    pub(crate) turnover: u64,
}

impl Shelf {
    /// One good per line, the most first; one sentence for empty shelves.
    pub(crate) fn stock_lines(&self) -> Vec<String> {
        let mut stock = self.stock.clone();
        stock.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        if stock.is_empty() {
            return vec!["The shelves are empty: its makers restock it as they work.".to_owned()];
        }
        stock
            .iter()
            .map(|(good, units)| crate::game_ui::words::count(u64::from(*units), good))
            .collect()
    }
    pub(crate) fn lines(&self) -> Vec<String> {
        let mut stock = self.stock.clone();
        stock.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let more = stock.len().saturating_sub(6);
        let mut listed: Vec<String> = stock
            .iter()
            .take(6)
            .map(|(good, units)| crate::game_ui::words::count(u64::from(*units), good))
            .collect();
        if more > 0 {
            listed.push(format!("{more} more"));
        }
        vec![
            format!("Stock {}/{} units", self.used, self.capacity),
            if listed.is_empty() {
                "The shelves are empty: its makers restock it as they work.".into()
            } else {
                listed.join(", ")
            },
            format!("Sold {} units for {} coins", self.sold_units, self.turnover),
        ]
    }
}

/// What a building card shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CardBody {
    Site(SiteRow),
    Home {
        beds: u32,
        residents: Vec<String>,
        heated: bool,
    },
    Workplace {
        trade: String,
        seats: Option<u32>,
        workers: Vec<String>,
        /// For an inn: who is inside now. `None` for any other workplace.
        inside: Option<Vec<String>>,
        /// For an inn: its guest beds in order, each with its guest and when
        /// the rented rest ends, or `None` for a free bed.
        guest_beds: Vec<Option<(String, String)>>,
    },
    Depot {
        stall: bool,
        shelf: Shelf,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Card {
    pub(crate) title: String,
    pub(crate) settlement: Option<String>,
    /// The building's condition, for one that wears (`civic::upkeep`).
    pub(crate) upkeep: Option<CardUpkeep>,
    pub(crate) body: CardBody,
}

/// What a building card says of the building's upkeep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CardUpkeep {
    /// Condition, permille.
    pub(crate) condition: u32,
    pub(crate) needs_repair: bool,
    pub(crate) worn: bool,
    pub(crate) unusable: bool,
    pub(crate) burning: bool,
    /// A repair site is open for it.
    pub(crate) repairing: bool,
}

impl CardUpkeep {
    /// "Condition 42% · needs repair", and what follows from it.
    pub(crate) fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if self.burning {
            lines.push("On fire: residents nearby are fighting it.".to_owned());
        }
        let state = if self.unusable {
            " · unusable until repaired"
        } else if self.worn {
            " · worn, works worse · needs repair"
        } else if self.needs_repair {
            " · needs repair"
        } else {
            ""
        };
        lines.push(format!("Condition {}%{state}", self.condition / 10));
        if self.repairing {
            lines.push("Repairs are under way.".to_owned());
        } else if self.needs_repair {
            lines.push("The village repairs it when it has the builders and the coins.".to_owned());
        }
        lines
    }
}

fn names(list: &[String], nobody: &str) -> String {
    if list.is_empty() {
        nobody.to_owned()
    } else {
        list.join(", ")
    }
}

/// "Guest beds 1/4 taken"; nothing for a place without guest beds.
fn guest_beds_line(beds: &[Option<(String, String)>]) -> Option<String> {
    (!beds.is_empty()).then(|| {
        format!(
            "Guest beds {}/{} taken",
            beds.iter().flatten().count(),
            beds.len()
        )
    })
}

/// One line per guest bed: "Bed 1 \u{b7} Odo, until 06:30" or "Bed 2 \u{b7} free".
fn guest_bed_lines(beds: &[Option<(String, String)>]) -> Vec<String> {
    beds.iter()
        .enumerate()
        .map(|(index, bed)| match bed {
            Some((guest, until)) if until.is_empty() => format!("Bed {} \u{b7} {guest}", index + 1),
            Some((guest, until)) => format!("Bed {} \u{b7} {guest}, until {until}", index + 1),
            None => format!("Bed {} \u{b7} free", index + 1),
        })
        .collect()
}

/// A tab of the building card.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CardTab {
    #[default]
    Overview,
    /// Who lives or works there (and, at an inn, who is inside).
    People,
    Stock,
}

impl Card {
    /// The card's tabs for this kind of building. A site has one view and
    /// therefore no tab strip.
    pub(crate) fn tabs(&self) -> Vec<CardTab> {
        match &self.body {
            CardBody::Site(_) => vec![CardTab::Overview],
            CardBody::Home { .. } | CardBody::Workplace { .. } => {
                vec![CardTab::Overview, CardTab::People]
            }
            CardBody::Depot { .. } => vec![CardTab::Overview, CardTab::Stock],
        }
    }

    /// What a tab is called on this card.
    pub(crate) fn tab_label(&self, tab: CardTab) -> &'static str {
        match (tab, &self.body) {
            (CardTab::Overview, _) => "Overview",
            (CardTab::Stock, _) => "Stock",
            (CardTab::People, CardBody::Home { .. }) => "Residents",
            (
                CardTab::People,
                CardBody::Workplace {
                    inside: Some(_), ..
                },
            ) => "Workers and guests",
            (CardTab::People, _) => "Workers",
        }
    }

    /// The lines of one tab. A tab the card does not have shows its Overview.
    pub(crate) fn tab_lines(&self, tab: CardTab) -> Vec<String> {
        let tab = if self.tabs().contains(&tab) {
            tab
        } else {
            CardTab::Overview
        };
        if tab == CardTab::Overview
            && let Some(upkeep) = &self.upkeep
        {
            let mut lines = upkeep.lines();
            lines.extend(self.tab_lines_of(tab));
            return lines;
        }
        self.tab_lines_of(tab)
    }

    fn tab_lines_of(&self, tab: CardTab) -> Vec<String> {
        let listed = |names: &[String], nobody: &str| {
            if names.is_empty() {
                vec![nobody.to_owned()]
            } else {
                names.to_vec()
            }
        };
        match (&self.body, tab) {
            // The bar already says how far a site is.
            (CardBody::Site(_), _) => self
                .lines()
                .into_iter()
                .filter(|line| !line.starts_with("Progress "))
                .collect(),
            (
                CardBody::Home {
                    beds,
                    residents,
                    heated,
                },
                CardTab::Overview,
            ) => vec![
                format!("Beds {}/{beds}", residents.len()),
                if *heated {
                    "The hearth is lit.".to_owned()
                } else {
                    "The hearth is cold.".to_owned()
                },
            ],
            (CardBody::Home { residents, .. }, _) => listed(
                residents,
                "Nobody lives here yet: residents move in when they need a home.",
            ),
            (
                CardBody::Workplace {
                    trade,
                    seats,
                    workers,
                    inside,
                    guest_beds,
                },
                CardTab::Overview,
            ) => {
                let mut lines = vec![match seats {
                    Some(seats) => format!("{trade} \u{b7} {}/{seats} places taken", workers.len()),
                    None => trade.clone(),
                }];
                if let Some(line) = guest_beds_line(guest_beds) {
                    lines.push(line);
                }
                if let Some(inside) = inside {
                    lines.push(match inside.len() {
                        0 => "Nobody is inside now.".to_owned(),
                        1 => "1 person is inside now.".to_owned(),
                        count => format!("{count} people are inside now."),
                    });
                }
                lines
            }
            (
                CardBody::Workplace {
                    workers,
                    inside,
                    guest_beds,
                    ..
                },
                _,
            ) => {
                let mut lines = listed(
                    workers,
                    "Nobody works here yet: a resident's Home and job tab can request a place here.",
                );
                lines.extend(guest_bed_lines(guest_beds));
                if let Some(inside) = inside {
                    lines.push(format!("Inside now: {}", names(inside, "nobody")));
                }
                lines
            }
            (CardBody::Depot { shelf, .. }, CardTab::Stock) => shelf.stock_lines(),
            (CardBody::Depot { stall, shelf }, _) => {
                let mut lines = vec![
                    format!("Stock {}/{} units", shelf.used, shelf.capacity),
                    format!(
                        "Sold {} units for {} coins",
                        shelf.sold_units, shelf.turnover
                    ),
                ];
                if *stall {
                    lines.push("Sells to anyone, visitors included.".to_owned());
                }
                lines
            }
        }
    }

    /// The line under the title: what kind of thing this is, and where.
    pub(crate) fn kind_line(&self) -> String {
        let kind = match &self.body {
            CardBody::Site(site) if site.civic => "Building site · ordered by the village",
            CardBody::Site(_) => "Building site · placed by you",
            CardBody::Home { .. } => "Home",
            CardBody::Workplace {
                inside: Some(_), ..
            } => "Inn",
            CardBody::Workplace { .. } => "Workplace",
            CardBody::Depot { stall: true, .. } => "Market stall",
            CardBody::Depot { stall: false, .. } => "Storage",
        };
        match &self.settlement {
            Some(settlement) => format!("{kind} · {settlement}"),
            None => kind.to_owned(),
        }
    }
    /// The card's lines under the kind line: the condition of a building
    /// that wears, then what its kind shows.
    pub(crate) fn lines(&self) -> Vec<String> {
        let mut lines = self.upkeep.map(|upkeep| upkeep.lines()).unwrap_or_default();
        lines.extend(self.body_lines());
        lines
    }

    fn body_lines(&self) -> Vec<String> {
        match &self.body {
            CardBody::Site(site) => {
                let mut lines = vec![
                    format!("Progress {}%", site.progress_permille() / 10),
                    format!("Materials: {}", site.materials_line()),
                    capitalise(&site.status()),
                ];
                if site.civic && !site.reason.is_empty() {
                    lines.push(format!("Why: {}", site.reason));
                }
                lines
            }
            CardBody::Home {
                beds,
                residents,
                heated,
            } => vec![
                format!("Beds {}/{beds}", residents.len()),
                format!("Lives here: {}", names(residents, "nobody yet")),
                if *heated {
                    "The hearth is lit.".into()
                } else {
                    "The hearth is cold.".into()
                },
            ],
            CardBody::Workplace {
                trade,
                seats,
                workers,
                inside,
                guest_beds,
            } => {
                let mut lines = vec![
                    match seats {
                        Some(seats) => {
                            format!("{trade} · {}/{seats} places taken", workers.len())
                        }
                        None => trade.clone(),
                    },
                    format!("Works here: {}", names(workers, "nobody yet")),
                ];
                lines.extend(guest_beds_line(guest_beds));
                lines.extend(guest_bed_lines(guest_beds));
                if let Some(inside) = inside {
                    lines.push(format!("Inside now: {}", names(inside, "nobody")));
                }
                lines
            }
            CardBody::Depot { stall, shelf } => {
                let mut lines = shelf.lines();
                lines.push(
                    if *stall {
                        "Sells to anyone, visitors included."
                    } else {
                        "Sells to residents; goods still belong to whoever stored them."
                    }
                    .into(),
                );
                lines
            }
        }
    }
    /// 0..=1000 for the bar under the title, when the card has one.
    pub(crate) fn bar_permille(&self) -> Option<u32> {
        match &self.body {
            CardBody::Site(site) => Some(site.progress_permille()),
            CardBody::Depot { shelf, .. } if shelf.capacity > 0 => Some(
                (u64::from(shelf.used.min(shelf.capacity)) * 1000 / u64::from(shelf.capacity))
                    as u32,
            ),
            _ => None,
        }
    }
}

/// A workplace's card is titled by the building, not the trade: "Harbour
/// innkeeper" reads "Harbour inn" (the trade is a row of the card). A
/// building the village raised keeps its own name.
pub(crate) fn workplace_title(label: &str, trade: &str, built: Option<&str>) -> String {
    if let Some(built) = built {
        return built.to_owned();
    }
    let building = match trade {
        "farmer" => "farm",
        "miller" => "mill",
        "baker" => "bakery",
        "well keeper" => "well",
        "woodcutter" => "woodcutter's camp",
        "carpenter" => "workshop",
        "miner" => "mine",
        "blacksmith" => "smithy",
        "innkeeper" => "inn",
        "hunter" => "hunting lodge",
        "tanner" => "tannery",
        "tailor" => "tailor's shop",
        "fisher" => "fishing hut",
        _ => return label.to_owned(),
    };
    let lower = label.to_lowercase();
    match lower.strip_suffix(trade) {
        Some(head) if !head.is_empty() => format!("{}{building}", &label[..head.len()]),
        Some(_) => capitalise(building),
        None => label.to_owned(),
    }
}

pub(crate) fn capitalise(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The job verbs that belong to a running leaf, by the leaf's own label. A
/// job stated for an earlier leaf must not name what this one is doing.
pub(crate) fn errand_verbs(leaf: &str) -> &'static [&'static str] {
    match leaf {
        "building" => &["build", "build_cottage"],
        "carrying building materials" => &["haul", "fetch"],
        "storing surplus goods" => &["store"],
        "restocking the stall" => &["restock", "fetch"],
        "delivering goods" => &["sell"],
        "buying goods" | "buying work materials" | "buying food" | "stocking provisions" => {
            &["buy"]
        }
        _ => &[],
    }
}

/// What a resident on a civic errand is doing, from their job intent:
/// `(verb, good, target)` with display names. `None` for other errands.
pub(crate) fn errand_label(verb: &str, good: &str, target: Option<&str>) -> Option<String> {
    let place = |fallback: &str| target.map_or_else(|| fallback.to_owned(), str::to_owned);
    Some(match verb {
        "build" => format!("building {}", place("on a site")),
        "build_cottage" => "building a cottage".to_owned(),
        "haul" => format!("carrying {good} to {}", place("a building site")),
        "fetch" => format!("fetching {good} from {}", place("the store")),
        "store" => format!("storing {good} at {}", place("the store")),
        "restock" => format!("restocking {} with {good}", place("the stall")),
        _ => return None,
    })
}

/// A sale or a purchase at a store or a stall, as opposed to one between
/// two residents: `selling bread at Harbour market stall 1`.
pub(crate) fn shelf_trade_label(verb: &str, good: &str, shelf: &str) -> Option<String> {
    match verb {
        "sell" => Some(format!("selling {good} at {shelf}")),
        "buy" => Some(format!("buying {good} at {shelf}")),
        _ => None,
    }
}

/// How many of `slots` a shelf holding `used` of `capacity` fills: none when
/// empty, at least one as soon as anything lies there, all when full.
pub(crate) fn filled_slots(used: u32, capacity: u32, slots: usize) -> usize {
    if used == 0 || capacity == 0 || slots == 0 {
        return 0;
    }
    let filled = (u64::from(used.min(capacity)) * slots as u64).div_ceil(u64::from(capacity));
    (filled as usize).clamp(1, slots)
}

/// Units of delivered material still lying beside a site: the pile shrinks
/// as the work uses it up.
pub(crate) fn pile_units(delivered: u32, work_done: u32, work_ticks: u32) -> u32 {
    if work_ticks == 0 {
        return 0;
    }
    let left = u64::from(work_ticks.saturating_sub(work_done));
    (u64::from(delivered) * left).div_ceil(u64::from(work_ticks)) as u32
}

/// A ray against an upright box; the distance along the ray, if it hits.
pub(crate) fn ray_box(origin: [f32; 3], dir: [f32; 3], lo: [f32; 3], hi: [f32; 3]) -> Option<f32> {
    let mut near = 0.0_f32;
    let mut far = f32::INFINITY;
    for axis in 0..3 {
        if dir[axis].abs() < 1e-6 {
            if origin[axis] < lo[axis] || origin[axis] > hi[axis] {
                return None;
            }
            continue;
        }
        let a = (lo[axis] - origin[axis]) / dir[axis];
        let b = (hi[axis] - origin[axis]) / dir[axis];
        near = near.max(a.min(b));
        far = far.min(a.max(b));
        if near > far {
            return None;
        }
    }
    Some(near)
}

/// Which prop each of `slots` shows for a stock of counted kinds: every kind
/// gets its share of the slots, in the order given, and a kind with any
/// stock at all is not crowded out while a slot is left for it.
pub(crate) fn spread<T: Copy + PartialEq>(stock: &[(T, u32)], slots: usize) -> Vec<T> {
    let total: u64 = stock.iter().map(|(_, units)| u64::from(*units)).sum();
    if total == 0 || slots == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(slots);
    for slot in 0..slots {
        // The unit in the middle of this slot's share of the stock.
        let at = (2 * slot as u64 + 1) * total / (2 * slots as u64);
        let mut seen = 0_u64;
        for (kind, units) in stock {
            seen += u64::from(*units);
            if at < seen {
                out.push(*kind);
                break;
            }
        }
    }
    // A small lot still shows once when the larger ones have slots to spare.
    for (kind, units) in stock {
        if *units > 0 && !out.contains(kind) {
            let crowded = stock
                .iter()
                .map(|(other, _)| (*other, out.iter().filter(|shown| *shown == other).count()))
                .max_by_key(|(_, count)| *count);
            if let Some((other, count)) = crowded
                && count > 1
                && let Some(at) = out.iter().rposition(|shown| *shown == other)
            {
                out[at] = *kind;
            }
        }
    }
    out
}
