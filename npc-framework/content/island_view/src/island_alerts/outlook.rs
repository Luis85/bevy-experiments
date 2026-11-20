//! "Village outlook" (design-ui 2.5): what needs the player next, one line
//! per topic, the worst village of each, at most three. Pure lines from the
//! simulation's `outlook` view; the card is filled in place.
use crate::game_ui::tokens::Status;
use island_content::village::view::Outlook;
use npc_framework::prelude::SimId;

/// The topics in their order of precedence at equal severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Topic {
    Food,
    Water,
    Fuel,
    Homes,
    Work,
    Cold,
    /// Households waiting for a home (a bed too few): a warning below
    /// food, water and cold.
    Housing,
    Shortage,
}

/// What a click on a line opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Opens {
    /// Economy › Goods of the village, with this good's card when named.
    Goods(SimId, Option<String>),
    /// The build tray on Homes.
    Homes,
    /// Economy › Jobs of the village.
    Jobs(SimId),
    /// Economy › Priorities of the village.
    Priorities(SimId),
}

/// One line of the card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OutlookLine {
    pub(crate) topic: Topic,
    pub(crate) status: Status,
    pub(crate) text: String,
    /// The tooltip's body: what the click does, or why.
    pub(crate) tip: String,
    /// `None`: a line that only informs (all is well).
    pub(crate) opens: Option<Opens>,
}

/// One village as the card reads it.
pub(crate) struct VillageOutlook<'a> {
    pub(crate) id: SimId,
    pub(crate) name: &'a str,
    pub(crate) outlook: &'a Outlook,
    /// The simulation's housing sentence ("2 households are waiting for a
    /// home"), `None` when no household waits.
    pub(crate) housing: Option<&'a str>,
}

fn rank(status: Status) -> u8 {
    match status {
        Status::Crit => 0,
        Status::Warn => 1,
        Status::Info => 2,
        Status::Good | Status::Idle => 3,
    }
}

/// Days of supply below which a line is trouble. A village holding its
/// planned stock (four to six days a head) is normal and says nothing: a
/// line only below two days, a warning at one, critical at none (final
/// pass: the outlook read as a permanent emergency).
pub(crate) const TROUBLE_DAYS: u32 = 2;

fn days_status(days: u32) -> Status {
    if days == 0 {
        Status::Crit
    } else {
        Status::Warn
    }
}

/// How long a village's stock of a good lasts, as one sentence right for
/// any good, a mass noun or a plural: "Harbour has no logs.", "Harbour runs
/// out of logs tomorrow.", "Harbour has food for 2 days." (closing phase,
/// defect 6).
pub(crate) fn stock_line(village: &str, good: &str, days: u32) -> String {
    match days {
        0 => format!("{village} has no {good}."),
        1 => format!("{village} runs out of {good} tomorrow."),
        days => format!("{village} has {good} for {days} days."),
    }
}

/// A coming shortage: "Harbour runs out of logs in 2 days."
pub(crate) fn runs_out(village: &str, good: &str, days: u32) -> String {
    match days {
        0 | 1 => stock_line(village, good, days),
        days => format!("{village} runs out of {good} in {days} days."),
    }
}

/// "1 has no coat", "8 have no coat".
pub(crate) fn without_coat(count: u32) -> String {
    if count == 1 {
        "1 has no coat".to_owned()
    } else {
        format!("{count} have no coat")
    }
}

fn with_prefix(prefix: Option<&str>, body: &str) -> String {
    match prefix {
        Some(name) => format!("{name}: {body}"),
        None => crate::game_ui::words::capital(body),
    }
}

fn residents(count: u32) -> String {
    if count == 1 {
        "1 resident has".to_owned()
    } else {
        format!("{count} residents have")
    }
}

/// Every candidate line of one village, one per topic.
fn candidates(
    village: &VillageOutlook,
    prefix: Option<&str>,
    fuel: &str,
    good_name: &dyn Fn(&str) -> String,
    topic_of: &dyn Fn(&str) -> Option<Topic>,
) -> Vec<(OutlookLine, u32)> {
    let outlook = village.outlook;
    let id = village.id;
    let opens_goods = |good: Option<String>| Some(Opens::Goods(id, good));
    let goods_tip = format!("Opens Goods for {}.", village.name);
    let mut lines = Vec::new();
    // The second value orders the villages of one topic: lower is worse.
    for (topic, days, what) in [
        (Topic::Food, outlook.food_days, "food"),
        (Topic::Water, outlook.water_days, "water"),
    ] {
        if let Some(days) = days.filter(|days| *days < TROUBLE_DAYS) {
            lines.push((
                OutlookLine {
                    topic,
                    status: days_status(days),
                    text: stock_line(village.name, what, days),
                    tip: goods_tip.clone(),
                    opens: opens_goods(None),
                },
                days,
            ));
        }
    }
    if let Some(days) = outlook.fuel_days.filter(|days| *days < TROUBLE_DAYS) {
        let status = if days == 0 || outlook.cold_now {
            Status::Crit
        } else {
            Status::Warn
        };
        lines.push((
            OutlookLine {
                topic: Topic::Fuel,
                status,
                text: stock_line(village.name, &good_name(fuel), days),
                tip: goods_tip.clone(),
                opens: opens_goods(Some(fuel.to_owned())),
            },
            days,
        ));
    }
    if outlook.homeless > 0 {
        lines.push((
            OutlookLine {
                topic: Topic::Homes,
                status: if outlook.cold_now {
                    Status::Crit
                } else {
                    Status::Warn
                },
                text: with_prefix(prefix, &format!("{} no home.", residents(outlook.homeless))),
                tip: "Opens the build tray on Homes.".to_owned(),
                opens: Some(Opens::Homes),
            },
            u32::MAX - outlook.homeless,
        ));
    }
    // One homes line per village: residents with no home at all come
    // first; only without them is a household waiting for a home said.
    if let Some(housing) = village.housing.filter(|_| outlook.homeless == 0) {
        let waiting = housing
            .split_whitespace()
            .next()
            .and_then(|count| count.parse::<u32>().ok())
            .unwrap_or(1);
        lines.push((
            OutlookLine {
                topic: Topic::Housing,
                status: Status::Warn,
                text: with_prefix(prefix, &format!("{}.", housing.trim_end_matches('.'))),
                tip: "Opens the build tray on Homes.".to_owned(),
                opens: Some(Opens::Homes),
            },
            u32::MAX - waiting,
        ));
    }
    if outlook.idle_hands > 0 {
        let no_place = outlook.open_places == 0;
        lines.push((
            OutlookLine {
                topic: Topic::Work,
                status: if no_place { Status::Warn } else { Status::Info },
                text: with_prefix(
                    prefix,
                    &format!("{} no work.", residents(outlook.idle_hands)),
                ),
                tip: if no_place {
                    format!("No job in {} has an open place.", village.name)
                } else {
                    format!("Opens Jobs for {}.", village.name)
                },
                opens: Some(Opens::Jobs(id)),
            },
            u32::MAX - outlook.idle_hands,
        ));
    }
    if outlook.without_coat > 0 && (outlook.cold_in_days.is_some() || outlook.cold_now) {
        let coats = outlook.without_coat;
        let missing = without_coat(coats);
        let (status, body) = match outlook.cold_in_days {
            _ if outlook.cold_now => (Status::Crit, format!("it is cold, {missing}.")),
            Some(0) => (Status::Crit, format!("it is cold, {missing}.")),
            Some(days) => (
                if days <= 7 {
                    Status::Warn
                } else {
                    Status::Info
                },
                format!("winter in {days} days, {missing}."),
            ),
            None => (Status::Info, format!("{missing}.")),
        };
        // The village's name stays when there are several.
        let text = with_prefix(prefix, &body);
        lines.push((
            OutlookLine {
                topic: Topic::Cold,
                status,
                text,
                tip: format!("Opens Priorities for {}.", village.name),
                opens: Some(Opens::Priorities(id)),
            },
            u32::MAX - coats,
        ));
    }
    if let Some(short) = outlook
        .next_shortage
        .as_ref()
        .filter(|short| short.days_left < TROUBLE_DAYS)
        .filter(|short| {
            // A good already on a Food, Water or Fuel line is not said twice.
            topic_of(&short.good)
                .is_none_or(|topic| !lines.iter().any(|(line, _)| line.topic == topic))
        })
    {
        let good = good_name(&short.good);
        let text = runs_out(village.name, &good, short.days_left);
        lines.push((
            OutlookLine {
                topic: Topic::Shortage,
                status: Status::Warn,
                text,
                tip: crate::game_ui::words::sentence(&short.cause_sentence),
                opens: opens_goods(Some(short.good.clone())),
            },
            short.days_left,
        ));
    }
    lines
}

/// The card's lines: per topic the worst village, ordered by severity and
/// then topic, the first three; with nothing wrong, one line that says so.
/// `good_name` words a good's key; `topic_of` says whether a good is food,
/// water or fuel (so a shortage of it is not said twice).
pub(crate) fn lines(
    villages: &[VillageOutlook],
    fuel: &str,
    good_name: &dyn Fn(&str) -> String,
    topic_of: &dyn Fn(&str) -> Option<Topic>,
) -> Vec<OutlookLine> {
    let one = villages.len() == 1;
    let mut best: Vec<(OutlookLine, u32)> = Vec::new();
    for village in villages {
        let prefix = (!one).then_some(village.name);
        for (line, worse) in candidates(village, prefix, fuel, good_name, topic_of) {
            match best.iter_mut().find(|(held, _)| held.topic == line.topic) {
                Some(held) => {
                    if (rank(line.status), worse) < (rank(held.0.status), held.1) {
                        *held = (line, worse);
                    }
                }
                None => best.push((line, worse)),
            }
        }
    }
    let mut lines: Vec<OutlookLine> = best.into_iter().map(|(line, _)| line).collect();
    lines.sort_by_key(|line| (rank(line.status), line.topic));
    lines.truncate(3);
    if lines.is_empty() {
        let food = villages
            .iter()
            .filter_map(|village| village.outlook.food_days)
            .min();
        lines.push(OutlookLine {
            topic: Topic::Food,
            status: Status::Good,
            text: match food {
                Some(days) => format!(
                    "All is well: food for {days} day{} and a bed for everyone.",
                    if days == 1 { "" } else { "s" }
                ),
                None => "All is well.".to_owned(),
            },
            tip: "Nothing needs you now.".to_owned(),
            opens: None,
        });
    }
    lines
}

/// How many lines the card shows in a window `height` high: two in a
/// window lower than 640 px, else three.
pub(crate) fn shown(height: f32) -> usize {
    if height < 640.0 { 2 } else { 3 }
}

/// The lines that need the player, for the folded header's count: the
/// critical and warning ones.
pub(crate) fn attention(lines: &[OutlookLine]) -> usize {
    lines
        .iter()
        .filter(|line| matches!(line.status, Status::Crit | Status::Warn))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use island_content::village::Season;
    use island_content::village::view::Shortage;

    fn outlook() -> Outlook {
        Outlook {
            day: 20,
            season: Season::Autumn,
            population: 12,
            food_days: Some(20),
            water_days: Some(20),
            fuel_days: Some(20),
            beds_free: 2,
            homeless: 0,
            idle_hands: 0,
            open_places: 3,
            cold_in_days: None,
            cold_now: false,
            without_coat: 0,
            next_shortage: None,
        }
    }

    fn words(villages: &[VillageOutlook]) -> Vec<OutlookLine> {
        lines(
            villages,
            "village_firewood",
            &|good| good.replace("village_", ""),
            &|good| (good == "village_bread").then_some(Topic::Food),
        )
    }

    fn texts(lines: &[OutlookLine]) -> Vec<(Status, String)> {
        lines
            .iter()
            .map(|line| (line.status, line.text.clone()))
            .collect()
    }

    #[test]
    fn the_outlook_shows_the_three_worst_lines_in_order() {
        let mut hilltop = outlook();
        hilltop.food_days = Some(1);
        hilltop.idle_hands = 1;
        let mut harbour = outlook();
        harbour.food_days = Some(5);
        harbour.homeless = 2;
        harbour.water_days = Some(0);
        harbour.cold_in_days = Some(9);
        harbour.without_coat = 11;
        let villages = [
            VillageOutlook {
                id: SimId(1),
                name: "Hilltop",
                outlook: &hilltop,
                housing: None,
            },
            VillageOutlook {
                id: SimId(2),
                name: "Harbour",
                outlook: &harbour,
                housing: None,
            },
        ];
        assert_eq!(
            texts(&words(&villages)),
            vec![
                (Status::Crit, "Harbour has no water.".to_owned()),
                (
                    Status::Warn,
                    "Hilltop runs out of food tomorrow.".to_owned()
                ),
                (
                    Status::Warn,
                    "Harbour: 2 residents have no home.".to_owned()
                ),
            ],
            "the worst village of a topic, severity first, at most three"
        );
        // One village: no name in front.
        let single = [VillageOutlook {
            id: SimId(1),
            name: "Hilltop",
            outlook: &hilltop,
            housing: None,
        }];
        assert_eq!(
            texts(&words(&single)),
            vec![
                (
                    Status::Warn,
                    "Hilltop runs out of food tomorrow.".to_owned()
                ),
                (Status::Info, "1 resident has no work.".to_owned()),
            ]
        );
        // A coming winter and a coming shortage.
        let mut cold = outlook();
        cold.cold_in_days = Some(9);
        cold.without_coat = 11;
        cold.next_shortage = Some(Shortage {
            good: "village_plank".into(),
            days_left: 1,
            cause_sentence: "nobody saws logs".into(),
        });
        let lines = words(&[VillageOutlook {
            id: SimId(1),
            name: "Hilltop",
            outlook: &cold,
            housing: None,
        }]);
        assert_eq!(
            texts(&lines),
            vec![
                (
                    Status::Warn,
                    "Hilltop runs out of plank tomorrow.".to_owned()
                ),
                (
                    Status::Info,
                    "Winter in 9 days, 11 have no coat.".to_owned()
                ),
            ]
        );
        assert_eq!(lines[0].tip, "Nobody saws logs.");
    }

    #[test]
    fn a_low_window_shows_two_lines_and_the_fold_counts_the_urgent_ones() {
        assert_eq!(shown(600.0), 2);
        assert_eq!(shown(720.0), 3);
        let mut hilltop = outlook();
        hilltop.food_days = Some(1);
        hilltop.idle_hands = 1;
        hilltop.homeless = 1;
        let lines = words(&[VillageOutlook {
            id: SimId(1),
            name: "Hilltop",
            outlook: &hilltop,
            housing: None,
        }]);
        assert_eq!(attention(&lines), 2, "food and homes; work only informs");
        assert_eq!(super::super::fold_title(2), "Alerts \u{b7} Outlook");
        assert_eq!(super::super::fold_title(0), "Alerts");
    }

    #[test]
    fn shortage_and_coat_sentences_are_right_for_any_good_and_count() {
        assert_eq!(
            stock_line("Harbour", "logs", 1),
            "Harbour runs out of logs tomorrow."
        );
        assert_eq!(stock_line("Harbour", "logs", 0), "Harbour has no logs.");
        assert_eq!(
            stock_line("Harbour", "food", 4),
            "Harbour has food for 4 days."
        );
        assert_eq!(
            runs_out("Harbour", "logs", 2),
            "Harbour runs out of logs in 2 days."
        );
        assert_eq!(
            runs_out("Mill", "flour", 1),
            "Mill runs out of flour tomorrow."
        );
        assert_eq!(without_coat(1), "1 has no coat");
        assert_eq!(without_coat(5), "5 have no coat");
        // With several villages the cold line keeps the village's name.
        let mut cold = outlook();
        cold.cold_now = true;
        cold.without_coat = 1;
        let calm = outlook();
        let lines = words(&[
            VillageOutlook {
                id: SimId(1),
                name: "Harbour",
                outlook: &cold,
                housing: None,
            },
            VillageOutlook {
                id: SimId(2),
                name: "Mill",
                outlook: &calm,
                housing: None,
            },
        ]);
        assert_eq!(lines[0].text, "Harbour: it is cold, 1 has no coat.");
    }

    #[test]
    fn households_waiting_for_a_home_are_a_warning_below_food_water_and_cold() {
        let mut harbour = outlook();
        harbour.food_days = Some(1);
        harbour.cold_now = true;
        harbour.without_coat = 2;
        let calm = outlook();
        let villages = [
            VillageOutlook {
                id: SimId(1),
                name: "Harbour",
                outlook: &harbour,
                housing: Some("1 household is waiting for a home"),
            },
            VillageOutlook {
                id: SimId(2),
                name: "Mill",
                outlook: &calm,
                housing: Some("3 households are waiting for a home"),
            },
        ];
        let lines = words(&villages);
        assert_eq!(
            texts(&lines),
            vec![
                (
                    Status::Crit,
                    "Harbour: it is cold, 2 have no coat.".to_owned()
                ),
                (
                    Status::Warn,
                    "Harbour runs out of food tomorrow.".to_owned()
                ),
                (
                    Status::Warn,
                    "Mill: 3 households are waiting for a home.".to_owned()
                ),
            ],
            "the worst village's line, below food and cold, the name kept"
        );
        assert_eq!(lines[2].opens, Some(Opens::Homes));
        // One village: no name, and the singular is the simulation's own.
        let lines = words(&[VillageOutlook {
            id: SimId(1),
            name: "Mill",
            outlook: &calm,
            housing: Some("1 household is waiting for a home"),
        }]);
        assert_eq!(
            texts(&lines),
            vec![(
                Status::Warn,
                "1 household is waiting for a home.".to_owned()
            )]
        );
        // Residents with no home at all: one homes line, theirs.
        let mut homeless = outlook();
        homeless.homeless = 2;
        let lines = words(&[VillageOutlook {
            id: SimId(1),
            name: "Mill",
            outlook: &homeless,
            housing: Some("1 household is waiting for a home"),
        }]);
        assert_eq!(
            texts(&lines),
            vec![(Status::Warn, "2 residents have no home.".to_owned())]
        );
    }

    #[test]
    fn a_village_holding_its_planned_stock_is_not_in_trouble() {
        // Four days of food, three of water and fuel, a shortage two days
        // off: the plan's normal; nothing to say.
        let mut planned = outlook();
        planned.food_days = Some(4);
        planned.water_days = Some(3);
        planned.fuel_days = Some(2);
        planned.next_shortage = Some(Shortage {
            good: "village_plank".into(),
            days_left: 2,
            cause_sentence: "nobody saws logs".into(),
        });
        let lines = words(&[VillageOutlook {
            id: SimId(1),
            name: "Hilltop",
            outlook: &planned,
            housing: None,
        }]);
        assert_eq!(
            texts(&lines),
            vec![(
                Status::Good,
                "All is well: food for 4 days and a bed for everyone.".to_owned()
            )]
        );
        // One day left is a warning, none critical; in the cold, fuel for
        // one day is critical already.
        let mut low = outlook();
        low.food_days = Some(1);
        low.water_days = Some(0);
        low.fuel_days = Some(1);
        low.cold_now = true;
        let lines = words(&[VillageOutlook {
            id: SimId(1),
            name: "Hilltop",
            outlook: &low,
            housing: None,
        }]);
        assert_eq!(
            texts(&lines),
            vec![
                (Status::Crit, "Hilltop has no water.".to_owned()),
                (
                    Status::Crit,
                    "Hilltop runs out of firewood tomorrow.".to_owned()
                ),
                (
                    Status::Warn,
                    "Hilltop runs out of food tomorrow.".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn a_village_with_nothing_wrong_says_all_is_well() {
        let calm = outlook();
        let lines = words(&[VillageOutlook {
            id: SimId(1),
            name: "Hilltop",
            outlook: &calm,
            housing: None,
        }]);
        assert_eq!(
            texts(&lines),
            vec![(
                Status::Good,
                "All is well: food for 20 days and a bed for everyone.".to_owned()
            )]
        );
        assert_eq!(lines[0].opens, None);
        let mut fasting = outlook();
        fasting.food_days = None;
        let lines = words(&[VillageOutlook {
            id: SimId(1),
            name: "Hilltop",
            outlook: &fasting,
            housing: None,
        }]);
        assert_eq!(lines[0].text, "All is well.");
    }

    #[test]
    fn an_outlook_line_opens_the_page_that_answers_it() {
        let mut all = outlook();
        all.food_days = Some(1);
        all.fuel_days = Some(1);
        all.cold_now = true;
        all.without_coat = 3;
        all.homeless = 1;
        all.idle_hands = 2;
        all.open_places = 0;
        let village = VillageOutlook {
            id: SimId(4),
            name: "Mill",
            outlook: &all,
            housing: None,
        };
        let opens: Vec<(Topic, Option<Opens>)> = candidates(
            &village,
            None,
            "village_firewood",
            &|good| good.into(),
            &|_| None,
        )
        .into_iter()
        .map(|(line, _)| (line.topic, line.opens))
        .collect();
        assert_eq!(
            opens,
            vec![
                (Topic::Food, Some(Opens::Goods(SimId(4), None))),
                (
                    Topic::Fuel,
                    Some(Opens::Goods(SimId(4), Some("village_firewood".into())))
                ),
                (Topic::Homes, Some(Opens::Homes)),
                (Topic::Work, Some(Opens::Jobs(SimId(4)))),
                (Topic::Cold, Some(Opens::Priorities(SimId(4)))),
            ]
        );
        let work = candidates(
            &village,
            None,
            "village_firewood",
            &|good| good.into(),
            &|_| None,
        )
        .into_iter()
        .find(|(line, _)| line.topic == Topic::Work)
        .unwrap()
        .0;
        assert_eq!(work.status, Status::Warn);
        assert_eq!(work.tip, "No job in Mill has an open place.");
    }
}
