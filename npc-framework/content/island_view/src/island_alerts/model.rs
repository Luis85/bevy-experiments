//! The alert feed as plain data: classification of journal effects, severity,
//! repeat folding, the per-day fold of routine notes and the length cap. No
//! `World` access and no simulation state.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AlertKind {
    Death,
    Starving,
    Dehydrated,
    Freezing,
    Exhausted,
    Depleted,
    Shortage,
    Homeless,
    Built,
    Occupation,
    Arrival,
    Trade,
    Levy,
    FirstTool,
    /// A construction site was opened, by the planner or the overseer.
    ProjectStarted,
    ProjectCompleted,
    ProjectCancelled,
    /// A site has gone whole days without a delivery it still needs.
    SiteWaiting,
    /// Somebody who died was laid to rest: their body has left the island.
    LaidToRest,
    /// A sleeper was woken before morning, by a need or by an order.
    Woken,
    /// Bonds and village life: a gift, a new tier, an invitation, a
    /// gathering, a mourning.
    Social,
    /// The played character's homestead grew a tier.
    Homestead,
}

impl AlertKind {
    /// Critical: a life is in danger. Warning: the village is short of
    /// something. Info: routine notes, folded into one line per day.
    pub(crate) fn severity(self) -> Severity {
        match self {
            Self::Death | Self::Starving | Self::Dehydrated | Self::Freezing => Severity::Critical,
            Self::Exhausted
            | Self::Depleted
            | Self::Shortage
            | Self::Homeless
            | Self::SiteWaiting => Severity::Warning,
            Self::Built
            | Self::Occupation
            | Self::Arrival
            | Self::Trade
            | Self::Levy
            | Self::FirstTool
            | Self::ProjectStarted
            | Self::ProjectCompleted
            | Self::ProjectCancelled
            | Self::LaidToRest
            | Self::Woken
            | Self::Social
            | Self::Homestead => Severity::Info,
        }
    }
    /// The reduced Live feed interrupts play for emergencies only.
    pub(crate) fn toast(self) -> bool {
        self.severity() == Severity::Critical
    }
    /// Singular and plural noun of a routine note in a day's fold line.
    fn note(self) -> (&'static str, &'static str) {
        match self {
            Self::Occupation => ("job change", "job changes"),
            Self::Arrival => ("arrival", "arrivals"),
            Self::Trade => ("sale", "sales"),
            Self::Built => ("new cottage", "new cottages"),
            Self::Levy => ("levy day", "levy days"),
            Self::FirstTool => ("first tool", "first tools"),
            Self::ProjectStarted => ("project started", "projects started"),
            Self::ProjectCompleted => ("building finished", "buildings finished"),
            Self::ProjectCancelled => ("project cancelled", "projects cancelled"),
            Self::LaidToRest => ("burial", "burials"),
            Self::Woken => ("sleeper woken", "sleepers woken"),
            Self::Social => ("village note", "village notes"),
            Self::Homestead => ("homestead change", "homestead changes"),
            _ => ("alert", "alerts"),
        }
    }
}

/// Which rows the Oversee feed shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Filter {
    /// Critical and warning rows; routine notes folded into one line a day.
    #[default]
    Attention,
    /// Emergencies only.
    Critical,
    /// Every row on its own line.
    All,
}

impl Filter {
    pub(crate) const ALL: [Self; 3] = [Self::Attention, Self::Critical, Self::All];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Attention => "Important",
            Self::Critical => "Critical",
            Self::All => "Everything",
        }
    }
}

/// One line of the Oversee feed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Row<'a> {
    Alert(&'a Alert),
    /// The routine notes of one game day, as one expandable line.
    Fold {
        day: u64,
        count: u32,
        /// "6 job changes", or "9 notes" when the day mixes kinds.
        label: String,
        /// The breakdown of a mixed day: "6 job changes, 2 sales, 1 levy day".
        detail: Option<String>,
        open: bool,
    },
}

fn plural(count: u32, (one, many): (&'static str, &'static str)) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// "6 job changes" for a day of one kind; "9 notes" and the breakdown
/// "6 job changes, 2 sales, 1 levy day" for a mixed one.
fn fold_label(counts: &BTreeMap<AlertKind, u32>) -> (String, Option<String>) {
    let mut parts: Vec<(AlertKind, u32)> =
        counts.iter().map(|(kind, count)| (*kind, *count)).collect();
    parts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let total: u32 = parts.iter().map(|(_, count)| *count).sum();
    match parts.as_slice() {
        [(kind, count)] => (plural(*count, kind.note()), None),
        _ => (
            plural(total, ("note", "notes")),
            Some(
                parts
                    .iter()
                    .map(|(kind, count)| plural(*count, kind.note()))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        ),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Alert {
    /// Stable identity of the row while it stays in the feed.
    pub(crate) serial: u64,
    pub(crate) kind: AlertKind,
    /// The resident or site the alert is about; also the repeat-folding key.
    pub(crate) subject: Option<u32>,
    /// Where it happened, in simulation millimetres, for click-to-focus.
    pub(crate) focus: Option<(i32, i32)>,
    pub(crate) text: String,
    pub(crate) tick: u64,
    /// How many times this alert repeated inside the folding window.
    pub(crate) count: u32,
    /// Real seconds (host time) when it last arrived; toasts fade by this.
    pub(crate) arrived: f64,
}

impl Alert {
    /// "Today · Bram is starving ×2": `when` is the capitalised date
    /// (`game_ui::words::when`), never the running day count.
    pub(crate) fn line(&self, when: &str) -> String {
        let repeat = if self.count > 1 {
            format!(" ×{}", self.count)
        } else {
            String::new()
        };
        format!("{when} · {}{repeat}", self.text)
    }
}

/// The need a resident's alert is about: `Hunger`, `Thirst`, `Energy` or
/// `Warmth`; `None` for an alert that is not about a need.
pub(crate) fn need_of(kind: AlertKind) -> Option<&'static str> {
    match kind {
        AlertKind::Starving => Some("Hunger"),
        AlertKind::Dehydrated => Some("Thirst"),
        AlertKind::Exhausted => Some("Energy"),
        AlertKind::Freezing => Some("Warmth"),
        _ => None,
    }
}

/// A need alert's reason in the view's own words (design-ui 2.4): the short
/// form for the row's second line and the full sentence for its tooltip.
/// `target` names whom an `OnTheWay` resident goes to; `fallback` is the
/// simulation's own sentence, used as the short form for a cause the view
/// does not word yet.
pub(crate) fn reason_words(
    need: &str,
    cause: Option<&island_content::village::view::NeedCause>,
    name: &str,
    target: Option<&str>,
    fallback: Option<&str>,
) -> (String, String) {
    use island_content::village::view::NeedCause;
    let it = match need {
        "Thirst" => "water",
        _ => "food",
    };
    match (cause, need) {
        (Some(NeedCause::HasItNotUsed), "Thirst") => (
            "has water, has not drunk yet".to_owned(),
            format!("{name} carries water and has not drunk yet."),
        ),
        (Some(NeedCause::HasItNotUsed), "Hunger") => (
            "has food, has not eaten yet".to_owned(),
            format!("{name} carries food and has not eaten yet."),
        ),
        (Some(NeedCause::OnTheWay { good, .. }), _) => {
            let good = crate::game_ui::words::noun(good, 2);
            (
                format!("on the way to get {good}"),
                match target {
                    Some(target) => format!("{name} is on the way to get {good} from {target}."),
                    None => format!("{name} is on the way to get {good}."),
                },
            )
        }
        (Some(NeedCause::NoCoins), "Hunger" | "Thirst") => (
            format!("no {it} and no coins"),
            format!("{name} has no {it} and no coins to buy any."),
        ),
        (Some(NeedCause::NoSeller), "Hunger" | "Thirst") => (
            format!("nobody sells {it} they can pay for"),
            format!("{name} has no {it}, and nobody sells any at a price they can pay."),
        ),
        (Some(NeedCause::SourceEmpty), "Thirst") => (
            "no water, and the wells are dry".to_owned(),
            format!("{name} has no water, and the wells are dry."),
        ),
        (Some(NeedCause::Asleep), "Energy") => (
            "asleep now".to_owned(),
            format!("{name} is asleep. The night restores them."),
        ),
        (_, "Energy") => (
            "needs a night in a bed".to_owned(),
            format!("{name} is tired. A night in a bed restores them."),
        ),
        (Some(NeedCause::NoCoat), "Warmth") => (
            "no warm coat".to_owned(),
            format!("{name} has no warm coat, and it is cold outside."),
        ),
        (Some(NeedCause::Outdoors), "Warmth") => (
            "out in the cold".to_owned(),
            format!("{name} is out in the cold. A heated home warms them."),
        ),
        _ => (
            fallback.map(str::to_owned).unwrap_or_default(),
            format!("{name} needs help."),
        ),
    }
}

/// The head of a row that folds several residents of one village with one
/// need: "3 residents in Harbour are starving".
pub(crate) fn group_title(count: usize, village: &str, kind: AlertKind) -> String {
    let state = match kind {
        AlertKind::Starving => "are starving",
        AlertKind::Dehydrated => "are thirsty",
        AlertKind::Exhausted => "are exhausted",
        AlertKind::Freezing => "are freezing",
        _ => "need help",
    };
    format!("{count} residents in {village} {state}")
}

/// An alert row's tooltip: what a click does, then the full sentence and
/// since when (design-ui 2.4).
pub(crate) fn alert_tip(name: &str, full: &str, since: &str) -> (String, String) {
    (format!("Go to {name}"), format!("{full} Since {since}."))
}

/// A note about something the player ordered: "Tom became a baker, as you
/// asked." The note keeps its own full stop, if it has one.
pub(crate) fn as_you_asked(text: &str, by_order: bool) -> String {
    if !by_order {
        return text.to_owned();
    }
    match text.strip_suffix('.') {
        Some(stem) => format!("{stem}, as you asked."),
        None => format!("{text}, as you asked"),
    }
}

/// One incoming event before it is folded into the feed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Incoming {
    pub(crate) kind: AlertKind,
    pub(crate) subject: Option<u32>,
    pub(crate) focus: Option<(i32, i32)>,
    pub(crate) text: String,
    pub(crate) tick: u64,
}

/// Newest-first, bounded, with repeats of one `(kind, subject)` folded.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Feed {
    entries: VecDeque<Alert>,
    cap: usize,
    window_ticks: u64,
    next_serial: u64,
    /// Bumped on every visible change, so widgets rebuild only when needed.
    pub(crate) revision: u64,
    /// New critical or warning alerts so far (not repeats, not notes): the
    /// alert sound plays when this grows.
    pub(crate) fresh: u64,
}

impl Feed {
    pub(crate) fn new(cap: usize, window_ticks: u64) -> Self {
        Self {
            entries: VecDeque::new(),
            cap: cap.max(1),
            window_ticks,
            next_serial: 1,
            revision: 0,
            fresh: 0,
        }
    }
    pub(crate) fn push(&mut self, incoming: Incoming, arrived: f64) {
        self.revision = self.revision.wrapping_add(1);
        let repeat = self.entries.iter().position(|alert| {
            alert.kind == incoming.kind
                && alert.subject == incoming.subject
                && incoming.tick.saturating_sub(alert.tick) <= self.window_ticks
        });
        if let Some(index) = repeat {
            let mut alert = self.entries.remove(index).expect("repeat index");
            alert.count = alert.count.saturating_add(1);
            alert.tick = incoming.tick;
            alert.text = incoming.text;
            alert.focus = incoming.focus.or(alert.focus);
            alert.arrived = arrived;
            self.entries.push_front(alert);
            return;
        }
        if matches!(
            incoming.kind.severity(),
            Severity::Critical | Severity::Warning
        ) {
            self.fresh = self.fresh.wrapping_add(1);
        }
        self.entries.push_front(Alert {
            serial: self.next_serial,
            kind: incoming.kind,
            subject: incoming.subject,
            focus: incoming.focus,
            text: incoming.text,
            tick: incoming.tick,
            count: 1,
            arrived,
        });
        self.next_serial += 1;
        // Routine notes never push an emergency or a warning out of the feed.
        while self.entries.len() > self.cap {
            let oldest_note = self
                .entries
                .iter()
                .rposition(|alert| alert.kind.severity() == Severity::Info);
            match oldest_note {
                Some(index) => self.entries.remove(index),
                None => self.entries.pop_back(),
            };
        }
    }
    /// Replace what the feed says about one `(kind, subject)`: a condition
    /// that lasts (a site still waiting, a day later) is one row with the
    /// latest wording, not a row per report.
    pub(crate) fn supersede(&mut self, incoming: Incoming, arrived: f64) {
        self.entries
            .retain(|alert| alert.kind != incoming.kind || alert.subject != incoming.subject);
        self.push(incoming, arrived);
    }
    /// The lines of the Oversee feed, newest first. `day_of` names the game
    /// day of a tick; `open` holds the days whose notes are unfolded.
    pub(crate) fn rows(
        &self,
        filter: Filter,
        open: &BTreeSet<u64>,
        day_of: &dyn Fn(u64) -> u64,
    ) -> Vec<Row<'_>> {
        let critical = |alert: &&Alert| alert.kind.severity() == Severity::Critical;
        let note = |alert: &Alert| alert.kind.severity() == Severity::Info;
        let mut rows = Vec::new();
        match filter {
            Filter::All => rows.extend(self.entries.iter().map(Row::Alert)),
            Filter::Critical => rows.extend(self.entries.iter().filter(critical).map(Row::Alert)),
            Filter::Attention => {
                let mut folded = BTreeSet::new();
                for alert in &self.entries {
                    if !note(alert) {
                        rows.push(Row::Alert(alert));
                        continue;
                    }
                    let day = day_of(alert.tick);
                    if !folded.insert(day) {
                        continue;
                    }
                    // The fold sits where the day's newest note would be.
                    let notes: Vec<&Alert> = self
                        .entries
                        .iter()
                        .filter(|other| note(other) && day_of(other.tick) == day)
                        .collect();
                    let mut counts: BTreeMap<AlertKind, u32> = BTreeMap::new();
                    for other in &notes {
                        *counts.entry(other.kind).or_default() += other.count;
                    }
                    let is_open = open.contains(&day);
                    let (label, detail) = fold_label(&counts);
                    rows.push(Row::Fold {
                        day,
                        count: counts.values().sum(),
                        label,
                        detail,
                        open: is_open,
                    });
                    if is_open {
                        rows.extend(notes.into_iter().map(Row::Alert));
                    }
                }
            }
        }
        rows
    }
    /// Rows that need attention: everything above a routine note.
    #[cfg(test)]
    pub(crate) fn attention(&self) -> usize {
        self.entries
            .iter()
            .filter(|alert| alert.kind.severity() != Severity::Info)
            .count()
    }
    /// Take away the alerts `gone` picks (a dead resident's needs).
    pub(crate) fn remove_where(&mut self, gone: impl Fn(&Alert) -> bool) {
        let before = self.entries.len();
        self.entries.retain(|alert| !gone(alert));
        if self.entries.len() != before {
            self.revision = self.revision.wrapping_add(1);
        }
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Alert> {
        self.entries.iter()
    }
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(crate) fn get(&self, serial: u64) -> Option<&Alert> {
        self.entries.iter().find(|alert| alert.serial == serial)
    }
    /// The reduced Live view: at most `limit` recent toast-worthy alerts that
    /// arrived within `secs` of `now`.
    pub(crate) fn toasts(&self, now: f64, secs: f64, limit: usize) -> Vec<&Alert> {
        self.entries
            .iter()
            .filter(|alert| alert.kind.toast() && now - alert.arrived <= secs)
            .take(limit)
            .collect()
    }
}

/// Display noun for a recipe that makes a tool, or `None` for other recipes.
pub(crate) fn tool_recipe(recipe: &str) -> Option<&'static str> {
    use island_content::village::catalog;
    Some(match recipe {
        catalog::TOOL => "hammer",
        catalog::FORGE_AXE => "axe",
        catalog::FORGE_PICKAXE => "pickaxe",
        catalog::FORGE_HOE => "hoe",
        catalog::FORGE_SPEAR => "spear",
        catalog::MAKE_ROD => "fishing rod",
        _ => return None,
    })
}

/// "village_bread" as "bread".
fn good_words(kind: &str) -> String {
    kind.strip_prefix("village_")
        .unwrap_or(kind)
        .replace('_', " ")
}

fn text_field<'a>(
    read: &'a dyn Fn(&str) -> Option<serde_json::Value>,
    key: &str,
) -> Option<String> {
    read(key).and_then(|value| value.as_str().map(str::to_owned))
}

/// What a starving or dangerously thirsty resident's alert says after the
/// name, whichever source reports it.
pub(crate) const STARVING: &str = "is starving";
pub(crate) const DEHYDRATED: &str = "is dangerously thirsty";

/// What a site that is short of materials says after whole days without a
/// delivery. `missing` lists the materials as the projects panel does.
pub(crate) fn waiting_text(label: &str, days: u32, missing: &str) -> String {
    format!(
        "{label} has waited {days} day{} for materials ({missing})",
        if days == 1 { "" } else { "s" }
    )
}

/// The site and the wait ("2 days") of a [`waiting_text`] sentence, for the
/// stack's short title.
pub(crate) fn waiting_parts(text: &str) -> Option<(&str, &str)> {
    let (label, rest) = text.split_once(" has waited ")?;
    let (wait, _) = rest.split_once(" for materials")?;
    (!label.is_empty() && !wait.is_empty()).then_some((label, wait))
}

/// Whether a waiting site is due a new line: on the first whole day, and
/// again each time the count of days grows.
pub(crate) fn waiting_due(days: u32, announced: Option<u32>) -> bool {
    days > 0 && announced.is_none_or(|known| days > known)
}

/// The journal effects that are about a building project, and the payload
/// field that names the site in each.
pub(crate) fn project_site_field(effect: &str) -> Option<&'static str> {
    match effect {
        "VillageSitePlaced" | "VillageSiteCancelled" => Some("site"),
        "VillageBuildingCompleted" => Some("building"),
        _ => None,
    }
}

/// Conditions the simulation journals once, when they begin. Where it does,
/// that effect is their only source; a state read may stand in for it only
/// for what had already begun when the session started.
pub(crate) fn onset(kind: AlertKind) -> bool {
    matches!(
        kind,
        AlertKind::Starving | AlertKind::Dehydrated | AlertKind::Depleted
    )
}

/// Classify one applied journal effect. `read` returns a payload field and
/// `who` names the unit the journal attributed it to. Effects that are not
/// alert-worthy return `None`; resource depletion and first tools need world
/// lookups and are classified by the tracker instead.
pub(crate) fn classify(
    effect: &str,
    who: &str,
    read: &dyn Fn(&str) -> Option<serde_json::Value>,
) -> Option<(AlertKind, String)> {
    let number = |key: &str| read(key).and_then(|value| value.as_i64());
    Some(match effect {
        "VillageDeath" => (AlertKind::Death, format!("{who} has died")),
        // A day after a death; the name then comes from the record of the
        // departed, because the resident is no longer in the world.
        "VillageLaidToRest" => (AlertKind::LaidToRest, format!("{who} was laid to rest")),
        "VillageWoken" => (AlertKind::Woken, format!("{who} was woken before morning")),
        // Live mode's bonds and village life, for the Journal.
        "VillageGift" => (
            AlertKind::Social,
            format!(
                "{who} gave a gift of {}",
                text_field(read, "kind")
                    .map_or_else(|| "goods".to_owned(), |kind| good_words(&kind))
            ),
        ),
        "VillageGiftReturned" => (
            AlertKind::Social,
            format!(
                "{who} gave {} back as a friend",
                text_field(read, "kind")
                    .map_or_else(|| "goods".to_owned(), |kind| good_words(&kind))
            ),
        ),
        "VillageBondTier" => (
            AlertKind::Social,
            format!(
                "{who} became {} with a neighbour",
                text_field(read, "tier").unwrap_or_else(|| "closer".into())
            ),
        ),
        "VillageInvitation" => (
            AlertKind::Social,
            format!("{who} sent an invitation to the next gathering"),
        ),
        "VillageFriendHelped" => (AlertKind::Social, format!("{who} lent a friend a hand")),
        "VillageGatheringBegan" => (
            AlertKind::Social,
            format!(
                "{} began in the square",
                text_field(read, "name").unwrap_or_else(|| "A gathering".into())
            ),
        ),
        "VillageGathering" => (
            AlertKind::Social,
            format!(
                "{} ended; {} came",
                text_field(read, "name").unwrap_or_else(|| "A gathering".into()),
                number("attended").unwrap_or(0)
            ),
        ),
        "VillageMourning" => (
            AlertKind::Social,
            format!(
                "The village mourns {}",
                text_field(read, "for").unwrap_or_else(|| "a resident".into())
            ),
        ),
        "VillageHomesteadUpgraded" => (
            AlertKind::Homestead,
            format!(
                "{who} finished {}",
                text_field(read, "name").unwrap_or_else(|| "a homestead improvement".into())
            ),
        ),
        "VillageHomesteadUpgradeStarted" => (
            AlertKind::Homestead,
            format!(
                "{who} began {}",
                text_field(read, "name").unwrap_or_else(|| "a homestead improvement".into())
            ),
        ),
        "VillageHomesteadOrder" => (
            AlertKind::Homestead,
            format!(
                "{who} ordered {} {} for the homestead",
                number("quantity").unwrap_or(0),
                text_field(read, "kind")
                    .map_or_else(|| "goods".to_owned(), |kind| good_words(&kind))
            ),
        ),
        "VillageHomesteadBuilderHired" => (
            AlertKind::Homestead,
            format!("{who} hired a builder for the homestead"),
        ),
        "VillageStarvationOnset" => (AlertKind::Starving, format!("{who} {STARVING}")),
        "VillageThirstOnset" => (AlertKind::Dehydrated, format!("{who} {DEHYDRATED}")),
        "VillageColdExposure" => (
            AlertKind::Freezing,
            format!("{who} is freezing and losing health"),
        ),
        "VillageExhausted" => (
            AlertKind::Exhausted,
            format!("{who} collapsed from exhaustion"),
        ),
        "VillageCottageBuilt" => (
            AlertKind::Built,
            format!("{who} finished building a cottage"),
        ),
        "RegionalOccupationChanged" => {
            let from = text_field(read, "from").unwrap_or_else(|| "work".into());
            let to = text_field(read, "to").unwrap_or_else(|| "new work".into());
            // An order of the player's reads "as you asked" (2.7).
            let by_order = read("by_order").and_then(|value| value.as_bool()) == Some(true);
            (
                AlertKind::Occupation,
                as_you_asked(&format!("{who} changed from {from} to {to}"), by_order),
            )
        }
        // The player's orders, as the village took them (2.7).
        "LivingProfessionTargetSet" => {
            let village =
                text_field(read, "settlement_name").unwrap_or_else(|| "The village".to_owned());
            let trade = text_field(read, "trade").unwrap_or_else(|| "worker".to_owned());
            let by_order = read("by_order").and_then(|value| value.as_bool()) == Some(true);
            let text = match number("wanted") {
                Some(wanted) => format!(
                    "{village} wants {}",
                    crate::game_ui::words::count(u64::try_from(wanted).unwrap_or(0), &trade)
                ),
                None => format!(
                    "{village} decides its {} again",
                    crate::game_ui::words::noun(&trade, 2)
                ),
            };
            (AlertKind::Social, as_you_asked(&text, by_order))
        }
        "LivingVillagePrioritySet" => {
            let village =
                text_field(read, "settlement_name").unwrap_or_else(|| "The village".to_owned());
            let topic = text_field(read, "topic").unwrap_or_default();
            let what = match (topic.as_str(), text_field(read, "profession")) {
                ("workplace", Some(job)) => format!(
                    "{} workplaces",
                    crate::game_ui::words::noun(&job.replace('_', " "), 2)
                ),
                ("workplace", None) => "workplaces".to_owned(),
                ("food_reserve", _) => "food".to_owned(),
                ("fuel_reserve", _) => "firewood".to_owned(),
                ("boat_orders", _) => "the boat's orders".to_owned(),
                (other, _) => other.replace('_', " "),
            };
            let by_order = read("by_order").and_then(|value| value.as_bool()) == Some(true);
            let text = match text_field(read, "level").as_deref() {
                Some("high") => format!("{village} sees to {what} first"),
                Some("low") => format!("{village} leaves {what} for last"),
                _ => format!("{village} sees to {what} as usual"),
            };
            (AlertKind::Social, as_you_asked(&text, by_order))
        }
        "VillageArrival" => (AlertKind::Arrival, format!("{who} arrived on the island")),
        // A resident put on a coat for the cold.
        "VillageCoatWorn" => (AlertKind::Social, format!("{who} put on a warm coat")),
        "VillageNewLife" => (
            AlertKind::Arrival,
            format!("{who} begins a new life in the village"),
        ),
        "VillageTrade" => {
            let good = text_field(read, "kind").unwrap_or_else(|| "goods".to_owned());
            let quantity = u64::try_from(number("quantity").unwrap_or(1)).unwrap_or(1);
            (
                AlertKind::Trade,
                format!(
                    "{who} bought {} for {} coins",
                    crate::game_ui::words::count(quantity, &good),
                    number("coins").unwrap_or(0)
                ),
            )
        }
        // The merchant boat's order for its next call, and its end.
        "VillageBoatOrder" => {
            let goods: Vec<String> = read("order")
                .and_then(|order| order.as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|order| {
                    let good = order.get("good")?.as_str()?;
                    let units = order.get("units")?.as_u64()?;
                    let price = order.get("unit_price")?.as_u64()?;
                    Some(format!(
                        "{} at {price} coins each",
                        crate::game_ui::words::count(units, good)
                    ))
                })
                .collect();
            if goods.is_empty() {
                return None;
            }
            (
                AlertKind::Trade,
                format!(
                    "The merchant boat will buy {} at its next call",
                    goods.join(" and ")
                ),
            )
        }
        "VillageBoatOrderClosed" => {
            let good = text_field(read, "good").unwrap_or_else(|| "goods".to_owned());
            let units = u64::try_from(number("units").unwrap_or(0)).unwrap_or(0);
            let delivered = number("delivered").unwrap_or(0);
            (
                AlertKind::Trade,
                format!(
                    "The boat's order for {} closed with {delivered} sold",
                    crate::game_ui::words::count(units, &good)
                ),
            )
        }
        "VillageSitePlaced" => {
            let label = text_field(read, "label").unwrap_or_else(|| "a building".into());
            let planned = read("civic").and_then(|value| value.as_bool()) == Some(true);
            let reason = text_field(read, "reason").unwrap_or_default();
            (
                AlertKind::ProjectStarted,
                if planned && !reason.is_empty() {
                    format!("The village began {label}: {reason}")
                } else if planned {
                    format!("The village began {label}")
                } else {
                    format!("You placed {label}; residents will supply and build it")
                },
            )
        }
        "VillageBuildingCompleted" => (
            AlertKind::ProjectCompleted,
            format!(
                "{} is finished",
                text_field(read, "label").unwrap_or_else(|| "A building".into())
            ),
        ),
        "VillageSiteCancelled" => {
            let label = text_field(read, "label").unwrap_or_else(|| "A building site".into());
            let reason = text_field(read, "reason").unwrap_or_default();
            (
                AlertKind::ProjectCancelled,
                if reason.is_empty() {
                    format!("{label} was cancelled")
                } else {
                    format!("{label} was cancelled: {reason}")
                },
            )
        }
        "VillageCivicLevy" => (
            AlertKind::Levy,
            format!(
                "Levy day: {} coins collected, {} granted, fund {}",
                number("contributed").unwrap_or(0),
                number("floor_grants").unwrap_or(0) + number("dividend").unwrap_or(0),
                number("balance").unwrap_or(0)
            ),
        ),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn incoming(kind: AlertKind, subject: u32, tick: u64) -> Incoming {
        Incoming {
            kind,
            subject: Some(subject),
            focus: Some((1000, 2000)),
            text: format!("{kind:?} {subject} at {tick}"),
            tick,
        }
    }

    #[test]
    fn a_coat_put_on_reads_as_a_sentence() {
        let coat = json!({"coat": "warm_clothes"});
        let read = |key: &str| coat.get(key).cloned();
        assert_eq!(
            classify("VillageCoatWorn", "Ada", &read),
            Some((AlertKind::Social, "Ada put on a warm coat".to_owned()))
        );
    }

    #[test]
    fn the_boat_order_and_its_end_read_as_sentences() {
        let order = json!({"next_visit_day": 9, "order": [
            {"good": "plank", "units": 10, "unit_price": 14},
            {"good": "ingot", "units": 1, "unit_price": 30}]});
        let read = |key: &str| order.get(key).cloned();
        assert_eq!(
            classify("VillageBoatOrder", "", &read),
            Some((
                AlertKind::Trade,
                "The merchant boat will buy 10 planks at 14 coins each and 1 ingot at 30 coins each at its next call".to_owned()
            ))
        );
        let closed = json!({"good": "plank", "units": 10, "delivered": 3, "unit_price": 14});
        let read = |key: &str| closed.get(key).cloned();
        assert_eq!(
            classify("VillageBoatOrderClosed", "", &read).map(|(_, text)| text),
            Some("The boat's order for 10 planks closed with 3 sold".to_owned())
        );
        let empty = json!({"order": []});
        let read = |key: &str| empty.get(key).cloned();
        assert_eq!(classify("VillageBoatOrder", "", &read), None);
    }

    #[test]
    fn only_a_new_critical_or_warning_alert_counts_for_the_sound() {
        let mut feed = Feed::new(10, 1200);
        feed.push(incoming(AlertKind::Freezing, 7, 100), 1.0);
        assert_eq!(feed.fresh, 1);
        feed.push(incoming(AlertKind::Freezing, 7, 300), 2.0);
        assert_eq!(feed.fresh, 1, "a repeat folds in silently");
        feed.push(incoming(AlertKind::Built, 9, 150), 3.0);
        assert_eq!(feed.fresh, 1, "a note is silent");
        feed.push(incoming(AlertKind::Shortage, 3, 400), 4.0);
        assert_eq!(feed.fresh, 2);
    }

    #[test]
    fn repeats_fold_into_one_row_and_move_to_the_front() {
        let mut feed = Feed::new(10, 1200);
        feed.push(incoming(AlertKind::Freezing, 7, 100), 1.0);
        feed.push(incoming(AlertKind::Built, 9, 150), 2.0);
        feed.push(incoming(AlertKind::Freezing, 7, 900), 3.0);
        assert_eq!(feed.len(), 2, "the repeat did not add a row");
        let first = feed.iter().next().unwrap();
        assert_eq!(
            (first.kind, first.count, first.tick),
            (AlertKind::Freezing, 2, 900)
        );
        assert_eq!(first.serial, 1, "the row keeps its identity for the widget");
        assert!(first.line("Today").ends_with("×2"));
        // Another resident, or the same one long after, is a new row.
        feed.push(incoming(AlertKind::Freezing, 8, 950), 4.0);
        feed.push(incoming(AlertKind::Freezing, 7, 900 + 1201), 5.0);
        assert_eq!(feed.len(), 4);
        assert_eq!(feed.iter().next().unwrap().count, 1);
    }

    #[test]
    fn the_feed_is_capped_and_drops_the_oldest() {
        let mut feed = Feed::new(3, 10);
        for subject in 0..5 {
            feed.push(
                incoming(AlertKind::Built, subject, u64::from(subject) * 100),
                0.0,
            );
        }
        assert_eq!(feed.len(), 3);
        let subjects: Vec<_> = feed.iter().map(|alert| alert.subject.unwrap()).collect();
        assert_eq!(subjects, [4, 3, 2], "newest first; the two oldest fell off");
        assert!(feed.get(1).is_none() && feed.get(5).is_some());
    }

    #[test]
    fn live_toasts_are_recent_emergencies_only() {
        let mut feed = Feed::new(10, 10);
        feed.push(incoming(AlertKind::Occupation, 1, 0), 10.0);
        feed.push(incoming(AlertKind::Death, 2, 0), 11.0);
        feed.push(incoming(AlertKind::Exhausted, 3, 0), 12.0);
        feed.push(incoming(AlertKind::Built, 4, 0), 13.0);
        feed.push(incoming(AlertKind::Starving, 5, 0), 14.0);
        feed.push(incoming(AlertKind::Levy, 6, 0), 15.0);
        feed.push(incoming(AlertKind::Depleted, 7, 0), 15.5);
        feed.push(incoming(AlertKind::Freezing, 8, 0), 15.6);
        feed.push(incoming(AlertKind::Dehydrated, 9, 0), 15.7);
        let toasts: Vec<_> = feed.toasts(16.0, 8.0, 3).iter().map(|a| a.kind).collect();
        assert_eq!(
            toasts,
            [
                AlertKind::Dehydrated,
                AlertKind::Freezing,
                AlertKind::Starving
            ],
            "limited to three, newest first; warnings and notes never interrupt play"
        );
        let all: Vec<_> = feed.toasts(16.0, 8.0, 9).iter().map(|a| a.kind).collect();
        assert_eq!(all.len(), 4, "the death is the fourth and last toast");
        assert!(feed.toasts(40.0, 8.0, 3).is_empty(), "toasts fade");
    }

    #[test]
    fn every_kind_has_the_agreed_severity() {
        use AlertKind::*;
        for kind in [Death, Starving, Dehydrated, Freezing] {
            assert_eq!(kind.severity(), Severity::Critical, "{kind:?}");
            assert!(kind.toast());
        }
        for kind in [Depleted, Shortage, Homeless, Exhausted, SiteWaiting] {
            assert_eq!(kind.severity(), Severity::Warning, "{kind:?}");
            assert!(!kind.toast());
        }
        for kind in [
            Occupation,
            Arrival,
            Trade,
            Built,
            Levy,
            FirstTool,
            ProjectStarted,
            ProjectCompleted,
            ProjectCancelled,
        ] {
            assert_eq!(kind.severity(), Severity::Info, "{kind:?}");
            assert!(!kind.toast());
        }
        assert!(Severity::Critical > Severity::Warning && Severity::Warning > Severity::Info);
    }

    /// 2,400 ticks to the day, as in the authored world.
    fn day(tick: u64) -> u64 {
        tick / 2400 + 1
    }

    fn lines(feed: &Feed, filter: Filter, open: &[u64]) -> Vec<String> {
        let open: BTreeSet<u64> = open.iter().copied().collect();
        feed.rows(filter, &open, &day)
            .into_iter()
            .map(|row| match row {
                Row::Alert(alert) => format!("{:?} {}", alert.kind, alert.subject.unwrap_or(0)),
                Row::Fold {
                    day,
                    count,
                    label,
                    detail,
                    open,
                } => format!(
                    "fold day {day}: {label}{} ({count}){}",
                    detail.map_or_else(String::new, |detail| format!(": {detail}")),
                    if open { " open" } else { "" }
                ),
            })
            .collect()
    }

    #[test]
    fn routine_notes_fold_into_one_line_per_day_and_unfold_on_request() {
        let mut feed = Feed::new(40, 10);
        // Day 20: six job changes around one death and one empty patch.
        let base = 19 * 2400;
        for subject in 1..=3 {
            feed.push(incoming(AlertKind::Occupation, subject, base + 10), 0.0);
        }
        feed.push(incoming(AlertKind::Death, 50, base + 20), 0.0);
        for subject in 4..=6 {
            feed.push(incoming(AlertKind::Occupation, subject, base + 30), 0.0);
        }
        feed.push(incoming(AlertKind::Depleted, 60, base + 40), 0.0);
        // Day 21: a levy, a trade repeated five times and one more job change.
        let next = base + 2400;
        feed.push(incoming(AlertKind::Levy, 0, next + 5), 0.0);
        for step in 0..5 {
            feed.push(incoming(AlertKind::Trade, 0, next + 6 + step), 0.0);
        }
        feed.push(incoming(AlertKind::Occupation, 7, next + 50), 0.0);

        assert_eq!(
            lines(&feed, Filter::Attention, &[]),
            [
                "fold day 21: 7 notes: 5 sales, 1 job change, 1 levy day (7)",
                "Depleted 60",
                "fold day 20: 6 job changes (6)",
                "Death 50",
            ],
            "critical and warning rows stay; each day's notes are one line"
        );
        assert_eq!(feed.attention(), 2);
        // Unfolding day 20 lists its six notes under the fold, newest first.
        let open = lines(&feed, Filter::Attention, &[20]);
        assert_eq!(open.len(), 4 + 6);
        assert_eq!(open[2], "fold day 20: 6 job changes (6) open");
        assert_eq!(
            &open[3..9],
            [
                "Occupation 6",
                "Occupation 5",
                "Occupation 4",
                "Occupation 3",
                "Occupation 2",
                "Occupation 1"
            ]
        );
        assert_eq!(open[9], "Death 50");
        // The other filters never fold.
        assert_eq!(lines(&feed, Filter::Critical, &[20]), ["Death 50"]);
        let all = lines(&feed, Filter::All, &[]);
        assert_eq!(all.len(), feed.len());
        assert!(all.iter().all(|line| !line.starts_with("fold")));
        assert_eq!(Filter::default(), Filter::Attention);
        assert_eq!(
            Filter::ALL.map(Filter::label),
            ["Important", "Critical", "Everything"]
        );
    }

    #[test]
    fn a_flood_of_notes_never_evicts_what_needs_attention() {
        let mut feed = Feed::new(5, 10);
        feed.push(incoming(AlertKind::Death, 1, 0), 0.0);
        feed.push(incoming(AlertKind::Homeless, 2, 1), 0.0);
        for subject in 10..40 {
            feed.push(incoming(AlertKind::Occupation, subject, 100), 0.0);
        }
        assert_eq!(feed.len(), 5);
        assert_eq!(feed.attention(), 2, "the death and the warning are kept");
        let newest = feed.iter().next().unwrap();
        assert_eq!(newest.subject, Some(39), "the newest notes are kept too");
        // Only when nothing routine is left does the oldest row fall off.
        for subject in 50..56 {
            feed.push(incoming(AlertKind::Starving, subject, 200), 0.0);
        }
        assert_eq!(feed.len(), 5);
        assert!(feed.iter().all(|alert| alert.kind == AlertKind::Starving));
    }

    #[test]
    fn journal_effects_become_readable_alerts() {
        let payload = json!({"from": "farmer", "to": "fisher", "reason": "vacancy"});
        let read = |key: &str| payload.get(key).cloned();
        assert_eq!(
            classify("RegionalOccupationChanged", "Ada", &read),
            Some((
                AlertKind::Occupation,
                "Ada changed from farmer to fisher".into()
            ))
        );
        let levy =
            json!({"day": 21, "contributed": 30, "floor_grants": 4, "dividend": 6, "balance": 120});
        let read = |key: &str| levy.get(key).cloned();
        assert_eq!(
            classify("VillageCivicLevy", "The village", &read),
            Some((
                AlertKind::Levy,
                "Levy day: 30 coins collected, 10 granted, fund 120".into()
            ))
        );
        let none = |_: &str| None;
        assert_eq!(
            classify("VillageDeath", "Bram", &none).unwrap().0,
            AlertKind::Death
        );
        assert_eq!(
            classify("VillageColdExposure", "Bram", &none).unwrap().0,
            AlertKind::Freezing
        );
        // A burial and a broken night are notes, never alarms.
        assert_eq!(
            classify("VillageLaidToRest", "Bram", &none),
            Some((AlertKind::LaidToRest, "Bram was laid to rest".into()))
        );
        assert_eq!(
            classify("VillageWoken", "Ada", &none),
            Some((AlertKind::Woken, "Ada was woken before morning".into()))
        );
        for kind in [AlertKind::LaidToRest, AlertKind::Woken] {
            assert_eq!(kind.severity(), Severity::Info);
            assert!(!kind.toast());
        }
        assert_eq!(
            classify("VillageCottageBuilt", "Jun", &none).unwrap().0,
            AlertKind::Built
        );
        assert_eq!(
            classify("VillageArrival", "Jun", &none).unwrap().0,
            AlertKind::Arrival
        );
        let trade =
            json!({"buyer": 3, "seller": 4, "kind": "village_flour", "quantity": 2, "coins": 6});
        let read = |key: &str| trade.get(key).cloned();
        assert_eq!(
            classify("VillageTrade", "Jun", &read),
            Some((AlertKind::Trade, "Jun bought 2 flour for 6 coins".into())),
            "a trade is a routine note"
        );
        assert_eq!(
            classify("VillageGathered", "Jun", &none),
            None,
            "everyday work is not an alert"
        );
        // Onsets the simulation journals itself.
        assert_eq!(
            classify("VillageStarvationOnset", "Ada", &none),
            Some((AlertKind::Starving, "Ada is starving".into()))
        );
        assert_eq!(
            classify("VillageThirstOnset", "Ada", &none),
            Some((AlertKind::Dehydrated, "Ada is dangerously thirsty".into()))
        );
        assert_eq!(
            classify("VillagePatchDepleted", "Ada", &none),
            None,
            "an emptied patch is about the patch; the tracker looks it up"
        );
        for kind in [
            AlertKind::Starving,
            AlertKind::Dehydrated,
            AlertKind::Depleted,
        ] {
            assert!(onset(kind), "{kind:?}");
        }
        for kind in [AlertKind::Homeless, AlertKind::Shortage, AlertKind::Death] {
            assert!(!onset(kind), "{kind:?}");
        }

        assert_eq!(tool_recipe("village_forge_axe"), Some("axe"));
        assert_eq!(tool_recipe("village_bake"), None);
    }

    #[test]
    fn building_projects_become_notes_and_a_stalled_site_a_warning() {
        let placed = json!({"site": 812, "building": "cottage", "label": "Harbour cottage 3",
            "civic": true, "reason": "2 resident(s) have had no bed for 3 reviews"});
        let read = |key: &str| placed.get(key).cloned();
        assert_eq!(
            classify("VillageSitePlaced", "A resident", &read),
            Some((
                AlertKind::ProjectStarted,
                "The village began Harbour cottage 3: 2 resident(s) have had no bed for 3 reviews"
                    .into()
            ))
        );
        let own = json!({"site": 813, "label": "Harbour barn 1", "civic": false,
            "reason": "placed by the overseer"});
        let read = |key: &str| own.get(key).cloned();
        assert_eq!(
            classify("VillageSitePlaced", "A resident", &read)
                .unwrap()
                .1,
            "You placed Harbour barn 1; residents will supply and build it"
        );
        let done = json!({"building": 812, "definition": "cottage", "label": "Harbour cottage 3"});
        let read = |key: &str| done.get(key).cloned();
        assert_eq!(
            classify("VillageBuildingCompleted", "Ada", &read),
            Some((
                AlertKind::ProjectCompleted,
                "Harbour cottage 3 is finished".into()
            ))
        );
        // The cancelled site is gone; the tracker supplies its remembered label.
        let gone = json!({"site": 813, "building": "barn", "label": "Harbour barn 1",
            "reason": "cancelled by the overseer"});
        let read = |key: &str| gone.get(key).cloned();
        assert_eq!(
            classify("VillageSiteCancelled", "A resident", &read),
            Some((
                AlertKind::ProjectCancelled,
                "Harbour barn 1 was cancelled: cancelled by the overseer".into()
            ))
        );
        let bare = json!({"site": 813, "building": "barn"});
        let read = |key: &str| bare.get(key).cloned();
        assert_eq!(
            classify("VillageSiteCancelled", "A resident", &read)
                .unwrap()
                .1,
            "A building site was cancelled"
        );
        assert_eq!(project_site_field("VillageSitePlaced"), Some("site"));
        assert_eq!(
            project_site_field("VillageBuildingCompleted"),
            Some("building")
        );
        assert_eq!(project_site_field("VillageSiteCancelled"), Some("site"));
        assert_eq!(project_site_field("VillageTrade"), None);
        for kind in [
            AlertKind::ProjectStarted,
            AlertKind::ProjectCompleted,
            AlertKind::ProjectCancelled,
        ] {
            assert_eq!(kind.severity(), Severity::Info, "{kind:?}");
        }
        assert_eq!(AlertKind::SiteWaiting.severity(), Severity::Warning);

        assert_eq!(
            waiting_text("Harbour cottage 3", 1, "planks 3/8 · logs 4/4"),
            "Harbour cottage 3 has waited 1 day for materials (planks 3/8 · logs 4/4)"
        );
        assert!(waiting_text("Mill", 3, "logs 0/6").contains("waited 3 days"));
        assert!(!waiting_due(0, None), "less than a day is not a wait");
        assert!(waiting_due(1, None));
        assert!(!waiting_due(1, Some(1)), "announced once per day count");
        assert!(waiting_due(2, Some(1)));
    }

    #[test]
    fn a_lasting_condition_keeps_one_row_with_the_latest_wording() {
        let mut feed = Feed::new(10, 10);
        feed.push(incoming(AlertKind::Homeless, 4, 0), 0.0);
        let mut waiting = incoming(AlertKind::SiteWaiting, 812, 24_000);
        waiting.text = "one day".into();
        feed.supersede(waiting.clone(), 1.0);
        waiting.tick = 48_000;
        waiting.text = "two days".into();
        feed.supersede(waiting, 2.0);
        let rows: Vec<_> = feed
            .iter()
            .map(|alert| (alert.kind, alert.text.clone()))
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], (AlertKind::SiteWaiting, "two days".into()));
        assert_eq!(
            feed.iter().next().unwrap().count,
            1,
            "replaced, not repeated"
        );
        assert_eq!(feed.attention(), 2);
        // Another site's wait is its own row.
        feed.supersede(incoming(AlertKind::SiteWaiting, 900, 48_000), 3.0);
        assert_eq!(feed.len(), 3);
    }
}
