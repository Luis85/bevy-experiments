//! Pure projections for the island session bar: which way of playing is
//! active, what a mode button does for the current life, and the shared clock.
//! Nothing here reads a `World`; the widget layer supplies plain values.
use crate::IslandMode;
use crate::colony_panel::Trend;
use crate::game_ui::icon::Icon;
use crate::game_ui::scale::LayoutClass;
use bevy::prelude::KeyCode;

/// The three ways to play one persistent island, in bar order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModeTab {
    Live,
    Oversee,
    Shape,
}

impl ModeTab {
    pub(crate) const ALL: [Self; 3] = [Self::Live, Self::Oversee, Self::Shape];

    pub(crate) fn mode(self) -> IslandMode {
        match self {
            Self::Live => IslandMode::Character,
            Self::Oversee => IslandMode::Inspector,
            Self::Shape => IslandMode::Editor,
        }
    }
    pub(crate) fn of(mode: IslandMode) -> Self {
        match mode {
            IslandMode::Character => Self::Live,
            IslandMode::Inspector => Self::Oversee,
            IslandMode::Editor => Self::Shape,
        }
    }
    /// The name alone: the binding is a key cap, never part of the label.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Live => "Live",
            Self::Oversee => "Oversee",
            Self::Shape => "Shape",
        }
    }
    pub(crate) fn key_cap(self) -> &'static str {
        match self {
            Self::Live => "1",
            Self::Oversee => "2",
            Self::Shape => "3",
        }
    }
    pub(crate) fn icon(self) -> Icon {
        match self {
            Self::Live => Icon::Live,
            Self::Oversee => Icon::Oversee,
            Self::Shape => Icon::Shape,
        }
    }
    /// One sentence for the segment's tooltip.
    pub(crate) fn summary(self) -> &'static str {
        match self {
            Self::Live => "Live as your character on the island.",
            Self::Oversee => "Watch the villages, read the ledger and place buildings.",
            Self::Shape => "Raise, lower and paint the land.",
        }
    }
    /// Its key, from the one table of keys (the same in every mode).
    pub(crate) fn key(self) -> KeyCode {
        use crate::keymap::{self as map, Mode::Live};
        map::key(
            Live,
            match self {
                Self::Live => map::LIVE,
                Self::Oversee => map::OVERSEE,
                Self::Shape => map::SHAPE,
            },
        )
    }
}

/// What the played identity can do right now, as far as the bar is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LifeState {
    /// A living character that native input may control.
    Playable,
    /// The named visitor has not arrived; Live performs the recorded arrival.
    AwaitingArrival,
    /// The life has ended; Live leads to the successor choices.
    Ended,
    /// Another controller owns the character.
    Foreign,
    /// No played identity exists in this world.
    Missing,
}

/// The effect of choosing a mode. The widget layer performs it through the
/// existing control coordinator; this function never changes a mode itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModeIntent {
    /// Already there: nothing to request.
    Stay,
    Switch(IslandMode),
    /// Live with an ended life: reopen the successor / keep-watching choices.
    LifeChoices,
    Refuse(&'static str),
}

pub(crate) const FOREIGN_REASON: &str =
    "Live is unavailable: another controller owns your character.";
pub(crate) const MISSING_REASON: &str = "Live is unavailable: this island has no character.";

pub(crate) fn intent(current: IslandMode, requested: ModeTab, life: LifeState) -> ModeIntent {
    if requested != ModeTab::Live {
        return if current == requested.mode() {
            ModeIntent::Stay
        } else {
            ModeIntent::Switch(requested.mode())
        };
    }
    match life {
        LifeState::Foreign => ModeIntent::Refuse(FOREIGN_REASON),
        LifeState::Missing => ModeIntent::Refuse(MISSING_REASON),
        LifeState::Ended => ModeIntent::LifeChoices,
        LifeState::Playable | LifeState::AwaitingArrival if current == IslandMode::Character => {
            ModeIntent::Stay
        }
        LifeState::Playable | LifeState::AwaitingArrival => {
            ModeIntent::Switch(IslandMode::Character)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ModeButton {
    pub(crate) tab: ModeTab,
    pub(crate) active: bool,
    pub(crate) enabled: bool,
    pub(crate) reason: Option<&'static str>,
}

pub(crate) fn mode_bar(current: IslandMode, life: LifeState) -> [ModeButton; 3] {
    ModeTab::ALL.map(|tab| {
        let reason = match intent(current, tab, life) {
            ModeIntent::Refuse(reason) => Some(reason),
            _ => None,
        };
        ModeButton {
            tab,
            active: ModeTab::of(current) == tab,
            enabled: reason.is_none(),
            reason,
        }
    })
}

/// Why a mode button is disabled. Shown under the bar for as long as it holds.
pub(crate) fn mode_reason(current: IslandMode, life: LifeState) -> Option<&'static str> {
    mode_bar(current, life)
        .iter()
        .find_map(|button| button.reason)
}

/// What choosing Live will do when that is not simply "play your character".
pub(crate) fn live_tip(current: IslandMode, life: LifeState) -> Option<&'static str> {
    if current == IslandMode::Character {
        return None;
    }
    match life {
        LifeState::AwaitingArrival => Some("Live begins your character's arrival on the island."),
        LifeState::Ended => Some("Your character's life has ended. Live offers a successor."),
        _ => None,
    }
}

/// The sentence of a mode segment's tooltip: why it is disabled, what Live
/// will do when that is not simply "play your character", or what the mode
/// is. Nothing sits under the bar permanently.
pub(crate) fn mode_tooltip(current: IslandMode, tab: ModeTab, life: LifeState) -> &'static str {
    let button = mode_bar(current, life)
        .into_iter()
        .find(|button| button.tab == tab)
        .expect("every tab is on the bar");
    button
        .reason
        .or_else(|| live_tip(current, life).filter(|_| tab == ModeTab::Live))
        .unwrap_or(tab.summary())
}

/// Speeds offered by the clock; the same presets the host keys step through.
pub(crate) const SPEEDS: [u32; 3] = [1, 3, 10];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClockAction {
    TogglePause,
    Speed(u32),
    Slower,
    Faster,
}

/// Next `(paused, multiplier)`. Choosing a speed resumes; `-` / `=` keep the
/// pause state; resuming from a 0x host speed returns to 1x.
pub(crate) fn clock_step(paused: bool, multiplier: u32, action: ClockAction) -> (bool, u32) {
    let at = SPEEDS
        .iter()
        .position(|speed| *speed >= multiplier)
        .unwrap_or(SPEEDS.len() - 1);
    match action {
        ClockAction::TogglePause if paused || multiplier == 0 => (false, multiplier.max(1)),
        ClockAction::TogglePause => (true, multiplier),
        ClockAction::Speed(speed) => (false, speed.max(1)),
        ClockAction::Slower => (paused, SPEEDS[at.saturating_sub(1)]),
        ClockAction::Faster => (paused, SPEEDS[(at + 1).min(SPEEDS.len() - 1)]),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClockModel {
    /// 1-based day shown to the player.
    pub(crate) day: u64,
    pub(crate) season: String,
    pub(crate) hour: u32,
    pub(crate) minute: u32,
    pub(crate) phase: &'static str,
    pub(crate) stopped: bool,
    pub(crate) multiplier: u32,
    /// Position within the day, 0 at sunrise.
    pub(crate) permille: u32,
}

impl ClockModel {
    /// `time_permille` is the position within the day (0 = sunrise) and
    /// `night_permille` the share of the day that is night, at its end.
    pub(crate) fn new(
        day_index: u64,
        season: impl Into<String>,
        time_permille: u32,
        night_permille: u32,
        paused: bool,
        multiplier: u32,
    ) -> Self {
        let time = time_permille.min(999);
        // The day starts at sunrise: map it onto a 24-hour face from 06:00.
        let minutes = (6 * 60 + time * 24 * 60 / 1000) % (24 * 60);
        let night_from = 1000_u32.saturating_sub(night_permille.min(1000));
        let phase = if time >= night_from {
            "Night"
        } else {
            // Daylight split into four equal named parts.
            match time * 4 / night_from.max(1) {
                0 => "Morning",
                1 => "Midday",
                2 => "Afternoon",
                _ => "Evening",
            }
        };
        Self {
            day: day_index + 1,
            season: season.into(),
            hour: minutes / 60,
            minute: minutes % 60,
            phase,
            stopped: paused || multiplier == 0,
            multiplier,
            permille: time,
        }
    }
    /// "07:30", as the calendar shows it.
    pub(crate) fn time_text(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
    /// How far the day has run, for the line under the date.
    pub(crate) fn day_fraction(&self) -> f32 {
        self.permille as f32 / 1000.0
    }
    /// The highlighted speed; none while stopped (the pause control is lit).
    pub(crate) fn active_speed(&self) -> Option<u32> {
        (!self.stopped).then(|| {
            SPEEDS
                .iter()
                .copied()
                .find(|speed| *speed >= self.multiplier)
                .unwrap_or(SPEEDS[SPEEDS.len() - 1])
        })
    }
    /// Short state for other widgets ("Paused" / "3×").
    pub(crate) fn rate(&self) -> String {
        if self.stopped {
            "Paused".into()
        } else {
            format!("{}×", self.multiplier)
        }
    }
}

/// Season names in calendar order; the index is `day / days_per_season % 4`,
/// as in the simulation's `calendar_at`.
pub(crate) const SEASONS: [&str; 4] = ["Spring", "Summer", "Autumn", "Winter"];
const WINTER: usize = 3;

/// The read-only calendar behind the clock: where the day sits in its season
/// and when the light comes and goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CalendarModel {
    /// Index into [`SEASONS`].
    pub(crate) season: usize,
    /// 1-based day within the season.
    pub(crate) day_of_season: u64,
    pub(crate) days_per_season: u64,
    /// Days until the next season's first day; 1 is tomorrow.
    pub(crate) days_to_next: u64,
    pub(crate) sunrise: String,
    pub(crate) sunset: String,
}

impl CalendarModel {
    /// `day_index` is the simulation's 0-based calendar day and
    /// `night_permille` the share of the day, at its end, that is night.
    pub(crate) fn new(day_index: u64, days_per_season: u64, night_permille: u32) -> Self {
        let length = days_per_season.max(1);
        let into = day_index % length;
        let face = |permille: u32| {
            // The same 24-hour face as the clock: the day starts at 06:00.
            let minutes = (6 * 60 + permille.min(999) * 24 * 60 / 1000) % (24 * 60);
            format!("{:02}:{:02}", minutes / 60, minutes % 60)
        };
        Self {
            season: (day_index / length % 4) as usize,
            day_of_season: into + 1,
            days_per_season: length,
            days_to_next: length - into,
            sunrise: face(0),
            sunset: face(1000_u32.saturating_sub(night_permille.min(1000))),
        }
    }
    pub(crate) fn next_season(&self) -> usize {
        (self.season + 1) % 4
    }
    /// "Autumn, day 6": the date as the bar shows it.
    pub(crate) fn date_text(&self) -> String {
        format!("{}, day {}", SEASONS[self.season], self.day_of_season)
    }
    /// "Autumn · day 6 of 7".
    pub(crate) fn season_line(&self) -> String {
        format!(
            "{} · day {} of {}",
            SEASONS[self.season], self.day_of_season, self.days_per_season
        )
    }
    /// "Winter begins in 2 days".
    pub(crate) fn next_line(&self) -> String {
        let next = SEASONS[self.next_season()];
        match self.days_to_next {
            1 => format!("{next} begins tomorrow"),
            days => format!("{next} begins in {days} days"),
        }
    }
    pub(crate) fn light_line(&self) -> String {
        format!("Sunrise {} · sunset {}", self.sunrise, self.sunset)
    }
}

/// What the running world says a season changes. A field the world does not
/// expose stays empty and its line is left out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SeasonFacts {
    /// Residents feel cold (a world with needs).
    pub(crate) cold: bool,
    /// The regional ecology regrows patches; `winter_growth` lists the goods
    /// whose rate differs in winter as `(name, winter rate, usual rate)`.
    pub(crate) regional: bool,
    pub(crate) winter_growth: Vec<(String, u32, u32)>,
    /// The older island refills its grain fields by season.
    pub(crate) seasonal_grain: bool,
    /// Logs a hearth burns in a winter day and in any other day.
    pub(crate) hearth_logs: Option<(u32, u32)>,
}

/// One line for growth and one for cold in `season`, where the world has
/// something to say about them.
pub(crate) fn season_lines(season: usize, facts: &SeasonFacts) -> Vec<String> {
    let winter = season % 4 == WINTER;
    let mut lines = Vec::new();
    if facts.regional {
        let slowed: Vec<String> = facts
            .winter_growth
            .iter()
            .filter(|(_, in_winter, usual)| in_winter != usual)
            .map(|(name, in_winter, usual)| {
                if *in_winter == 0 {
                    format!("{name} does not regrow")
                } else if in_winter < usual && usual % in_winter == 0 {
                    format!("{name} regrows {}× slower", usual / in_winter)
                } else {
                    format!(
                        "{name} regrows at {}% of its usual rate",
                        in_winter * 100 / (*usual).max(1)
                    )
                }
            })
            .collect();
        lines.push(if winter && !slowed.is_empty() {
            format!("Growth: {}; the rest as usual.", slowed.join(", "))
        } else if winter {
            "Growth: fields and woods regrow as in any season.".to_owned()
        } else {
            "Growth: fields and woods regrow at their full rate.".to_owned()
        });
    } else if facts.seasonal_grain {
        lines.push(
            match season % 4 {
                WINTER => "Growth: grain fields refill to a quarter each day.",
                1 => "Growth: grain fields refill completely each day.",
                _ => "Growth: grain fields refill to three quarters each day.",
            }
            .to_owned(),
        );
    }
    if facts.cold {
        let mut line = if winter {
            "Cold: all day and night; a coat or a lit hearth keeps a resident warm.".to_owned()
        } else {
            "Cold: only at night.".to_owned()
        };
        if let Some((in_winter, usual)) = facts.hearth_logs
            && winter
            && in_winter != usual
        {
            line.push_str(&format!(
                " A hearth burns {in_winter} logs a day instead of {usual}."
            ));
        }
        lines.push(line);
    }
    lines
}

/// The popover's text: this season, then what the next one will change.
pub(crate) fn calendar_text(calendar: &CalendarModel, facts: &SeasonFacts) -> (String, String) {
    let mut now = vec![calendar.season_line(), calendar.light_line()];
    now.extend(season_lines(calendar.season, facts));
    let mut next = vec![calendar.next_line()];
    next.extend(season_lines(calendar.next_season(), facts));
    (now.join("\n"), next.join("\n"))
}

/// Where the merchant boat is, for the calendar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BoatOutlook {
    /// In at the trading post until this clock time.
    In { until: String },
    /// Due in this many days; 0 is later today.
    Due { days: u64 },
}

/// The days ahead as the calendar tells them: the weather of each coming
/// day, what today's weather does, a season's hardship and the boat.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Outlook {
    /// "clear", "rain": today's weather, lower case.
    pub(crate) today: String,
    /// Tomorrow first.
    pub(crate) ahead: Vec<String>,
    pub(crate) growth_percent: u32,
    pub(crate) warmth_loss_percent: u32,
    /// "drought", "harsh winter".
    pub(crate) shock: Option<String>,
    pub(crate) boat: Option<BoatOutlook>,
    /// What the boat wants at its next call (or, in port, this call), one
    /// sentence each: [`boat_order_lines`].
    pub(crate) boat_orders: Vec<String>,
}

/// One good the merchant boat announced it will buy: `(good, units, coins a
/// unit, units bought so far)`, from `BoatView::next_order`.
pub(crate) type BoatOrderRow = (String, u32, u32, u32);

/// The boat's orders as sentences, goods through the one plural helper.
/// Before it calls: "Next call wants 10 planks at 14 coins each." In port:
/// "In port, it wants 10 planks at 14 coins each: 3 sold so far." Nothing
/// when it wants nothing.
pub(crate) fn boat_order_lines(orders: &[BoatOrderRow], docked: bool) -> Vec<String> {
    orders
        .iter()
        .map(|(good, units, price, delivered)| {
            let goods = crate::game_ui::words::count(u64::from(*units), good);
            let coins = if *price == 1 { "coin" } else { "coins" };
            if docked {
                format!(
                    "In port, it wants {goods} at {price} {coins} each: {delivered} sold so far."
                )
            } else {
                format!("Next call wants {goods} at {price} {coins} each.")
            }
        })
        .collect()
}

/// The outlook's lines; empty in a world with neither weather nor a boat.
/// A village gathering for the calendar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GatheringLine {
    pub(crate) name: String,
    /// 0 today, 1 tomorrow.
    pub(crate) days_away: u64,
    /// "13:26\u{2013}16:48".
    pub(crate) hours: String,
    pub(crate) active: bool,
    /// How many came last time, once it has been held.
    pub(crate) last: Option<u32>,
}

/// The calendar's gathering lines: `(today, ahead)`. Today's say so and
/// whether they are under way; the others say when.
/// The calendar's gatherings line on an island whose rules hold no feast day.
pub(crate) const NO_GATHERINGS: &str =
    "No gatherings or mourning: feast days and a death in the village are listed here.";

pub(crate) fn gathering_lines(list: &[GatheringLine]) -> (Vec<String>, Vec<String>) {
    let (mut today, mut ahead) = (Vec::new(), Vec::new());
    for gathering in list {
        let last = gathering
            .last
            .map_or_else(String::new, |came| format!(" \u{b7} last time {came} came"));
        if gathering.active {
            today.push(format!(
                "Now: {} in the square, {}{last}",
                gathering.name, gathering.hours
            ));
        } else if gathering.days_away == 0 {
            today.push(format!(
                "Today: {}, {}{last}",
                gathering.name, gathering.hours
            ));
        } else {
            let when = if gathering.days_away == 1 {
                "tomorrow".to_owned()
            } else {
                format!("in {} days", gathering.days_away)
            };
            ahead.push(format!(
                "{} {when}, {}{last}",
                gathering.name, gathering.hours
            ));
        }
    }
    (today, ahead)
}

/// "Ada mourns Bram for 2 more days."
pub(crate) fn mourning_line(name: &str, whom: &str, days_left: u64) -> String {
    match days_left {
        0 => format!("{name} mourns {whom} today."),
        1 => format!("{name} mourns {whom} for 1 more day."),
        days => format!("{name} mourns {whom} for {days} more days."),
    }
}

pub(crate) fn outlook_lines(outlook: &Outlook) -> Vec<String> {
    let mut lines = Vec::new();
    if !outlook.today.is_empty() {
        let mut today = format!("Today: {}.", outlook.today);
        if outlook.growth_percent != 100 {
            today.push_str(&format!(" Crops grow at {}%.", outlook.growth_percent));
        }
        if outlook.warmth_loss_percent != 100 {
            today.push_str(&format!(
                " Warmth is lost at {}%.",
                outlook.warmth_loss_percent
            ));
        }
        lines.push(today);
    }
    if !outlook.ahead.is_empty() {
        let days: Vec<String> = outlook
            .ahead
            .iter()
            .enumerate()
            .map(|(index, weather)| match index {
                0 => format!("tomorrow {weather}"),
                index => format!("in {} days {weather}", index + 1),
            })
            .collect();
        let mut line = days.join(" \u{b7} ");
        if let Some(first) = line.get(..1) {
            line = first.to_uppercase() + &line[1..];
        }
        lines.push(format!("{line}."));
    }
    if let Some(shock) = &outlook.shock {
        lines.push(format!("This season brings a {shock}."));
    }
    match &outlook.boat {
        Some(BoatOutlook::In { until }) => lines.push(format!(
            "The merchant boat is in at the trading post until {until}."
        )),
        Some(BoatOutlook::Due { days: 0 }) => {
            lines.push("The merchant boat calls later today.".to_owned());
        }
        Some(BoatOutlook::Due { days: 1 }) => {
            lines.push("The merchant boat calls tomorrow.".to_owned());
        }
        Some(BoatOutlook::Due { days }) => {
            lines.push(format!("The merchant boat calls in {days} days."));
        }
        None => {}
    }
    lines.extend(outlook.boat_orders.iter().cloned());
    lines
}

/// What gives way on a Standard-width bar when more than five totals are on
/// it (a building in hand adds logs and planks): first the weather chip,
/// whose icon moves into the date, then the words of the modes that are not
/// active. `(weather folds, mode words fold)`.
pub(crate) fn bar_crowding(class: LayoutClass, totals_shown: usize) -> (bool, bool) {
    match class {
        LayoutClass::Compact => (true, true),
        LayoutClass::Standard => (totals_shown > 5, totals_shown > 6),
        LayoutClass::Wide => (false, false),
    }
}

/// Where the toast lane (the Paused pill and the notices) may stand:
/// `(left, right)` insets of the window. Centred in the window while the pill
/// there clears the Live workspace; beside a wide workspace it is centred in
/// what is left between the goal tracker and the workspace; where that is too
/// narrow for a sentence, under the folded goal tracker. The third value is
/// how far the lane drops below the bar.
#[cfg(test)]
pub(crate) fn toast_lane(width: f32, workspace: f32, goal: f32) -> (f32, f32, f32) {
    toast_lane_for(width, workspace, goal, false)
}

/// The lane's right inset beside an open right column `column` px wide
/// (0: none): never less than the column.
pub(crate) fn beside_column(right: f32, column: f32) -> f32 {
    right.max(column)
}

/// Whether a new host toast may take the place of the one showing:
/// `current` is (has a button, seconds left). A toast with a button is kept
/// until it expires, unless the new one has a button too.
pub(crate) fn replaces_toast(current: Option<(bool, Option<f32>)>, new_has_button: bool) -> bool {
    match current {
        Some((true, Some(left))) if left > 0.0 => new_has_button,
        _ => true,
    }
}

/// The Paused pill's words: "Paused", "Paused · 1 order waiting",
/// "Paused · 3 orders waiting".
pub(crate) fn paused_words(waiting: usize) -> String {
    match waiting {
        0 => "Paused".to_owned(),
        1 => "Paused \u{b7} 1 order waiting".to_owned(),
        waiting => format!("Paused \u{b7} {waiting} orders waiting"),
    }
}

/// [`toast_lane`] while the Paused pill counts waiting orders: the pill is
/// wider then.
pub(crate) fn toast_lane_for(
    width: f32,
    workspace: f32,
    goal: f32,
    counting: bool,
) -> (f32, f32, f32) {
    use crate::game_ui::tokens as t;
    // Half of the Paused pill with its key cap, and a little air; wider
    // with "· 2 orders waiting".
    let half_pill: f32 = if counting { 150.0 } else { 90.0 };
    // A sentence-long toast needs about this much.
    const TOAST: f32 = 320.0;
    // The goal tracker folds to this height while a workspace is open.
    const GOAL_FOLDED: f32 = 72.0;
    if workspace <= 0.0 || width * 0.5 + half_pill <= width - workspace - t::SCREEN_MARGIN {
        (0.0, 0.0, 0.0)
    } else if width - workspace - t::SCREEN_MARGIN - goal >= TOAST {
        (goal, workspace + t::SCREEN_MARGIN, 0.0)
    } else {
        // Too narrow beside the goal tracker: under it, across what the
        // workspace leaves free.
        (t::SCREEN_MARGIN, workspace + t::SCREEN_MARGIN, GOAL_FOLDED)
    }
}

/// The pages of the ledger, in dock order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LedgerPage {
    #[default]
    Village,
    Economy,
    People,
    Projects,
    Journal,
}

impl LedgerPage {
    pub(crate) const ALL: [Self; 5] = [
        Self::Village,
        Self::Economy,
        Self::People,
        Self::Projects,
        Self::Journal,
    ];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Village => "Village",
            Self::Economy => "Economy",
            Self::People => "People",
            Self::Projects => "Projects",
            Self::Journal => "Journal",
        }
    }
    pub(crate) fn icon(self) -> Icon {
        match self {
            Self::Village => Icon::VillagePage,
            Self::Economy => Icon::EconomyPage,
            Self::People => Icon::PeoplePage,
            Self::Projects => Icon::ProjectsPage,
            Self::Journal => Icon::JournalPage,
        }
    }
    /// Its key, from the one table of keys.
    pub(crate) fn key(self) -> KeyCode {
        use crate::keymap::{self as map, Mode::Oversee};
        map::key(
            Oversee,
            match self {
                Self::Village => map::LEDGER_VILLAGE,
                Self::Economy => map::LEDGER_ECONOMY,
                Self::People => map::LEDGER_PEOPLE,
                Self::Projects => map::LEDGER_PROJECTS,
                Self::Journal => map::LEDGER_JOURNAL,
            },
        )
    }
    pub(crate) fn key_cap(self) -> &'static str {
        match self {
            Self::Village => "F1",
            Self::Economy => "F2",
            Self::People => "F3",
            Self::Projects => "F4",
            Self::Journal => "F5",
        }
    }
    pub(crate) fn summary(self) -> &'static str {
        match self {
            Self::Village => "Homes, wellbeing and the colony's policies.",
            Self::Economy => "Stock, prices and professions in each village.",
            Self::People => "Every resident: where they are and how they fare.",
            Self::Projects => "The civic fund and every open building site.",
            Self::Journal => "Everything that happened, day by day.",
        }
    }
}

/// What the single right column holds. Never two at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Column {
    Empty,
    Ledger(LedgerPage),
    /// The inspector of a resident, a building or a site.
    Inspector,
}

/// The right column's state: closed by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ColumnState {
    pub(crate) ledger: Option<LedgerPage>,
    /// The page F11 reopens.
    pub(crate) last: LedgerPage,
    /// A resident, a building or a site is selected.
    pub(crate) selected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColumnEvent {
    /// A dock tile, F1 to F5 or a total: open that page, or close it again.
    Page(LedgerPage),
    /// F11: the last page.
    Last,
    /// Close, Esc or leaving Oversee: the column empties and the selection clears.
    Close,
    /// Something was selected in the world or in a list: the inspector
    /// replaces the ledger.
    Selected,
    Deselected,
}

impl ColumnState {
    pub(crate) fn column(self) -> Column {
        match (self.ledger, self.selected) {
            (Some(page), _) => Column::Ledger(page),
            (None, true) => Column::Inspector,
            (None, false) => Column::Empty,
        }
    }
    pub(crate) fn step(self, event: ColumnEvent) -> Self {
        match event {
            ColumnEvent::Page(page) if self.ledger == Some(page) => Self {
                ledger: None,
                selected: false,
                ..self
            },
            ColumnEvent::Page(page) => Self {
                ledger: Some(page),
                last: page,
                ..self
            },
            ColumnEvent::Last => self.step(ColumnEvent::Page(self.ledger.unwrap_or(self.last))),
            ColumnEvent::Close => Self {
                ledger: None,
                selected: false,
                ..self
            },
            ColumnEvent::Selected => Self {
                ledger: None,
                selected: true,
                ..self
            },
            ColumnEvent::Deselected => Self {
                selected: false,
                ..self
            },
        }
    }
}

/// The village totals of the top bar, in bar order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TotalKind {
    Residents,
    Morale,
    Food,
    Water,
    Logs,
    Planks,
    Fund,
}

impl TotalKind {
    pub(crate) const ALL: [Self; 7] = [
        Self::Residents,
        Self::Morale,
        Self::Food,
        Self::Water,
        Self::Logs,
        Self::Planks,
        Self::Fund,
    ];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Residents => "Residents",
            Self::Morale => "Morale",
            Self::Food => "Food",
            Self::Water => "Clean water",
            Self::Logs => "Logs",
            Self::Planks => "Planks",
            Self::Fund => "Civic fund",
        }
    }
    pub(crate) fn icon(self) -> Icon {
        match self {
            Self::Residents => Icon::Residents,
            Self::Morale => Icon::Morale,
            Self::Food => Icon::Food,
            Self::Water => Icon::CleanWater,
            Self::Logs => Icon::Logs,
            Self::Planks => Icon::Planks,
            Self::Fund => Icon::Coins,
        }
    }
    /// The ledger page a click opens.
    pub(crate) fn page(self) -> LedgerPage {
        match self {
            Self::Residents => LedgerPage::People,
            Self::Morale => LedgerPage::Village,
            Self::Food | Self::Water | Self::Logs | Self::Planks => LedgerPage::Economy,
            Self::Fund => LedgerPage::Projects,
        }
    }
    /// Catalogue key of the good a building cost names, for the totals that are goods.
    pub(crate) fn good(self) -> Option<&'static str> {
        match self {
            Self::Logs => Some("log"),
            Self::Planks => Some("plank"),
            _ => None,
        }
    }
}

/// The mark after a total's number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TotalMark {
    /// Below one day's use, or below what open sites still need.
    Low,
    /// Moved by more than a tenth since yesterday.
    Up,
    Down,
}

/// What the simulation exposes for the totals; plain numbers only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TotalsInput {
    pub(crate) residents: u32,
    pub(crate) housed: u32,
    pub(crate) beds: u32,
    pub(crate) households: u32,
    pub(crate) morale_percent: i32,
    pub(crate) social_events: u64,
    pub(crate) food: u32,
    pub(crate) water: u32,
    pub(crate) logs: u32,
    pub(crate) planks: u32,
    pub(crate) fund: u32,
    pub(crate) spendable: u32,
    pub(crate) held: u32,
    pub(crate) open_sites: usize,
    /// Logs and planks open sites still wait for.
    pub(crate) needed: [u32; 2],
    /// Change since yesterday for food, water, logs and planks.
    pub(crate) trends: [Option<Trend>; 4],
    /// Per-village stock of logs and planks: `(village, logs, planks)`.
    pub(crate) villages: Vec<(String, u32, u32)>,
    /// Goods the building in hand costs, by catalogue key.
    pub(crate) in_hand: Vec<String>,
}

/// One total as the bar shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Total {
    pub(crate) kind: TotalKind,
    pub(crate) value: String,
    pub(crate) unit: Option<String>,
    pub(crate) mark: Option<TotalMark>,
    /// A building in hand draws on this total.
    pub(crate) in_use: bool,
    /// Tooltip: the exact reading, one sentence, then facts.
    pub(crate) reading: String,
    pub(crate) sentence: String,
    pub(crate) facts: Vec<String>,
}

fn trend_mark(trend: Option<Trend>) -> Option<TotalMark> {
    match trend {
        Some(Trend::Rising(_)) => Some(TotalMark::Up),
        Some(Trend::Falling(_)) => Some(TotalMark::Down),
        _ => None,
    }
}

fn trend_fact(trend: Option<Trend>) -> Option<String> {
    Some(match trend? {
        Trend::Rising(by) => format!("Up {by} since yesterday."),
        Trend::Falling(by) => format!("Down {by} since yesterday."),
        Trend::Steady => "Steady since yesterday.".to_owned(),
        Trend::Unknown => return None,
    })
}

/// The seven totals. A total is marked low when it is below one unit per
/// resident (the alert feed's own shortage rule) or below what open sites
/// still need; otherwise it shows a trend when it moved.
pub(crate) fn totals(input: &TotalsInput) -> Vec<Total> {
    let in_use = |kind: TotalKind| {
        kind.good()
            .is_some_and(|good| input.in_hand.iter().any(|kind| kind == good))
    };
    let click = |page: LedgerPage| format!("Click: open {}", page.label());
    TotalKind::ALL
        .into_iter()
        .map(|kind| {
            let (value, unit, mark, reading, sentence, mut facts) = match kind {
                TotalKind::Residents => (
                    input.residents.to_string(),
                    Some(format!("/ {}", input.beds)),
                    (input.housed < input.residents).then_some(TotalMark::Low),
                    format!("{} of {} beds", input.residents, input.beds),
                    if input.housed < input.residents {
                        format!(
                            "{} without a home.",
                            input.residents.saturating_sub(input.housed)
                        )
                    } else {
                        "Everyone has a home.".to_owned()
                    },
                    vec![format!(
                        "{} housed in {} households.",
                        input.housed, input.households
                    )],
                ),
                TotalKind::Morale => (
                    format!("{}%", input.morale_percent),
                    None,
                    None,
                    format!("{}%", input.morale_percent),
                    "The average morale of the residents.".to_owned(),
                    vec![format!("{} social encounters so far.", input.social_events)],
                ),
                TotalKind::Food | TotalKind::Water => {
                    let (units, slot, noun) = if kind == TotalKind::Food {
                        (input.food, 0, "portions")
                    } else {
                        (input.water, 1, "units")
                    };
                    let low = input.residents > 0 && units < input.residents;
                    (
                        units.to_string(),
                        None,
                        if low {
                            Some(TotalMark::Low)
                        } else {
                            trend_mark(input.trends[slot])
                        },
                        format!("{units} {noun}"),
                        if low {
                            format!(
                                "Low: less than one for each of {} residents.",
                                input.residents
                            )
                        } else {
                            "Held by residents across the villages.".to_owned()
                        },
                        trend_fact(input.trends[slot]).into_iter().collect(),
                    )
                }
                TotalKind::Logs | TotalKind::Planks => {
                    let slot = usize::from(kind == TotalKind::Planks);
                    let units = if slot == 0 { input.logs } else { input.planks };
                    let needed = input.needed[slot];
                    let low = needed > units;
                    let mut facts = Vec::new();
                    if !input.villages.is_empty() {
                        facts.push(
                            input
                                .villages
                                .iter()
                                .map(|(name, logs, planks)| {
                                    format!("{name} {}", if slot == 0 { logs } else { planks })
                                })
                                .collect::<Vec<_>>()
                                .join(" · "),
                        );
                    }
                    facts.extend(trend_fact(input.trends[2 + slot]));
                    (
                        units.to_string(),
                        None,
                        if low {
                            Some(TotalMark::Low)
                        } else {
                            trend_mark(input.trends[2 + slot])
                        },
                        format!("{units} in the villages"),
                        if needed == 0 {
                            "No open site is waiting for them.".to_owned()
                        } else if low {
                            format!("Running low: open sites still need {needed}.")
                        } else {
                            format!("Open sites still need {needed}.")
                        },
                        facts,
                    )
                }
                // The chip reads the civic fund itself; what can be spent
                // now and what is held back are its sentence and facts. (A
                // chip of the spendable part read "0 fund" while the fund
                // held coins in reserve.)
                TotalKind::Fund => (
                    input.fund.to_string(),
                    Some("fund".to_owned()),
                    None,
                    format!("{} coins in the civic fund", input.fund),
                    if input.open_sites == 0 {
                        format!(
                            "{} coins to spend on village buildings; no site is open.",
                            input.spendable
                        )
                    } else if input.fund == 0 {
                        // Held means set aside from coins the fund has; an
                        // empty fund holds nothing and its sites wait.
                        format!(
                            "The fund is empty: {} open site{} wait for about {} coins.",
                            input.open_sites,
                            if input.open_sites == 1 { "" } else { "s" },
                            input.held
                        )
                    } else {
                        format!(
                            "{} coins to spend; {} held for {} open site{}.",
                            input.spendable,
                            input.held.min(input.fund),
                            input.open_sites,
                            if input.open_sites == 1 { "" } else { "s" }
                        )
                    },
                    vec![
                        "The civic fund collects a levy and pays residents for the materials of village buildings; part of it is kept in reserve.".to_owned(),
                    ],
                ),
            };
            facts.push(click(kind.page()));
            Total {
                kind,
                value,
                unit,
                mark,
                in_use: in_use(kind),
                reading,
                sentence,
                facts,
            }
        })
        .collect()
}

/// Whether a total has a place on the bar. At the Standard width logs and
/// planks appear only while low or while a building that costs them is in hand.
pub(crate) fn total_shown(total: &Total, class: LayoutClass) -> bool {
    class == LayoutClass::Wide
        || !matches!(total.kind, TotalKind::Logs | TotalKind::Planks)
        || total.in_use
        || total.mark == Some(TotalMark::Low)
}

/// Height of the second bar row that carries the totals at the Compact width.
pub(crate) const TOTALS_ROW: f32 = 28.0;

/// Whether the totals sit on a row of their own under the bar. Compact has
/// no room for them beside the mode switch and the clock.
pub(crate) fn totals_on_second_row(class: LayoutClass, overseeing: bool) -> bool {
    overseeing && class == LayoutClass::Compact
}

/// The three bottom docks of Oversee as `(left, width)`, tiles included.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DockLayout {
    pub(crate) tile: f32,
    pub(crate) left: f32,
    pub(crate) build: (f32, f32),
    pub(crate) ledger: (f32, f32),
}

/// Tiles on the ledger dock: one per page, or the one "Ledger" tile where the
/// Compact layout has no room for five.
pub(crate) fn ledger_tiles(class: LayoutClass) -> usize {
    if class == LayoutClass::Compact {
        1
    } else {
        LedgerPage::ALL.len()
    }
}

/// Whether the ledger's five tabs show their icons: the Compact column has
/// room for the names alone.
pub(crate) fn ledger_tab_icons(class: LayoutClass) -> bool {
    class != LayoutClass::Compact
}

/// Dock rectangles for a window `width` wide with `build_tiles` categories.
/// The left dock hugs the left edge, the ledger dock the right edge and the
/// build dock is centred between what is left.
pub(crate) fn dock_layout(width: f32, class: LayoutClass, build_tiles: usize) -> DockLayout {
    let tile = if class == LayoutClass::Wide {
        64.0
    } else {
        56.0
    };
    let (padding, gap, margin) = (5.0, 4.0, 8.0);
    let run = |tiles: usize| {
        2.0 * padding + tiles as f32 * tile + tiles.saturating_sub(1) as f32 * gap + 2.0
    };
    let left = run(2);
    let ledger = run(ledger_tiles(class));
    // The Build caption (44) and a divider (1) sit before the categories.
    let build = run(build_tiles) + 44.0 + gap + 1.0 + gap;
    DockLayout {
        tile,
        left,
        build: (((width - build) * 0.5).round(), build),
        ledger: (width - margin - ledger, ledger),
    }
}

/// Width of the right column for what it holds.
pub(crate) fn column_width(column: Column, class: LayoutClass) -> f32 {
    use crate::game_ui::tokens as t;
    match (column, class) {
        (Column::Empty, _) => 0.0,
        (Column::Ledger(_), LayoutClass::Wide) => t::COLUMN_LEDGER,
        (Column::Ledger(_), LayoutClass::Standard) => t::COLUMN_LEDGER_STANDARD,
        (Column::Inspector, LayoutClass::Wide) => t::COLUMN_INSPECTOR,
        (Column::Inspector, LayoutClass::Standard) => t::COLUMN_INSPECTOR_STANDARD,
        (_, LayoutClass::Compact) => t::COLUMN_INSPECTOR_STANDARD,
    }
}

/// A notice for the shell's lane. While one shows, a new one waits its turn
/// (the same words are not queued twice); a waiting line shortens the one
/// showing to two seconds, so a burst of outcomes is read in order.
pub(crate) fn queue_notice(
    current: &mut Option<(String, f32)>,
    queued: &mut std::collections::VecDeque<String>,
    text: String,
    secs: f32,
) {
    match current {
        None => *current = Some((text, secs)),
        Some((shown, _)) if *shown == text => {}
        Some(_) if queued.contains(&text) => {}
        Some(_) => {
            queued.push_back(text);
            // At most a handful wait; the oldest gives way.
            while queued.len() > 4 {
                queued.pop_front();
            }
        }
    }
}

/// Count the notice down; the next waiting one follows when it ends.
pub(crate) fn advance_notice(
    current: &mut Option<(String, f32)>,
    queued: &mut std::collections::VecDeque<String>,
    delta: f32,
    secs: f32,
) {
    if let Some((_, left)) = current.as_mut() {
        *left -= delta;
        if !queued.is_empty() {
            *left = left.min(2.0);
        }
    }
    if current.as_ref().is_some_and(|(_, left)| *left <= 0.0) {
        *current = queued.pop_front().map(|text| (text, secs));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_is_reachable_and_exactly_one_is_active() {
        for current in [
            IslandMode::Inspector,
            IslandMode::Character,
            IslandMode::Editor,
        ] {
            let bar = mode_bar(current, LifeState::Playable);
            assert_eq!(bar.iter().filter(|button| button.active).count(), 1);
            assert!(bar.iter().all(|button| button.enabled));
            for tab in ModeTab::ALL {
                let expected = if tab.mode() == current {
                    ModeIntent::Stay
                } else {
                    ModeIntent::Switch(tab.mode())
                };
                assert_eq!(intent(current, tab, LifeState::Playable), expected);
                assert_eq!(ModeTab::of(tab.mode()), tab);
            }
        }
    }

    #[test]
    fn live_follows_the_existing_arrival_successor_and_ownership_rules() {
        let from = IslandMode::Inspector;
        assert_eq!(
            intent(from, ModeTab::Live, LifeState::AwaitingArrival),
            ModeIntent::Switch(IslandMode::Character),
            "the coordinator performs the recorded arrival on entry"
        );
        assert_eq!(
            intent(from, ModeTab::Live, LifeState::Ended),
            ModeIntent::LifeChoices
        );
        assert_eq!(
            intent(from, ModeTab::Live, LifeState::Foreign),
            ModeIntent::Refuse(FOREIGN_REASON)
        );
        let bar = mode_bar(from, LifeState::Foreign);
        assert!(!bar[0].enabled && bar[0].reason == Some(FOREIGN_REASON));
        assert!(bar[1].enabled && bar[2].enabled, "tools stay reachable");
        assert_eq!(
            mode_reason(from, LifeState::Foreign),
            Some(FOREIGN_REASON),
            "a disabled reason needs no pointer"
        );
        assert_eq!(
            mode_tooltip(from, ModeTab::Live, LifeState::Foreign),
            FOREIGN_REASON,
            "and it is the disabled segment's tooltip"
        );
        assert_eq!(
            mode_reason(IslandMode::Character, LifeState::Playable),
            None
        );
        // A dead life never blocks the tools.
        assert_eq!(
            intent(IslandMode::Character, ModeTab::Shape, LifeState::Ended),
            ModeIntent::Switch(IslandMode::Editor)
        );
    }

    #[test]
    fn the_live_tip_is_the_live_segments_tooltip_and_nothing_sits_under_the_bar() {
        for life in [LifeState::AwaitingArrival, LifeState::Ended] {
            for mode in [IslandMode::Inspector, IslandMode::Editor] {
                assert_eq!(mode_reason(mode, life), None, "no permanent line");
                assert_eq!(
                    Some(mode_tooltip(mode, ModeTab::Live, life)),
                    live_tip(mode, life)
                );
                // The other segments say what they are.
                assert_eq!(
                    mode_tooltip(mode, ModeTab::Shape, life),
                    ModeTab::Shape.summary()
                );
            }
            assert_eq!(
                mode_tooltip(IslandMode::Character, ModeTab::Live, life),
                ModeTab::Live.summary()
            );
        }
        assert_eq!(
            mode_tooltip(IslandMode::Inspector, ModeTab::Live, LifeState::Playable),
            ModeTab::Live.summary(),
            "a living character needs no explanation"
        );
        // No label carries its key: the key is a cap beside it.
        for tab in ModeTab::ALL {
            assert!(!tab.label().contains('·') && tab.label().len() <= 7);
            assert_eq!(tab.key_cap().len(), 1);
        }
    }

    #[test]
    fn clock_actions_are_the_same_in_every_mode() {
        assert_eq!(clock_step(false, 3, ClockAction::TogglePause), (true, 3));
        assert_eq!(clock_step(true, 3, ClockAction::TogglePause), (false, 3));
        assert_eq!(clock_step(false, 0, ClockAction::TogglePause), (false, 1));
        assert_eq!(clock_step(true, 1, ClockAction::Speed(10)), (false, 10));
        assert_eq!(clock_step(false, 1, ClockAction::Faster), (false, 3));
        assert_eq!(clock_step(false, 3, ClockAction::Faster), (false, 10));
        assert_eq!(clock_step(false, 10, ClockAction::Faster), (false, 10));
        assert_eq!(clock_step(true, 10, ClockAction::Slower), (true, 3));
        assert_eq!(clock_step(false, 1, ClockAction::Slower), (false, 1));
        // A host speed between presets steps to its neighbours.
        assert_eq!(clock_step(false, 5, ClockAction::Slower), (false, 3));
    }

    #[test]
    fn clock_names_day_season_time_and_the_active_rate() {
        let dawn = ClockModel::new(19, "Autumn", 0, 300, false, 1);
        assert_eq!((dawn.day, dawn.season.as_str()), (20, "Autumn"));
        assert_eq!(
            (dawn.time_text().as_str(), dawn.phase),
            ("06:00", "Morning")
        );
        assert_eq!(dawn.active_speed(), Some(1));
        assert_eq!(dawn.day_fraction(), 0.0);

        let afternoon = ClockModel::new(0, "Spring", 400, 300, false, 3);
        assert_eq!(afternoon.phase, "Afternoon");
        assert_eq!((afternoon.hour, afternoon.minute), (15, 36));
        assert_eq!(afternoon.active_speed(), Some(3));

        let night = ClockModel::new(0, "Winter", 700, 300, true, 10);
        assert_eq!(night.phase, "Night");
        assert_eq!(night.active_speed(), None, "pause is the lit control");
        assert_eq!(night.day_fraction(), 0.7);
        assert_eq!(night.rate(), "Paused");
        let late = ClockModel::new(0, "Winter", 999, 300, false, 10);
        assert_eq!(late.hour, 5, "the face wraps past midnight before sunrise");
        assert_eq!(late.rate(), "10×");
    }

    #[test]
    fn the_calendar_places_the_day_in_its_season() {
        // The living world opens on calendar day 19 with seven-day seasons.
        let opening = CalendarModel::new(19, 7, 300);
        assert_eq!(opening.date_text(), "Autumn, day 6");
        assert_eq!(opening.season_line(), "Autumn · day 6 of 7");
        assert_eq!(opening.next_line(), "Winter begins in 2 days");
        assert_eq!(opening.light_line(), "Sunrise 06:00 · sunset 22:48");
        let last = CalendarModel::new(20, 7, 300);
        assert_eq!(last.season_line(), "Autumn · day 7 of 7");
        assert_eq!(last.next_line(), "Winter begins tomorrow");
        let winter = CalendarModel::new(21, 7, 300);
        assert_eq!(winter.season_line(), "Winter · day 1 of 7");
        assert_eq!(winter.next_line(), "Spring begins in 7 days");
        // The year wraps; day 0 is the first day of spring.
        assert_eq!(
            CalendarModel::new(28, 7, 300).season_line(),
            "Spring · day 1 of 7"
        );
        assert_eq!(CalendarModel::new(0, 7, 300).season, 0);
        // Sunset follows the world's share of night; a season is never shorter than a day.
        assert_eq!(CalendarModel::new(0, 7, 500).sunset, "18:00");
        assert_eq!(CalendarModel::new(0, 7, 0).sunset, "05:58");
        assert_eq!(CalendarModel::new(5, 0, 300).days_to_next, 1);
    }

    #[test]
    fn the_calendar_agrees_with_the_simulation_calendar() {
        use island_content::village::{DAYS_PER_SEASON, calendar_at};
        use npc_framework::prelude::SimConfig;
        let config = SimConfig::default();
        for day in 0..60 {
            let tick = day * config.day_length + 17;
            let (index, season, _) = calendar_at(tick, &config, true);
            let calendar = CalendarModel::new(index, DAYS_PER_SEASON, config.night_permille);
            assert_eq!(SEASONS[calendar.season], format!("{season:?}"), "day {day}");
            // Counting down the days reaches a different season exactly then.
            let next_tick = tick + calendar.days_to_next * config.day_length;
            let (_, next, _) = calendar_at(next_tick, &config, true);
            assert_eq!(SEASONS[calendar.next_season()], format!("{next:?}"));
            let (_, still, _) = calendar_at(next_tick - config.day_length, &config, true);
            assert_eq!(format!("{still:?}"), format!("{season:?}"));
            // The clock and the calendar name sunset the same way.
            let dusk = ClockModel::new(
                index,
                "",
                1000 - config.night_permille,
                config.night_permille,
                false,
                1,
            );
            assert_eq!(
                calendar.sunset,
                format!("{:02}:{:02}", dusk.hour, dusk.minute)
            );
            assert_eq!(dusk.phase, "Night");
        }
    }

    #[test]
    fn season_lines_say_only_what_the_world_exposes() {
        let regional = SeasonFacts {
            cold: true,
            regional: true,
            winter_growth: vec![("grain".into(), 1, 4)],
            seasonal_grain: false,
            hearth_logs: Some((3, 2)),
        };
        assert_eq!(
            season_lines(2, &regional),
            [
                "Growth: fields and woods regrow at their full rate.",
                "Cold: only at night."
            ]
        );
        assert_eq!(
            season_lines(3, &regional),
            [
                "Growth: grain regrows 4× slower; the rest as usual.",
                "Cold: all day and night; a coat or a lit hearth keeps a resident warm. A hearth \
                 burns 3 logs a day instead of 2."
            ]
        );
        // Other ratios, and a world whose winter changes no growth.
        let odd = SeasonFacts {
            winter_growth: vec![("berries".into(), 0, 4), ("reeds".into(), 3, 4)],
            hearth_logs: Some((2, 2)),
            ..regional.clone()
        };
        assert_eq!(
            season_lines(3, &odd)[0],
            "Growth: berries does not regrow, reeds regrows at 75% of its usual rate; the rest \
             as usual."
        );
        assert!(!season_lines(3, &odd)[1].contains("hearth burns"));
        let even = SeasonFacts {
            winter_growth: Vec::new(),
            ..regional.clone()
        };
        assert_eq!(
            season_lines(3, &even)[0],
            "Growth: fields and woods regrow as in any season."
        );
        // The older island: grain by season, cold the same way.
        let legacy = SeasonFacts {
            cold: true,
            seasonal_grain: true,
            ..SeasonFacts::default()
        };
        assert_eq!(
            season_lines(1, &legacy)[0],
            "Growth: grain fields refill completely each day."
        );
        assert_eq!(
            season_lines(3, &legacy)[0],
            "Growth: grain fields refill to a quarter each day."
        );
        // A world without needs or seasons has nothing to add.
        assert!(season_lines(3, &SeasonFacts::default()).is_empty());
        let (now, next) = calendar_text(&CalendarModel::new(19, 7, 300), &regional);
        assert_eq!(
            now,
            "Autumn · day 6 of 7\nSunrise 06:00 · sunset 22:48\nGrowth: fields and woods regrow \
             at their full rate.\nCold: only at night."
        );
        assert!(next.starts_with("Winter begins in 2 days\nGrowth: grain regrows 4× slower"));
    }

    fn input() -> TotalsInput {
        TotalsInput {
            residents: 42,
            housed: 42,
            beds: 84,
            households: 42,
            morale_percent: 97,
            social_events: 70,
            food: 114,
            water: 104,
            logs: 105,
            planks: 11,
            fund: 40,
            spendable: 0,
            held: 40,
            open_sites: 2,
            needed: [0, 13],
            trends: [
                Some(Trend::Rising(12)),
                Some(Trend::Steady),
                None,
                Some(Trend::Rising(7)),
            ],
            villages: vec![
                ("Harbour".into(), 60, 6),
                ("Alderwood".into(), 45, 5),
                ("Stone Ridge".into(), 0, 0),
            ],
            in_hand: Vec::new(),
        }
    }

    #[test]
    fn the_totals_show_real_numbers_marks_and_reasons() {
        let totals = totals(&input());
        let of = |kind| totals.iter().find(|total| total.kind == kind).unwrap();
        assert_eq!(
            totals.iter().map(|total| total.kind).collect::<Vec<_>>(),
            TotalKind::ALL
        );
        let residents = of(TotalKind::Residents);
        assert_eq!(
            (residents.value.as_str(), residents.unit.as_deref()),
            ("42", Some("/ 84"))
        );
        assert_eq!(residents.reading, "42 of 84 beds");
        assert_eq!(residents.mark, None);
        assert_eq!(of(TotalKind::Morale).value, "97%");
        // Food moved by more than a tenth: an arrow, and the fact in words.
        assert_eq!(of(TotalKind::Food).mark, Some(TotalMark::Up));
        assert!(
            of(TotalKind::Food)
                .facts
                .contains(&"Up 12 since yesterday.".to_owned())
        );
        assert_eq!(of(TotalKind::Water).mark, None, "steady shows no arrow");
        // Planks are below what open sites need: the triangle wins over the trend.
        let planks = of(TotalKind::Planks);
        assert_eq!(planks.mark, Some(TotalMark::Low));
        assert_eq!(planks.sentence, "Running low: open sites still need 13.");
        assert_eq!(planks.facts[0], "Harbour 6 · Alderwood 5 · Stone Ridge 0");
        assert_eq!(of(TotalKind::Logs).mark, None);
        assert_eq!(
            of(TotalKind::Logs).sentence,
            "No open site is waiting for them."
        );
        let fund = of(TotalKind::Fund);
        assert_eq!(
            (fund.value.as_str(), fund.unit.as_deref()),
            ("40", Some("fund")),
            "the chip reads the civic fund, not what is left to spend"
        );
        assert_eq!(fund.sentence, "0 coins to spend; 40 held for 2 open sites.");
        assert!(fund.facts[0].starts_with("The civic fund collects a levy"));
        // Every total says where a click leads.
        assert_eq!(fund.facts.last().unwrap(), "Click: open Projects");
        assert_eq!(planks.facts.last().unwrap(), "Click: open Economy");
        assert_eq!(TotalKind::Residents.page(), LedgerPage::People);
        assert_eq!(TotalKind::Morale.page(), LedgerPage::Village);
    }

    #[test]
    fn shortages_and_the_building_in_hand_mark_their_totals() {
        let short = TotalsInput {
            food: 30,
            water: 0,
            housed: 40,
            in_hand: vec!["plank".into(), "log".into()],
            ..input()
        };
        let totals = totals(&short);
        let of = |kind| totals.iter().find(|total| total.kind == kind).unwrap();
        assert_eq!(of(TotalKind::Food).mark, Some(TotalMark::Low));
        assert_eq!(of(TotalKind::Water).mark, Some(TotalMark::Low));
        assert_eq!(of(TotalKind::Residents).mark, Some(TotalMark::Low));
        assert_eq!(of(TotalKind::Residents).sentence, "2 without a home.");
        assert!(of(TotalKind::Logs).in_use && of(TotalKind::Planks).in_use);
        assert!(!of(TotalKind::Food).in_use);
        // An empty island reports no shortage.
        let empty = TotalsInput::default();
        assert!(
            super::totals(&empty)
                .iter()
                .all(|total| total.mark.is_none())
        );
    }

    #[test]
    fn the_standard_bar_keeps_five_totals_until_logs_or_planks_matter() {
        let calm = TotalsInput {
            needed: [0, 0],
            ..input()
        };
        let shown = |input: &TotalsInput, class| {
            totals(input)
                .iter()
                .filter(|total| total_shown(total, class))
                .map(|total| total.kind)
                .collect::<Vec<_>>()
        };
        assert_eq!(shown(&calm, LayoutClass::Wide).len(), 7);
        assert_eq!(
            shown(&calm, LayoutClass::Standard),
            [
                TotalKind::Residents,
                TotalKind::Morale,
                TotalKind::Food,
                TotalKind::Water,
                TotalKind::Fund
            ]
        );
        // Low planks, or a building that costs logs, bring them back.
        assert!(shown(&input(), LayoutClass::Standard).contains(&TotalKind::Planks));
        let holding = TotalsInput {
            in_hand: vec!["log".into()],
            ..calm.clone()
        };
        assert!(shown(&holding, LayoutClass::Standard).contains(&TotalKind::Logs));
    }

    #[test]
    fn the_right_column_holds_one_thing_and_is_closed_by_default() {
        let closed = ColumnState::default();
        assert_eq!(closed.column(), Column::Empty, "closed by default");
        let economy = closed.step(ColumnEvent::Page(LedgerPage::Economy));
        assert_eq!(economy.column(), Column::Ledger(LedgerPage::Economy));
        // Another page switches; the same page closes.
        let people = economy.step(ColumnEvent::Page(LedgerPage::People));
        assert_eq!(people.column(), Column::Ledger(LedgerPage::People));
        assert_eq!(
            people.step(ColumnEvent::Page(LedgerPage::People)).column(),
            Column::Empty
        );
        // A selection replaces the ledger with the inspector.
        let inspecting = people.step(ColumnEvent::Selected);
        assert_eq!(inspecting.column(), Column::Inspector);
        // Opening a page over the inspector hides it; closing empties the column.
        let over = inspecting.step(ColumnEvent::Page(LedgerPage::People));
        assert_eq!(over.column(), Column::Ledger(LedgerPage::People));
        assert!(over.selected, "the page can still manage the selection");
        let done = over.step(ColumnEvent::Close);
        assert_eq!(done.column(), Column::Empty);
        assert!(
            !done.selected,
            "Esc closes the column and clears the selection"
        );
        // F11 reopens the last page and closes it again.
        let again = done.step(ColumnEvent::Last);
        assert_eq!(again.column(), Column::Ledger(LedgerPage::People));
        assert_eq!(again.step(ColumnEvent::Last).column(), Column::Empty);
        assert_eq!(
            ColumnState::default().step(ColumnEvent::Last).column(),
            Column::Ledger(LedgerPage::Village)
        );
        assert_eq!(
            inspecting.step(ColumnEvent::Deselected).column(),
            Column::Empty
        );
        // Pages, keys and widths as the spec lists them.
        let keys: Vec<&str> = LedgerPage::ALL.iter().map(|page| page.key_cap()).collect();
        assert_eq!(keys, ["F1", "F2", "F3", "F4", "F5"]);
        assert_eq!(column_width(over.column(), LayoutClass::Wide), 440.0);
        assert_eq!(column_width(over.column(), LayoutClass::Standard), 400.0);
        assert_eq!(column_width(Column::Inspector, LayoutClass::Wide), 344.0);
        assert_eq!(
            column_width(Column::Inspector, LayoutClass::Standard),
            320.0
        );
        assert_eq!(column_width(Column::Empty, LayoutClass::Wide), 0.0);
    }

    #[test]
    fn the_docks_fit_both_window_sizes_without_touching() {
        for (width, class, tile) in [
            (1280.0, LayoutClass::Wide, 64.0),
            (1024.0, LayoutClass::Standard, 56.0),
        ] {
            for build_tiles in [4, 5] {
                let docks = dock_layout(width, class, build_tiles);
                assert_eq!(docks.tile, tile);
                let left_end = 8.0 + docks.left;
                let build_end = docks.build.0 + docks.build.1;
                assert!(docks.build.0 >= left_end + 8.0, "{width}: left and build");
                assert!(
                    docks.ledger.0 >= build_end + 8.0,
                    "{width}: build and ledger"
                );
                assert!(
                    (docks.ledger.0 + docks.ledger.1 - (width - 8.0)).abs() < 0.01,
                    "the ledger dock ends at the margin"
                );
            }
        }
        // Compact (140% in a 1280x720 window is 914 wide): five build tiles
        // and the left dock leave room for one ledger tile, not five.
        let compact = dock_layout(914.0, LayoutClass::Compact, 5);
        assert_eq!(ledger_tiles(LayoutClass::Compact), 1);
        assert!(!ledger_tab_icons(LayoutClass::Compact));
        assert!(ledger_tab_icons(LayoutClass::Standard) && ledger_tab_icons(LayoutClass::Wide));
        assert_eq!(ledger_tiles(LayoutClass::Standard), 5);
        assert!(compact.build.0 >= 8.0 + compact.left + 8.0);
        assert!(compact.ledger.0 >= compact.build.0 + compact.build.1 + 8.0);
        // The spec's rectangles at 1280: left 144, ledger 348 at x 924.
        let wide = dock_layout(1280.0, LayoutClass::Wide, 5);
        assert_eq!(wide.left, 144.0);
        assert_eq!(wide.ledger, (924.0, 348.0));
    }
}
