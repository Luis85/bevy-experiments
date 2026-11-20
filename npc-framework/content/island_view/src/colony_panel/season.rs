//! The season-end card (design-ui 2.6): the season that just ended beside
//! the one before it, from the simulation's season summaries. Pure words;
//! the ledger shows the card in the right column, never over anything and
//! never pausing time.
use super::Action;
use super::model::{Entry, Extra};
use island_content::village::view::SeasonSummary;

/// The card's words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SeasonCard {
    /// "Autumn has ended".
    pub(crate) title: String,
    /// "Year 1 · the island after 24 days".
    pub(crate) subtitle: String,
    /// The two seasons' names, the earlier one `None` for the first season.
    pub(crate) heads: (String, Option<String>),
    /// Label, the ended season, the one before.
    pub(crate) rows: Vec<(String, String, Option<String>)>,
    /// "Most made: ...", "Went short: ...", "Dearer: ...", "Deaths: ...".
    pub(crate) lines: Vec<String>,
}

fn sum<T>(items: &[(T, u32)]) -> u32 {
    items.iter().map(|(_, count)| *count).sum()
}

fn made(season: &SeasonSummary) -> u64 {
    season.goods.iter().map(|good| good.produced).sum()
}

fn spoiled(season: &SeasonSummary) -> u64 {
    season.goods.iter().map(|good| good.spoiled).sum()
}

/// The card for `ended`, beside `before` when the island has one. `days`
/// is the island's age in days at the season's end; `good` words a good's
/// key for `n` of it ("bread", "planks").
pub(crate) fn season_card(
    ended: &SeasonSummary,
    before: Option<&SeasonSummary>,
    days: u64,
    good: &dyn Fn(&str, u64) -> String,
) -> SeasonCard {
    let subtitle = match before {
        None => format!("Year {} \u{b7} the first season", ended.year.max(1)),
        Some(_) => format!(
            "Year {} \u{b7} the island after {days} day{}",
            ended.year.max(1),
            if days == 1 { "" } else { "s" }
        ),
    };
    type Figure = fn(&SeasonSummary) -> String;
    let mut figures: Vec<(&str, Figure)> = vec![
        // Grown residents, and children apart: a birth adds a child, so the
        // two together add up (final pass: "42 to 42" beside a birth).
        ("Residents", |season| {
            format!("{} to {}", season.population_start, season.population_end)
        }),
        ("Children", |season| {
            format!("{} to {}", season.children_start, season.children_end)
        }),
        ("Births", |season| season.births.to_string()),
        ("Deaths", |season| sum(&season.deaths).to_string()),
        ("Newcomers", |season| season.arrivals.to_string()),
    ];
    if ended.came_of_age > 0 || before.is_some_and(|before| before.came_of_age > 0) {
        figures.push(("Came of age", |season| season.came_of_age.to_string()));
    }
    figures.extend::<[(&str, Figure); 2]>([
        ("Buildings finished", |season| {
            sum(&season.built).to_string()
        }),
        ("Goods made", |season| made(season).to_string()),
    ]);
    if spoiled(ended) > 0 || before.is_some_and(|before| spoiled(before) > 0) {
        figures.push(("Goods spoiled", |season| spoiled(season).to_string()));
    }
    figures.push(("Boat visits", |season| season.boat_visits.to_string()));
    figures.push(("Fund", fund_words));
    let rows = figures
        .into_iter()
        .map(|(label, figure)| (label.to_owned(), figure(ended), before.map(figure)))
        .collect();
    let mut lines = Vec::new();
    let mut most: Vec<_> = ended.goods.iter().filter(|row| row.produced > 0).collect();
    most.sort_by(|a, b| b.produced.cmp(&a.produced).then(a.good.cmp(&b.good)));
    lines.push(if most.is_empty() {
        "Nothing was made.".to_owned()
    } else {
        format!(
            "Most made: {}.",
            most.iter()
                .take(3)
                .map(|row| format!("{} {}", good(&row.good, row.produced), row.produced))
                .collect::<Vec<_>>()
                .join(", ")
        )
    });
    lines.push(match &ended.worst_shortage {
        Some((short, days)) => format!(
            "Went short: {}, for {days} day{}.",
            good(short, 2),
            if *days == 1 { "" } else { "s" }
        ),
        None => "Nothing ran out.".to_owned(),
    });
    let moves: Vec<(&str, i64, u32, u32)> = ended
        .goods
        .iter()
        .filter_map(|row| {
            let (from, to) = (row.price_start?, row.price_end?);
            Some((row.good.as_str(), i64::from(to) - i64::from(from), from, to))
        })
        .collect();
    let dearer = moves
        .iter()
        .filter(|(_, change, ..)| *change > 0)
        .max_by_key(|(name, change, ..)| (*change, std::cmp::Reverse(*name)));
    let cheaper = moves
        .iter()
        .filter(|(_, change, ..)| *change < 0)
        .min_by_key(|(name, change, ..)| (*change, *name));
    let half = |word: &str, (name, _, from, to): &(&str, i64, u32, u32)| {
        format!("{word}: {}, {from} to {to} coins.", good(name, 2))
    };
    match (dearer, cheaper) {
        (Some(up), Some(down)) => {
            lines.push(format!("{} {}", half("Dearer", up), half("Cheaper", down)))
        }
        (Some(up), None) => lines.push(half("Dearer", up)),
        (None, Some(down)) => lines.push(half("Cheaper", down)),
        (None, None) => {}
    }
    let deaths: Vec<String> = ended
        .deaths
        .iter()
        .filter(|(_, count)| *count > 0)
        .map(|(cause, count)| format!("{count} of {}", cause.replace('_', " ")))
        .collect();
    if !deaths.is_empty() {
        lines.push(format!("Deaths: {}.", deaths.join(", ")));
    }
    SeasonCard {
        title: format!("{} has ended", ended.name),
        subtitle,
        heads: (ended.name.clone(), before.map(|before| before.name.clone())),
        rows,
        lines,
    }
}

/// "52 to 39 coins"; the first season of a new island, whose fund truly
/// starts at none: "64 coins (the island started with none)".
pub(crate) fn fund_words(season: &SeasonSummary) -> String {
    if season.season == 0 && season.fund_start == 0 {
        format!("{} coins (the island started with none)", season.fund_end)
    } else {
        format!("{} to {} coins", season.fund_start, season.fund_end)
    }
}

/// The card as ledger rows: title, subtitle, the table, the lines and the
/// two buttons.
pub(crate) fn season_entries(card: &SeasonCard) -> Vec<Entry> {
    let mut rows = vec![
        Entry::text("season_title", &card.title, true),
        Entry::caption("season_subtitle", &card.subtitle),
    ];
    let head = |first: &str, second: Option<&str>| {
        let mut columns = vec![String::new(), first.to_owned()];
        columns.extend(second.map(str::to_owned));
        columns
    };
    rows.push(Entry::table(
        "season_head",
        head(&card.heads.0, card.heads.1.as_deref()),
        None,
        true,
    ));
    // Counts stand in the table; "39 to 43" figures are too wide for its
    // value columns and stand on their own lines under it.
    let mut spans = Vec::new();
    for (label, ended, before) in &card.rows {
        if ended.contains(" to ") || label == "Fund" {
            spans.push(span_line(label, ended, before.as_deref(), &card.heads));
            continue;
        }
        let mut columns = vec![label.clone(), ended.clone()];
        columns.extend(before.clone());
        rows.push(Entry::table(
            format!("season_row_{}", label.to_lowercase().replace(' ', "_")),
            columns,
            None,
            false,
        ));
    }
    for (index, line) in spans.into_iter().enumerate() {
        rows.push(Entry::text(&format!("season_span{index}"), line, false).with(Extra::Strong));
    }
    for (index, line) in card.lines.iter().enumerate() {
        rows.push(Entry::text(&format!("season_line{index}"), line, false).with(Extra::Strong));
    }
    rows.push(Entry::choices(
        "season_buttons",
        vec![
            ("Open the journal".to_owned(), Action::SeasonJournal),
            ("Close".to_owned(), Action::SeasonClose),
        ],
    ));
    rows
}

/// "Residents: 39 to 43 (Summer: 36 to 39)." / "Fund: 52 to 39 coins."
pub(crate) fn span_line(
    label: &str,
    ended: &str,
    before: Option<&str>,
    heads: &(String, Option<String>),
) -> String {
    match (before, &heads.1) {
        (Some(before), Some(name)) => format!("{label}: {ended} ({name}: {before})."),
        _ => format!("{label}: {ended}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use island_content::village::view::GoodSeason;

    fn good_season(good: &str, produced: u64, prices: (u32, u32)) -> GoodSeason {
        GoodSeason {
            good: good.into(),
            produced,
            price_start: Some(prices.0),
            price_end: Some(prices.1),
            ..Default::default()
        }
    }

    fn autumn() -> SeasonSummary {
        SeasonSummary {
            season: 2,
            year: 1,
            name: "Autumn".into(),
            population_start: 39,
            population_end: 43,
            births: 3,
            deaths: vec![("hunger".into(), 1), ("old_age".into(), 1)],
            arrivals: 1,
            built: vec![("cottage".into(), 2)],
            goods: vec![
                good_season("bread", 96, (3, 4)),
                good_season("plank", 71, (7, 6)),
                good_season("flour", 64, (2, 2)),
                good_season("fish", 3, (1, 1)),
            ],
            fund_start: 52,
            fund_end: 39,
            boat_visits: 2,
            worst_shortage: Some(("firewood".into(), 3)),
            ..Default::default()
        }
    }

    fn word(good: &str, count: u64) -> String {
        crate::game_ui::words::noun(good, count)
    }

    #[test]
    fn the_first_season_has_no_before_column() {
        let card = season_card(&autumn(), None, 24, &word);
        assert_eq!(card.title, "Autumn has ended");
        assert_eq!(card.subtitle, "Year 1 \u{b7} the first season");
        assert_eq!(card.heads, ("Autumn".to_owned(), None));
        assert!(card.rows.iter().all(|(_, _, before)| before.is_none()));
        assert!(
            !card.rows.iter().any(|(label, ..)| label == "Goods spoiled"),
            "no spoiled row while both seasons are 0"
        );
        assert_eq!(
            card.rows[0],
            ("Residents".to_owned(), "39 to 43".to_owned(), None)
        );
        assert_eq!(
            card.lines,
            vec![
                "Most made: bread 96, planks 71, flour 64.".to_owned(),
                "Went short: firewood, for 3 days.".to_owned(),
                "Dearer: bread, 3 to 4 coins. Cheaper: planks, 7 to 6 coins.".to_owned(),
                "Deaths: 1 of hunger, 1 of old age.".to_owned(),
            ]
        );
        let entries = season_entries(&card);
        assert_eq!(
            entries
                .iter()
                .find(|entry| entry.key == "season_head")
                .and_then(|entry| entry.columns.clone())
                .map(|columns| columns.len()),
            Some(2),
            "one season column"
        );
    }

    #[test]
    fn residents_and_children_add_up_and_the_first_fund_reads_as_a_start() {
        let mut first = autumn();
        first.season = 0;
        first.population_start = 42;
        first.population_end = 42;
        first.children_start = 3;
        first.children_end = 4;
        first.births = 1;
        first.deaths = Vec::new();
        first.arrivals = 0;
        first.fund_start = 0;
        first.fund_end = 64;
        let card = season_card(&first, None, 7, &word);
        let spans: Vec<String> = season_entries(&card)
            .into_iter()
            .filter(|entry| entry.key.starts_with("season_span"))
            .map(|entry| entry.text)
            .collect();
        assert_eq!(
            spans,
            vec![
                "Residents: 42 to 42.".to_owned(),
                "Children: 3 to 4.".to_owned(),
                "Fund: 64 coins (the island started with none).".to_owned(),
            ]
        );
        assert!(
            !card.rows.iter().any(|(label, ..)| label == "Came of age"),
            "no row while nobody came of age"
        );
        // A child who came of age is a resident now: the row says so.
        let mut later = first.clone();
        later.season = 1;
        later.came_of_age = 1;
        later.population_end = 43;
        later.children_end = 3;
        later.fund_start = 64;
        later.fund_end = 80;
        let card = season_card(&later, Some(&first), 14, &word);
        assert!(
            card.rows
                .iter()
                .any(|(label, ended, before)| label == "Came of age"
                    && ended == "1"
                    && before.as_deref() == Some("0"))
        );
        assert_eq!(fund_words(&later), "64 to 80 coins");
    }

    #[test]
    fn a_second_season_stands_beside_the_first() {
        let mut summer = autumn();
        summer.name = "Summer".into();
        summer.population_start = 36;
        summer.population_end = 39;
        summer.goods[0].spoiled = 9;
        let mut ended = autumn();
        ended.goods = Vec::new();
        ended.worst_shortage = None;
        ended.deaths = Vec::new();
        let card = season_card(&ended, Some(&summer), 24, &word);
        assert_eq!(card.subtitle, "Year 1 \u{b7} the island after 24 days");
        assert_eq!(
            card.rows[0],
            (
                "Residents".to_owned(),
                "39 to 43".to_owned(),
                Some("36 to 39".to_owned())
            )
        );
        assert!(card.rows.iter().any(|(label, ..)| label == "Goods spoiled"));
        assert_eq!(
            card.lines,
            vec![
                "Nothing was made.".to_owned(),
                "Nothing ran out.".to_owned()
            ]
        );
    }
}
